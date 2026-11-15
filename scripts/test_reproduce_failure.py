import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from reproduce_failure import EXAMPLE, REPO


class RetainedWalkthroughTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.evidence = self.root / "evidence"
        shutil.copytree(EXAMPLE, self.evidence)
        self.output = self.root / "bundle"

    def command(self):
        return subprocess.run([sys.executable, "-B", str(REPO / "scripts/reproduce_failure.py"),
                               "--evidence", str(self.evidence), "--output", str(self.output)],
                              cwd=self.root, capture_output=True, text=True)

    def test_real_retained_reports_produce_portable_missing_first_frame_diagnosis(self):
        result = self.command()
        self.assertEqual(result.returncode, 0, result.stderr)
        diagnosis = json.loads((self.output / "diagnosis.json").read_text())
        self.assertEqual(len(diagnosis["cases"]), 2)
        for case in diagnosis["cases"]:
            difference = case["observations"]["first_divergence"]
            self.assertEqual((difference["kind"], difference["index"]), ("frame", 1))
            self.assertIsNone(difference["candidate"])
            replay = case["replay"]["candidate"]
            self.assertEqual(replay["checkout"], "0e5c340885989c1f70bdb11ba9570a06ef1384b8")
            self.assertEqual(replay["nro_sha256"], "d92512342d6d571023da3c98d907f04c0c85043de7b2ffd50a745cbcc928bc7f")
        comparison = json.loads((self.output / "comparison.json").read_text())
        self.assertEqual(comparison["coverage"]["expected"], 40)
        self.assertEqual(comparison["regressions"], [])
        self.assertFalse(comparison["wall_times_comparable"])
        manifest = json.loads((self.evidence / "manifest.json").read_text())
        for name in manifest["files"]:
            self.assertEqual((self.output / name).read_bytes(), (self.evidence / name).read_bytes())
        for path in self.output.rglob("*"):
            if path.is_file():
                self.assertNotIn(str(self.root), path.read_text())
        replay = (self.output / "REPLAY.md").read_text()
        self.assertIn("Fresh replay acceptance remains pending", replay)
        self.assertIn("cargo test --release --locked", replay)

    def test_missing_and_corrupt_reports_fail_without_partial_bundle(self):
        path = self.evidence / "failure/canonical-report.json"
        original = path.read_bytes()
        for value in (None, b"{bad json"):
            if value is None:
                path.unlink()
            else:
                path.write_bytes(value)
            result = self.command()
            self.assertEqual(result.returncode, 2)
            self.assertIn("failure/canonical-report.json", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse(self.output.exists())
        path.write_bytes(original)

    def test_raw_observation_disagreement_is_rejected_even_with_updated_digest(self):
        name = "failure/sdl2-simple.oracle.json"
        path = self.evidence / name
        observation = json.loads(path.read_text())
        observation["runs"][0]["pc"] = "0x0000000000000000"
        path.write_text(json.dumps(observation))
        manifest_path = self.evidence / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["files"][name]["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        manifest_path.write_text(json.dumps(manifest))
        result = self.command()
        self.assertEqual(result.returncode, 2)
        self.assertIn("raw observations disagree", result.stderr)
        self.assertFalse(self.output.exists())

    def test_existing_output_is_preserved(self):
        self.output.mkdir()
        (self.output / "peer-work").write_text("preserve")
        result = self.command()
        self.assertEqual(result.returncode, 2)
        self.assertIn("choose a fresh directory", result.stderr)
        self.assertEqual((self.output / "peer-work").read_text(), "preserve")

    def test_path_escape_is_rejected(self):
        manifest_path = self.evidence / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["files"]["../outside"] = {"sha256": "a" * 64}
        manifest_path.write_text(json.dumps(manifest))
        result = self.command()
        self.assertEqual(result.returncode, 2)
        self.assertIn("escapes its directory", result.stderr)

    def test_malformed_manifest_has_an_actionable_nonzero_exit(self):
        manifest_path = self.evidence / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["files"] = []
        manifest_path.write_text(json.dumps(manifest))
        result = self.command()
        self.assertEqual(result.returncode, 2)
        self.assertIn("ounce-verify walkthrough:", result.stderr)
        self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
