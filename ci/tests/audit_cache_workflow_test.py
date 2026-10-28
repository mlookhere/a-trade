#!/usr/bin/env python3
"""Mutation tests for required pinned audit cache / execution sequence."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from workflow.self_test import audit_cache_policy_failures  # noqa: E402


class AuditCacheWorkflowContract(unittest.TestCase):
    def setUp(self) -> None:
        self.workflows = {
            str(path.relative_to(ROOT)): path.read_text(encoding="utf-8")
            for path in sorted((ROOT / ".github" / "workflows").glob("*.yml"))
        }
        self.pr = ".github/workflows/ci-pr.yml"

    def assert_rejected(self, before: str, after: str, *, path: str | None = None) -> None:
        path = path or self.pr
        self.assertIn(before, self.workflows[path], "test fixture must match active workflow")
        revised = dict(self.workflows)
        revised[path] = revised[path].replace(before, after, 1)
        self.assertNotEqual(revised[path], self.workflows[path])
        self.assertTrue(
            audit_cache_policy_failures(revised),
            f"unsafe workflow mutation not rejected: {path}",
        )

    def test_real_pr_release_and_nightly_cache_contracts_pass(self) -> None:
        self.assertEqual(audit_cache_policy_failures(self.workflows), [])

    def test_missing_cache_step_fails(self) -> None:
        self.assert_rejected(
            "Cache pinned cargo-audit executable",
            "Unverified external cache",
        )

    def test_cache_binary_path_cannot_expand(self) -> None:
        self.assert_rejected(
            "path: ~/.cargo/bin/cargo-audit",
            "path: ~/.cargo",
        )

    def test_unpinned_cache_action_fails(self) -> None:
        self.assert_rejected(
            "actions/cache@0400d5f644dc74513175e3cd8d07132dd4860809",
            "actions/cache@v4",
        )

    def test_policy_hash_cannot_be_removed(self) -> None:
        self.assert_rejected(
            "${{ hashFiles('ci/dependency-policy.toml') }}",
            "unchanging-policy",
        )

    def test_toolchain_hash_cannot_be_removed(self) -> None:
        self.assert_rejected(
            "${{ hashFiles('rust-toolchain.toml') }}",
            "rust-hardcoded",
        )

    def test_bootstrap_cannot_be_skipped(self) -> None:
        self.assert_rejected(
            "- run: bash ./scripts/bootstrap-audit",
            "- run: echo skipped-version-check",
        )

    def test_audit_stage_cannot_be_omitted(self) -> None:
        self.assert_rejected(
            "- run: ./ci/run audit",
            "- run: echo skipped-audit",
        )

    def test_conditional_cache_hit_cannot_skip_audit(self) -> None:
        self.assert_rejected(
            "      - run: ./ci/run audit\n",
            "      - run: ./ci/run audit\n        if: steps.cache.outputs.cache-hit != 'true'\n",
        )

    def test_continue_on_error_cannot_hide_failure(self) -> None:
        self.assert_rejected(
            "      - run: bash ./scripts/bootstrap-audit\n",
            "      - run: bash ./scripts/bootstrap-audit\n        continue-on-error: true\n",
        )

    def test_step_order_cannot_change(self) -> None:
        self.assert_rejected(
            "      - run: bash ./scripts/bootstrap-audit\n      - run: ./ci/run audit",
            "      - run: ./ci/run audit\n      - run: bash ./scripts/bootstrap-audit",
        )

    def test_duplicate_cache_action_cannot_appear(self) -> None:
        self.assert_rejected(
            "      - name: Cache pinned cargo-audit executable",
            "      - name: Cache pinned cargo-audit executable\n"
            "        uses: actions/cache@0400d5f644dc74513175e3cd8d07132dd4860809\n"
            "      - name: Cache pinned cargo-audit executable",
        )

    def test_release_audit_remains_mandatory(self) -> None:
        self.assert_rejected(
            "- run: ./ci/run release",
            "- run: echo skipped-release",
            path=".github/workflows/ci-release.yml",
        )

    def test_nightly_audit_remains_mandatory(self) -> None:
        self.assert_rejected(
            "- run: ./ci/run nightly",
            "- run: echo skipped-nightly",
            path=".github/workflows/nightly-quality.yml",
        )


if __name__ == "__main__":
    unittest.main()
