#!/usr/bin/env python3
"""Issue claim helper for contributor self-assignment on good first issues."""
import argparse
import json
import os
import re
import subprocess
import sys

CLAIM_MARKER = '<!-- ounce-core-claim-v1 -->'
MAX_OUTPUT_CHARS = 4000

# Exact phrases recognized as intent to work on an issue
CLAIM_PHRASES = [
    "i'll take this",
    "ill take this",
    "i will take this",
    "can i work on this",
    "can i take this",
    "claim",
    "/claim",
    "i would like to work on this",
    "i'd like to work on this",
    "id like to work on this",
    "i want to work on this",
]


def normalize_line(line: str) -> str:
    cleaned = line.strip().lower()
    if cleaned.startswith('/'):
        return '/' + re.sub(r'^[^\w/]+|[^\w]+$', '', cleaned[1:])
    return re.sub(r'^[^\w]+|[^\w]+$', '', cleaned)


def is_claim_comment(body: str) -> bool:
    """Return True if the comment clearly expresses intent to claim the issue."""
    if not body:
        return False

    # Check whole comment normalized
    if normalize_line(body) in CLAIM_PHRASES:
        return True

    # Check line by line
    for line in body.splitlines():
        trimmed = line.strip()
        if not trimmed:
            continue
        if normalize_line(trimmed) in CLAIM_PHRASES:
            return True

    return False


def is_bot_or_maintainer(author: str, owner: str = 'hffgzktbcc-blip') -> bool:
    if not author:
        return True
    author_lower = author.lower()
    if author_lower == owner.lower():
        return True
    if author_lower.endswith('[bot]') or author_lower.endswith('-bot'):
        return True
    if author_lower in {'github-actions', 'dependabot', 'shiftbot', 'copilot'}:
        return True
    return False


def find_other_good_first_issue(repo: str, current_issue_number: int, mock_issues=None):
    """Find another open, unassigned issue with the 'good first issue' label."""
    if mock_issues is not None:
        issues = mock_issues
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues?state=open&labels=good+first+issue&per_page=30']
            output = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            issues = json.loads(output)
        except Exception as exc:
            print(f'Warning: Failed to fetch good first issues: {exc}', file=sys.stderr)
            return None

    for item in issues:
        if item.get('pull_request') is not None:
            continue
        num = item.get('number')
        if num is None or int(num) == int(current_issue_number):
            continue
        assignees = item.get('assignees') or []
        if len(assignees) == 0:
            return (int(num), item.get('title', ''))

    return None


def format_assigned_comment() -> str:
    return (
        f'{CLAIM_MARKER}\n'
        'Assigned to you. Tell us if you get stuck. If it goes quiet for 14 days we will ask whether to keep it reserved.'
    )


def format_already_assigned_comment(assignee: str, repo: str, other_issue=None) -> str:
    lines = [
        CLAIM_MARKER,
        f'This issue is already assigned to @{assignee}.',
        '',
    ]
    if other_issue:
        num, title = other_issue
        lines.extend([
            f"You can check other open issues with the 'good first issue' label, such as #{num} ({title}):",
            f'https://github.com/{repo}/issues/{num}',
            '',
            f"Browse all unassigned starter tasks: https://github.com/{repo}/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22+no%3Aassignee",
        ])
    else:
        lines.extend([
            f"You can browse other available starter tasks with the 'good first issue' label here:",
            f'https://github.com/{repo}/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22+no%3Aassignee',
        ])

    return '\n'.join(lines).strip()


def format_not_good_first_issue_comment(repo: str) -> str:
    return (
        f'{CLAIM_MARKER}\n'
        "Self-assignment is available for issues labeled 'good first issue'. For other issues, a maintainer will coordinate assignment.\n\n"
        f"You can browse open beginner tasks here:\n"
        f'https://github.com/{repo}/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22+no%3Aassignee'
    )


def assign_issue(repo: str, issue_number: int, assignee: str, dry_run: bool = False) -> bool:
    if dry_run:
        print(f'[dry-run] Would assign {assignee} to {repo}#{issue_number}')
        return True
    try:
        cmd = [
            'gh', 'api', '-X', 'POST',
            f'repos/{repo}/issues/{issue_number}/assignees',
            '-f', f'assignees[]={assignee}',
        ]
        subprocess.run(cmd, check=True)
        print(f'Successfully assigned @{assignee} to #{issue_number}.')
        return True
    except subprocess.CalledProcessError as exc:
        print(f'Failed to assign @{assignee}: {exc}', file=sys.stderr)
        return False


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
        print(f'Successfully posted comment on #{issue_number}.')
        return True
    except subprocess.CalledProcessError as exc:
        print(f'Failed to post comment: {exc}', file=sys.stderr)
        return False


def main() -> int:
    parser = argparse.ArgumentParser(description='Process claim comments on issues.')
    parser.add_argument('--repo', default=os.environ.get('GH_REPO', 'hffgzktbcc-blip/ounce-core'))
    parser.add_argument('--issue-number', type=int, default=int(os.environ.get('ISSUE_NUMBER', '0') or '0'))
    parser.add_argument('--comment-body', default=os.environ.get('COMMENT_BODY', ''))
    parser.add_argument('--comment-author', default=os.environ.get('COMMENT_AUTHOR', ''))
    parser.add_argument('--owner', default=os.environ.get('REPO_OWNER', 'hffgzktbcc-blip'))
    parser.add_argument('--issue-state', default=os.environ.get('ISSUE_STATE', 'open'))
    parser.add_argument('--issue-assignees', default=os.environ.get('ISSUE_ASSIGNEES', '[]'))
    parser.add_argument('--issue-labels', default=os.environ.get('ISSUE_LABELS', '[]'))
    parser.add_argument('--dry-run', action='store_true', default=False)
    args = parser.parse_args()

    if not args.issue_number or not args.comment_author or not args.comment_body:
        print('Missing required issue or comment data; nothing to do.')
        return 0

    if is_bot_or_maintainer(args.comment_author, args.owner):
        print(f'Author {args.comment_author} is a bot or repository owner; skipping claim.')
        return 0

    if not is_claim_comment(args.comment_body):
        print('Comment does not match claim phrases; skipping.')
        return 0

    if args.issue_state.lower() != 'open':
        print(f'Issue #{args.issue_number} is not open ({args.issue_state}); skipping claim.')
        return 0

    # Parse assignees
    try:
        assignees = json.loads(args.issue_assignees) if isinstance(args.issue_assignees, str) else args.issue_assignees
    except Exception:
        assignees = []

    # Parse labels
    try:
        raw_labels = json.loads(args.issue_labels) if isinstance(args.issue_labels, str) else args.issue_labels
    except Exception:
        raw_labels = []

    label_names = set()
    for l in raw_labels:
        if isinstance(l, dict):
            label_names.add(l.get('name', '').lower())
        elif isinstance(l, str):
            label_names.add(l.lower())

    # Case 1: Already assigned
    if assignees:
        assignee_names = [a.get('login') if isinstance(a, dict) else str(a) for a in assignees]
        if args.comment_author in assignee_names:
            print(f'Issue #{args.issue_number} is already assigned to {args.comment_author}.')
            return 0

        other_issue = find_other_good_first_issue(args.repo, args.issue_number)
        reply = format_already_assigned_comment(assignee_names[0], args.repo, other_issue)
        post_comment(args.repo, args.issue_number, reply, dry_run=args.dry_run)
        return 0

    # Case 2: Open and unassigned, check label
    if 'good first issue' not in label_names:
        reply = format_not_good_first_issue_comment(args.repo)
        post_comment(args.repo, args.issue_number, reply, dry_run=args.dry_run)
        return 0

    # Case 3: Open, unassigned, good first issue -> assign!
    assign_success = assign_issue(args.repo, args.issue_number, args.comment_author, dry_run=args.dry_run)
    reply = format_assigned_comment()
    post_comment(args.repo, args.issue_number, reply, dry_run=args.dry_run)

    return 0 if assign_success else 1


if __name__ == '__main__':
    sys.exit(main())
