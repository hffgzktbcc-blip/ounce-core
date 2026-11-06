"""Offline tests for the corpus Ounce-Verify adapter (records are fabricated in memory)."""
import sys
import copy
import json
import tempfile
from pathlib import Path
import unittest

import corpus_verify

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import ounce_verify  # noqa: E402

SHA = "ab" * 32
ORACLE = {"id": "demo", "build": "b", "nro": "demo/demo.nro",
          "checks": [{"frames": [1], "expect": {"width": 1280, "height": 720, "rgba_fnv1a64": "0x0123456789abcdef"}}]}
INVENTORY = {"repository": "https://example.invalid/demo.git", "revision": "1" * 40,
             "source_sha256": {"demo/main.c": SHA}, "samples": [{"nro_sha256": {"demo/demo.nro": SHA}}]}


def run(engine, failures=()):
    return {"engine": engine, "stop": "FrameLimit", "instructions": 10, "pc": "0x0000000080000000",
            "x0": "0x0000000000000000", "failures": list(failures), "host_timeout_seconds": 60,
            "max_guest_instructions": 1000, "queued": []}


def records(verdict):
    return ounce_verify.validate_records(corpus_verify.records_for(ORACLE, verdict, INVENTORY, "gcc", "2" * 40))


class AdapterTests(unittest.TestCase):
    def test_pass_needs_both_engines_and_validates(self):
        out = records({"nro_sha256": SHA, "host_status": "exited-zero", "failures": [],
                       "runs": [run("Interpreter"), run("Tier3")]})
        self.assertEqual([r["verdict"] for r in out], ["pass", "pass"])
        self.assertEqual(out[0]["reproducibility"], "content-addressed")

    def test_frame_failure_is_fail_for_that_engine_only(self):
        out = records({"nro_sha256": SHA, "host_status": "exited-101", "failures": ["Tier3: hash"],
                       "runs": [run("Interpreter"), run("Tier3", ["hash"])]})
        self.assertEqual({r["case_id"]: r["verdict"] for r in out},
                         {"corpus/demo/interpreter": "pass", "corpus/demo/tier3": "fail"})

    def test_missing_run_is_blocked_never_pass(self):
        out = records({"host_status": "outer-timeout", "host_timeout_seconds": 60, "failures": ["timeout"]})
        self.assertEqual([r["verdict"] for r in out], ["blocked", "blocked"])
        self.assertTrue(out[0]["execution"]["observation_missing"])

    def test_binary_must_match_inventory(self):
        with self.assertRaises(SystemExit):
            corpus_verify.records_for(ORACLE, {"nro_sha256": "cd" * 32, "runs": []}, INVENTORY, "gcc", "2" * 40)


class ReadmeTests(unittest.TestCase):
    README = ("[![o](https://img.shields.io/badge/first--frame_oracles-3%2F4_pass-green)](x)\n"
              "<!-- corpus-headline -->3/4<!-- /corpus-headline --> and <!-- corpus-headline -->3/4<!-- /corpus-headline -->\n")

    def test_matching_readme_is_unchanged(self):
        self.assertEqual(corpus_verify.render_readme(self.README, 3, 4), self.README)

    def test_headline_and_badge_follow_the_report(self):
        text = corpus_verify.render_readme(self.README, 4, 4)
        self.assertEqual(text.count("4/4<!--"), 2)
        self.assertIn("first--frame_oracles-4%2F4_pass", text)
        self.assertNotIn("3", text.replace("first--frame", ""))

    def test_missing_badge_fails(self):
        with self.assertRaises(SystemExit):
            corpus_verify.render_readme("<!-- corpus-headline -->1/1<!-- /corpus-headline -->", 1, 1)


class CoverageTests(unittest.TestCase):
    def count(self, values):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "oracles.json"
            manifest.write_text(json.dumps({"oracles": [{"id": "demo"}, {"id": "missing"}]}))
            report = root / "report.json"
            report.write_text(json.dumps({"schema_version": ounce_verify.SCHEMA, "records": values}))
            return corpus_verify.count(report, manifest)

    def passing(self):
        return records({"nro_sha256": SHA, "runs": [run("Interpreter"), run("Tier3")]})

    def test_missing_program_stays_in_denominator(self):
        self.assertEqual(self.count(self.passing()), (["demo"], 2))

    def test_missing_engine_cannot_pass(self):
        self.assertEqual(self.count(self.passing()[:1]), ([], 2))

    def test_duplicates_unknown_engines_and_engine_mismatch_rejected(self):
        values = self.passing()
        with self.assertRaises(SystemExit):
            self.count([values[0], values[0]])
        for case_id, engine in (("corpus/demo/native", "native"),
                                ("corpus/demo/tier3", "interpreter"),
                                ("corpus/unknown/interpreter", "interpreter")):
            value = copy.deepcopy(values[0])
            value["case_id"] = case_id
            value["execution"]["engine"] = engine
            with self.subTest(case_id=case_id), self.assertRaises(SystemExit):
                self.count([value])

    def test_relabeling_cannot_enter_the_original_switch_headline(self):
        values = self.passing()
        values[0]["target"] = "switch2-homebrew"
        with self.assertRaisesRegex(SystemExit, "target"):
            self.count(values)

    def test_adapter_rejects_missing_executed_hash_and_duplicate_runs(self):
        for verdict in ({"runs": [run("Interpreter")]},
                        {"nro_sha256": SHA, "runs": [run("Interpreter"), run("Interpreter")]},
                        {"nro_sha256": SHA, "runs": [run("Native")]}):
            with self.assertRaises(SystemExit):
                records(verdict)


if __name__ == "__main__":
    unittest.main()

