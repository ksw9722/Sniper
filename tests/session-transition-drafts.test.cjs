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
