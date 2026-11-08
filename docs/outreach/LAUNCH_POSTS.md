# Launch posts (drafts for the owner to publish)

Rules: post as yourself, say you are the author, lead with the result not the project, ask for one specific thing (run it on your library, or review the method), answer every reply, and do not ask for stars. Do not post the same text in several places on the same day. Use real numbers only; every number below is in `docs/verify/`.

## Show HN
**Title:** Show HN: Ounce-Core, an ARM64 emulator that checks itself against real hardware
**Text:**
I build Ounce-Core, a safe-Rust ARM64 execution lab. This week I used it differently: build an open-source program twice (normal computer and ARM64), run both, diff the output. The normal build runs on an Apple Silicon Mac, so it is a real-hardware reference. Running LZ4, xxHash, SQLite, Lua, zlib, cJSON and Monocypher this way found about a dozen emulator bugs, including a prefetch instruction that silently overwrote registers and two floating-point instructions that made sin/cos/pow wrong on every engine. Everything matches on all three engines now.
Limits: coverage is partial, it is not fast (about 20M instructions/s), nobody outside the project has reproduced it yet, and running arbitrary Linux apps is not something it can do. I would like a library maintainer to try the template on their code and tell me where the guide breaks. https://github.com/hffgzktbcc-blip/ounce-core

## r/rust
**Title:** I diffed my ARM64 emulator (safe Rust, `forbid(unsafe_code)`) against an Apple Silicon Mac and found a dozen bugs
Short version of the blog post; mention it is safe Rust, the three engines, and ask for review of the differential method and the CI plan (an instruction fuzzer on an ARM64 runner).

## Mastodon / Bluesky / X thread (5 posts)
1. Differential testing found 12 bugs in my ARM64 emulator in a week. Method: build a real program twice, once for the host (real ARM64 on a Mac), once for the emulator, and diff the output.
2. Worst one: `sin(1)` returned 1.1585. Two fused-multiply instructions had the wrong sign. All three engines agreed with each other, so engine-vs-engine comparison could never find it.
3. Second: a prefetch instruction decoded as a load, silently overwriting x0. A hash library crashed with no explanation.
4. Everything now matches on LZ4, xxHash, SQLite, Lua and more. Limits: partial coverage, slow, nobody outside the project has reproduced it yet.
5. If you maintain a C/C++ library, the guide takes an afternoon: github.com/hffgzktbcc-blip/ounce-core (docs/ADD_A_PROGRAM.md). Tags: #rustlang #arm64 #emulation #testing

## LinkedIn
A short, plain version: the problem (emulators hide wrong answers), the method (diff against real hardware), two concrete bugs, the limits, and one ask (maintainers: try your library).

## Where else (one at a time, after the first reply cycle)
- This Week in Rust: submit the blog post.
- Rust Users forum and r/ARM: the method, not the repository.
- awesome-rust: open a PR only once an outside user exists.
- Projects that care about emulation correctness (QEMU, Unicorn, FEX, box64): do not advertise; open an issue only if a result concerns them, with a reproduction.

## What not to do
Do not claim Switch or Switch 2 compatibility, do not call it a QEMU replacement, do not quote a speed, do not count stars, forks or views as adoption, and do not post anything that includes keys, firmware or retail content.
