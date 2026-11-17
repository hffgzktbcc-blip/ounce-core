import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import refresh_notice_review as r


class RefreshNoticeReview(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        (self.root / 'docs').mkdir()
        for name, _ in r.PINNED:
            (self.root / name).write_text(name + ' v1')
        review = {'status': 'REVIEWED', 'reviewer': 'owner', 'notice_sources': [1, 2]}
        for name, field in r.PINNED:
            review[field] = hashlib.sha256((self.root / name).read_bytes()).hexdigest()
        self.review = self.root / r.REVIEW
        self.review.write_text(json.dumps(review, indent=2) + '\n')

    def test_clean_tree_passes(self):
        self.assertEqual(0, r.main(['--check'], self.root))

    def test_edit_is_detected_and_check_does_not_write(self):
        (self.root / 'Cargo.toml').write_text('changed')
        before = self.review.read_text()
        self.assertEqual(1, r.main(['--check'], self.root))
        self.assertEqual(before, self.review.read_text())

    def test_refresh_updates_only_hashes(self):
        (self.root / 'Cargo.lock').write_text('bumped')
        before = json.loads(self.review.read_text())
        self.assertEqual(0, r.main([], self.root))
        after = json.loads(self.review.read_text())
        self.assertEqual(0, r.main(['--check'], self.root))
        changed = {k for k in after if after[k] != before[k]}
        self.assertEqual({'cargo_lock_sha256'}, changed)
        self.assertEqual(('REVIEWED', 'owner', [1, 2]),
                         (after['status'], after['reviewer'], after['notice_sources']))


if __name__ == '__main__':
    unittest.main()
