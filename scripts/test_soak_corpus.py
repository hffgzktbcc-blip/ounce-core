import json
from pathlib import Path
import unittest

import soak_corpus

REPO = Path(__file__).resolve().parents[1]


class SoakKnownFindingTests(unittest.TestCase):
    def test_only_listed_findings_are_tolerated(self):
        known = {"p": [{"finding": "Tier3: crash (Break"}]}
        new, listed = soak_corpus.split_findings(
            "p", ["Tier3: crash (Break { reason: 1 })", "Interpreter: hang (HostTimeout)"], known)
        self.assertEqual(listed, ["Tier3: crash (Break { reason: 1 })"])
        self.assertEqual(new, ["Interpreter: hang (HostTimeout)"])
        self.assertEqual(soak_corpus.split_findings("other", ["Tier3: crash (Break"], known)[0],
                         ["Tier3: crash (Break"])

    def test_known_file_names_registered_programs_and_issues(self):
        registered = {o["id"] for o in json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]}
        known = soak_corpus.known_findings(REPO / "tests/corpus/soak-known.json")
        self.assertTrue(set(known) <= registered)
        for entries in known.values():
            for entry in entries:
                self.assertRegex(entry["issue"], r"^https://github\.com/[^/]+/ounce-core/issues/\d+$")
                self.assertTrue(entry["finding"])

    def test_smoke_programs_are_registered(self):
        registered = {o["id"] for o in json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]}
        self.assertTrue(set(soak_corpus.SMOKE_IDS) <= registered)


if __name__ == "__main__":
    unittest.main()
