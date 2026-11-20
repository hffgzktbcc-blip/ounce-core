# Ounce-Core repository review — 20 November 2026

**Purpose:** Record verified audit coverage, fixes and remaining evidence gaps. **This is not a certification that every file or code path is safe or correct.**

**Baseline:** `main` at `40580daa135c19ae5e69719d4e4deb4adc4eafc9`, after documentation PRs #518–#525. Audit observations reflect the public GitHub repository and its API at this commit unless specified.

## Inventory and audit coverage

The recursive GitHub tree was **not truncated** and lists **1,112 tracked blobs (about 29.4 MB of tracked content)**.

| Area | Inventory | This review checked | Limit |
| --- | ---: | --- | --- |
| Active reference and task Markdown (excluding history, releases and verification records) | 92 files | Read each and screened for obsolete versions, unsupported present-tense claims, old counts and incomplete differential commands | Heuristic document screening, not detailed technical proof of every paragraph |
| Verification entry-point and harness-index Markdown under `docs/verify/` | 38 files | Read and screened for stale version/compatibility language | This is a subset of 414 files in `docs/verify/`; raw C fixtures, output records and other supporting data were not re-executed |
| GitHub Actions workflows | 24 files | Inspected triggers, action pins, permissions, release/security job configuration and suspect shell execution patterns | Not a formal workflow exploit assessment; check the live Settings, branch protections and scanner alerts separately |
| Public website HTML/pages | 10 HTML pages plus related assets in `site/` | Screened for contradictory release labels and claims; previous PRs #519–#524 updated key visitor pages | Browser accessibility, mobile layout and live deployment behaviour were not rerun locally |
| Selected high-risk execution sources | Paths in `src/fs/`, `src/kernel/fsp.rs`, `src/loader/`, `src/cli/`, `src/diverge.rs`, core entry points and the separate unsafe crates | Inspected for unsafe blocks, filesystem operations, subprocesses, canonicalisation and selected explicit boundary tests | **Not a full manual security audit of the 145 `src/` files or their transitive dependencies** |
| Historical studies and release records | Many dated documents/fixtures | Retained historical test numbers and checked links to current release where directly used | No rerun of 2026 historical benchmarks or review of every dated experiment |

## Verified outcomes

- **Release identity:** [v0.10.0](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.10.0) was published on 20 November, and main-branch CI succeeded at the review baseline. The separate differential-verification workflow was still `in_progress` when queried; do **not** equate a passing core CI run with completion of that separate workflow.
- **No current PR backlog** was open when this review began. The independent-test [issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) remained open without a reproduced outside result.
- **Documentation defect fixed in this PR:** `docs/DETAILS.md` instructed an ARM64 tester to run `scripts/verify_differential.py` with `--json` but **without `--nro` or `--nro-dir`**. The script explicitly records `skipped (no NRO)` when it has no guest input; a host-only exit 0 is not hardware differential verification. We now link the scoped Lua/FP rerun and require inspection of per-engine statuses.
- **Workflow review:** all 24 workflow files were checked for their declared triggers and pinned `uses:` steps. `rust-security.yml` configures CodeQL for Rust, `cargo audit` and `cargo deny`, but their **current findings and pass state were not established by this review**.
- **Host-file boundary:** `src/fs/romfs.rs` contains a canonicalisation check for host-backed files and Unix symlink-escape tests; `src/kernel/fsp.rs` has tests for escaping `sdmc:/`. This is positive test coverage, **not proof** against every race, invalid path or guest input.
- **Process lifecycle:** `src/diverge.rs` uses a process-ID-specific directory under the system temporary directory, rebuilds a helper with `cc`, and removes that directory. Further review should check concurrent execution and attacker-controlled temporary directories; no exploit was demonstrated.
- **Sibling release drift:** `ounce-hb` published [v0.2.0](https://github.com/hffgzktbcc-blip/ounce-hb/releases/tag/v0.2.0), but its README still said v0.1.0 was latest and Ounce-Core v0.9.0 was current. A separate documentation-only HB PR corrects this, keeping the intentional v0.8.1 nightly CI compatibility pin clearly labelled.

## Evidence gaps before claiming a complete repository audit

1. **Security scanners:** Read GitHub's live CodeQL, RustSec/cargo-audit, cargo-deny, Dependabot and secret-scanning results, including any open alerts. The connected public GitHub endpoint did not expose private CodeQL/Dependabot alert details. **No security-clean sign-off is possible without those results.**
2. **All source code:** Review the remaining CPU, kernel, GPU, MMU, loader, networking and CLI paths manually, including fuzz and input-boundary tests; run unit, integration and differential suites on the exact head commit. Lexical scanning does not establish correctness or exploitability.
3. **Hardware and third parties:** Obtain genuine external ARM64 runs with pinned inputs and retained logs. CI and maintainers' runs do not count as outside reproduction.
4. **Legal/distribution:** Validate dependency licences, complete source attributions, notice/SBOM packaging and published archives with the appropriate owner review. Existing automated gates are helpful, not a substitute for that review.
5. **Release hygiene:** Before each new release, compare the release page, README and CLI docs against the exact tag, supported platforms and actual artefact hashes. Do not rewrite dated studies to claim results they did not measure.

## Writing and review standard

Lead with **who does what and why**; show one pinned before/after example; disclose the tested inputs, host, command, version and limitations. Separate **released**, **measured internally**, **planned**, **blocked** and **independently reproduced**. A documentation claim is not validated just because its Markdown link resolves.

This review is deliberately bounded and dated. Update a follow-up report only after fresh evidence, not to manufacture a `100% audited` label.
