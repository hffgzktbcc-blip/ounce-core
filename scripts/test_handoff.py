import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = str(Path(__file__).with_name('handoff.py'))
ENV = dict(os.environ, GIT_AUTHOR_NAME='t', GIT_AUTHOR_EMAIL='t@t', GIT_COMMITTER_NAME='t', GIT_COMMITTER_EMAIL='t@t',
           GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_SYSTEM='/dev/null')


def git(cwd, *args):
    return subprocess.run(['git', *args], cwd=cwd, env=ENV, capture_output=True, text=True, check=True).stdout.strip()


def handoff(cwd, *args, agent='alice', ok=True):
    result = subprocess.run([sys.executable, '-B', SCRIPT, '--agent', agent, *args], cwd=cwd, env=ENV,
                            capture_output=True, text=True)
    if ok:
        assert result.returncode == 0, result.stderr + result.stdout
    return result


class HandoffTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.origin = root / 'origin.git'
        git(root, 'init', '--bare', '-q', '-b', 'main', str(self.origin))
        self.a = root / 'a'
        self.b = root / 'b'
        git(root, 'clone', '-q', str(self.origin), str(self.a))
        (self.a / '.gitignore').write_text('ignored.txt\n')
        (self.a / 'f.txt').write_text('one\n')
        git(self.a, 'add', '-A')
        git(self.a, 'commit', '-q', '-m', 'init')
        git(self.a, 'push', '-q', 'origin', 'HEAD:main')
        git(root, 'clone', '-q', str(self.origin), str(self.b))

    def tearDown(self):
        self.tmp.cleanup()

    def test_save_leaves_working_state_alone_and_captures_everything(self):
        (self.a / 'f.txt').write_text('two\n')
        (self.a / 'new.txt').write_text('new\n')
        (self.a / 'ignored.txt').write_text('x\n')
        (self.a / 'secret.keys').write_text('k\n')
        git(self.a, 'switch', '-q', '-c', 'topic')
        before = (git(self.a, 'status', '--porcelain'), git(self.a, 'rev-parse', 'HEAD'), git(self.a, 'branch', '--show-current'))
        handoff(self.a, 'save', '--note', 'half done', '--next', 'finish tests', '--task', 'R9')
        after = (git(self.a, 'status', '--porcelain'), git(self.a, 'rev-parse', 'HEAD'), git(self.a, 'branch', '--show-current'))
        self.assertEqual(before, after)
        files = git(self.a, 'ls-tree', '-r', '--name-only', 'refs/handoff/alice/topic').split()
        self.assertIn('new.txt', files)
        self.assertNotIn('ignored.txt', files)
        self.assertNotIn('secret.keys', files)
        self.assertEqual(git(self.a, 'show', 'refs/handoff/alice/topic:f.txt'), 'two')

    def test_unchanged_save_adds_no_commit(self):
        (self.a / 'f.txt').write_text('two\n')
        handoff(self.a, 'save')
        first = git(self.a, 'rev-parse', 'refs/handoff/alice/main')
        handoff(self.a, 'save')
        self.assertEqual(first, git(self.a, 'rev-parse', 'refs/handoff/alice/main'))

    def test_claims_conflict_expire_and_release(self):
        handoff(self.a, 'claim', 'R9', '--hours', '4', '--note', 'working')
        clash = handoff(self.b, 'claim', 'R9', agent='bob', ok=False)
        self.assertNotEqual(clash.returncode, 0)
        self.assertIn('claimed by alice', clash.stderr)
        handoff(self.a, 'claim', 'R9', '--hours', '-1')            # alice's claim is now expired
        handoff(self.b, 'claim', 'R9', agent='bob')                # so bob can take it
        status = handoff(self.a, 'status').stdout
        self.assertIn('bob', status)
        handoff(self.b, 'release', 'R9', agent='bob')
        self.assertNotIn('R9', handoff(self.a, 'status').stdout.split('Snapshots')[0].replace('Claims:', ''))

    def test_resume_on_another_clone_restores_unsaved_work(self):
        git(self.a, 'switch', '-q', '-c', 'topic')
        (self.a / 'f.txt').write_text('unsaved edit\n')
        (self.a / 'brand-new.txt').write_text('hello\n')
        handoff(self.a, 'save', '--next', 'run the gate', '--push')
        listing = handoff(self.b, 'resume').stdout
        self.assertIn('alice/topic', listing)
        self.assertIn('run the gate', listing)
        applied = handoff(self.b, 'resume', '--apply', 'alice/topic')
        self.assertIn('run the gate', applied.stdout)
        self.assertEqual((self.b / 'f.txt').read_text(), 'unsaved edit\n')
        self.assertEqual((self.b / 'brand-new.txt').read_text(), 'hello\n')
        self.assertTrue(git(self.b, 'branch', '--show-current').startswith('resume/topic'))
        self.assertIn('f.txt', git(self.b, 'status', '--porcelain'))  # the work is uncommitted, ready to review

    def test_resume_refuses_to_overwrite_local_work(self):
        handoff(self.a, 'save', '--push')
        (self.b / 'local.txt').write_text('mine\n')
        refused = handoff(self.b, 'resume', '--apply', 'alice/main', ok=False)
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn('not clean', refused.stderr)
        self.assertEqual((self.b / 'local.txt').read_text(), 'mine\n')

    def test_journal_is_shared_ordered_and_survives_concurrent_writers(self):
        handoff(self.a, 'log', 'decided to keep the 4h claim expiry', '--task', 'R9')
        handoff(self.b, 'log', 'PRFM must stay a no-op', agent='bob')       # b fetches a's entry, then appends
        handoff(self.a, 'log', 'ran the gate, 36 checks pass')               # a fetches b's entry, then appends
        text = handoff(self.b, 'journal', agent='bob').stdout.splitlines()
        self.assertEqual(len(text), 3)
        self.assertIn('ran the gate', text[0])                               # newest first
        self.assertIn('bob: PRFM must stay', text[1])
        self.assertIn('alice [R9]: decided', text[2])

    def test_digest_is_short_markdown_with_claims_snapshots_and_journal(self):
        handoff(self.a, 'claim', 'R9', '--note', 'hash tests')
        (self.a / 'f.txt').write_text('changed\n')
        handoff(self.a, 'save', '--next', 'run the gate', '--push')
        handoff(self.a, 'log', 'xxhash passes on all engines')
        digest = handoff(self.b, 'digest', agent='bob').stdout
        self.assertIn('R9 (alice', digest)
        self.assertIn('next: run the gate', digest)
        self.assertIn('unsaved work saved', digest)
        self.assertIn('xxhash passes on all engines', digest)
        self.assertLess(len(digest.splitlines()), 20)


if __name__ == '__main__':
    unittest.main()
