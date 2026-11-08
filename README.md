# Ounce diagnosis toolkit 0.2.0

This standalone Python tool inspects two retained failure/reproduction cases:
SDL2 startup and the external libnx #685 audio-padding interaction. It does not
contain or distribute a runtime, homebrew executable, upstream source, firmware,
keys, fonts or frame pixels. Project-authored tooling and observation data are
covered by LICENSE; observation-specific notices are included in generated bundles.

Python 3.9+ is required. No pip package, Cargo, Docker or network is needed for
recorded diagnosis. From this standalone source snapshot:

```sh
python3 -B scripts/build_diagnosis_cli.py --output dist/ounce-diagnose.pyz
python3 -I dist/ounce-diagnose.pyz --version
python3 -I dist/ounce-diagnose.pyz example --output sdl2-dossier
python3 -I dist/ounce-diagnose.pyz audio-case --output audio-dossier
python3 -B -m unittest discover -s scripts -p test_diagnosis_cli.py
```

For the downloaded archive, run the same commands with its path. Verify SHA256SUMS
first. Checksums detect changed files; they do not authenticate a publisher.
The audio command explains both engines' seven rejected-before and stereo-after
replies. It checks recorded identities/digests and reply consistency; it does not
execute the guest, re-verify absent binaries, certify new code, or authenticate
input manifests. Custom dossiers need review before sharing; no credential scrubber
is provided. See each generated NOTICE.md and REPLAY.md.

Exit codes: 0 = dossier written or explicit SDL2 replay matched; 1 = recorded audio
repair/replay not matched; 2 = invalid input/prerequisite/output; 124 = explicit
SDL2 replay timeout; 130 = interruption. `replay` is SDL2-only and requires a
separate clean full-runtime checkout, Git/Cargo and the authorized exact-hash NRO.
Audio replay uses the separate generated Cargo recipe. The runtime is absent from
this source snapshot. Retained examples are internal evidence; no outside user
endorsement, independent rerun, six-channel playback or general debugger support
is claimed. No maintainer was contacted and no adoption feedback is available.

SOURCE.json identifies the original repository commit and every exported input.
This snapshot has its own standalone release history. It excludes the runtime
fixtures and historical source tree whose separate provenance review remains open.
The runtime distribution and its release assets are unchanged.
