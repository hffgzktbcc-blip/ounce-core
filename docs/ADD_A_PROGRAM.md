# Add a program as a differential test

The most useful thing you can do for Ounce-Core is run a real program in it and compare the result with a reference. That is how it found a prefetch instruction being decoded as a load, a floating-point instruction with the wrong sign, and several missing vector instructions. It takes an afternoon and needs no console, firmware or game files.

**The idea.** Compile the same C test twice: once for your computer (the reference) and once as a Switch program (an NRO). Run the NRO in Ounce-Core. If the text output differs, there is a bug, and the first differing line says where to look.

## You need
- A Rust toolchain and this repository (`cargo build --release`).
- devkitPro with `devkitA64` and `libnx` (https://devkitpro.org). It builds the NRO.
- An ARM64 computer (Apple Silicon or Linux ARM) if you want to test native execution; any computer works for the reference.
- An open-source program whose licence permits this, pinned to a commit or release.

## Steps
1. **Pick the program.** Small and deterministic is best: a compression or hash library, a JSON parser, an interpreter. See `docs/verify/` for finished examples (LZ4, xxHash, SQLite, Lua).
2. **Copy the template.** `cp -r docs/verify/_template docs/verify/<name>-harness` and put the program's sources next to `cases.c`. Do not commit them; record the pinned commit in the README.
3. **Write `cases.c`.** Call the program on varied inputs (empty, tiny, large, malformed, edge values). Print results as text. Fix any random seed. Print nothing that varies between runs.
4. **Build the reference.** `cc -O1 -o ref host/host_main.c cases.c <program sources>` then `./ref > expected.txt`. Run it twice and check the output is identical. If the program uses vector code, also build once with it disabled and check the two agree, so the reference is trustworthy.
5. **Build the NRO.** Start from devkitPro's `examples/switch/templates/application` Makefile, set `SOURCES := .`, add `-lm` to `LIBS` if you use math, and run `make`. If the link fails on missing system calls (for example `fchown`), add a small stub file; an in-memory test never reaches them.
6. **Run it on each engine** and compare:
   ```sh
   mkdir out && OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro name.nro --present=headless --sdmc=$PWD/out --cpu=tier3
   cmp out/results.txt expected.txt
   ```
   Repeat with `--cpu=tier4` and `--cpu=native` (build with `--features "nce native-jit"`; native needs an ARM64 host).
7. **If it differs,** the log shows `[VM Error] Unimplemented instruction 0x...` for a missing instruction (the number is the opcode; `aarch64-none-elf-objdump -d` on the `.elf` shows the instruction). A silent difference with no error is a wrong result: bisect by writing a tiny test of one instruction with inline assembly and running it on both the host and Ounce-Core, as `docs/verify/fp-instructions` does.
8. **Fix it and add a unit test.** Decoder tests are in `src/cpu/decoder.rs`, semantics tests in `src/cpu/simd.rs` and `src/cpu/interpreter.rs`. Run `scripts/gate.sh`.
9. **Open a PR.** Include the harness, `expected.txt`, `observed-ounce-core.txt`, the pinned version and licence, the result per engine, and what the test does not cover.

## Rules
Commit no keys, firmware, retail software or dumps, and no third-party source you are not allowed to redistribute. Test inputs must be synthetic or from the program's own test data. See [CONTRIBUTING.md](../CONTRIBUTING.md) and [docs/CLEAN_ROOM.md](CLEAN_ROOM.md).
