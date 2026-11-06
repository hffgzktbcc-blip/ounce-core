"""Execution verdicts must belong to this invocation, even when evidence is reused."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
from threading import Barrier
import unittest
from unittest.mock import patch
import subprocess
import sys

import run_corpus_oracles as runner


class FreshVerdictTests(unittest.TestCase):
    def test_failed_rerun_does_not_reuse_previous_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            corpus = root / "tests/corpus"
            corpus.mkdir(parents=True)
            (corpus / "oracles.json").write_text(json.dumps({"oracles": [
                {"id": "sample", "build": "b", "nro": "sample.nro"}]}))
            (corpus / "oracle-results.json").write_text(json.dumps({"results": {"sample": "pass"}}))
            nro = root / "builds/b/build/source/sample.nro"
            nro.parent.mkdir(parents=True)
            nro.write_bytes(b"test")
            evidence = root / "evidence"
            evidence.mkdir()
            (evidence / "sample.oracle.json").write_text(json.dumps(
                {"id": "sample", "pass": True, "failures": []}))
            arguments = ["runner", "--executable", "not-executed", "--builds", str(root / "builds"),
                         "--evidence", str(evidence)]
            with patch.object(runner, "REPO", root), patch("sys.argv", arguments), \
                    patch.object(runner.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)), \
                    contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                runner.main()
            result = json.loads((evidence / "oracle-verdicts.json").read_text())
            self.assertEqual(result["passed"], 0)
            self.assertFalse(result["verdicts"][0]["pass"])

    def test_parallel_shards_preserve_order_and_report_regressions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            corpus = root / "tests/corpus"
            corpus.mkdir(parents=True)
            builds = root / "builds"
            source = builds / "b/build/source"
            source.mkdir(parents=True)
            ids = ["one", "two", "three"]
            (corpus / "oracles.json").write_text(json.dumps({"oracles": [
                {"id": name, "build": "b", "nro": f"{name}.nro"} for name in ids]}))
            (corpus / "oracle-results.json").write_text(json.dumps(
                {"results": {name: "pass" for name in ids}}))
            for name in ids:
                (source / f"{name}.nro").write_bytes(b"test")

            # The first case of each shard must reach the barrier concurrently.
            barrier = Barrier(2, timeout=5)

            def run(command, *, env, **kwargs):
                name = env["OUNCE_ORACLE_ID"]
                if name in ("one", "two"):
                    barrier.wait()
                passed = name != "two"
                Path(env["OUNCE_ORACLE_RESULT"]).write_text(json.dumps(
                    {"id": name, "pass": passed, "failures": [] if passed else ["frame mismatch"]}))
                return subprocess.CompletedProcess(command, 0 if passed else 1)

            evidence = root / "evidence"
            arguments = ["runner", "--executable", "not-executed", "--builds", str(builds),
                         "--evidence", str(evidence), "--shards", "2"]
            with patch.object(runner, "REPO", root), patch("sys.argv", arguments), \
                    patch.object(runner.subprocess, "run", side_effect=run), \
                    contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(SystemExit, "two"):
                runner.main()
            result = json.loads((evidence / "oracle-verdicts.json").read_text())
            self.assertEqual([verdict["id"] for verdict in result["verdicts"]], ids)
            self.assertEqual(result["passed"], 2)

    def test_checkpoint_requires_matching_evidence_and_a_completed_guest_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkpoint, result, log = (root / name for name in ("checkpoint", "result", "log"))
            identity = {"oracle": {"id": "sample"}, "nro_sha256": "digest", "seconds": 180}
            verdict = {"id": "sample", "nro_sha256": "digest", "pass": True,
                       "runs": [{"engine": "Interpreter"}, {"engine": "Tier3"}]}
            result.write_text(json.dumps(verdict))
            log.write_text("guest output")
            checkpoint.write_text(json.dumps(runner.checkpoint_for(identity, result, log, "exited-zero")))
            self.assertTrue(runner.resume_case(checkpoint, identity, result, log)["pass"])
            self.assertIsNone(runner.resume_case(checkpoint, dict(identity, seconds=181), result, log))
            log.write_text("changed")
            self.assertIsNone(runner.resume_case(checkpoint, identity, result, log))
            checkpoint.write_text(json.dumps(runner.checkpoint_for(identity, result, log, "exited-101")))
            self.assertIsNone(runner.resume_case(checkpoint, identity, result, log))


class ProcessEvidenceTests(unittest.TestCase):
    def test_invalid_result_preserves_raw_file_and_does_not_abort_other_cases(self):
        # Real host processes inject corrupt output; these are runner fault tests,
        # not guest execution or compatibility evidence.
        for malformed in ("{", "[]", '{"id":"wrong","pass":true,"failures":[]}',
                          '{"id":"bad","pass":"yes","failures":[]}',
                          '{"id":"bad","pass":true,"failures":"wrong"}'):
            with self.subTest(malformed=malformed), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                corpus = root / "tests/corpus"
                corpus.mkdir(parents=True)
                ids = ["bad", "other-shard", "following"]
                (corpus / "oracles.json").write_text(json.dumps({"oracles": [
                    {"id": name, "build": "b", "nro": name + ".nro"} for name in ids]}))
                (corpus / "oracle-results.json").write_text(json.dumps({"results": {}}))
                source = root / "builds/b/build/source"
                source.mkdir(parents=True)
                for name in ids:
                    (source / (name + ".nro")).write_bytes(b"host-fault-test-input")
                executable = root / "fault-process"
                executable.write_text(
                    "#!" + sys.executable + "\nimport os, pathlib, json\n"
                    "name = os.environ['OUNCE_ORACLE_ID']\n"
                    "payload = " + repr(malformed) + " if name == 'bad' else json.dumps("
                    "{'id': name, 'pass': False, 'failures': ['intentional host test failure']})\n"
                    "pathlib.Path(os.environ['OUNCE_ORACLE_RESULT']).write_text(payload)\n")
                executable.chmod(0o755)
                evidence = root / "evidence"
                arguments = ["runner", "--executable", str(executable), "--builds", str(root / "builds"),
                             "--evidence", str(evidence), "--resume", "--shards", "2"]
                with patch.object(runner, "REPO", root), patch("sys.argv", arguments), \
                        contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(SystemExit, "invalid oracle evidence"):
                    runner.main()
                aggregate = json.loads((evidence / "oracle-verdicts.json").read_text())
                self.assertEqual([v["id"] for v in aggregate["verdicts"]], ids)
                self.assertEqual(aggregate["passed"], 0)
                self.assertEqual((evidence / "bad.oracle.json").read_text(), malformed)
                self.assertFalse((evidence / "bad.checkpoint.json").exists())
