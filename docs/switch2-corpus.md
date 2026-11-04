# How a Switch 2 homebrew program enters the corpus (PLAN step 8)

Status, 2026-11-04: **no outside Switch 2 homebrew program is in the corpus**, because no public,
open-source toolchain can build one yet. ounce-core makes no Switch 2 claim. The harness below is
ready so that, when such tools appear, adding a program is the same flow as step 2
(`docs/corpus-oracles.md`): add source and provenance, pre-register the oracle, let CI run it.

Commercial Switch 2 software never enters this repo (PLAN "Legal guardrails"). Only homebrew built
from public source with public tools, already decrypted by nature, is accepted.

## The files

| file | what it holds |
|---|---|
| `tests/corpus/switch2/oracles.json` | pre-registered oracles of **outside** programs (header `platform: switch2`, `origin: outside`) |
| `tests/corpus/switch2/oracle-builds/<build>.json` | one build manifest per pinned source tree |
| `tests/corpus/switch2/oracle-results.json` | verdicts observed in CI; a recorded pass is a ratchet |
| `tests/corpus/switch2/synthetic-oracles.json` | our own **synthetic** harness programs; never counted |
| `src/loader/switch2.rs` | oracle-set header and intake check (platform, origin, container) |
| `scripts/check_switch2_corpus.py` | validates all of the above before any run |

The `switch2-oracles` job in `.github/workflows/external-corpus.yml` checks the files, recomputes every
expected value from its spec, builds each manifest with its own toolchain image, runs every oracle under
the interpreter and Tier-3 (`scripts/run_corpus_oracles.py --set switch2`), and writes Ounce-Verify
records with target `switch2-homebrew` (`scripts/corpus_verify.py evidence --set switch2`). Original
Switch records can never be relabelled as Switch 2 (`tests/test_ounce_verify.py`).

## Adding a program, step by step

1. **Pick a program we did not write**, with public source and a licence that allows building it
   (no GPL code is ported into ounce-core, but building and running a GPL program is fine). No game
   names in ids or docs; neutral names only.
2. **Add the build manifest** `tests/corpus/switch2/oracle-builds/<build>.json`:

   ```json
   {
     "schema_version": 1,
     "platform": "switch2",
     "repository": "https://github.com/<owner>/<repo>.git",
     "revision": "<exact 40-hex commit>",
     "upstream_terms": "<licence and file it was read from>",
     "toolchain_image": "<public docker image, pinned by digest>",
     "compiler": "<compiler path relative to DEVKITPRO, if not devkitA64's gcc>",
     "samples": [{"id": "<sample>", "path": "<dir with its Makefile, or .>"}]
   }
   ```

   `scripts/prepare_external_corpus.py` clones the commit, checks the tree is unchanged, runs `make`
   in each sample directory inside `toolchain_image` (which must set `DEVKITPRO`), and records the
   source, compiler and output hashes. If the first public toolchain does not build with `make`,
   extend that script once, for every later program.
3. **Record provenance** in `docs/PROVENANCE.md`: repository, commit, licence, toolchain and version.
4. **Write the oracle from the source, before any run.** Add an entry to
   `tests/corpus/switch2/oracles.json`:

   ```json
   {"id": "<sample>", "platform": "switch2", "format": "nro", "build": "<build>",
    "nro": "<output path in the build tree>", "source": "<files read>", "queued_frames": 5,
    "checks": [{"frames": [1, 2, 3, 4, 5], "spec": {"kind": "console_sw", "stream": "..."}}],
    "assumptions": ["<what the spec rests on, e.g. no input injected>"]}
   ```

   Spec kinds are those of `tools/corpus_oracle.py` (`console_sw`, `console_deko`, `rects`, or
   `probes` for named pixels). If the program draws something no kind can predict, add a kind to that
   tool first, from the program's source. Then fill in the expected values:

   ```sh
   python3 tools/corpus_oracle.py --font <libnx>/nx/data/default_font.bin \
     --oracles tests/corpus/switch2/oracles.json
   python3 scripts/check_switch2_corpus.py
   ```

5. **Commit and push the manifest and oracle on their own**, before ounce-core has ever run the
   program. That commit is the pre-registration; name it in the PR. Nothing observed from an
   ounce-core run may go into a spec or a hash. A wrong oracle stays wrong until someone shows from the
   source why, and the fix is recorded with that reason.
6. **Let CI run it.** Copy the observed verdict into `oracle-results.json` (`pass` or `fail`). The
   first `pass` ticks PLAN step 8, with the CI run as proof.

## Program containers

The intake check accepts the container named by each entry's `format` and refuses a file that holds
anything else. Only `nro` exists today. If public Switch 2 homebrew uses another container, add it to
`ProgramFormat` in `src/loader/switch2.rs`, `FORMATS` in `scripts/check_switch2_corpus.py`, and the
runner's load path, with the format's public documentation cited in `docs/PROVENANCE.md`. Encrypted
or retail containers (NCA, NSP, XCI) are never accepted, under any platform.

Nothing in the runtime is tuned for Switch 2 yet: no public homebrew exists to show what differs.
Each difference lands with the first outside program that needs it, as a step 3 style fix.

## The synthetic programs (never progress)

`tests/fixtures/switch2-synthetic/` holds two programs we wrote (framebuffer rectangles and CPU
console text), built with the original-Switch toolchain because no Switch 2 one exists. Their oracles
run on every CI test job (`switch2_synthetic_oracles_pass_on_both_engines`) to keep the Switch 2 path
working: set header, intake check, both engines, frame hashes. They are labelled `origin: synthetic`,
the checker refuses them in the outside set, and they are never counted (PLAN Rule 0).
