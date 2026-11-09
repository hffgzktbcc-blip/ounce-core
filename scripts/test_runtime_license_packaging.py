"""Release archives must retain notices for the toolchain that built their binary."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import subprocess
from unittest.mock import patch
import zipfile

import package_release
from collect_toolchain_notices import collect


class RuntimeLicensePackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'docs').mkdir()
        (self.root / 'assets/fonts').mkdir(parents=True)
        for name in ['LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_NOTICES.txt',
                     'Cargo.lock', 'Cargo.toml', 'assets/fonts/DejaVu-LICENSE.txt']:
            (self.root / name).write_text('test input: ' + name)
        review = {'status': 'REVIEWED', 'reviewer': 'test fixture only',
                  'decision_evidence': 'test fixture only', 'open_items': []}
        for name, key in [('Cargo.lock', 'cargo_lock_sha256'),
                          ('Cargo.toml', 'cargo_manifest_sha256'),
                          ('THIRD_PARTY_NOTICES.txt', 'notices_sha256')]:
            review[key] = self.digest(self.root / name)
        (self.root / 'docs/RELEASE_NOTICE_REVIEW.json').write_text(json.dumps(review))
        self.binary = self.root / 'ounce-core'
        self.binary.write_bytes(b'test binary, not an executable')
        self.output = self.root / 'release.zip'
        self.notices = self.root / 'toolchain-notices'
        self.patch = patch.object(package_release, '__file__', str(self.root / 'scripts/package_release.py'))
        self.patch.start()
        self.addCleanup(self.patch.stop)

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def make_notices(self):
        self.notices.mkdir()
        for name in ['COPYRIGHT.html', 'LICENSE-MIT', 'LICENSE-APACHE']:
            (self.notices / name).write_text('retained toolchain notice: ' + name)
        data = {'schema_version': 'ounce-core/toolchain-notices-v1',
                'binary_sha256': self.digest(self.binary),
                'rustc_version': 'rustc fixture only', 'target': 'x86_64-unknown-linux-gnu',
                'files_sha256': {p.name: self.digest(p) for p in self.notices.iterdir()}}
        (self.notices / 'manifest.json').write_text(json.dumps(data))
        return data

    def test_publication_without_actual_toolchain_notices_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'toolchain notices'):
            package_release.package(self.binary, self.output, True)
        self.assertFalse(self.output.exists())

    def test_linux_publication_requires_binary_bound_native_notices(self):
        self.binary.write_bytes(b'\x7fELFfixture only')
        self.make_notices()
        with self.assertRaisesRegex(ValueError, 'native'):
            package_release.package(self.binary, self.output, True, self.notices)
        self.assertFalse(self.output.exists())

    def test_native_notices_are_retained_and_tampering_rejected(self):
        self.binary.write_bytes(b'\x7fELFfixture only')
        self.make_notices()
        native = self.root / 'native'
        (native / 'packages/example').mkdir(parents=True)
        notice = native / 'packages/example/copyright.txt'
        notice.write_text('actual installed package copyright fixture')
        (native / 'common').mkdir()
        lgpl = native / 'common/LGPL-2.1.txt'
        lgpl.write_text('installed LGPL text fixture')
        data = {'schema_version': 'ounce-core/native-notices-v1',
                'binary_sha256': self.digest(self.binary),
                'libraries': [{'soname': 'libexample.so', 'package': 'example', 'version': '1'}],
                'files_sha256': {'packages/example/copyright.txt': self.digest(notice),
                                 'common/LGPL-2.1.txt': self.digest(lgpl)}}
        (native / 'manifest.json').write_text(json.dumps(data))
        package_release.package(self.binary, self.output, True, self.notices, native)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read('licenses/native/packages/example/copyright.txt'), notice.read_bytes())
        for field, value in [('binary_sha256', '0' * 64),
                             ('files_sha256', {'../outside': '0' * 64}),
                             ('files_sha256', {'packages/example/copyright.txt': self.digest(notice)})]:
            bad = dict(data)
            bad[field] = value
            (native / 'manifest.json').write_text(json.dumps(bad))
            with self.assertRaises(ValueError):
                package_release.package(self.binary, self.root / 'rejected.zip', True, self.notices, native)
            self.assertFalse((self.root / 'rejected.zip').exists())
        (native / 'manifest.json').write_text(json.dumps(data))
        notice.write_text('tampered copyright')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            package_release.package(self.binary, self.root / 'rejected.zip', True, self.notices, native)
        self.assertFalse((self.root / 'rejected.zip').exists())

    def test_archive_retains_the_bound_notices_and_manifest(self):
        self.make_notices()
        package_release.package(self.binary, self.output, True, self.notices)
        with zipfile.ZipFile(self.output) as archive:
            for name in ['manifest.json', 'COPYRIGHT.html', 'LICENSE-MIT', 'LICENSE-APACHE']:
                self.assertEqual(archive.read('licenses/rust/' + name),
                                 (self.notices / name).read_bytes())

    def test_changed_binary_and_notice_bytes_are_rejected(self):
        self.make_notices()
        for changed in ['binary', 'notice']:
            with self.subTest(changed=changed):
                self.binary.write_bytes(b'test binary, not an executable')
                (self.notices / 'LICENSE-MIT').write_text('retained toolchain notice: LICENSE-MIT')
                if changed == 'binary':
                    self.binary.write_bytes(b'other build')
                else:
                    (self.notices / 'LICENSE-MIT').write_text('altered notice')
                with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                    package_release.package(self.binary, self.output, True, self.notices)
                self.assertFalse(self.output.exists())

    def test_collector_preserves_actual_files_without_local_sysroot_paths(self):
        sysroot = self.root / 'private-local-sysroot'
        source = sysroot / 'share/doc/rust'
        source.mkdir(parents=True)
        for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'COPYRIGHT.html']:
            (source / name).write_text('installed notice ' + name)
        with patch('collect_toolchain_notices.subprocess.check_output',
                   side_effect=['rustc test fixture\nhost: x86_64-unknown-linux-gnu', str(sysroot)]):
            data = collect(self.binary, 'x86_64-unknown-linux-gnu', self.notices)
        self.assertEqual(data['binary_sha256'], self.digest(self.binary))
        self.assertNotIn(str(sysroot), (self.notices / 'manifest.json').read_text())
        package_release.package(self.binary, self.output, True, self.notices)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read('licenses/rust/COPYRIGHT.html'),
                             (source / 'COPYRIGHT.html').read_bytes())

    def test_collector_missing_copyright_fails_without_partial_output(self):
        sysroot = self.root / 'sysroot'
        source = sysroot / 'share/doc/rust'
        source.mkdir(parents=True)
        for name in ['LICENSE-MIT', 'LICENSE-APACHE']:
            (source / name).write_text('licence fixture')
        with patch('collect_toolchain_notices.subprocess.check_output',
                   side_effect=['rustc test fixture', str(sysroot)]):
            with self.assertRaisesRegex(ValueError, 'missing'):
                collect(self.binary, 'x86_64-unknown-linux-gnu', self.notices)
        self.assertFalse(self.notices.exists())

    def test_reuse_toolchain_layout_retains_every_supplied_licence(self):
        sysroot = self.root / 'reuse-sysroot'
        source = sysroot / 'share/doc/rust'
        (source / 'licenses').mkdir(parents=True)
        (source / 'COPYRIGHT-library.html').write_text('installed library copyright inventory')
        for name in ['MIT.txt', 'Apache-2.0.txt', 'BSD-3-Clause.txt']:
            (source / 'licenses' / name).write_text('installed REUSE notice ' + name)
        with patch('collect_toolchain_notices.subprocess.check_output',
                   side_effect=['rustc REUSE test fixture', str(sysroot)]):
            collect(self.binary, 'x86_64-unknown-linux-gnu', self.notices)
        package_release.package(self.binary, self.output, True, self.notices)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read('licenses/rust/LICENSE-MIT'),
                             (source / 'licenses/MIT.txt').read_bytes())
            self.assertEqual(archive.read('licenses/rust/licenses/BSD-3-Clause.txt'),
                             (source / 'licenses/BSD-3-Clause.txt').read_bytes())

    def test_repository_supplement_hashes_match_the_packaged_aggregate(self):
        root = Path(__file__).resolve().parents[1]
        review = json.loads((root / 'docs/RELEASE_NOTICE_REVIEW.json').read_text())
        aggregate = (root / 'THIRD_PARTY_NOTICES.txt').read_bytes()
        self.assertEqual(hashlib.sha256(aggregate).hexdigest(), review['notices_sha256'])
        for item in review['notice_supplements']:
            raw = (root / item['path']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), item['sha256'])

    def test_supplement_is_packaged_and_altered_notice_is_rejected(self):
        self.make_notices()
        (self.root / 'licenses').mkdir()
        supplement = self.root / 'licenses/supplement.txt'
        supplement.write_text('retained supplemental grant')
        path = self.root / 'docs/RELEASE_NOTICE_REVIEW.json'
        review = json.loads(path.read_text())
        review['notice_supplements'] = [{'path': 'licenses/supplement.txt',
                                        'sha256': self.digest(supplement)}]
        path.write_text(json.dumps(review))
        package_release.package(self.binary, self.output, True, self.notices)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read('licenses/supplement.txt'), supplement.read_bytes())
        self.output.unlink()
        supplement.write_text('changed grant')
        with self.assertRaisesRegex(ValueError, 'Supplemental notice hash mismatch'):
            package_release.package(self.binary, self.output, True, self.notices)
        self.assertFalse(self.output.exists())

    def test_windows_style_checkout_preserves_supplement_hashes(self):
        repo = Path(__file__).resolve().parents[1]
        source = self.root / 'checkout-source'
        source.mkdir()
        (source / 'licenses').mkdir()
        (source / '.gitattributes').write_bytes((repo / '.gitattributes').read_bytes())
        for path in (repo / 'licenses').glob('*.txt'):
            (source / 'licenses' / path.name).write_bytes(path.read_bytes())
        def git(*args, cwd=source):
            subprocess.run(['git', *args], cwd=cwd, check=True, capture_output=True)
        git('init', '-q')
        git('add', '.')
        git('-c', 'user.name=License test', '-c', 'user.email=license-test@example.invalid',
            'commit', '-qm', 'test checkout bytes')
        clone = self.root / 'windows-checkout'
        git('-c', 'core.autocrlf=true', 'clone', '-q', str(source), str(clone))
        for original in (source / 'licenses').glob('*.txt'):
            self.assertEqual((clone / 'licenses' / original.name).read_bytes(), original.read_bytes())

    def test_traversal_and_missing_notice_coverage_are_rejected(self):
        data = self.make_notices()
        for files in [{'../LICENSE': self.digest(self.root / 'LICENSE')},
                      {'LICENSE-MIT': data['files_sha256']['LICENSE-MIT']}]:
            with self.subTest(files=files):
                data['files_sha256'] = files
                (self.notices / 'manifest.json').write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    package_release.package(self.binary, self.output, True, self.notices)
                self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
