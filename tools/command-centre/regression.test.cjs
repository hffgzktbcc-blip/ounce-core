const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const {parseHTML}=require('linkedom');
function app(options={}){
 const {window}=parseHTML(fs.readFileSync(__dirname+'/public/index.html','utf8'));
 if(options.localStorage)window.localStorage=options.localStorage;
 const code=fs.readFileSync(__dirname+'/public/app.js','utf8').replace('  init();','  globalThis.appTest={state,render,setTab,api,updateRunOutput,init};');
 const ctx=vm.createContext({window,document:window.document,console,setTimeout:(fn,ms)=>{const t=setTimeout(fn,ms);t.unref();return t;},clearTimeout,setInterval:()=>0,URL,AbortController,fetch:options.fetch|| (async()=>({status:503,ok:false,json:async()=>({error:'Service unavailable'})}))});
 vm.runInContext(code,ctx);return {a:ctx.appTest,document:window.document};
}
test('Plan renders a loaded milestone without a scope error',async()=>{
 const {a,document}=app();a.state.roadmap=[{quarter:'Q1',theme:'Release',period:'Months 1–3',milestones:[{id:'Q1-1',title:'Ship a release',status:'open'}]}];
 const rejected=[];const onRejection=e=>rejected.push(e);process.on('unhandledRejection',onRejection);
 try{a.setTab('roadmap');await new Promise(r=>setTimeout(r,5));assert.equal(rejected.length,0);assert.match(document.getElementById('viewContainer').textContent,/Ship a release/);}finally{process.off('unhandledRejection',onRejection);}
});
test('Queue search retains the input node while results update',()=>{
 const {a,document}=app();a.state.records=[{id:'T-1',title:'Alpha',kind:'task',status:'open'},{id:'T-2',title:'Beta',kind:'task',status:'open'}];a.setTab('queue');
 const input=document.getElementById('qSearch');input.value='Alpha';input.oninput({target:input});
 assert.ok(document.getElementById('qSearch')===input,'search input must remain mounted');assert.doesNotMatch(document.getElementById('viewContainer').textContent,/Beta/);
});
test('API errors reject with the service message',async()=>{const {a}=app();await assert.rejects(a.api('/api/records'),/Service unavailable/);});
test('Home is the default daily landing view',()=>{const {a}=app();assert.equal(a.state.tab,'home');});
test('mobile navigation has five primary destinations',()=>{const {document}=app();assert.equal(document.querySelectorAll('.mobile-nav button').length,5);});
test('agent composer draft survives a status render',()=>{const {a,document}=app();a.setTab('bots');document.getElementById('chatInput').value='Keep this next question';a.render();assert.equal(document.getElementById('chatInput').value,'Keep this next question');});
test('empty roadmap explains absence instead of loading forever',()=>{const {a,document}=app();a.setTab('roadmap');assert.doesNotMatch(document.getElementById('viewContainer').textContent,/Loading/);assert.match(document.getElementById('viewContainer').textContent,/No roadmap/i);});
test('Queue defaults to tasks, keeping claims in their own view',()=>{const {a,document}=app();a.state.records=[{id:'T1',kind:'task',title:'Task title',status:'open'},{id:'C1',kind:'claim',title:'Claim title',status:'reported'}];a.setTab('queue');assert.match(document.getElementById('viewContainer').textContent,/Task title/);assert.doesNotMatch(document.getElementById('viewContainer').textContent,/Claim title/);});

test('a delayed milestone update cannot replace the destination after navigation',async()=>{let resolve;const reply=new Promise(r=>resolve=r);const {a,document}=app({fetch:()=>reply});a.state.roadmap=[{quarter:'Q1',milestones:[{id:'M1',title:'Milestone',status:'open'}]}];a.setTab('roadmap');const pending=document.querySelector('.roadmap-toggle-btn').onclick();a.setTab('queue');resolve({status:200,ok:true,json:async()=>({ok:true,quarters:a.state.roadmap})});await pending;assert.ok(document.getElementById('qSearch'),'Queue must remain rendered');});
test('dispatch submission unlocks the visible replacement button after navigation',async()=>{let resolve;const reply=new Promise(r=>resolve=r);const {a,document}=app({fetch:()=>reply});a.state.dispatch={targets:[{id:'eelpout',label:'eelpout',available:true}],engines:[{id:'bash',installed:true}],jobs:[]};a.setTab('bots');document.getElementById('dispatchForm').onsubmit({preventDefault(){}});a.setTab('queue');a.setTab('bots');assert.equal(document.getElementById('dispatchRunBtn').disabled,true);resolve({status:202,ok:true,json:async()=>({ok:true,job:{id:'run1',status:'complete',agent:'bash',worktree:'eelpout'}})});await new Promise(r=>setImmediate(r));assert.equal(document.getElementById('dispatchRunBtn').disabled,false);});
test('API errors retain per-repository diagnostic output',async()=>{const {a}=app({fetch:async()=>({status:200,ok:true,json:async()=>({ok:false,error:'Pull failed',output:'Ounce-Core: merge conflict'})})});try{await a.api('/api/oracle/action');assert.fail('Expected error');}catch(error){assert.equal(error.data?.output,'Ounce-Core: merge conflict');}});

test('completed dispatch removes its running indicator without replacing the composer',()=>{const {a,document}=app();const job={id:'j1',status:'running',agent:'bash',worktree:'eelpout',prompt:'git status',startedAt:new Date().toISOString()};a.state.dispatch={targets:[],engines:[],jobs:[job]};a.setTab('bots');const composer=document.getElementById('chatInput');job.status='complete';a.updateRunOutput();assert.doesNotMatch(document.querySelector('.dispatch-card>summary').textContent,/1 running/);assert.ok(document.getElementById('chatInput')===composer);});
test('Home never calls a never-loaded queue clear',()=>{const {a,document}=app();a.setTab('home');assert.doesNotMatch(document.getElementById('viewContainer').textContent,/Your queue is clear/);assert.match(document.getElementById('viewContainer').textContent,/Queue (unavailable|is loading)/);});
test('Agents cannot submit when no engine data is loaded',()=>{const {a,document}=app();a.setTab('bots');assert.equal(document.querySelector('#chatForm button[type=submit]').disabled,true);});

test('session-check failure offers retry without pretending the user signed out',async()=>{const {a,document}=app();await a.init();assert.equal(document.getElementById('loginGate').classList.contains('hidden'),true);assert.equal(document.getElementById('retryConnectionBtn')?.classList.contains('hidden'),false);});

test('Home recent run opens the selected output directly',()=>{
 const {a,document}=app();a.state.dispatch.jobs=[{id:'saved-run',status:'complete',agent:'bash',worktree:'eelpout',prompt:'git status',output:'clean worktree'}];a.setTab('home');
 const button=document.querySelector('[data-home-run="saved-run"]');assert.ok(button,'recent run must be actionable');button.onclick();
 assert.equal(a.state.tab,'bots');assert.equal(document.getElementById('dispatchOutput').textContent,'clean worktree');assert.equal(document.querySelector('.dispatch-card').hasAttribute('open'),true);
});
test('Queue agent handoff fills and saves the draft before navigation',async()=>{
 const {a,document}=app();a.state.records=[{id:'T-next',kind:'task',status:'open',project:'Ounce-Core',title:'Check release',detail:'Inspect evidence'}];a.setTab('queue');document.querySelector('[data-bot-task]').onclick();
 assert.match(a.state.chatDraft,/T-next/);assert.match(document.getElementById('chatInput').value,/Inspect evidence/);
 document.getElementById('chatInput').value='My revised question';document.getElementById('chatInput').oninput({target:document.getElementById('chatInput')});
 await new Promise(r=>setTimeout(r,60));assert.equal(document.getElementById('chatInput').value,'My revised question');
});
test('a completed selected run with empty output is not described as unselected',()=>{
 const {a,document}=app();a.state.dispatch.jobs=[{id:'empty-run',status:'complete',agent:'bash',worktree:'eelpout',prompt:'git status --short',output:''}];a.setTab('bots');
 assert.equal(document.getElementById('dispatchOutput').textContent,'No output');
});

test('saved agent draft restores its project scope after reload',async()=>{
 const data=new Map();const localStorage={getItem:k=>data.get(k)||null,setItem:(k,v)=>data.set(k,v)};
 const first=app({localStorage});first.a.state.records=[{id:'DS-next',kind:'task',status:'open',project:'DeckStream',title:'Check stream'}];first.a.setTab('queue');first.document.querySelector('[data-bot-task]').onclick();
 assert.equal(data.get('ounce-chat-project'),'DeckStream');
 const next=app({localStorage,fetch:async url=>url==='/api/auth/me'?{status:200,ok:true,json:async()=>({ok:true,authenticated:true})}:{status:503,ok:false,json:async()=>({error:'Unavailable'})}});await next.a.init();
 assert.equal(next.a.state.selectedProject,'DeckStream');assert.match(next.a.state.chatDraft,/DS-next/);
});

test('Ounce-Spark is available everywhere project scope is chosen',()=>{const {a,document}=app();a.state.records=[{id:'SP-1',kind:'task',status:'open',project:'Ounce-Spark',title:'Check PTX semantics'}];a.setTab('queue');assert.match(document.getElementById('qProj').textContent,/Ounce-Spark/);document.querySelector('[data-bot-task]').onclick();assert.equal(a.state.selectedProject,'Ounce-Spark');assert.match(document.getElementById('projectSelect').textContent,/Ounce-Spark/);});

test('Home distinguishes live GitHub checks from stale maintained records',()=>{const {a,document}=app();a.state.queueLoaded=true;a.state.github={source:{kind:'github',label:'GitHub live',checkedAt:new Date().toISOString(),stale:false},repos:[]};a.state.recordSource={kind:'manual',label:'Maintained records',updatedAt:'2026-11-13T10:00:00Z',stale:true,staleAfterHours:24};a.setTab('home');const text=document.getElementById('viewContainer').textContent;assert.match(text,/GitHub live/);assert.match(text,/Maintained records/);assert.match(text,/stale/i);});

test('an empty maintained-record source says it was never updated',()=>{const {a,document}=app();a.state.queueLoaded=true;a.state.recordSource={kind:'manual',label:'Maintained records',checkedAt:new Date().toISOString(),updatedAt:null,stale:true};a.setTab('home');const text=document.getElementById('viewContainer').textContent;assert.match(text,/Never updated/);assert.doesNotMatch(text,/last changed Just updated/);});

test('GitHub cards expose per-project checked time and experimental scope',()=>{const {a,document}=app();a.state.github={source:{label:'GitHub live',checkedAt:'2026-11-21T08:00:00Z',stale:false},repos:[{project:'Ounce-Spark',maturity:'experimental',scopeNote:'CPU-based PTX research; no hardware-validated Blackwell performance.',checkedAt:'2026-11-21T08:00:00Z',openPullsCount:0,openIssuesCount:2,pulls:[],issues:[]}]};a.setTab('github');const text=document.getElementById('viewContainer').textContent;assert.match(text,/Ounce-Spark/);assert.match(text,/Experimental/);assert.match(text,/no hardware-validated Blackwell performance/i);assert.match(text,/Checked/);});
