# Security policy

Please report a vulnerability privately through GitHub's "Report a vulnerability" option on the repository's Security tab, not in a public issue. Include the affected version or commit, how to reproduce, and the impact. You should get an acknowledgement within a week.

Ounce-Core runs untrusted guest code. Anything that lets a guest program read or write the host outside its sandbox (the `--sdmc` directory), crash the host, or consume unbounded host resources is in scope. A guest program computing a wrong result is a normal bug: use the issue tracker.

## Supported versions

Security fixes are best effort and land on `main`. When a fix warrants a release,
the newest `0.10.x` patch supersedes earlier patches; there are no long-term or
parallel security-backport branches.

| Version | Security status |
| --- | --- |
| `main` | Best-effort fixes |
| Latest `0.10.x` patch | Best-effort until superseded |
| Earlier releases | Not supported |

The current static-analysis alert review is recorded in
[docs/security/CODEQL_ALERT_REVIEW_2026-11-21.md](docs/security/CODEQL_ALERT_REVIEW_2026-11-21.md).
