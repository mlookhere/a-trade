#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".claude-workflow.json"


def git_sha() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False
    )
    return result.stdout.strip() if result.returncode == 0 else "UNKNOWN"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: ./ci/run <stage>", file=sys.stderr)
        return 2

    config = json.loads(CONFIG.read_text(encoding="utf-8"))
    stage = sys.argv[1]
    groups = config.get("stages", {}).get(stage)
    if not groups:
        print(f"unknown or empty stage: {stage}", file=sys.stderr)
        return 2

    report = {"stage": stage, "git_sha": git_sha(), "success": True, "commands": []}
    started = time.monotonic()

    for group in groups:
        commands = config.get("commands", {}).get(group)
        if not commands:
            print(f"stage {stage} references empty command group {group}", file=sys.stderr)
            report["success"] = False
            break
        for command in commands:
            print(f"[{stage}:{group}] {command}", flush=True)
            command_started = time.monotonic()
            completed = subprocess.run(["bash", "-lc", command], cwd=ROOT, check=False)
            report["commands"].append(
                {
                    "group": group,
                    "command": command,
                    "exit_code": completed.returncode,
                    "duration_seconds": round(time.monotonic() - command_started, 4),
                }
            )
            if completed.returncode != 0:
                report["success"] = False
                break
        if not report["success"]:
            break

    report["duration_seconds"] = round(time.monotonic() - started, 4)
    out = ROOT / "artifacts" / "ci"
    out.mkdir(parents=True, exist_ok=True)
    (out / f"{stage}.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 0 if report["success"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
