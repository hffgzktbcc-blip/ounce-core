import hashlib
import shutil
import subprocess
import json
from pathlib import Path
import sys
import tempfile
import unittest

from run_conformance import COUNTS, ROOT, run, validate_trace, verdict


class ConformanceTests(unittest.TestCase):
    def summary(self, **overrides):
        return '\n'.join(f'{k}: {overrides.get(k, v)}' for k, v in COUNTS.items())

    def test_unsupported_zero_exit_is_not_a_pass(self):
        self.assertFalse(verdict(self.summary(**{'emulator unimplemented': 1}), 0)[0])

    def test_missing_duplicate_and_failed_adapter_are_nonpasses(self):
        valid = self.summary()
        self.assertTrue(verdict(valid, 0)[0])
        for stdout, code in [('', 0), (valid + '\nMISMATCH: 0', 0), (valid.replace('MISMATCH: 0', 'MISMATCH: 0\nMISMATCH: 0'), 0), (valid + '\nMISMATCH: -1', 0), (valid, 1)]:
            self.assertFalse(verdict(stdout, code)[0])

    def test_tampering_rejected_before_adapter_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            suite = Path(tmp)
            (suite / 'case.trace').write_text('changed')
            (suite / 'suite.json').write_text(json.dumps({
                'schema': 'ounce-core/conformance-v1', 'cases': [
                    {'id': 'test', 'path': 'case.trace', 'sha256': '0' * 64}]}))
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                run(suite, ['nonexistent-adapter'])

    def test_real_suite_adapter_observations_retained(self):
        # Synthetic adapter tests plumbing only, not emulator correctness.
        result = run(ROOT / 'conformance/arm64-v0.1.0',
                     [sys.executable, '-c', 'print(' + repr(self.summary()) + ')'])
        self.assertTrue(result['passed'])
        self.assertEqual(result['cases'][0]['counts'], COUNTS)

    def test_v020_validates_all_twelve_real_inputs(self):
        result = run(ROOT / 'conformance/arm64-v0.2.0',
                     [sys.executable, '-c', 'print(' + repr(self.summary()) + ')'])
        self.assertTrue(result['passed'])
        self.assertEqual(len(result['cases']), 12)
        self.assertEqual(len({c['id'] for c in result['cases']}), 12)

    def test_trace_shape_and_register_bounds(self):
        trace = (ROOT / 'conformance/arm64-v0.2.0/cases/default-nan-mul2d.trace').read_bytes()
        for invalid in (trace + trace, b'U ' + trace[2:]):
            with self.assertRaises(ValueError):
                validate_trace(invalid, '6e62dc20')
        fields = trace.decode().split()
        for index, value in ((2, '10'), (3, '10000000000000000'), (34, '1' + '0' * 32), (68, '100000000')):
            changed = fields.copy()
            changed[index] = value
            with self.assertRaisesRegex(ValueError, 'invalid register'):
                validate_trace(' '.join(changed).encode(), '6e62dc20')

    def test_v020_cli_rejects_tampering_before_execution(self):
        for rehash in (False, True):
            with self.subTest(rehash=rehash), tempfile.TemporaryDirectory() as tmp:
                suite = Path(tmp) / 'suite'
                shutil.copytree(ROOT / 'conformance/arm64-v0.2.0', suite)
                manifest = json.loads((suite / 'suite.json').read_text())
                case = manifest['cases'][-1]
                trace = suite / case['path']
                trace.write_text(trace.read_text().replace(case['word'], 'd503201f', 1))
                if rehash:
                    case['sha256'] = hashlib.sha256(trace.read_bytes()).hexdigest()
                    (suite / 'suite.json').write_text(json.dumps(manifest))
                report = Path(tmp) / 'report.json'
                result = subprocess.run(
                    [sys.executable, str(ROOT / 'scripts/run_conformance.py'),
                     '--suite', str(suite), '--report', str(report), '--', 'nonexistent-adapter'],
                    capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn('word differs' if rehash else 'hash mismatch', result.stderr)
                self.assertFalse(report.exists())


if __name__ == '__main__':
    unittest.main()
