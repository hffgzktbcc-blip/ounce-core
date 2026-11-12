#!/usr/bin/env python3
"""Unit tests for issue claim helper."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

from claim import (
    CLAIM_MARKER,
    assign_issue,
    find_other_good_first_issue,
    format_already_assigned_comment,
    format_assigned_comment,
    format_not_good_first_issue_comment,
    is_bot_or_maintainer,
    is_claim_comment,
    post_comment,
)


class TestClaim(unittest.TestCase):

    def test_is_claim_comment(self):
        # Matching phrases
        self.assertTrue(is_claim_comment("I'll take this"))
        self.assertTrue(is_claim_comment("i'll take this"))
        self.assertTrue(is_claim_comment("can I work on this?"))
        self.assertTrue(is_claim_comment("Can I work on this"))
        self.assertTrue(is_claim_comment("claim"))
        self.assertTrue(is_claim_comment("Claim!"))
        self.assertTrue(is_claim_comment("/claim"))
        self.assertTrue(is_claim_comment("I want to work on this"))
        self.assertTrue(is_claim_comment("Can I work on this?\nThanks!"))

        # Non-matching comments
        self.assertFalse(is_claim_comment("This is a claim."))
        self.assertFalse(is_claim_comment("I don't think anyone should claim this yet."))
        self.assertFalse(is_claim_comment("Can someone take a look?"))
        self.assertFalse(is_claim_comment(""))

    def test_is_bot_or_maintainer(self):
        self.assertTrue(is_bot_or_maintainer('hffgzktbcc-blip', 'hffgzktbcc-blip'))
        self.assertTrue(is_bot_or_maintainer('github-actions[bot]', 'hffgzktbcc-blip'))
        self.assertFalse(is_bot_or_maintainer('alice', 'hffgzktbcc-blip'))

    def test_find_other_good_first_issue(self):
        mock_issues = [
            {'number': 10, 'title': 'Current issue', 'assignees': []},
            {'number': 11, 'title': 'Assigned issue', 'assignees': [{'login': 'bob'}]},
            {'number': 12, 'title': 'A pull request', 'pull_request': {}, 'assignees': []},
            {'number': 13, 'title': 'Available issue', 'assignees': []},
        ]
        res = find_other_good_first_issue('owner/repo', current_issue_number=10, mock_issues=mock_issues)
        self.assertEqual(res, (13, 'Available issue'))

        # No other issue available
        res_none = find_other_good_first_issue('owner/repo', current_issue_number=13, mock_issues=[
            {'number': 13, 'title': 'Available issue', 'assignees': []}
        ])
        self.assertIsNone(res_none)

    def test_format_assigned_comment(self):
        comment = format_assigned_comment()
        self.assertIn(CLAIM_MARKER, comment)
        self.assertIn('Assigned to you. Tell us if you get stuck. If it goes quiet for 14 days we will ask whether to keep it reserved.', comment)
        self.assertTrue(all(ord(char) < 128 for char in comment))

    def test_format_already_assigned_comment(self):
        comment = format_already_assigned_comment('bob', 'owner/repo', other_issue=(15, 'Fix doc typo'))
        self.assertIn(CLAIM_MARKER, comment)
        self.assertIn('already assigned to @bob', comment)
        self.assertIn('#15 (Fix doc typo)', comment)
        self.assertTrue(all(ord(char) < 128 for char in comment))

    def test_format_not_good_first_issue_comment(self):
        comment = format_not_good_first_issue_comment('owner/repo')
        self.assertIn(CLAIM_MARKER, comment)
        self.assertIn("Self-assignment is available for issues labeled 'good first issue'", comment)
        self.assertTrue(all(ord(char) < 128 for char in comment))

    def test_dry_run_actions(self):
        self.assertTrue(assign_issue('owner/repo', 10, 'alice', dry_run=True))
        self.assertTrue(post_comment('owner/repo', 10, 'comment', dry_run=True))


if __name__ == '__main__':
    unittest.main()
