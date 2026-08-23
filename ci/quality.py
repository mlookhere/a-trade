#!/usr/bin/env python3
from __future__ import annotations

import fnmatch
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = json.loads((ROOT / ".claude-workflow.json").read_text(encoding="utf-8"))["quality"]
SECRET_PATTERNS = (
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{30,}\b"),
)
WORK_MARKER = re.compile(r"\b(?:TO" + "DO|FIX" + r"ME)\b(?![^\n]*#\d+)")


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)


def changed_files() -> list[str]:
    base_ref = CONFIG["base_ref"]
    if run("git", "rev-parse", "--verify", base_ref).returncode == 0:
        result = run("git", "diff", "--name-only", f"{base_ref}...HEAD")
    elif run("git", "rev-parse", "--verify", "HEAD^").returncode == 0:
        result = run("git", "diff", "--name-only", "HEAD^", "HEAD")
    else:
        result = run("git", "ls-files")
    return [line for line in result.stdout.splitlines() if line]


def excluded(path: str) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in CONFIG["exclude_globs"])


def main() -> int:
    failures: list[str] = []
    files = [path for path in changed_files() if not excluded(path)]
    if len(files) > CONFIG["max_changed_files"]:
        failures.append(f"changed files {len(files)} > {CONFIG['max_changed_files']}")

    changed_lines = 0
    for path in files:
        file_path = ROOT / path
        if not file_path.is_file() or file_path.suffix not in CONFIG["tracked_extensions"]:
            continue
        text = file_path.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines()
        changed_lines += len(lines)
        if len(lines) > CONFIG["max_file_lines"]:
            failures.append(f"{path}: {len(lines)} lines > {CONFIG['max_file_lines']}")
        for pattern in SECRET_PATTERNS:
            if pattern.search(text):
                failures.append(f"{path}: possible secret/private key pattern")
        for number, line in enumerate(lines, start=1):
            if WORK_MARKER.search(line):
                failures.append(f"{path}:{number}: work marker must reference a GitHub issue")
            if file_path.suffix == ".rs" and "dbg!(" in line:
                failures.append(f"{path}:{number}: dbg! is not allowed in committed Rust")

    if changed_lines > CONFIG["max_changed_lines"]:
        failures.append(f"changed text lines {changed_lines} > {CONFIG['max_changed_lines']}")

    for failure in failures:
        print(f"failure: {failure}")
    if failures:
        return 1
    print(f"quality: checked {len(files)} changed files / {changed_lines} text lines")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
