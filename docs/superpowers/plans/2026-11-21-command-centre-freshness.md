# Command Centre freshness and mobile plan

## Intent

Make the private Ounce Family Command Centre an honest, phone-first operational view of all five projects. Ounce-Spark is early research and must never be presented as a released GPU product or as hardware-validated Blackwell performance.

## Approach

Keep the existing dependency-free Node application and bring its deployed source into this repository under `tools/command-centre`. Use one shared project registry for ingestion, filters, prompts, and labels. Preserve the current JSON store and PM2 deployment contract.

GitHub-derived data and manually maintained records remain separate sources. Each source exposes its own checked/updated timestamp, stale/error state, and reconciliation status. A bounded server refresh keeps GitHub data warm, a signed webhook endpoint invalidates it after relevant GitHub events, and the open app periodically reconciles without replacing already loaded data on errors.

## Tasks

1. Import the retained deployed source and prove its current tests pass unchanged.
2. Add failing tests for five-project coverage, unreleased Spark handling, source freshness metadata, background/webhook reconciliation, and mobile-readable source status.
3. Implement the shared registry and reconciliation state with the smallest server/API changes that satisfy those tests.
4. Update the UI to label live GitHub and manual records clearly, show checked-at/error states, include Ounce-Spark throughout, and reduce narrow-screen crowding.
5. Run focused tests, the full command-centre suite, syntax checks, repository scope checks, and a separate desktop/phone browser pass.
6. Commit and push only the scoped app/source-control changes, open a PR, inspect final-head CI, and deploy only if the reviewable source and live VM can be matched exactly. Do not merge or publish a release.

## Verification limits

Local tests prove application behavior. A deployed claim additionally requires the Oracle VM to be reachable, the deployed hashes to match the branch, authenticated API responses to show five projects, and phone/desktop browser evidence from the live URL. If any of these are unavailable, report that exact blocker.
