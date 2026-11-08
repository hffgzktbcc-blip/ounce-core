# We diffed an ARM64 emulator against an Apple Silicon Mac for a week. It found a dozen bugs.

*Draft for the project owner to publish (blog, dev.to, or the repository Discussions). Edit freely; every claim below is backed by a merged or open PR linked from `docs/verify/`.*

Emulators usually get tested the way you test any big program: run things, see what breaks. The bugs that matter are the quiet ones, where the program runs and prints the wrong answer.

We tried something simpler. Take a real open-source program (LZ4, xxHash, SQLite, Lua, zlib, cJSON, Monocypher). Build it twice from the same pinned source: once for a normal computer, once for ARM64. Run both, print the results as text, and diff. The normal-computer build runs on an Apple Silicon Mac, which is real ARM64, so it is a trusted reference for what the instructions should do. Then run the ARM64 build in Ounce-Core on three engines: an interpreter, a JIT and native host execution.

## What it found

- **A prefetch instruction was being decoded as a load.** `prfm` has a field that looks like a destination register and is really the prefetch type, usually zero. The emulator treated it as "load into x0". Every prefetch silently overwrote a register. xxHash crashed on a corrupted pointer; nothing said why.
- **`sin(1)` returned 1.1585.** Two fused floating-point instructions (`fnmadd`, `fnmsub`) negated the wrong operand. The standard maths library uses them, so `sin`, `cos` and `pow` returned wrong numbers on every engine. All three engines agreed with each other, so comparing engines would never have caught it. Only comparing with hardware did. A 5,964-line test of floating-point instructions pinned it to those two.
- **A long tail of missing vector instructions:** widening multiply and add, byte reverse, register shifts, table lookup, polynomial multiply, narrowing and saturating operations, CRC32, scalar add and duplicate forms. Each one stopped a real program with its opcode.
- **A hard cap of 50 million instructions** on every run, which SQLite exceeds.

Each fix has a unit test. The programs that failed now match the reference byte for byte on all three engines.

## What it does not show

- It checks the inputs the tests use, not that a program or the emulator is correct everywhere.
- The native engine runs on the host CPU, so on an ARM64 machine it tests the harness, not the emulator. The interpreter and JIT are the real evidence.
- Coverage is partial, and until this week the saturation flag was not modelled at all.
- No one outside the project has repeated these runs yet. That is the next milestone, and the reason for this post.

## Try it on your library

If you maintain a C or C++ library, the guide at `docs/ADD_A_PROGRAM.md` walks through adding it as a test in an afternoon, with a template. You do not need ARM hardware. If it finds nothing, that is a result too; if it finds a gap, you get a small reproduction.

Repository: https://github.com/hffgzktbcc-blip/ounce-core
