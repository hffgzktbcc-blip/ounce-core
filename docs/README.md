# docs/ index

Start here when you do not know which file is current. **Live** files describe how the
project works today. **History** files record what was true on a past date; they are not
tasks and not rules.

## For agents (live)
| Need | File |
|---|---|
| Working rules (single source) | [../AGENTS.md](../AGENTS.md) (`CLAUDE.md` / `GEMINI.md` / `.github/copilot-instructions.md` only point to it) |
| Backlog: what is ready, blocked, done | [../PLAN.md](../PLAN.md) (finished-task evidence: [history/PLAN-COMPLETED.md](history/PLAN-COMPLETED.md)) |
| Longer-range strategy | [roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md](roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md), [roadmap/milestones.json](roadmap/milestones.json) |
| Environment setup, branches, claims, handoff | [AGENT_SETUP.md](AGENT_SETUP.md), [AGENT_HUB.md](AGENT_HUB.md) |
| What the project does **not** prove | [LIMITS.md](LIMITS.md), [CAPABILITY_SCOPE.md](CAPABILITY_SCOPE.md) |

## Using and verifying (live)
- Quick start: [QUICKSTART.md](QUICKSTART.md); reproduce results: [REPRODUCE.md](REPRODUCE.md); add a test program: [ADD_A_PROGRAM.md](ADD_A_PROGRAM.md)
- CLI: [DIAGNOSIS_CLI.md](DIAGNOSIS_CLI.md), [FAILURE_TO_REPRODUCTION.md](FAILURE_TO_REPRODUCTION.md), [INSTRUCTION_DIAGNOSIS.md](INSTRUCTION_DIAGNOSIS.md), [DIVERGE.md](DIVERGE.md), run report schema [OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md)
- Apps: [PLAYGROUND.md](PLAYGROUND.md), [MAC_APP.md](MAC_APP.md), [INTERACTIVE_LAB.md](INTERACTIVE_LAB.md), [WORKBENCH_UI.md](WORKBENCH_UI.md)
- Verification evidence and method: [verify/README.md](verify/README.md) (dated result folders live under `verify/`)

## Corpus and oracles (live reference)
Independent homebrew corpus: [independent-corpus.md](independent-corpus.md) ([cpu](independent-corpus-cpu.md), [graphics](independent-corpus-graphics.md), [services](independent-corpus-services.md), [runner](independent-corpus-runner.md)).
Oracles (each was written before execution): [corpus-oracles.md](corpus-oracles.md), [startup](startup-oracle.md), [simplegfx](first-frame-oracle.md), [deko_basic](deko-basic-oracle.md), [romfs](romfs-oracle.md), [interactive triangle](interactive-triangle-oracle.md). Reproduce recipes: [simplegfx](REPRODUCE_SIMPLEGFX.md), [deko_basic](REPRODUCE_DEKO_BASIC.md), [romfs](REPRODUCE_ROMFS.md). Also [soak.md](soak.md), [svc-coverage.md](svc-coverage.md).

## Code layout
`src/main.rs` (dispatch only) -> `src/cli/` (commands) -> `src/cpu`, `kernel`, `gpu`, `loader`, `mmu`, `audio`, `hid`, `fs`, `linux` (runtime); `crates/` holds the isolated unsafe-capable crates. See [AGENT_SETUP.md](AGENT_SETUP.md#known-traps-each-one-has-cost-real-time).

## Design notes
[design/](design/) (per-milestone designs; a design note is intent, check code and PLAN.md for what shipped), [architecture/layers.json](architecture/layers.json), [guest-host-gpu.md](guest-host-gpu.md), [VISION.md](VISION.md), [DETAILS.md](DETAILS.md).

## Legal and provenance (live)
[PROVENANCE.md](PROVENANCE.md), [CLEAN_ROOM.md](CLEAN_ROOM.md), [LICENSE_DISPOSITION.md](LICENSE_DISPOSITION.md), [E2_DECISION_MEMO.md](E2_DECISION_MEMO.md), [DEPENDENCY_LICENSE_AUDIT.md](DEPENDENCY_LICENSE_AUDIT.md). Provenance is audited; do not re-inventory.

## Releases
[releases/](releases/) holds per-version notes and proofs; [../CHANGELOG.md](../CHANGELOG.md) is the summary.

## Community and outreach
[community/](community/), [outreach/](outreach/), [INDEPENDENT_TESTER_CHALLENGE.md](INDEPENDENT_TESTER_CHALLENGE.md), [COMMUNITY_VERIFICATION_GUIDE.md](COMMUNITY_VERIFICATION_GUIDE.md), [GITHUB_FREE_TOOLING.md](GITHUB_FREE_TOOLING.md), [IMPROVEMENT_IDEAS.md](IMPROVEMENT_IDEAS.md) (ideas list, not a backlog).

## History (do not treat as instructions)
- [history/handoffs/](history/handoffs/): every past agent handoff note (moved here 2026-11-17)
- [history/PLAN-2026-11-07.md](history/PLAN-2026-11-07.md), [history/PLAN-COMPLETED.md](history/PLAN-COMPLETED.md): old plans and finished-task evidence
- [history/OUR_JOURNEY.md](history/OUR_JOURNEY.md): founder narrative
- [history/](history/): provenance remediation drafts and reports
