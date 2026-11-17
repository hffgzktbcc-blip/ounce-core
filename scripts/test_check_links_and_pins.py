import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_links
import check_pins

ROOT = Path(__file__).resolve().parent.parent


def make_repo(files):
    tmp = tempfile.TemporaryDirectory()
    root = Path(tmp.name)
    subprocess.run(['git', 'init', '-q'], cwd=root, check=True)
    for name, text in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
    subprocess.run(['git', 'add', '-A'], cwd=root, check=True)
    return tmp, root


class LinkCheckTest(unittest.TestCase):
    def test_finds_missing_and_ignores_the_rest(self):
        tmp, root = make_repo({
            'README.md': '[ok](docs/a.md) [bad](docs/missing.md) [web](https://x.invalid/y) [anchor](#top)\n'
                         '`[code](nope.md)`\n```\n[fenced](nope2.md)\n```\n[frag](docs/a.md#section)\n',
            'docs/a.md': '[up](../README.md) ![img](gone.png)\n',
        })
        with tmp:
            found = check_links.broken_links(root)
        self.assertEqual(sorted(found), ['README.md:1: broken link -> docs/missing.md',
                                         'docs/a.md:1: broken link -> gone.png'])

    def test_repository_has_no_broken_links(self):
        self.assertEqual(check_links.broken_links(ROOT), [])


class PinCheckTest(unittest.TestCase):
    FILES = {
        'Cargo.toml': '[package]\nname = "x"\nversion = "1.2.3"\n[dependencies]\ny = { version = "9" }\n',
        'CITATION.cff': 'cff-version: 1.2.0\nversion: 1.2.3\n',
        'CHANGELOG.md': '# Changelog\n\n## Unreleased\n\n## 1.2.3 — released 2026-01-01\n',
        'docs/releases/v1.2.3.md': 'notes\n',
    }

    def check(self, **changes):
        files = {**self.FILES, **changes}
        files = {name: text for name, text in files.items() if text is not None}
        tmp, root = make_repo(files)
        with tmp:
            return check_pins.problems(root)

    def test_consistent(self):
        self.assertEqual(self.check(), [])

    def test_detects_mismatch(self):
        found = self.check(**{'CITATION.cff': 'version: 1.2.2\n'})
        self.assertTrue(found and found[0].startswith('version mismatch'), found)

    def test_detects_missing_release_notes(self):
        self.assertEqual(self.check(**{'docs/releases/v1.2.3.md': None}), ['docs/releases/v1.2.3.md is missing'])

    def test_repository_pins_agree(self):
        self.assertEqual(check_pins.problems(ROOT), [])


if __name__ == '__main__':
    unittest.main()
