"""Tests for the evidence-gated, generated README roadmap."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
REGISTRY = ROOT / "docs/roadmap/milestones.json"


class ReadmeRoadmapTest(unittest.TestCase):
    def test_architecture_is_generated_from_source_registry(self):
        registry = json.loads((ROOT / "docs/architecture/layers.json").read_text(encoding="utf-8"))
        text = README.read_text(encoding="utf-8")
        architecture = text.split("### The architecture at a glance", 1)[1].split(
            "<details>", 1
        )[0]
        self.assertIn("BEGIN GENERATED ARCHITECTURE", architecture)
        self.assertIn("END GENERATED ARCHITECTURE", architecture)
        for layer in registry["layers"]:
            with self.subTest(layer=layer["id"]):
                self.assertIn(layer["name"], architecture)
                self.assertTrue(layer["sources"])
                for source in layer["sources"]:
                    self.assertTrue((ROOT / source).is_file(), source)

    def test_registry_schema_and_generator_are_valid(self):
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/render_roadmap.py"), "--check"],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_readme_uses_registry_not_a_frozen_roadmap(self):
        text = README.read_text(encoding="utf-8")
        registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
        roadmap = text.split("## The road ahead", 1)[1].split(
            "## Sources and porting policy", 1
        )[0]
        self.assertIn("BEGIN GENERATED ROADMAP", roadmap)
        self.assertIn("END GENERATED ROADMAP", roadmap)
        for row in registry["tracks"]:
            with self.subTest(milestone=row["id"]):
                self.assertIn(row["name"], roadmap)
        self.assertIn("scripts/corpus_verify.py", roadmap)
        self.assertIn("A green badge or merged PR is not proof", roadmap)

    def test_vision_navigation_points_to_the_section(self):
        text = README.read_text(encoding="utf-8")
        self.assertIn("[The Ounce Vision](#the-ounce-vision)", text)
        self.assertEqual(text.count('<a id="the-ounce-vision"></a>'), 1)
        self.assertIn("## ✦ THE OUNCE VISION ✦", text)

    def test_old_claims_have_not_returned(self):
        text = README.read_text(encoding="utf-8")
        for obsolete in ("Waiting on public toolchains", "Corpus 4 → 20",
                         "The 20-program count", "four original oracles",
                         "last checked green on main:"):
            with self.subTest(obsolete=obsolete):
                self.assertNotIn(obsolete, text)


if __name__ == "__main__":
    unittest.main()
