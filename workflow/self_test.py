#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINNED_ACTION = re.compile(r"^\s*uses:\s*([^\s]+)@([0-9a-fA-F]{40})\s*(?:#.*)?$")
USES_LINE = re.compile(r"^\s*uses:\s*([^\s]+)@([^\s#]+)")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ci", action="store_true")
    parser.parse_args()

    failures: list[str] = []
    config = json.loads((ROOT / ".claude-workflow.json").read_text(encoding="utf-8"))

    if config.get("branches") != {"integration": "dev", "production": "main"}:
        failures.append("branches must remain dev integration / main production")
    github = config.get("github", {})
    if github.get("expected_owner") != "mlookhere" or github.get("expected_repository") != "a-trade":
        failures.append("GitHub repository identity does not match mlookhere/a-trade")

    commands = config.get("commands", {})
    for name, command_list in commands.items():
        if not isinstance(command_list, list) or not command_list or not all(command_list):
            failures.append(f"command group {name!r} is empty")

    stages = config.get("stages", {})
    for required in ("fast", "pr", "audit", "release", "nightly"):
        if not stages.get(required):
            failures.append(f"required stage {required!r} is missing or empty")
    for stage, groups in stages.items():
        for group in groups:
            if group not in commands:
                failures.append(f"stage {stage!r} references unknown command group {group!r}")

    workflow_text = "\n".join(
        path.read_text(encoding="utf-8") for path in sorted((ROOT / ".github" / "workflows").glob("*.yml"))
    )
    for branch_kind in ("integration", "production"):
        for check in github["branch_protection"][branch_kind]["required_checks"]:
            if f"name: {check}" not in workflow_text:
                failures.append(f"required check {check!r} has no workflow job")

    for path in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        text = path.read_text(encoding="utf-8")
        if "pull_request_target:" in text:
            failures.append(f"{path.relative_to(ROOT)} uses pull_request_target")
        for number, line in enumerate(text.splitlines(), start=1):
            match = USES_LINE.match(line)
            if match and not match.group(1).startswith("./") and not PINNED_ACTION.match(line):
                failures.append(f"{path.relative_to(ROOT)}:{number}: action must be pinned to a 40-char SHA")

    authority = json.loads((ROOT / "strategy" / "authority.json").read_text(encoding="utf-8"))
    if authority.get("canonical_sha256") != "9e24b9a4ffc55a8815cb62e5c9f72d8356a3d52de5e716a2db8ced38df508deb":
        failures.append("canonical strategy fingerprint drifted")
    if authority.get("canonical_sections") != 123:
        failures.append("canonical section count drifted")

    for failure in failures:
        print(f"failure: {failure}")
    if failures:
        return 1
    print("workflow self-test: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
