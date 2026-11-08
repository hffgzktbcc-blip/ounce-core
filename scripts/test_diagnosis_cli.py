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
        self.assertEqual(version.stdout.strip(), 'ounce-diagnose 0.2.0')
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

    def test_installed_audio_case_preserves_records_and_explains_padding(self):
        output = self.root / 'audio'
        result = self.command('audio-case', '--output', output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Offline audio diagnosis', result.stdout)
        self.assertIn('No guest executed', result.stdout)
        report = json.loads((output / 'diagnosis.json').read_text())
        self.assertEqual(report['schema_version'], 'ounce-diagnose/audio-padding-v1')
        self.assertEqual(report['engines'], ['Interpreter', 'Tier3'])
        self.assertTrue(report['recorded_repair_matches'])
        for case in report['cases']:
            self.assertEqual(case['first_rejected_open'], 1)
            self.assertEqual(case['raw_channels'], 65536)
            self.assertEqual(case['low_channel_halfword'], 0)
            self.assertEqual(case['padding_halfword'], 1)
            self.assertEqual(case['rejected_before'], 7)
            self.assertEqual(case['stereo_after'], 7)
        for name in ['before.json', 'after.json']:
            self.assertEqual((output / name).read_bytes(),
                             (REPO / 'docs/verify/audout-padding' / name).read_bytes())
        self.assertIn('0b8b38a35dd73a97209ee43ec818904720bd6432',
                      (output / 'REPLAY.md').read_text())
        for path in output.rglob('*'):
            if path.is_file():
                self.assertNotIn(str(self.root), path.read_text())
                self.assertNotIn('/Users/', path.read_text())

    def test_audio_dossier_rejects_corruption_and_semantically_forged_records(self):
        evidence = self.root / 'audio-evidence'
        shutil.copytree(REPO / 'docs/verify/audout-padding', evidence)
        original = (evidence / 'after.json').read_bytes()
        manifest_path = evidence / 'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        def check_error(expected):
            result = self.command('audio-case', '--evidence', evidence, '--output', self.root / 'out')
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn(expected, result.stderr)
            self.assertNotIn('Traceback', result.stderr)
            self.assertFalse((self.root / 'out').exists())
        (evidence / 'after.json').write_text('{bad')
        check_error('SHA-256')
        for mutation in ['claim', 'input', 'duplicate', 'boolean']:
            document = json.loads(original)
            if mutation == 'claim':
                document['runs'][0]['audio_open'][1]['result'] = 62977
                document['runs'][0]['audio_open'][1]['channels'] = None
            elif mutation == 'input':
                document['input']['buttons'] = 2
            elif mutation == 'duplicate':
                document['runs'][1] = document['runs'][0]
            else:
                document['runs'][0]['audio_open'][1]['result'] = False
            (evidence / 'after.json').write_text(json.dumps(document))
            manifest['files_sha256']['after.json'] = hashlib.sha256((evidence / 'after.json').read_bytes()).hexdigest()
            manifest_path.write_text(json.dumps(manifest))
            check_error('audio case')

    def test_audio_case_does_not_promote_an_unrepaired_after_report(self):
        evidence = self.root / 'unrepaired'
        shutil.copytree(REPO / 'docs/verify/audout-padding', evidence)
        (evidence / 'after.json').write_bytes((evidence / 'before.json').read_bytes())
        manifest_path = evidence / 'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        manifest['files_sha256']['after.json'] = hashlib.sha256((evidence / 'after.json').read_bytes()).hexdigest()
        manifest_path.write_text(json.dumps(manifest))
        output = self.root / 'diagnosed'
        result = self.command('audio-case', '--evidence', evidence, '--output', output)
        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads((output / 'diagnosis.json').read_text())
        self.assertFalse(report['recorded_repair_matches'])
        self.assertTrue(all(case['stereo_after'] == 0 for case in report['cases']))
        self.assertIn('recorded repair not matched', result.stdout)

    def test_audio_case_preserves_existing_output_and_rejects_evidence_escape(self):
        output = self.root / 'exists'
        output.mkdir()
        (output / 'peer').write_text('keep')
        existing = self.command('audio-case', '--output', output)
        self.assertEqual(existing.returncode, 2)
        self.assertEqual((output / 'peer').read_text(), 'keep')
        evidence = self.root / 'evidence'
        shutil.copytree(REPO / 'docs/verify/audout-padding', evidence)
        manifest_path = evidence / 'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        manifest['files_sha256']['../outside'] = 'a' * 64
        manifest_path.write_text(json.dumps(manifest))
        result = self.command('audio-case', '--evidence', evidence, '--output', self.root / 'out')
        self.assertEqual(result.returncode, 2)
        self.assertIn('audio case', result.stderr)
        self.assertFalse((self.root / 'out').exists())


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
