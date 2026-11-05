"""Execution verdicts must belong to this invocation, even when evidence is reused."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import subprocess

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
