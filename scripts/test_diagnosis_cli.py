import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

REPO = Path(__file__).resolve().parents[1]


class InstalledCliTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / 'installed' / 'ounce-diagnose.pyz'
        build = subprocess.run([sys.executable, '-B', str(REPO / 'scripts/build_diagnosis_cli.py'),
                                '--output', str(self.archive)], capture_output=True, text=True)
        self.assertEqual(build.returncode, 0, build.stderr)

    def command(self, *args):
        return subprocess.run([sys.executable, '-I', str(self.archive), *map(str, args)],
                              cwd=self.root, capture_output=True, text=True)

    def test_installed_archive_works_outside_checkout_with_python_only(self):
        version = self.command('--version')
        self.assertEqual(version.returncode, 0, version.stderr)
        self.assertEqual(version.stdout.strip(), 'ounce-diagnose 0.1.0')
        output = self.root / 'portable'
        result = self.command('example', '--output', output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Offline', result.stdout)
        report = json.loads((output / 'diagnosis.json').read_text())
        self.assertEqual(len(report['cases']), 2)
        for case in report['cases']:
            self.assertEqual(case['observations']['first_divergence']['index'], 1)
        for path in output.rglob('*'):
            if path.is_file():
                self.assertNotIn(str(self.root), path.read_text())
                self.assertNotIn('/Users/', path.read_text())
        digest = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.assertEqual(self.archive.with_suffix('.pyz.sha256').read_text().split()[0], digest)

    def test_missing_corrupt_evidence_and_existing_output_are_actionable(self):
        missing = self.command('bundle', '--evidence', self.root / 'missing', '--output', self.root / 'out')
        self.assertEqual(missing.returncode, 2)
        self.assertNotIn('Traceback', missing.stderr)
        evidence = self.root / 'bad'
        evidence.mkdir()
        (evidence / 'manifest.json').write_text('{bad')
        corrupt = self.command('bundle', '--evidence', evidence, '--output', self.root / 'out')
        self.assertEqual(corrupt.returncode, 2)
        self.assertNotIn('Traceback', corrupt.stderr)
        self.assertFalse((self.root / 'out').exists())
        output = self.root / 'existing'
        output.mkdir()
        (output / 'peer').write_text('keep')
        existing = self.command('example', '--output', output)
        self.assertEqual(existing.returncode, 2)
        self.assertEqual((output / 'peer').read_text(), 'keep')

    def test_replay_rejects_wrong_binary_before_starting_cargo(self):
        checkout = self.root / 'checkout'
        checkout.mkdir()
        corpus = self.root / 'corpus'
        nro = corpus / 'switch-examples/build/source/graphics/sdl2/sdl2-simple/sdl2-simple.nro'
        nro.parent.mkdir(parents=True)
        nro.write_bytes(b'wrong binary')
        result = self.command('replay', '--side', 'failure', '--checkout', checkout,
                              '--corpus-root', corpus, '--output', self.root / 'replay')
        self.assertEqual(result.returncode, 2)
        self.assertIn('SHA-256', result.stderr)
        self.assertFalse((self.root / 'replay').exists())


@unittest.skipUnless(os.name == 'posix', 'POSIX process-group lifecycle')
class ReplayProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def worker(self, timeout):
        # The descendant would leave a marker if the whole group were not stopped.
        child = "import time; from pathlib import Path; time.sleep(1); Path('leaked').write_text('alive')"
        command = [sys.executable, '-c',
                   'import subprocess,sys,time; subprocess.Popen([sys.executable,"-c",sys.argv[1]]); '
                   'print("started",flush=True); time.sleep(30)', child]
        code = ('import json,sys; from pathlib import Path; from diagnosis_cli import run_bounded; '
                'print(json.dumps(run_bounded(json.loads(sys.argv[1]),Path.cwd(),Path("run.log"),'
                'float(sys.argv[2]))),flush=True)')
        return subprocess.Popen([sys.executable, '-u', '-c', code, json.dumps(command), str(timeout)],
                                cwd=self.root, env=dict(os.environ, PYTHONPATH=str(REPO / 'scripts')),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def test_timeout_kills_and_reaps_descendants_and_retains_log(self):
        worker = self.worker(.15)
        out, err = worker.communicate(timeout=5)
        self.assertEqual(worker.returncode, 0, err)
        self.assertEqual(json.loads(out)['status'], 'timeout')
        self.assertEqual(json.loads(out)['exit_code'], 124)
        self.assertIn('started', (self.root / 'run.log').read_text())
        time.sleep(1.1)
        self.assertFalse((self.root / 'leaked').exists())

    def interrupt_worker(self, interrupt):
        worker = self.worker(10)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            log = self.root / 'run.log'
            if log.exists() and 'started' in log.read_text():
                break
            time.sleep(.02)
        else:
            worker.kill()
            self.fail('worker did not start')
        worker.send_signal(interrupt)
        out, err = worker.communicate(timeout=5)
        self.assertEqual(worker.returncode, 0, err)
        self.assertEqual(json.loads(out)['status'], 'interrupted')
        self.assertEqual(json.loads(out)['exit_code'], 130)
        time.sleep(1.1)
        self.assertFalse((self.root / 'leaked').exists())

    def test_interruption_kills_descendants_and_retains_log(self):
        self.interrupt_worker(signal.SIGINT)

    def test_termination_kills_descendants_and_retains_log(self):
        self.interrupt_worker(signal.SIGTERM)


if __name__ == '__main__':
    unittest.main()
