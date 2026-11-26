> **Provenance:** an external audit run by the Grok AI agent and supplied by the project owner on 25 Nov 2026. It audited `main` at `2355b1e`; its Big Tech section reran on `94966ba`. It is reproduced below exactly as received. It is an AI agent's audit, not an independent human reproduction (see item 35). Changes made in response: #571 (versioning policy), #572 (claims and evidence table), #573 (stale docs and CLI text), and later PRs that cite this file.

# ounce-core audit handoff (25 Nov 2026)

Audited: `main` at `2355b1e` (x86 Linux box + Apple M3 MacBook). Main has since moved to `94966ba` (#569 merged); re-check line numbers before editing.
All audits were read-only. Verdict summary: **every test that could be rerun matched (0 mismatches)**. The problems are the paper trail, overstated wording, stale docs, and a few emulator behaviours that guess instead of failing.

## What was verified (for context, no action needed)
- 102/102 paper checks on interpreter, Tier-3, Tier-4; reference data matches upstream sources (e.g. 8,322 xxHash64 values); deliberately broken builds fail as documented.
- fp-edge: 389,052 matches, 0 value/flag mismatches, all 9 FPCR modes, 3 engines (x86 vs qemu and on M3).
- TestFloat f32/f64: 2,892,387 cases, 0 mismatches (f16 groups have no committed script).
- libmpdec 148/148, Paranoia "Excellent!", Csmith sample 223/242 identical (19 timeouts, 0 differ), 13/51 Linux CI programs identical, simdjson, BLAKE3 test vectors, libc-test math 198/199.
- M3: nightly fuzzer 383,733 words compared vs hardware, 0 mismatches/flag diffs on all engines; FP16 #558-class stand-in 600,000 cases 0 mismatches; FMA NaN-priority 156,141 cases 0 mismatches; f16 FMOV/fixed-point 137,983 cases 0 mismatches; fp-workload 211/211.
- cargo test: 838 pass (default), 873 (native-jit, x86), 877 (native-jit,nce, M3).
- Tier-4 (Cranelift) also works on x86, ~3x Tier-3.

## P0: Claims and paper trail (docs/scripts only)
1. **Commit scripts, seeds, versions and logs for every headline suite**, or mark them "not reproducible from repo". Missing: physics/quantum 9/9, Shewchuk 90,000, YARPGen, gcc torture, CompCert, PolyBench, MicroPython, QuickJS 5/5, libpng, libdeflate, bzip2, brotli, PCRE2, Csmith 2,815 run, TestFloat f16 groups and full 5.92M run, 144/144 FZ/FZ16/DN, 908/908 f16, #558 595,472-case probe, bigtech test drivers (BLAKE3 822, simdjson 6/6). Locations: README.md:37-40, CHANGELOG.md:16,28-30, docs/releases/v0.11.0.md:12,38-40, docs/releases/v0.12.0.md:13.
2. **Rerun or relabel numbers measured on `baec66b`** (before #559 LSE atomics fix). README.md:38.
3. **"Compared with references recorded on real ARM64 hardware"** overstated: README.md:5, :23, docs/LIMITS.md:12. Say which suites use qemu-aarch64, native host builds, or Apple M3 (true for fuzzer, fp-edge, fp-workload).
4. **README.md:27 safety/determinism line**: `forbid(unsafe_code)` holds for `src/` only; Tier-4 (`ounce-recompiler` exec.rs) and native (`ounce-nce`) use unsafe. Determinism requires `--deterministic` (default uses host realtime/monotonic clocks).
5. **README.md:27 "native hardware" cross-engine comparison**: native covers NRO only; `run-elf --cpu=native` exits 72 by design (src/linux/runner.rs:248-250). Say so.
6. **"Tier-3 JIT" naming**: it is a decoded-instruction cache that steps and falls back to `cpu.step()` (src/cpu/tier3_jit.rs:94,181,221; tier2_jit.rs:207; DETAILS.md:82). ~13% over interpreter on simdjson. Rename (e.g. "block-cache interpreter"); Tier-4 is the JIT.
7. **Engine independence**: engines share one decoder and Tier-3/4 fall back to the interpreter. State that cross-engine agreement is a regression check, not independent evidence.
8. **Paper checks scope**: ~79/102 are integer known-answer tests (ciphers/hashes/RNG/encodings), 23 FP; several cite OEIS/Crypto++ rather than the paper; built once at clang -O1, run on Tier-3 only, not in CI. Reword "reproduces published results" and add to CI.
9. **Physics/quantum wording**: these are classical C simulations of Grover/QFT etc. Make clear no quantum hardware is involved, and commit the sources.

## P1: Stale or contradictory docs
10. FPSR flags: docs/LIMITS.md:11 says not modelled; they are (DETAILS.md:351, CHANGELOG.md:318, interpreter.rs:939; verified). Fix LIMITS.
11. docs/LIMITS.md:24 stamped `14b3c4d`; :20 "roughly 20 MIPS" (measured ~28 interp / ~84 Tier-3 best case). Restamp.
12. docs/verify/bigtech-2026-11-23.md:66 lists LDXP/STXP and HWCAP_ATOMICS as gaps; fixed by #564.
13. docs/verify/insn-fuzzer/README.md:129,134,135 say FP16 missing, FZ16 not fuzzed, FPSR not modelled (all now false); :95,:114 say 8 FPCR modes, fuzz_host.c:401 uses 10.
14. docs/verify/fp-edge/README.md:41 says FZ16/half not modelled (stale).
15. CHANGELOG still says HWCAP_ATOMICS not advertised; duplicate bullet CHANGELOG.md:321 and :366.
16. Archived-repo references: CONTRACT_V1.md, OUNCE_RUN_V1_SCHEMA.md, src/trace_summary.rs:10, src/run_report.rs:88 still name ounce-hb / ounce-runtime as owners.
17. Add SVE, SME, MTE, threads/futex, memory model as explicit non-goals in LIMITS.md.
18. `ounce-core info` prints a console-chip "octa-core preemptive multi-core scheduler" banner; contradicts single-threaded runner and "testing tool" framing. Tone down.
19. Help text calls tier3 "reference interpreter". Fix.
20. Define "0 real mismatches", "bounded-verified", "paper checks".

## P2: Emulator behaviour (code)
21. **Unknown MRS returns 0, unknown MSR dropped** (src/cpu/interpreter.rs:963, :974). Deliver SIGILL instead. M3 comparison:
    - MIDR_EL1 / MPIDR / ID_AA64ISAR0/ISAR1/PFR0: emulator 0 (contradicts HWCAP advertising LSE/CRC32/SHA3/PMULL). Give real values consistent with HWCAP (Linux exposes these via MRS emulation).
    - CTR_EL0: emulator 0 (Linux userland may read it for cache-line size; returns 4-byte lines). Return a sane value (e.g. M3-like).
    - CNTVCT_EL0: emulator stays 0, so busy-waits hang. Make it advance (deterministic under `--deterministic`).
    - CNTFRQ_EL0: fixed 19.2 MHz vs M3 1 GHz. Document or make consistent with CNTVCT.
    - `MSR SCTLR_EL1` at EL0 succeeds; should SIGILL.
22. **UDF / unimplemented instructions abort the emulator** (exit 71) instead of delivering SIGILL to the guest; guest SIGILL handlers never run.
23. **Misaligned LDXR** allowed; hardware raises SIGBUS. Exclusive monitor: single address, no size, plain store doesn't clear it.
24. **ASIMD gaps**: fuzzer still records unimplemented valid FP/vector encodings (18,796 words / 329 classes in a 1M M3 run; 359k in earlier counts). Prioritise compiler-emitted encodings.
25. **Silent fallbacks**:
    - `--cpu=tier4` in default build silently runs Tier-3.
    - native without `nce` feature silently runs interpreter and exits 0 (src/cli/run.rs:282).
    - unknown `--cpu` value (e.g. `interp`) ignored, default engine runs, exit 0 (src/lib.rs:334); fuzz-replay accepts `interp`, run doesn't.
    - `silent_fallback` field in `ounce.run.v1` is always false. Make fallbacks fail or be reported.
26. tools: `fp-workload/run.sh` rebuilds with only `--features native-jit`, dropping `nce` (triggers #25). Tier-4 "ran natively" count means different things in engine-vs-engine vs host-replay mode; label it.
27. CPU-time clocks return EINVAL; deterministic clock epoch (realtime 1700000000, mono 0.001) undocumented.

## P3: CI, quality, maturity
28. CI on PRs runs only 3 of the 51 Linux programs; paper checks not in CI; Rust test/Tier-4/native jobs are skipped by the change classifier when a PR touches only release/docs files (e.g. #569). Consider a weekly full run publishing logs.
29. Hardware fuzzer: FP/vector only, default-ish FPCR, compares to interpreter. Widen to integer, load/store, system instructions; compare Tier-3/Tier-4 directly to hardware.
30. 20 core files disable dead-code warnings file-wide (55 allows), weakening `clippy -D warnings`.
31. "Headless" default build still links minifb and loads X11/Wayland/EGL at runtime.
32. Run report schema `ounce.run.v1` leaks Switch-specific fields into Linux reports.
33. Release cadence: 15 tags in a month (v0.11/v0.12 ~14h apart), `publish = false`, no semver policy. Add a short versioning policy.
34. `tools/testfloat/gen_trace.py` default `--count 0` exhausts memory (~14 GB); set a sane default.
35. Bus factor/outside reproduction: get one external person to reproduce a headline number before public launch posts.

## Bigtech vs native M3 (25 Nov, main 94966ba, Zig 0.17.1 aarch64-linux-musl static; own drivers since originals aren't committed)
- simdjson: 6/6 identical to native on interpreter, Tier-3, Tier-4 at -O2 and -O3 (4 upstream examples + 2 synthetic).
- libwebp: 50/50 lines identical to native on all three engines at -O2 (17.38G instructions; 463s / 999s / 1345s) and Tier-4 at -O3 (227s). Driver prints 50 lines, doc claims 57; -O3 Tier-3/interpreter interrupted.
- secp256k1: Tier-4 -O2 119/119, rc=0, ~65 min (~18 MIPS). Matches the doc's Tier-4 row.
- SGP4 (699-line driver): all 6 engine/opt outputs byte-identical. vs native with the same musl libm: 673/699 identical, 26 differ by 1-6 ulp (max 4 with -ffp-contract=off), likely Apple clang 21 vs zig clang 22. Native musl libm vs macOS libm alone: 161 lines differ, so the doc's 962/1,222 gap is mostly host libm, not the emulator. **Doc action:** reword the SGP4 row to say this.
- **Doc action:** commit the bigtech drivers/build scripts (counts like BLAKE3 822 and libwebp 57 aren't reproducible without them).

## Not verified
- Stopped at user's request (MacBook rebooted under parallel load): secp256k1 on Tier-3/interpreter and -O3, libwebp -O3 Tier-3/interpreter, s2n-bignum (13 FEAT_AES skips), BLAKE3 vs native/official vectors.
- 50/50 first-frame NRO oracle corpus: needs external collection and devkit container.
- Workspace-crate tests on M3.
