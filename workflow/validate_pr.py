#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import re
from pathlib import Path

REQUIRED_SECTIONS = ("Issue", "Result", "Implementation", "Verification", "Risk", "Remaining work")
WORK_BRANCH = re.compile(r"^work/(\d+)-[a-z0-9][a-z0-9-]*$")
ISSUE_REF = re.compile(r"(?:^|\s)#(\d+)\b")


def main() -> int:
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        raise SystemExit("GITHUB_EVENT_PATH is required")
    event = json.loads(Path(event_path).read_text(encoding="utf-8"))
    if "pull_request" not in event:
        print("PR metadata: merge-group/non-PR event; source PR already validated")
        return 0

    pr = event["pull_request"]
    base = pr["base"]["ref"]
    head = pr["head"]["ref"]
    title = pr.get("title") or ""
    body = pr.get("body") or ""
    failures: list[str] = []

    issue_numbers = {int(value) for value in ISSUE_REF.findall(body)}
    if base == "dev":
        match = WORK_BRANCH.fullmatch(head)
        if not match:
            failures.append("PRs into dev must come from work/<issue>-slug")
        elif int(match.group(1)) not in issue_numbers:
            failures.append("branch Issue number must be referenced in PR body")
        for section in REQUIRED_SECTIONS:
            if f"## {section}" not in body:
                failures.append(f"PR body missing section: {section}")
    elif base == "main":
        if head != "dev":
            failures.append("production PRs must originate from dev")
        if not title.startswith("[RELEASE]"):
            failures.append("production PR title must start with [RELEASE]")
        if not issue_numbers:
            failures.append("release PR must reference a release Issue")
    else:
        failures.append(f"unsupported PR base branch: {base}")

    for failure in failures:
        print(f"failure: {failure}")
    if failures:
        return 1
    print("PR metadata: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
