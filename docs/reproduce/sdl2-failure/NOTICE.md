# Retained observation scope and provenance

These files contain Ounce-Core-generated canonical JSON, SDL2 oracle observations,
and textual test logs, distributed with the repository's MIT license (included in
generated bundles). They contain no upstream source, executable/library bytes,
firmware, keys, or captured frame pixels. Hashes and frame dimensions identify
observations; they do not embed the corresponding content. This scope review
applies only to these six observation files, not to historical binary artifacts
or the broader provenance questions tracked in E2.

The files are byte-for-byte archive members downloaded on 2026-11-07. The manifest
records each original workflow run, artifact ID, archive SHA-256, member name and
member SHA-256. All four downloaded archives matched GitHub's recorded digest.
Local inspection rechecks member digests and validates canonical evidence, but
cannot authenticate a manifest edited alongside its files. Preserve this checkout
revision and the original GitHub artifact identities when sharing results.

- Failure: run [41872194259](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41872194259),
  recorded core `0e5c340885989c1f70bdb11ba9570a06ef1384b8`.
- Passing reference: run [42175855697](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42175855697),
  recorded core `bc46c7616acd5b2a2875194b591931789e83c8b0`.
  The workflow head was `f72a097f2913905dc6e66870301683647f49bdbf`;
  replay uses the report's recorded core, preserving this distinction.

Both reports retain all 40 historical engine records (20 programs). This does not
redefine the current 50-program corpus denominator. The passing report's historical
`artifact_integrity: verified` is a recorded CI claim, not a fresh verification by
the offline command. Required expectations, source identity and binary hash for
SDL2 match across the reports. The source is switchbrew/switch-examples at
`669786898205b7beb25ff1731e72982e6d0397d3`, path
`graphics/sdl2/sdl2-simple`; its public-domain notice and linked-library provenance
are catalogued in the source checkout's
[PROVENANCE.md](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/PROVENANCE.md).
No permission to redistribute linked executables is inferred here.

The NRO is not available in these observation-only archives. Exact replay requires
an authorized copy with SHA-256
`d92512342d6d571023da3c98d907f04c0c85043de7b2ffd50a745cbcc928bc7f`.
The toolchain identity is retained in each record; rebuilding does not guarantee
identical bytes. The recipe cannot substitute a newer NRO without changing the
claim. [PR #158](https://github.com/hffgzktbcc-blip/ounce-core/pull/158) reported an
actual historical replay, but its raw replay artifacts were not found in this
checkout. Offline diagnosis does not satisfy the new R1 fresh-replay acceptance.
