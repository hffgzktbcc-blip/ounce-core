"""scripts/insn_budgets.py against a fake ounce-core that writes chosen reports, and checks of the committed budgets."""
import contextlib
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import insn_budgets  # noqa: E402

FIXTURE = 'tests/linux-elf/bin/hello.elf'

# Writes the report the FAKE_RESULTS entry for this tier describes: [insns, stop_kind], "crash" or "hang".
FAKE_OUNCE = r'''#!/usr/bin/env python3
import json, os, sys, time
args = sys.argv[1:]
tier = next(a.split('=', 1)[1] for a in args if a.startswith('--cpu='))
report = next(a.split('=', 1)[1] for a in args if a.startswith('--report-json='))
assert '--deterministic' in args, args
result = json.loads(os.environ['FAKE_RESULTS'])[tier]
if result == 'crash':
    print('[VM Error] fake crash', file=sys.stderr)
    sys.exit(12)
if result == 'hang':
    time.sleep(30)
insns, stop = result
with open(report, 'w') as f:
    json.dump({'schema': 'ounce.run.v1', 'insns_executed': insns, 'stop_kind': stop}, f)
'''


class BudgetCheck(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__('shutil').rmtree(self.tmp))
        self.fake = self.tmp / 'ounce-core'
        self.fake.write_text(FAKE_OUNCE)
        self.fake.chmod(self.fake.stat().st_mode | stat.S_IXUSR)
        self.budgets = self.tmp / 'budgets.json'
        self.write_budgets()

    def write_budgets(self, **overrides):
        spec = {'command': ['run-elf', FIXTURE], 'sha256': insn_budgets.sha256(insn_budgets.ROOT / FIXTURE),
                'stop_kind': 'clean_exit', 'baseline': 1000, 'budget': 1020}
        spec.update(overrides)
        self.budgets.write_text(json.dumps({'schema': insn_budgets.SCHEMA, 'headroom_percent': 2,
                                            'tiers': ['interpreter', 'tier3'], 'workloads': {'elf/hello': spec}}))

    def run_check(self, results, *extra):
        out = io.StringIO()
        env = {'FAKE_RESULTS': json.dumps(results), 'GITHUB_ACTIONS': ''}
        with mock.patch.dict(os.environ, env), contextlib.redirect_stdout(out):
            code = insn_budgets.main(['--binary', str(self.fake), '--budgets', str(self.budgets), *extra])
        return code, out.getvalue()

    def test_within_budget_passes_and_prints_the_table(self):
        code, out = self.run_check({'interpreter': [1015, 'clean_exit'], 'tier3': [1015, 'clean_exit']})
        self.assertEqual(code, 0, out)
        self.assertIn('| `elf/hello` | clean_exit | 1,015 | 1,020 | +1.5% |', out)
        self.assertIn('All 1 workloads within budget', out)

    def test_over_budget_fails_with_the_numbers_and_the_way_to_raise_it(self):
        code, out = self.run_check({'interpreter': [1100, 'clean_exit'], 'tier3': [1100, 'clean_exit']})
        self.assertEqual(code, 1)
        self.assertIn('FAIL elf/hello: 1,100 guest instructions is over its budget of 1,020 (+10.0% vs the baseline '
                      'of 1,000)', out)
        self.assertIn('--update', out)

    def test_exactly_at_budget_passes(self):
        code, out = self.run_check({'interpreter': [1020, 'clean_exit'], 'tier3': [1020, 'clean_exit']})
        self.assertEqual(code, 0, out)

    def test_a_different_stop_is_not_compared(self):
        code, out = self.run_check({'interpreter': [10, 'memory_fault'], 'tier3': [10, 'memory_fault']})
        self.assertEqual(code, 1)
        self.assertIn('stopped with memory_fault, recorded clean_exit', out)
        self.assertNotIn('over its budget', out)

    def test_tiers_that_count_differently_fail(self):
        code, out = self.run_check({'interpreter': [1000, 'clean_exit'], 'tier3': [1005, 'clean_exit']})
        self.assertEqual(code, 1)
        self.assertIn('tiers disagree: interpreter 1,000 (clean_exit), tier3 1,005 (clean_exit)', out)

    def test_a_run_without_a_report_fails_with_its_stderr(self):
        code, out = self.run_check({'interpreter': 'crash', 'tier3': [1000, 'clean_exit']})
        self.assertEqual(code, 1)
        self.assertIn('interpreter: wrote no report (exit status 12): [VM Error] fake crash', out)

    def test_a_run_that_no_longer_stops_fails(self):
        with mock.patch.object(insn_budgets, 'TIMEOUT_SECONDS', 1):
            code, out = self.run_check({'interpreter': 'hang', 'tier3': [1000, 'clean_exit']})
        self.assertEqual(code, 1)
        self.assertIn('interpreter: did not stop within 1 s', out)

    def test_a_rebuilt_fixture_needs_a_new_baseline(self):
        self.write_budgets(sha256='0' * 64)
        code, out = self.run_check({'interpreter': [1000, 'clean_exit'], 'tier3': [1000, 'clean_exit']})
        self.assertEqual(code, 1)
        self.assertIn('no longer matches the recorded SHA-256', out)

    def test_an_improvement_passes_with_a_notice(self):
        code, out = self.run_check({'interpreter': [900, 'clean_exit'], 'tier3': [900, 'clean_exit']})
        self.assertEqual(code, 0, out)
        self.assertIn('note: elf/hello: 900 guest instructions, below its baseline of 1,000 (-10.0%)', out)

    def test_update_records_the_count_and_a_budget_with_headroom(self):
        self.write_budgets(sha256='', baseline=0, budget=0)
        code, out = self.run_check({'interpreter': [5000, 'clean_exit'], 'tier3': [5000, 'clean_exit']}, '--update')
        self.assertEqual(code, 0, out)
        spec = json.loads(self.budgets.read_text())['workloads']['elf/hello']
        self.assertEqual((spec['baseline'], spec['budget']), (5000, 5100))
        self.assertEqual(spec['sha256'], insn_budgets.sha256(insn_budgets.ROOT / FIXTURE))

    def test_update_refuses_to_record_a_broken_run(self):
        before = self.budgets.read_text()
        code, _ = self.run_check({'interpreter': [5000, 'clean_exit'], 'tier3': [5001, 'clean_exit']}, '--update')
        self.assertEqual(code, 1)
        self.assertEqual(self.budgets.read_text(), before)

    def test_summary_file_gets_the_table_and_the_failures(self):
        summary = self.tmp / 'summary.md'
        code, _ = self.run_check({'interpreter': [1100, 'clean_exit'], 'tier3': [1100, 'clean_exit']},
                                 '--summary', str(summary))
        self.assertEqual(code, 1)
        text = summary.read_text()
        self.assertIn('### Instruction budgets', text)
        self.assertIn('**Over budget or not comparable:** elf/hello: 1,100', text)

    def test_budget_headroom_is_at_least_one_instruction(self):
        self.assertEqual(insn_budgets.budget_for(8, 2), 9)
        self.assertEqual(insn_budgets.budget_for(1_619_330, 2), 1_651_717)


class CommittedBudgets(unittest.TestCase):
    """The committed file must describe fixtures as they are, so a rebuilt fixture cannot slip past CI."""

    def setUp(self):
        self.data = json.loads(insn_budgets.DEFAULT_BUDGETS.read_text())

    def test_every_workload_points_at_the_committed_fixture(self):
        self.assertEqual(self.data['schema'], insn_budgets.SCHEMA)
        self.assertGreaterEqual(len(self.data['workloads']), 10)
        for name, spec in self.data['workloads'].items():
            with self.subTest(name):
                self.assertIn(spec['command'][0], ('run-nro', 'run-elf'))
                fixture = insn_budgets.ROOT / spec['command'][1]
                self.assertEqual(insn_budgets.sha256(fixture), spec['sha256'], 're-record with --update')
                self.assertGreater(spec['baseline'], 0)
                self.assertEqual(spec['budget'], insn_budgets.budget_for(spec['baseline'],
                                                                         self.data['headroom_percent']))
                self.assertNotIn('--deterministic', spec['command'], 'the check adds it')

    def test_both_portable_tiers_are_checked(self):
        self.assertEqual(self.data['tiers'], ['interpreter', 'tier3'])


if __name__ == '__main__':
    unittest.main()
