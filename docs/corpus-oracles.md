# First-frame oracles for the step 2 corpus

PLAN step 2 grows the corpus from 4 to 20 outside programs. The 4 original programs keep their own
oracles (`docs/first-frame-oracle.md`, `docs/deko-basic-oracle.md`, `docs/romfs-oracle.md`,
`docs/startup-oracle.md`). This file covers all 20 in `tests/corpus/oracles.json`: the 16 added in step 2, plus the
original 4 restated so one runner and one evidence format cover the whole corpus. Their dedicated
tests stay in CI unchanged. `startup` and `romfs` get new exact oracles written in step 2 from source
(their old oracles only required a non-zero hash); earlier CI logs printed their frame hashes and
were not consulted. `simplegfx` restates its documented hashes, which `tools/corpus_oracle.py`
reproduces independently from the source formula. `deko-basic` reuses the hashes of its existing
oracle, which were recorded from a run, so it is a regression check of an observed frame.

## How the oracles were registered

1. For each program, a person read its source at the pinned commit and wrote down what the first
   queued frames must show: the exact text and escape codes a console program prints before each
   `consoleUpdate`, the rectangles an SDL program fills, or the clear colour a GPU program uses.
   That written-down "spec" sits next to each oracle in `tests/corpus/oracles.json`, together with
   the assumptions it rests on (for example "no input is injected").
2. `tools/corpus_oracle.py` turns each spec into the exact expected frame and stores its 1280x720 (or
   1920x1080) RGBA8 FNV-1a 64 hash, the same hash as `Frame::content_hash`. For console programs it
   models libnx 4.12.0's console (escape parser, cursor, clearing, scrolling) and the renderer the
   program uses: the CPU renderer (`console_sw.c`, RGB565 palette) or the deko3d GPU renderer of
   `deko_console` (`gpu_console.c`, RGBA8 palette). Glyphs come from libnx's
   `nx/data/default_font.bin` (ISC), read from a libnx checkout and never committed.
3. The specs and hashes of the 16 new programs were committed and pushed in `2fe006e` **before any
   ounce-core run of them**. The next commit only corrected the NRO file name of two repo-root
   builds (`source.nro`, from `$(notdir $(CURDIR))`) before any run. CI recomputes every hash from its spec (`--check`) on each run, so a spec and its
   hash cannot drift apart.

No ounce-core output was consulted to write a spec or a hash. If an oracle is wrong, it stays
wrong until a person shows from the source why, and the fix is recorded here with that reason.

## Pass criteria (all required, both engines)

1. The NRO is built from the pinned, unchanged source with the devkitA64 Docker image
   (`tests/corpus/oracle-builds/*.json`, built by `scripts/prepare_external_corpus.py`).
2. The guest queues `queued_frames` VI frames and stops on the runner's frame limit. A fault,
   unhandled SVC, guest break, budget or host timeout is a failure, even after a frame.
3. Every checked frame has the expected size and, for exact oracles, the expected hash (every
   pixel matches). Probe oracles check the stated pixels only.
4. The interpreter and Tier-3 produce identical frames.

Kinds: `console_sw` and `console_deko` and `rects` are **pixel-exact** (whole-frame hash).
`probes` (2 programs whose frames depend on font rasterising or 3D geometry that cannot be
predicted exactly from source) check named pixels only, so a pass is weaker; the results table says
which kind each oracle is.

## The 20 oracles

| oracle | build | source read | kind | frames checked |
|---|---|---|---|---|
| `printing-hello-world` | switch-examples | `graphics/printing/hello-world/source/main.c` | console_sw | 1–5 of 5 |
| `vt52-demo` | switch-examples | `graphics/printing/vt52-demo/source/main.c` | console_sw | 1–5 of 5 |
| `read-controls` | switch-examples | `hid/read-controls/source/main.c` | console_sw | 1–5 of 5 |
| `touch-screen` | switch-examples | `hid/touch-screen/source/main.c` | console_sw | 1–5 of 5 |
| `sixaxis` | switch-examples | `hid/sixaxis/source/main.c` | console_sw | 1–5 of 5 |
| `audren-simple` | switch-examples | `audio/audren-simple/source/main.c` | console_sw | 1–5 of 5 |
| `get-system-language` | switch-examples | `settings/get_system_language/source/main.c` | console_sw | 1–5 of 5 |
| `barrier` | switch-examples | `misc/barrier/source/main.c` | console_sw | 1 of 1 |
| `user-events` | switch-examples | `misc/user_events/source/main.c` | console_sw | 1 of 1 |
| `user-timers` | switch-examples | `misc/user_timers/source/main.c` | console_sw | 1 of 1 |
| `deko-console` | switch-examples | `graphics/deko3d/deko_console/source/main.c` | console_deko | 1–5 of 5 |
| `deko-examples-menu` | switch-examples | `graphics/deko3d/deko_examples/source/main.cpp` | console_sw | 1–5 of 5 |
| `sdl2-simple` | switch-examples | `graphics/sdl2/sdl2-simple/source/main.cpp` | rects | 1–2 of 2 |
| `raufang-hello-cpp` | raufang-switch-hello-world | `source/main.cpp` | console_sw | 1–5 of 5 |
| `nx-hbmenu` | nx-hbmenu | `common/menu.c, common/theme.c, nx_main/main.c` | probes | 1 of 1 |
| `hybrid-app` | hybrid-app | `source/main.cpp` | probes | 1 of 1 |
| `simplegfx` | original | `graphics/simplegfx/source/main.c` | rects | 1, 2, 60, 61 of 61 |
| `deko-basic` | original | `graphics/deko3d/deko_basic/source/main.cpp` | observed hash | 1–5 of 5 |
| `startup` | original | `templates/application/source/main.c` | console_sw | 1–5 of 5 |
| `romfs` | original | `fs/romfs/source/main.c and romfs/` | console_sw | 1–5 of 5 |

## Running one locally

```sh
OUNCE_CORPUS_ROOT=<builds> OUNCE_ORACLE_ID=read-controls \
  cargo test --release corpus_first_frame_oracle -- --ignored --nocapture
```

`<builds>/<build>/build/source/<nro>` must hold the program built as above. CI runs all of them with
`scripts/run_corpus_oracles.py` in the `first-frame-oracles` job of `.github/workflows/external-corpus.yml`.

## Ounce-Verify evidence

`scripts/corpus_verify.py evidence` turns each run into `ounce-verify/v0` records
(`docs/verify/README.md`), one per program and engine (`corpus/<id>/interpreter`,
`corpus/<id>/tier3`). Each record has the pinned commit, a source-tree SHA-256 (over the sorted
`<sha256>  <path>` lines of every tracked file at that commit), the executed NRO's SHA-256 (checked
against the build inventory), the toolchain image, the engine, limits, the guest stop, and the
raw guest-frame oracle with expected and observed values. Required oracles: `guest-stop` (frame
limit) and `first-frame`; engine agreement is recorded as a non-required comparison. CI validates
the combined file with `tools/ounce_verify.py` and uploads `canonical-report.json` as the
`ounce-verify-canonical-report` artifact. The README headline is derived from that report: a
program counts only when both of its records pass, and CI fails if the README count differs
(`scripts/corpus_verify.py headline <report> --write-readme` updates it).

## Results

Observed verdicts are copied from CI into `tests/corpus/oracle-results.json`. A recorded pass is a
ratchet: CI fails if it stops passing. Failures are PLAN step 3's to-do list.
