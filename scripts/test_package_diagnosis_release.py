"""The release source archive must contain only the reviewed toolkit inputs."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

REPO = Path(__file__).resolve().parents[1]


class ReleasePackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'repo'
        self.source.mkdir()
        # Real small inputs in a disposable committed repository, plus material
        # that must never be swept into a standalone source distribution.
        for relative in ['scripts', 'tools', 'docs/reproduce/sdl2-failure', 'docs/verify/audout-padding']:
            source = REPO / relative
            destination = self.source / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if relative in ['scripts', 'tools']:
                destination.mkdir()
                for path in source.glob('*.py'):
                    shutil.copyfile(path, destination / path.name)
            else:
                shutil.copytree(source, destination)
        shutil.copyfile(REPO / 'LICENSE', self.source / 'LICENSE')
        workflow = self.source / '.github/workflows/diagnosis-release.yml'
        workflow.parent.mkdir(parents=True)
        existing = REPO / '.github/workflows/diagnosis-release.yml'
        workflow.write_bytes(existing.read_bytes() if existing.exists() else b'name: fixture\n')
        (self.source / 'unreviewed.bin').write_bytes(b'do not ship')
        (self.source / 'src').mkdir()
        (self.source / 'src/runtime.rs').write_text('// not part of the Python tool\n')
        for args in [['init', '-q'], ['config', 'user.name', 'Test'],
                     ['config', 'user.email', 'test@example.invalid'], ['add', '.'],
                     ['commit', '-qm', 'fixture']]:
            subprocess.run(['git', *args], cwd=self.source, check=True, capture_output=True)

    def command(self, output):
        return subprocess.run([sys.executable, '-B', str(REPO / 'scripts/package_diagnosis_release.py'),
                               '--repo', str(self.source), '--output', str(output)],
                              capture_output=True, text=True)

    def test_source_export_builds_and_tests_without_the_runtime_checkout(self):
        output = self.root / 'release'
        result = self.command(output)
        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot = output / 'source'
        self.assertFalse((snapshot / '.git').exists())
        self.assertFalse((snapshot / 'src').exists())
        self.assertFalse((snapshot / 'unreviewed.bin').exists())
        provenance = json.loads((snapshot / 'SOURCE.json').read_text())
        self.assertEqual(len(provenance['original_commit']), 40)
        self.assertIn('scripts/diagnosis_cli.py', provenance['files_sha256'])
        with zipfile.ZipFile(output / 'ounce-diagnose-0.2.0-source.zip') as archive:
            names = archive.namelist()
            self.assertIn('scripts/build_diagnosis_cli.py', names)
            self.assertNotIn('unreviewed.bin', names)
            self.assertFalse(any(name.endswith(('.nro', '.elf', '.nca', '.keys')) for name in names))
        run = subprocess.run([sys.executable, '-I', str(output / 'ounce-diagnose-0.2.0.pyz'),
                              'audio-case', '--output', str(self.root / 'bundle')],
                             cwd=self.root, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertTrue((output / 'SHA256SUMS').is_file())

    def test_dirty_source_and_existing_output_are_not_published(self):
        (self.source / 'scripts/diagnosis_cli.py').write_text('# changed\n')
        output = self.root / 'release'
        dirty = self.command(output)
        self.assertEqual(dirty.returncode, 2)
        self.assertIn('clean', dirty.stderr)
        self.assertFalse(output.exists())
        subprocess.run(['git', 'restore', 'scripts/diagnosis_cli.py'], cwd=self.source, check=True)
        output.mkdir()
        (output / 'peer').write_text('keep')
        existing = self.command(output)
        self.assertEqual(existing.returncode, 2)
        self.assertEqual((output / 'peer').read_text(), 'keep')


if __name__ == '__main__':
    unittest.main()
