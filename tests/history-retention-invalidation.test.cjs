// Synthetic DOM/fetch regression tests for confirmed HTTP History cleanup only.
const assert = require('node:assert/strict');
const test = require('node:test');
const { appSource, loadFunctions } = require('./frontend-test-helpers.cjs');
const tick = () => new Promise((resolve) => setImmediate(resolve));
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function node() {
  const events = new Map(), classes = new Set();
  return { children: [], dataset: {}, value: '', textContent: '', attributes: {}, isConnected: true,
    classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name), contains: (name) => classes.has(name), toggle(name, value) { if (value) classes.add(name); else classes.delete(name); } },
    addEventListener(type, callback) { if (!events.has(type)) events.set(type, new Set()); events.get(type).add(callback); },
    removeEventListener(type, callback) { events.get(type)?.delete(callback); },
    dispatch(type, extra = {}) { const event = { target: this, preventDefault() {}, stopPropagation() {}, ...extra }; for (const callback of [...events.get(type) || []]) callback(event); },
    setAttribute(key, value) { this.attributes[key] = value; },
    append(...children) { this.children.push(...children); }, appendChild(child) { this.children.push(child); },
    replaceChildren(...children) { this.children = children; },
    select() {}, focus() {},
  };
}
function response(payload, status = 200) { return { ok: status < 400, status, json: async () => payload, text: async () => 'fixture missing' }; }
function fixture() {
  const row = { id: 'row-a', sequence: 1, method: 'GET', host: 'example.com', path: '/', has_user_note: true, note_preview: 'old' };
  const other = { id: 'row-b', sequence: 2, method: 'GET', host: 'example.com', path: '/second' };
  const state = { activeSession: { id: 'session-a', name: 'Fixture session' }, activeTool: 'proxy', activeProxyTab: 'http-history',
    items: [row, other], _itemsVersion: 0, historyPaging: { generation: 1, querySignature: 'query-a', total: 2, filteredTotal: 2, pageSize: 250 },
    selectedId: 'row-a', selectedRecord: { ...row }, filterSettings: {}, sortKey: 'index', sortDirection: 'desc' };
  const requests = [], toasts = [], renders = [], cells = [], timers = new Map(), confirmations = [], streams = [];
  let timerId = 0, annotationVersion = 0;
  const document = { ...node(), visibilityState: 'visible', activeElement: node(),
    querySelectorAll: () => cells.filter((cell) => cell.isConnected && cell.input).map((cell) => cell.input),
    createElement() { const el = node(); el.focus = () => { document.activeElement = el; document.dispatch('focusin', { target: el }); }; return el; },
  };
  const window = { ...node(), setTimeout(fn) { timers.set(++timerId, fn); return timerId; }, clearTimeout(id) { timers.delete(id); } };
  const els = { historyRecoveredNotes: node(), historyRecoveredNotesSummary: node(), historyRecoveredNotesList: node(),
    historyMeta: node(), liveStatus: node(), contextMenu: node(), displaySettingsModal: node(), filterModal: node() };
  els.contextMenu.classList.add('hidden'); els.displaySettingsModal.classList.add('hidden'); els.filterModal.classList.add('hidden');
  const functions = [
    'applyHistoryRetentionDeletedEvent', 'applyConfirmedHistoryRemoval', 'preserveDeletedHistoryNote', 'renderRecoveredHistoryNotes', 'discardRecoveredHistoryNote',
    'invalidateHistoryAfterEventGap', 'reconcileMissingHistoryNoteEditors', 'beginNoteEdit', 'truncateUtf8', 'truncateUtf8Preview', 'utf8ByteLength',
    'releaseHistoryNoteEdit', 'inheritPendingHistoryNoteEdit', 'currentHistoryNoteEditWindow', 'syncHistoryNoteEditWindow', 'retainFilteredHistoryNote',
    'updateAnnotations', 'flushPendingAnnotations', 'flushAnnotationsOnUnload', 'loadTransactionDetail', 'fetchTransactionPage',
    'createHistoryPagingState', 'loadTransactions', 'applyPendingAnnotationsToItems', 'applyTransactionDeltaEvent',
    'flushBeforeNativeClose', 'clearHttpHistorySelectionPreview', 'scheduleIncrementalRefresh', 'connectEvents',
  ];
  const context = loadFunctions(functions, {
    state, document, window, els, TextEncoder, MAX_ANNOTATION_NOTE_BYTES: 32768, WORKSPACE_UNLOAD_KEEPALIVE_MAX_BYTES: 60000,
    pendingHistoryNoteRead: null, contextMenuTargetId: null, _historyPagingGeneration: 1, _historyDetailGeneration: 0,
    _pendingTransactionSummaries: [], _searchActiveUntil: 0, _incrementalTimer: 0, _transactionDeltaTimer: 0,
    _historyFullLoadInFlight: 0, HTTP_HISTORY_PAGE_SIZE: 250, workspaceStateGeneration: 0,
    eventSource: null, _lastHttpHistoryFallbackPoll: 0, _lastWebsocketFallbackPoll: 0,
    uiSettingsSaveTimer: null, uiSettingsDirty: false, uiSettingsInFlight: false,
    annotationClientId: 'fixture-client', nextAnnotationClientVersion: () => ++annotationVersion,
    currentSessionId: () => state.activeSession?.id || null,
    transactionPath: (id, session) => `detail:${session}:${id}`,
    sessionQueryPath: (path, session) => `${path}?session_id=${session}`,
    sessionWritePath: (path, session) => `${path}?session_id=${session}`,
    fetch(url, options) { const request = { url, options, ...deferred() }; requests.push(request); return request.promise; },
    async requireOkResponse(res) { if (!res.ok) throw new Error('missing'); },
    showToast(...args) { toasts.push(args); }, closeContextMenu() {},
    isHttpHistoryVisible: () => true, isWebsocketHistoryVisible: () => false, isModalVisible: (el) => !el.classList.contains('hidden'),
    clearHistoryBackfill() {}, cancelHistoryDetailLoading() {}, observeAnnotationRevision() {},
    precomputeItemIndexes() {}, invalidateVisibleEntriesCache() {}, refreshHistoryPagingCursorFromItems() {},
    renderHistory(options) { renders.push(options); }, renderDetail() { renders.push('detail'); },
    renderEmptyDetail() { renders.push('empty'); }, updateHistorySelection() {},
    scheduleRefresh() { state.refreshScheduled = (state.refreshScheduled || 0) + 1; }, scheduleTransactionDeltaFlush() {},
    getHistoryItem: (id) => state.items.find((item) => item.id === id), getHistoryItemIndex: (id) => state.items.findIndex((item) => item.id === id),
    summaryMatchesActiveHistoryFilters: () => true, prepareHistoryItem() {}, resortLoadedHistoryItemsForCurrentSort: () => false,
    historyQuerySignature: () => 'query-a', createHistoryQueryState: () => ({ sessionId: state.activeSession.id }),
    isCurrentHistoryQuerySignature: (signature) => signature === 'query-a', buildTransactionsPageUrl: () => 'page:fixture',
    jsonArray: (items) => Array.isArray(items) ? items : [], updateHistoryPagingCursor() {},
    getVisibleEntries: () => state.items.map((item) => ({ item })), canReuseSelectedHistoryRecord: () => false,
    selectHistoryTransaction: async () => {}, canMergeRecentTransactions: () => true, resetHistoryScrollPosition() {},
    showConfirmDialog(message, callback, options) { confirmations.push({ message, callback, options }); },
    async flushAllPendingAnnotations() {}, async flushProxySettingsSaveBeforeClose() {}, async flushSequenceDraft() {},
    syncReplayDraftsBeforeWorkspaceClose() {}, async flushWsReplayTabsBeforeClose() {}, async flushWorkspaceState() {},
    console: { ...console, error() {} },
    EventSource: function (url) { this.url = url; this.events = new Map(); this.addEventListener = (name, callback) => this.events.set(name, callback); this.close = () => { this.closed = true; }; streams.push(this); },
  });
  function deletion(ids = ['row-a'], sequence = 10, generation = 'generation-a', session = 'session-a') {
    return { data: JSON.stringify({ session_id: session, ids, event_sequence: sequence, generation }), lastEventId: String(sequence) };
  }
  async function openNote(id = 'row-a', initial = 'old') {
    const cell = { ...node(), input: null, _html: 'old-note',
      get innerHTML() { return this._html; }, set innerHTML(value) { this._html = value; this.input = null; },
      querySelector() { return this.input; }, appendChild(input) { this.input = input; input.closest = () => ({ remove: () => { cell.isConnected = false; } }); },
      contains(target) { return target === this || target === this.input; },
    }; cells.push(cell);
    const opening = context.beginNoteEdit(cell, id); requests.at(-1).resolve(response({ id, user_note: initial })); await opening;
    return { cell, input: cell.input };
  }
  function runTimers() { const jobs = [...timers.values()]; timers.clear(); return jobs.map((fn) => fn()); }
  return { context, state, requests, toasts, renders, cells, timers, confirmations, streams, document, els, deletion, openNote, runTimers, row, other };
}

test('confirmed deletion clears selected row explicitly and invalidates paging/detail/queued summaries', () => {
  const f = fixture(); f.context._pendingTransactionSummaries.push({ sessionId: 'session-a', summary: f.row });
  assert.equal(f.context.applyHistoryRetentionDeletedEvent(f.deletion()), true);
  assert.deepEqual(f.state.items.map((row) => row.id), ['row-b']);
  assert.equal(f.state.selectedId, null); assert.equal(f.state.selectedRecord, null);
  assert.equal(f.state._historyRetentionClearedSelection, true);
  assert.equal(f.context._pendingTransactionSummaries.length, 0);
  assert.equal(f.state.historyPaging.generation, 2); assert.equal(f.context._historyDetailGeneration, 1);
  assert.equal(f.state._historyRetentionEpoch, 1); assert.equal(f.state.refreshScheduled, 1);
  assert.match(f.toasts[0][0], /selected.*permanently removed/);
});

test('duplicate/stale deletion, wrong session and malformed events cannot mutate state', () => {
  const f = fixture(); f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a'], 10));
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-b'], 9));
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-b'], 10));
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-b'], 20, 'generation-a', 'session-b'));
  assert.equal(f.context.applyHistoryRetentionDeletedEvent({ data: '{' }), false);
  assert.deepEqual(f.state.items.map((row) => row.id), ['row-b']); assert.equal(f.state._historyRetentionEpoch, 1);
});

test('new store generation accepts a new boundary and keeps only the latest deletion batch', () => {
  const f = fixture(); f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a'], 100));
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-b'], 1, 'generation-b'));
  assert.equal(f.state._historyRetentionEpoch, 2); assert.equal(f.state._historyRetentionEvents.generation, 'generation-b');
  assert.deepEqual([...f.state._historyDeletedIds], ['row-b']);
});

for (const composing of [false, true]) {
  test(`deleted live ${composing ? 'IME' : 'dirty'} editor preserves exact local text and cancels later saves`, async () => {
    const f = fixture(); const { input, cell } = await f.openNote();
    if (composing) input.dispatch('compositionstart');
    input.value = '  exact draft text  '; input.dispatch('input', { isComposing: composing });
    f.context.applyHistoryRetentionDeletedEvent(f.deletion());
    const saved = f.state.historyRecoveredNotes.get('session-a:row-a');
    assert.equal(saved.text, '  exact draft text  '); assert.equal(saved.sessionName, 'Fixture session');
    assert.equal(cell.isConnected, false); assert.equal(input.hasPendingNoteEdit(), false);
    input.dispatch('compositionend'); input.dispatch('blur'); input.dispatch('keydown', { key: 'Enter' }); f.runTimers();
    assert.equal(f.requests.length, 1, 'no annotation write after confirmed deletion');
    assert.equal(f.els.historyRecoveredNotes.open, true);
    const textarea = f.els.historyRecoveredNotesList.children[0].children[0].children[1];
    assert.equal(textarea.value, '  exact draft text  '); assert.equal(textarea.readOnly, true);
  });
}

test('unchanged deleted editor does not invent an unsaved recovery', async () => {
  const f = fixture(); await f.openNote(); f.context.applyHistoryRetentionDeletedEvent(f.deletion());
  assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
});

test('queued/in-flight deleted annotation text is recovered without disturbing other IDs or color metadata', async () => {
  const f = fixture();
  const deleted = { sessionId: 'session-a', payload: { user_note: 'pending text', color_tag: 'blue' } };
  const other = { sessionId: 'session-a', payload: { user_note: 'other draft', color_tag: 'red' } };
  f.state._pendingAnnotations = new Map([['row-a', deleted], ['row-b', other]]);
  const saving = f.context.flushPendingAnnotations('row-a');
  f.context.applyHistoryRetentionDeletedEvent(f.deletion());
  assert.equal(f.state._pendingAnnotations.get('row-b'), other);
  assert.equal(other.payload.color_tag, 'red'); assert.equal(f.state._pendingAnnotations.has('row-a'), false);
  assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, 'pending text');
  f.requests[0].resolve(response({ ...f.row, note_preview: 'server ack', annotation_revision: 2 }));
  assert.equal(await saving, false); assert.equal(f.state.items.some((row) => row.id === 'row-a'), false);
  assert.equal(f.state._annotationInFlight.size, 0); assert.equal(f.requests.length, 1);
});

test('late parsed annotation acknowledgement cannot resurrect filtered-row recovery', async () => {
  const f = fixture(); const body = deferred();
  const editor = { cell: node(), sessionId: 'session-a', closed: false, removed: null };
  const oldWindow = { editors: new Set([editor]), order: new Map([['row-a', 0]]) };
  editor.removed = { item: f.row, window: oldWindow };
  const entry = { sessionId: 'session-a', payload: { user_note: 'keep me' }, noteEdit: editor };
  f.state._pendingAnnotations = new Map([['row-a', entry]]);
  const saving = f.context.flushPendingAnnotations('row-a'); f.requests[0].resolve({ ok: true, status: 200, json: () => body.promise }); await tick();
  f.context.applyHistoryRetentionDeletedEvent(f.deletion());
  body.resolve({ ...f.row, has_user_note: true, annotation_revision: 3 }); assert.equal(await saving, false);
  assert.equal(editor.removed, null); assert.equal(oldWindow.editors.size, 0);
  assert.equal(f.state.items.some((row) => row.id === 'row-a'), false);
});

test('a late failed deleted annotation does not retry or recreate its pending entry', async () => {
  const f = fixture(); f.state._pendingAnnotations = new Map([['row-a', { sessionId: 'session-a', payload: { user_note: 'draft' } }]]);
  const saving = f.context.flushPendingAnnotations('row-a'); f.context.applyHistoryRetentionDeletedEvent(f.deletion());
  f.requests[0].reject(new Error('connection lost')); assert.equal(await saving, false);
  assert.equal(f.state._pendingAnnotations.size, 0); assert.equal(f.requests.length, 1);
  f.context.flushAnnotationsOnUnload(); assert.equal(f.requests.length, 1);
});

test('authoritative annotation 404 recovers pending text even when the deletion event was lost', async () => {
  const f = fixture(); const { input } = await f.openNote(); input.value = ' unsaved final text '; input.dispatch('input'); f.runTimers();
  const request = f.requests.at(-1); assert.equal(request.options.method, 'PATCH');
  request.resolve(response(null, 404)); await tick();
  assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, ' unsaved final text ');
  assert.equal(f.state._pendingAnnotations.size, 0); assert.equal(f.state.selectedId, null);
  assert.equal(f.requests.filter((r) => r.options?.method === 'PATCH').length, 1);
});

for (const parsing of [false, true]) {
  test(`stale page and detail ${parsing ? 'JSON parsing' : 'fetch'} responses cannot resurrect removed records`, async () => {
    const f = fixture(); const pageBody = deferred(), detailBody = deferred();
    const page = f.context.fetchTransactionPage(); const detail = f.context.loadTransactionDetail('row-a');
    if (parsing) { f.requests[0].resolve({ ok: true, json: () => pageBody.promise }); f.requests[1].resolve({ ok: true, json: () => detailBody.promise }); await tick(); }
    f.context.applyHistoryRetentionDeletedEvent(f.deletion());
    if (parsing) { pageBody.resolve({ items: [f.row] }); detailBody.resolve(f.row); }
    else { f.requests[0].resolve(response({ items: [f.row] })); f.requests[1].resolve(response(f.row)); }
    assert.equal(await page, null); assert.equal(await detail, null); assert.equal(f.state.selectedRecord, null);
  });
}

test('queued and late transaction events at/before retention sequence are ignored; newer ones still queue', () => {
  const f = fixture(); f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.row), lastEventId: '1' });
  assert.equal(f.context._pendingTransactionSummaries.length, 1);
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a'], 10));
  for (const id of ['', '9', '10']) f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.row), lastEventId: id });
  assert.equal(f.context._pendingTransactionSummaries.length, 0);
  f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.other), lastEventId: '11' });
  assert.equal(f.context._pendingTransactionSummaries.length, 1);
});

test('gap invalidates in-flight page and summary work without assuming any specific ID was deleted', async () => {
  const f = fixture(); const page = f.context.fetchTransactionPage();
  f.context._pendingTransactionSummaries.push({ summary: f.row }); f.context.invalidateHistoryAfterEventGap();
  f.requests[0].resolve(response({ items: [f.row] })); assert.equal(await page, null);
  assert.equal(f.context._pendingTransactionSummaries.length, 0);
  assert.deepEqual(f.state.items.map((row) => row.id), ['row-a', 'row-b']);
  assert.equal(f.state._historyRetentionReconcile, true);
});

for (const missing of [false, true]) {
  test(`missing edited row after gap is verified before recovery (404=${missing})`, async () => {
    const f = fixture(); const { input, cell } = await f.openNote(); input.value = 'live unsaved'; input.dispatch('input');
    f.context.invalidateHistoryAfterEventGap(); f.state.items = [f.other];
    f.context.reconcileMissingHistoryNoteEditors(); const check = f.requests.at(-1);
    assert.equal(check.url, 'detail:session-a:row-a');
    assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
    check.resolve(response({ id: 'row-a', user_note: 'old' }, missing ? 404 : 200)); await tick();
    assert.equal(f.state.historyRecoveredNotes?.size || 0, missing ? 1 : 0);
    assert.equal(cell.isConnected, !missing);
    if (!missing) assert.equal(input.hasPendingNoteEdit(), true, 'filtered/paged-out records still own their live editor');
  });
}

test('full refresh after a lost event retains the live editor until its missing-row lookup confirms deletion', async () => {
  const f = fixture(); const { input, cell } = await f.openNote(); input.value = 'not in page'; input.dispatch('input');
  f.context.invalidateHistoryAfterEventGap(); const loading = f.context.loadTransactions(true);
  const page = f.requests.at(-1); page.resolve(response({ items: [f.other], total: 1, filtered_total: 1, has_more: false })); await loading;
  assert.equal(cell.isConnected, true); assert.equal(f.state.selectedId, 'row-a');
  assert.equal(f.renders.at(-1).preserveNoteEditor, true);
  const lookup = f.requests.find((r, i) => i > 0 && r.url === 'detail:session-a:row-a');
  assert.ok(lookup); lookup.resolve(response(null, 404)); await tick();
  assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, 'not in page');
  assert.equal(f.state.selectedId, null);
});

test('generic background refresh also preserves unresolved live notes before any deletion event arrives', async () => {
  const f = fixture(); const { input } = await f.openNote(); input.value = 'pre-event draft'; input.dispatch('input');
  const loading = f.context.loadTransactions(true); f.requests.at(-1).resolve(response({ items: [f.other], total: 1, has_more: false })); await loading;
  assert.equal(f.renders.at(-1).preserveNoteEditor, true);
  assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
  f.requests.at(-1).resolve(response(null, 404)); await tick();
  assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, 'pre-event draft');
});

test('old availability read after session change cannot retire or recover the current session row', async () => {
  const f = fixture(); const { input } = await f.openNote(); input.value = 'draft'; input.dispatch('input'); f.state.items = [];
  f.context.reconcileMissingHistoryNoteEditors(); const check = f.requests.at(-1);
  f.state.activeSession = { id: 'session-b', name: 'Other session' }; f.state.items = [f.row];
  check.resolve(response(null, 404)); await tick();
  assert.equal(f.state.items.length, 1); assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
});

test('recovery remains across session/tab navigation, blocks native close, and only explicit discard removes it', async () => {
  const f = fixture(); f.context.preserveDeletedHistoryNote('row-a', 'session-a', ' copy this ');
  f.state.activeSession = { id: 'session-b', name: 'Other session' }; f.state.activeTool = 'dashboard'; f.context.renderRecoveredHistoryNotes();
  assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, ' copy this ');
  await assert.rejects(f.context.flushBeforeNativeClose(), /Copy and discard recovered unsaved notes/);
  f.context.discardRecoveredHistoryNote('session-a:row-a');
  assert.equal(f.state.historyRecoveredNotes.size, 1); assert.match(f.confirmations[0].message, /cannot be restored/);
  f.confirmations[0].callback(); assert.equal(f.state.historyRecoveredNotes.size, 0);
  await f.context.flushBeforeNativeClose();
  const reset = appSource.match(/^function resetSessionScopedUiState\([^]*?^\}/m)[0];
  assert.doesNotMatch(reset, /historyRecoveredNotes/);
});

test('a newer recovered draft survives an earlier discard confirmation', () => {
  const f = fixture(); f.context.preserveDeletedHistoryNote('row-a', 'session-a', 'first');
  f.context.discardRecoveredHistoryNote('session-a:row-a'); f.context.preserveDeletedHistoryNote('row-a', 'session-a', 'newer');
  f.confirmations[0].callback(); assert.equal(f.state.historyRecoveredNotes.get('session-a:row-a').text, 'newer');
});

test('existing live-note close guard still blocks without forcing blur or save', async () => {
  const f = fixture(); const { input } = await f.openNote(); input.value = 'draft'; input.dispatch('input');
  await assert.rejects(f.context.flushBeforeNativeClose(), /Finish or cancel the note edit/);
  assert.equal(input.hasPendingNoteEdit(), true); assert.equal(f.requests.length, 1);
});

test('stale EventSource or reset generation cannot deliver deletion/transaction/gap callbacks', () => {
  const f = fixture(); f.context.connectEvents(); const old = f.streams[0]; f.context.connectEvents(); const current = f.streams[1];
  old.events.get('retention_deleted')(f.deletion()); old.events.get('transaction')({ data: JSON.stringify(f.row), lastEventId: '11' }); old.events.get('transactions_gap')();
  assert.equal(f.state.items.length, 2); assert.equal(f.context._pendingTransactionSummaries.length, 0); assert.equal(f.state.refreshScheduled, undefined);
  f.context.workspaceStateGeneration += 1; current.events.get('retention_deleted')(f.deletion()); assert.equal(f.state.items.length, 2);
  f.context.connectEvents(); f.streams[2].events.get('retention_deleted')(f.deletion()); assert.equal(f.state.items.length, 1);
});

test('a retention-selected detail 404 explicitly clears selection without picking a neighboring record', async () => {
  const f = fixture(); f.state._historyRetentionReconcile = true;
  const detail = f.context.loadTransactionDetail('row-a'); f.requests[0].resolve(response(null, 404));
  assert.equal(await detail, null); assert.equal(f.state.selectedId, null); assert.equal(f.state._historyRetentionClearedSelection, true);
  assert.deepEqual(f.state.items.map((item) => item.id), ['row-b']);
});

test('late unrelated annotation acknowledgement schedules fresh read after removal epoch changes', async () => {
  const f = fixture();
  f.state._pendingAnnotations = new Map([['row-b', { sessionId: 'session-a', payload: { user_note: 'other note' } }]]);
  const saving = f.context.flushPendingAnnotations('row-b');
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a'])); const refreshes = f.state.refreshScheduled;
  f.requests[0].resolve(response({ ...f.other, note_preview: 'other note', annotation_revision: 1 }));
  assert.equal(await saving, true); assert.equal(f.state.refreshScheduled, refreshes + 1);
  assert.equal(f.state._pendingAnnotations.size, 0); assert.equal(f.state.items[0].id, 'row-b');
});

for (const parsing of [false, true]) {
  test(`incremental refresh started before removal cannot replace items/counts (${parsing ? 'during JSON' : 'before response'})`, async () => {
    const f = fixture(); const body = deferred(); f.context.scheduleIncrementalRefresh();
    const [refresh] = f.runTimers();
    if (parsing) { f.requests[0].resolve({ ok: true, json: () => body.promise }); await tick(); }
    f.context.applyHistoryRetentionDeletedEvent(f.deletion());
    if (parsing) body.resolve({ items: [f.row, f.other], total: 9000 });
    else f.requests[0].resolve(response({ items: [f.row, f.other], total: 9000 }));
    await refresh;
    assert.deepEqual(f.state.items.map((row) => row.id), ['row-b']);
    assert.notEqual(f.state.historyPaging.total, 9000);
  });
}

test('initial SSE connection and reconnect reconcile missed deletions; stale stream open is ignored', () => {
  const f = fixture(); f.context.connectEvents(); const first = f.streams[0];
  first.onopen(); assert.equal(f.state.refreshScheduled, 1);
  first.onopen(); assert.equal(f.state.refreshScheduled, 2);
  f.context.connectEvents(); first.onopen(); assert.equal(f.state.refreshScheduled, 2);
  f.streams[1].onopen(); assert.equal(f.state.refreshScheduled, 3);
});

test('retention SSE ID preserves a u64 sequence above Number precision', () => {
  const f = fixture();
  const event = { data: '{"session_id":"session-a","ids":["row-a"],"generation":"generation-a","event_sequence":9007199254740993}', lastEventId: '9007199254740993' };
  assert.equal(f.context.applyHistoryRetentionDeletedEvent(event), true);
  assert.equal(f.state._historyRetentionEvents.sequence, '9007199254740993');
  f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.row), lastEventId: '9007199254740992' });
  assert.equal(f.context._pendingTransactionSummaries.length, 0);
  f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.other), lastEventId: '9007199254740994' });
  assert.equal(f.context._pendingTransactionSummaries.length, 1);
});

test('reconnect forgets the old store sequence boundary before accepting a restarted store update', () => {
  const f = fixture(); f.context.connectEvents(); f.streams[0].onopen();
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a'], 100));
  f.streams[0].onopen(); assert.equal(f.state._historyRetentionEvents, null);
  f.context.applyTransactionDeltaEvent({ data: JSON.stringify(f.other), lastEventId: '1' });
  assert.equal(f.context._pendingTransactionSummaries.length, 1);
});

test('initial SSE reconciliation performs only a page read and keeps a still-existing selection stable', async () => {
  const vm = require('node:vm');
  const f = fixture();
  // Install the real refresh timer handler as well as the real SSE/page handlers.
  vm.runInContext(appSource.match(/^function scheduleRefresh\([^]*?^\}/m)[0], f.context);
  f.context.refreshTimer = null;
  f.context.consumeHistoryLoadOptions = () => ({});
  f.context.canReuseSelectedHistoryRecord = (id) => f.state.selectedRecord?.id === id;
  f.context.connectEvents(); f.streams[0].onopen(); f.runTimers();
  assert.equal(f.requests.length, 1); assert.equal(f.requests[0].url, 'page:fixture');
  assert.equal(f.requests[0].options, undefined);
  f.requests[0].resolve(response({ items: [f.row, f.other], total: 2, filtered_total: 2, has_more: false })); await tick();
  assert.equal(f.state.selectedId, 'row-a'); assert.equal(f.state.selectedRecord.id, 'row-a');
  assert.equal(f.requests.length, 1); assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
});

test('deleting only a stale A preview does not clear the surviving B selected by a passive menu action', async () => {
  const vm = require('node:vm'); const f = fixture();
  const action = node(); action.dataset.action = 'copy-url';
  f.els.contextMenu.querySelectorAll = () => [action];
  f.context.contextMenuRecordTarget = { id: 'row-b', sessionId: 'session-a' };
  f.context.contextMenuSessionId = 'session-a'; f.context.contextMenuTargetId = 'row-b';
  f.context.contextMenuSessionIsCurrent = () => true;
  f.context.copyMenuTargetUrl = () => {};
  const start = appSource.indexOf('els.contextMenu.querySelectorAll(".context-menu-item").forEach((item) => {');
  const end = appSource.indexOf('\n/* ─── WS Frame context menu', start);
  vm.runInContext(appSource.slice(start, end), f.context);
  action.dispatch('click');
  assert.equal(f.state.selectedId, 'row-b'); assert.equal(f.state.selectedRecord.id, 'row-a');
  const reading = f.context.loadTransactionDetail('row-b');
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a']));
  assert.equal(f.state.selectedId, 'row-b', 'removing the old preview must not clear a surviving newer selection');
  assert.equal(f.state.selectedRecord, null);
  f.requests[0].resolve(response(f.other)); assert.equal(await reading, null, 'the old detail generation remains invalidated');
  assert.equal(f.state.refreshScheduled, 1, 'the surviving selection will be re-fetched in the authoritative refresh');
});

test('actual A→B row click survives A deletion and loads B with a fresh detail request', async () => {
  const vm = require('node:vm'); const f = fixture();
  for (const name of ['selectHistoryTransaction', 'scheduleRefresh']) vm.runInContext(appSource.match(new RegExp(`^(?:async )?function ${name}\\([^]*?^\\}`, 'm'))[0], f.context);
  f.els.historyTableBody = node(); f.els.trafficRegion = node();
  f.context.scheduleHistoryDetailLoading = () => {}; f.context.scrollSelectedHistoryRowIntoView = () => {};
  f.context.refreshTimer = null; f.context.consumeHistoryLoadOptions = () => ({});
  const start = appSource.indexOf('  els.historyTableBody.addEventListener("click", (event) => {');
  const end = appSource.indexOf('  els.historyTableBody.addEventListener("dblclick"', start);
  vm.runInContext(appSource.slice(start, end), f.context);
  f.els.historyTableBody.dispatch('click', { target: { closest: () => ({ dataset: { id: 'row-b' } }) } });
  assert.equal(f.state.selectedId, 'row-b'); assert.equal(f.state.selectedRecord, null);
  const oldDetail = f.requests[0];
  // Real DOM rows support querySelectorAll; this fixture has no rendered rows.
  f.els.historyTableBody.querySelectorAll = () => [];
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a']));
  assert.equal(f.state.selectedId, 'row-b'); f.runTimers();
  const page = f.requests.at(-1); assert.equal(page.url, 'page:fixture');
  page.resolve(response({ items: [f.other], total: 1, filtered_total: 1, has_more: false })); await tick();
  const freshDetail = f.requests.at(-1); assert.equal(freshDetail.url, 'detail:session-a:row-b'); assert.notEqual(freshDetail, oldDetail);
  oldDetail.resolve(response({ ...f.other, marker: 'stale' })); await tick(); assert.equal(f.state.selectedRecord, null);
  freshDetail.resolve(response({ ...f.other, marker: 'fresh' })); await tick();
  assert.equal(f.state.selectedId, 'row-b'); assert.equal(f.state.selectedRecord.marker, 'fresh');
  assert.equal(f.state._historyRetentionReconcile, false);
  assert.equal(f.requests.length, 3); assert.ok(f.requests.every((request) => request.options === undefined));
});

test('successful initial reconciliation releases its guard; later default-Off autoselection retains legacy behavior', async () => {
  const f = fixture(); f.context.invalidateHistoryAfterEventGap(); f.context.canReuseSelectedHistoryRecord = () => true;
  let loading = f.context.loadTransactions(true); f.requests.at(-1).resolve(response({ items: [f.row, f.other], total: 2, has_more: false })); await loading;
  assert.equal(f.state._historyRetentionReconcile, false); assert.equal(f.state.selectedId, 'row-a');
  loading = f.context.loadTransactions(true); f.requests.at(-1).resolve(response({ items: [f.other], total: 1, has_more: false })); await loading;
  assert.equal(f.state.selectedId, 'row-b');
  assert.equal(f.requests.filter((r) => r.options).length, 0);
});

test('confirmed 404 recovery uses neutral cause wording, not an automatic-cleanup claim', async () => {
  const f = fixture(); f.state._historyRetentionReconcile = true;
  const reading = f.context.loadTransactionDetail('row-a'); f.requests[0].resolve(response(null, 404)); await reading;
  assert.match(f.toasts[0][0], /no longer available/); assert.doesNotMatch(f.toasts[0][0], /automatic cleanup/);
  const fs = require('node:fs'), path = require('node:path');
  const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
  assert.match(html, /The original record is no longer available\. This unsaved text/);
});

test('failed background refresh preserves an unresolved live dirty editor and existing close guard', async () => {
  const f = fixture(); const { input, cell } = await f.openNote(); input.value = 'still local'; input.dispatch('input');
  f.context.invalidateHistoryAfterEventGap(); const loading = f.context.loadTransactions(true); f.requests.at(-1).reject(new Error('read unavailable'));
  await assert.rejects(loading, /read unavailable/);
  assert.equal(cell.isConnected, true); assert.equal(input.value, 'still local'); assert.equal(input.hasPendingNoteEdit(), true);
  assert.equal(f.renders.at(-1).preserveNoteEditor, true);
  assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
  await assert.rejects(f.context.flushBeforeNativeClose(), /Finish or cancel/);
});

test('deleted non-editor rows are removed from DOM even when another dirty editor defers table redraw', async () => {
  const f = fixture(); const { input, cell } = await f.openNote('row-b', 'old B'); input.value = 'keep B draft'; input.dispatch('input');
  const removed = [];
  f.els.historyTableBody = { querySelectorAll: () => [
    { dataset: { id: 'row-a' }, remove: () => removed.push('row-a') },
    { dataset: { id: 'row-b' }, remove: () => removed.push('row-b') },
  ] };
  f.context.applyHistoryRetentionDeletedEvent(f.deletion(['row-a']));
  assert.deepEqual(removed, ['row-a']); assert.equal(cell.isConnected, true); assert.equal(input.hasPendingNoteEdit(), true);
  assert.equal(f.state.historyRecoveredNotes?.size || 0, 0);
});
