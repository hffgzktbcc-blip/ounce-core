#!/usr/bin/env python3
"""Unit tests for outside activity helper."""
from datetime import datetime, timedelta, timezone
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))

from outside_activity import (
    fetch_outside_activity,
    format_activity_report,
    format_iso,
    is_excluded_account,
    make_one_line_excerpt,
    parse_iso,
    read_state_timestamp,
    write_state_timestamp,
)


class TestOutsideActivity(unittest.TestCase):

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.state_file = os.path.join(self.temp_dir, 'state.json')

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_is_excluded_account(self):
        self.assertTrue(is_excluded_account('hffgzktbcc-blip', 'hffgzktbcc-blip'))
        self.assertTrue(is_excluded_account('github-actions[bot]', 'hffgzktbcc-blip'))
        self.assertTrue(is_excluded_account('shiftbot', 'hffgzktbcc-blip'))
        self.assertTrue(is_excluded_account('dependabot[bot]', 'hffgzktbcc-blip'))
        self.assertFalse(is_excluded_account('alice', 'hffgzktbcc-blip'))

    def test_make_one_line_excerpt(self):
        text = '<!-- marker -->\n\nFirst real line with detail.\nSecond line.'
        excerpt = make_one_line_excerpt(text)
        self.assertEqual(excerpt, 'First real line with detail.')

        long_text = 'A' * 200
        long_excerpt = make_one_line_excerpt(long_text, max_length=50)
        self.assertEqual(len(long_excerpt), 50)
        self.assertTrue(long_excerpt.endswith('...'))

        self.assertEqual(make_one_line_excerpt(''), '(no content)')

    def test_state_file_read_write(self):
        # Default when missing
        default_ts = read_state_timestamp(self.state_file, default_days=7)
        self.assertTrue(default_ts.endswith('Z'))

        # Write and read back
        custom_ts = '2026-11-10T12:00:00Z'
        self.assertTrue(write_state_timestamp(self.state_file, custom_ts))
        read_back = read_state_timestamp(self.state_file)
        self.assertEqual(read_back, custom_ts)

    def test_fetch_outside_activity(self):
        since_str = '2026-11-10T00:00:00Z'
        mock_issues = [
            # Outside issue (should be included)
            {
                'number': 101,
                'title': 'Crash on M1 in svc',
                'created_at': '2026-11-11T10:00:00Z',
                'user': {'login': 'alice'},
                'html_url': 'https://github.com/owner/repo/issues/101',
                'body': 'Observed panic during run-nro hello.nro',
            },
            # Outside PR (should be included)
            {
                'number': 102,
                'title': 'Fix svc bounds',
                'pull_request': {},
                'created_at': '2026-11-11T12:00:00Z',
                'user': {'login': 'bob'},
                'html_url': 'https://github.com/owner/repo/pull/102',
                'body': 'Adds check to prevent overflow.',
            },
            # Owner issue (should be excluded)
            {
                'number': 103,
                'title': 'Owner maintenance',
                'created_at': '2026-11-11T14:00:00Z',
                'user': {'login': 'hffgzktbcc-blip'},
                'html_url': 'https://github.com/owner/repo/issues/103',
                'body': 'Maintenance tasks.',
            },
            # Bot issue (should be excluded)
            {
                'number': 104,
                'title': 'Bot report',
                'created_at': '2026-11-11T15:00:00Z',
                'user': {'login': 'shiftbot'},
                'html_url': 'https://github.com/owner/repo/issues/104',
                'body': 'Bot summary.',
            },
            # Old issue before since (should be excluded)
            {
                'number': 99,
                'title': 'Old bug',
                'created_at': '2026-11-01T10:00:00Z',
                'user': {'login': 'alice'},
                'html_url': 'https://github.com/owner/repo/issues/99',
                'body': 'Old report.',
            },
        ]

        mock_comments = [
            # Outside comment (should be included)
            {
                'created_at': '2026-11-11T11:00:00Z',
                'user': {'login': 'charlie'},
                'issue_url': 'https://api.github.com/repos/owner/repo/issues/101',
                'html_url': 'https://github.com/owner/repo/issues/101#issuecomment-1',
                'body': 'I can reproduce this on ARM64 Linux.',
            },
            # Owner comment (should be excluded)
            {
                'created_at': '2026-11-11T11:30:00Z',
                'user': {'login': 'hffgzktbcc-blip'},
                'issue_url': 'https://api.github.com/repos/owner/repo/issues/101',
                'html_url': 'https://github.com/owner/repo/issues/101#issuecomment-2',
                'body': 'Thanks for reporting.',
            },
        ]

        items = fetch_outside_activity(
            repo='owner/repo',
            since_str=since_str,
            owner='hffgzktbcc-blip',
            mock_issues=mock_issues,
            mock_comments=mock_comments,
        )

        self.assertEqual(len(items), 3)
        kinds = [it['kind'] for it in items]
        self.assertIn('Issue', kinds)
        self.assertIn('PR', kinds)
        self.assertIn('Comment', kinds)

        report = format_activity_report(items, 'owner/repo', since_str)
        self.assertIn('Found 3 item(s)', report)
        self.assertIn('https://github.com/owner/repo/issues/101', report)
        self.assertIn('https://github.com/owner/repo/pull/102', report)
        self.assertIn('Observed panic during run-nro', report)

    def test_format_activity_report_empty(self):
        report = format_activity_report([], 'owner/repo', '2026-11-10T00:00:00Z')
        self.assertIn('No new outside activity found.', report)


if __name__ == '__main__':
    unittest.main()
