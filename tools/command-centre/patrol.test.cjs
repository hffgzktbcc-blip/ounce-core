const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { runPatrol } = require('./patrol');

test('degraded GitHub data never completes the patrol evidence record', async t => {
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ounce-patrol-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const schedule = { id: 'CLOUD-SCHEDULE', status: 'open' };
  const store = { cloudWorker: {}, records: [schedule] };
  const result = await runPatrol({
    getGitHubSummary: async () => ({ source: { error: '1 of 5 projects failed' }, repos: [{ project: 'Ounce-Spark', slug: 'Ounce-Spark', error: 'HTTP 500', issues: [], pulls: [] }] }),
    store,
    saveStore() {},
    ntfyUrl: '',
    stateDir,
  });
  assert.equal(result.ok, false);
  assert.equal(store.cloudWorker.status, 'degraded');
  assert.equal(schedule.status, 'open');
  assert.match(result.summary, /items listed/i);
});
