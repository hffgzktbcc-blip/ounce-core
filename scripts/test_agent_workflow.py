"""Exercise agent commands in real disposable repositories, without Cargo or GitHub."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parent


class WorkflowTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'scripts').mkdir()
        for name in ('next.sh', 'gate.sh', 'agent_workflow.py', 'ci_scope.py'):
            if (SOURCE / name).exists():
                shutil.copy(SOURCE / name, self.root / 'scripts' / name)
        self.git('init', '-q', '-b', 'work')
        self.git('config', 'user.name', 'Workflow Test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.write('PLAN.md', '- [ ] **A. Waiting.**\n      Status: blocked\n      Blocked by: outside review\n- [ ] **B. Useful task.**\n      Status: ready\n      Next: reproduce a failure\n')
        self.write('.gitignore', 'target/\n*.so\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'baseline')
        self.bin = self.root / 'fake-bin'
        self.bin.mkdir()
        for name in ('gh', 'cargo'):
            self.write(f'fake-bin/{name}', '#!/bin/sh\necho "unexpected external command" >&2\nexit 99\n')
            (self.bin / name).chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'])

    def git(self, *args):
        return subprocess.run(['git', *args], cwd=self.root, check=True,
                              capture_output=True, text=True).stdout.strip()

    def write(self, path, text):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def command(self, script, *args):
        return subprocess.run(['bash', f'scripts/{script}.sh', *args], cwd=self.root,
                              env=self.env, capture_output=True, text=True, timeout=15)

    def test_next_skips_blocked_and_does_not_fetch_or_mutate(self):
        before = self.git('status', '--porcelain'), self.git('rev-parse', 'HEAD')
        result = self.command('next')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('B. Useful task', result.stdout)
        self.assertNotIn('unexpected external command', result.stderr)
        self.assertEqual(before, (self.git('status', '--porcelain'), self.git('rev-parse', 'HEAD')))

    def test_dependencies_and_all_blocked_are_reported_without_selecting_done_work(self):
        self.write('PLAN.md', '- [x] **A. Complete.**\n- [ ] **B. Pending.**\n      Depends: C\n- [ ] **C. External.**\n      Status: blocked\n')
        result = self.command('next')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('No ready task', result.stdout)
        self.assertIn('B', result.stdout)
        self.write('PLAN.md', '- [x] **A. Complete.**\n- [ ] **B. Ready now.**\n      Depends: A\n')
        self.assertIn('B. Ready now', self.command('next').stdout)

    def test_duplicate_ids_fail_before_any_change(self):
        self.write('PLAN.md', '- [x] **15. Old.**\n- [ ] **15. New.**\n')
        original = (self.root / 'PLAN.md').read_bytes()
        result = self.command('next')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('duplicate', result.stderr.lower())
        self.assertEqual(original, (self.root / 'PLAN.md').read_bytes())

    def test_unknown_dependency_and_requested_id_are_actionable_errors(self):
        self.write('PLAN.md', '- [ ] **B. Task.**\n      Depends: TYPO\n')
        self.assertIn('unknown dependency', self.command('next').stderr.lower())
        self.write('PLAN.md', '- [ ] **B. Task.**\n')
        self.assertIn('unknown task', self.command('next', 'TYPO').stderr.lower())

    def test_checkpoint_is_local_staged_only_and_does_not_edit_plan(self):
        plan = (self.root / 'PLAN.md').read_bytes()
        self.write('owned.txt', 'my change\n')
        self.write('peer.txt', 'leave me alone\n')
        self.write('target/cache.so', 'local build')
        self.git('add', 'owned.txt')
        result = self.command('gate', '--checkpoint', 'save useful progress', '--next', 'run focused reproduction')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.git('diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD'), 'owned.txt')
        self.assertIn('peer.txt', self.git('status', '--porcelain'))
        self.assertEqual(plan, (self.root / 'PLAN.md').read_bytes())
        self.assertIn('not run', self.git('log', '-1', '--format=%B').lower())

    def test_checkpoint_no_staged_changes_does_not_sweep_other_work(self):
        self.write('peer.txt', 'unfinished')
        before = self.git('rev-parse', 'HEAD')
        result = self.command('gate', '--checkpoint', 'nothing staged')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('stage', result.stderr.lower())
        self.assertEqual(before, self.git('rev-parse', 'HEAD'))

    def test_staged_key_is_rejected_but_unstaged_key_does_not_block_save(self):
        self.write('private.keys', 'secret')
        self.git('add', 'private.keys')
        self.assertNotEqual(self.command('gate', '--checkpoint', 'bad input').returncode, 0)
        self.git('reset', '-q', '--', 'private.keys')
        self.write('owned.txt', 'okay')
        self.git('add', 'owned.txt')
        result = self.command('gate', '--checkpoint', 'safe work')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_argument_is_clear_and_does_not_commit(self):
        result = self.command('gate', '--checkpoint')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('argument', result.stderr.lower())

    def test_failed_push_preserves_local_checkpoint(self):
        self.write('owned.txt', 'saved even offline')
        self.git('add', 'owned.txt')
        before = self.git('rev-parse', 'HEAD')
        result = self.command('gate', '--checkpoint', 'offline progress', '--push')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('saved locally', result.stderr)
        self.assertNotEqual(before, self.git('rev-parse', 'HEAD'))
        self.assertEqual(self.git('status', '--porcelain'), '?? fake-bin/')

    def test_submit_creates_pr_without_tests_ticks_or_merge(self):
        # Stub the remote transport only; the branch, index and commits are real.
        git_binary = shutil.which('git')
        self.write('fake-bin/git', f'#!/bin/sh\nif [ "$1" = push ]; then exit 0; fi\nexec "{git_binary}" "$@"\n')
        (self.bin / 'git').chmod(0o755)
        self.write('fake-bin/gh', '#!/bin/sh\ncase "$*" in\n  "pr view --json url --jq .url") exit 1;;\n  "pr create "*) echo https://github.com/example/core/pull/1;;\n  *) echo "unexpected gh call: $*" >&2; exit 99;;\nesac\n')
        (self.bin / 'gh').chmod(0o755)
        before = self.git('rev-parse', 'HEAD'), (self.root / 'PLAN.md').read_bytes()
        result = self.command('gate', '--ship')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('/pull/1', result.stdout)
        self.assertEqual(before, (self.git('rev-parse', 'HEAD'), (self.root / 'PLAN.md').read_bytes()))


if __name__ == '__main__':
    unittest.main()
