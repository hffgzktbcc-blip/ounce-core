"""Guard the README roadmap against outdated milestone prose.

The canonical program count is still checked separately against actual corpus evidence
by scripts/corpus_verify.py; this file deliberately does not certify a count.
"""
from pathlib import Path
import re
import unittest

README = Path(__file__).resolve().parents[1] / "README.md"


class ReadmeRoadmapTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = README.read_text(encoding="utf-8")
        cls.roadmap = cls.text.split("## The road ahead", 1)[1].split(
            "## Sources and porting policy", 1
        )[0]

    def test_no_obsolete_roadmap_states(self):
        for phrase in (
            "Waiting on public toolchains",
            "Corpus 4 → 20",
            "the unmerged raw-frame evidence work",
            "The 20-program count",
            "four original oracles",
            "last checked green on main:",
        ):
            with self.subTest(phrase=phrase):
                self.assertNotIn(phrase, self.text)

    def test_live_links_and_proof_boundary(self):
        for link in (
            "actions/workflows/ci.yml",
            "actions/workflows/external-corpus.yml",
            "/pulls?q=is%3Apr+is%3Aopen",
            "/actions",
            "docs/verify/README.md",
        ):
            with self.subTest(link=link):
                self.assertIn(link, self.roadmap)
        self.assertIn("A green workflow badge does not", self.roadmap)

    def test_roadmap_corpus_count_stays_ci_managed(self):
        matches = re.findall(
            r"<!-- corpus-headline -->[^<]+<!-- /corpus-headline -->",
            self.roadmap,
        )
        self.assertEqual(len(matches), 1)
        self.assertIn("scripts/corpus_verify.py", self.roadmap)


if __name__ == "__main__":
    unittest.main()
