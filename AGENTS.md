PLAN.md is the single source of truth.
Start with `scripts/next.sh`; save with `scripts/gate.sh --checkpoint "<what/why>"`; finish with `scripts/gate.sh --ship`.
Guardrails (legal, `unsafe` boundary, provenance) are enforced by CI and by `gate.sh`.
