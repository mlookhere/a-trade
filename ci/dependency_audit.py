#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY_FILE = ROOT / "ci" / "dependency-policy.toml"


def load_policy() -> tuple[str, list[str], list[str]]:
    policy = tomllib.loads(POLICY_FILE.read_text(encoding="utf-8"))
    audit = policy["audit"]
    sources = policy["sources"]
    return audit["cargo_audit_version"], list(audit["deny"]), list(sources["allowed"])


def validate_lock(lock: Path, allowed_sources: list[str]) -> None:
    if not lock.is_file():
        raise ValueError("Cargo.lock is missing; dependency state cannot be audited")

    data = tomllib.loads(lock.read_text(encoding="utf-8"))
    packages = data.get("package")
    if not isinstance(packages, list):
        raise ValueError("Cargo.lock has no package list; dependency state is invalid")

    allowed = set(allowed_sources)
    for package in packages:
        source = package.get("source")
        if source is None:
            continue
        name = package.get("name", "UNKNOWN")
        version = package.get("version", "UNKNOWN")
        if source not in allowed:
            raise ValueError(
                f"unsupported dependency source for {name} {version}: {source}; "
                "source requires explicit reviewed policy approval"
            )
        checksum = package.get("checksum")
        if not isinstance(checksum, str) or not checksum.strip():
            raise ValueError(f"registry dependency {name} {version} is missing a lockfile checksum")


def run_audit(lock: Path, expected_version: str, deny: list[str]) -> None:
    binary = os.environ.get("CARGO_AUDIT_BIN", "cargo-audit")
    try:
        version = subprocess.run(
            [binary, "--version"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
    except OSError as exc:
        raise ValueError(f"cargo-audit backend unavailable: {exc}") from exc

    expected = f"cargo-audit {expected_version}"
    if version.returncode != 0:
        detail = version.stderr.strip() or version.stdout.strip() or f"exit {version.returncode}"
        raise ValueError(f"cargo-audit backend unavailable: {detail}")
    if version.stdout.strip() != expected:
        raise ValueError(
            f"cargo-audit version mismatch: expected {expected!r}, got {version.stdout.strip()!r}"
        )

    command = [binary, "audit", "--file", str(lock)]
    for value in deny:
        command.extend(["--deny", value])
    audit = subprocess.run(command, cwd=ROOT, text=True, check=False)
    if audit.returncode != 0:
        raise ValueError(f"cargo-audit failed closed with exit code {audit.returncode}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock-file", type=Path, default=ROOT / "Cargo.lock")
    args = parser.parse_args()

    try:
        expected_version, deny, allowed_sources = load_policy()
        validate_lock(args.lock_file, allowed_sources)
        run_audit(args.lock_file, expected_version, deny)
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError, ValueError) as exc:
        print(f"dependency audit failure: {exc}", file=sys.stderr)
        return 1

    print("dependency audit: lockfile source policy and RustSec audit passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
