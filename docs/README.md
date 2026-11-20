# Find the right Ounce-Core document

This index points you to instructions, verified results, and the current development plan. Start here when you do not know which file is current. **Live** files describe how the project works
today. **History** files record what was true on a past date; they are not tasks and not rules.
Before starting any work run `scripts/done.sh <issue number or keyword>` (searches merged history,
changelog and release notes) so shipped work is not redone.

## For agents (live)
| Need | File |
|---|---|
| Working rules (single source) | [../AGENTS.md](../AGENTS.md) (`CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md` only point to it) |
| Setup, branch names, claims, **known traps** | [AGENT_SETUP.md](AGENT_SETUP.md); phone/hub workflow [AGENT_HUB.md](AGENT_HUB.md) |
| Backlog: what is ready, blocked, done | [../PLAN.md](../PLAN.md) (finished-task evidence: [history/PLAN-COMPLETED.md](history/PLAN-COMPLETED.md)) |
| Direction for the Ounce family | [roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md](roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md) (committed); [roadmap/OUNCE_EDUCATION_ROADMAP.md](roadmap/OUNCE_EDUCATION_ROADMAP.md) is a proposal only |
| What the project does **not** prove | [LIMITS.md](LIMITS.md), [CAPABILITY_SCOPE.md](CAPABILITY_SCOPE.md) |
| What shipped, by release | [../CHANGELOG.md](../CHANGELOG.md), [releases/](releases/) (latest: [v0.10.0](releases/v0.10.0.md), published 2026-11-20; [GitHub Release](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.10.0)) |

## For readers: start with a task

- **Run a supported program:** follow the [Quick Start](QUICKSTART.md), then use [Add a Program](ADD_A_PROGRAM.md) for your own test.
- **Investigate a mismatch:** start with the [diagnosis guide](DIAGNOSIS_CLI.md) and the [run report schema](OUNCE_RUN_V1_SCHEMA.md).
- **Check a published result:** use the [reproduction guide](REPRODUCE.md), read the [limits](LIMITS.md), and share outside results through [issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248).
- **Understand the current release:** read [v0.10.0 highlights and evidence](releases/v0.10.0.md) before relying on older, dated results.

## Using and verifying (live)
- Quick start: [QUICKSTART.md](QUICKSTART.md); reproduce results: [REPRODUCE.md](REPRODUCE.md); add a test program: [ADD_A_PROGRAM.md](ADD_A_PROGRAM.md); full reference: [DETAILS.md](DETAILS.md)
- CLI: [DIAGNOSIS_CLI.md](DIAGNOSIS_CLI.md), [FAILURE_TO_REPRODUCTION.md](FAILURE_TO_REPRODUCTION.md), [INSTRUCTION_DIAGNOSIS.md](INSTRUCTION_DIAGNOSIS.md), [DIVERGE.md](DIVERGE.md), run report schema [OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md), CI instruction budgets [INSN_BUDGETS.md](INSN_BUDGETS.md), step trace and first-divergence schema [OUNCE_TRACE_V1_SCHEMA.md](OUNCE_TRACE_V1_SCHEMA.md), v1.0 compatibility contract (DRAFT, owner decision pending) [CONTRACT_V1.md](CONTRACT_V1.md)
- Apps: [PLAYGROUND.md](PLAYGROUND.md), [MAC_APP.md](MAC_APP.md), [INTERACTIVE_LAB.md](INTERACTIVE_LAB.md), [WORKBENCH_UI.md](WORKBENCH_UI.md)
- Evidence and method: [verify/README.md](verify/README.md); dated results stay under `verify/` ([audio case](verify/AUDIO_CHANNEL_CASE.md), [census](verify/CENSUS_2026-11-08.md), [stress](verify/STRESS_2026-11-08.md), [issue shortlist](verify/ISSUE_SHORTLIST.md))
- Research record (a dated preprint, not guidance): [research/OUNCE_CORE_AARCH64_VERIFICATION_2026.md](research/OUNCE_CORE_AARCH64_VERIFICATION_2026.md)

## Corpus and oracles (live reference)
Oracles were written **before** execution and are referenced by path from code and tests, so they never move.
[corpus-oracles.md](corpus-oracles.md), [startup](startup-oracle.md), [simplegfx](first-frame-oracle.md), [deko_basic](deko-basic-oracle.md), [romfs](romfs-oracle.md), [interactive triangle](interactive-triangle-oracle.md). Reproduce recipes: [simplegfx](REPRODUCE_SIMPLEGFX.md), [deko_basic](REPRODUCE_DEKO_BASIC.md), [romfs](REPRODUCE_ROMFS.md). Also [soak.md](soak.md), [svc-coverage.md](svc-coverage.md), [guest-host-gpu.md](guest-host-gpu.md). The early corpus-preparation proposals are in [history/independent-corpus/](history/independent-corpus/).

## Code layout
`src/main.rs` (dispatch only) -> `src/cli/` (commands) -> `src/cpu`, `kernel`, `gpu`, `loader`, `mmu`, `audio`, `hid`, `fs`, `linux` (runtime); `crates/` holds the isolated unsafe-capable crates. See [AGENT_SETUP.md](AGENT_SETUP.md#known-traps-each-one-has-cost-real-time).

## Design notes
Live: [design/ounce-verify.md](design/ounce-verify.md), [design/gpu-replay-correctness.md](design/gpu-replay-correctness.md), [architecture/layers.json](architecture/layers.json), [VISION.md](VISION.md).
Shipped or dropped (banner at the top of each; code is the truth): [design/m4-native-jit.md](design/m4-native-jit.md) and [design/m4-native-linking.md](design/m4-native-linking.md) shipped as `native-jit`; [design/m5-deko3d-triangle.md](design/m5-deko3d-triangle.md) was not built as written. Archived: [history/design/](history/design/).

## Legal and provenance (live)
[PROVENANCE.md](PROVENANCE.md), [CLEAN_ROOM.md](CLEAN_ROOM.md), [LICENSE_DISPOSITION.md](LICENSE_DISPOSITION.md) (the current disposition), [DEPENDENCY_LICENSE_AUDIT.md](DEPENDENCY_LICENSE_AUDIT.md) (dated snapshot). The release notice review is `RELEASE_NOTICE_REVIEW.json`; if CI says its hashes are stale, see trap 1 in [AGENT_SETUP.md](AGENT_SETUP.md). Provenance is audited; do not re-inventory.

## Community (live engineering docs)
[INDEPENDENT_TESTER_CHALLENGE.md](INDEPENDENT_TESTER_CHALLENGE.md), [COMMUNITY_VERIFICATION_GUIDE.md](COMMUNITY_VERIFICATION_GUIDE.md), [community/START_HERE.md](community/START_HERE.md), [community/GLOSSARY.md](community/GLOSSARY.md), [community/EXTERNAL_TEST_REPORT.md](community/EXTERNAL_TEST_REPORT.md), [community/TRY_THIS_WEEK.md](community/TRY_THIS_WEEK.md) (time-boxed; check the issues it names are still open), [community/OUTSIDE_REPRODUCTION_2026.md](community/OUTSIDE_REPRODUCTION_2026.md) (dated snapshot; live status is PLAN.md E1), [GITHUB_FREE_TOOLING.md](GITHUB_FREE_TOOLING.md), [IMPROVEMENT_IDEAS.md](IMPROVEMENT_IDEAS.md) (ideas list, not a backlog). Drafts for a macOS app release: [releases/app-next-draft.md](releases/app-next-draft.md).

## History (never take work or rules from here)
- [history/handoffs/](history/handoffs/): every past agent handoff note
- [history/PLAN-2026-11-07.md](history/PLAN-2026-11-07.md), [history/PLAN-COMPLETED.md](history/PLAN-COMPLETED.md): old plans and finished-task evidence
- [history/E2_DECISION_MEMO.md](history/E2_DECISION_MEMO.md): one-day owner memo; decisions are recorded in LICENSE_DISPOSITION.md
- [history/outreach/](history/outreach/), [history/community/](history/community/): marketing drafts, naming research, recruitment lead lists. These are **owner/marketing material, not agent tasks**.
- [history/OUR_JOURNEY.md](history/OUR_JOURNEY.md): founder narrative; [history/](history/): provenance remediation drafts and reports

## Keep live documentation clear and current

Use this checklist when changing the README, website, quick starts, status pages, releases or contributor guides. Historic releases and dated experiment records remain unchanged unless you are correcting a factual error in their stated historical context.

1. **Name the actor and use a verb.** Write "Ounce-Core runs supported ARM64 programs and compares their outputs" instead of "ARM64 multi-engine differential execution verification". Say what a developer can do next.
2. **Lead with the outcome.** Give readers a short task or reason to care before introducing terms such as `FPCR`, `NRO`, `Tier-4` or `CMIF`. Link the [glossary](community/GLOSSARY.md) when a term matters.
3. **Keep claims bounded.** State the tested input, engine, hardware or CI job. "Passed on these 51 workloads" is evidence; "runs any ARM64 application" is not. A green CI badge does not mean an outside developer reproduced a result.
4. **Separate shipped from planned.** Check live tags, merged PRs, CI and [the latest release notes](releases/v0.10.0.md) before saying "unreleased", "not yet merged" or "coming soon". Keep genuinely draft contracts clearly marked draft.
5. **Update the source, not just the rendered copy.** The roadmap table in [DETAILS.md](DETAILS.md) is generated from [roadmap/milestones.json](roadmap/milestones.json). Change both consistently through `scripts/render_roadmap.py` or its source; do not hand-edit a generated claim alone.
6. **Preserve evidence and limitations.** Simplify sentences, not test results. Keep legal notices, licence attribution, source links, dates, unsupported features and failure reports intact.

Before proposing a documentation-only PR, run `python3 scripts/check_links.py`, `python3 scripts/check_pins.py`, and inspect the diff for outdated counts or commands that no longer match their scripts. These checks catch link and version mistakes, not every inaccurate claim.
