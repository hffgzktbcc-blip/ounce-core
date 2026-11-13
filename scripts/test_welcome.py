#!/usr/bin/env python3
"""Unit tests for contributor welcome script."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

from welcome import (
    WELCOME_MARKER,
    check_is_first_time,
    format_welcome_comment,
    has_welcome_comment,
    is_bot_or_maintainer,
    post_welcome_comment,
)


class TestWelcome(unittest.TestCase):

    def test_is_bot_or_maintainer(self):
        self.assertTrue(is_bot_or_maintainer('hffgzktbcc-blip', 'hffgzktbcc-blip'))
        self.assertTrue(is_bot_or_maintainer('github-actions[bot]', 'hffgzktbcc-blip'))
        self.assertTrue(is_bot_or_maintainer('dependabot[bot]', 'hffgzktbcc-blip'))
        self.assertTrue(is_bot_or_maintainer('shiftbot', 'hffgzktbcc-blip'))
        self.assertFalse(is_bot_or_maintainer('new-contributor', 'hffgzktbcc-blip'))

    def test_check_is_first_time(self):
        # Empty issues returned
        self.assertTrue(check_is_first_time('owner/repo', 'alice', 10, mock_issues=[]))
        # Only the current issue returned
        self.assertTrue(check_is_first_time('owner/repo', 'alice', 10, mock_issues=[{'number': 10}]))
        # Another earlier issue exists
        self.assertFalse(check_is_first_time('owner/repo', 'alice', 10, mock_issues=[{'number': 5}, {'number': 10}]))

    def test_has_welcome_comment(self):
        self.assertFalse(has_welcome_comment('owner/repo', 10, mock_comments=[]))
        self.assertFalse(has_welcome_comment('owner/repo', 10, mock_comments=[{'body': 'Just a discussion comment'}]))
        self.assertTrue(has_welcome_comment('owner/repo', 10, mock_comments=[
            {'body': f'Some intro text\n{WELCOME_MARKER}\nMore info'}
        ]))

    def test_format_welcome_comment_issue(self):
        comment = format_welcome_comment(is_pr=False, repo='owner/repo')
        self.assertIn(WELCOME_MARKER, comment)
        self.assertIn('Thank you for opening your first issue in Ounce-Core.', comment)
        self.assertIn('scripts/gate.sh --check quick', comment)
        self.assertIn('CONTRIBUTING.md', comment)
        self.assertIn('TRY_THIS_WEEK.md', comment)
        self.assertNotIn('AI assistant', comment)
        # Check no emojis
        self.assertTrue(all(ord(char) < 128 for char in comment))

    def test_format_welcome_comment_pr(self):
        comment = format_welcome_comment(is_pr=True, repo='owner/repo')
        self.assertIn(WELCOME_MARKER, comment)
        self.assertIn('Thank you for opening your first pull request in Ounce-Core.', comment)
        self.assertIn('scripts/gate.sh --check quick', comment)
        self.assertIn('CONTRIBUTING.md', comment)
        self.assertIn('TRY_THIS_WEEK.md', comment)
        self.assertIn('AI assistant', comment)
        self.assertIn('AI-assisted contributions', comment)
        # Check no emojis
        self.assertTrue(all(ord(char) < 128 for char in comment))

    def test_post_welcome_comment_dry_run(self):
        res = post_welcome_comment('owner/repo', 123, 'Body', dry_run=True)
        self.assertTrue(res)


if __name__ == '__main__':
    unittest.main()
