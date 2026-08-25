#!/usr/bin/env python3
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "ci" / "dependency_audit.py"
FIXTURES = ROOT / "ci" / "fixtures" / "dependency_audit"


class DependencyAuditContract(unittest.TestCase):
    def make_fake_auditor(self, directory: Path, *, audit_exit: int = 0, version: str = "0.22.2") -> Path:
        path = directory / "cargo-audit"
        path.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "if [[ ${1:-} == --version ]]; then\n"
            f"  echo 'cargo-audit {version}'\n"
            "  exit 0\n"
            "fi\n"
            "if [[ -n ${FAKE_AUDIT_MARKER:-} ]]; then touch \"$FAKE_AUDIT_MARKER\"; fi\n"
            f"exit {audit_exit}\n",
            encoding="utf-8",
        )
        path.chmod(0o755)
        return path

    def run_audit(self, lock_name: str, auditor: Path, marker: Path | None = None) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["CARGO_AUDIT_BIN"] = str(auditor)
        if marker is not None:
            env["FAKE_AUDIT_MARKER"] = str(marker)
        return subprocess.run(
            ["python3", str(AUDIT), "--lock-file", str(FIXTURES / lock_name)],
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_no_third_party_dependencies_still_verify_audit_backend_and_database_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            marker = directory / "audit-ran"
            result = self.run_audit(
                "no_third_party.lock",
                self.make_fake_auditor(directory),
                marker,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(marker.exists(), "audit backend must run even for a workspace-only lockfile")

    def test_clean_third_party_registry_dependency_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            result = self.run_audit("clean_registry.lock", self.make_fake_auditor(directory))
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_backend_unavailable_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            missing = Path(temp) / "missing-cargo-audit"
            result = self.run_audit("clean_registry.lock", missing)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("backend unavailable", result.stderr)

    def test_vulnerability_backend_failure_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            result = self.run_audit(
                "clean_registry.lock",
                self.make_fake_auditor(directory, audit_exit=1),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("failed closed", result.stderr)

    def test_unsupported_source_fails_before_audit_backend(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            marker = directory / "audit-ran"
            result = self.run_audit(
                "unsupported_source.lock",
                self.make_fake_auditor(directory),
                marker,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsupported dependency source", result.stderr)
            self.assertFalse(marker.exists())

    def test_wrong_audit_version_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            result = self.run_audit(
                "clean_registry.lock",
                self.make_fake_auditor(directory, version="0.0.0"),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("version mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()
