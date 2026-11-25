# Security policy

Please report a vulnerability privately through GitHub's "Report a vulnerability" option on the repository's Security tab, not in a public issue. Include the affected version or commit, how to reproduce, and the impact. You should get an acknowledgement within a week.

Ounce-Core runs untrusted guest code. Anything that lets a guest program read or write the host outside its sandbox (the `--sdmc` directory), crash the host, or consume unbounded host resources is in scope. A guest program computing a wrong result is a normal bug: use the issue tracker.

## Supported versions

Security fixes are best effort and land on `main`. Only the newest patch of the newest
minor release is supported. When a fix warrants a release, it ships as the next patch
of that minor (or in the next minor); there are no long-term or parallel
security-backport branches. See [versioning and releases](CONTRIBUTING.md#versioning-and-releases).

| Version | Security status |
| --- | --- |
| `main` | Best-effort fixes |
| [Latest release](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest) (newest patch of the newest minor) | Best-effort until superseded |
| Earlier releases | Not supported |

The current static-analysis alert review is recorded in
[docs/security/CODEQL_ALERT_REVIEW_2026-11-21.md](docs/security/CODEQL_ALERT_REVIEW_2026-11-21.md).
