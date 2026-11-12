#!/usr/bin/env python3
"""Unit tests for contributor issue nudge script."""
from datetime import datetime, timedelta, timezone
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

from nudge import (
    NUDGE_MARKER,
    UNASSIGN_MARKER,
    evaluate_assignee_activity,
    find_assigned_at,
    format_nudge_comment,
    format_unassign_comment,
    parse_iso,
    post_comment,
    unassign_user,
)


class TestNudge(unittest.TestCase):

    def setUp(self):
        self.now = datetime(2026, 11, 12, 12, 0, 0, tzinfo=timezone.utc)

    def test_parse_iso(self):
        dt = parse_iso('2026-11-01T10:00:00Z')
        self.assertEqual(dt.year, 2026)
        self.assertEqual(dt.month, 11)
        self.assertEqual(dt.day, 1)
        self.assertEqual(dt.tzinfo, timezone.utc)

    def test_evaluate_recently_assigned_active(self):
        assigned_at = self.now - timedelta(days=5)
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[], timeline_events=[], now=self.now)
        self.assertEqual(status, 'active')

    def test_evaluate_inactive_needs_nudge(self):
        assigned_at = self.now - timedelta(days=15)
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[], timeline_events=[], now=self.now)
        self.assertEqual(status, 'nudge')

    def test_evaluate_inactive_assigned_but_active_comment(self):
        assigned_at = self.now - timedelta(days=20)
        recent_comment = {
            'user': {'login': 'alice'},
            'body': 'Still investigating this crash.',
            'created_at': (self.now - timedelta(days=3)).isoformat(),
        }
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[recent_comment], timeline_events=[], now=self.now)
        self.assertEqual(status, 'active')

    def test_evaluate_inactive_assigned_but_active_timeline_pr(self):
        assigned_at = self.now - timedelta(days=20)
        pr_event = {
            'event': 'cross-referenced',
            'actor': {'login': 'alice'},
            'created_at': (self.now - timedelta(days=2)).isoformat(),
        }
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[], timeline_events=[pr_event], now=self.now)
        self.assertEqual(status, 'active')

    def test_evaluate_nudged_waiting(self):
        assigned_at = self.now - timedelta(days=20)
        nudge_comment = {
            'user': {'login': 'bot'},
            'body': f'{NUDGE_MARKER}\nstill working on this?',
            'created_at': (self.now - timedelta(days=5)).isoformat(),
        }
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[nudge_comment], timeline_events=[], now=self.now)
        self.assertEqual(status, 'waiting')

    def test_evaluate_nudged_further_silence_unassign(self):
        assigned_at = self.now - timedelta(days=35)
        nudge_comment = {
            'user': {'login': 'bot'},
            'body': f'{NUDGE_MARKER}\nstill working on this?',
            'created_at': (self.now - timedelta(days=15)).isoformat(),
        }
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[nudge_comment], timeline_events=[], now=self.now)
        self.assertEqual(status, 'unassign')

    def test_evaluate_nudged_then_assignee_replied(self):
        assigned_at = self.now - timedelta(days=35)
        nudge_comment = {
            'user': {'login': 'bot'},
            'body': f'{NUDGE_MARKER}\nstill working on this?',
            'created_at': (self.now - timedelta(days=15)).isoformat(),
        }
        reply_comment = {
            'user': {'login': 'alice'},
            'body': 'Yes, opening a PR tomorrow!',
            'created_at': (self.now - timedelta(days=2)).isoformat(),
        }
        status, _ = evaluate_assignee_activity('alice', assigned_at, comments=[nudge_comment, reply_comment], timeline_events=[], now=self.now)
        self.assertEqual(status, 'active')

    def test_find_assigned_at(self):
        timeline = [
            {'event': 'commented', 'created_at': '2026-10-01T10:00:00Z'},
            {'event': 'assigned', 'assignee': {'login': 'alice'}, 'created_at': '2026-10-15T12:00:00Z'},
        ]
        dt = find_assigned_at('alice', '2026-10-01T09:00:00Z', timeline)
        self.assertEqual(dt, parse_iso('2026-10-15T12:00:00Z'))

        # Fallback to issue created_at if no assigned event found
        dt_fallback = find_assigned_at('bob', '2026-10-01T09:00:00Z', timeline)
        self.assertEqual(dt_fallback, parse_iso('2026-10-01T09:00:00Z'))

    def test_format_comments(self):
        nudge = format_nudge_comment('alice')
        self.assertIn(NUDGE_MARKER, nudge)
        self.assertIn('still working on this? Reply to keep it reserved; otherwise we may unassign so someone else can try.', nudge)
        self.assertTrue(all(ord(char) < 128 for char in nudge))

        unassign = format_unassign_comment('alice')
        self.assertIn(UNASSIGN_MARKER, unassign)
        self.assertIn('Thank you for your interest in Ounce-Core', unassign)
        self.assertTrue(all(ord(char) < 128 for char in unassign))

    def test_dry_run_actions(self):
        self.assertTrue(post_comment('owner/repo', 10, 'text', dry_run=True))
        self.assertTrue(unassign_user('owner/repo', 10, 'alice', dry_run=True))


if __name__ == '__main__':
    unittest.main()
