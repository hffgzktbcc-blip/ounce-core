"""Offline tests: python3 -m unittest discover -s scripts -p 'test_*.py'."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import prepare_external_corpus as corpus


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        corpus.run("git", "init", str(self.upstream))
        corpus.run("git", "config", "user.email", "tests@example.invalid", cwd=self.upstream)
        corpus.run("git", "config", "user.name", "Offline test", cwd=self.upstream)
        (self.upstream / "sample").mkdir()
        (self.upstream / "sample/main.c").write_text("int main(void) { return 7; }\n")
        corpus.run("git", "add", ".", cwd=self.upstream)
        corpus.run("git", "commit", "-m", "fixture", cwd=self.upstream)
        self.revision = corpus.run("git", "rev-parse", "HEAD", cwd=self.upstream)
        self.manifest = {"repository": str(self.upstream), "revision": self.revision,
                         "samples": [{"id": "cpu", "path": "sample"}]}
        self.manifest_path = self.root / "manifest.json"

    def write_manifest(self):
        self.manifest_path.write_text(json.dumps(self.manifest))

    def test_pins_old_commit_and_hashes_without_executing(self):
        (self.upstream / "sample/main.c").write_text("changed\n")
        corpus.run("git", "commit", "-am", "later", cwd=self.upstream)
        self.write_manifest()
        result = corpus.prepare(self.manifest_path, self.root / "out")
        self.assertEqual(result["revision"], self.revision)
        self.assertEqual(result["compatibility"], "not-tested")
        self.assertEqual(result["status"], "prepared-not-built")
        self.assertEqual(result["source_sha256"]["sample/main.c"],
                         corpus.digest(self.root / "out/source/sample/main.c"))
        self.assertIn("return 7", (self.root / "out/source/sample/main.c").read_text())

    def test_refuses_existing_output_without_modification(self):
        self.write_manifest()
        out = self.root / "out"
        out.mkdir()
        (out / "keep").write_text("keep")
        with self.assertRaisesRegex(ValueError, "new or empty"):
            corpus.prepare(self.manifest_path, out)
        self.assertEqual((out / "keep").read_text(), "keep")

    def test_rejects_output_in_repository(self):
        self.write_manifest()
        with self.assertRaisesRegex(ValueError, "outside"):
            corpus.prepare(self.manifest_path, corpus.REPO / "corpus-output")

    def test_rejects_traversal_duplicate_ids_and_moving_refs(self):
        for path in ("../sample", "/tmp/sample", "sample/../sample", "sample\\other", "./sample"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                corpus.validate({**self.manifest, "samples": [{"id": "cpu", "path": path}]})
        with self.assertRaises(ValueError):
            corpus.validate({**self.manifest, "revision": "main"})
        with self.assertRaises(ValueError):
            corpus.validate({**self.manifest, "samples": self.manifest["samples"] * 2})

    def test_detects_modified_tracked_files(self):
        (self.upstream / "sample/main.c").write_text("changed")
        with self.assertRaisesRegex(ValueError, "modified"):
            corpus.checked_source(self.upstream, self.revision)

    def test_build_requires_toolchain_before_cloning(self):
        self.write_manifest()
        with patch.dict(os.environ, {}, clear=True), self.assertRaisesRegex(ValueError, "DEVKITPRO"):
            corpus.prepare(self.manifest_path, self.root / "out", build=True)
        self.assertFalse((self.root / "out").exists())

    @unittest.skipIf(os.name == "nt", "fake compiler uses a POSIX shell")
    def test_opt_in_build_records_binary_and_toolchain_hashes(self):
        (self.upstream / "sample/Makefile").write_text("all:\n\tprintf 'NRO test bytes' > example.nro\n")
        corpus.run("git", "add", ".", cwd=self.upstream)
        corpus.run("git", "commit", "-m", "build fixture", cwd=self.upstream)
        self.manifest["revision"] = corpus.run("git", "rev-parse", "HEAD", cwd=self.upstream)
        self.write_manifest()
        devkit = self.root / "devkit"
        compiler = devkit / "devkitA64/bin/aarch64-none-elf-gcc"
        compiler.parent.mkdir(parents=True)
        compiler.write_text("#!/bin/sh\nprintf 'offline compiler\\n'\n")
        compiler.chmod(0o755)
        with patch.dict(os.environ, {"DEVKITPRO": str(devkit)}):
            result = corpus.prepare(self.manifest_path, self.root / "out", build=True)
        self.assertEqual(result["status"], "built-not-executed")
        self.assertEqual(result["compatibility"], "not-tested")
        self.assertEqual(result["samples"][0]["nro_sha256"]["sample/example.nro"],
                         corpus.digest(self.root / "out/source/sample/example.nro"))
        self.assertTrue((self.root / "out/cpu.build.log").is_file())


if __name__ == "__main__":
    unittest.main()
