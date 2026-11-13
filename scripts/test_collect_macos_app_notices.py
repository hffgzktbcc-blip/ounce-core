import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from collect_macos_app_notices import collect


class AppNoticesTests(unittest.TestCase):
    def test_selected_dependency_text_and_inventory_are_retained(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            (base / 'LICENSE').write_text('Copyright fixture; licence text')
            package = {'id': 'fixture', 'name': 'fixture', 'version': '1',
                       'manifest_path': str(base / 'Cargo.toml'), 'license': 'MIT',
                       'source': 'registry', 'repository': None}
            metadata = {'packages': [package], 'resolve': {'nodes': [{'id': 'fixture'}]}}
            with patch('collect_macos_app_notices.subprocess.check_output', return_value=json.dumps(metadata)) as command:
                collect(base / 'out')
            self.assertIn('gpu-present nce native-jit', command.call_args.args[0])
            raw = (base / 'out/fixture-1/LICENSE').read_bytes()
            inventory = json.loads((base / 'out/manifest.json').read_text())
            self.assertEqual(inventory['packages'][0]['files_sha256']['LICENSE'], hashlib.sha256(raw).hexdigest())
            package['manifest_path'] = str(base / 'missing/Cargo.toml')
            metadata['packages'] = [package]
            with patch('collect_macos_app_notices.subprocess.check_output', return_value=json.dumps(metadata)):
                with self.assertRaisesRegex(ValueError, 'No licence text'):
                    collect(base / 'empty')
