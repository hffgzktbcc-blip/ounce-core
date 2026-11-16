# `ounce-education` 1-Year Roadmap (Nov 2026 – Nov 2027)

**North Star:** Build **`ounce-education`** (MIT) as a zero-hardware, clean-room **AArch64 architecture, operating systems, compilers/JIT, and graphics lab curriculum** for university students and self-learners—powered exclusively by MIT **`ounce-core`** + **`ounce-hb`** via CLI & `ounce.run.v1` / `ounce.repro.v1` (no `GPL` mixing, no retail/key/SDK material, no AI needed to grade or debug).

---

## Q1 (Nov 2026 – Feb 2027): Zero-Setup Student Environment & First 5 Labs

1. **Repository & Toolchain Setup (MIT):**
   - Create `ounce-education` (MIT) with one-command setup via DevContainer / standalone install script (local & GitHub Codespaces) and `hffgzktbcc-blip/ounce-hb/.github/actions/setup-ounce` (GitHub Actions: Linux x86_64/arm64, macOS arm64, Windows x86_64 once Q1 CI/`.exe` verification lands) verified by `ounce-hb doctor`.
   - Keep all lab inputs original & permissive: static `ET_EXEC` (`-no-pie`) musl C ELFs via `zig cc -target aarch64-linux-musl`, `#![no_std]` static Rust ELFs, and prebuilt clean-room `.nro` stubs (no `devkitA64` required on student machines).
2. **Core Intro Labs (5 Modules):**
   - **Lab 1 — AArch64 Registers & Control Flow:** Inspect `X0`/`D0` (and `NZCV` flags once `--dump-regs` lands on `run-elf`) and clean exit (`SVC`).
   - **Lab 2 — Stack Frames & Calling Convention:** AAPCS64 prologue/epilogue and memory-fault diagnosis (exit `11` / VA inspection).
   - **Lab 3 — Syscalls & Virtual Memory (`run-elf`):** `brk`, `mmap`, `mprotect`, `PROT_NONE` guard pages, and `--trace-syscalls`.
   - **Lab 4 — IEEE-754 Floating-Point & SIMD:** NaN propagation, rounding modes, and NEON vector ops.
   - **Lab 5 — Determinism & Clocks:** Why non-deterministic time breaks tests (`--deterministic`).

---

## Q2 (Feb – May 2027): Automated GitHub Classroom Grading via `ounce.run.v1`

1. **Zero-AI Autograder (`ounce-edu grade` / Python stdlib + `ounce-hb`):**
   - Grade `.elf` submissions via `ounce-core run-elf --report-json` (after Q2 wires `ounce.run.v1` into `run-elf`) and `.nro` submissions via `ounce-hb repro`, checking `stop_kind`, `exit_code`, stdout, and `insns_executed` complexity budgets (e.g., $O(n)$ vs $O(n^2)$ sorting/hashing).
   - Add DWARF `.debug_line` symbolication (in `ounce-core` or `ounce-edu`) so students get exact failing `file:line` + register state instead of cryptic hex dumps.
2. **GitHub Classroom Starter Template:**
   - Drop-in `.github/workflows/grade.yml` using `hffgzktbcc-blip/ounce-hb/.github/actions/setup-ounce@<pinned-tag>` (until `setup-ounce@v1` is published) that posts instant `$GITHUB_STEP_SUMMARY` pass/fail tables on every student push.

---

## Q3 (May – Aug 2027): Upper-Division Compiler/JIT & Graphics Labs

1. **Compiler & JIT Differential Lab (`ounce-core diverge`):**
   - Students implement a toy bytecode/JIT or optimization pass (`-O0` vs `-O2`/`-O3`) and compare execution across `interpreter` and `tier3` (plus `tier4` once release binaries enable `--features native-jit`) using `ounce-core diverge --json` (and `ounce.trace.v1`).
2. **Headless 2D Framebuffer Graphics Lab:**
   - Software rasterizer / sprite-blitter lab running headlessly (requires Q1/Q3 `--max-frames`, `framebuffer_hash` in `ounce.run.v1`, and `--dump-png` on `ounce-core run-nro`), graded automatically via `framebuffer_hash` (`fnv1a64:...`) with PNG render artifacts attached on mismatch.
3. **Lightweight VS Code Lab Pack:**
   - Preconfigured `.vscode/tasks.json` + `problemMatcher` + DevContainer so students click "Run & Grade Lab" locally or in GitHub Codespaces.

---

## Q4 (Aug – Nov 2027): Open-Access Courseware & `v1.0.0` Release

1. **Instructor Pack & Curriculum Freeze:**
   - Complete lecture notes, lab handouts, solution-branch autograder tests, and grading rubrics.
   - Include clear nominative-use disclaimer and zero-retail/clean-room pledge.
2. **Tag `ounce-education` `v1.0.0`:**
   - Co-release `v1.0.0` locked to the frozen `ounce-core` `v1.0.0` / `ounce-hb` `v1.0.0` / `ounce.run.v1` contracts (with an optional handheld demo note showing how to run Q3's `.nro` graphics lab via `deckstream`).
