from copy import deepcopy
import unittest
from unittest.mock import patch

from compare_corpus_runs import compare, comparison_failed, state


class RunComparisonTests(unittest.TestCase):
    def test_reports_regression_and_ignores_wall_time_across_hosts(self):
        key = ("example", "tier3")
        record = {
            "verdict": "pass", "oracles": [],
            "source": {"binary_sha256": "same"},
            "execution": {"limits": {}, "guest_instructions": 100},
        }
        reference = ({key: record}, {"core_commit": "old", "timing_host": "cpu-a"}, {key: 1.0})
        candidate = deepcopy(reference)
        candidate[1].update(core_commit="new", timing_host="cpu-b")
        candidate[0][key]["execution"]["guest_instructions"] = 150
        candidate[2][key] = 2.0
        report = compare(reference, candidate, {key})
        self.assertEqual(report["performance_changes"], [{"case": "example/tier3", "instructions": [100, 150]}])
        candidate[1]["timing_host"] = "cpu-a"
        report = compare(reference, candidate, {key})
        self.assertEqual(report["performance_changes"][0]["wall_seconds"], [1.0, 2.0])
        candidate[0][key]["verdict"] = "fail"
        with patch("compare_corpus_runs.diagnose", return_value={}):
            report = compare(reference, candidate, {key})
        self.assertEqual(report["regressions"], [{"case": "example/tier3", "before": "pass", "after": "fail"}])

    def test_corpus_growth_can_establish_a_new_baseline(self):
        coverage = {"reference_missing": ["new/tier3"], "reference_extra": [],
                    "candidate_missing": [], "candidate_extra": []}
        report = {"coverage": coverage, "regressions": []}
        self.assertFalse(comparison_failed(report))
        coverage["candidate_missing"] = ["new/tier3"]
        self.assertTrue(comparison_failed(report))

    def test_engine_disagreement_has_a_distinct_state(self):
        record = {"verdict": "pass", "oracles": [{"scope": "comparison", "status": "fail"}]}
        self.assertEqual(state(record), "engine-disagreement")
