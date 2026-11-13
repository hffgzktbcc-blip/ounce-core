#!/usr/bin/env python3
"""Weekly inactive issue check and contributor nudge helper."""
import argparse
from datetime import datetime, timezone
import json
import os
import subprocess
import sys

NUDGE_MARKER = '<!-- ounce-core-nudge-v1 -->'
UNASSIGN_MARKER = '<!-- ounce-core-unassign-v1 -->'
INACTIVITY_DAYS = 14
INACTIVITY_SECONDS = INACTIVITY_DAYS * 86400


def parse_iso(ts_str: str) -> datetime:
    """Parse an ISO 8601 timestamp into a timezone-aware UTC datetime."""
    if not ts_str:
        return datetime.now(timezone.utc)
    # Replace trailing 'Z' with '+00:00' for fromisoformat compatibility
    cleaned = ts_str.replace('Z', '+00:00')
    dt = datetime.fromisoformat(cleaned)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def format_nudge_comment(assignee: str) -> str:
    return (
        f'{NUDGE_MARKER}\n'
        f'@{assignee} still working on this? Reply to keep it reserved; otherwise we may unassign so someone else can try.'
    )


def format_unassign_comment(assignee: str) -> str:
    return (
        f'{UNASSIGN_MARKER}\n'
        f'Unassigning @{assignee} after 14 days of silence following the check-in to make this issue available for other contributors. '
        'Thank you for your interest in Ounce-Core, and feel free to pick up another task when you are ready.'
    )


def evaluate_assignee_activity(
    assignee: str,
    assigned_at: datetime,
    comments: list,
    timeline_events: list,
    now: datetime = None,
) -> tuple:
    """Evaluate whether an assignee is active, needs a nudge, needs unassign, or is waiting.

    Returns:
        (status, timestamp) where status is one of:
        - 'active': assignee had recent activity within 14 days
        - 'nudge': inactive for >= 14 days with no pending nudge
        - 'waiting': nudged < 14 days ago, waiting for reply
        - 'unassign': nudged >= 14 days ago with continuous silence
    """
    if now is None:
        now = datetime.now(timezone.utc)

    latest_activity_at = assigned_at

    # Check comments by assignee
    for c in comments:
        user_login = c.get('user', {}).get('login') if isinstance(c.get('user'), dict) else None
        if user_login == assignee:
            c_time = parse_iso(c.get('created_at', ''))
            if c_time > latest_activity_at:
                latest_activity_at = c_time

    # Check timeline events by assignee (commit, cross-reference, comment)
    for ev in timeline_events:
        actor_login = ev.get('actor', {}).get('login') if isinstance(ev.get('actor'), dict) else None
        source_user = ev.get('source', {}).get('issue', {}).get('user', {}).get('login') if isinstance(ev.get('source'), dict) else None
        commit_author = ev.get('author', {}).get('login') if isinstance(ev.get('author'), dict) else None

        if assignee in {actor_login, source_user, commit_author}:
            ev_type = ev.get('event')
            if ev_type in {'commented', 'committed', 'cross-referenced'}:
                ev_time = parse_iso(ev.get('created_at', ''))
                if ev_time > latest_activity_at:
                    latest_activity_at = ev_time

    # Find the most recent nudge comment posted at or after assigned_at
    latest_nudge_at = None
    for c in comments:
        body = c.get('body', '')
        if NUDGE_MARKER in body:
            nudge_time = parse_iso(c.get('created_at', ''))
            if nudge_time >= assigned_at:
                if latest_nudge_at is None or nudge_time > latest_nudge_at:
                    latest_nudge_at = nudge_time

    # If the assignee was active after the latest nudge, the nudge was resolved
    if latest_nudge_at is not None and latest_activity_at > latest_nudge_at:
        latest_nudge_at = None

    # Decision tree
    if latest_nudge_at is not None:
        # A nudge is pending
        nudge_age = (now - latest_nudge_at).total_seconds()
        if nudge_age >= INACTIVITY_SECONDS:
            return ('unassign', latest_nudge_at)
        return ('waiting', latest_nudge_at)

    inactivity_seconds = (now - latest_activity_at).total_seconds()
    if inactivity_seconds >= INACTIVITY_SECONDS:
        return ('nudge', latest_activity_at)

    return ('active', latest_activity_at)


def find_assigned_at(assignee: str, issue_created_at: str, timeline_events: list) -> datetime:
    """Find the latest time this assignee was assigned to the issue."""
    latest_assigned = None
    for ev in timeline_events:
        if ev.get('event') == 'assigned':
            assigned_user = ev.get('assignee', {}).get('login') if isinstance(ev.get('assignee'), dict) else None
            if assigned_user == assignee:
                ev_time = parse_iso(ev.get('created_at', ''))
                if latest_assigned is None or ev_time > latest_assigned:
                    latest_assigned = ev_time

    if latest_assigned is not None:
        return latest_assigned
    return parse_iso(issue_created_at)


def post_comment(repo: str, issue_number: int, body: str, dry_run: bool = False) -> bool:
    if dry_run:
        print(f'[dry-run] Would post comment on {repo}#{issue_number}:\n{body}')
        return True
    try:
        cmd = [
            'gh', 'api', '-X', 'POST',
            f'repos/{repo}/issues/{issue_number}/comments',
            '-f', f'body={body}',
        ]
        subprocess.run(cmd, check=True)
        print(f'Posted comment on #{issue_number}.')
        return True
    except subprocess.CalledProcessError as exc:
        print(f'Failed to post comment on #{issue_number}: {exc}', file=sys.stderr)
        return False


def unassign_user(repo: str, issue_number: int, assignee: str, dry_run: bool = False) -> bool:
    if dry_run:
        print(f'[dry-run] Would unassign @{assignee} from {repo}#{issue_number}')
        return True
    try:
        payload = json.dumps({'assignees': [assignee]})
        cmd = [
            'gh', 'api', '-X', 'DELETE',
            f'repos/{repo}/issues/{issue_number}/assignees',
            '--input', '-',
        ]
        subprocess.run(cmd, input=payload, text=True, check=True)
        print(f'Unassigned @{assignee} from #{issue_number}.')
        return True
    except subprocess.CalledProcessError as exc:
        print(f'Failed to unassign @{assignee} from #{issue_number}: {exc}', file=sys.stderr)
        return False


def process_repo_issues(repo: str, owner: str = 'hffgzktbcc-blip', dry_run: bool = False, mock_issues=None) -> int:
    """Scan open assigned issues and nudge or unassign as appropriate."""
    if mock_issues is not None:
        issues = mock_issues
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues?state=open&filter=all&per_page=100']
            output = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            issues = json.loads(output)
        except Exception as exc:
            print(f'Failed to fetch open issues: {exc}', file=sys.stderr)
            return 1

    actions_count = 0
    now = datetime.now(timezone.utc)

    for issue in issues:
        if issue.get('pull_request') is not None:
            continue

        issue_number = issue.get('number')
        assignees = issue.get('assignees') or []
        if not assignees or not issue_number:
            continue

        # Skip maintainer-assigned issues (assignee = repo owner)
        has_owner_assignee = any(
            (a.get('login', '').lower() == owner.lower())
            for a in assignees if isinstance(a, dict)
        )
        if has_owner_assignee:
            continue

        # Fetch comments and timeline for this issue
        comments = []
        timeline = []
        try:
            c_out = subprocess.check_output(['gh', 'api', f'repos/{repo}/issues/{issue_number}/comments?per_page=100'], text=True, stderr=subprocess.DEVNULL)
            comments = json.loads(c_out)
            t_out = subprocess.check_output(['gh', 'api', f'repos/{repo}/issues/{issue_number}/timeline?per_page=100'], text=True, stderr=subprocess.DEVNULL)
            timeline = json.loads(t_out)
        except Exception as exc:
            print(f'Warning: Could not fetch details for issue #{issue_number}: {exc}', file=sys.stderr)
            continue

        for a in assignees:
            assignee_login = a.get('login') if isinstance(a, dict) else str(a)
            if not assignee_login or assignee_login.lower() == owner.lower() or assignee_login.endswith('[bot]'):
                continue

            assigned_at = find_assigned_at(assignee_login, issue.get('created_at', ''), timeline)
            status, ts = evaluate_assignee_activity(assignee_login, assigned_at, comments, timeline, now=now)

            if status == 'nudge':
                nudge_body = format_nudge_comment(assignee_login)
                post_comment(repo, issue_number, nudge_body, dry_run=dry_run)
                actions_count += 1
            elif status == 'unassign':
                unassign_user(repo, issue_number, assignee_login, dry_run=dry_run)
                unassign_body = format_unassign_comment(assignee_login)
                post_comment(repo, issue_number, unassign_body, dry_run=dry_run)
                actions_count += 1

    print(f'Completed inactivity scan for {repo}. Actions taken: {actions_count}.')
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description='Scan open assigned issues and send nudges or unassign.')
    parser.add_argument('--repo', default=os.environ.get('GH_REPO', 'hffgzktbcc-blip/ounce-core'))
    parser.add_argument('--owner', default=os.environ.get('REPO_OWNER', 'hffgzktbcc-blip'))
    parser.add_argument('--dry-run', action='store_true', default=False)
    args = parser.parse_args()

    return process_repo_issues(args.repo, owner=args.owner, dry_run=args.dry_run)


if __name__ == '__main__':
    sys.exit(main())
