# Soak: corpus programs beyond the first frame

The first-frame oracles ([corpus-oracles.md](corpus-oracles.md)) stop after a few frames. The soak keeps each
registered program running for thousands of frames or a host time budget, headless, on the interpreter and
Tier-3, and looks for four things: a crash, a hang, memory that keeps growing, and the two engines disagreeing.
It does **not** say what a frame should show: no expected image is involved, so passing the soak is not a
compatibility result.

```sh
cargo test --release --no-run          # note the ounce_core test executable it prints
python3 scripts/soak_corpus.py --executable <that executable> --builds <corpus builds dir> \
  --evidence /tmp/soak --frames 3000 --seconds 600 --shards 4          # whole corpus, or --ids a,b
python3 scripts/soak_corpus.py ... --input-seed 7                     # add scripted pseudo-random buttons
python3 scripts/soak_corpus.py ... --smoke                            # CI-sized (below)
```

`<corpus builds dir>` is the directory that holds `<build>/build/source/<nro>` (the output of
`scripts/prepare_external_corpus.py` / the CI `corpus-nros` artifact). Each program runs in its own process under an
outer timeout, writes `<id>.soak.json` (stop reason, one frame hash per frame, periodic samples, last SVCs and
registers) and `<id>.soak.log`, and the run writes `soak-summary.json`. The runner is `corpus_soak` in
`src/loader/corpus_soak.rs`; it reuses the corpus runner's loader, deterministic clock and frame capture.

## What counts as a finding

| finding | rule |
|---|---|
| crash | an engine stops on a fault, unhandled SVC, guest break or CPU halt |
| hang | the host budget ran out and no frame was queued for the last quarter of it (at least 5 s), or no frame at all |
| memory growth | after the first quarter of the samples, the peak of host RSS exceeds the warm-up peak by 128 MiB, guest pages by 2048, or Tier-3 cache entries by 2048 |
| divergence | interpreter and Tier-3 frame hashes differ at any frame both reached, or both exit at different frame counts |

A run that reaches the frame target is `soaked`. A run that is still producing frames when the time budget ends is
`slow`, not a finding. Register hashes at each sample are compared too and listed as `register_divergent_samples`;
they are information only, because scheduler quanta differ by engine for threaded guests (none occurred below).
Host RSS comes from `ps`, which on macOS leaves compressed pages out, so the guest-page count is the steadier number.

Findings that are understood and filed are listed in [`tests/corpus/soak-known.json`](../tests/corpus/soak-known.json)
with their issue; only an unlisted finding fails the script.

## CI-sized variant

`--smoke` soaks four programs (`printing-hello-world`, `sdl2-simple`, `user-timers`, `deko-basic`) for up to 300
frames or 90 s per engine. The `corpus-soak-smoke` job in `external-corpus.yml` runs it on the corpus NROs the first-frame job
built. Locally (macOS arm64, 4 shards) it took 3 minutes; it was not run in CI before this change.

## What the first runs showed (macOS arm64, 2026-11-11)

All numbers are from the retained runs below; nothing here was run on other hardware.

* **Whole corpus, idle, 60 s per engine, 50 programs:** 48 stayed alive on both engines for the whole budget
  (interpreter 10–263 frames, Tier-3 17–280 frames; the frame cost is dominated by the guest's software
  block-linear conversion, about 4 million guest instructions per console frame). No divergence over every frame
  both engines reached, no growth finding. The same 48 with scripted input (seed 7, 40 s): same result; the input
  changes frames for `read-controls` (9 distinct frames against 1 idle).
* **Long run, 4 programs, up to 3000 frames or 600 s per engine, idle:** Tier-3 reached 3000 frames on
  `user-timers`, `deko-examples-menu`, `hybrid-app` and `deko-basic`; the interpreter reached 743, 737, 1795 and 2107
  frames in its 600 s. Frame hashes equal on every compared frame (743, 737, 1795, 2107), guest pages constant,
  host RSS growth after warm-up at most 80 KiB. The Tier-3 block cache ended at 1,518 (`user-timers`), 1,764
  (`deko-examples-menu`) and 2,357 (`deko-basic`) entries, but is large and still creeping for `hybrid-app`: 24,424
  entries after about 220 frames in the 60 s run, 27,584 at 3000 frames (growth 1,941 after warm-up, below the 2048
  threshold). Whether it plateaus was not established, and I did not check whether the cache has a size limit.
* **Two crashes, same on both engines, both after the first frame the oracles check:**
  [#386](https://github.com/hffgzktbcc-blip/ounce-core/issues/386) `exception-handler` (guest faults on purpose;
  no userland exception handler delivery) and
  [#387](https://github.com/hffgzktbcc-blip/ounce-core/issues/387) `applet-software-keyboard` (deadlock marker
  `0xDEAD10C4` in `swkbdShow`; library applets are not modelled). Neither is fixed here.
* **Observation, not a finding:** the process RSS is about 290 MB for most programs and for Tier-3 on all of them,
  and about 30 MB for a few interpreter runs, with 65,911 guest pages (257 MiB) mapped in all cases. It is flat
  within a run, so it is not growth; why Tier-3 touches those pages when the interpreter does not was not
  investigated.

## Limits

Frames came from at most 3000 frames or 10 minutes per engine on four programs and about a minute on the rest.
That does not show absence of leaks or hangs over longer runs, other hosts, Native or Tier-4 (not covered), or any
input beyond idle and one seeded pseudo-random script. Interpreter speed is low enough (about 1–4 frames per second here)
that thousands of frames need a long budget; issue
[#383](https://github.com/hffgzktbcc-blip/ounce-core/issues/383) tracks one overhead in that path.
