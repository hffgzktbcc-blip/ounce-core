#!/usr/bin/env python3
"""Fail if a tracked Markdown file links to a file that does not exist.

Checks relative links only (external URLs and pure #anchors are skipped), ignores fenced code blocks and
inline code, and strips #fragments and ?queries. Usage: python3 scripts/check_links.py [repo_root]
"""
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

LINK = re.compile(r'(?<!\!)\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)|!\[[^\]]*\]\(\s*<?([^)\s>]+)>?')
FENCE = re.compile(r'^\s*(```|~~~)')
INLINE_CODE = re.compile(r'`[^`\n]*`')
SKIP = ('http://', 'https://', 'mailto:', 'tel:', 'data:', '#', '//')


def tracked_markdown(root):
    out = subprocess.run(['git', '-C', str(root), 'ls-files', '*.md'], capture_output=True, text=True, check=True).stdout
    return [root / line for line in out.splitlines() if line]


def broken_links(root):
    root = Path(root).resolve()
    problems = []
    for md in tracked_markdown(root):
        if not md.exists():
            continue
        in_fence = False
        for number, line in enumerate(md.read_text(encoding='utf-8', errors='replace').splitlines(), 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for match in LINK.finditer(INLINE_CODE.sub('', line)):
                target = match.group(1) or match.group(2)
                if target.startswith(SKIP):
                    continue
                path = unquote(re.split(r'[#?]', target, maxsplit=1)[0])
                if not path:
                    continue
                base = root if path.startswith('/') else md.parent
                if not (base / path.lstrip('/')).exists():
                    problems.append(f'{md.relative_to(root)}:{number}: broken link -> {target}')
    return problems


def main(argv):
    root = Path(argv[1]) if len(argv) > 1 else Path(__file__).resolve().parent.parent
    problems = broken_links(root)
    for problem in problems:
        print(problem)
    print(f'{len(problems)} broken relative Markdown link(s)')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
