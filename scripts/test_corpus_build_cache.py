import json
from pathlib import Path
import tempfile
import unittest

from corpus_build_cache import cache_key, digest, restore, store


class BuildCacheTests(unittest.TestCase):
    def test_round_trip_and_corruption_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            builds = root / "builds"
            builds.mkdir()
            (builds / "toolchain-packages.txt").write_text("fixture toolchain")
            relative = Path("original/build/source/sample.nro")
            binary = builds / relative
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"sample")
            manifest = root / "manifest.json"
            source = {"repository": "https://example.com/source", "revision": "revision"}
            manifest.write_text(json.dumps(source))
            manifests = {"original": manifest}
            oracles = [{"build": "original", "nro": "sample.nro"}]
            inventory = {
                **source, "manifest_sha256": digest(manifest), "status": "built-not-executed",
                "samples": [{"nro_sha256": {"sample.nro": digest(binary)}}],
            }
            (binary.parent.parent / "inventory.json").write_text(json.dumps(inventory))
            key = cache_key("image-a", "recipe", manifests, oracles)
            self.assertNotEqual(key, cache_key("image-b", "recipe", manifests, oracles))
            entry = root / "cache" / key
            restored = root / "restored"
            restored.mkdir()
            self.assertEqual(restore(entry, restored, manifests, oracles), "miss")
            store(entry, builds, manifests, oracles)
            self.assertEqual(restore(entry, restored, manifests, oracles), "hit")
            self.assertEqual((restored / relative).read_bytes(), b"sample")

            (entry / relative).write_bytes(b"corrupt")
            empty = root / "empty"
            empty.mkdir()
            self.assertIn("hash mismatch", restore(entry, empty, manifests, oracles))
            self.assertEqual(list(empty.iterdir()), [])
