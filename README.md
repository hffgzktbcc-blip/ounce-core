<div align="center">

# Ounce-Core checks how ARM64 programs behave

Ounce-Core runs supported ARM64 programs through three CPU engines, compares their results with references recorded on real ARM64 hardware, and helps developers investigate differences.

[![CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml/badge.svg)](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml)
[![Bounded verification evidence](https://img.shields.io/badge/verification-bounded_evidence-84b89e?style=flat-square)](docs/DETAILS.md#legacy-switch-regression-corpus)
[![MIT](https://img.shields.io/badge/License-MIT-84b89e?style=flat-square)](LICENSE)

[![Ounce-Core in 90 seconds: one program, three engines, one hardware reference. Click to watch the full video.](docs/media/ounce-core-hero.gif)](docs/media/ounce-core-main.mp4)

<sub>[Watch the full 90-second video](docs/media/ounce-core-main.mp4)</sub>

**[Try it in your browser](https://hffgzktbcc-blip.github.io/ounce-core/playground/)** · **[Latest published release](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest)**

</div>

## What it is

You ship a library built on x86. Will it give the same answer on ARM64? You may not own an ARM64 machine to find out.

Ounce-Core runs supported programs through three CPU engines and compares their outputs with results recorded on real ARM64 hardware. When results differ, its reports and bounded traces help developers find where the runs split. Ounce-Core is open source, written in Rust, and licensed under MIT. The [vision](docs/VISION.md) explains the aim.

**It is not only for homebrew.** The same engines run static Linux ARM64 programs (`ounce-core run-elf`): open-source C programs such as LZ4, xxHash, SQLite and Lua, and first-party workloads for sorting, hashing, big-number maths and floating point. Switch-style homebrew (`run-nro`) is one way in, not the only one. You can also use it to check published results: a [paper check](docs/verify/papers/README.md) rebuilds a paper's algorithm as an ARM64 program, runs it and compares the output with the numbers the paper prints.

## A real bug in one minute

**Lua finished running, but it gave the wrong answer.** In a recorded November 2026 test, `math.sin(1)` produced about `1.1585` instead of the expected `0.84147`. Three tested execution paths agreed with each other, but a separately built Apple Silicon reference exposed the mistake.

Ounce-Core's floating-point checks isolated the bug to two instructions. The maintainers fixed them and recorded a matching rerun. **[See the before-and-after, the real evidence and how to rerun it](docs/community/LUA_FP_BUG_WALKTHROUGH.md)**. You can read the case in your browser without installing anything.

## What it has done

- **It found real bugs.** In one week of running unmodified open-source programs (LZ4, xxHash, SQLite, Lua) against an Apple Silicon Mac, it found a prefetch instruction that corrupted registers, two floating-point instructions with the wrong sign (so `sin`, `cos` and `pow` returned wrong values), and many missing vector instructions. All were fixed, each with a test.
- **It checks real programs against hardware references.** Recorded tests of LZ4, xxHash, SQLite 3.50.4, Lua 5.4.7, and upstream C libraries (ibireme/yyjson, nothings/stb stb_truetype, facebook/zstd) produced matching output across the tested engines and native execution. **The 51-harness Linux ARM64 ELF matrix passed on the v0.10.0 tagged commit** ([CI evidence](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/44746386566/job/136322674982)). The first-frame corpus of <!-- corpus-headline -->50/50<!-- /corpus-headline --> outside programs passes its pixel checks on the inputs those checks cover. These results describe bounded tests, not general compatibility.
- **It reproduces published results.** Two [paper checks](docs/verify/papers/README.md) are in the repository: the Philox random number generator (the authors' known-answer vectors) and the Ogita-Rump-Oishi accurate summation paper (its error bounds and its Table 6.2, with the output identical to a native build). They show the numbers match, not that the papers' wider claims hold.
- **It is safe Rust.** The runtime in `src/` forbids `unsafe` code, and the compiler enforces it. The two small crates that need `unsafe` are kept apart, and CI checks the boundary.

**Limit:** this is a testing tool, not a fast emulator. It runs static Linux ARM64 programs and unencrypted homebrew; running arbitrary or dynamically linked Linux applications is not established. Instruction coverage is partial, and a match is evidence for the inputs tested, not proof ([limits](docs/LIMITS.md)).

## The Growing Family

<p align="center">
  <img src="docs/assets/family-banner.jpg" alt="The Ounce Project Family: Ounce-Core, Ounce-HB, Ounce-Runtime, and DeckStream" width="100%">
</p>

> *Built by curiosity. Tested by evidence. Powered by community.*

Four independent, open-source projects sharing a development philosophy, an evidence-first discipline, and an active community:

**Four independent projects. Choose the job you need done:**

- **[Ounce-Core](https://github.com/hffgzktbcc-blip/ounce-core)** checks supported ARM64 programs against recorded hardware references.
- **[Ounce-HB](https://github.com/hffgzktbcc-blip/ounce-hb)** runs your own open-source homebrew through an engine and explains why it stopped.
- **[Ounce-Runtime](https://github.com/hffgzktbcc-blip/ounce-runtime)** experiments with faster ARM64 execution, graphics and controller input.
- **[DeckStream](https://github.com/hffgzktbcc-blip/deckstream)** aims to launch PC games from your NAS/WebDAV storage on Steam Deck; an independent device run is still outstanding.

**[Choose the right Ounce project in one minute](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/FAMILY.md)** for the first task, limitations, licences and current-release links. These projects have separate evidence and release decisions. DeckStream is not an ARM64 engine, and GPL code is not copied into MIT siblings.

## Try it

Build, run the hello fixture, and write an `ounce.run.v1` report: [docs/QUICKSTART.md](docs/QUICKSTART.md).

**1. In your browser.** Step through ARM64 code one instruction at a time in the [playground](https://hffgzktbcc-blip.github.io/ounce-core/playground/). Nothing to install.

**2. On your machine.** You need Rust 1.89 or newer. On Debian or Ubuntu, first run `sudo apt-get install -y libasound2-dev libudev-dev`.

```bash
git clone https://github.com/hffgzktbcc-blip/ounce-core.git && cd ounce-core
cargo build --release
cargo run --release -- run tests/fixtures/nro/hello/hello.nro fixture
```

The last command runs a small test program and prints `result=0x00000EEE`. The default engine is Tier-3. Since v0.9.0 it can also write a machine-readable stop report: add `--present=headless --report-json=run.json` (or set `OUNCE_REPORT_JSON`). `run-elf` also writes this report as of v0.10.0. To check your own program against a host build, see [Add a program](docs/ADD_A_PROGRAM.md) or the [Community Verification Guide](docs/COMMUNITY_VERIFICATION_GUIDE.md). To find where a run first goes wrong, see the [diagnosis guide](docs/DIAGNOSIS_CLI.md).

**3. Help out.** Have an ARM64 Mac or Linux machine, plus devkitPro/devkitA64? Follow the [independent rerun instructions](docs/REPRODUCE.md): the differential test needs `--nro` (or valid `--nro-dir` inputs) to actually compare guest engines. **Without guest binaries it can report skipped engine checks**, not a reproduction. Share matching, failing and blocked results in [issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248). No ARM64 machine? Try a [browser-only task](docs/community/TRY_THIS_WEEK.md) or a [good first issue](https://github.com/hffgzktbcc-blip/ounce-core/contribute). Read [CONTRIBUTING.md](CONTRIBUTING.md) before contributing.

## More

- [Quick start](docs/QUICKSTART.md): build, hello fixture, `--report-json`
- [Details](docs/DETAILS.md): subsystem status, how the proof works, test harness, full commands, roadmap, repository layout
- [Reproduce the results](docs/REPRODUCE.md) and [what they do not show](docs/LIMITS.md)
- [Research paper](site/research.html) and [vision](docs/VISION.md)
- [Mac app](docs/MAC_APP.md) and [interactive lab](docs/INTERACTIVE_LAB.md)
- [Sources, provenance and legal notice](docs/DETAILS.md#responsible-research-and-legal)
- Came here for the Switch emulator? [What changed and why](https://github.com/hffgzktbcc-blip/ounce-core/discussions/330)

Ounce-Core is not affiliated with Nintendo or NVIDIA. It runs open-source, homebrew and synthetic programs you provide; it is not designed to load commercial games, firmware or keys. The only source of this project is this repository, so build from source. Licensed under [MIT](LICENSE).
