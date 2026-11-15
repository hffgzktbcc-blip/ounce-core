#!/usr/bin/env python3
"""Contributor welcome helper for first-time issue and PR authors."""
import argparse
import json
import os
import subprocess
import sys

WELCOME_MARKER = '<!-- ounce-core-welcome-v1 -->'
MAX_OUTPUT_CHARS = 4000


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


def check_is_first_time(repo: str, author: str, current_number: int, mock_issues=None) -> bool:
    """Return True if this author has no earlier issues or PRs in the repository."""
    if mock_issues is not None:
        issues = mock_issues
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues?creator={author}&state=all&per_page=100']
            output = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            issues = json.loads(output)
        except Exception as exc:
            print(f'Warning: Failed to query author issues via API: {exc}', file=sys.stderr)
            return False

    for item in issues:
        num = item.get('number')
        if num is not None and int(num) != int(current_number):
            return False
    return True


def has_welcome_comment(repo: str, issue_number: int, mock_comments=None) -> bool:
    """Return True if a welcome comment has already been posted on this issue/PR."""
    if mock_comments is not None:
        comments = mock_comments
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues/{issue_number}/comments?per_page=100']
            output = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            comments = json.loads(output)
        except Exception as exc:
            print(f'Warning: Failed to fetch issue comments: {exc}', file=sys.stderr)
            return False

    for c in comments:
        if WELCOME_MARKER in c.get('body', ''):
            return True
    return False


def format_welcome_comment(is_pr: bool, repo: str) -> str:
    """Format a short, polite welcome comment without emoji."""
    item_type = 'pull request' if is_pr else 'issue'
    lines = [
        WELCOME_MARKER,
        f'Thank you for opening your first {item_type} in Ounce-Core.',
        '',
        f'A maintainer will review your {item_type}. In the meantime, you can run local checks with:',
        '```sh',
        'scripts/gate.sh --check quick',
        '```',
    ]

    if is_pr:
        lines.extend([
            '',
            'If you prepared this change with an AI assistant, please ensure you have disclosed the tool used and verified the result as outlined in the [AI-assisted contributions](https://github.com/'
            f'{repo}/blob/main/CONTRIBUTING.md#ai-assisted-contributions) section of CONTRIBUTING.md.',
        ])

    lines.extend([
        '',
        'Helpful references for contributors:',
        f'- [Contribution guide](https://github.com/{repo}/blob/main/CONTRIBUTING.md)',
        f'- [Try Ounce-Core this week](https://github.com/{repo}/blob/main/docs/community/TRY_THIS_WEEK.md)',
    ])

    body = '\n'.join(lines).strip()
    if len(body) > MAX_OUTPUT_CHARS:
        body = body[:MAX_OUTPUT_CHARS]
    return body


def post_welcome_comment(repo: str, issue_number: int, body: str, dry_run: bool = False) -> bool:
    if dry_run:
        print(f'[dry-run] Would post welcome comment on {repo}#{issue_number}:\n{body}')
        return True

    try:
        cmd = [
            'gh', 'api', '-X', 'POST',
            f'repos/{repo}/issues/{issue_number}/comments',
            '-f', f'body={body}',
        ]
        subprocess.run(cmd, check=True)
        print(f'Successfully posted welcome comment on #{issue_number}.')
        return True
    except subprocess.CalledProcessError as exc:
        print(f'Failed to post welcome comment: {exc}', file=sys.stderr)
        return False


def main() -> int:
    parser = argparse.ArgumentParser(description='Post a one-time welcome comment for first-time contributors.')
    parser.add_argument('--repo', default=os.environ.get('GH_REPO', 'hffgzktbcc-blip/ounce-core'))
    parser.add_argument('--issue-number', type=int, default=int(os.environ.get('ISSUE_NUMBER', '0') or '0'))
    parser.add_argument('--author', default=os.environ.get('AUTHOR_LOGIN', ''))
    parser.add_argument('--is-pr', action='store_true', default=(os.environ.get('IS_PR', '').lower() in {'true', '1'}))
    parser.add_argument('--owner', default=os.environ.get('REPO_OWNER', 'hffgzktbcc-blip'))
    parser.add_argument('--dry-run', action='store_true', default=False)
    args = parser.parse_args()

    if not args.issue_number or not args.author:
        print('Missing issue number or author; nothing to do.')
        return 0

    if is_bot_or_maintainer(args.author, args.owner):
        print(f'Author {args.author} is a bot or repository owner; skipping welcome.')
        return 0

    if not check_is_first_time(args.repo, args.author, args.issue_number):
        print(f'Author {args.author} has previous contributions; skipping welcome.')
        return 0

    if has_welcome_comment(args.repo, args.issue_number):
        print(f'Welcome comment already present on #{args.issue_number}; skipping.')
        return 0

    comment_body = format_welcome_comment(args.is_pr, args.repo)
    success = post_welcome_comment(args.repo, args.issue_number, comment_body, dry_run=args.dry_run)
    return 0 if success else 1


if __name__ == '__main__':
    sys.exit(main())
