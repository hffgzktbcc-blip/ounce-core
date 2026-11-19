# Paper checks

A paper check takes a published result, rebuilds it as a small ARM64 program, runs it on ounce-core and compares the output
with the numbers the paper or its published test data give. It shows that a paper's numbers come out of its method, to the precision each check states: exactly for known-answer vectors,
within a declared tolerance for a results table, and the same on every run.
It does **not** show that the paper's wider claims are right: a wrong method reproduces wrong numbers just as exactly.

This is one way ounce-core is used beyond homebrew. The programs are plain static Linux ARM64 ELF files, run with
`ounce-core run-elf`. Nothing here needs a console, firmware or keys.

## What is here

| Check | Paper | What is reproduced | Run time |
|---|---|---|---|
| [`philox/`](philox/) | Salmon, Moraes, Dror, Shaw, "Parallel random numbers: as easy as 1, 2, 3" (SC'11) | The three published known-answer vectors for Philox4x32-10, plus a statistical smoke battery (not the paper's BigCrush claim) | about 18 s |
| [`accurate-sum/`](accurate-sum/) | Ogita, Rump, Oishi, "Accurate Sum and Dot Product" (SIAM J. Sci. Comput. 2005) | Theorem 3.4 and Propositions 4.5, 4.10 and 5.5 on 240 generated cases, and Table 6.2 within 1.0 digit | about 23 s |

Each folder has a `paper.json` (what is claimed, where the expected numbers come from, what is **not** checked), the program,
a `check.sh` that builds and runs it, and the expected output. Run one from the repository root:

```sh
cargo build --release --locked
docs/verify/papers/philox/check.sh /tmp/philox-check target/release/ounce-core
```

The output folder (`/tmp/philox-check` here) must not exist yet: `check.sh` refuses any path that does.

`check.sh` needs `clang` and `ld.lld` (or a Rust toolchain, which has `rust-lld`). If a host C compiler is present it also
tries to build the same source natively and requires identical output, so ounce-core's arithmetic is compared with the machine
it runs on. For Philox, only the statistical smoke program (`smoke.c`) is compared this way; the known-answer program
(`philox4x32.c`) is checked against the published vectors only. If that native build fails, the check prints "not compared" and can still pass, so read the last line: only
"identical to a native ... build" means the comparison happened. Both checks were run on x86-64 and Tier-3 only; Tier-4 and native ARM64 runs are not done yet.

## How to add one

1. **Pick a paper with numbers you can compare**: a table, known-answer vectors, or a bound a program can test. Read the paper
   text, not a summary, and take algorithms and numbers from it.
2. **Write the program yourself from the algorithm.** Do not copy code. Published numbers are data and may be copied as
   numbers; say where they came from. Do not vendor GPL code (for example TestU01).
3. **Fix the tolerance before the first run** and write it in `paper.json`. If the result is outside it, report that. Do not
   move the tolerance to fit.
4. **Add a negative control**: break one constant and show the check fails. A check that cannot fail proves nothing.
5. **List what is not checked** in `paper.json`: speed, wider claims, other variants. Emulator run time says nothing about the
   authors' hardware.
6. **Keep it deterministic**: fixed random seeds, no clock, no host files. The output must be the same on every run.
7. Record the output as `expected.txt` and require it byte for byte in `check.sh`.

## What a pass means

The program, run on ounce-core, produces the paper's numbers. Evidence for the inputs tested, not proof for every input and
not an independent reproduction by someone else.
