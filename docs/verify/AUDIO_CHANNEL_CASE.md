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
At this registration point no guest run or repair success is claimed.
