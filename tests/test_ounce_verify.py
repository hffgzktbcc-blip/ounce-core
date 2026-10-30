import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ounce_verify", ROOT / "tools/ounce_verify.py")
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def record():
    return {
        "schema_version": "ounce-verify/v0",
        "case_id": "synthetic/example",
        "target": "synthetic-aarch64",
        "source": {
            "origin": "project-authored",
            "repository": "local fixture",
            "commit": "fixture-v1",
            "source_tree_sha256": "1" * 64,
            "binary_sha256": "2" * 64,
            "toolchain": "recorded-test-toolchain",
        },
        "execution": {
            "core_commit": "3" * 40,
            "engine": "interpreter",
            "runner": "unit-test",
            "input_script_sha256": None,
            "limits": {"max_guest_instructions": 1000, "host_timeout_seconds": 5},
            "host_status": "exited-zero",
            "guest_stop": "exit-process",
            "guest_pc": "0x0000000000000000",
            "guest_x0": "0x0000000000000000",
            "guest_instructions": 10,
            "diagnostics_truncated": False,
        },
        "oracles": [{
            "name": "result-register", "scope": "application", "required": True,
            "status": "pass", "expected": "0x0", "observed": "0x0",
        }],
        "verdict": "pass",
    }


class OunceVerifyTests(unittest.TestCase):
    def test_host_zero_and_svc_break_never_pass_from_stop(self):
        value = record()
        value["execution"]["guest_stop"] = "svc-break"
        value["oracles"] = [{"name": "guest-stop", "scope": "application", "required": True,
                             "status": "pass", "expected": "svc-break", "observed": "svc-break"}]
        with self.assertRaisesRegex(VERIFY.EvidenceError, "svc-break"):
            VERIFY.validate_record(value)

    def test_exit_without_graphics_oracle_is_not_assessed(self):
        value = record()
        value["oracles"] = [{"name": "frame-count", "scope": "regression", "required": True,
                             "status": "pass", "expected": 1, "observed": 1}]
        value["verdict"] = "not-assessed"
        self.assertEqual(VERIFY.validate_record(value)["verdict"], "not-assessed")

    def test_expected_unsupported_regression_does_not_elevate_application(self):
        value = record()
        value["oracles"] = [{"name": "unsupported-ipc", "scope": "regression", "required": True,
                             "status": "pass", "expected": "explicit-error", "observed": "explicit-error"}]
        value["verdict"] = "not-assessed"
        self.assertEqual(VERIFY.validate_record(value)["verdict"], "not-assessed")

    def test_missing_hashes_mark_reproducibility_incomplete(self):
        value = record()
        value["source"]["source_tree_sha256"] = None
        value["source"]["binary_sha256"] = None
        self.assertEqual(VERIFY.validate_record(value)["reproducibility"], "incomplete")

    def test_engine_divergence_is_comparison_failure_not_engine_guilt(self):
        value = record()
        value["oracles"] = [{"name": "engine-comparison", "scope": "comparison", "required": True,
                             "status": "fail", "expected": "equal", "observed": "different"}]
        value["verdict"] = "not-assessed"
        self.assertEqual(VERIFY.validate_record(value)["verdict"], "not-assessed")

    def test_malformed_duplicate_unknown_and_truncated_records_are_rejected(self):
        for mutate in (
            lambda value: value.pop("target"),
            lambda value: value.update(schema_version="ounce-verify/v9"),
            lambda value: value.update(verdict="probably"),
            lambda value: value["execution"].update(diagnostics_truncated=True),
        ):
            value = record()
            mutate(value)
            with self.assertRaises(VERIFY.EvidenceError):
                VERIFY.validate_record(value)
        with self.assertRaisesRegex(VERIFY.EvidenceError, "duplicate case_id"):
            VERIFY.validate_records([record(), record()])

    def test_synthetic_frame_golden_match_and_mismatch(self):
        value = record()
        value["oracles"] = [{"name": "raw-frame-rgba8-2x1", "scope": "graphics", "required": True,
                             "status": "pass", "expected": "fnv64:1234", "observed": "fnv64:1234"}]
        self.assertEqual(VERIFY.validate_record(value)["verdict"], "pass")
        value["oracles"][0].update(status="fail", observed="fnv64:9999")
        value["verdict"] = "fail"
        self.assertEqual(VERIFY.validate_record(value)["verdict"], "fail")

    def test_canonical_report_is_deterministic_and_sorted(self):
        first = record()
        second = copy.deepcopy(first)
        first["case_id"] = "z/case"
        second["case_id"] = "a/case"
        a = VERIFY.canonical_report([first, second])
        b = VERIFY.canonical_report([second, first])
        self.assertEqual(a, b)
        self.assertLess(a.index("a/case"), a.index("z/case"))

    def test_host_and_guest_states_are_preserved(self):
        value = VERIFY.validate_record(record())
        self.assertEqual(value["execution"]["host_status"], "exited-zero")
        self.assertEqual(value["execution"]["guest_stop"], "exit-process")

    def test_original_switch_record_cannot_be_relabelled_switch2(self):
        value = record()
        value["target"] = "original-switch-libnx"
        checked = VERIFY.validate_record(value)
        self.assertEqual(checked["target"], "original-switch-libnx")
        self.assertNotEqual(checked["target"], "switch2-homebrew")

    def test_cli_writes_canonical_report_and_rejects_invalid_json(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "record.json"
            output = Path(directory) / "report.json"
            source.write_text(json.dumps(record()), encoding="utf-8")
            self.assertEqual(VERIFY.main([str(source), "--output", str(output)]), 0)
            self.assertEqual(output.read_text(), VERIFY.canonical_report([record()]))
            source.write_text("{", encoding="utf-8")
            self.assertEqual(VERIFY.main([str(source)]), 2)


if __name__ == "__main__":
    unittest.main()
