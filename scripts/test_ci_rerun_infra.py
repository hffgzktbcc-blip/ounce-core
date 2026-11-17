"""Run the CI infrastructure re-run decision from its workflow against recorded job shapes, with a fake `gh`."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest

WORKFLOW = Path(__file__).resolve().parents[1] / '.github/workflows/ci-rerun-infra.yml'

FAKE_GH = r'''#!/usr/bin/env python3
import os, subprocess, sys
args = sys.argv[1:]
if args[:2] == ['run', 'rerun']:
    with open(os.environ['RERUN_LOG'], 'a') as log:
        log.write(' '.join(args) + '\n')
    sys.exit(0)
if args[:1] == ['api'] and '--jq' in args:
    query = args[args.index('--jq') + 1]
    if any('/jobs' in a for a in args):
        sys.exit(subprocess.run(['jq', '-r', query, os.environ['JOBS_JSON']]).returncode)
    print(os.environ['CURRENT_ATTEMPT'])
    sys.exit(0)
print('unexpected gh call: ' + ' '.join(args), file=sys.stderr)
sys.exit(99)
'''


def ran(name, conclusion='success'):
    return {'name': name, 'conclusion': conclusion, 'runner_id': 1000025162,
            'runner_name': 'GitHub Actions 1000025162', 'steps': [{'name': 'Set up job'}]}


def never_started(name):
    # The jobs listing reports a job that never reached a runner like this (run 44144034971, attempt 1).
    return {'name': name, 'conclusion': 'cancelled', 'runner_id': None, 'runner_name': None, 'steps': []}


def never_started_single_job_shape(name):
    # The single-job endpoint reports the same state as 0 and '' (job 134275612699, attempt 2).
    return {'name': name, 'conclusion': 'cancelled', 'runner_id': 0, 'runner_name': '', 'steps': []}


SUMMARIES = [ran('Test (ubuntu-latest)', 'failure'), ran('Test (windows-latest)', 'cancelled')]


@unittest.skipUnless(shutil.which('jq') and shutil.which('bash'), 'needs jq and bash')
class RerunDecisionTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.dir = Path(temp.name)
        (self.dir / 'bin').mkdir()
        gh = self.dir / 'bin' / 'gh'
        gh.write_text(FAKE_GH)
        gh.chmod(0o755)
        self.script = textwrap.dedent(WORKFLOW.read_text().split('run: |\n', 1)[1])

    def decide(self, jobs, attempt, current=None):
        """Return (re-run requested?, output) for a finished CI attempt with these jobs."""
        (self.dir / 'jobs.json').write_text(json.dumps({'total_count': len(jobs), 'jobs': jobs}))
        log = self.dir / 'rerun.log'
        log.unlink(missing_ok=True)
        env = dict(os.environ, PATH=f"{self.dir / 'bin'}{os.pathsep}{os.environ['PATH']}",
                   GH_TOKEN='unused', REPO='owner/repo', RUN_ID='44144034971', RUN_ATTEMPT=str(attempt),
                   CURRENT_ATTEMPT=str(current or attempt), JOBS_JSON=str(self.dir / 'jobs.json'),
                   RERUN_LOG=str(log))
        result = subprocess.run(['bash', '-c', self.script], env=env, capture_output=True, text=True, timeout=30)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        requested = log.exists()
        if requested:
            self.assertEqual('run rerun 44144034971 --repo owner/repo --failed\n', log.read_text())
        return requested, result.stdout

    def test_first_attempt_reruns_when_only_cancelled_jobs_failed(self):
        jobs = [ran('Lint (clippy, warnings are errors)'), ran('Runtime (macos-latest)', 'cancelled'),
                never_started('Release notice packaging (macos-latest)'), *SUMMARIES]
        self.assertTrue(self.decide(jobs, attempt=1)[0])

    def test_second_attempt_reruns_jobs_that_never_reached_a_runner(self):
        # PR #490: the macOS release-notice job waited 15 minutes on attempt 2 and was cancelled unstarted.
        for shape in (never_started, never_started_single_job_shape):
            jobs = [ran('Lint (clippy, warnings are errors)'), shape('Release notice packaging (macos-latest)'),
                    *SUMMARIES]
            requested, out = self.decide(jobs, attempt=2)
            self.assertTrue(requested, out)
            self.assertIn('re-run 2 of 2', out)

    def test_second_attempt_leaves_a_job_that_started_and_was_cancelled(self):
        jobs = [never_started('Release notice packaging (macos-latest)'), ran('Runtime (macos-latest)', 'cancelled'),
                *SUMMARIES]
        requested, out = self.decide(jobs, attempt=2)
        self.assertFalse(requested)
        self.assertIn('only jobs that never started', out)

    def test_a_real_failure_is_never_rerun(self):
        for conclusion in ('failure', 'timed_out'):
            for attempt in (1, 2):
                jobs = [never_started('Release notice packaging (macos-latest)'), ran('Runtime (ubuntu-latest)', conclusion),
                        *SUMMARIES]
                requested, out = self.decide(jobs, attempt=attempt)
                self.assertFalse(requested, (conclusion, attempt))
                self.assertIn('really failed', out)

    def test_failed_summaries_alone_are_not_rerun(self):
        requested, out = self.decide([ran('Lint (clippy, warnings are errors)'), *SUMMARIES], attempt=1)
        self.assertFalse(requested)
        self.assertIn('nothing to re-run', out)

    def test_no_third_automatic_rerun(self):
        requested, out = self.decide([never_started('Release notice packaging (macos-latest)'), *SUMMARIES], attempt=3)
        self.assertFalse(requested)
        self.assertIn('past the automatic limit', out)

    def test_a_run_someone_already_reran_is_left_alone(self):
        jobs = [never_started('Release notice packaging (macos-latest)'), *SUMMARIES]
        requested, out = self.decide(jobs, attempt=1, current=2)
        self.assertFalse(requested)
        self.assertIn('already on attempt 2', out)

    def test_only_failed_runs_reach_the_script_and_it_owns_the_attempt_limit(self):
        condition = WORKFLOW.read_text().split('    if: ', 1)[1].split('\n', 1)[0]
        self.assertEqual("github.event.workflow_run.conclusion == 'failure'", condition)


if __name__ == '__main__':
    unittest.main()
