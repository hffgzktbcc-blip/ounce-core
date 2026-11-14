"""Unit tests for scripts/run_linux_arm64_matrix.py (no network required)."""
import json
from pathlib import Path
import tempfile
import unittest

import run_linux_arm64_matrix as matrix
from verify_differential import VERIFY


class HarnessSelectionTests(unittest.TestCase):
    def test_select_harnesses_from_real_manifest(self):
        manifest = json.loads((VERIFY / 'differential.json').read_text(encoding='utf-8'))
        selected = matrix.select_harnesses(manifest)
        names = [h['name'] for h in selected]
        for expected in ('lz4', 'pmull', 'lua', 'monocypher', 'quickjs', 'sqlite', 'stb-image', 'wasm3', 'zlib', 'yyjson', 'stb-truetype', 'zstd'):
            self.assertIn(expected, names)
        for excluded in ('fp-instructions', 'integer-vector', 'narrow-saturating', 'qc-flag', 'qc-saturating-shifts', 'vector-fp', 'soak'):
            self.assertNotIn(excluded, names)

    def test_select_harnesses_filters_nx_only_and_handles_only(self):
        manifest = {
            'harnesses': [
                {'name': 'lz4', 'host_main': 'host/host_main.c', 'upstream': {'repo': 'x'}},
                {'name': 'nx-thing', 'host_main': 'host/host_main.c', 'upstream': {'repo': 'x'}, 'nx_only': True},
                {'name': 'no-host', 'upstream': {'repo': 'x'}},
                {'name': 'pmull', 'host_main': 'host/host_main.c', 'arm64_reference': True},
            ]
        }
        selected = matrix.select_harnesses(manifest)
        self.assertEqual([h['name'] for h in selected], ['lz4', 'pmull'])
        self.assertEqual([h['name'] for h in matrix.select_harnesses(manifest, only=['pmull'])], ['pmull'])
        with self.assertRaises(ValueError):
            matrix.select_harnesses(manifest, only=['nx-thing'])

    def test_harness_profiles_fast_vs_heavy(self):
        fast = matrix.harness_profile('lz4')
        self.assertFalse(fast['heavy'])
        self.assertEqual(fast['levels'], ['O1', 'O2', 'O3_M1'])
        self.assertEqual(fast['budget'], 100_000_000)

        for name in ('lua', 'monocypher', 'quickjs', 'sqlite', 'stb-image', 'wasm3', 'zlib'):
            heavy = matrix.harness_profile(name)
            self.assertTrue(heavy['heavy'])
            self.assertEqual(heavy['levels'], ['O1', 'O2'])
            self.assertEqual(heavy['budget'], 4_000_000_000)

    def test_build_verify_cmd_defaults_and_qemu(self):
        profile = matrix.harness_profile('kissfft')
        cmd = matrix.build_verify_cmd(
            {'name': 'kissfft', 'elf_reference': 'qemu'},
            Path('/tmp/work/kissfft'),
            ounce=Path('/tmp/ounce-core'),
            profile=profile,
            machine='aarch64',
            qemu='qemu-aarch64',
        )
        self.assertIn('--host-cc', cmd)
        self.assertEqual(cmd[cmd.index('--host-cc') + 1], 'cc')
        self.assertIn('--elf-cc', cmd)
        self.assertEqual(cmd[cmd.index('--elf-cc') + 1], 'python3 -m ziglang cc -target aarch64-linux-musl')
        self.assertIn('--opt-matrix', cmd)
        self.assertEqual(cmd[cmd.index('--opt-matrix') + 1], 'O1,O2,O3_M1')
        self.assertIn('--diverge', cmd)
        self.assertIn('--qemu', cmd)
        self.assertEqual(cmd[cmd.index('--qemu') + 1], 'qemu-aarch64')


class MatrixAggregationTests(unittest.TestCase):
    def _write_manifest(self, tmp_dir):
        manifest_path = tmp_dir / 'differential.json'
        manifest_path.write_text(json.dumps({
            'harnesses': [
                {'name': 'lz4', 'host_main': 'host/host_main.c', 'upstream': {'repo': 'x'}},
                {'name': 'lua', 'host_main': 'host/host_main.c', 'upstream': {'repo': 'x'}},
                {'name': 'soak', 'host_main': 'host/host_main.c', 'arm64_reference': True},
            ]
        }), encoding='utf-8')
        return manifest_path

    def test_run_matrix_aggregates_passing_rows(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            manifest_path = self._write_manifest(root)
            work = root / 'work'
            work.mkdir()

            def fake_runner(cmd, harness, sub_work):
                sub_work.mkdir(parents=True, exist_ok=True)
                opt_str = cmd[cmd.index('--opt-matrix') + 1]
                levels = opt_str.split(',')
                runs = {}
                checks = {}
                for opt in levels:
                    runs[opt] = {
                        'failed': False,
                        'checks': {'tier3': 'identical', 'tier4': 'identical', 'diverge': 'identical'},
                        'host_sha256': 'a' * 64,
                        'elf_sha256': 'b' * 64,
                        'error': None,
                        'report': str(sub_work / opt / 'report.json'),
                    }
                    checks[f'{opt}:tier3'] = 'identical'
                    checks[f'{opt}:tier4'] = 'identical'
                    checks[f'{opt}:diverge'] = 'identical'
                (sub_work / 'report.json').write_text(json.dumps({
                    'schema': 'ounce-core/linux-library-matrix-v1',
                    'harness': harness['name'],
                    'opt_matrix': levels,
                    'failed': False,
                    'runs': runs,
                    'checks': checks,
                }), encoding='utf-8')
                return 0

            report = matrix.run_matrix(
                work=work,
                manifest_path=manifest_path,
                ounce=root / 'ounce-core',
                machine='aarch64',
                runner=fake_runner,
            )
            self.assertFalse(report['failed'])
            self.assertEqual(report['total_harnesses'], 2)
            self.assertEqual(report['failed_harnesses'], 0)
            self.assertEqual(report['total_rows'], 5)
            self.assertEqual(report['passed_rows'], 5)
            self.assertEqual(report['failed_rows'], 0)
            self.assertEqual(report['harnesses']['lz4']['opt_matrix'], ['O1', 'O2', 'O3_M1'])
            self.assertEqual(report['harnesses']['lua']['opt_matrix'], ['O1', 'O2'])
            self.assertEqual(report['harnesses']['lua']['budget'], 4_000_000_000)

    def test_run_matrix_fails_when_any_row_fails_or_report_missing(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            manifest_path = self._write_manifest(root)
            work = root / 'work'
            work.mkdir()

            def fake_runner(cmd, harness, sub_work):
                sub_work.mkdir(parents=True, exist_ok=True)
                opt_str = cmd[cmd.index('--opt-matrix') + 1]
                levels = opt_str.split(',')
                if harness['name'] == 'lua':
                    # Simulate missing report.json and non-zero exit
                    return 1
                runs = {}
                for opt in levels:
                    failed = (opt == 'O3_M1')
                    runs[opt] = {
                        'failed': failed,
                        'checks': {'tier3': 'different' if failed else 'identical'},
                        'error': 'mismatch' if failed else None,
                        'report': str(sub_work / opt / 'report.json'),
                    }
                (sub_work / 'report.json').write_text(json.dumps({
                    'schema': 'ounce-core/linux-library-matrix-v1',
                    'harness': harness['name'],
                    'opt_matrix': levels,
                    'failed': True,
                    'runs': runs,
                    'checks': {},
                }), encoding='utf-8')
                return 1

            report = matrix.run_matrix(
                work=work,
                manifest_path=manifest_path,
                ounce=root / 'ounce-core',
                machine='aarch64',
                runner=fake_runner,
            )
            self.assertTrue(report['failed'])
            self.assertEqual(report['total_harnesses'], 2)
            self.assertEqual(report['failed_harnesses'], 2)
            self.assertEqual(report['total_rows'], 5)
            self.assertEqual(report['passed_rows'], 2)
            self.assertEqual(report['failed_rows'], 3)


if __name__ == '__main__':
    unittest.main()
