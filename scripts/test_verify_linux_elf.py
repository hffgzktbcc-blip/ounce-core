"""An output match alone must never certify an ELF execution."""
import unittest
from verify_linux_elf import execution_verdict, uses_qemu_reference


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


if __name__ == '__main__':
    unittest.main()
