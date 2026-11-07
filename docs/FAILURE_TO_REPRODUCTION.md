# Diagnose a retained failure with Python only

From a fresh checkout, with Python 3.10 or newer:

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

## Execution acceptance still open

This checkout retains genuine before/after CI observations, not a newly executed
replay. PR #158 reported an earlier successful reproduction, but its separate raw
replay files were not available here. The observation artifacts contain no NRO.
To finish R1, obtain an authorized exact-hash binary, execute the generated recipe
on a supported Rust/runtime host, retain the new raw JSON/log and compare both
engines' digest, frame absence, stop, PC and instruction count to the original
failure. A build failure or different binary does not satisfy acceptance.

The observation-only distribution review is recorded in
[NOTICE.md](reproduce/sdl2-failure/NOTICE.md). Broader historical provenance issues
do not authorize distribution of additional binaries. See
[the verifier documentation](verify/README.md) for other retained-report commands.
