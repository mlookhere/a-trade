#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINNED_ACTION = re.compile(r"^\s*uses:\s*([^\s]+)@([0-9a-fA-F]{40})\s*(?:#.*)?$")
USES_LINE = re.compile(r"^\s*uses:\s*([^\s]+)@([^\s#]+)")

REQUIRED_STAGE_GROUPS = {
    "fast": ["workflow_self_test", "quality", "rust_format", "rust_lint"],
    "pr": ["rust_test", "rust_release_build"],
    "audit": ["dependency_audit_tests", "dependency_audit"],
    "release": [
        "workflow_self_test",
        "quality",
        "rust_format",
        "rust_lint",
        "rust_test",
        "rust_release_build",
        "dependency_audit_tests",
        "dependency_audit",
    ],
    "nightly": [
        "workflow_self_test",
        "quality",
        "rust_format",
        "rust_lint",
        "rust_test",
        "rust_release_build",
        "dependency_audit_tests",
        "dependency_audit",
    ],
}



# CI bootstrap cache is a performance optimization, never an audit bypass.
AUDIT_CACHE_PIN = "actions/cache@0400d5f644dc74513175e3cd8d07132dd4860809"
AUDIT_WORKFLOW_JOBS = {
    ".github/workflows/ci-pr.yml": ("audit", "audit"),
    ".github/workflows/ci-release.yml": ("regression", "release"),
    ".github/workflows/nightly-quality.yml": ("deep-quality", "nightly"),
}


def audit_cache_policy_failures(workflows: dict[str, str]) -> list[str]:
    """Fail closed on malformed/missing audit bootstrap and cache sequencing.

    Only inspect the named audit-bearing job, not similarly named steps in
    unrelated jobs. No YAML dependency or network/tool execution is required.
    """
    failures: list[str] = []
    for path, (job_name, stage) in AUDIT_WORKFLOW_JOBS.items():
        source = workflows.get(path)
        if source is None:
            failures.append(f"{path}: required workflow missing")
            continue

        job_match = re.search(r"(?m)^  " + re.escape(job_name) + r":\s*$", source)
        if job_match is None:
            failures.append(f"{path}: required audit-bearing job {job_name!r} missing")
            continue

        tail = source[job_match.end():]
        next_job = re.search(r"(?m)^  [a-zA-Z][\w-]*:\s*$", tail)
        job_text = tail[:next_job.start()] if next_job else tail
        if re.search(r"(?m)^    (?:if|continue-on-error):", job_text):
            failures.append(f"{path}: audit job must not have conditional or error bypass")

        step_starts = list(re.finditer(r"(?m)^      - (?:name|run|uses):", job_text))
        steps = [
            job_text[m.start():step_starts[i + 1].start() if i + 1 < len(step_starts) else len(job_text)]
            for i, m in enumerate(step_starts)
        ]
        cache_steps = [i for i, step in enumerate(steps) if "Cache pinned cargo-audit executable" in step]
        if len(cache_steps) != 1 or job_text.count("uses: actions/cache@") != 1:
            failures.append(f"{path}: audit job must contain exactly one pinned cache step")
            continue
        cache_idx = cache_steps[0]
        expected_key = (
            "cargo-audit-v1-"
            + "$" + "{{ runner.os }}-" + "$" + "{{ runner.arch }}-rust-"
            + "$" + "{{ hashFiles('rust-toolchain.toml') }}-"
            + "$" + "{{ hashFiles('ci/dependency-policy.toml') }}"
        )
        expected_cache = [
            "      - name: Cache pinned cargo-audit executable",
            "        uses: " + AUDIT_CACHE_PIN + " # v4.2.4",
            "        with:",
            "          path: ~/.cargo/bin/cargo-audit",
            "          key: " + expected_key,
        ]
        if steps[cache_idx].rstrip("\n").splitlines() != expected_cache:
            failures.append(f"{path}: cache pin/path/key must match the approved tool-only contract")

        expected_runs = [
            "      - run: ./scripts/bootstrap",
            "      - run: bash ./scripts/bootstrap-audit",
            "      - run: ./ci/run " + stage,
        ]
        run_indices: list[int] = []
        for expected in expected_runs:
            hits = [i for i, step in enumerate(steps) if step.splitlines()[0].strip() == expected.strip()]
            if len(hits) != 1:
                failures.append(f"{path}: required unmodified step {expected.strip()} absent or duplicated")
            else:
                run_indices.append(hits[0])

        if len(run_indices) == 3:
            bootstrap_idx, verify_idx, audit_idx = run_indices
            if not (bootstrap_idx + 1 == cache_idx and cache_idx + 1 == verify_idx and verify_idx + 1 == audit_idx):
                failures.append(f"{path}: cache, version check, and audit must be ordered and adjacent")
            for idx in run_indices:
                lines = steps[idx].strip().splitlines()
                if len(lines) != 1:
                    failures.append(f"{path}: required audit step must not be conditional or mask failures")

    return failures


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
    for stage, required_groups in REQUIRED_STAGE_GROUPS.items():
        if stages.get(stage) != required_groups:
            failures.append(
                f"stage {stage!r} must be exactly {required_groups!r}; got {stages.get(stage)!r}"
            )
    for stage, groups in stages.items():
        for group in groups:
            if group not in commands:
                failures.append(f"stage {stage!r} references unknown command group {group!r}")

    workflow_sources = {
        str(path.relative_to(ROOT)): path.read_text(encoding="utf-8")
        for path in sorted((ROOT / ".github" / "workflows").glob("*.yml"))
    }
    workflow_text = "\n".join(workflow_sources.values())
    failures.extend(audit_cache_policy_failures(workflow_sources))
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
                failures.append(
                    f"{path.relative_to(ROOT)}:{number}: action must be pinned to a 40-char SHA"
                )

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
