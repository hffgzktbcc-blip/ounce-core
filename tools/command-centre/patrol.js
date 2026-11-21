'use strict';

const https = require('https');
const fs = require('fs');
const path = require('path');

const REPO_SLUGS = [
  { key: 'Ounce-Core', slug: 'ounce-core' },
  { key: 'Ounce-HB', slug: 'ounce-hb' },
  { key: 'Ounce-Runtime', slug: 'ounce-runtime' },
  { key: 'DeckStream', slug: 'deckstream' },
  { key: 'Ounce-Spark', slug: 'Ounce-Spark' },
];

function sendNtfy({ topicUrl, title, message, priority = 'default', tags = 'robot' }) {
  return new Promise(resolve => {
    if (!topicUrl) return resolve({ ok: false, skipped: true });
    try {
      const u = new URL(topicUrl);
      const body = Buffer.from(String(message || ''), 'utf8');
      const req = https.request({
        hostname: u.hostname,
        port: u.port || 443,
        path: u.pathname,
        method: 'POST',
        headers: {
          'Title': title || 'Ounce Command Centre',
          'Priority': priority,
          'Tags': tags,
          'Content-Length': body.length,
        },
        timeout: 8000,
      }, res => {
        res.resume();
        resolve({ ok: res.statusCode >= 200 && res.statusCode < 300, status: res.statusCode });
      });
      req.on('error', err => resolve({ ok: false, error: err.message }));
      req.on('timeout', () => { req.destroy(); resolve({ ok: false, error: 'timeout' }); });
      req.write(body);
      req.end();
    } catch (e) {
      resolve({ ok: false, error: e.message });
    }
  });
}

async function runPatrol({ getGitHubSummary, store, saveStore, ntfyUrl, notifyAlways = false }) {
  const gh = await getGitHubSummary(true);
  const now = new Date().toISOString();

  const seenFile = path.join(__dirname, 'data', 'patrol_seen.json');
  let seen = { issues: [], pulls: [] };
  if (fs.existsSync(seenFile)) {
    try { seen = JSON.parse(fs.readFileSync(seenFile, 'utf8')); } catch {}
  }
  const seenIssues = new Set(seen.issues || []);
  const seenPulls = new Set(seen.pulls || []);

  const newItems = [];
  for (const r of gh.repos || []) {
    for (const p of r.pulls || []) {
      const key = `${r.slug}#pr${p.number}`;
      if (!seenPulls.has(key)) {
        seenPulls.add(key);
        newItems.push(`PR ${r.project} #${p.number}: ${p.title}`);
      }
    }
    for (const i of r.issues || []) {
      const key = `${r.slug}#${i.number}`;
      if (!seenIssues.has(key)) {
        seenIssues.add(key);
        newItems.push(`Issue ${r.project} #${i.number}: ${i.title}`);
      }
    }
  }

  const firstRun = !fs.existsSync(seenFile);
  fs.writeFileSync(seenFile, JSON.stringify({
    issues: Array.from(seenIssues),
    pulls: Array.from(seenPulls),
    lastPatrolAt: now,
  }, null, 2));

  const openIssuesTotal = (gh.repos || []).reduce((a, r) => a + (r.openIssuesCount || 0), 0);
  const openPullsTotal = (gh.repos || []).reduce((a, r) => a + (r.openPullsCount || 0), 0);
  const summary = `Patrol @ ${now}: ${openPullsTotal} open PRs, ${openIssuesTotal} open issues across 5 repos.` +
    (newItems.length && !firstRun ? ` New: ${newItems.join(' | ')}` : ' No new items since last check.');

  store.cloudWorker = {
    lastCheckedAt: now,
    lastTask: 'ORACLE-HOURLY-PATROL',
    runRef: 'oracle-vm · ounce-cc hourly patrol',
    status: 'complete',
    summary,
  };

  // Mark CLOUD-SCHEDULE complete if present
  const schedRec = (store.records || []).find(r => r.id === 'CLOUD-SCHEDULE');
  if (schedRec && schedRec.status !== 'complete') {
    schedRec.status = 'complete';
    schedRec.result = `Resolved on Oracle VM: ounce-cc hourly patrol active (${now}).`;
    schedRec.updatedAt = now;
  }
  saveStore();

  let ntfyResult = { skipped: true };
  if (ntfyUrl && (notifyAlways || (!firstRun && newItems.length > 0))) {
    ntfyResult = await sendNtfy({
      topicUrl: ntfyUrl,
      title: newItems.length && !firstRun ? 'Ounce Patrol: New GitHub Activity' : 'Ounce Patrol: All Clear',
      message: summary,
      tags: newItems.length && !firstRun ? 'loudspeaker,octocat' : 'white_check_mark',
    });
  }

  return { ok: true, summary, newItems: firstRun ? [] : newItems, ntfyResult };
}

module.exports = { runPatrol, sendNtfy, REPO_SLUGS };
