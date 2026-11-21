'use strict';

const https = require('https');
const fs = require('fs');
const path = require('path');

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

async function runPatrol({ getGitHubSummary, store, saveStore, ntfyUrl, notifyAlways = false, stateDir = path.join(__dirname, 'data') }) {
  const gh = await getGitHubSummary(true);
  const now = new Date().toISOString();
  const degraded = Boolean(gh.source?.error || (gh.repos || []).some(repo => repo.error || repo.releaseError));

  const seenFile = path.join(stateDir, 'patrol_seen.json');
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
  if (!degraded) {
    fs.writeFileSync(seenFile, JSON.stringify({
      issues: Array.from(seenIssues),
      pulls: Array.from(seenPulls),
      lastPatrolAt: now,
    }, null, 2));
  }

  const openIssuesTotal = (gh.repos || []).reduce((a, r) => a + (r.openIssuesCount || 0), 0);
  const openPullsTotal = (gh.repos || []).reduce((a, r) => a + (r.openPullsCount || 0), 0);
  const summary = `${degraded ? 'Patrol degraded' : 'Patrol'} @ ${now}: ${openPullsTotal} PR items listed, ${openIssuesTotal} issue items listed across 5 repos.` +
    (degraded ? ' GitHub refresh incomplete; prior data may be shown.' : newItems.length && !firstRun ? ` New: ${newItems.join(' | ')}` : ' No new items since last check.');

  store.cloudWorker = {
    lastCheckedAt: now,
    lastTask: 'ORACLE-HOURLY-PATROL',
    runRef: 'oracle-vm · ounce-cc hourly patrol',
    status: degraded ? 'degraded' : 'complete',
    summary,
  };

  // Mark CLOUD-SCHEDULE complete if present
  const schedRec = (store.records || []).find(r => r.id === 'CLOUD-SCHEDULE');
  if (!degraded && schedRec && schedRec.status !== 'complete') {
    schedRec.status = 'complete';
    schedRec.result = `Resolved on Oracle VM: ounce-cc hourly patrol active (${now}).`;
    schedRec.updatedAt = now;
  }
  saveStore();

  let ntfyResult = { skipped: true };
  if (!degraded && ntfyUrl && (notifyAlways || (!firstRun && newItems.length > 0))) {
    ntfyResult = await sendNtfy({
      topicUrl: ntfyUrl,
      title: newItems.length && !firstRun ? 'Ounce Patrol: New GitHub Activity' : 'Ounce Patrol: All Clear',
      message: summary,
      tags: newItems.length && !firstRun ? 'loudspeaker,octocat' : 'white_check_mark',
    });
  }

  return { ok: !degraded, summary, newItems: firstRun ? [] : newItems, ntfyResult };
}

module.exports = { runPatrol, sendNtfy };
