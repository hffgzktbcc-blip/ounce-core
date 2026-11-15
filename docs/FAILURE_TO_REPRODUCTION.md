# Diagnose a retained failure with Python only

For a standalone archive, installation and explicit replay, see [the diagnosis CLI](DIAGNOSIS_CLI.md).

From a fresh checkout, with Python 3.9 or newer:

```sh
python3 -B scripts/reproduce_failure.py --output /tmp/sdl2-reproduction
```

Choose a fresh output directory. No package installation, Cargo, Docker, network
access or executable input is needed for this offline step. It validates the
retained report/log digests, checks the raw observations against the reports, and
writes a portable directory containing the original evidence, input identities,
diagnosis, full historical comparison, notices, license and exact replay recipe.
Missing/corrupt evidence or an existing output directory exits 2 with an error;
success exits 0. A successful offline command does not mean the guest passed.

The two SDL2 engine cases first differ at queued frame 1: the passing reference
has the registered 1920×1080 frame; the failing run has none. Both failing engines
exit at `0x000000007ffe0004` after 9,104,054 instructions. Both sides identify the
same binary, upstream source, toolchain, execution limits and pixel expectations.
The suggested subsystem is guest initialization/service emulation. This narrows
the next tracing step; it does not identify a first differing guest instruction
or prove a root cause.

Read `REPLAY.md` in the output for the exact core revisions, NRO digest, environment
and Cargo command. `diagnosis.json` uses passing reference → failing candidate;
`comparison.json` uses historical failure → passing reference and retains all
40 historical engine records, showing zero lost prior passes. This is not a new
full-corpus execution or certification of today's 50 programs. Wall-time claims
are disabled because matched timing-host evidence is absent.

You can share the output directory as an archive. It contains observation data
and relative artifact paths, no binaries or local credentials. Keep `manifest.json`
and `NOTICE.md`: they link original CI artifacts and distinguish their recorded
integrity claims from local report validation. The original report bytes are
preserved, including the passing run's recorded-core/workflow-head distinction.

## Executed replay acceptance (2026-11-07)

The generated recipe was executed on macOS 26.6.2 arm64 with Rust 1.98.0,
using the exact-hash NRO recovered from a local build archive. Both recorded
revisions were checked out cleanly and built into separate target directories.
The NRO remains local and is not distributed in this walkthrough.

| Recorded revision | Cargo exit | Interpreter / Tier-3 result |
| --- | --- | --- |
| Failure `0e5c340` | 101 (expected oracle failure) | ExitProcess, PC `0x000000007ffe0004`, 9,104,054 instructions, no queued frames |
| Reference `bc46c76` | 0 | FrameLimit, PC `0x0000000080494204`, 172,663,220 instructions, both registered frames pass |

[Replay observations and manifest](reproduce/sdl2-failure/replay-2026-11-07/manifest.json)
retain raw JSON and the runtime portions of stdout/stderr. For each engine,
stop, PC, instruction count, X0, queued frame dimensions/hashes, failures and
execution limits match the corresponding historical observation exactly.
Wall times are retained but are not equality checks or performance claims.
This is an internal matched replay of one input, not an independent rerun or
a new full-corpus certification. No expectations or runtime source were changed.

The first exploratory runs shared an existing Cargo target directory and produced
inconsistent results, including a failing reference. Those runs were rejected as
acceptance evidence. Fresh builds in separate target directories produced the
matched observations above. Keep each revision's target directory separate when
replaying (`CARGO_TARGET_DIR`); do not reuse an unqualified historical test binary.

The bundle generator still performs offline inspection only. Its replay warning
means that generating a dossier does not execute the guest; the dated observations
linked above establish the separately performed execution acceptance.

The observation-only distribution review is recorded in
[NOTICE.md](reproduce/sdl2-failure/NOTICE.md). Broader historical provenance issues
do not authorize distribution of additional binaries. See
[the verifier documentation](verify/README.md) for other retained-report commands.
