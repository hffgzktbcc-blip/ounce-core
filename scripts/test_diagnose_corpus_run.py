import unittest

from diagnose_corpus_run import diagnose, first_divergence


def record(frames, stop="frame-limit", binary="a" * 64, expected="registered"):
    return {
        "source": {"binary_sha256": binary},
        "execution": {
            "core_commit": "b" * 40,
            "guest_stop": stop,
            "limits": {"host_timeout_seconds": 180},
        },
        "oracles": [{
            "scope": "graphics", "name": "first-frame", "required": True,
            "expected": expected, "observed": {"queued": frames},
        }],
        "verdict": "pass" if stop == "frame-limit" else "fail",
    }


class DiagnosisTests(unittest.TestCase):
    def test_first_divergence_uses_frame_index(self):
        first = {"index": 1, "rgba_fnv1a64": "one"}
        second = {"index": 2, "rgba_fnv1a64": "two"}
        left = record([second, first])
        right = record([{"index": 2, "rgba_fnv1a64": "new"}, first])
        self.assertEqual(first_divergence(left, right)["index"], 2)
        self.assertIsNone(first_divergence(left, record([first, second])))

    def test_incompatible_inputs_are_not_compared_as_frames(self):
        left = record([{"index": 1}])
        self.assertEqual(first_divergence(left, record([], binary="c" * 64))["kind"], "different-binary")
        self.assertEqual(first_divergence(left, record([], expected="different"))["kind"], "different-oracle")

    def test_groups_early_exit_in_both_directions_and_retains_replay(self):
        passed = record([{"index": 1}])
        failed = record([], stop="exit-process")
        key = ("sample", "interpreter")
        for before, after, failing_side in ((passed, failed, "candidate"), (failed, passed, "reference")):
            with self.subTest(failing_side=failing_side):
                bundle = diagnose({key: before}, {key: after})
                case, = bundle["cases"]
                self.assertIsNone(case["observations"]["first_divergence"][failing_side])
                self.assertIn("guest initialization or service emulation", bundle["groups"])
                self.assertEqual(case["replay"][failing_side]["env"]["OUNCE_ORACLE_ID"], "sample")

    def test_missing_candidate_has_no_replay(self):
        bundle = diagnose({("sample", "tier3"): record([])}, {})
        case, = bundle["cases"]
        self.assertEqual(case["observations"]["first_divergence"]["kind"], "coverage")
        self.assertIsNone(case["replay"]["candidate"])


if __name__ == "__main__":
    unittest.main()
