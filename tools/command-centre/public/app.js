(() => {
  const FALLBACK_PROJECTS = [
    { key: 'Ounce-Core', license: 'MIT', maturity: 'established' },
    { key: 'Ounce-HB', license: 'MIT', maturity: 'established' },
    { key: 'Ounce-Runtime', license: 'GPL-3.0-or-later', maturity: 'established' },
    { key: 'DeckStream', license: 'MIT', maturity: 'established' },
    { key: 'Ounce-Spark', license: 'MIT', maturity: 'experimental', scopeNote: 'CPU-based PTX research; no GPU acceleration or hardware-validated Blackwell performance.' },
  ];
  const state = {
    tab: 'home',
    queueKind: 'task',
    queueLimit: 20,
    chatDraft: '',
    posting: false,
    sourceErrors: [],
    queueLoaded: false,
    roadmapLoaded: false,
    dispatchLoaded: false,
    bootLoading: false,
    syncedAt: null,
    dispatch: { targets: [], engines: [], jobs: [] },
    dispatchDraft: '',
    dispatchTarget: 'command-centre-handoff',
    dispatchAgent: 'bash',
    dispatchSubmitting: false,
    roadmap: [],
    records: [],
    projects: FALLBACK_PROJECTS,
    recordSource: null,
    cloudWorker: {},
    github: null,
    oracle: null,
    orca: null,
    roster: [],
    engines: [],
    chatHistory: [],
    selectedBot: 'grok-core',
    selectedEngine: 'codex',
    selectedProject: 'All',
    activeJobId: null,
    activeJobOutput: '',
    queueFilterProject: 'All',
    queueFilterStatus: 'All',
    queueSearch: '',
  };

  const $ = id => document.getElementById(id);

  let noticeTimer;
  function notice(message, error = false) {
    const el = $('notice');
    if (!el) return;
    el.textContent = message;
    el.classList.remove('hidden');
    el.classList.toggle('error', error);
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => el.classList.add('hidden'), 6000);
  }
  async function api(url, opts = {}) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 20000);
    try {
      const res = await fetch(url, { ...opts, headers: { 'Content-Type': 'application/json', ...(opts.headers || {}) }, signal: controller.signal });
      if (res.status === 401 && url !== '/api/auth/login') { showLogin(); throw new Error('Session expired. Unlock to continue.'); }
      let data;
      try { data = await res.json(); } catch { throw new Error(`Server returned an unreadable response (${res.status}). Try refreshing.`); }
      if (!res.ok || data.ok === false) throw Object.assign(new Error(data.error || `Request failed (${res.status})`), { data });
      return data;
    } catch (error) {
      if (error.name === 'AbortError') throw new Error('The request timed out. Refresh to check the result before retrying.');
      throw error;
    } finally { clearTimeout(timer); }
  }
  async function action(button, task) {
    const label = button?.textContent;
    if (button) { button.disabled = true; button.setAttribute('aria-busy', 'true'); }
    try { return await task(); }
    catch (error) { notice(error.message, true); }
    finally { if (button) { button.disabled = false; button.removeAttribute('aria-busy'); if (label) button.textContent = label; } }
  }
  function localGet(key) { try { return window.localStorage.getItem('ounce-' + key); } catch { return null; } }
  function localSet(key, value) { try { window.localStorage.setItem('ounce-' + key, value); } catch {} }
  function restoreProject() {
    const saved=localGet('chat-project');
    state.selectedProject=['All',...state.projects.map(project=>project.key)].includes(saved)?saved:'All';
  }
  function age(value) {
    if (!value) return 'Not synced';
    const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
    return seconds < 60 ? 'Just updated' : `${Math.floor(seconds / 60)} min ago`;
  }

  function esc(str) {
    return String(str ?? '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function showLogin() {
    $('connectionGate')?.classList.add('hidden');
    state.authenticated=false;
    clearTimeout(dispatchPollTimer);clearTimeout(botPollTimer);
    $('loginGate').classList.remove('hidden');
    $('appShell').classList.add('hidden');
  }

  function showApp() {
    $('connectionGate')?.classList.add('hidden');
    state.authenticated=true;
    $('loginGate').classList.add('hidden');
    $('appShell').classList.remove('hidden');
  }

  async function init() {
    let me;
    $('connectionGate').classList.remove('hidden');
    $('retryConnectionBtn').classList.add('hidden');
    $('connectionMessage').textContent='Checking your saved session…';
    try { me=await api('/api/auth/me'); } catch(error) {
      $('connectionMessage').textContent='Oracle is taking too long to respond. Your saved session has not been cleared. Retry when the connection recovers.';
      $('retryConnectionBtn').classList.remove('hidden');
      $('retryConnectionBtn').onclick=()=>init();
      return;
    }
    if (!me.authenticated) {
      showLogin();
      return;
    }
    showApp();
    state.bootLoading=true;
    state.chatDraft = localGet('chat-draft') || '';
    restoreProject();
    const tab = localGet('tab');
    if (['home','queue','roadmap','bots','infra','github','claims','releases','decisions'].includes(tab)) state.tab = tab;
    $('viewContainer').innerHTML = '<div class="empty-state" role="status">Loading your command centre…</div>';
    await loadAll(false);
    render();
    resumeRuns();
  }

  async function loadAll(forceGithub = false) {
    const progressive = apply => value => { apply(value); if(state.bootLoading&&state.authenticated)render(); return value; };
    const [recRes, roadmapRes, botRes, oracleRes, orcaRes, ghRes, dispatchRes] = await Promise.allSettled([
      api('/api/records').then(progressive(value=>{state.records=value.records||[];state.cloudWorker=value.cloudWorker||{};state.projects=value.projects?.length?value.projects:FALLBACK_PROJECTS;state.recordSource=value.source||null;state.queueLoaded=true;})),
      api('/api/roadmap').then(progressive(value=>{state.roadmap=value.quarters||[];state.roadmapLoaded=true;})),
      api('/api/bots/roster').then(progressive(value=>{state.roster=value.bots||[];state.engines=value.engines||[];state.chatHistory=value.history||[];})),
      state.tab === 'infra' ? api('/api/oracle/status') : Promise.resolve({ok:false,skipped:true}),
      state.tab === 'infra' ? api('/api/orca/status') : Promise.resolve({ok:false,skipped:true}),
      api(`/api/github${forceGithub ? '?force=1' : ''}`),
      api('/api/orca/dispatch').then(progressive(value=>{state.dispatch=value;state.dispatchLoaded=true;})),
    ]);

    if (recRes.status === 'fulfilled' && recRes.value.ok) {
      state.records = recRes.value.records || [];
      state.cloudWorker = recRes.value.cloudWorker || {};
      state.projects = recRes.value.projects?.length ? recRes.value.projects : FALLBACK_PROJECTS;
      state.recordSource = recRes.value.source || null;
    }
    if (roadmapRes.status === 'fulfilled' && roadmapRes.value.ok) {
      state.roadmap = roadmapRes.value.quarters || [];
    }
    if (botRes.status === 'fulfilled' && botRes.value.ok) {
      state.roster = botRes.value.bots || [];
      state.engines = botRes.value.engines || [];
      state.chatHistory = botRes.value.history || [];
    }
    if (oracleRes.status === 'fulfilled' && oracleRes.value.ok) {
      state.oracle = oracleRes.value;
    }
    if (orcaRes.status === 'fulfilled' && orcaRes.value.ok) {
      state.orca = orcaRes.value;
    }
    if (ghRes.status === 'fulfilled' && ghRes.value.ok) {
      state.github = ghRes.value;
    }

    if (dispatchRes.status === 'fulfilled') state.dispatch = dispatchRes.value;
    state.bootLoading=false;
    const sources = [['Queue',recRes],['Plan',roadmapRes],['Agents',botRes],['Oracle',oracleRes],['Orca',orcaRes],['GitHub',ghRes],['Runs',dispatchRes]];
    state.sourceErrors = sources.filter(([,r]) => r.status === 'rejected').map(([name]) => name);
    if (sources.some(([,r]) => r.status === 'fulfilled' && !r.value.skipped)) state.syncedAt = new Date().toISOString();
    if (botRes.status === 'fulfilled') {
      const running = botRes.value.activeJobs?.[0];
      if (running) { state.activeJobId = running.id; state.activeJobOutput = running.output; }
    }
    updateSyncStatus();
    const openTasks = state.records.filter(r => r.kind === 'task' && !['complete','deferred'].includes(r.status)).length;
    if ($('sideQueueCount')) $('sideQueueCount').textContent = openTasks;
  }

  function updateSyncStatus() {
    const el = $('syncStatus'); if (!el) return;
    el.textContent = state.sourceErrors.length ? `Could not refresh ${state.sourceErrors.join(', ')}. Showing any previously loaded data. Use Refresh to retry.` : `${age(state.syncedAt)} · Updates while this app is open`;
    el.classList.toggle('warning', state.sourceErrors.length > 0);
  }
  function sourceStatus(source, fallbackLabel) {
    if (!source) return `<div class="source-status unavailable"><strong>${esc(fallbackLabel)}</strong><span>Unavailable · use Refresh to retry</span></div>`;
    const stamp = source.updatedAt || source.checkedAt;
    const detail = source.error || (source.stale ? (source.updatedAt ? `Stale · last changed ${age(source.updatedAt)}` : 'Never updated') : `Checked ${age(source.checkedAt || stamp)}`);
    return `<div class="source-status ${source.error?'error':source.stale?'stale':'fresh'}"><strong>${esc(source.label || fallbackLabel)}</strong><span>${esc(detail)}</span></div>`;
  }
  function setTab(tab) {
    if ($('chatInput')) state.chatDraft = $('chatInput').value;
    if(tab==='bots')localSet('chat-project',state.selectedProject);
    state.tab = tab;
    localSet('tab', tab);
    closeModal();
    render();
    window.scrollTo?.({ top: 0 });
    if(tab==='infra')loadAll(false).then(()=>{if(state.tab==='infra')render();}).catch(error=>notice(error.message,true));
  }

  function statusPill(status) {
    const s = String(status || 'open').toLowerCase();
    if (['complete', 'done', 'approved', 'merged', 'online', 'ready'].includes(s)) return `<span class="pill ok">${esc(status)}</span>`;
    if (['in_progress', 'running', 'proposed', 'submitted', 'reported'].includes(s)) return `<span class="pill info">${esc(status)}</span>`;
    if (['blocked', 'error', 'interrupted', 'unavailable', 'not-established'].includes(s)) return `<span class="pill bad">${esc(status)}</span>`;
    return `<span class="pill warn">${esc(status)}</span>`;
  }

  function render() {
    if ($('chatInput')) state.chatDraft = $('chatInput').value;
    const titles = {
      bots: ['Agents', 'Ask, inspect and run work on Oracle'],
      roadmap: ['Plan', 'Your next year, one milestone at a time'],
      home: ['Home', 'Ounce Family command centre'],
      queue: ['Queue', 'Find the next action'],
      infra: ['Oracle & Orca', 'Live VM & runtime control'],
      github: ['GitHub Live', 'hffgzktbcc-blip repositories'],
      claims: ['Evidence & Claims', 'Strict reproduction tracking'],
      releases: ['Release Gates', '7-gate milestone readiness'],
      decisions: ['Owner Decisions', 'Retained private directives'],
    };
    const [t, sub] = titles[state.tab] || ['Command Centre', 'Ounce Family'];
    $('pageTitle').textContent = t;
    $('pageSub').textContent = sub;

    document.querySelectorAll('[data-tab]').forEach(el => {
      el.classList.toggle('active', el.dataset.tab === state.tab);
      if (el.dataset.tab === state.tab) el.setAttribute('aria-current', 'page'); else el.removeAttribute('aria-current');
    });

    $('moreNavBtn').classList.toggle('active', !['home','queue','roadmap','bots'].includes(state.tab));
    const container = $('viewContainer');
    if (state.tab === 'bots') renderBots(container);
    else if (state.tab === 'roadmap') renderRoadmap(container);
    else if (state.tab === 'home') renderHome(container);
    else if (state.tab === 'queue') renderQueue(container);
    else if (state.tab === 'infra') renderInfra(container);
    else if (state.tab === 'github') renderGitHub(container);
    else if (state.tab === 'claims') renderClaims(container);
    else if (state.tab === 'releases') renderReleases(container);
    else if (state.tab === 'decisions') renderDecisions(container);
  }

  function renderBots(el) {
    const activeBot = state.roster.find(b => b.id === state.selectedBot) || state.roster[0] || { name: 'Grok-Core', emoji: '⚡', tagline: '' };
    const quickChips = [
      'Give me a snappy status report across all 5 repos',
      'Check open PRs, CI status, and any blockers',
      'Audit Oracle VM health and Orca runtime',
      'What should I focus on first today?',
      'Roast our blocked release gates & tell me what is missing',
    ];

    el.innerHTML = `
      ${dispatcherMarkup()}
      <div class="section-heading"><h3>Ask an agent</h3><span>Choose a role and project</span></div>
      <div class="bot-strip">
        ${state.roster.map(b => `
          <button class="bot-chip ${b.id === state.selectedBot ? 'active' : ''}" data-pick-bot="${esc(b.id)}">
            <span>${b.emoji}</span><span>${esc(b.name)}</span>
          </button>
        `).join('')}
      </div>

      <div class="card" style="padding:11px 13px;margin-top:4px;">
        <div class="card-head" style="margin-bottom:4px;">
          <strong>${activeBot.emoji || '⚡'} ${esc(activeBot.name)} · <span style="color:var(--accent)">${esc(activeBot.role || '')}</span></strong>
          <button id="clearChatBtn" class="btn btn-sm">Clear chat</button>
        </div>
        <div style="font-size:0.82rem;color:var(--text-muted);">${esc(activeBot.tagline)}</div>
        <div class="chat-controls">
          <select id="engineSelect" aria-label="Agent engine">
            ${state.engines.length?'':'<option value="">Engine data unavailable</option>'}
            ${state.engines.map(e => `<option value="${esc(e.id)}" ${e.installed === false ? 'disabled' : ''} ${e.id === state.selectedEngine ? 'selected' : ''}>Engine: ${esc(e.label)}${e.installed === false ? ' (not installed)' : ''}</option>`).join('')}
          </select>
          <select id="projectSelect" aria-label="Project scope">
            <option value="All" ${state.selectedProject === 'All' ? 'selected' : ''}>Scope: All 5 Repos</option>
            ${state.projects.map(project=>`<option value="${esc(project.key)}" ${state.selectedProject===project.key?'selected':''}>${esc(project.key)} (${project.license==='GPL-3.0-or-later'?'GPLv3+':project.license}${project.maturity==='experimental'?' · experimental':''})</option>`).join('')}
          </select>
        </div>
        <div class="quick-prompts">
          ${quickChips.map(q => `<button class="quick-chip" data-quick="${esc(q)}">${esc(q)}</button>`).join('')}
        </div>
      </div>

      <div id="chatBox" class="chat-box" aria-label="Agent conversation">
        ${state.chatHistory.length === 0 ? `<div style="color:var(--text-muted);text-align:center;padding:30px 10px;">Choose a prompt or ask a question about your projects.</div>` : ''}
        ${state.chatHistory.map(m => `
          <div class="msg ${m.role === 'user' ? 'user' : 'bot'}">
            <div class="msg-meta">
              <strong>${m.role === 'user' ? 'You' : `${m.botEmoji || '⚡'} ${esc(m.botName || m.botId)}`}</strong>
              <span>· ${esc(m.engine)} · ${esc(m.project)}</span>
            </div>
            <div>${esc(m.text)}</div>
          </div>
        `).join('')}
        ${state.activeJobId ? `
          <div class="msg bot" id="activeBotMessage">
            <div class="msg-meta"><strong>${activeBot.emoji} ${esc(activeBot.name)}</strong><span>· running on ${esc(state.selectedEngine)}...</span></div>
            <div>${esc(state.activeJobOutput || 'Thinking & checking Oracle VM...')}</div>
          </div>
        ` : ''}
      </div>

      <form id="chatForm" class="chat-composer">
        <textarea id="chatInput" aria-label="Message to agent" placeholder="What would you like to check?">${esc(state.chatDraft)}</textarea>
        <button type="submit" class="btn btn-primary" ${state.activeJobId || state.posting || !state.engines.some(e=>e.id===state.selectedEngine&&e.installed!==false) ? 'disabled' : ''}>Send</button>
      </form>
    `;

    bindDispatcher();
    $('chatInput').oninput = e => { state.chatDraft = e.target.value; localSet('chat-draft', state.chatDraft); };
    const box = $('chatBox');
    if (box) box.scrollTop = box.scrollHeight;

    el.querySelectorAll('[data-pick-bot]').forEach(btn => {
      btn.onclick = () => { state.selectedBot = btn.dataset.pickBot; render(); };
    });
    $('engineSelect').onchange = e => { state.selectedEngine = e.target.value; };
    $('projectSelect').onchange = e => { state.selectedProject = e.target.value; localSet('chat-project',state.selectedProject); };
    $('clearChatBtn').onclick = () => action($('clearChatBtn'),async()=>{
      if(!window.confirm('Clear the saved conversation history?'))return;
      await api('/api/bots/clear',{method:'POST'});state.chatHistory=[];render();
    });
    el.querySelectorAll('[data-quick]').forEach(btn => {
      btn.onclick = () => sendBotPrompt(btn.dataset.quick);
    });
    $('chatForm').onsubmit = e => {
      e.preventDefault();
      const txt = $('chatInput').value.trim();
      if (!txt) return;
      sendBotPrompt(txt);
    };
  }

  async function sendBotPrompt(prompt) {
    if (state.activeJobId || state.posting) return;
    state.posting = true;
    const submit = $('chatForm')?.querySelector('[type=submit]'); if (submit) submit.disabled = true;
    try {
      const res = await api('/api/bots/chat', {
        method: 'POST',
        body: JSON.stringify({
          botId: state.selectedBot,
          engine: state.selectedEngine,
          project: state.selectedProject,
          prompt,
        }),
      });
      state.chatDraft = ''; localSet('chat-draft', ''); if ($('chatInput')) $('chatInput').value = '';
      state.activeJobId = res.job.id;
      state.activeJobOutput = '';
      state.chatHistory.push({
        role: 'user',
        botId: state.selectedBot,
        engine: state.selectedEngine,
        project: state.selectedProject,
        text: prompt,
      });
      render();
      pollJob(res.job.id);
    } catch (e) {
      notice('Could not send: ' + e.message, true);
    } finally { state.posting = false; if (submit && !state.activeJobId) submit.disabled = false; }
  }

  async function pollJob(jobId) {
    try {
      const res = await api(`/api/bots/jobs/${jobId}`);
      if (!res.ok) { state.activeJobId = null; render(); return; }
      const job = res.job;
      state.activeJobOutput = job.output || '';
      if (job.status === 'running') {
        if (state.tab === 'bots') {
          const message = $('activeBotMessage');
          if (message) { const atBottom = $('chatBox').scrollHeight - $('chatBox').scrollTop - $('chatBox').clientHeight < 60; message.lastElementChild.textContent = state.activeJobOutput || 'Working…'; if (atBottom) $('chatBox').scrollTop = $('chatBox').scrollHeight; }
        }
        clearTimeout(botPollTimer);botPollTimer=setTimeout(() => {if(state.authenticated)pollJob(jobId);},1500);
      } else {
        state.activeJobId = null;
        await loadAll(false);
        if(['bots','home'].includes(state.tab))render();
      }
    } catch (error) {
      notice('Run status unavailable. Refresh to reconnect; do not resend the task.', true);
      if(state.authenticated)botPollTimer=setTimeout(() => pollJob(jobId),5000);
    }
  }

  function renderHome(el) {
    const queueKnown=state.queueLoaded||state.records.length>0, planKnown=state.roadmapLoaded||state.roadmap.length>0;
    const tasks = state.records.filter(r => r.kind === 'task' && !['complete','deferred'].includes(r.status));
    const blocked = tasks.filter(r => r.status === 'blocked');
    const decisions = state.records.filter(r => r.kind === 'decision' && r.status !== 'complete');
    const next = [...tasks].sort((a,b) => (Number(a.priority)||2)-(Number(b.priority)||2)).slice(0,4);
    const milestones = state.roadmap.flatMap(q => q.milestones || []);
    const complete = milestones.filter(m => m.status === 'complete').length;
    const jobs = state.dispatch.jobs || [];
    el.innerHTML = `
      <section class="home-intro"><div><h3>Your next move.</h3><p>${!queueKnown ? 'Queue unavailable. Use Refresh to load your tasks.' : tasks.length ? `${tasks.length} active tasks across the family. Start with the work that needs you.` : 'Your queue is clear. Review the plan or add your next task.'}</p></div><button class="btn btn-primary" data-home-tab="queue">Open queue</button></section>
      <div class="attention-strip">
        <button data-home-tab="queue" data-blocked="true"><strong>${queueKnown?blocked.length:'—'}</strong><span>Blocked tasks</span></button>
        <button data-home-tab="decisions"><strong>${queueKnown?decisions.length:'—'}</strong><span>Decisions to review</span></button>
        <button data-home-tab="roadmap"><strong>${planKnown?`${complete}/${milestones.length}`:'—'}</strong><span>Plan milestones done</span></button>
      </div>
      <section class="source-grid" aria-label="Data freshness">${sourceStatus(state.github?.source,'GitHub live')}${sourceStatus(state.recordSource,'Maintained records')}</section>
      <section class="card"><div class="card-head"><h3 class="card-title">Needs attention</h3><span class="muted">Priority first</span></div>
      ${next.length ? next.map(r => `<button class="attention-row" data-open-item="${esc(r.id)}"><span><small>${esc(r.project)} · ${esc(r.id)}</small><strong>${esc(r.title)}</strong></span>${statusPill(r.status)}</button>`).join('') : `<div class="empty-state">${queueKnown?'No active tasks. Add an item when you have something to track.':'Your queue has not loaded. Use Refresh to retry.'}</div>`}</section>
      <section class="card"><div class="card-head"><h3 class="card-title">Recent runs</h3><button class="btn btn-sm" data-home-tab="bots">Open agents</button></div>
      ${jobs.length ? jobs.slice(0,3).map(j => `<button type="button" class="run-row" data-home-run="${esc(j.id)}"><span><strong>${esc(j.worktree.replace('ounce-family/',''))}</strong><small>${esc(j.agent)} · ${age(j.startedAt)}</small></span>${statusPill(j.status)}</button>`).join('') : `<p class="muted">${state.dispatchLoaded?'No command runs yet. Open Agents to run a check on a worktree.':'Run history has not loaded. Use Refresh to retry.'}</p>`}</section>
      <section class="card"><div class="card-head"><h3 class="card-title">Projects</h3><button class="btn btn-sm" data-home-tab="github">GitHub details</button></div>
      ${(state.github?.repos || []).map(r => `<a class="project-row" href="${esc(r.url)}" target="_blank" rel="noopener noreferrer"><strong>${esc(r.project)}</strong><span>${r.error ? 'Status unavailable' : r.releaseError ? `${esc(r.latestRelease)} last known · release check failed` : `${esc(r.latestRelease)} · ${r.openPullsCount} PRs listed`}</span><span aria-hidden="true">↗</span></a>`).join('') || '<p class="muted">GitHub data has not loaded. Use Refresh to retry.</p>'}</section>`;
    el.querySelectorAll('[data-home-tab]').forEach(button => { button.onclick = () => { if (button.dataset.blocked) state.queueFilterStatus = 'blocked'; setTab(button.dataset.homeTab); }; });
    el.querySelectorAll('[data-home-run]').forEach(button => { button.onclick = () => { state.selectedRunId=button.dataset.homeRun; setTab('bots'); const panel=document.querySelector('.dispatch-card'); panel?.setAttribute('open',''); $('dispatchOutput')?.scrollIntoView?.({block:'nearest'}); }; });
    el.querySelectorAll('[data-open-item]').forEach(button => { button.onclick = () => openRecordModal(state.records.find(r => r.id === button.dataset.openItem)); });
  }

  function queueRecords() {
    const query = state.queueSearch.toLowerCase();
    return state.records.filter(r => (state.queueKind === 'All' || r.kind === state.queueKind) && (state.queueFilterProject === 'All' || r.project === state.queueFilterProject) && (state.queueFilterStatus === 'All' || r.status === state.queueFilterStatus) && (!query || [r.id,r.title,r.detail,r.project,r.revision,r.ciHead,r.result].join(' ').toLowerCase().includes(query))).sort((a,b) => (Number(a.priority)||2)-(Number(b.priority)||2));
  }
  function queueResultsMarkup() {
    if(!state.queueLoaded&&!state.records.length)return '<div class="empty-state"><h3>Queue unavailable</h3><p>Use Refresh to load your items. No task counts are available yet.</p></div>';
    const filtered = queueRecords();
    return `<div class="results-count" role="status">${filtered.length} ${filtered.length === 1 ? 'item' : 'items'}${filtered.length > state.queueLimit ? ` · Showing ${state.queueLimit}` : ''}</div>
      ${filtered.slice(0,state.queueLimit).map(r => `<article class="card queue-card"><div class="card-head"><span class="muted">${esc(r.project)} · ${esc(r.id)} · P${esc(r.priority || 2)}</span>${statusPill(r.status)}</div><h3>${esc(r.title)}</h3><p class="queue-summary">${esc((r.detail || '').slice(0,180))}${(r.detail || '').length>180?'…':''}</p>
      <div class="row-actions"><button class="btn btn-sm" data-edit-rec="${esc(r.id)}">Edit item</button><button class="btn btn-sm" data-bot-task="${esc(r.id)}">Ask agent</button>${r.url ? `<a class="btn btn-sm" href="${esc(r.url)}" target="_blank" rel="noopener noreferrer">Source ↗</a>` : ''}</div>
      ${r.result || (r.detail || '').length>180 ? `<details class="record-details"><summary>Details &amp; result</summary><p>${esc(r.detail)}</p>${r.result ? `<pre>${esc(r.result)}</pre>` : ''}</details>` : ''}</article>`).join('') || '<div class="empty-state"><h3>No matching items</h3><p>Try a different search, project or status.</p></div>'}
      ${filtered.length>state.queueLimit ? '<button id="queueMoreBtn" class="btn load-more">Show 20 more</button>' : ''}`;
  }
  function renderQueue(el) {
    el.innerHTML = `<div class="card queue-filters"><label class="search-field">Search<input id="qSearch" type="search" placeholder="Title, ID, revision or result" value="${esc(state.queueSearch)}"></label><div class="filter-grid">
    <label>Type<select id="qKind">${[['task','Tasks'],['claim','Claims'],['evidence','Evidence'],['release','Releases'],['milestone','Milestones'],['decision','Decisions'],['All','All records']].map(([value,label])=>`<option value="${value}" ${state.queueKind===value?'selected':''}>${label}</option>`).join('')}</select></label>
    <label>Project<select id="qProj">${['All',...state.projects.map(project=>project.key)].map(value=>`<option ${state.queueFilterProject===value?'selected':''} value="${value}">${value==='All'?'All projects':value}</option>`).join('')}</select></label>
    <label>Status<select id="qStatus">${['All',...new Set(['open','in_progress','blocked','complete','deferred',...state.records.map(r=>r.status).filter(Boolean)])].map(value=>`<option value="${esc(value)}" ${state.queueFilterStatus===value?'selected':''}>${value==='All'?'All statuses':esc(value.replaceAll('_',' '))}</option>`).join('')}</select></label></div></div><div id="queueResults">${queueResultsMarkup()}</div>`;
    const update = () => { state.queueLimit=20; el.querySelector('#queueResults').innerHTML=queueResultsMarkup(); bindQueueActions(el); };
    el.querySelector('#qSearch').oninput = e => { state.queueSearch=e.target.value; update(); };
    for(const [id,key] of [['qKind','queueKind'],['qProj','queueFilterProject'],['qStatus','queueFilterStatus']]) el.querySelector('#'+id).onchange=e=>{state[key]=e.target.value;update();};
    bindQueueActions(el);
  }

  function bindQueueActions(el) {
    const more=el.querySelector('#queueMoreBtn');if(more) more.onclick=()=>{state.queueLimit+=20;el.querySelector('#queueResults').innerHTML=queueResultsMarkup();bindQueueActions(el);};
    el.querySelectorAll('[data-edit-rec]').forEach(btn => {
      btn.onclick = () => openRecordModal(state.records.find(r => r.id === btn.dataset.editRec));
    });
    el.querySelectorAll('[data-bot-task]').forEach(btn => {
      btn.onclick = () => {
        const rec = state.records.find(r => r.id === btn.dataset.botTask);
        if (!rec) return;
        state.selectedProject = rec.project || 'All';
        if (rec.botRole) state.selectedBot = rec.botRole;
        state.chatDraft = `Look at [${rec.id}] "${rec.title}" — ${rec.detail || ''}. Give me next steps & status.`;
        localSet('chat-draft', state.chatDraft);
        setTab('bots');
        $('chatInput')?.focus();
      };
    });
  }

  function renderInfra(el) {
    const o=state.oracle, orca=state.orca;
    el.innerHTML=`<section class="card"><div class="card-head"><h3 class="card-title">Oracle VM</h3>${statusPill(o?'online':'unavailable')}</div>
    ${o?`<p class="muted">${esc(o.hostname)} · Updated ${age(o.checkedAt)}</p><div class="telemetry-grid"><div><strong>${Math.round(o.memory.usedMb/Math.max(1,o.memory.totalMb)*100)}%</strong><span>Memory · ${o.memory.availMb} MB available</span></div><div><strong>${esc(o.disk.pct)}</strong><span>Disk · ${esc(o.disk.avail)} free</span></div><div><strong>${o.loadAvg.join(' / ')}</strong><span>Load · ${Math.floor(o.uptimeSec/3600)} hours uptime</span></div></div><details><summary>Services &amp; checkouts</summary>${(o.pm2||[]).map(p=>`<div class="run-row"><span>${esc(p.name)} · ${p.memoryMb} MB · ${p.restarts} restarts</span>${statusPill(p.status)}</div>`).join('')}${(o.repos||[]).map(r=>`<div class="run-row"><span>${esc(r.project)}</span><code>${esc(r.sha||'not found')}${r.dirty?' · changed':''}</code></div>`).join('')}</details>`:'<div class="empty-state">VM telemetry unavailable. Use Refresh to retry.</div>'}</section>
    <section class="card"><div class="card-head"><h3 class="card-title">Orca runtime</h3>${statusPill(orca?.connected?'ready':'unavailable')}</div><p class="muted">${orca?.connected?`Version ${esc(orca.runtime?.version)} · ${orca.worktrees?.length||0} worktrees · ${orca.terminals?.length||0} terminals`:'The runtime is disconnected or its status could not be checked. Agent and command execution have separate connections.'}</p>${(orca?.worktrees||[]).length?`<details><summary>Worktrees</summary>${orca.worktrees.map(w=>`<div class="run-row"><span>${esc(w.name||w.branch||w.id)}</span></div>`).join('')}</details>`:''}</section>
    <section class="card"><h3 class="card-title">Maintenance</h3><p class="muted">These actions change checkouts, restart services or send a notification. Run commands and agent tasks from Agents.</p><div class="maintenance-actions"><button id="pullAllBtn" class="btn">Pull all repositories</button><button id="restartOrcadBtn" class="btn">Restart Orca service</button><button id="runPatrolBtn" class="btn">Run GitHub patrol</button><button id="testNtfyBtn" class="btn">Send test notification</button></div><pre id="infraActionOut" class="command-output" role="status"></pre></section>`;
    for(const [id,url,body] of [['pullAllBtn','/api/oracle/action',{action:'git-pull-all'}],['restartOrcadBtn','/api/oracle/action',{action:'restart-orcad'}],['runPatrolBtn','/api/patrol/run',{}],['testNtfyBtn','/api/ntfy/test',{}]]) {
      const button=$(id);button.onclick=()=>action(button,async()=>{
        if (id==='restartOrcadBtn' && !window.confirm('Restart Orca? Active runtime connections may disconnect.')) return;
        const output=$('infraActionOut');output.textContent='Working…';
        try { const result=await api(url,{method:'POST',body:JSON.stringify(body)});output.textContent=result.output||result.summary||'Completed';notice('Action completed'); }
        catch(error){output.textContent=error.data?.output||error.message;throw error;}
      });
    }
  }

  let dispatchPollTimer, botPollTimer;
  function selectedRun() { return (state.dispatch.jobs||[]).find(j=>j.id===state.selectedRunId)||(state.dispatch.jobs||[])[0]; }
  function dispatcherMarkup() {
    const options=state.dispatch;
    const job=selectedRun();
    const active=(options.jobs||[]).filter(j=>j.status==='running').length;
    return `<details class="card dispatch-card" ${job?.status==='running'?'open':''}><summary><span><strong>Run a task</strong><small>Commands and agent work on a checkout</small></span><span id="dispatchRunningCount" class="${active?'pill info':'muted'}">${active?`${active} running`:'Open'}</span></summary>
    <form id="dispatchForm"><div class="filter-grid dispatch-fields"><label>Worktree<select id="dispatchWorktree">${options.targets.map(t=>`<option value="${esc(t.id)}" ${t.id===state.dispatchTarget?'selected':''} ${!t.available?'disabled':''}>${esc(t.label)}${!t.available?' (unavailable)':''}</option>`).join('')}</select></label><label>Run with<select id="dispatchAgent">${options.engines.map(e=>`<option value="${esc(e.id)}" ${e.id===state.dispatchAgent?'selected':''} ${!e.installed?'disabled':''}>${e.id==='bash'?'Command (Bash, no AI)':esc(e.id)}${!e.installed?' (not installed)':''}</option>`).join('')}</select></label></div>
    <label class="task-field" for="dispatchPrompt">Task or command</label><textarea id="dispatchPrompt" rows="3" placeholder="Describe the task or enter a command" required>${esc(state.dispatchDraft||'git status --short')}</textarea>
    <div class="recipe-row"><button type="button" class="btn btn-sm" data-recipe="status">Git status</button><button type="button" class="btn btn-sm" data-recipe="log">Recent commits</button><button type="button" class="btn btn-sm" data-recipe="tests">Run tests</button></div>
    <p id="dispatchHint" class="muted">${state.dispatchAgent==='bash'?'Runs your command as written in the selected worktree.':'Runs on the VM with the engine’s configured permissions. Installed does not confirm account readiness.'}</p>
    <button id="dispatchRunBtn" type="submit" class="btn btn-primary" ${state.dispatchSubmitting||!options.targets.some(t=>t.available)?'disabled':''}>${state.dispatchSubmitting?'Starting…':'Run task'}</button></form>
    <div class="dispatch-result"><div class="card-head"><h4>Run output</h4><span id="dispatchStatus">${job?statusPill(job.status):''}</span></div><p id="dispatchMeta" class="muted">${job?`${esc(job.agent)} · ${esc(job.worktree)} · ${age(job.startedAt)}`:'Output and status will appear here. You can leave this screen while a run is active.'}</p><pre id="dispatchOutput" class="command-output">${esc(job?.output||job?.error||(job?.status==='running'?'Starting process…':job?'No output':'No run selected.'))}</pre><p id="dispatchError" class="error-text">${esc(job?.error||'')}</p></div>
    ${(options.jobs||[]).length?`<details class="run-history"><summary>Recent runs (${options.jobs.length})</summary>${options.jobs.map(j=>`<button type="button" class="attention-row" data-select-run="${esc(j.id)}"><span><small>${esc(j.agent)} · ${age(j.startedAt)}</small><strong>${esc(j.prompt.slice(0,100))}</strong></span><span data-run-status="${esc(j.id)}">${statusPill(j.status)}</span></button>`).join('')}</details>`:''}</details>`;
  }
  function bindDispatcher() {
    const form=$('dispatchForm');if(!form)return;
    $('dispatchWorktree').onchange=e=>{state.dispatchTarget=e.target.value;};
    $('dispatchAgent').onchange=e=>{state.dispatchAgent=e.target.value;$('dispatchHint').textContent=e.target.value==='bash'?'Runs your command as written in the selected worktree.':'Uses the selected engine’s configured permissions on Oracle.';};
    $('dispatchPrompt').oninput=e=>{state.dispatchDraft=e.target.value;};
    document.querySelectorAll('[data-recipe]').forEach(button=>button.onclick=()=>{
      const target=state.dispatch.targets.find(t=>t.id===$('dispatchWorktree').value);
      const recipe={status:'git status --short',log:'git log -5 --oneline',tests:target?.testCommand||'python3 -m unittest discover -s tests'}[button.dataset.recipe];
      state.dispatchDraft=recipe;$('dispatchPrompt').value=recipe;state.dispatchAgent='bash';$('dispatchAgent').value='bash';$('dispatchAgent').onchange({target:$('dispatchAgent')});
    });
    document.querySelectorAll('[data-select-run]').forEach(button=>button.onclick=()=>{state.selectedRunId=button.dataset.selectRun;updateRunOutput();});
    form.onsubmit=e=>{e.preventDefault();if(state.dispatchSubmitting)return;action($('dispatchRunBtn'),async()=>{
      state.dispatchSubmitting=true;
      const body={worktree:$('dispatchWorktree').value,agent:$('dispatchAgent').value,prompt:$('dispatchPrompt').value.trim(),requestId:window.crypto?.randomUUID?.()||`request-${Date.now()}-${Math.random().toString(36).slice(2)}`};
      if(!body.prompt){state.dispatchSubmitting=false;return;}
      try{
        const response=await api('/api/orca/dispatch',{method:'POST',body:JSON.stringify(body)});
        state.dispatch.jobs=[response.job,...state.dispatch.jobs.filter(j=>j.id!==response.job.id)].slice(0,20);state.selectedRunId=response.job.id;updateRunOutput();pollDispatch();notice('Task started. You can switch screens.');
      }catch(error){
        // A lost receipt does not mean a task failed to start. Reconcile by request identity.
        try{const known=await api('/api/orca/dispatch');state.dispatch=known;const job=known.jobs.find(j=>j.requestId===body.requestId);if(job){state.selectedRunId=job.id;updateRunOutput();pollDispatch();notice('Found the running task after reconnecting.');return;}}catch{}
        throw new Error(`${error.message} Check Recent runs before starting another task.`);
      }finally{state.dispatchSubmitting=false;const visible=$('dispatchRunBtn');if(visible){visible.disabled=!state.dispatch.targets.some(t=>t.available);visible.textContent='Run task';}}
    });};
  }
  function updateRunOutput(){
    const active=state.dispatch.jobs.filter(j=>j.status==='running').length;
    if($('dispatchRunningCount')){$('dispatchRunningCount').textContent=active?`${active} running`:'Open';$('dispatchRunningCount').className=active?'pill info':'muted';}
    document.querySelectorAll('[data-run-status]').forEach(el=>{const job=state.dispatch.jobs.find(j=>j.id===el.dataset.runStatus);if(job)el.innerHTML=statusPill(job.status);});
    const job=selectedRun();if(!job||!$('dispatchOutput'))return;
    $('dispatchOutput').textContent=job.output||(job.status==='running'?'Starting process…':'No output');
    $('dispatchStatus').innerHTML=statusPill(job.status);
    $('dispatchMeta').textContent=`${job.agent} · ${job.worktree} · ${age(job.startedAt)}`;
    $('dispatchError').textContent=job.error||'';
  }
  async function pollDispatch(){
    clearTimeout(dispatchPollTimer);
    const running=(state.dispatch.jobs||[]).filter(j=>j.status==='running');
    if(!running.length||!state.authenticated)return;
    await Promise.all(running.map(async job=>{
      try{const res=await api('/api/orca/jobs/'+encodeURIComponent(job.id));Object.assign(job,res.job);}
      catch(error){notice('Run status unavailable. Refresh to reconnect; do not resend the task.',true);}
    }));
    updateRunOutput();
    if(state.tab==='home')render();
    if(state.dispatch.jobs.some(j=>j.status==='running')&&state.authenticated)dispatchPollTimer=setTimeout(pollDispatch,1800);
  }
  function resumeRuns(){pollDispatch();if(state.activeJobId)pollJob(state.activeJobId);}

  function renderGitHub(el) {
    const repos = state.github?.repos || [];
    el.innerHTML = `
      <div class="github-toolbar">
        <div>${sourceStatus(state.github?.source,'GitHub live')}</div>
        <button id="ghSyncBtn" class="btn btn-sm btn-primary">↻ Sync GitHub</button>
      </div>
      ${repos.map(r => `
        <article class="card github-project">
          <div class="card-head">
            <div><h3 class="card-title">${esc(r.project)}</h3><span class="muted">${r.checkedAt?`Data checked ${age(r.checkedAt)}`:'Data has not loaded successfully'}${r.stale&&r.lastAttemptAt?` · last attempt ${age(r.lastAttemptAt)}`:''}</span></div>
            <span class="pill info">${r.error ? 'Unavailable' : `${r.openPullsCount} PRs listed · ${r.openIssuesCount} issues listed`}</span>
          </div>
          ${r.maturity==='experimental'?`<div class="scope-note"><span class="pill warn">Experimental</span><p>${esc(r.scopeNote)}</p></div>`:''}
          ${r.error ? `<p class="error-text">${esc(r.error)}</p>` : ''}
          ${r.releaseError ? `<p class="error-text">Release check: ${esc(r.releaseError)}</p>` : ''}
          ${(r.pulls || []).map(p => `
            <div style="padding:7px 9px;background:var(--bg);border-radius:10px;margin-bottom:6px;font-size:0.84rem;">
              <span class="pill ok">PR #${p.number}</span> <a href="${esc(p.url)}" target="_blank" style="color:var(--text);">${esc(p.title)}</a>
            </div>
          `).join('')}
          ${(r.issues || []).slice(0, 6).map(i => `
            <div style="padding:7px 9px;background:var(--bg);border-radius:10px;margin-bottom:6px;font-size:0.84rem;display:flex;justify-content:space-between;gap:8px;align-items:center;">
              <a href="${esc(i.url)}" target="_blank" style="color:var(--text);text-decoration:none;">#${i.number} ${esc(i.title)}</a>
              <button class="btn btn-sm" data-ask-issue="${esc(r.project)}|#${i.number} ${esc(i.title)}">⚡</button>
            </div>
          `).join('')}
        </article>
      `).join('')}
    `;

    $('ghSyncBtn').onclick = () => action($('ghSyncBtn'),async()=>{await loadAll(true);render();});
    el.querySelectorAll('[data-ask-issue]').forEach(btn => {
      btn.onclick = () => {
        const [proj, title] = btn.dataset.askIssue.split('|');
        state.selectedProject = proj;
        state.selectedBot = 'inspector';
        setTab('bots');
        setTimeout(() => {
          if ($('chatInput')) $('chatInput').value = `Inspect GitHub issue ${title} in ${proj} and tell me what's needed.`;
        }, 40);
      };
    });
  }

  function renderClaims(el) {
    const claims = state.records.filter(r => r.kind === 'claim');
    el.innerHTML = claims.map(c => `
      <div class="card">
        <div class="card-head">
          <span class="pill">${esc(c.id)} · ${esc(c.project)}</span>
          ${statusPill(c.status)}
        </div>
        <div style="font-weight:700;margin-bottom:4px;">${esc(c.title)}</div>
        <div style="font-size:0.84rem;color:var(--text-muted);">${esc(c.detail)}</div>
      </div>
    `).join('') || '<div class="card">No claims found.</div>';
  }

  function renderReleases(el) {
    const gates = state.records.filter(r => ['milestone','release'].includes(r.kind));
    el.innerHTML = gates.map(g => `
      <div class="card">
        <div class="card-head">
          <span class="pill">${esc(g.id)} · ${esc(g.project)}</span>
          ${statusPill(g.status)}
        </div>
        <div style="font-weight:700;margin-bottom:4px;">${esc(g.title)}</div>
        <div style="font-size:0.84rem;color:var(--text-muted);margin-bottom:8px;">${esc(g.detail)}</div>
        <button class="btn btn-sm btn-primary" data-qm-check="${esc(g.project)}">📦 Ask Quartermaster</button>
      </div>
    `).join('') || '<div class="card">No release milestones found.</div>';

    el.querySelectorAll('[data-qm-check]').forEach(btn => {
      btn.onclick = () => {
        state.selectedProject = btn.dataset.qmCheck;
        state.selectedBot = 'quartermaster';
        setTab('bots');
        setTimeout(() => {
          if ($('chatInput')) $('chatInput').value = `Check all 7 release gates for ${btn.dataset.qmCheck} and tell me what's still blocked.`;
        }, 40);
      };
    });
  }

  function renderDecisions(el) {
    const decs = state.records.filter(r => r.kind === 'decision');
    el.innerHTML = decs.map(d => `
      <div class="card">
        <div class="card-head">
          <span class="pill">${esc(d.id)} · ${esc(d.owner)}</span>
          ${statusPill(d.status)}
        </div>
        <div style="font-weight:700;margin-bottom:4px;">${esc(d.title)}</div>
        <div style="font-size:0.84rem;color:var(--text-muted);">${esc(d.detail)}</div>
      </div>
    `).join('') || '<div class="card">No owner decisions recorded.</div>';
  }

  let modalReturnFocus;
  function openModal(html) {
    modalReturnFocus=document.activeElement;
    $('modalSheet').innerHTML=html;
    $('modalBackdrop').classList.remove('hidden');
    const heading=$('modalSheet').querySelector('h3');if(heading){heading.id='dialogTitle';$('modalSheet').setAttribute('aria-labelledby','dialogTitle');}
    $('modalSheet').querySelectorAll('.form-row').forEach((row,index)=>{const label=row.querySelector('label'),input=row.querySelector('input,select,textarea');if(label&&input){input.id='dialog-field-'+index;label.htmlFor=input.id;}});
    $('modalSheet').querySelectorAll('button').forEach(button=>{if(button.textContent.trim()==='✕')button.setAttribute('aria-label','Close dialog');});
    document.body.style.overflow='hidden';
    ($('modalSheet').querySelector('input,select,textarea,button')||$('modalSheet')).focus();
  }
  function closeModal() {
    $('modalBackdrop').classList.add('hidden');
    $('modalSheet').innerHTML='';
    document.body.style.overflow='';
    modalReturnFocus?.focus?.();
    modalReturnFocus=null;
  }
  document.addEventListener('keydown',event=>{
    if($('modalBackdrop').classList.contains('hidden'))return;
    if(event.key==='Escape'){event.preventDefault();closeModal();return;}
    if(event.key!=='Tab')return;
    const controls=[...$('modalSheet').querySelectorAll('button:not([disabled]),input,textarea,select,a[href]')];
    const first=controls[0],last=controls.at(-1);
    if(!first){event.preventDefault();$('modalSheet').focus();return;}
    if(event.shiftKey&&(document.activeElement===first||document.activeElement===$('modalSheet'))){event.preventDefault();last.focus();}
    else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}
  });

  function openRecordModal(rec = null) {
    const isEdit = Boolean(rec);
    openModal(`
      <div class="card-head">
        <h3 class="card-title">${isEdit ? `Edit ${esc(rec.id)}` : 'New Command Centre Item'}</h3>
        <button id="closeModalBtn" class="btn btn-sm">✕</button>
      </div>
      <form id="recForm">
        <div class="form-row">
          <label>Project</label>
          <select name="project">
            ${state.projects.map(project => `<option value="${esc(project.key)}" ${rec?.project === project.key ? 'selected' : ''}>${esc(project.key)}${project.maturity==='experimental'?' · experimental':''}</option>`).join('')}
          </select>
        </div>
        <div class="grid-2">
          <div class="form-row">
            <label>Kind</label>
            <select name="kind">
              ${[...new Set(['task', 'claim', 'evidence', 'release', 'milestone', 'decision',rec?.kind].filter(Boolean))].map(k => `<option value="${k}" ${rec?.kind === k ? 'selected' : ''}>${k}</option>`).join('')}
            </select>
          </div>
          <div class="form-row">
            <label>Status</label>
            <select name="status">
              ${[...new Set(['open', 'in_progress', 'blocked', 'complete', 'deferred',rec?.status].filter(Boolean))].map(s => `<option value="${s}" ${rec?.status === s ? 'selected' : ''}>${s}</option>`).join('')}
            </select>
          </div>
        </div>
        <div class="form-row">
          <label>Title</label>
          <input name="title" type="text" required value="${esc(rec?.title || '')}" />
        </div>
        <div class="form-row">
          <label>Detail</label>
          <textarea name="detail" rows="3">${esc(rec?.detail || '')}</textarea>
        </div>
        <div class="grid-2">
          <div class="form-row">
            <label>Priority</label>
            <select name="priority">
              <option value="1" ${rec?.priority == 1 ? 'selected' : ''}>1 — High</option>
              <option value="2" ${(!rec || rec?.priority == 2) ? 'selected' : ''}>2 — Normal</option>
              <option value="3" ${rec?.priority == 3 ? 'selected' : ''}>3 — Low</option>
            </select>
          </div>
          <div class="form-row">
            <label>Source URL</label>
            <input name="url" type="text" value="${esc(rec?.url || '')}" />
          </div>
        </div>
        <div style="display:flex;gap:8px;margin-top:12px;">
          <button type="submit" class="btn btn-primary" style="flex:1;">Save Item</button>
          ${isEdit ? `<button type="button" id="delRecBtn" class="btn btn-danger">Delete</button>` : ''}
        </div>
      </form>
    `);

    $('closeModalBtn').onclick = closeModal;
    $('recForm').onsubmit = async e => {
      e.preventDefault();
      const fd = new FormData(e.target);
      const payload = Object.fromEntries(fd.entries());
      if (isEdit) payload.id = rec.id;
      await action(e.target.querySelector('[type=submit]'),async()=>{
        await api('/api/records', { method: 'POST', body: JSON.stringify(payload) });
        closeModal();await loadAll(false);render();notice('Item saved');
      });
    };
    if ($('delRecBtn')) {
      $('delRecBtn').onclick = async () => {
        if(!window.confirm(`Delete ${rec.id}? This removes the saved record.`))return;
        await action($('delRecBtn'),async()=>{await api(`/api/records/${encodeURIComponent(rec.id)}`,{method:'DELETE'});closeModal();await loadAll(false);render();notice('Item deleted');});
      };
    }
  }

  function openMoreDrawer() {
    openModal(`
      <div class="card-head">
        <h3 class="card-title">All Views</h3>
        <button id="closeModalBtn" class="btn btn-sm">✕</button>
      </div>
      <div style="display:flex;flex-direction:column;gap:8px;">
        <button class="btn" data-more-tab="infra">Infrastructure &amp; maintenance</button>
        <button class="btn" data-more-tab="github">🐙 GitHub Live (5 Repos)</button>
        <button class="btn" data-more-tab="claims">🧪 Evidence &amp; Claims</button>
        <button class="btn" data-more-tab="releases">🚀 Release Readiness (7 Gates)</button>
        <button class="btn" data-more-tab="decisions">🧭 Owner Decisions</button>
        <button id="mobileLogoutBtn" class="btn btn-danger">🔒 Lock / Sign out</button>
      </div>
    `);
    $('closeModalBtn').onclick = closeModal;
    $('modalSheet').querySelectorAll('[data-more-tab]').forEach(b => {
      b.onclick = () => setTab(b.dataset.moreTab);
    });
    $('mobileLogoutBtn').onclick = async () => {
      await api('/api/auth/logout', { method: 'POST' });
      closeModal();
      showLogin();
    };
  }

  // Global bindings
  $('loginForm').onsubmit = async event => {
    event.preventDefault();
    $('loginError').textContent='';
    const button=event.target.querySelector('[type=submit]');
    button.disabled=true;
    try {
      await api('/api/auth/login',{method:'POST',body:JSON.stringify({pin:$('pinInput').value})});
      $('pinInput').value='';showApp();state.bootLoading=true;state.chatDraft=localGet('chat-draft')||'';restoreProject();
      $('viewContainer').innerHTML='<div class="empty-state" role="status">Loading your command centre…</div>';
      await loadAll(false);render();resumeRuns();
    } catch(error) { $('loginError').textContent=error.message; }
    finally { button.disabled=false; }
  };

  document.querySelectorAll('[data-tab]').forEach(btn => {
    btn.onclick = () => setTab(btn.dataset.tab);
  });
  $('moreNavBtn').onclick = openMoreDrawer;
  $('addItemTopBtn').onclick = () => openRecordModal(null);
  $('sideAddBtn').onclick = () => openRecordModal(null);
  $('refreshAllBtn').onclick = () => action($('refreshAllBtn'),async()=>{await loadAll(true);render();resumeRuns();notice(state.sourceErrors.length?'Some sources could not refresh. Check the status message.':'Data refreshed',state.sourceErrors.length>0);});
  $('sideLogoutBtn').onclick = async () => {
    await api('/api/auth/logout', { method: 'POST' });
    showLogin();
  };
  $('modalBackdrop').onclick = e => { if (e.target === $('modalBackdrop')) closeModal(); };

  window.addEventListener?.('unhandledrejection', event=>{notice(event.reason?.message||'Action failed. Try refreshing.',true);event.preventDefault();});
  window.addEventListener?.('offline',()=>notice('You are offline. Saved data may be out of date.',true));
  document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='visible'&&state.authenticated){loadAll(false).then(()=>{if(state.tab==='home')render();resumeRuns();}).catch(e=>notice(e.message,true));}});
  setInterval(()=>{if(state.authenticated&&document.visibilityState==='visible')loadAll(false).then(()=>{if(state.tab==='home')render();}).catch(()=>{});},60000);
  init();


  function renderRoadmap(el) {
    const quarters = state.roadmap || [];
    if (!quarters.length) { el.innerHTML='<div class="empty-state"><h3>No roadmap loaded</h3><p>Use Refresh to retry. Existing milestone progress is kept on the server.</p></div>'; return; }
    const all=quarters.flatMap(q=>q.milestones||[]), done=all.filter(m=>m.status==='complete').length;
    const pct=all.length?Math.round(done/all.length*100):0;
    el.innerHTML=`<section class="plan-overview"><div><h3>The year ahead</h3><p>${done} of ${all.length} milestones complete</p></div><strong>${pct}%</strong><progress max="100" value="${pct}" aria-label="Overall roadmap progress">${pct}%</progress></section>`+quarters.map((q,index)=>{
      const items=q.milestones||[], count=items.filter(m=>m.status==='complete').length;
      return `<details class="card quarter" ${index===0?'open':''}><summary><span><small>${esc(q.quarter)} · ${esc(q.period)}</small><strong>${esc(q.theme)}</strong></span><span class="pill">${count}/${items.length}</span></summary><div class="milestone-list">${items.map(m=>`<div class="milestone"><div class="milestone-head"><span><small>${esc(m.id)}</small><strong>${esc(m.title)}</strong></span><button class="btn btn-sm roadmap-toggle-btn ${m.status==='complete'?'is-complete':''}" data-id="${esc(m.id)}" data-status="${m.status==='complete'?'open':'complete'}" aria-label="${m.status==='complete'?'Reopen':'Complete'} ${esc(m.title)}" aria-pressed="${m.status==='complete'}">${m.status==='complete'?'✓ Done':'Mark done'}</button></div>${m.subtasks?.length?`<details class="milestone-details"><summary>Show ${m.subtasks.length} requirements</summary><ul>${m.subtasks.map(st=>`<li><strong>${esc(st.title)}</strong>${st.desc?`: ${esc(st.desc)}`:''}</li>`).join('')}</ul></details>`:''}</div>`).join('')}</div></details>`;
    }).join('');
    el.querySelectorAll('.roadmap-toggle-btn').forEach(button=>{button.onclick=()=>action(button,async()=>{
      const res=await api('/api/roadmap/toggle',{method:'POST',body:JSON.stringify({id:button.dataset.id,status:button.dataset.status})});
      state.roadmap=res.quarters||[];
      if(state.tab==='roadmap'){const opened=[...el.querySelectorAll('.quarter')].map(q=>q.open);renderRoadmap(el);el.querySelectorAll('.quarter').forEach((q,i)=>q.open=opened[i]);}notice('Milestone updated');
    });});
  }
})();
