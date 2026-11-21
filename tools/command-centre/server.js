'use strict';

const http = require('http');
const https = require('https');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const os = require('os');
const { exec, spawn } = require('child_process');

const PORT = Number(process.env.PORT || 4100);
const HOST = process.env.HOST || '127.0.0.1';
const BASE_DIR = __dirname;
const PUBLIC_DIR = path.join(BASE_DIR, 'public');
const DATA_DIR = path.join(BASE_DIR, 'data');
const STORE_PATH = path.join(DATA_DIR, 'store.json');
const SEED_PATH = path.join(DATA_DIR, 'seed_data.json');
const ROADMAP_PATH = path.join(DATA_DIR, 'roadmap.json');
const ENV_PATH = path.join(BASE_DIR, '.env');
const ORCA_BIN = process.env.ORCA_BIN || '/home/ubuntu/.orca/cli/bin/orca';
const OUNCE_DIR = process.env.OUNCE_DIR || '/home/ubuntu/ounce-family';

// Load .env if present
function loadEnv() {
  if (!fs.existsSync(ENV_PATH)) return;
  const lines = fs.readFileSync(ENV_PATH, 'utf8').split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    const key = trimmed.slice(0, eq).trim();
    const val = trimmed.slice(eq + 1).trim();
    if (!(key in process.env)) process.env[key] = val;
  }
}
loadEnv();

const ACCESS_PIN = process.env.ACCESS_PIN;
const SESSION_SECRET = process.env.SESSION_SECRET;
if (!ACCESS_PIN || !SESSION_SECRET) throw new Error('ACCESS_PIN and SESSION_SECRET are required. Set them in .env.');
const NTFY_URL = process.env.NTFY_URL || '';
const { runPatrol, sendNtfy } = require('./patrol');

const REPOS = [
  { key: 'Ounce-Core', slug: 'ounce-core', license: 'MIT', maturity: 'established', desc: 'ARM64 execution & verification research platform', dir: path.join(OUNCE_DIR, 'ounce-core') },
  { key: 'Ounce-HB', slug: 'ounce-hb', license: 'MIT', maturity: 'established', desc: 'Homebrew runner and diagnostics', dir: path.join(OUNCE_DIR, 'ounce-hb') },
  { key: 'Ounce-Runtime', slug: 'ounce-runtime', license: 'GPL-3.0-or-later', maturity: 'established', desc: 'Experimental performance & host GPU runtime', dir: path.join(OUNCE_DIR, 'ounce-runtime') },
  { key: 'DeckStream', slug: 'deckstream', license: 'MIT', maturity: 'established', desc: 'Steam Deck own-storage NAS/WebDAV launcher', dir: path.join(OUNCE_DIR, 'deckstream') },
  { key: 'Ounce-Spark', slug: 'Ounce-Spark', license: 'MIT', maturity: 'experimental', desc: 'CPU-based PTX semantics and performance-model research', scopeNote: 'CPU-based PTX research; no GPU acceleration or hardware-validated Blackwell performance.', dir: path.join(OUNCE_DIR, 'Ounce-Spark') },
];
const PROJECTS = REPOS.map(({ dir, ...project }) => project);
const PROJECT_KEYS = REPOS.map(repo => repo.key);

const BOT_ROSTER = [
  {
    id: 'grok-core',
    name: 'Grok-Core',
    role: 'Chief of Staff',
    emoji: '⚡',
    tagline: 'Snappy, unfiltered family coordinator. Sees all 5 repos, Oracle & Orca.',
    defaultEngine: 'codex',
    persona: `You are Grok-Core, the witty, razor-sharp, slightly cheeky Chief of Staff for the Ounce Family Command Centre. Keep replies punchy, mobile-readable (short bullets, bold highlights), and fun with subtle dry humor — like a clever little Grok bot — while NEVER fabricating evidence, SHAs, or test passes.`,
  },
  {
    id: 'builder',
    name: 'Builder',
    role: 'CI & Fix Gremlin',
    emoji: '🛠️',
    tagline: 'Fixes scoped CI on milestone branches. Never rewrites history.',
    defaultEngine: 'codex',
    persona: `You are Builder, a fast-talking, pragmatic CI-fixing Grok bot. You love clean diffs and green checkmarks, roast flaky builds, and strictly work on scoped branches without force-pushing or squashing.`,
  },
  {
    id: 'inspector',
    name: 'Inspector',
    role: 'SHA & Evidence Auditor',
    emoji: '🔍',
    tagline: 'Zero-mercy auditor. Demands exact SHAs, logs, and proof.',
    defaultEngine: 'codex',
    persona: `You are Inspector, a skeptical, razor-sharp Grok bot auditor. You roast vague claims, demand exact commit SHAs, workflow run IDs, and hard logs, and never label an agent run as an outside-human reproduction.`,
  },
  {
    id: 'scout',
    name: 'Scout',
    role: 'Issue & Signal Hunter',
    emoji: '🛰️',
    tagline: 'Hunts open issues, contributor leads, and repo drift.',
    defaultEngine: 'codex',
    persona: `You are Scout, an energetic, curious Grok bot who sniffs out high-signal GitHub issues, contributor opportunities, and stale tasks across Ounce-Core, Ounce-HB, Ounce-Runtime, DeckStream, and the experimental Ounce-Spark research project.`,
  },
  {
    id: 'quartermaster',
    name: 'Quartermaster',
    role: 'Release Gatekeeper',
    emoji: '📦',
    tagline: 'Guards the 7 release gates & MIT/GPL boundaries.',
    defaultEngine: 'codex',
    persona: `You are Quartermaster, a meticulous yet witty Grok bot guarding the 7 release gates, MIT vs GPL-3.0-or-later license boundaries, and outreach drafts. Nothing ships without owner approval.`,
  },
  {
    id: 'deckhand',
    name: 'Deckhand',
    role: 'Steam Deck & WebDAV Op',
    emoji: '🎮',
    tagline: 'DeckStream field operative. Lives for real-device NAS logs.',
    defaultEngine: 'codex',
    persona: `You are Deckhand, a playful, salty-dog Grok bot for DeckStream. You care about real Steam Deck + Proton + WebDAV/NAS test logs (passes AND failures) and zero fake benchmarks.`,
  },
];

const COORDINATOR_CONTRACT = `OUNCE FAMILY RULES (STRICT):
- Work only on: ounce-core (MIT), ounce-hb (MIT), ounce-runtime (GPL-3.0-or-later), deckstream (MIT), Ounce-Spark (MIT, experimental research).
- Ounce-Spark is CPU-based PTX research. Never imply GPU acceleration, a published release, cycle accuracy, or hardware-validated Blackwell performance without direct evidence.
- Never copy GPL runtime code into MIT siblings.
- Evidence first: report exact SHAs, commands, and observed outputs only. Never invent test passes or outside-human reproductions.
- No retail keys, firmware, proprietary dumps, or circumvention.
- No external messages, PR merges, or release publications without explicit owner approval.`;

// Ensure store exists
fs.mkdirSync(DATA_DIR, { recursive: true });
function loadStore() {
  if (!fs.existsSync(STORE_PATH)) {
    let seed = { cloudWorker: {}, records: [] };
    if (fs.existsSync(SEED_PATH)) {
      try { seed = JSON.parse(fs.readFileSync(SEED_PATH, 'utf8')); } catch {}
    }
    const initial = {
      cloudWorker: seed.cloudWorker || {},
      records: Array.isArray(seed.records) ? seed.records : [],
      chatHistory: [],
      jobs: [],
    };
    fs.writeFileSync(STORE_PATH, JSON.stringify(initial, null, 2));
    return initial;
  }
  return JSON.parse(fs.readFileSync(STORE_PATH, 'utf8'));
}

let store = loadStore();
function saveStore() {
  fs.writeFileSync(STORE_PATH, JSON.stringify(store, null, 2));
}

// Active async jobs in memory
const activeJobs = new Map();
const DISPATCH_PATH = path.join(DATA_DIR, 'dispatch-jobs.json');
let dispatchJobs = [];
try { dispatchJobs = JSON.parse(fs.readFileSync(DISPATCH_PATH, 'utf8')); } catch {}
if (!Array.isArray(dispatchJobs)) dispatchJobs = [];
for (const job of dispatchJobs) {
  if (job.status === 'running') {
    job.status = 'interrupted';
    job.error = 'The server restarted. Check the worktree before starting again.';
    job.finishedAt = new Date().toISOString();
  }
}
function saveDispatchJobs() {
  const next = DISPATCH_PATH + '.tmp';
  fs.writeFileSync(next, JSON.stringify(dispatchJobs.slice(0, 20), null, 2));
  fs.renameSync(next, DISPATCH_PATH);
}
if (dispatchJobs.length) saveDispatchJobs();
function engineInstalled(command) {
  if (path.isAbsolute(command)) { try { fs.accessSync(command, fs.constants.X_OK); return true; } catch { return false; } }
  return (process.env.PATH || '/usr/local/bin:/usr/bin:/bin').split(path.delimiter).some(dir => {
    try { fs.accessSync(path.join(dir, command), fs.constants.X_OK); return true; } catch { return false; }
  });
}
function dispatchOptions() {
  return {
    targets: ['command-centre-handoff', 'eelpout', 'ounce-family/deckstream', 'ounce-family/ounce-runtime'].map(id => ({ id, label: id.replace('ounce-family/', ''), available: fs.existsSync(dispatchTarget(id)), testCommand: fs.existsSync(path.join(dispatchTarget(id), 'Cargo.toml')) ? 'cargo test --lib' : 'python3 -m unittest discover -s tests' })),
    engines: ['bash', 'codex', 'openclaude', 'opencode'].map(id => ({ id, installed: engineInstalled(id) })),
  };
}

// Auth helpers
function signToken(payloadObj) {
  const payload = Buffer.from(JSON.stringify(payloadObj)).toString('base64url');
  const sig = crypto.createHmac('sha256', SESSION_SECRET).update(payload).digest('base64url');
  return `${payload}.${sig}`;
}

function verifyToken(token) {
  if (!token || !token.includes('.')) return null;
  const [payload, sig] = token.split('.');
  const expected = crypto.createHmac('sha256', SESSION_SECRET).update(payload).digest('base64url');
  if (sig.length !== expected.length) return null;
  if (!crypto.timingSafeEqual(Buffer.from(sig), Buffer.from(expected))) return null;
  try {
    const data = JSON.parse(Buffer.from(payload, 'base64url').toString('utf8'));
    if (data.exp && Date.now() > data.exp) return null;
    return data;
  } catch {
    return null;
  }
}

function parseCookies(req) {
  const raw = req.headers.cookie || '';
  const out = {};
  for (const part of raw.split(';')) {
    const idx = part.indexOf('=');
    if (idx === -1) continue;
    out[part.slice(0, idx).trim()] = decodeURIComponent(part.slice(idx + 1).trim());
  }
  return out;
}

function isAuthenticated(req) {
  const cookies = parseCookies(req);
  if (verifyToken(cookies.ounce_cc_session)) return true;
  const auth = req.headers.authorization || '';
  if (auth.startsWith('Bearer ') && auth.slice(7).trim() === ACCESS_PIN) return true;
  return false;
}

function sendJson(res, status, data, extraHeaders = {}) {
  const body = JSON.stringify(data);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
    ...extraHeaders,
  });
  res.end(body);
}

function readRawBody(req) {
  return new Promise((resolve, reject) => {
    let data = '';
    req.on('data', chunk => {
      data += chunk;
      if (data.length > 2 * 1024 * 1024) {
        reject(new Error('Body too large'));
        req.destroy();
      }
    });
    req.on('end', () => {
      resolve(data);
    });
    req.on('error', reject);
  });
}

async function readBody(req) {
  const data = await readRawBody(req);
  if (!data) return {};
  try { return JSON.parse(data); }
  catch { throw new Error('Invalid JSON'); }
}

function execPromise(cmd, opts = {}) {
  return new Promise(resolve => {
    exec(cmd, { timeout: opts.timeout || 12000, maxBuffer: 1024 * 1024, ...opts }, (err, stdout, stderr) => {
      resolve({
        ok: !err,
        code: err ? err.code : 0,
        stdout: (stdout || '').trim(),
        stderr: (stderr || '').trim(),
      });
    });
  });
}

// Agent prompts are argv, never shell source. Only the explicitly selected Bash runner
// treats its command field as a shell program.
function spawnPromise(command, args, { cwd, timeout = 120000, onOutput } = {}) {
  return new Promise(resolve => {
    const child = spawn(command, args, { cwd, detached: process.platform !== 'win32', env: process.env });
    let output = '', settled = false, timedOut = false, forceKill;
    const stop = signal => {
      try { if (child.pid && process.platform !== 'win32') process.kill(-child.pid, signal); else child.kill(signal); } catch {}
    };
    const finish = (code, error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      // Keep the escalation armed after a timed-out parent exits: descendants may remain.
      if (!timedOut) clearTimeout(forceKill);
      resolve({ ok: !timedOut && !error && code === 0, code, output: output.trim(), error: timedOut ? 'Task exceeded its two-minute limit.' : error?.message });
    };
    const timer = setTimeout(() => {
      timedOut = true;
      stop('SIGTERM');
      forceKill = setTimeout(() => stop('SIGKILL'), 2000);
      forceKill.unref?.();
    }, timeout);
    const append = chunk => {
      output = (output + chunk.toString()).slice(-65536);
      onOutput?.(output);
    };
    child.stdout.on('data', append);
    child.stderr.on('data', append);
    child.on('error', error => finish(null, error));
    child.on('close', code => finish(code));
  });
}

function dispatchTarget(id) {
  const targets = {
    'command-centre-handoff': '/home/ubuntu/orca/workspaces/ounce-core/command-centre-handoff',
    'eelpout': '/home/ubuntu/orca/workspaces/ounce-core/eelpout',
    'ounce-family/deckstream': path.join(OUNCE_DIR, 'deckstream'),
    'ounce-family/ounce-runtime': path.join(OUNCE_DIR, 'ounce-runtime'),
  };
  return Object.hasOwn(targets, id) ? targets[id] : null;
}

const githubAgent = new https.Agent({ keepAlive: true, maxSockets: 2, maxFreeSockets: 2 });
function fetchHttpsJson(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, {
      agent: githubAgent,
      headers: {
        'User-Agent': 'Ounce-Family-Command-Centre/2.0',
        'Accept': 'application/vnd.github+json',
        ...(process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}),
      },
      timeout: 8000,
    }, res => {
      let body = '';
      res.on('data', c => body += c);
      res.on('end', () => {
        try { resolve({ status: res.statusCode, data: JSON.parse(body) }); }
        catch { resolve({ status: res.statusCode, data: null }); }
      });
    });
    req.on('error', reject);
    req.on('timeout', () => { req.destroy(); reject(new Error('Timeout')); });
  });
}

let githubCache = { fetchedAt: 0, data: null };
let githubPending;
function getGitHubSummary(force = false) {
  if (githubPending) return githubPending;
  githubPending = buildGitHubSummary(force).finally(() => { githubPending = null; });
  return githubPending;
}
async function buildGitHubSummary(force = false) {
  if (!force && githubCache.data && Date.now() - githubCache.fetchedAt < 300000) {
    return githubCache.data;
  }
  const results = await Promise.all(REPOS.map(async repo => {
    try {
      const [repoRes, issuesRes, pullsRes, relRes] = await Promise.all([
        fetchHttpsJson(`https://api.github.com/repos/hffgzktbcc-blip/${repo.slug}`),
        fetchHttpsJson(`https://api.github.com/repos/hffgzktbcc-blip/${repo.slug}/issues?state=open&per_page=15`),
        fetchHttpsJson(`https://api.github.com/repos/hffgzktbcc-blip/${repo.slug}/pulls?state=open&per_page=10`),
        fetchHttpsJson(`https://api.github.com/repos/hffgzktbcc-blip/${repo.slug}/releases/latest`),
      ]);
      const failed = [repoRes, issuesRes, pullsRes].find(r => r.status !== 200);
      if (failed) throw new Error(`GitHub data unavailable (HTTP ${failed.status}). Refresh to retry.`);
      const rawIssues = Array.isArray(issuesRes.data) ? issuesRes.data.filter(i => !i.pull_request) : [];
      const pulls = Array.isArray(pullsRes.data) ? pullsRes.data : [];
      return {
        project: repo.key,
        slug: repo.slug,
        license: repo.license,
        maturity: repo.maturity,
        scopeNote: repo.scopeNote || '',
        url: `https://github.com/hffgzktbcc-blip/${repo.slug}`,
        checkedAt: new Date().toISOString(),
        stars: repoRes.data?.stargazers_count ?? 0,
        openIssuesCount: rawIssues.length,
        openPullsCount: pulls.length,
        latestRelease: relRes.status === 200 && relRes.data?.tag_name ? relRes.data.tag_name : 'No release published',
        releaseStatus: relRes.status === 200 && relRes.data?.tag_name ? 'published' : 'unpublished',
        issues: rawIssues.slice(0, 10).map(i => ({
          number: i.number,
          title: i.title,
          url: i.html_url,
          updatedAt: i.updated_at,
          labels: (i.labels || []).map(l => l.name),
        })),
        pulls: pulls.slice(0, 10).map(p => ({
          number: p.number,
          title: p.title,
          url: p.html_url,
          branch: p.head?.ref,
          sha: p.head?.sha?.slice(0, 8),
        })),
      };
    } catch (e) {
      return { project: repo.key, slug: repo.slug, maturity: repo.maturity, scopeNote: repo.scopeNote || '', checkedAt: new Date().toISOString(), error: e.message, issues: [], pulls: [] };
    }
  }));
  const checkedAt = new Date().toISOString();
  const previous = new Map((githubCache.data?.repos || []).map(repo => [repo.project, repo]));
  const repos = results.map(repo => repo.error && previous.has(repo.project)
    ? { ...previous.get(repo.project), checkedAt: repo.checkedAt, stale: true, error: repo.error }
    : { ...repo, stale: Boolean(repo.error) });
  const failed = repos.filter(repo => repo.error).length;
  const source = {
    kind: 'github', label: 'GitHub live', checkedAt, stale: failed > 0, staleAfterMinutes: 10,
    lastSuccessfulAt: failed === 0 ? checkedAt : githubCache.data?.source?.lastSuccessfulAt || null,
    error: failed ? `${failed} of ${repos.length} projects could not be refreshed.` : null,
  };
  githubCache = { fetchedAt: Date.now(), data: { fetchedAt: checkedAt, source, repos } };
  return githubCache.data;
}

const GITHUB_REFRESH_MS = 5 * 60 * 1000;
const githubRefreshTimer = setInterval(() => getGitHubSummary(true).catch(error => {
  console.error('Background GitHub reconciliation failed:', error.message);
}), GITHUB_REFRESH_MS);
githubRefreshTimer.unref?.();

function recordsSource() {
  const updatedAt = store.records.map(record => record.updatedAt).filter(Boolean).sort().at(-1) || null;
  const staleAfterHours = 24;
  const stale = !updatedAt || Date.now() - new Date(updatedAt).getTime() > staleAfterHours * 60 * 60 * 1000;
  return { kind: 'manual', label: 'Maintained records', checkedAt: new Date().toISOString(), updatedAt, stale, staleAfterHours, error: null };
}

function verifyGitHubSignature(raw, signature) {
  const secret = process.env.GITHUB_WEBHOOK_SECRET || '';
  if (!secret || !signature?.startsWith('sha256=')) return false;
  const expected = `sha256=${crypto.createHmac('sha256', secret).update(raw).digest('hex')}`;
  if (signature.length !== expected.length) return false;
  return crypto.timingSafeEqual(Buffer.from(signature), Buffer.from(expected));
}

function sharedRead(loader, ttlMs) {
  let value, until = 0, pending;
  return () => {
    if (pending) return pending;
    if (value && Date.now() < until) return Promise.resolve(value);
    pending = loader().then(result => { value = result; until = Date.now() + ttlMs; return result; }).finally(() => { pending = null; });
    return pending;
  };
}
const readOracleStatus = sharedRead(getOracleStatus, 30000);
const readOrcaStatus = sharedRead(getOrcaStatus, 30000);

async function getOracleStatus() {
  const [freeRes, dfRes, pm2Res, repoStatuses] = await Promise.all([
    execPromise("free -m | awk 'NR==2{printf \"%s %s %s\", $2,$3,$7} NR==3{printf \" %s %s\", $2,$3}'"),
    execPromise("df -BG / | awk 'NR==2{print $2,$3,$4,$5}'"),
    execPromise("pm2 jlist 2>/dev/null || echo '[]'"),
    Promise.all(REPOS.map(async r => {
      if (!fs.existsSync(r.dir)) return { project: r.key, exists: false };
      const [branch, sha, dirty] = await Promise.all([
        execPromise(`git -C ${r.dir} rev-parse --abbrev-ref HEAD`),
        execPromise(`git -C ${r.dir} rev-parse --short HEAD`),
        execPromise(`git -C ${r.dir} status --porcelain`),
      ]);
      return {
        project: r.key,
        slug: r.slug,
        exists: true,
        branch: branch.stdout || 'unknown',
        sha: sha.stdout || 'unknown',
        dirty: Boolean(dirty.stdout),
      };
    })),
  ]);

  const memParts = (freeRes.stdout || '0 0 0 0 0').split(/\s+/).map(Number);
  const dfParts = (dfRes.stdout || '0G 0G 0G 0%').split(/\s+/);
  let pm2 = [];
  try {
    pm2 = JSON.parse(pm2Res.stdout || '[]').map(p => ({
      name: p.name,
      status: p.pm2_env?.status || 'unknown',
      cpu: p.monit?.cpu ?? 0,
      memoryMb: Math.round((p.monit?.memory ?? 0) / 1024 / 1024),
      restarts: p.pm2_env?.restart_time ?? 0,
    }));
  } catch {}

  return {
    hostname: os.hostname(),
    uptimeSec: Math.round(os.uptime()),
    loadAvg: os.loadavg().map(n => Number(n.toFixed(2))),
    memory: { totalMb: memParts[0] || 0, usedMb: memParts[1] || 0, availMb: memParts[2] || 0 },
    swap: { totalMb: memParts[3] || 0, usedMb: memParts[4] || 0 },
    disk: { total: dfParts[0], used: dfParts[1], avail: dfParts[2], pct: dfParts[3] },
    pm2,
    repos: repoStatuses,
    checkedAt: new Date().toISOString(),
  };
}

async function getOrcaStatus() {
  const [statusRes, projRes, wtRes, termRes] = await Promise.all([
    spawnPromise(ORCA_BIN, ['status','--json'], {timeout:8000}),
    spawnPromise(ORCA_BIN, ['project','list','--json'], {timeout:8000}),
    spawnPromise(ORCA_BIN, ['worktree','ps','--limit','12','--json'], {timeout:8000}),
    spawnPromise(ORCA_BIN, ['terminal','list','--json'], {timeout:8000}),
  ]);

  const safeParse = (txt) => { try { return JSON.parse(txt); } catch { return null; } };
  const statusJson = safeParse(statusRes.output);
  const projJson = safeParse(projRes.output);
  const wtJson = safeParse(wtRes.output);
  const termJson = safeParse(termRes.output);

  return {
    connected: Boolean(statusJson?.ok && statusJson?.result?.runtime?.reachable),
    runtime: statusJson?.result?.runtime ? {
      state: statusJson.result.runtime.state,
      version: statusJson.result.runtime.appVersion,
      runtimeId: statusJson.result.runtime.runtimeId,
    } : null,
    projects: projJson?.result?.projects || [],
    worktrees: wtJson?.result?.worktrees || [],
    terminals: termJson?.result?.terminals || [],
    checkedAt: new Date().toISOString(),
  };
}

function buildBotPrompt(bot, projectKey, userMessage) {
  const repoObj = REPOS.find(r => r.key === projectKey);
  const contextNote = repoObj
    ? `Target project: ${repoObj.key} (repo dir: ${repoObj.dir}, license: ${repoObj.license}).`
    : `Target scope: Ounce Family (all 5 repos in ${OUNCE_DIR}).`;

  const recentRecords = store.records
    .filter(r => !projectKey || projectKey === 'All' || r.project === projectKey)
    .slice(0, 8)
    .map(r => `- [${r.id}] (${r.status}, P${r.priority}) ${r.title}`)
    .join('\n');

  return `${bot.persona}

${COORDINATOR_CONTRACT}

${contextNote}
Current active queue items:
${recentRecords || '(none)'}

User from phone command centre says:
${userMessage}

Keep your reply concise, high-signal, and formatted cleanly for a phone screen.`;
}

function startBotJob({ botId, engine, projectKey, prompt }) {
  const bot = BOT_ROSTER.find(b => b.id === botId) || BOT_ROSTER[0];
  const jobId = `job_${Date.now()}_${crypto.randomBytes(3).toString('hex')}`;
  const repoObj = REPOS.find(r => r.key === projectKey);
  const cwd = (repoObj && fs.existsSync(repoObj.dir)) ? repoObj.dir : OUNCE_DIR;

  const fullPrompt = buildBotPrompt(bot, projectKey, prompt);
  const job = {
    id: jobId,
    botId: bot.id,
    botName: bot.name,
    engine,
    project: projectKey || 'All',
    prompt,
    status: 'running',
    output: '',
    startedAt: new Date().toISOString(),
    finishedAt: null,
  };
  activeJobs.set(jobId, job);

  // Record user message in chatHistory
  store.chatHistory.push({
    id: `msg_${Date.now()}_u`,
    role: 'user',
    botId: bot.id,
    engine,
    project: projectKey || 'All',
    text: prompt,
    at: new Date().toISOString(),
  });
  if (store.chatHistory.length > 150) store.chatHistory = store.chatHistory.slice(-150);
  saveStore();

  let cmd = '';
  let args = [];

  if (engine === 'codex') {
    cmd = 'codex';
    args = ['exec', '--skip-git-repo-check', '-s', 'read-only', fullPrompt];
  } else if (engine === 'openclaude') {
    cmd = 'openclaude';
    args = ['-p', fullPrompt];
  } else if (engine === 'claude') {
    cmd = 'claude';
    args = ['-p', fullPrompt];
  } else if (engine === 'opencode') {
    cmd = 'opencode';
    args = ['run', fullPrompt];
  } else if (engine === 'orca') {
    cmd = ORCA_BIN;
    args = ['status', '--json'];
  } else {
    cmd = 'codex';
    args = ['exec', '--skip-git-repo-check', '-s', 'read-only', fullPrompt];
  }

  const child = spawn(cmd, args, {
    cwd,
    env: { ...process.env, HOME: '/home/ubuntu', TERM: 'dumb' },
  });

  let stdout = '';
  let stderr = '';
  const timeoutHandle = setTimeout(() => {
    try { child.kill('SIGTERM'); } catch {}
    job.output += '\n[Timed out after 90s]';
  }, 90000);

  child.stdout.on('data', chunk => {
    stdout += chunk.toString();
    job.output = stdout;
  });
  child.stderr.on('data', chunk => {
    stderr += chunk.toString();
  });

  child.on('close', code => {
    clearTimeout(timeoutHandle);
    let finalText = stdout.trim();
    if (!finalText && stderr.trim()) {
      finalText = `⚠️ Engine (${engine}) exited with code ${code}:\n${stderr.trim().slice(0, 1200)}`;
    }
    if (!finalText) {
      finalText = `(${bot.name} finished with code ${code} and no text output.)`;
    }
    job.status = code === 0 ? 'complete' : 'error';
    job.output = finalText;
    job.finishedAt = new Date().toISOString();

    store.chatHistory.push({
      id: `msg_${Date.now()}_b`,
      role: 'bot',
      botId: bot.id,
      botName: bot.name,
      botEmoji: bot.emoji,
      engine,
      project: projectKey || 'All',
      text: finalText,
      status: job.status,
      at: job.finishedAt,
    });
    if (store.chatHistory.length > 150) store.chatHistory = store.chatHistory.slice(-150);
    saveStore();
  });

  child.on('error', err => {
    clearTimeout(timeoutHandle);
    job.status = 'error';
    job.output = `Failed to start ${engine}: ${err.message}`;
    job.finishedAt = new Date().toISOString();
  });

  return job;
}

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.webmanifest': 'application/manifest+json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

const server = http.createServer(async (req, res) => {
  const reqUrl = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
  const pathname = reqUrl.pathname;

  if (pathname === '/api/hooks/github' && req.method === 'POST') {
    if (!process.env.GITHUB_WEBHOOK_SECRET) return sendJson(res, 503, { ok: false, error: 'GitHub webhook is not configured.' });
    try {
      const raw = await readRawBody(req);
      if (!verifyGitHubSignature(raw, req.headers['x-hub-signature-256'])) return sendJson(res, 401, { ok: false, error: 'Invalid GitHub webhook signature.' });
      const event = req.headers['x-github-event'] || '';
      const payload = raw ? JSON.parse(raw) : {};
      const relevant = (event === 'pull_request' && payload.action === 'closed' && payload.pull_request?.merged)
        || (event === 'release' && ['published', 'released'].includes(payload.action))
        || (event === 'issues' && ['opened', 'closed', 'reopened', 'edited'].includes(payload.action))
        || event === 'push';
      if (relevant) getGitHubSummary(true).catch(error => console.error('Webhook reconciliation failed:', error.message));
      return sendJson(res, 202, { ok: true, accepted: relevant });
    } catch (error) {
      return sendJson(res, 400, { ok: false, error: error.message });
    }
  }

  // Auth endpoints (public)
  if (pathname === '/api/auth/login' && req.method === 'POST') {
    try {
      const body = await readBody(req);
      if ((body.pin || '') !== ACCESS_PIN) {
        return sendJson(res, 401, { ok: false, error: 'Invalid passcode' });
      }
      const token = signToken({ sub: 'owner', exp: Date.now() + 30 * 24 * 3600 * 1000 });
      const isHttps = req.headers['x-forwarded-proto'] === 'https' || req.socket.encrypted;
      const cookieFlags = `HttpOnly; Path=/; Max-Age=${30 * 24 * 3600}; SameSite=Lax${isHttps ? '; Secure' : ''}`;
      return sendJson(res, 200, { ok: true }, {
        'Set-Cookie': `ounce_cc_session=${encodeURIComponent(token)}; ${cookieFlags}`,
      });
    } catch (e) {
      return sendJson(res, 400, { ok: false, error: e.message });
    }
  }

  if (pathname === '/api/auth/logout' && req.method === 'POST') {
    return sendJson(res, 200, { ok: true }, {
      'Set-Cookie': 'ounce_cc_session=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax',
    });
  }

  if (pathname === '/api/auth/me' && req.method === 'GET') {
    return sendJson(res, 200, { ok: true, authenticated: isAuthenticated(req) });
  }

  // Protected API routes
  if (pathname.startsWith('/api/')) {
    if (!isAuthenticated(req)) {
      return sendJson(res, 401, { ok: false, error: 'Authentication required' });
    }

    if (pathname === '/api/roadmap' && req.method === 'GET') {
      try {
        if (!fs.existsSync(ROADMAP_PATH)) return sendJson(res, 200, { ok: true, quarters: [] });
        const quarters = JSON.parse(fs.readFileSync(ROADMAP_PATH, 'utf8'));
        return sendJson(res, 200, { ok: true, quarters });
      } catch (e) {
        return sendJson(res, 500, { ok: false, error: e.message });
      }
    }

    if (pathname === '/api/roadmap/toggle' && req.method === 'POST') {
      try {
        const body = await readBody(req);
        if (!fs.existsSync(ROADMAP_PATH)) return sendJson(res, 404, { ok: false, error: 'Roadmap not found' });
        const quarters = JSON.parse(fs.readFileSync(ROADMAP_PATH, 'utf8'));
        let found = false;
        for (const q of quarters) {
          for (const m of q.milestones) {
            if (m.id === body.id) {
              if (body.status && !['open','complete'].includes(body.status)) return sendJson(res, 400, { ok: false, error: 'Invalid milestone status' });
              m.status = body.status || (m.status === 'complete' ? 'open' : 'complete');
              found = true;
              break;
            }
          }
          if (found) break;
        }
        if (found) {
          fs.writeFileSync(ROADMAP_PATH, JSON.stringify(quarters, null, 2), 'utf8');
          return sendJson(res, 200, { ok: true, quarters });
        }
        return sendJson(res, 404, { ok: false, error: 'Milestone not found' });
      } catch (e) {
        return sendJson(res, 400, { ok: false, error: e.message });
      }
    }

        if (pathname === '/api/records' && req.method === 'GET') {
      return sendJson(res, 200, {
        ok: true,
        cloudWorker: store.cloudWorker,
        records: store.records,
        projects: PROJECTS,
        source: recordsSource(),
      });
    }

    if (pathname === '/api/records' && req.method === 'POST') {
      try {
        const body = await readBody(req);
        const now = new Date().toISOString();
        const existingIdx = body.id ? store.records.findIndex(r => r.id === body.id) : -1;
        if (existingIdx >= 0) {
          store.records[existingIdx] = {
            ...store.records[existingIdx],
            ...body,
            updatedAt: now,
            version: (store.records[existingIdx].version || 1) + 1,
          };
          saveStore();
          return sendJson(res, 200, { ok: true, record: store.records[existingIdx] });
        } else {
          const id = body.id || `ITEM-${Date.now().toString().slice(-5)}`;
          const rec = {
            id,
            version: 1,
            project: PROJECT_KEYS.includes(body.project) ? body.project : 'Ounce-Core',
            kind: body.kind || 'task',
            title: body.title || 'Untitled item',
            detail: body.detail || '',
            category: body.category || 'verification',
            status: body.status || 'open',
            priority: Number(body.priority || 2),
            owner: body.owner || 'Owner',
            url: body.url || '',
            public: false,
            updatedAt: now,
            botRole: body.botRole || '',
            executionProvider: body.executionProvider || 'codex',
          };
          store.records.unshift(rec);
          saveStore();
          return sendJson(res, 201, { ok: true, record: rec });
        }
      } catch (e) {
        return sendJson(res, 400, { ok: false, error: e.message });
      }
    }

    if (pathname.startsWith('/api/records/') && req.method === 'DELETE') {
      const id = decodeURIComponent(pathname.slice('/api/records/'.length));
      store.records = store.records.filter(r => r.id !== id);
      saveStore();
      return sendJson(res, 200, { ok: true });
    }

    if (pathname === '/api/github' && req.method === 'GET') {
      const force = reqUrl.searchParams.get('force') === '1';
      const data = await getGitHubSummary(force);
      return sendJson(res, 200, { ok: true, ...data });
    }

    if (pathname === '/api/oracle/status' && req.method === 'GET') {
      const data = await readOracleStatus();
      return sendJson(res, 200, { ok: true, ...data });
    }

    if (pathname === '/api/oracle/action' && req.method === 'POST') {
      const body = await readBody(req);
      if (body.action === 'git-pull-all') {
        const out = await Promise.all(REPOS.map(async r => {
          const resPull = await execPromise(`git -C ${r.dir} pull --ff-only 2>&1`);
          return { project:r.key, ok:resPull.ok, output:resPull.stdout || resPull.stderr || 'No output' };
        }));
        return sendJson(res, 200, { ok: out.every(r=>r.ok), output: out.map(r=>`${r.project}: ${r.output}`).join('\n'), error: out.some(r=>!r.ok) ? 'One or more repositories could not be pulled. Check each checkout.' : undefined });
      }
      if (body.action === 'restart-orcad') {
        const r = await execPromise('pm2 restart orcad 2>&1');
        return sendJson(res, 200, { ok: r.ok, output: r.stdout || r.stderr });
      }
      return sendJson(res, 400, { ok: false, error: 'Unknown action' });
    }

    if (pathname === '/api/orca/status' && req.method === 'GET') {
      const data = await readOrcaStatus();
      return sendJson(res, 200, { ok: true, ...data });
    }

    if (pathname === '/api/orca/dispatch' && req.method === 'GET') {
      return sendJson(res, 200, { ok: true, ...dispatchOptions(), jobs: dispatchJobs });
    }
    if (pathname.startsWith('/api/orca/jobs/') && req.method === 'GET') {
      const id = pathname.slice('/api/orca/jobs/'.length);
      const job = dispatchJobs.find(j => j.id === id);
      return sendJson(res, job ? 200 : 404, job ? { ok: true, job } : { ok: false, error: 'Run not found. Refresh to see retained runs.' });
    }

    if (pathname === '/api/orca/dispatch' && req.method === 'POST') {
      try {
        const body = await readBody(req);
        if (body.requestId && !/^[a-zA-Z0-9_-]{8,100}$/.test(body.requestId)) return sendJson(res, 400, { ok: false, error: 'Invalid request id' });
        const previous = body.requestId && dispatchJobs.find(j => j.requestId === body.requestId);
        if (previous) return sendJson(res, 202, { ok: true, job: previous });
        if (dispatchJobs.filter(j => j.status === 'running').length >= 2) return sendJson(res, 409, { ok: false, error: 'Two tasks are already running. Wait for one to finish.' });
        const worktree = body.worktree || 'command-centre-handoff';
        const agent = body.agent || 'openclaude';
        const prompt = body.prompt ?? '';
        if (typeof prompt !== 'string') return sendJson(res, 400, { ok:false, error:'Task must be text' });
        const wtDir = dispatchTarget(worktree);
        if (!wtDir || !fs.existsSync(wtDir)) return sendJson(res, 400, { ok: false, error: 'Worktree is unavailable. Choose another target.' });
        const commands = {
          bash: ['bash', ['-lc', String(prompt)]],
          openclaude: ['openclaude', ['-p', String(prompt)]],
          claude: ['openclaude', ['-p', String(prompt)]],
          codex: ['codex', ['exec', '--skip-git-repo-check', '-s', 'read-only', String(prompt)]],
          opencode: ['opencode', ['run', String(prompt)]],
        };
        if (!Object.hasOwn(commands, agent)) return sendJson(res, 400, { ok: false, error: 'Unknown agent engine' });
        if (!String(prompt).trim() || String(prompt).length > 20000) return sendJson(res, 400, { ok: false, error: 'Enter a task of 1–20,000 characters.' });
        const [command, args] = commands[agent];
        const job = { id: crypto.randomUUID(), requestId: body.requestId || null, agent, worktree, prompt: String(prompt), status: 'running', output: '', startedAt: new Date().toISOString(), finishedAt: null };
        dispatchJobs.unshift(job);
        // Keep active jobs even when trimming the retained history.
        dispatchJobs = [...dispatchJobs.filter(j => j.status === 'running'), ...dispatchJobs.filter(j => j.status !== 'running')].slice(0, 20);
        saveDispatchJobs();
        let lastSaved = 0;
        spawnPromise(command, args, { cwd: wtDir, timeout: 120000, onOutput(output) {
          job.output = output;
          if (Date.now() - lastSaved > 1000) { try { saveDispatchJobs(); } catch (e) { console.error('Could not retain run output:', e.message); } lastSaved = Date.now(); }
        } }).then(result => {
          job.status = result.ok ? 'complete' : 'error';
          job.output = result.output;
          job.error = result.error || (result.ok ? null : `Process exited with code ${result.code}`);
          job.finishedAt = new Date().toISOString();
          try { saveDispatchJobs(); } catch (e) { console.error('Could not retain run result:', e.message); }
        }).catch(error => {
          job.status = 'error'; job.error = error.message; job.finishedAt = new Date().toISOString();
          try { saveDispatchJobs(); } catch {}
        });
        return sendJson(res, 202, { ok: true, job });

      } catch (e) {
        return sendJson(res, 500, { ok: false, error: e.message });
      }
    }

        if (pathname === '/api/orca/action' && req.method === 'POST') {
      const body = await readBody(req);
      if (body.action === 'terminal-send' && body.terminalId && body.text) {
        const r = await spawnPromise(ORCA_BIN, ['terminal', 'send', '--terminal', String(body.terminalId), '--text', String(body.text), '--enter', '--json']);
        return sendJson(res, 200, { ok: r.ok, output: r.output, error: r.error });
      }
      return sendJson(res, 400, { ok: false, error: 'Unsupported Orca action' });
    }

    if (pathname === '/api/bots/roster' && req.method === 'GET') {
      return sendJson(res, 200, {
        ok: true,
        bots: BOT_ROSTER,
        engines: ['codex', 'openclaude', 'claude', 'opencode', 'orca'].map(id => ({ id, label: id === 'orca' ? 'Orca status (no AI)' : id, installed: engineInstalled(id === 'orca' ? ORCA_BIN : id) })),
        activeJobs: [...activeJobs.values()].filter(j => j.status === 'running'),
        history: store.chatHistory.slice(-50),
      });
    }

    if (pathname === '/api/bots/clear' && req.method === 'POST') {
      store.chatHistory = [];
      saveStore();
      return sendJson(res, 200, { ok: true });
    }

    if (pathname === '/api/bots/chat' && req.method === 'POST') {
      try {
        const body = await readBody(req);
        if (!body.prompt || !String(body.prompt).trim()) {
          return sendJson(res, 400, { ok: false, error: 'Prompt is required' });
        }
        if (!['codex', 'openclaude', 'claude', 'opencode', 'orca'].includes(body.engine || 'codex')) return sendJson(res, 400, { ok: false, error: 'Unsupported engine' });
        const job = startBotJob({
          botId: body.botId || 'grok-core',
          engine: body.engine || 'codex',
          projectKey: body.project || 'All',
          prompt: String(body.prompt).trim(),
        });
        return sendJson(res, 202, { ok: true, job });
      } catch (e) {
        return sendJson(res, 400, { ok: false, error: e.message });
      }
    }

    if (pathname.startsWith('/api/bots/jobs/') && req.method === 'GET') {
      const id = pathname.slice('/api/bots/jobs/'.length);
      const job = activeJobs.get(id);
      if (!job) return sendJson(res, 404, { ok: false, error: 'Job not found' });
      return sendJson(res, 200, { ok: true, job });
    }

    if (pathname === '/api/patrol/run' && req.method === 'POST') {
      const result = await runPatrol({ getGitHubSummary, store, saveStore, ntfyUrl: NTFY_URL, notifyAlways: true });
      return sendJson(res, 200, result);
    }

    if (pathname === '/api/ntfy/test' && req.method === 'POST') {
      const r = await sendNtfy({
        topicUrl: NTFY_URL,
        title: 'Ounce Command Centre Connected',
        message: 'Your Grok Bot crew on Oracle VM is wired up to ntfy!',
        tags: 'rocket,white_check_mark',
      });
      return sendJson(res, 200, { ok: r.ok, ...r });
    }

    return sendJson(res, 404, { ok: false, error: 'API route not found' });
  }

  // Serve static files
  let safePath = pathname === '/' ? '/index.html' : pathname;
  safePath = path.normalize(safePath).replace(/^(\.\.[/\\])+/, '');
  let filePath = path.join(PUBLIC_DIR, safePath);
  if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    filePath = path.join(PUBLIC_DIR, 'index.html');
  }
  const ext = path.extname(filePath);
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';
  res.writeHead(200, { 'Content-Type': contentType, 'Cache-Control': 'no-cache' });
  fs.createReadStream(filePath).pipe(res);
});

server.keepAliveTimeout = 95000;
server.headersTimeout = 96000;
server.listen(PORT, HOST, () => {
  console.log(`Ounce Family Command Centre running at http://${HOST}:${PORT}`);
  // Initial baseline patrol + hourly unattended check
  setTimeout(() => runPatrol({ getGitHubSummary, store, saveStore, ntfyUrl: NTFY_URL, notifyAlways: false }).catch(() => {}), 5000);
  setInterval(() => runPatrol({ getGitHubSummary, store, saveStore, ntfyUrl: NTFY_URL, notifyAlways: false }).catch(() => {}), 60 * 60 * 1000);
});
