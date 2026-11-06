from copy import deepcopy
import unittest

from qualify_corpus_repair import evaluate, registered_cases


class RepairQualificationTests(unittest.TestCase):
    def setUp(self):
        self.expected = registered_cases(b'{"oracles":[{"id":"target"},{"id":"guard"}]}')
        self.reference = {}
        for program, engine in self.expected:
            fail = program == "target"
            self.reference[program, engine] = {
                "case_id": f"corpus/{program}/{engine}",
                "source": {"binary_sha256": "a" * 64},
                "execution": {"host_status": "exited-101" if fail else "exited-zero"},
                "oracles": [{"scope": "graphics", "name": "first-frame",
                             "required": True, "expected": "registered"}],
                "verdict": "fail" if fail else "pass",
            }
        self.candidate = deepcopy(self.reference)
        for engine in ("interpreter", "tier3"):
            record = self.candidate["target", engine]
            record["verdict"] = "pass"
            record["execution"]["host_status"] = "exited-zero"

    def issues(self):
        return evaluate(self.reference, self.candidate, "target", self.expected)

    def test_requires_an_observed_failure_and_clean_pass_on_both_engines(self):
        self.assertEqual(self.issues(), [])
        self.candidate["target", "tier3"]["execution"]["host_status"] = "outer-timeout"
        self.assertTrue(any("target did not pass" in issue for issue in self.issues()))
        self.reference = deepcopy(self.candidate)
        self.assertTrue(any("no observed oracle failure" in issue for issue in self.issues()))

    def test_rejects_dropped_cases_and_regressions(self):
        del self.candidate["guard", "tier3"]
        self.candidate["guard", "interpreter"]["verdict"] = "fail"
        issues = self.issues()
        self.assertTrue(any("coverage differs" in issue for issue in issues))
        self.assertTrue(any("regressed" in issue for issue in issues))

    def test_rejects_changed_oracles_and_nro_bytes(self):
        record = self.candidate["target", "tier3"]
        record["source"]["binary_sha256"] = "b" * 64
        record["oracles"][0]["expected"] = "replacement"
        issues = self.issues()
        self.assertTrue(any("oracle expectations changed" in issue for issue in issues))
        self.assertTrue(any("same NRO bytes" in issue for issue in issues))


if __name__ == "__main__":
    unittest.main()
