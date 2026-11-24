#!/usr/bin/env python3
"""Hermetic tests for scripts/verify_differential.py."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))
import verify_differential


class VerifyDifferentialTests(unittest.TestCase):
    def test_makefile_honours_devkitpro_env(self):
        custom_dkp = "/custom/devkitpro/path"
        proc = subprocess.run(
            [
                sys.executable,
                "-c",
                "import os, verify_differential; print(verify_differential.MAKEFILE)",
            ],
            cwd=REPO / "scripts",
            env=dict(os.environ, DEVKITPRO=custom_dkp),
            capture_output=True,
            text=True,
            check=True,
        )
        expected = os.path.join(custom_dkp, "examples/switch/templates/application/Makefile")
        self.assertEqual(proc.stdout.strip(), expected)

    def test_format_summary_reports_skipped_engines(self):
        report = [
            {
                "name": "qc-flag",
                "checks": {
                    "committed-vs-fresh-host": "identical",
                    "engines": "skipped (no NRO)",
                },
            }
        ]
        summary = verify_differential.format_summary(report, failed=False)
        self.assertIn("all compared outputs identical", summary)
        self.assertIn("engine comparisons skipped; pass --nro to verify engines", summary)
        self.assertEqual(
            summary,
            "all compared outputs identical (engine comparisons skipped; pass --nro to verify engines)",
        )

    def test_format_summary_without_skips(self):
        report = [
            {
                "name": "qc-flag",
                "checks": {
                    "committed-vs-fresh-host": "identical",
                    "tier3": "identical",
                },
            }
        ]
        summary = verify_differential.format_summary(report, failed=False)
        self.assertEqual(summary, "all compared outputs identical")

    def test_format_summary_with_documented_differences(self):
        report = [
            {
                "name": "qc-flag",
                "checks": {
                    "committed-vs-fresh-host": "expected-different (documented)",
                    "engines": "skipped (no NRO)",
                },
                "documented": {
                    "committed-vs-fresh-host": {"reason": "test diff", "evidence": "doc"}
                },
            }
        ]
        summary = verify_differential.format_summary(report, failed=False)
        self.assertEqual(
            summary,
            "all compared outputs identical (engine comparisons skipped; pass --nro to verify engines) except 1 documented host difference(s)",
        )

    def test_format_summary_failed(self):
        report = [
            {
                "name": "qc-flag",
                "checks": {
                    "committed-vs-fresh-host": "DIFFERENT",
                },
            }
        ]
        summary = verify_differential.format_summary(report, failed=True)
        self.assertEqual(summary, "FAILED")

    @mock.patch("verify_differential.build_host")
    @mock.patch("verify_differential.stage")
    def test_main_without_nro_stubbed(self, mock_stage, mock_build_host):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmppath = Path(tmpdir)
            expected_file = REPO / "docs" / "verify" / "qc-flag" / "expected-host.txt"
            mock_stage.return_value = tmppath
            mock_build_host.return_value = (expected_file, None)
            json_out = tmppath / "result.json"

            with mock.patch("sys.argv", ["verify_differential.py", "--only", "qc-flag", "--json", str(json_out), "--work", str(tmppath)]):
                with mock.patch("sys.exit") as mock_exit:
                    verify_differential.main()
                    mock_exit.assert_called_with(0)

            data = json.loads(json_out.read_text())
            self.assertFalse(data["failed"])
            self.assertEqual(len(data["harnesses"]), 1)
            self.assertEqual(data["harnesses"][0]["name"], "qc-flag")
            self.assertEqual(data["harnesses"][0]["checks"].get("engines"), "skipped (no NRO)")


if __name__ == "__main__":
    unittest.main()
