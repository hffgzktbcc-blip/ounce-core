# Capability scope

Owner decision (2026-11-11): ounce-core is a homebrew and ARM64 execution, diagnosis and
verification platform. The current tree does not contain key handling, encrypted-container
support, firmware or system-title loading, or retail-console framing. This is a cleanup of the
current tree only. It is not a provenance re-audit and it does not rewrite history; see
[PROVENANCE.md](PROVENANCE.md) for history.

**Supported inputs:** homebrew `.nro` and `.nso` modules, raw AArch64 images, static AArch64 Linux
ELF files, unencrypted loose `exefs/` + `romfs/` directories (OunceCake format), NPDM metadata and
RomFS images.

**Guardrail:** `scripts/check_legal_guardrails.sh` (CI job "Legal guardrails") fails if key-file
parsing, NCA/XCI/NSP/HFS0 decryption or loading code, or key-name tables reappear in `src/`,
`crates/`, `tools/`, `scripts/`, `site/`, `assets/`, `tests/` or `.github/`. It is grep-based, so
it catches the obvious cases and does not replace review.

## Removed

| Path | What it did | Decision |
| :--- | :--- | :--- |
| `src/crypto/mod.rs` | AES-128 ECB/CTR/XTS engine, `KeySet` with `parse_prod_keys` (prod.keys / title.keys parsing, key-name tables) | Removed. Only `main.rs` and `loader/nca.rs` used it. |
| `src/loader/nca.rs` | `NcaImage` (NCA3 header and section decryption, `SYNTHETIC_TEST_KEY`), `Pfs0Archive` (NSP/ExeFS), `Hfs0Archive`, `XciCartridge` (game card), encrypted-package paths in `BootLoader` | Removed. The file became `src/loader/boot.rs` and keeps only NRO0/NSO0/raw AArch64 loading. |
| `src/loader/nro.rs` `LoadError::RestrictedCipher` | Error variant for the retail-key policy | Removed with the code that raised it. |
| `src/loader/boot.rs` tests (was `nca.rs` tests) | NSP/NCA AES-CTR pipeline, XCI boot, NCA multi-section RomFS, retail-key rejection | Deleted with the features. Two tests of the remaining loader were added. |
| `src/loader/fuzz_tests.rs` `fuzz_containers` | Fuzzed PFS0/HFS0/XCI/NCA parsers | Deleted with the parsers. NRO, NSO, ELF, MOD0, NPDM and RomFS fuzzers are unchanged. |
| `src/ui/inspector.rs` | `inspect` for `.xci`/`.nsp`/`.nca`, title-ID and decrypted RomFS listing | Rewritten as a `.nro`/`.nso` inspector (format, code size, PAC/BTI scan). |
| `src/main.rs` `package` / `showcase` command and `build_and_boot_showcase_nsp` | Built and booted an encrypted "Switch 2 showcase" `.nsp` | Removed, including the usage line. |
| `src/main.rs` stages 6, 7, 14, 15 of the `test` harness | Wrapped guest code in encrypted NSP/XCI/NCA containers | Same guest programs and expected results; they now load through `BootLoader` as raw images or NSO0, and stage 15 mounts the synthetic RomFS directly. Stage numbers and expected values are unchanged. |
| CLI help, `run`/`inspect` prompts, `info` text | Listed `.xci/.nsp/.nca`, "Decryption Engine", "GameCard Media", "Digital Packages", "Zero-Key Engine ... prod.keys" | Reworded to homebrew framing. |
| `README.md`, `CONTRIBUTING.md`, `docs/CLEAN_ROOM.md`, `docs/CODEX_HANDOFF.md`, `docs/architecture/layers.json`, `docs/INTERACTIVE_LAB.md` | Described PFS0/HFS0/NCA3 support, the crypto layer, `RestrictedCipher`, `loader/nca.rs`, the prod.keys-format parser, `./dumps/title` examples | Reworded or removed. |
| `site/lab.html` | Accepted and described XCI/NSP files ("retail decryption disabled by policy") | XCI detection branch and wording removed. |
| `assets/architecture.svg` | Diagram of GameCard -> HFS0/PFS0 -> NCA3 (AES-128-CTR) | Relabelled to homebrew formats. |

## Kept (reviewed, not circumvention-related)

| Path | What it is | Decision |
| :--- | :--- | :--- |
| `.gitignore` (`*.keys`, `*.nsp`, `*.xci`, `*.nca`) | Prevents committing user files | Keep. |
| `scripts/check_legal_guardrails.sh` | Fails on key/container files and ROM-site links; now also on key-parsing and decryption code | Keep and extend. |
| `README.md` line 34 ("Came here for the Switch emulator? ...") | Points to the discussion explaining that the project is not an emulator | Keep. It disclaims the framing. |
| `site/press.html`, `site/index.html`, `site/beyond-first-frame.html` | Say the project must not be described as a Switch/Switch 2 emulator and that no compatibility is claimed | Keep. |
| `src/fs/romfs.rs` | Generic RomFS image parser/builder used by homebrew and loose directories | Keep. |
| `src/loader/package.rs` | Loose directory + NPDM loader (unencrypted) | Keep. |
| `tests/fixtures/nro/**`, `docs/verify/**`, corpus | Homebrew and independent test programs; third-party AES test code in `docs/verify/*-aes-*` is an ordinary crypto library under test | Keep. |
| Responsible-use and "no retail content" notices in README, CONTRIBUTING, issue templates | Tell contributors not to submit keys, firmware or dumps | Keep. |

## Owner decision

These were not removed because they are ambiguous, tied to provenance, or would change tested
behavior.

| Path | What it does | Why it is listed |
| :--- | :--- | :--- |
| `src/kernel/set.rs` (`firmware_version`), `src/kernel/pl.rs`, `src/kernel/applet.rs`, `src/kernel/sdk_hle.rs` | Synthetic HLE responses (system version string, shared-font memory block, applet state) that homebrew queries | Not a firmware loader and no firmware bytes; homebrew needs these services. `pl.rs` mentions real firmware only in comments. Confirm the wording is acceptable. |
| `src/kernel/svc.rs` `FS_QUERY_PAYLOAD`, stage 10 "Native FS Payload", stage 11 `"NS2ROEXE"` reply | Hand-written test guest programs and a synthetic IPC reply tag | Names read as exploit-adjacent. Renaming changes the harness output that existing oracles and docs reference, so it was left alone. |
| `docs/permissions/NS2JB_REQUEST_DRAFT.md` | Draft permission request about material from a third-party project with "jb" in its name | Provenance item (see PROVENANCE.md). Decide whether to keep, rename or delete. |
| `docs/PROVENANCE.md`, `docs/LICENSE_DISPOSITION.md`, `docs/E2_DECISION_MEMO.md`, `docs/history/**` | Historical record of earlier firmware-labelled byte arrays and a removed external-payload loader | Historical records, outside this cleanup. They intentionally name the old items. |
| `docs/HANDOFF_HISTORY.md`, `docs/media/cli-demo.cast` | Past notes (mention PFS0/HFS0/XCI/NCA fuzzing; older CLI output) | History or recorded media. The recording only shows current help text from before this change; re-record if the owner wants it refreshed. |
| `docs/verify/*/Makefile` (`.nsp` targets, `--titleid`) | Stock devkitPro makefile templates for building homebrew test programs | Third-party template text. Not loaded by ounce-core. |
| Synthetic Switch 2 / "Joy-Con 2" / "T239" wording in `src/main.rs` demos and `info` | Describes the modelled hardware | Not retail material, but it is Switch 2 framing. Owner to say whether to keep. |
| Title IDs such as `0x0100...` in loose-package code and tests (`src/loader/package.rs`, `src/main.rs`) | Synthetic program IDs in NPDM and applet code | Synthetic values; no retail game IDs found in the tree. |
