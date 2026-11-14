"""An output match alone must never certify an ELF execution."""
import json
import unittest
from verify_linux_elf import (
    diverge_verdict,
    execution_verdict,
    normalize_elf_build_flags,
    opt_flags,
    parse_opt_matrix,
    uses_qemu_reference,
)


class ExecutionVerdictTests(unittest.TestCase):
    def test_aarch64_reference_requires_qemu_only_on_other_architectures(self):
        h = {'elf_reference': 'aarch64'}
        for machine in ('arm64', 'aarch64', 'ARM64'):
            self.assertFalse(uses_qemu_reference(h, machine))
        for machine in ('x86_64', 'AMD64', 'riscv64'):
            self.assertTrue(uses_qemu_reference(h, machine))
        self.assertTrue(uses_qemu_reference({'elf_reference': 'qemu'}, 'arm64'))
        self.assertFalse(uses_qemu_reference({}, 'x86_64'))

    def test_matching_output_with_guest_failure_is_not_a_pass(self):
        self.assertEqual(execution_verdict(b'fails=0\n', b'fails=0\n', 71,
                                          '[run-elf] engine=Tier-3 instructions=10 syscalls=1', 'tier3'),
                         'execution-failed')

    def test_tier4_fallback_is_not_native_jit_evidence(self):
        self.assertEqual(execution_verdict(b'ok', b'ok', 0,
                                          '[run-elf] engine=Tier-3 instructions=10 syscalls=1', 'tier4'),
                         'engine-not-confirmed')

    def test_missing_execution_summary_is_not_a_pass(self):
        self.assertEqual(execution_verdict(b'ok', b'ok', 0, '', 'tier3'), 'engine-not-confirmed')

    def test_different_output_is_reported(self):
        self.assertEqual(execution_verdict(b'correct', b'wrong', 0,
                                          '[run-elf] engine=Tier-3 instructions=10 syscalls=1', 'tier3'),
                         'different')

    def test_interpreter_identity_is_required(self):
        self.assertEqual(execution_verdict(b'ok', b'ok', 0,
                                          '[run-elf] engine=Interpreter instructions=10 syscalls=1', 'interpreter'),
                         'identical')
        self.assertEqual(execution_verdict(b'ok', b'ok', 0,
                                          '[run-elf] engine=Tier-3 instructions=10 syscalls=1', 'interpreter'),
                         'engine-not-confirmed')

    def test_matching_successful_requested_engine_passes(self):
        self.assertEqual(execution_verdict(b'ok', b'ok', 0,
                                          '[run-elf] engine=Tier-4 instructions=10 syscalls=1', 'tier4'),
                         'identical')


class OptMatrixTests(unittest.TestCase):
    def test_opt_flags_mapping_and_apple_m1_isolation(self):
        self.assertEqual(opt_flags('O1'), (['-O1'], ['-O1']))
        self.assertEqual(opt_flags('O2'), (['-O2'], ['-O2']))
        self.assertEqual(opt_flags('O3'), (['-O3'], ['-O3']))
        host_fl, elf_fl = opt_flags('O3_M1')
        self.assertEqual(host_fl, ['-O3'])
        self.assertEqual(elf_fl, ['-O3', '-mcpu=apple_m1'])
        self.assertNotIn('-mcpu=apple_m1', host_fl)
        with self.assertRaises(ValueError):
            opt_flags('Os')

    def test_parse_opt_matrix_single_multi_and_invalid(self):
        self.assertEqual(parse_opt_matrix('O1', None), ['O1'])
        self.assertEqual(parse_opt_matrix('O2', None), ['O2'])
        self.assertEqual(parse_opt_matrix('O1', 'O1, O2, O3_M1, O2'), ['O1', 'O2', 'O3_M1'])
        with self.assertRaises(ValueError):
            parse_opt_matrix('O1', '')
        with self.assertRaises(ValueError):
            parse_opt_matrix('O1', 'O1,O9')
        with self.assertRaises(ValueError):
            parse_opt_matrix('O9', None)

    def test_normalize_elf_build_flags_adapts_crypto_march(self):
        self.assertEqual(
            normalize_elf_build_flags(['-march=armv8-a+crypto', '-DPMULL_LIB'], ['-O2']),
            ['-mcpu=generic+crypto', '-DPMULL_LIB'],
        )
        self.assertEqual(
            normalize_elf_build_flags(['-march=armv8-a+crypto', '-DPMULL_LIB'], ['-O3', '-mcpu=apple_m1']),
            ['-DPMULL_LIB'],
        )


class DivergeVerdictTests(unittest.TestCase):
    def _ok_report(self):
        return {
            'mode': 'elf',
            'checked': 2,
            'mismatched': 0,
            'mismatches': [],
            'engines': [
                {'engine': 'interpreter', 'exit_code': 0, 'stop': 'exited'},
                {'engine': 'Tier-3', 'exit_code': 0, 'stop': 'exited'},
                {'engine': 'Tier-4', 'exit_code': 0, 'stop': 'exited'},
            ],
            'not_checked': [],
        }

    def test_all_engines_identical_passes(self):
        self.assertEqual(diverge_verdict(0, self._ok_report()), 'identical')
        self.assertEqual(diverge_verdict(0, json.dumps(self._ok_report())), 'identical')

    def test_mismatch_returns_different(self):
        rep = self._ok_report()
        rep['mismatched'] = 1
        rep['mismatches'] = [{'kind': 'stdout'}]
        self.assertEqual(diverge_verdict(1, rep), 'different')
        self.assertEqual(diverge_verdict(0, rep), 'different')
        self.assertEqual(diverge_verdict(1, self._ok_report()), 'different')

    def test_missing_tier4_is_not_confirmed(self):
        rep = self._ok_report()
        rep['checked'] = 1
        rep['engines'] = [e for e in rep['engines'] if e['engine'] != 'Tier-4']
        rep['not_checked'] = [{'engine': 'Tier-4', 'reason': 'skipped'}]
        self.assertEqual(diverge_verdict(0, rep), 'engine-not-confirmed')

    def test_agreeing_on_guest_fault_or_budget_is_execution_failed(self):
        rep = self._ok_report()
        for e in rep['engines']:
            e['exit_code'] = 73
            e['stop'] = 'budget'
        self.assertEqual(diverge_verdict(0, rep), 'execution-failed')

    def test_missing_report_or_error_exit(self):
        self.assertEqual(diverge_verdict(2, None), 'execution-failed')
        self.assertEqual(diverge_verdict(2, 'not json'), 'execution-failed')
        self.assertEqual(diverge_verdict(0, None), 'engine-not-confirmed')


if __name__ == '__main__':
    unittest.main()
