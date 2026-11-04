import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_switch2_corpus as corpus  # noqa: E402


class Switch2CorpusTests(unittest.TestCase):
    """The committed Switch 2 corpus validates, and a drop-in outside program is checked like step 2."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        shutil.copytree(corpus.CORPUS, self.tmp / "switch2")
        self.corpus = self.tmp / "switch2"
        synthetic = json.loads((self.corpus / "synthetic-oracles.json").read_text())
        # A stand-in outside entry shaped like a real drop-in: synthetic checks, outside labels.
        self.entry = copy.deepcopy(synthetic["oracles"][0])
        self.entry.update(id="example-outside", build="example", source="source/main.c", nro="example.nro")
        self.manifest = {"schema_version": 1, "platform": "switch2", "repository": "https://example.invalid/x.git",
                         "revision": "0" * 40, "upstream_terms": "MIT (LICENSE).",
                         "toolchain_image": "example/toolchain@sha256:" + "0" * 64,
                         "samples": [{"id": "example", "path": "."}]}

    def write(self, name, data):
        (self.corpus / name).write_text(json.dumps(data))

    def add_outside(self, entry=None, manifest=True):
        outside = json.loads((self.corpus / "oracles.json").read_text())
        outside["oracles"].append(entry or self.entry)
        self.write("oracles.json", outside)
        if manifest:
            self.write("oracle-builds/example.json", self.manifest)

    def errors(self):
        return corpus.check(self.corpus)

    def test_committed_corpus_is_valid(self):
        self.assertEqual(corpus.check(), [])

    def test_drop_in_outside_program_validates(self):
        self.add_outside()
        self.assertEqual(self.errors(), [])

    def test_outside_program_needs_a_build_manifest(self):
        self.add_outside(manifest=False)
        self.assertIn("build 'example' has no oracle-builds/example.json", self.errors())

    def test_manifest_must_pin_source_licence_and_toolchain(self):
        self.manifest.update(revision="main", toolchain_image="")
        del self.manifest["upstream_terms"]
        self.add_outside()
        errors = "\n".join(self.errors())
        for field in ("revision", "upstream_terms", "toolchain_image"):
            self.assertIn(field, errors)

    def test_synthetic_program_cannot_enter_the_outside_set(self):
        synthetic = json.loads((self.corpus / "synthetic-oracles.json").read_text())["oracles"][0]
        self.add_outside(dict(synthetic, id="relabelled"), manifest=False)
        self.assertTrue(any("synthetic programs only" in e for e in self.errors()))

    def test_ids_are_unique_across_sets(self):
        self.add_outside(dict(self.entry, id="printing-hello-world"))
        self.assertTrue(any("used twice" in e for e in self.errors()))

    def test_oracle_needs_spec_and_expected_value(self):
        entry = copy.deepcopy(self.entry)
        del entry["checks"][0]["spec"]
        del entry["checks"][0]["expect"]
        self.add_outside(entry)
        errors = "\n".join(self.errors())
        self.assertIn("spec it was written from", errors)
        self.assertIn("no expected hash", errors)

    def test_unsupported_format_and_platform_fail(self):
        self.add_outside(dict(self.entry, format="nca", platform="switch"))
        errors = "\n".join(self.errors())
        self.assertIn("not supported by the loader", errors)
        self.assertIn("platform must be", errors)

    def test_results_must_name_outside_oracles(self):
        self.write("oracle-results.json", {"results": {"s2-synthetic-rects": "pass"}})
        self.assertTrue(any("not an outside Switch 2 oracle" in e for e in self.errors()))

    def test_orphan_manifest_fails(self):
        self.write("oracle-builds/example.json", self.manifest)
        self.assertIn("oracle-builds/example.json is used by no oracle", self.errors())


if __name__ == "__main__":
    unittest.main()
