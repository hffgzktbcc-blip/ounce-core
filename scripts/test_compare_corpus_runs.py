from copy import deepcopy
import unittest
from unittest.mock import patch

from compare_corpus_runs import compare


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
