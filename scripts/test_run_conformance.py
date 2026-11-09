import json
from pathlib import Path
import sys
import tempfile
import unittest

from run_conformance import COUNTS, ROOT, run, verdict


class ConformanceTests(unittest.TestCase):
    def summary(self, **overrides):
        return '\n'.join(f'{k}: {overrides.get(k, v)}' for k, v in COUNTS.items())

    def test_unsupported_zero_exit_is_not_a_pass(self):
        self.assertFalse(verdict(self.summary(**{'emulator unimplemented': 1}), 0)[0])

    def test_missing_duplicate_and_failed_adapter_are_nonpasses(self):
        valid = self.summary()
        self.assertTrue(verdict(valid, 0)[0])
        for stdout, code in [('', 0), (valid + '\nMISMATCH: 0', 0), (valid.replace('MISMATCH: 0', 'MISMATCH: 0\nMISMATCH: 0'), 0), (valid, 1)]:
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


if __name__ == '__main__':
    unittest.main()
