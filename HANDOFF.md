# ounce-core handoff (start here)

Every session: read this file and `AGENTS.md`, then work. Don't read `docs/HANDOFF_HISTORY.md` unless you need one specific old entry (grep it).

## 1. Mission
- **Measure of progress:** outside samples (unmodified upstream homebrew) with a verified frame. **Current count: 4** (`simplegfx`, `deko_basic`, `romfs`, `startup`).
- **Completed:**
  - `simplegfx`: First verified linear frame buffer display.
  - `deko_basic`: First verified 3D GPU-drawn frame (Maxwell/Ampere NVN/deko3d, block-linear detiling, smooth RGB vertex interpolation). PR #121 merged.
  - `romfs`: First verified filesystem and console text display sample (RomFS Unicode/ASCII reading, 16-bit R5G6B5Unorm surface decoding). PR #123 merged.
  - `startup`: Verified application startup baseline and text console presentation (`templates/application`). PR #125 merged.
- **Next target:** The next quickest outside sample (input, JIT, or audio).
- **Philosophy:** correctness first, observability second, performance third. No feature work that no real sample needs.

## 2. Rules
- The owner's instruction wins. No lanes, no turf, no standing down. One lead agent per goal (currently Claude).
- Claims are tasks with a date, written in `AGENTS.md`. A claim older than 24 h with no push is void.
- Before starting, check open PRs touching the same files. Build on them or comment; don't wait.
- Ask the owner only for human-only things: credentials, hardware, permissions, legal calls.
- **Merge gate:** all 9 CI matrix jobs green on the final head, plus applicable security checks. No admin bypass. Agents may merge.

### Batch work, merge once per milestone (owner rule)

- Work on one branch per milestone. Push frequently, but merge into main only once the milestone is complete.
- Do not wait for CI between development steps. Final CI must pass before merging.
- Use one PR per milestone. Include small documentation and coordination changes in that PR.
- Before every push, run `cargo test --release`, verify harness 27/27, and check for new unsafe code.
- Fix failing CI immediately before continuing development.
- Separate PR exceptions: broken main, security fixes or changes blocking another session.
- Do not implement CI path filters or weaken workflow checks without explicit owner approval.
- **Gates before every push:** `cargo test --release` passes; `cargo run --release -- test` prints `[Harness] All 27 checks passed across 25 stages.`; no new `unsafe` in `src` (`#![forbid(unsafe_code)]`).
- Never game a check: no skipping or weakening tests, no excluding code from scanners, no empty commits.

## 3. What code we may use (owner decision)
- **Use freely, port from, and credit:** Mesa (MIT; especially NVK/nil and the nouveau compiler), libnx, NVIDIA open-gpu-doc, envytools, the switchbrew wiki, ordinary Rust crates.
- **Read only, don't copy:** Atmosphère (GPL).
- **Never:** yuzu or Ryujinx code; Nintendo keys or firmware; retail decryption or retail game boot (`LoadError::RestrictedCipher` stays).
- Don't use the word "emulator" in docs or metadata.
- Stop guessing where a public answer exists. Anything still unchecked against a public source or hardware gets marked UNVERIFIED.

## 4. Token and model discipline
- Tools: git, cargo, the GitHub MCP tools, file read/edit. No plugins, skills or connectors. No subagents unless the owner asks.
- grep first, then `sed -n` line ranges. Never dump whole files or long logs. Batch independent commands into one call.
- PR bodies and handoff notes: what changed, what's verified, what's left. Short.
- **Model choice:** Opus for hard debugging (GPU bring-up, shader or tiling bugs); Sonnet for merges, CI babysitting, docs and recipe runs such as independent reruns; Haiku for status checks. Use lower reasoning effort for routine work.

## 5. Current state (2026-11-01)
- **Done:** simplegfx passes all 6 criteria in `docs/first-frame-oracle.md` locally and in CI (both engines, 61 frames, NRO SHA-256 `b59fc85f0079bcd93007d97150e2af8302ac0804d1120cee43cf32fd37b69bf1`); a separate session's rerun passed. Merged #114, #105, #99, #117, #103, #119, #120.
- **Open:** #104 (NIST CTR and prod.keys parser tests; alert dismissed, merge when green). #115 (texture mips) paused until a real sample needs mipmapped textures.
- **Closed:** #108, #113, #116, #118 (off-goal or duplicates).

## 6. Next work, in order
1. Merge #104 when green.
2. **Flight recorder:** the corpus runner writes its `Report` as one JSON file (`OUNCE_REPORT_JSON=<path>`): git commit, NRO SHA-256, stop reason, one `first_failure` (PC + opcode, SVC, IPC service + command, nvdrv ioctl, GPU class + method, or VI diagnostic), last SVCs, frames queued. Agents diagnose from that file, not from long logs.
3. **deko_basic:**
   1. Add it to `tests/corpus/manifest.json` and build it (toolbox below).
   2. Run the probe with `OUNCE_TRACE_IPC=1`; write down the first failure.
   3. Pre-register an oracle before chasing the frame: `docs/deko-basic-oracle.md`, from its `main.c` and shaders. Clear colour, triangle corner and centre pixels, frame count; pixels that vary by position so tiling is tested.
   4. Fix forward. Known gap: 2D class `0x902D` methods `0x2A4`, `0x290`, `0x884` (from open-gpu-doc `cl902d.h`). Likely next: present of a block-linear surface, the tiling sector layout (Tegra X1 vs Orin caveat in `src/gpu/tiling/`), Maxwell `ALD`/`AST`/`IPA` encodings (UNVERIFIED). Check each against Mesa.
   5. Add `deko_basic_first_frame_oracle` (opt-in test + CI step in `external-corpus.yml`, like simplegfx).
   6. Write `docs/REPRODUCE_DEKO_BASIC.md`; a separate session (Sonnet) reruns it and posts on the PR.
   7. Count = 2. Update `AGENTS.md`.
4. Next quick sample, same recipe.
5. Deferred until samples demand them: Tier-4 profiling, GPU backend split and wgpu/Metal speed, ARMv8.2 coverage, multi-core, fastmem (probably no: likely needs `unsafe`).

## 7. Toolbox (cloud container)
- Build deps: `sudo apt-get install -y libudev-dev libasound2-dev pkg-config`
- Docker: `sudo dockerd >/tmp/dockerd.log 2>&1 &`, then `sudo docker`. Toolchain: `sudo docker pull devkitpro/devkita64:latest`. Network inside containers: `--network host -e HTTPS_PROXY="$HTTPS_PROXY" -e https_proxy="$HTTPS_PROXY" -e GIT_SSL_CAINFO=/ca.crt -v /root/.ccr/ca-bundle.crt:/ca.crt:ro`.
- Build the corpus (output dir must be new and outside the repo); NROs land under `/tmp/corpus/build/source/<sample path>/`:
  ```bash
  sudo docker run --rm --network host -e HTTPS_PROXY="$HTTPS_PROXY" -e GIT_SSL_CAINFO=/ca.crt -v /root/.ccr/ca-bundle.crt:/ca.crt:ro -v "$PWD:/repo:ro" -v /tmp/corpus:/corpus devkitpro/devkita64:latest bash -lc 'python3 /repo/scripts/prepare_external_corpus.py --manifest /repo/tests/corpus/manifest.json --output /corpus/build --build'
  ```
- Probe any NRO: `OUNCE_CORPUS_NRO=<nro> cargo test --release external_corpus_nro_probe -- --ignored --nocapture` (add `OUNCE_REPORT_JSON=<path>` for the flight recorder).
- simplegfx oracle: `OUNCE_SIMPLEGFX_NRO=<nro> cargo test --release simplegfx_first_frame_oracle -- --ignored --nocapture` (~2 min; `OUNCE_SIMPLEGFX_FRAMES=3` for a quick check).
- GitHub: writes (merge, comment, close, open PR) go through the GitHub MCP tools; `gh api` for reads and search only (writes through it are blocked). Force pushes are blocked: push to a new branch name. Draft to ready: `gh api -X POST repos/<owner>/<repo>/pulls/<n>/ccr/ready_for_review`.
- CI: the 9 matrix jobs are in `ci.yml`; corpus build, probes and oracle steps in `external-corpus.yml` (`startup-probe`, 45 min); CodeQL in `rust-security.yml`.

## 8. Honest caveats
- simplegfx draws one flat colour from the CPU. It proves VI/Binder plumbing, not tiling, the GPU or timing. deko_basic is the real test.
- Many GPU pieces have never met real hardware data. deko_basic could pass in hours or take a couple of days. Using Mesa as reference is the shortcut.
- "Finished product" (most homebrew, fast, GUI, audio, input) is weeks to months. Count samples, not features.

## 9. Owner to-dos (human only)
- Archive idle sessions once their work is pushed.
- Turn off unused connectors in claude.ai settings.
- Optionally allow `gitlab.freedesktop.org` (Mesa) in the environment's network settings.
