# External audio-channel padding case (pre-registered)

Selected from ITotalJustice's [libnx issue #685](https://github.com/switchbrew/libnx/issues/685).
The author reports that the channel parameter occupies the low u16, with the upper
half ignored. Existing libnx defaults work because their low half is zero. This
case covers that padding/default-stereo behavior only. It does not establish
six-channel playback or fix upstream libnx's global-session replacement behavior.

Input: `ITotalJustice/audout-channel-test`, exact commit
`55911e5910ba282330dd9ab78b3bf12f3b394e8f`, built unchanged in the already pinned
`devkitpro/devkita64` image `sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282`.
Build uses `/case` as its working directory and `make -j2`; output `case.nro` has
SHA-256 `69106c5c4b0485abdbce1ca9f17395822c7c7643d15f57b11b9bfc994d7f252c`.
Toolchain: libnx 4.12.0-1, devkitA64 r29.2-1. The source remains in a separate local
checkout. No license file was found; no source, executable or linked libraries are
copied into this repository or distributed as artifacts. Retained reports contain
our protocol observations and frame hashes only. No maintainer was contacted.

Before the first execution, the registered scenario is: retain the original three
prompt frames, press A after queued frame 3, then stop at queued frame 5 (the
reporter's completed upper-halfword test). On Interpreter and Tier-3 require all
seven output requests with raw channel value `0x00010000` to succeed and return
stereo (2), at 48 kHz. Preserve guest code, input timing and criteria between runs.
The bounded protocol trace distinguishes actual requests/replies from a guessed
screen result; frames show the guest reached the selected interaction boundary.
Existing corpus expectations and the 50-program denominator are unchanged.

```sh
OUNCE_AUDIO_CHANNEL_NRO=/path/to/local/case.nro \
OUNCE_REPORT_JSON=/path/to/new-observation.json \
  cargo test --release --locked \
  loader::fixture_tests::corpus_runner::external_audout_padding_case \
  -- --exact --ignored --nocapture --test-threads=1
```

Use an outer process timeout as well as the runner's 180-second-per-engine limit.
The scenario above was committed before execution in `0b8b38a`; the expected
seven successful stereo opens, input checkpoint and binary hash were fixed there.

## Executed result (2026-11-08)

[Before/after dossier](audout-padding/manifest.json): both engines reached all
five queued frames with the unchanged NRO. Before repair, all seven upper-halfword
requests returned `0xF601`; after `430ce19`, all seven returned success and stereo.
Both engines agreed on the recorded protocol observations in each run. The trace
retains the first 64 manager-open attempts with the raw parameter and reply;
`OUNCE_TRACE_IPC=1` also exposes these fields in the ordinary runtime logs.

The root cause was checking the complete padded u32 against three special values
(`0`, `2`, `0x20000`). `0x10000` has a valid default channel count in its low u16,
but the previous check rejected it. Output negotiation now reads that low half
and validates supported default/stereo values. Input negotiation is unchanged;
invalid low values still fail. No upstream source or existing corpus expectation
was patched. Six-channel negotiation/playback remains unsupported in this model.
The retained initial frame hash identifies a prompt frame; it is not a pixel
oracle for the audio result. The result comes from the seven recorded protocol
replies, completion boundary, and the CMIF response regression tests.

The full local regression run preserved all 50 programs on both CPU engines:
100 canonical records, each binary SHA-256-verified from local bytes. The dossier
retains that canonical report. All 475 unit tests and 3 CLI tests passed; 10 opt-in
tests were ignored. The separate external case was explicitly executed, and the
25-stage harness passed all 36 checks. This is internal evidence, with no outside
developer feedback or hardware-conformance claim.

## Local source/build prerequisites

Clone the author's repository into a separate local directory, check out the exact
commit above and keep tracked sources unmodified. With Docker and the pinned image:

```sh
docker run --rm --platform linux/amd64 \
  -v /absolute/path/to/local-source:/case -w /case \
  devkitpro/devkita64@sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282 \
  bash -lc 'make -j2 && sha256sum case.nro'
```

Check the resulting hash before running the registered Cargo command. Replay the
failure from `0b8b38a` and the repaired case from `430ce19` in separate worktrees
and target directories, retaining each JSON/log separately. The artifact is not
included; source accessibility does not imply binary redistribution permission.
This dossier distributes only repository-generated protocol observations, hashes
and text logs, under the repository MIT license. Build logs with local paths and
the compiled NRO remain local. The selected developer can obtain the same
request-level answer without inferred success from a window or a synthetic test.
