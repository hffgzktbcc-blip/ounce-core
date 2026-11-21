# Ounce Family Command Centre

Private, dependency-free Node service for the five Ounce family repositories. The browser UI is mobile first; `linkedom` is used only by tests.

## Data freshness contract

- **GitHub live** comes from GitHub's API for Ounce-Core, Ounce-HB, Ounce-Runtime, DeckStream and Ounce-Spark. The server reconciles every five minutes and retains the last successful project result when a later check fails.
- **Maintained records** come from `data/store.json`. They are labelled stale when their newest `updatedAt` value is over 24 hours old. Reading the file does not make it fresh.
- Ounce-Spark is labelled experimental. Its releases endpoint may correctly have no published release; the UI must not turn that into a release claim or an ingestion error.

The open app rereads sources every minute and when it regains focus. Creating, editing or deleting a task rereads immediately.

## GitHub event trigger

Set `GITHUB_WEBHOOK_SECRET` in `.env`, then add the same webhook secret to each of the five repositories. Point the webhook to:

```text
https://<private-command-centre-host>/api/hooks/github
```

Use `application/json` and subscribe to pull requests, releases, issues and pushes. The endpoint verifies `X-Hub-Signature-256` before accepting an event. Relevant events trigger an immediate reconciliation; the five-minute loop remains the recovery path for missed deliveries.

## Local checks

```sh
npm install --ignore-scripts
npm test
npm run check
```

## Oracle deployment

The current VM path is `/home/ubuntu/ounce-command-centre`, served by PM2 as `ounce-cc` on `127.0.0.1:4100`. Preserve `.env` and `data/` across deployments. Deploy only a reviewed commit, then restart `ounce-cc` and verify:

1. `/api/github?force=1` returns five projects and source health.
2. `/api/records` returns independent manual-record freshness.
3. the deployed source hashes match the reviewed commit;
4. authenticated desktop and phone-width browser checks pass.

Do not copy `.env`, `data/store.json` or webhook secrets into Git.
