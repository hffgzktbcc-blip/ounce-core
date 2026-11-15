#!/usr/bin/env python3
"""Read-only helper using the gh CLI to list new outside contributor activity."""
import argparse
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys

DEFAULT_STATE_FILE = os.path.expanduser('~/.cache/ounce-outside-activity.json')
MAX_EXCERPT_CHARS = 100


def parse_iso(ts_str: str) -> datetime:
    if not ts_str:
        return datetime.now(timezone.utc)
    cleaned = ts_str.replace('Z', '+00:00')
    dt = datetime.fromisoformat(cleaned)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def format_iso(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def is_excluded_account(author: str, owner: str = 'hffgzktbcc-blip') -> bool:
    if not author:
        return True
    a_lower = author.lower()
    if a_lower == owner.lower():
        return True
    if a_lower.endswith('[bot]') or a_lower.endswith('-bot'):
        return True
    if author.lower() in {'github-actions', 'dependabot', 'shiftbot', 'copilot'}:
        return True
    return False


def make_one_line_excerpt(text: str, max_length: int = MAX_EXCERPT_CHARS) -> str:
    if not text:
        return '(no content)'
    lines = [
        l.strip() for l in text.splitlines()
        if l.strip() and not l.strip().startswith('<!--')
    ]
    if not lines:
        return '(no content)'
    clean = re.sub(r'\s+', ' ', lines[0]).strip()
    if len(clean) > max_length:
        clean = clean[:max_length - 3] + '...'
    return clean


def read_state_timestamp(state_path: str, default_days: int = 7) -> str:
    p = Path(state_path)
    if p.is_file():
        try:
            data = json.loads(p.read_text(encoding='utf-8'))
            last_checked = data.get('last_checked')
            if last_checked:
                return last_checked
        except Exception as exc:
            print(f'Warning: Could not read state file {state_path}: {exc}', file=sys.stderr)

    default_dt = datetime.now(timezone.utc) - timedelta(days=default_days)
    return format_iso(default_dt)


def write_state_timestamp(state_path: str, timestamp_str: str) -> bool:
    try:
        p = Path(state_path)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({'last_checked': timestamp_str}, indent=2) + '\n', encoding='utf-8')
        return True
    except Exception as exc:
        print(f'Warning: Could not write state file {state_path}: {exc}', file=sys.stderr)
        return False


def fetch_outside_activity(
    repo: str,
    since_str: str,
    owner: str = 'hffgzktbcc-blip',
    mock_issues=None,
    mock_comments=None,
) -> list:
    """Fetch new issues, PRs, and comments created since `since_str` by outside accounts."""
    since_dt = parse_iso(since_str)
    results = []

    # 1. Fetch issues and PRs
    if mock_issues is not None:
        issues = mock_issues
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues?since={since_str}&state=all&sort=created&direction=desc&per_page=100']
            out = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            issues = json.loads(out)
        except Exception as exc:
            print(f'Warning: Failed to query issues from gh CLI: {exc}', file=sys.stderr)
            issues = []

    for item in issues:
        created_str = item.get('created_at', '')
        if not created_str:
            continue
        created_dt = parse_iso(created_str)
        if created_dt < since_dt:
            continue

        author = item.get('user', {}).get('login', '') if isinstance(item.get('user'), dict) else ''
        author_type = item.get('user', {}).get('type', '') if isinstance(item.get('user'), dict) else ''
        if author_type == 'Bot' or is_excluded_account(author, owner):
            continue

        is_pr = item.get('pull_request') is not None
        item_kind = 'PR' if is_pr else 'Issue'
        results.append({
            'kind': item_kind,
            'number': item.get('number'),
            'title': item.get('title', ''),
            'author': author,
            'created_at': created_str,
            'url': item.get('html_url', ''),
            'excerpt': make_one_line_excerpt(item.get('body', '')),
        })

    # 2. Fetch comments
    if mock_comments is not None:
        comments = mock_comments
    else:
        try:
            cmd = ['gh', 'api', f'repos/{repo}/issues/comments?since={since_str}&sort=created&direction=desc&per_page=100']
            out = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            comments = json.loads(out)
        except Exception as exc:
            print(f'Warning: Failed to query comments from gh CLI: {exc}', file=sys.stderr)
            comments = []

    for c in comments:
        created_str = c.get('created_at', '')
        if not created_str:
            continue
        created_dt = parse_iso(created_str)
        if created_dt < since_dt:
            continue

        author = c.get('user', {}).get('login', '') if isinstance(c.get('user'), dict) else ''
        author_type = c.get('user', {}).get('type', '') if isinstance(c.get('user'), dict) else ''
        if author_type == 'Bot' or is_excluded_account(author, owner):
            continue

        issue_url = c.get('issue_url', '')
        issue_number = issue_url.rstrip('/').split('/')[-1] if issue_url else ''

        results.append({
            'kind': 'Comment',
            'number': issue_number,
            'title': f'Comment on #{issue_number}',
            'author': author,
            'created_at': created_str,
            'url': c.get('html_url', ''),
            'excerpt': make_one_line_excerpt(c.get('body', '')),
        })

    # Sort chronological or reverse chronological
    results.sort(key=lambda x: parse_iso(x['created_at']))
    return results


def format_activity_report(items: list, repo: str, since_str: str) -> str:
    lines = [
        f'Outside activity for {repo} since {since_str}:',
    ]
    if not items:
        lines.append('No new outside activity found.')
        return '\n'.join(lines)

    lines.append(f'Found {len(items)} item(s) needing attention:\n')
    for it in items:
        kind = it['kind']
        num = it['number']
        author = it['author']
        created = it['created_at']
        url = it['url']
        excerpt = it['excerpt']

        if kind in {'Issue', 'PR'}:
            title = it['title']
            lines.append(f'[{kind} #{num}] by @{author} ({created})')
            lines.append(f'  Title: {title}')
            lines.append(f'  Link: {url}')
            lines.append(f'  Excerpt: {excerpt}')
        else:
            lines.append(f'[Comment on #{num}] by @{author} ({created})')
            lines.append(f'  Link: {url}')
            lines.append(f'  Excerpt: {excerpt}')
        lines.append('')

    return '\n'.join(lines).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description='List new outside contributor activity since a saved timestamp.')
    parser.add_argument('--repo', default=os.environ.get('GH_REPO', 'hffgzktbcc-blip/ounce-core'))
    parser.add_argument('--owner', default=os.environ.get('REPO_OWNER', 'hffgzktbcc-blip'))
    parser.add_argument('--state-file', default=os.environ.get('OUTSIDE_ACTIVITY_STATE_FILE', DEFAULT_STATE_FILE))
    parser.add_argument('--since', default='', help='Override timestamp to check since (ISO format)')
    parser.add_argument('--no-update', action='store_true', help='Do not update state file after run')
    parser.add_argument('--json', action='store_true', help='Output JSON array instead of human-readable report')
    args = parser.parse_args()

    since_timestamp = args.since or read_state_timestamp(args.state_file)
    items = fetch_outside_activity(args.repo, since_timestamp, owner=args.owner)

    if args.json:
        print(json.dumps(items, indent=2))
    else:
        report = format_activity_report(items, args.repo, since_timestamp)
        print(report)

    if not args.no_update:
        new_timestamp = format_iso(datetime.now(timezone.utc))
        write_state_timestamp(args.state_file, new_timestamp)

    return 0


if __name__ == '__main__':
    sys.exit(main())
