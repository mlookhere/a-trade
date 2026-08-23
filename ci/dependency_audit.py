#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
lock = ROOT / "Cargo.lock"

if not lock.is_file():
    raise SystemExit("Cargo.lock is missing; dependency state cannot be audited")

sources = [line.strip() for line in lock.read_text(encoding="utf-8").splitlines() if line.strip().startswith("source =")]
if sources:
    joined = "\n".join(sources)
    raise SystemExit(
        "Third-party Rust dependencies are present but this repository has not yet installed a "
        "vulnerability-audit backend. Fail closed and add the audit backend in the same reviewed PR:\n"
        + joined
    )

print("dependency audit: no third-party Rust package sources present")
