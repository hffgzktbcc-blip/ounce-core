"""scripts/check_action_pins.py: every workflow `uses:` is pinned to a full commit SHA, except a short named list."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_action_pins  # noqa: E402

SHA = 'a' * 40


def run(workflow, pending=None):
    with tempfile.TemporaryDirectory() as tmp:
        wf = Path(tmp) / '.github' / 'workflows'
        wf.mkdir(parents=True)
        (wf / 'ci.yml').write_text(workflow)
        return check_action_pins.problems(Path(tmp), pending or {})


class ActionPins(unittest.TestCase):
    def test_a_sha_pinned_action_with_a_version_comment_passes(self):
        self.assertEqual(run(f'steps:\n  - uses: actions/checkout@{SHA} # v4\n'), [])

    def test_a_tag_is_refused_with_the_file_line_and_the_fix(self):
        out = run('steps:\n  - name: x\n    uses: actions/checkout@v4\n')
        self.assertEqual(len(out), 1)
        self.assertIn('ci.yml:3', out[0])
        self.assertIn('actions/checkout@v4', out[0])
        self.assertIn('40-character commit SHA', out[0])

    def test_a_branch_or_short_sha_is_refused(self):
        self.assertEqual(len(run('a:\n  - uses: x/y@main\n  - uses: x/y@abc1234\n')), 2)

    def test_subpath_actions_are_checked_like_any_other(self):
        self.assertEqual(len(run('- uses: github/codeql-action/init@v4\n')), 1)
        self.assertEqual(run(f'- uses: github/codeql-action/init@{SHA} # v4\n'), [])

    def test_local_actions_and_reusable_workflows_need_no_pin(self):
        self.assertEqual(run('jobs:\n  a:\n    uses: ./.github/workflows/external-corpus.yml\n'), [])

    def test_text_that_only_mentions_uses_is_not_a_reference(self):
        self.assertEqual(run('# uses: actions/checkout@v4\n- run: echo "uses: x/y@v1"\n- name: uses checkout\n'), [])

    def test_a_pending_action_is_allowed_only_while_it_is_still_unpinned(self):
        pending = {'x/y@v1': 'its SHA could not be looked up'}
        self.assertEqual(run('- uses: x/y@v1\n', pending), [])
        stale = run(f'- uses: x/y@{SHA}\n', pending)
        self.assertEqual(len(stale), 1)
        self.assertIn('no longer unpinned', stale[0])
        gone = run('- run: true\n', pending)
        self.assertEqual(len(gone), 1)
        self.assertIn('not used', gone[0])

    def test_the_committed_workflows_pass(self):
        self.assertEqual(check_action_pins.problems(check_action_pins.ROOT), [])


if __name__ == '__main__':
    unittest.main()
