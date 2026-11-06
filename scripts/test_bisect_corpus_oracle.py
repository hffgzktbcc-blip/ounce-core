import unittest

from bisect_corpus_oracle import narrow, observed_state


class BisectionTests(unittest.TestCase):
    def test_finds_adjacent_commits_and_leaves_unknown_interval(self):
        commits = ["a", "b", "c", "d"]
        states = {"a": "pass", "b": "pass", "c": "fail", "d": "fail"}
        self.assertEqual(narrow(commits, "pass", "fail", states.get), ("b", "c", True))
        states["c"] = "unknown"
        self.assertEqual(narrow(commits, "pass", "fail", states.get), ("b", "d", False))

    def test_infrastructure_and_different_bytes_are_inconclusive(self):
        result = {"id": "example", "nro_sha256": "digest", "pass": False, "runs": [
            {"engine": "Interpreter", "stop": "FrameLimit"},
            {"engine": "Tier3", "stop": "FrameLimit"},
        ]}
        self.assertEqual(observed_state(result, "example", "digest", 101), "fail")
        self.assertEqual(observed_state(result, "example", "digest", 1), "unknown")
        self.assertEqual(observed_state(result, "example", "other", 101), "unknown")
        result["runs"][0]["stop"] = "HostTimeout"
        self.assertEqual(observed_state(result, "example", "digest", 101), "unknown")
