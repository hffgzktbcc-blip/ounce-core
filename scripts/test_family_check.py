import json
import stat
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import family_check

BASE = {
    "schema": "ounce.run.v1",
    "stop_kind": "clean_exit",
    "exit_code": 0,
    "frames_presented": 0,
    "framebuffer_hash": None,
    "insns_executed": 100,
}


class CompareTest(unittest.TestCase):
    def test_identical_reports_agree(self):
        self.assertEqual(family_check.compare("a", BASE, "b", dict(BASE)), ([], []))

    def test_strict_difference_is_reported(self):
        other = dict(BASE, stop_kind="max_insns")
        strict, advisory = family_check.compare("a", BASE, "b", other)
        self.assertEqual(len(strict), 1)
        self.assertIn("stop_kind", strict[0])
        self.assertEqual(advisory, [])

    def test_missing_strict_key_is_a_difference(self):
        other = {k: v for k, v in BASE.items() if k != "frames_presented"}
        strict, _ = family_check.compare("a", BASE, "b", other)
        self.assertEqual(len(strict), 1)
        self.assertIn("frames_presented", strict[0])

    def test_advisory_differences_never_fail(self):
        other = dict(BASE, insns_executed=115)
        reference = dict(BASE, audio_hash="fnv1a64:1")
        strict, advisory = family_check.compare("a", reference, "b", other)
        self.assertEqual(strict, [])
        self.assertEqual(len(advisory), 2)
        self.assertTrue(any("only a writes it" in line for line in advisory))
        self.assertTrue(any("insns_executed" in line for line in advisory))

    def test_no_report_fails_and_names_the_tool(self):
        strict, _ = family_check.compare("a", BASE, "b", None)
        self.assertEqual(strict, ["no report written by b"])
        strict, _ = family_check.compare("a", None, "b", BASE)
        self.assertEqual(strict, ["no report written by a"])


class ToolSpecTest(unittest.TestCase):
    def test_rejects_bad_specs(self):
        with tempfile.TemporaryDirectory() as tmp:
            real = Path(tmp) / "tool"
            real.write_text("")
            for specs in (["a"], ["a=" + str(real)], ["a=" + str(real), "a=" + str(real)], ["a=" + str(real), "b=/nope"]):
                with self.assertRaises(ValueError):
                    family_check.parse_tools(specs)
            self.assertEqual(len(family_check.parse_tools(["a=" + str(real), "b=" + str(real)])), 2)


class EndToEndTest(unittest.TestCase):
    """Drive main() with two tiny fake tools that write reports, so no emulator is needed."""

    def fake_tool(self, directory, name, report):
        path = Path(directory) / name
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import sys, json\n"
            "out = [a for a in sys.argv if a.startswith('--report-json=')][0].split('=', 1)[1]\n"
            f"open(out, 'w').write(json.dumps({report!r}))\n"
        )
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def run_main(self, other_report):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "fixtures" / "hello").mkdir(parents=True)
            (tmp / "fixtures" / "hello" / "hello.nro").write_bytes(b"")
            ref = self.fake_tool(tmp, "ref", BASE)
            other = self.fake_tool(tmp, "other", other_report)
            out_json = tmp / "result.json"
            code = family_check.main([
                "--tool", f"ref={ref}", "--tool", f"other={other}",
                "--fixtures", str(tmp / "fixtures"), "--work", str(tmp / "work"), "--json", str(out_json),
            ])
            return code, json.loads(out_json.read_text())

    def test_agreement_exits_zero(self):
        code, result = self.run_main(dict(BASE, insns_executed=115))
        self.assertEqual(code, 0)
        self.assertEqual(result["results"][0]["others"]["other"]["strict"], [])
        self.assertEqual(len(result["results"][0]["others"]["other"]["advisory"]), 1)

    def test_strict_difference_exits_one(self):
        code, _ = self.run_main(dict(BASE, exit_code=3))
        self.assertEqual(code, 1)

    def test_refuses_a_non_empty_work_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "fixtures" / "a").mkdir(parents=True)
            (tmp / "fixtures" / "a" / "a.nro").write_bytes(b"")
            (tmp / "work").mkdir()
            (tmp / "work" / "old.json").write_text("{}")
            tool = self.fake_tool(tmp, "t", BASE)
            code = family_check.main(["--tool", f"x={tool}", "--tool", f"y={tool}",
                                      "--fixtures", str(tmp / "fixtures"), "--work", str(tmp / "work")])
            self.assertEqual(code, 2)


if __name__ == "__main__":
    unittest.main()
