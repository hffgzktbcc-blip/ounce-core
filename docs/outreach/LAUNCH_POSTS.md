# Launch posts (drafts for the owner to publish)

Rules: post as yourself, say you are the author, lead with the result not the project, ask for one specific thing (run it on your library, or review the method), answer every reply, and do not ask for stars. Do not post the same text in several places on the same day. Use real numbers only; every number below is in `docs/verify/`.

## Show HN
**Title:** Show HN: Ounce-Core, an ARM64 emulator that checks itself against real hardware
**Text:**
I build Ounce-Core, a safe-Rust ARM64 execution lab. This week I used it differently: build an open-source program twice (normal computer and ARM64), run both, diff the output. The normal build runs on real ARM64 hardware (an Apple Silicon Mac or a Linux ARM64 box), so it is a hardware reference. Running LZ4, xxHash, SQLite, Lua, zlib, cJSON and Monocypher this way found about a dozen emulator bugs, including a prefetch instruction that silently overwrote registers and two floating-point instructions that made sin/cos/pow wrong on every engine. Everything matches on all three engines now. There is also a local workbench (`ounce-core ui`): drop a homebrew `.nro` and it runs it on Tier-3 and Tier-4, optionally on your own CPU too, and names the first line where they differ.
Limits: coverage is partial, it is not fast, nobody outside the project has reproduced it yet, and running arbitrary Linux apps is not something it can do. I would like a library maintainer to try the template on their code and tell me where the guide breaks. https://github.com/hffgzktbcc-blip/ounce-core

## r/rust
**Title:** I diffed my safe-Rust ARM64 emulator against real hardware and found a dozen bugs
**Text:**
Flair: project. I am the author. Ounce-Core is an ARM64 emulator in safe Rust, with a CI guard that fails the build if `unsafe` appears outside one small, named boundary. To check it I build the same C program for the host and for the emulator and diff the output byte for byte. Doing that with LZ4, xxHash, SQLite, Lua, zlib, cJSON and Monocypher found about a dozen bugs, including two floating-point instructions that made `sin(1)` wrong on every engine, which engine-vs-engine comparison could never have caught.
What I would like from this sub: a review of the differential method (`docs/verify/`, `docs/REPRODUCE.md`) and of how the `unsafe` boundary check works (`scripts/check_unsafe_boundary.sh`). Limits: partial coverage, not fast, not reproduced by anyone outside the project yet. https://github.com/hffgzktbcc-blip/ounce-core

## r/EmuDev and the EmuDev Discord
**Title:** Differential-testing an AArch64 emulator against real silicon: what it found
**Text:**
I am the author. I run the same C program natively and in the emulator and compare output. It found a PRFM decoded as a load that clobbered x0, and two fused multiply instructions with the wrong sign. The harness sources and per-engine results are in `docs/verify/`.
Ask: if you have a program that behaves oddly on an ARM64 emulator, send it or open an issue with the binary and what you expected. Limits: partial coverage, slow, no claim of general correctness. https://github.com/hffgzktbcc-blip/ounce-core

## Rust Users forum (users.rust-lang.org)
Category: "Code Review". Ask one question: is the `unsafe` boundary guard (`scripts/check_unsafe_boundary.sh`, tracked files only via `git grep`) a sound way to enforce "safe Rust except one named module"? Link the script, not the project.

## Mastodon / Bluesky / X thread (5 posts)
1. Differential testing found 12 bugs in my ARM64 emulator in a week. Method: build a real program twice, once for the host (real ARM64 on a Mac), once for the emulator, and diff the output.
2. Worst one: `sin(1)` returned 1.1585. Two fused-multiply instructions had the wrong sign. All three engines agreed with each other, so engine-vs-engine comparison could never find it.
3. Second: a prefetch instruction decoded as a load, silently overwriting x0. A hash library crashed with no explanation.
4. Everything now matches on LZ4, xxHash, SQLite, Lua and more. Limits: partial coverage, slow, nobody outside the project has reproduced it yet.
5. If you maintain a C/C++ library, the guide takes an afternoon: github.com/hffgzktbcc-blip/ounce-core (docs/ADD_A_PROGRAM.md). Tags: #rustlang #arm64 #emulation #testing

## LinkedIn
A short, plain version: the problem (emulators hide wrong answers), the method (diff against real hardware), two concrete bugs, the limits, and one ask (maintainers: try your library).

## Where else (one at a time, after the first reply cycle)
- This Week in Rust: submit the blog post as a PR to `rust-lang/this-week-in-rust`, and add the harness issues under "Call for Participation".
- up-for-grabs.net and goodfirstissue.dev: they list repos by issue label. Our `Good first issue` and `Help wanted` issues already exist; list the repo there.
- r/ARM: the method, not the repository.
- awesome-rust: open a PR only once an outside user exists.
- Projects that care about emulation correctness (QEMU, Unicorn, FEX, box64): do not advertise; open an issue only if a result concerns them, with a reproduction.

## What not to do
Do not claim Switch or Switch 2 compatibility, do not call it a QEMU replacement, do not quote a speed, do not count stars, forks or views as adoption, and do not post anything that includes keys, firmware or retail content.
