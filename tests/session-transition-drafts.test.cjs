// Passive stored-workspace navigation only. Production input/session handlers and
// render/reset functions run in a VM; all fetches are deferred in-memory replies.
const assert = require('node:assert/strict');
const test = require('node:test');
const vm = require('node:vm');
const { fixture, plain } = require('./replay-workspace-fixture.cjs');
const { appSource } = require('./frontend-test-helpers.cjs');
const source = name => appSource.match(new RegExp(`^(?:async )?function ${name}\\([^]*?^\\}`, 'm'))[0];
const tick = () => new Promise(resolve => setImmediate(resolve));

function setup({ rendered = true } = {}) {
  const f = fixture();
  f.old = f.tab('old'); f.seed([f.old]); f.state.activeTool = 'replay';
  f.next = f.tab('new'); f.log = []; f.delayed = []; f.reconciles = []; f.confirms = [];
  Object.defineProperty(f.els.replayRequestHighlight, 'innerHTML', {
    configurable: true, set(value) { this.innerText = value; },
  });
  for (const name of [
    'renderReplay', 'applyWorkspaceState', 'createSession', 'activateSessionById',
    'reloadSessionWorkspace', 'resetSessionScopedUiState', 'renderReplayClearedState',
    'renderReplayRequestHighlight', 'renderToolPanels', 'applyReplayTargetFields',
    'setReplayTargetInputValidity', 'startHexByteEdit', 'syncReplayRequestTextFromEditor',
    'clearReplayResponseForDraftChange', 'syncReplayToolbar', 'parseEditableRawRequest',
    'splitRawHttpMessage', 'parseReplayHttpVersionToken', 'headerValue',
    'editableRequestBodyLength', 'validateRawHttpBodyFraming', 'clearReplaySendInFlight',
  ]) vm.runInContext(source(name), f.context);
  vm.runInContext(appSource.match(/^class WorkspaceSessionMismatchError[^]*?^\}/m)[0], f.context);
  for (const name of ['flushTargetScopeDraft', 'flushMatchReplaceDraft', 'flushSequenceDraft']) {
    f.context[name] = async () => { f.log.push(name); return true; };
  }
  for (const name of ['flushAllPendingAnnotations', 'cleanupWsReplayTabsBeforeStateReset']) {
    f.context[name] = async () => { f.log.push(name); };
  }
  // Omit unrelated painters while executing the complete production state reset.
  for (const name of [
    'createHistoryPagingState', 'createWebsocketPagingState', 'closeContextMenu',
    'clearHistoryBackfill', 'cancelHistoryDetailLoading', 'invalidateVisibleEntriesCache',
    'renderIntercepts', 'renderResponseIntercepts', 'renderInterceptRules', 'updateInterceptQueueBadges',
    'resetWebsocketFrameScroll', 'cancelWebsocketDetailLoading', 'clearWebsocketQueryBackfill',
    'clearFilteredWebsocketReload', 'clearWebsocketSearchReload', 'resetEventLogUiState',
    'resetOastUiState', 'resetScannerConfigUiState', 'resetFindingsUiState', 'clearFuzzerAttackRecord',
    'clearCompareState', 'renderHistory', 'renderEmptyDetail', 'renderWebsocketSessions', 'hideFrameDetail',
    'renderEventLog', 'renderMatchReplaceRules', 'renderTarget', 'renderFindings', 'renderFuzzer',
    'renderSequencePanel', 'renderReplayResponseView', 'updateReplaySearchPane', 'renderDashboard',
    'renderProxyPanels', 'refreshReplayTabLabel', 'renderReplayEmptyResponse', 'bindHexByteHandlers',
    'renderWsReplay', 'setReplaySending',
  ]) f.context[name] = () => {};
  for (const name of [
    'loadSettings', 'loadTransactions', 'loadIntercepts', 'loadResponseIntercepts', 'loadInterceptRules',
    'loadWebsockets', 'loadEventLog', 'loadMatchReplaceRules', 'loadSequences', 'loadTargetSiteMap',
    'refreshScannerQuickToggle', 'connectEvents',
  ]) f.context[name] = async () => {};
  Object.assign(f.context, {
    refreshTimer: null, _incrementalTimer: 0, _transactionDeltaTimer: 0,
    _pendingTransactionSummaries: [], _historyDetailGeneration: 0, _websocketLoadGeneration: 0,
    _websocketDetailGeneration: 0, _websocketDetailPendingId: null, _websocketDetailPendingSessionId: null,
    _websocketDetailPendingPromise: null, _websocketDetailRefreshTimer: null,
    _websocketDetailRefreshNeeded: null, _websocketSummaryMutationGeneration: 0,
    _websocketSummaryMutationById: new Map(), _websocketSummaryEventTimer: null,
    _websocketSummaryEventBuffer: new Map(), _lastWebsocketFallbackPoll: 0, _lastWebsocketPageRefreshAt: 0,
    mainTabs: [], sanitizeActiveTool: value => value, TextDecoder,
    renderCodeHtml: text => text, renderEditableHexHtml: () => '<hex>', toHexDumpFromBytes: () => '<hex dump>',
    normalizeFuzzerAttackRecord: () => null, setFuzzerAttackRecord() {}, fuzzerWorkspaceAttackRecordId: () => null,
    setTimeout: callback => { f.delayed.push(callback); return f.delayed.length; },
    canNavigateReplayHistory: () => false,
  });
  f.context.loadSessions = async () => { f.log.push('reload'); f.state.activeSession = { id: 'session-b' }; };
  f.context.loadWorkspaceState = async () => {
    const snapshot = f.remote([f.next], 20); snapshot.session_id = 'session-b';
    f.context.applyWorkspaceState(snapshot); f.context.workspaceLoaded = true;
  };
  f.context.reconcileSessionsAfterActionError = async (...args) => f.reconciles.push(args);
  f.context.showConfirmDialog = (message, callback, options) => f.confirms.push({ message, callback, options });
  f.context.handleWorkspaceActionError = error => { f.error = error; };
  f.els.dashboardCreateSessionName = { value: 'new session' };
  f.els.replayRequestHighlight.setAttribute = () => {};
  f.els.replayRequestHighlight.removeAttribute = () => {};
  for (const name of [
    'dashboardShell', 'proxyShell', 'replayShell', 'toolsShell',
    'fuzzerShell', 'sequenceShell', 'targetShell', 'loggerShell',
  ]) f.els[name] = { classList: { toggle: (key, value) => { f.els[name][key] = value; } } };
  f.els.footerMode = {};
  for (const name of ['sendReplayButton', 'cancelReplayButton', 'replayBackButton', 'replayForwardButton']) {
    f.els[name] = {};
  }
  for (const input of [f.els.replayHostInput, f.els.replayPortInput]) {
    input.listeners = {};
    input.addEventListener = (name, callback) => { input.listeners[name] = callback; };
    input.setCustomValidity = message => { input.validationMessage = message; };
    input.toggleAttribute = () => {};
  }
  const marker = '  els.replayPortInput.addEventListener("input", () => {';
  const binding = appSource.slice(appSource.indexOf(marker));
  vm.runInContext(binding.slice(0, binding.indexOf('\n  });') + 6), f.context);
  f.invalid = () => {
    f.document.activeElement = f.els.replayPortInput; f.els.replayPortInput.value = '8x';
    f.els.replayPortInput.listeners.input();
    assert.equal(f.old.targetPort, '443'); assert.ok(f.els.replayPortInput.validationMessage);
  };
  f.dashboard = () => {
    f.state.activeTool = 'dashboard'; f.context.renderToolPanels();
    assert.equal(f.els.replayShell.hidden, true);
  };
  f.pendingHex = () => {
    f.state.replayMessageViews.request = 'hex';
    f.old.requestBytes = new Uint8Array([255, 15, 0]);
    f.old.requestOriginalBytes = new Uint8Array(f.old.requestBytes);
    let input = null;
    const container = f.els.replayRequestHighlight;
    const span = {
      dataset: { idx: '0' }, classList: { add() {}, remove() {} },
      appendChild: child => { input = child; },
    };
    container.querySelectorAll = selector => selector === '.hex-byte-input' && input ? [input] : [];
    container.querySelector = selector => selector === '.hex-byte-input' ? input : null;
    Object.defineProperty(container, 'innerHTML', { configurable: true, set() { input = null; } });
    f.document.createElement = () => ({
      listeners: {}, addEventListener(name, callback) { this.listeners[name] = callback; },
      focus() { f.document.activeElement = this; }, select() {},
      closest: selector => selector === '.hex-byte' ? span : null, remove() { input = null; },
    });
    f.context.startHexByteEdit(span, f.old, container);
    const original = input; input.value = 'f'; input.listeners.input(); input.listeners.blur();
    return { input: original, current: () => input };
  };
  f.start = async (operation, id = 'session-b') => {
    const pending = (operation === 'create'
      ? f.context.createSession() : f.context.activateSessionById(id)).catch(error => error);
    await tick(); return { pending };
  };
  if (rendered) f.context.renderReplay();
  else f.context.renderReplayClearedState();
  return f;
}

const take = f => ({
  tabs: plain(f.state.replayTabs), active: f.state.activeReplayTabId,
  text: f.els.replayRequestHighlight.innerText, port: f.els.replayPortInput.value,
  dirty: f.context.workspaceSaveDirty, version: f.context.workspaceSaveVersion,
  generation: f.context.workspaceStateGeneration, timers: [...f.timers.keys()],
});
for (const operation of ['create','activate']) {
 for (const kind of ['invalid target','delayed hex']) test(`${operation}: previously rendered dashboard-hidden ${kind} blocks before preflight/storage/reset`, async()=>{
  const f=setup(); let hex; if(kind==='invalid target') f.invalid(); else hex=f.pendingHex();
  f.dashboard(); const before=take(f); const {pending}=await f.start(operation);
  assert.equal(f.requests.length,0,'unsafe draft must prevent session and workspace writes');
  assert.equal(await pending,undefined); assert.deepEqual(take(f),before); assert.deepEqual(f.log,[]);
  assert.match(f.toasts.at(-1)[0],/draft is still here/);
  if(hex) {assert.equal(hex.current(),hex.input);assert.equal(hex.input.value,'f');}
 });
 for (const kind of ['partial','empty','target','version']) test(`${operation}: captures and flushes supported DOM-only ${kind} before session mutation`,async()=>{
  const f=setup();
  if(kind==='partial') f.els.replayRequestHighlight.innerText='GET /unfinished HTT';
  if(kind==='empty') f.els.replayRequestHighlight.innerText='';
  if(kind==='target') f.els.replayPortInput.value='8080';
  if(kind==='version') f.versionSelect.value='HTTP/2';
  f.dashboard(); const {pending}=await f.start(operation);
  assert.equal(f.requests[0]?.url,'/api/workspace-state','newly captured editor changes must be queued');
  const body=JSON.parse(f.requests[0].options.body); assert.equal(body.session_id,'session-a');
  if(kind==='partial'||kind==='empty') assert.equal(body.replay.tabs[0].request_text,kind==='partial'?'GET /unfinished HTT':'');
  if(kind==='target') assert.equal(body.replay.tabs[0].target_port,'8080');
  if(kind==='version') assert.equal(body.replay.tabs[0].http_version_mode,'HTTP/2');
  f.succeed(0,{session_id:'session-a',revision:11});await tick();
  assert.equal(f.requests[1].url,operation==='create'?'/api/sessions':'/api/sessions/session-b/activate');
  f.succeed(1,{});assert.equal(await pending,undefined);
 });
 for(const kind of ['partial text','valid target']) test(`${operation}: ordinary model-backed ${kind} still flushes normally`,async()=>{
  const f=setup();if(kind==='partial text') {f.els.replayRequestHighlight.innerText='GET /ordinary HTT';f.context.syncReplayRequestTextFromEditor('GET /ordinary HTT');}
  else {f.document.activeElement=f.els.replayPortInput;f.els.replayPortInput.value='8080';f.els.replayPortInput.listeners.input();}
  f.dashboard();const {pending}=await f.start(operation);assert.equal(f.requests[0].url,'/api/workspace-state');
  f.succeed(0,{session_id:'session-a',revision:11});await tick();f.succeed(1,{});assert.equal(await pending,undefined);
 });
 test(`${operation}: never-rendered dashboard controls do not become saved edits`,async()=>{
  const f=setup({rendered:false});f.dashboard();const before=plain(f.old);const {pending}=await f.start(operation);
  assert.equal(f.requests.length,1);assert.equal(f.requests[0].url,operation==='create'?'/api/sessions':'/api/sessions/session-b/activate');
  assert.deepEqual(plain(f.old),before);f.succeed(0,{});assert.equal(await pending,undefined);
 });
 test(`${operation}: newly loaded dashboard session with cleared editor preserves stored request on second transition`,async()=>{
  const f=setup();f.dashboard();const first=await f.start('activate');f.succeed(0,{});assert.equal(await first.pending,undefined);
  const saved=f.context.getActiveReplayTab(),before=plain(saved);assert.notEqual(saved.requestText,'');assert.equal(f.els.replayRequestHighlight.innerText,'');assert.equal(f.context.workspaceLoaded,true);
  const second=await f.start(operation,'session-c');assert.equal(f.requests.length,2);assert.equal(f.requests[1].url,operation==='create'?'/api/sessions':'/api/sessions/session-c/activate');assert.deepEqual(plain(saved),before);
  f.succeed(1,{});assert.equal(await second.pending,undefined);
 });
 for(const mode of ['unloaded','no tab','same id new generation','same id new object','different session']) test(`${operation}: ${mode} does not attribute stale controls to active model`,async()=>{
  const f=setup();f.els.replayRequestHighlight.innerText='Stale DOM';f.els.replayPortInput.value='8x';
  if(mode==='unloaded')f.context.workspaceLoaded=false;
  if(mode==='no tab'){f.state.replayTabs=[];f.state.activeReplayTabId=null;}
  if(mode==='same id new generation')f.context.workspaceStateGeneration++;
  if(mode==='same id new object'){f.state.replayTabs=[{...f.old}];}
  if(mode==='different session')f.state.activeSession={id:'session-new'};
  f.dashboard();const before=plain(f.state.replayTabs);const {pending}=await f.start(operation);
  assert.equal(f.requests.length,1);assert.equal(f.requests[0].url,operation==='create'?'/api/sessions':'/api/sessions/session-b/activate');assert.deepEqual(plain(f.state.replayTabs),before);
  f.succeed(0,{});assert.equal(await pending,undefined);
 });
 test(`${operation}: clean binary stays navigable without byte-array changes`,async()=>{
  const f=setup();f.state.replayMessageViews.request='hex';const bytes=f.old.requestBytes=new Uint8Array([255,15]);const original=f.old.requestOriginalBytes=new Uint8Array([1,15]);f.context.renderReplay();f.dashboard();
  const {pending}=await f.start(operation);assert.equal(f.requests.length,1);assert.equal(f.old.requestBytes,bytes);assert.equal(f.old.requestOriginalBytes,original);f.succeed(0,{});assert.equal(await pending,undefined);
 });
 test(`${operation}: draft preflight and conflict retain original failure semantics`,async()=>{
  const f=setup();f.context.flushTargetScopeDraft=async()=>false;const {pending}=await f.start(operation);assert.equal(await pending,undefined);assert.equal(f.requests.length,0);
  f.context.flushTargetScopeDraft=async()=>true;f.context.workspaceSaveConflictPending=true;const next=await f.start(operation);assert.equal((await next.pending).name,'WorkspaceStateConflictError');assert.equal(f.requests.length,0);
 });
 test(`${operation}: failed mutation leaves editor and workspace intact`,async()=>{
  const f=setup();f.dashboard();const {pending}=await f.start(operation);f.invalid();const before=take(f);
  f.requests[0].resolve({ok:false,status:500,text:async()=>'Synthetic failure'});assert.equal((await pending).message,'Synthetic failure');assert.deepEqual(take(f),before);assert.equal(f.reconciles.length,1);
 });
}
test('busy confirmation keeps its original query/warning and rechecks fresh draft on force retry',async()=>{
 const f=setup();f.dashboard();const {pending}=await f.start('activate');f.requests[0].resolve({ok:false,status:409,text:async()=>'proxy activity is still running'});assert.equal(await pending,undefined);
 assert.equal(f.confirms.length,1);assert.equal(f.reconciles.length,0);assert.match(f.confirms[0].message,/Anything still in flight is cut and will not be recorded/);assert.equal(f.confirms[0].options.confirmLabel,'Switch anyway');
 f.invalid();const before=take(f);f.confirms[0].callback();await tick();assert.equal(f.requests.length,1,'force retry must not bypass draft preservation');assert.deepEqual(take(f),before);
 f.els.replayPortInput.value='443';f.confirms[0].callback();await tick();assert.equal(f.requests[1].url,'/api/sessions/session-b/activate?force=true');f.succeed(1,{});await tick();assert.equal(f.state.activeSession.id,'session-b');
});
test('direct same-session activation retains existing successful request/reset behavior',async()=>{
 const f=setup();const {pending}=await f.start('activate','session-a');assert.equal(f.requests[0].url,'/api/sessions/session-a/activate');f.succeed(0,{});assert.equal(await pending,undefined);assert.ok(f.context.workspaceStateGeneration>0);
});

for (const operation of ['create', 'activate']) {
  test(`${operation}: repeated refused actions keep the same draft and controls`, async () => {
    const f = setup(); f.invalid(); f.dashboard(); const before = take(f);
    for (let attempt = 0; attempt < 2; attempt++) {
      const { pending } = await f.start(operation); assert.equal(await pending, undefined);
      assert.deepEqual(take(f), before); assert.equal(f.requests.length, 0); assert.deepEqual(f.log, []);
    }
  });

  test(`${operation}: unchanged rendered draft neither schedules nor invalidates saved result`, async () => {
    const f = setup(); f.dashboard();
    const result = f.old.responseRecord = { id: 'saved' }; f.old.notice = 'saved notice';
    const version = f.context.workspaceSaveVersion;
    const { pending } = await f.start(operation);
    assert.equal(f.requests.length, 1); assert.notEqual(f.requests[0].url, '/api/workspace-state');
    assert.equal(f.context.workspaceSaveDirty, false); assert.equal(f.context.workspaceSaveVersion, version);
    assert.equal(f.timers.size, 0); assert.equal(f.old.responseRecord, result); assert.equal(f.old.notice, 'saved notice');
    f.succeed(0, {}); assert.equal(await pending, undefined);
  });

  for (const editor of ['CodeMirror', 'textarea']) {
    test(`${operation}: supported ${editor} fallback draft is captured and flushed`, async () => {
      const f = setup();
      if (editor === 'CodeMirror') {
        const view = { text: f.old.requestText, getContent() { return this.text; } };
        f.els.replayRequestCM = {}; f.context.getCMView = () => view;
        f.context.updateCodePaneCM = (_, __, text) => { view.text = text; };
        f.context.renderReplay(); view.text = 'GET /fallback HTT';
      } else {
        delete f.els.replayRequestHighlight; f.context.renderReplay();
        f.els.replayRequestEditor.value = 'GET /fallback HTT';
      }
      f.dashboard(); const { pending } = await f.start(operation);
      assert.equal(f.requests[0].url, '/api/workspace-state');
      assert.equal(JSON.parse(f.requests[0].options.body).replay.tabs[0].request_text, 'GET /fallback HTT');
      f.succeed(0, { session_id: 'session-a', revision: 11 }); await tick();
      f.succeed(1, {}); assert.equal(await pending, undefined);
    });
  }

  test(`${operation}: already committed legacy byte is saved on retry`, async () => {
    const f = setup(), hex = f.pendingHex(); f.dashboard();
    const refused = await f.start(operation); assert.equal(await refused.pending, undefined);
    assert.equal(f.requests.length, 0); assert.equal(hex.current(), hex.input);
    f.delayed[0](); assert.equal(f.old.requestBytes[0], 15); assert.equal(hex.current(), null);
    const { pending } = await f.start(operation); assert.equal(f.requests[0].url, '/api/workspace-state');
    assert.equal(JSON.parse(f.requests[0].options.body).replay.tabs[0].request_text.charCodeAt(0), 15);
    f.succeed(0, { session_id: 'session-a', revision: 11 }); await tick();
    f.succeed(1, {}); assert.equal(await pending, undefined);
  });

  test(`${operation}: sending retains original cleanup timing`, async () => {
    const f = setup(); let aborts = 0;
    const controller = { abort() { aborts++; } };
    f.context._replaySendControllers.set(f.old.id, controller); f.dashboard();
    const { pending } = await f.start(operation);
    assert.equal(f.requests.length, 1); assert.equal(aborts, 0);
    assert.equal(f.context._replaySendControllers.get(f.old.id), controller);
    f.succeed(0, {}); assert.equal(await pending, undefined);
    assert.equal(aborts, 1); assert.equal(f.context._replaySendControllers.size, 0);
  });

  test(`${operation}: WebSocket render clears HTTP ownership without blocking session controls`, async () => {
    const f = setup(); assert.ok(f.context.renderedHttpReplayDraftOwner);
    f.old.type = 'websocket'; f.context.renderReplay();
    assert.equal(f.context.renderedHttpReplayDraftOwner, null);
    f.els.replayPortInput.value = '8x'; f.dashboard();
    const { pending } = await f.start(operation); assert.equal(f.requests.length, 1);
    assert.ok(f.log.includes('cleanupWsReplayTabsBeforeStateReset'));
    f.succeed(0, {}); assert.equal(await pending, undefined);
  });

  test(`${operation}: failed workspace reload cannot leave ownership attached to cleared controls`, async () => {
    const f = setup(); f.dashboard(); const load = f.context.loadWorkspaceState;
    const failure = new Error('Synthetic workspace read failure');
    f.context.loadWorkspaceState = async () => { throw failure; };
    const first = await f.start(operation); f.succeed(0, {}); assert.equal(await first.pending, failure);
    assert.equal(f.context.renderedHttpReplayDraftOwner, null);
    assert.equal(f.context.workspaceLoaded, false); assert.equal(f.state.replayTabs.length, 0);
    assert.equal(f.reconciles.length, 0, 'successful mutation is not reclassified as a mutation error');
    f.context.loadWorkspaceState = load;
    const second = await f.start(operation); assert.equal(f.requests.length, 2);
    f.succeed(1, {}); assert.equal(await second.pending, undefined);
    assert.equal(f.context.renderedHttpReplayDraftOwner, null, 'dashboard reload does not render an HTTP editor');
    assert.notEqual(f.context.getActiveReplayTab().requestText, '');
  });
}

test('force retry reads the newly rendered workspace owner after session reset', async () => {
  const f = setup(); f.dashboard(); const { pending } = await f.start('activate');
  f.requests[0].resolve({ ok: false, status: 409, text: async () => 'proxy activity is still running' });
  assert.equal(await pending, undefined); assert.equal(f.confirms.length, 1);
  f.context.resetSessionScopedUiState();
  f.state.activeSession = { id: 'session-new' }; f.context.workspaceLoaded = true;
  const replacement = f.tab('replacement'); replacement.id = f.old.id; f.seed([replacement]);
  f.context.renderReplay(); f.els.replayPortInput.value = '8x'; f.dashboard();
  const before = take(f); f.confirms[0].callback(); await tick();
  assert.equal(f.requests.length, 1); assert.deepEqual(take(f), before);
  assert.equal(f.context.renderedHttpReplayDraftOwner.tab, replacement);
});

// Completion races use the same production render/reset/input fixture, with real
// session/workspace readers so list-only reconciliation cannot secretly reload.
function setupCompletedAction() {
  const f = setup(); f.dashboard(); f.warnings = [];
  f.rows = active => ['session-a', 'session-b', 'session-c'].map(id => ({ id, name: id, active: id === active }));
  f.state.sessions = f.rows('session-a'); f.state.selectedSessionId = 'session-a';
  Object.assign(f.context, {
    sessionsLoadGeneration: 0, sessionsAppliedLoadGeneration: 0,
    requireOkResponse: async response => { if (!response.ok) throw new Error('Synthetic session read failure'); },
    console: { error() {}, warn: (...args) => f.warnings.push(args) },
  });
  for (const name of ['loadSessions', 'loadWorkspaceState', 'reconcileSessionsAfterActionError', 'expectedActiveSessionIdForWrite']) {
    vm.runInContext(source(name), f.context);
  }
  f.snapshots = new Map();
  for (const id of ['session-a', 'session-b', 'session-c']) {
    const snapshot = f.remote([f.tab(`stored-${id}`)], 20); snapshot.session_id = id;
    f.snapshots.set(id, snapshot);
  }
  f.edit = (kind = 'model') => {
    f.state.activeTool = 'replay'; f.context.renderToolPanels();
    if (kind === 'invalid target') {
      f.document.activeElement = f.els.replayPortInput; f.els.replayPortInput.value = '8x';
      f.els.replayPortInput.listeners.input();
    } else {
      const text = kind === 'empty' ? '' : 'GET /pending-action-draft HTT';
      f.els.replayRequestHighlight.innerText = text;
      if (kind === 'model') f.context.syncReplayRequestTextFromEditor(text);
    }
  };
  f.retained = () => ({ owner: f.state.activeSession, generation: f.context.workspaceStateGeneration,
    tabs: f.state.replayTabs, selected: f.state.selectedSessionId, active: f.state.activeReplayTabId,
    baseline: f.context.workspaceSaveCommittedSnapshot, text: f.els.replayRequestHighlight.innerText,
    port: f.els.replayPortInput.value, name: f.els.dashboardCreateSessionName.value });
  f.assertRetained = before => {
    const after = f.retained();
    for (const key of Object.keys(before)) assert.equal(after[key], before[key], key);
  };
  f.finishReload = async (action, index, owner = 'session-b') => {
    const next = f.requests.length; f.succeed(index, {}); await tick();
    assert.equal(f.requests[next]?.url, '/api/sessions'); f.succeed(next, f.rows(owner)); await tick();
    assert.equal(f.requests[next + 1]?.url, '/api/workspace-state');
    f.succeed(next + 1, f.snapshots.get(owner)); assert.equal(await action.pending, undefined);
  };
  f.finishRetained = async (action, index, owner = 'session-b') => {
    const next = f.requests.length; f.succeed(index, {}); await tick();
    assert.equal(f.requests[next]?.url, '/api/sessions'); assert.equal(f.requests[next].options, undefined);
    f.succeed(next, f.rows(owner)); await tick();
    assert.equal(f.requests.length, next + 1, 'completed action may only refresh rows, never reload or retry');
    assert.equal(await action.pending, undefined);
    assert.match(f.toasts.at(-1)[0], /action completed.*current workspace is still open/);
  };
  return f;
}
for (const operation of ['create', 'activate']) {
  for (const active of ['session-b', 'session-c']) test(`${operation}: old success preserves newer B workspace when listed active is ${active}`, async () => {
    const f = setupCompletedAction(), older = await f.start(operation, 'session-c');
    const newer = await f.start('activate', 'session-b'); await f.finishReload(newer, 1);
    f.edit(); const before = f.retained(), tab = f.context.getActiveReplayTab(), cleanup = f.log.filter(name => name === 'cleanupWsReplayTabsBeforeStateReset').length;
    await f.finishRetained(older, 0, active); f.assertRetained(before);
    assert.equal(tab.requestText, 'GET /pending-action-draft HTT'); assert.equal(f.context.workspaceSaveDirty, true);
    assert.equal(f.state.sessions.find(row => row.active).id, active);
    assert.equal(f.log.filter(name => name === 'cleanupWsReplayTabsBeforeStateReset').length, cleanup);
    if (active === 'session-c') assert.match(f.toasts.at(-1)[0], /active session changed.*Keep a copy.*Reload sessions/);
  });
  test(`${operation}: old failure after newer success preserves the original exception and B draft`, async () => {
    const f = setupCompletedAction(), older = await f.start(operation, 'session-c');
    const newer = await f.start('activate', 'session-b'); await f.finishReload(newer, 1);
    f.edit(); const before = f.retained(), count = f.requests.length, error = new Error('old action failed');
    f.requests[0].reject(error); assert.equal(await older.pending, error);
    f.assertRetained(before); assert.equal(f.requests.length, count);
  });
  test(`${operation}: old success after newer failure still follows its ordinary reload path`, async () => {
    const f = setupCompletedAction(), older = await f.start(operation, 'session-c');
    const newer = await f.start('activate', 'session-b');
    const error = new Error('new action failed'); f.requests[1].reject(error); await tick();
    assert.equal(f.requests[2].url, '/api/sessions'); f.succeed(2, f.rows('session-a'));
    assert.equal(await newer.pending, error); assert.equal(f.context.workspaceStateGeneration, 0);
    await f.finishReload(older, 0, 'session-c'); assert.equal(f.state.activeSession.id, 'session-c');
  });
  for (const kind of ['model', 'partial', 'empty', 'invalid target']) test(`${operation}: same-generation ${kind} entered while pending survives completion`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation); f.edit(kind);
    const before = f.retained(), tab = f.context.getActiveReplayTab();
    await f.finishRetained(action, 0); f.assertRetained(before);
    if (kind === 'invalid target') {
      assert.equal(tab.targetPort, '443'); assert.ok(f.els.replayPortInput.validationMessage);
    } else {
      assert.equal(tab.requestText, kind === 'empty' ? '' : 'GET /pending-action-draft HTT');
      assert.equal(f.context.workspaceSaveDirty, true);
    }
    assert.equal(f.context.workspaceStateGeneration, 0);
  });
  test(`${operation}: an already autosaved edit still prevents the old completion from resetting`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation); f.edit();
    const saving = f.context.flushQueuedWorkspaceStateSave();
    assert.equal(f.requests[1].url, '/api/workspace-state');
    f.succeed(1, { session_id: 'session-a', revision: 11 }); await saving;
    assert.equal(f.context.workspaceSaveDirty, false); const before = f.retained();
    await f.finishRetained(action, 0); f.assertRetained(before); assert.equal(f.context.workspaceSaveDirty, false);
  });
  test(`${operation}: retained autosave remains pinned and conflicts if the backend changed sessions`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation); f.edit();
    await f.finishRetained(action, 0, 'session-b'); const before = f.retained();
    const saving = f.context.flushQueuedWorkspaceStateSave(), body = JSON.parse(f.requests[2].options.body);
    assert.equal(body.session_id, 'session-a'); assert.equal(body.expected_active_session_id, 'session-a');
    f.requests[2].resolve({ ok: false, status: 409, json: async () => ({ error: 'active session changed', session_id: 'session-b' }) });
    await saving; f.assertRetained(before);
    assert.equal(f.context.workspaceSaveConflictPending, true); assert.equal(f.context.workspaceSaveDirty, true);
    assert.equal(f.requests.length, 3);
  });
  test(`${operation}: equal-text input notification still conservatively retains its newer save generation`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation);
    f.context.syncReplayRequestTextFromEditor(f.old.requestText); const before = f.retained();
    await f.finishRetained(action, 0); f.assertRetained(before);
  });
  for (const unchanged of ['untouched', 'same value']) test(`${operation}: ${unchanged} editor retains ordinary success behavior`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation);
    if (unchanged === 'same value') f.els.replayRequestHighlight.innerText = f.old.requestText;
    await f.finishReload(action, 0); assert.equal(f.state.activeSession.id, 'session-b');
    assert.equal(f.toasts.length, 0);
  });
}
for (const failure of ['network', 'HTTP', 'shape']) test(`completed action list-only ${failure} failure retains newer edits and gives recovery guidance`, async () => {
  const f = setupCompletedAction(), action = await f.start('activate'); f.edit(); const before = f.retained();
  f.succeed(0, {}); await tick(); assert.equal(f.requests[1].url, '/api/sessions');
  if (failure === 'network') f.requests[1].reject(new Error('synthetic read loss'));
  else if (failure === 'HTTP') f.requests[1].resolve({ ok: false });
  else f.succeed(1, { invalid: true });
  assert.equal(await action.pending, undefined); f.assertRetained(before);
  assert.match(f.toasts.at(-1)[0], /action completed.*could not be refreshed.*Keep a copy.*Reload sessions/);
  assert.equal(f.warnings.length, 1); assert.equal(f.requests.length, 2);
});
test('a newer workspace supersedes completed-action row refresh without stale feedback', async () => {
  const f = setupCompletedAction(), action = await f.start('activate'); f.edit();
  f.succeed(0, {}); await tick(); assert.equal(f.requests[1].url, '/api/sessions');
  f.context.resetSessionScopedUiState(); f.state.activeSession = { id: 'session-c' };
  f.context.applyWorkspaceState(f.snapshots.get('session-c')); f.context.workspaceLoaded = true;
  f.edit(); const before = f.retained(), toasts = f.toasts.length;
  f.succeed(1, f.rows('session-b')); await tick();
  assert.equal(f.requests.length, 2);
  assert.equal(await action.pending, undefined);
  f.assertRetained(before); assert.equal(f.toasts.length, toasts); assert.equal(f.requests.length, 2);
});
for (const changedName of [false, true]) test(`create completion ${changedName ? 'preserves a newer' : 'clears only its submitted'} name field`, async () => {
  const f = setupCompletedAction(); f.els.dashboardCreateSessionName.value = '  Original name  ';
  const action = await f.start('create'); assert.equal(JSON.parse(f.requests[0].options.body).name, 'Original name');
  if (changedName) f.els.dashboardCreateSessionName.value = 'New name draft';
  await f.finishReload(action, 0);
  assert.equal(f.els.dashboardCreateSessionName.value, changedName ? 'New name draft' : '');
});
for (const changed of [false, true]) test(`forced activation ${changed ? 'retains edits made while pending' : 'keeps ordinary completion'}`, async () => {
  const f = setupCompletedAction();
  const action = { pending: f.context.activateSessionById('session-b', { force: true }).catch(error => error) };
  await tick(); assert.equal(f.requests[0].url, '/api/sessions/session-b/activate?force=true');
  if (changed) { f.edit(); const before = f.retained(); await f.finishRetained(action, 0); f.assertRetained(before); }
  else await f.finishReload(action, 0);
});

for (const operation of ['create', 'activate']) {
  test(`${operation}: older success can reload first, but later overlapping success retains that workspace`, async () => {
    const f = setupCompletedAction(), older = await f.start(operation, 'session-c');
    const newer = await f.start('activate', 'session-b'); await f.finishReload(older, 0, 'session-c');
    f.edit(); const before = f.retained();
    await f.finishRetained(newer, 1, 'session-b'); f.assertRetained(before);
    assert.equal(f.state.activeSession.id, 'session-c'); assert.equal(f.state.sessions.find(row => row.active).id, 'session-b');
  });
  test(`${operation}: pending legacy byte entered after dispatch remains in its input on success`, async () => {
    const f = setupCompletedAction(), action = await f.start(operation), hex = f.pendingHex();
    const before = f.retained(); await f.finishRetained(action, 0); f.assertRetained(before);
    assert.equal(hex.current(), hex.input); assert.equal(hex.input.value, 'f');
    assert.equal(f.old.requestBytes[0], 255); assert.equal(f.context.workspaceStateGeneration, 0);
  });
  test(`${operation}: clean binary editor still follows ordinary completion`, async () => {
    const f = setupCompletedAction(); f.state.replayMessageViews.request = 'hex';
    const bytes = f.old.requestBytes = new Uint8Array([255, 128]);
    const original = f.old.requestOriginalBytes = new Uint8Array([0, 128]);
    f.context.renderReplay(); const action = await f.start(operation);
    await f.finishReload(action, 0); assert.equal(f.toasts.length, 0);
    assert.equal(f.old.requestBytes, bytes); assert.equal(f.old.requestOriginalBytes, original);
  });
}
test('completed-action reconciliation cannot overwrite a newer accepted session list', async () => {
  const f = setupCompletedAction(), action = await f.start('activate'); f.edit(); const before = f.retained();
  f.succeed(0, {}); await tick(); assert.equal(f.requests[1].url, '/api/sessions');
  const newest = f.context.loadSessions({ preserveWorkspaceOwner: true });
  f.succeed(2, f.rows('session-c')); await newest;
  f.succeed(1, f.rows('session-b')); await tick();
  assert.equal(f.requests.length, 3);
  assert.equal(await action.pending, undefined);
  f.assertRetained(before); assert.equal(f.state.sessions.find(row => row.active).id, 'session-c');
  assert.equal(f.requests.length, 3); assert.match(f.toasts.at(-1)[0], /action completed and the active session changed/);
});
test('forced activation success is still owner-fenced after another activation reloads', async () => {
  const f = setupCompletedAction();
  const forced = { pending: f.context.activateSessionById('session-c', { force: true }).catch(error => error) };
  await tick(); const newer = await f.start('activate', 'session-b');
  await f.finishReload(newer, 1); f.edit(); const before = f.retained();
  await f.finishRetained(forced, 0, 'session-c'); f.assertRetained(before);
  assert.equal(f.requests[0].url, '/api/sessions/session-c/activate?force=true');
});

for (const operation of ['create', 'activate']) test(`${operation}: a real post-dispatch pin save prevents the action from resetting its workspace`, async () => {
  const f = setupCompletedAction(), action = await f.start(operation);
  vm.runInContext(source('toggleReplayTabPin'), f.context); f.context.toggleReplayTabPin(f.old.id);
  assert.equal(f.requests[1].url, '/api/workspace-state'); assert.equal(f.old.pinned, true);
  const before = f.retained(); await f.finishRetained(action, 0, 'session-b'); f.assertRetained(before);
  assert.equal(f.old.pinned, true); assert.equal(f.context.workspaceStateGeneration, 0);
  f.requests[1].resolve({ ok: false, status: 409, json: async () => ({ error: 'active session changed', session_id: 'session-b' }) });
  await tick(); assert.equal(f.context.workspaceSaveConflictPending, true);
  assert.equal(f.state.activeSession.id, 'session-a'); assert.equal(f.state.replayTabs, before.tabs);
  assert.equal(f.requests.length, 3);
});

// Entry/preflight races are separate from post-dispatch completion races. Each
// phase is held synthetically; the replacement uses production reset/hydration.
const preflightPhases = ['flushTargetScopeDraft', 'flushMatchReplaceDraft', 'flushSequenceDraft',
  'flushAllPendingAnnotations', 'flushWorkspaceState', 'cleanupWsReplayTabsBeforeStateReset'];
const stoppedPreflightMessage = 'Session action stopped before its request was sent because the workspace changed. Review the current workspace before trying again.';
function replacePreflightWorkspace(f, sameId = false) {
  const id = sameId ? 'session-a' : 'session-b';
  f.context.resetSessionScopedUiState(); f.state.activeSession = { id };
  f.context.applyWorkspaceState(f.snapshots.get(id)); f.context.workspaceLoaded = true;
  f.edit(); f.els.dashboardCreateSessionName.value = 'New workspace name';
  f.state.targetScopeDraft = 'new.example.com'; f.state.targetScopeDirty = true;
  f.state.targetScopeEditorSessionId = id;
}
function pausePreflight(f, phase) {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  const calls = [];
  for (const name of preflightPhases) f.context[name] = async () => {
    calls.push(name); return name === phase ? promise : true;
  };
  return { resolve, reject, calls };
}
for (const operation of ['create', 'activate']) {
  for (const phase of preflightPhases) for (const sameId of [false, true]) for (const outcome of ['success', 'false', 'rejection']) {
    test(`${operation}: ${phase} ${outcome} cannot continue across ${sameId ? 'same-ID reset' : 'new owner'}`, async () => {
      const f = setupCompletedAction(), gate = pausePreflight(f, phase);
      const action = await f.start(operation, 'session-c');
      assert.equal(gate.calls.at(-1), phase); assert.equal(f.requests.length, 0);
      replacePreflightWorkspace(f, sameId); const before = f.retained(), calls = [...gate.calls];
      if (outcome === 'rejection') gate.reject(new Error('Synthetic stale preflight failure'));
      else gate.resolve(outcome === 'success');
      await tick(); assert.equal(f.requests.length, 0, 'no session mutation or reconciliation before dispatch');
      assert.equal(await action.pending, undefined); assert.deepEqual(gate.calls, calls, 'no next preflight or cleanup');
      f.assertRetained(before); assert.equal(f.state.targetScopeDraft, 'new.example.com');
      assert.equal(f.state.targetScopeDirty, true); assert.equal(f.toasts.length, 1);
      assert.equal(f.toasts[0][0], stoppedPreflightMessage); assert.equal(f.confirms.length, 0);
    });
  }
  for (const phase of preflightPhases) test(`${operation}: current-owner ${phase} rejection propagates the identical error`, async () => {
    const f = setupCompletedAction(), gate = pausePreflight(f, phase), error = new Error('Synthetic current preflight error');
    const action = await f.start(operation); const calls = [...gate.calls]; gate.reject(error);
    assert.equal(await action.pending, error); assert.deepEqual(gate.calls, calls);
    assert.equal(f.requests.length, 0); assert.equal(f.toasts.length, 0); assert.equal(f.confirms.length, 0);
  });
  for (const phase of preflightPhases.slice(0, 3)) test(`${operation}: current-owner ${phase} refusal stays a quiet stop`, async () => {
    const f = setupCompletedAction(), gate = pausePreflight(f, phase), action = await f.start(operation);
    const calls = [...gate.calls]; gate.resolve(false); assert.equal(await action.pending, undefined);
    assert.deepEqual(gate.calls, calls); assert.equal(f.requests.length, 0); assert.equal(f.toasts.length, 0);
  });
  test(`${operation}: changed owner without a generation increment still cancels the preflight`, async () => {
    const f = setupCompletedAction(), gate = pausePreflight(f, preflightPhases[0]), action = await f.start(operation);
    f.state.activeSession = { id: 'session-b' }; gate.resolve(true); await tick();
    assert.equal(f.requests.length, 0); assert.equal(await action.pending, undefined);
    assert.equal(f.context.workspaceStateGeneration, 0); assert.equal(f.toasts[0][0], stoppedPreflightMessage);
  });
  test(`${operation}: a fresh action after cancellation can complete normally`, async () => {
    const f = setupCompletedAction(), gate = pausePreflight(f, preflightPhases[0]), stale = await f.start(operation);
    replacePreflightWorkspace(f); gate.resolve(true); await tick();
    assert.equal(f.requests.length, 0); assert.equal(await stale.pending, undefined);
    const fresh = await f.start(operation, 'session-c');
    assert.equal(f.requests.length, 1); assert.equal(f.toasts.length, 1);
    await f.finishReload(fresh, 0, 'session-c');
    assert.equal(f.state.activeSession.id, 'session-c'); assert.equal(f.toasts.length, 1);
  });
}

function useActualScopePreflight(f) {
  for (const name of ['flushTargetScopeDraft', 'saveTargetScope', 'formatScopePatternsText']) {
    vm.runInContext(source(name), f.context);
  }
  Object.assign(f.context, { renderInterceptStatus() {}, renderProxySettings() {}, scheduleRefresh() {} });
  let input;
  f.els.targetScopeEditor = { value: '', addEventListener: (name, callback) => { input = callback; } };
  const marker = '  els.targetScopeEditor.addEventListener("input", () => {';
  const binding = appSource.slice(appSource.indexOf(marker));
  vm.runInContext(binding.slice(0, binding.indexOf('\n  });') + 6), f.context);
  return value => { f.els.targetScopeEditor.value = value; input(); };
}
for (const operation of ['create', 'activate']) {
  test(`${operation}: actual older scope save cannot dispatch after newer Open B and consume B drafts`, async () => {
    const f = setupCompletedAction(), input = useActualScopePreflight(f); input('example.com');
    const older = await f.start(operation, 'session-c'), newer = await f.start('activate', 'session-b');
    for (const index of [0, 1]) {
      assert.equal(f.requests[index].url, '/api/runtime');
      assert.equal(JSON.parse(f.requests[index].options.body).session_id, 'session-a');
    }
    f.succeed(1, { scope_patterns: ['example.com'], excluded_scope_patterns: [] }); await tick();
    assert.equal(f.requests[2].url, '/api/sessions/session-b/activate'); await f.finishReload(newer, 2);
    f.state.activeTool = 'target'; f.context.renderToolPanels(); input('new.example.com');
    f.els.dashboardCreateSessionName.value = 'New B name draft';
    const before = f.retained(), cleanup = f.log.filter(name => name === 'cleanupWsReplayTabsBeforeStateReset').length;
    f.succeed(0, { scope_patterns: ['example.com'], excluded_scope_patterns: [] }); await tick();
    assert.equal(f.requests.length, 5, 'old action must stop before a mutation or B draft save');
    assert.equal(await older.pending, undefined); f.assertRetained(before);
    assert.equal(f.state.targetScopeDraft, 'new.example.com'); assert.equal(f.state.targetScopeDirty, true);
    assert.equal(f.state.targetScopeEditorSessionId, 'session-b');
    assert.equal(f.log.filter(name => name === 'cleanupWsReplayTabsBeforeStateReset').length, cleanup);
    assert.equal(f.toasts.at(-1)[0], stoppedPreflightMessage);
  });
  test(`${operation}: real same-owner preflight saves and a waiting name edit still permit ordinary dispatch`, async () => {
    const f = setupCompletedAction(), input = useActualScopePreflight(f); input('example.com');
    const action = await f.start(operation); f.els.dashboardCreateSessionName.value = '  Same owner new name  ';
    f.succeed(0, { scope_patterns: ['example.com'], excluded_scope_patterns: [] }); await tick();
    assert.equal(f.requests[1].url, operation === 'create' ? '/api/sessions' : '/api/sessions/session-b/activate');
    if (operation === 'create') assert.equal(JSON.parse(f.requests[1].options.body).name, 'Same owner new name');
    await f.finishReload(action, 1); assert.equal(f.toasts.length, 0);
    assert.equal(f.state.activeSession.id, 'session-b');
  });
}
test('force confirmation starts a fresh preflight owner, and a later reset cancels it before forced dispatch', async () => {
  const f = setupCompletedAction(), initial = await f.start('activate', 'session-c');
  f.requests[0].resolve({ ok: false, status: 409, text: async () => 'proxy activity is still running' });
  await initial.pending; assert.equal(f.confirms.length, 1);
  replacePreflightWorkspace(f);
  const gate = pausePreflight(f, preflightPhases[0]); f.confirms[0].callback(); await tick();
  assert.equal(gate.calls.length, 1, 'force retry must take a fresh entry owner');
  replacePreflightWorkspace(f, true); const before = f.retained(); gate.resolve(true); await tick();
  assert.equal(f.requests.length, 1); f.assertRetained(before);
  assert.equal(f.toasts.at(-1)[0], stoppedPreflightMessage); assert.equal(f.error, undefined);
});
for (const operation of ['create', 'activate']) test(`${operation}: an edit saved during same-owner preflight does not invalidate the dispatch version`, async () => {
  const f = setupCompletedAction(), input = useActualScopePreflight(f); input('example.com');
  const action = await f.start(operation), initialVersion = f.context.workspaceSaveVersion;
  f.edit(); assert.ok(f.context.workspaceSaveVersion > initialVersion);
  f.succeed(0, { scope_patterns: ['example.com'], excluded_scope_patterns: [] }); await tick();
  assert.equal(f.requests[1].url, '/api/workspace-state');
  const body = JSON.parse(f.requests[1].options.body);
  assert.equal(body.session_id, 'session-a'); assert.equal(body.expected_active_session_id, 'session-a');
  assert.equal(body.replay.tabs[0].request_text, 'GET /pending-action-draft HTT');
  f.succeed(1, { session_id: 'session-a', revision: 11 }); await tick();
  assert.equal(f.requests[2].url, operation === 'create' ? '/api/sessions' : '/api/sessions/session-b/activate');
  await f.finishReload(action, 2); assert.equal(f.toasts.length, 0);
  assert.equal(f.state.activeSession.id, 'session-b');
});
test('force confirmation after a newer reload captures that current owner and can complete normally', async () => {
  const f = setupCompletedAction(), initial = await f.start('activate', 'session-c');
  f.requests[0].resolve({ ok: false, status: 409, text: async () => 'proxy activity is still running' });
  await initial.pending;
  f.context.resetSessionScopedUiState(); f.state.activeSession = { id: 'session-b' };
  f.context.applyWorkspaceState(f.snapshots.get('session-b')); f.context.workspaceLoaded = true;
  const activate = f.context.activateSessionById; let pending;
  f.context.activateSessionById = (...args) => (pending = activate(...args));
  f.confirms[0].callback(); await tick();
  assert.equal(f.requests[1].url, '/api/sessions/session-c/activate?force=true');
  await f.finishReload({ pending }, 1, 'session-c'); assert.equal(f.toasts.length, 0);
  assert.equal(f.state.activeSession.id, 'session-c');
});

// A preserved leaf draft must also stop its real create/open caller. Otherwise
// the old unconditional flush success would authorize the following reset.
for (const operation of ['create', 'activate']) for (const editor of ['scope', 'rules']) {
  test(`${operation}: actual ${editor} preflight preserves an edit typed while its save waits`, async () => {
    const f = setupCompletedAction(); let edit;
    if (editor === 'scope') { edit = useActualScopePreflight(f); edit('example.com'); }
    else {
      for (const name of ['flushMatchReplaceDraft', 'saveMatchReplaceRules', 'getSelectedMatchReplaceRule', 'syncMatchReplaceEditor', 'sessionWritePath', 'jsonArray']) vm.runInContext(source(name), f.context);
      f.context.URLSearchParams = URLSearchParams;
      f.state.matchReplaceRules = [{ id: 'disabled-metadata', enabled: false, description: '', scope: 'request', target: 'any', search: '', replace: '', regex: false, case_sensitive: false }];
      f.state.selectedMatchReplaceRuleId = 'disabled-metadata'; f.state.matchReplaceEditorSessionId = 'session-a'; f.state.matchReplaceDirty = true;
      for (const [key, value] of Object.entries({ Scope: 'request', Target: 'any', Search: '', Replace: '' })) f.els[`matchReplace${key}`] = { value };
      f.els.matchReplaceRegex = { checked: false }; f.els.matchReplaceCaseSensitive = { checked: false };
      edit = () => { f.els.matchReplaceCaseSensitive.checked = true; f.context.syncMatchReplaceEditor(); };
    }
    const action = await f.start(operation), body = JSON.parse(f.requests[0].options.body);
    assert.equal(body.session_id, 'session-a'); edit('new.example.com'); const before = f.retained();
    f.succeed(0, editor === 'scope' ? { scope_patterns: ['example.com'], excluded_scope_patterns: [] } : body.rules);
    await tick(); assert.equal(f.requests.length, 1, 'no session mutation, workspace save, or retry after superseded preflight');
    assert.equal(await action.pending, undefined); f.assertRetained(before);
    assert.equal(f.log.includes('cleanupWsReplayTabsBeforeStateReset'), false);
    if (editor === 'scope') { assert.equal(f.state.targetScopeDraft, 'new.example.com'); assert.equal(f.state.targetScopeDirty, true); }
    else { assert.equal(f.state.matchReplaceRules[0].case_sensitive, true); assert.equal(f.state.matchReplaceDirty, true); }
    assert.equal(f.toasts.length, 1); assert.match(f.toasts[0][0], /earlier save completed/);
  });
}
