import json
import tempfile
import unittest
from pathlib import Path

import package_macos_app as pkg

ROOT = Path(__file__).resolve().parent.parent


def fake_app(base):
    app = base / "Ounce Core.app"
    for name in pkg.REQUIRED:
        path = app / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(name.encode())
    return app


class PackageMacApp(unittest.TestCase):
    def test_packages_with_manifest_and_matching_checksums(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            out = base / "out"
            manifest = pkg.package(fake_app(base), out, ROOT)
            self.assertIn("not a release", manifest["kind"])
            self.assertIn("not notarized", manifest["signing"])
            self.assertEqual(set(manifest["bundle_files"]), set(pkg.REQUIRED))
            sums = dict(line.split("  ")[::-1] for line in (out / "SHA256SUMS").read_text().splitlines())
            for name, digest in manifest["archives"].items():
                self.assertEqual(sums[name], digest)
                self.assertEqual(pkg.sha256(out / name), digest)
            self.assertEqual(json.loads((out / "MANIFEST.json").read_text())["version"], pkg.crate_version(ROOT))

    def test_rejects_incomplete_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            app = fake_app(Path(tmp))
            (app / "Contents/Resources/Licenses/LICENSE").unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                pkg.package(app, Path(tmp) / "out", ROOT)

    def test_refuses_existing_output_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            with self.assertRaisesRegex(ValueError, "fresh"):
                pkg.package(fake_app(base), base, ROOT)


if __name__ == "__main__":
    unittest.main()
