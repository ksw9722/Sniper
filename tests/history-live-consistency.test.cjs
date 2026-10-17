// Deferred responses exercise the real history, detail, paging, and annotation
// controllers. All records are synthetic; no server or captured traffic is used.
const assert = require("node:assert/strict");
const test = require("node:test");
const { loadFunctions } = require("./frontend-test-helpers.cjs");
const noop = () => {};
const tick = () => new Promise(setImmediate);
function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}
const response = (body) => ({ ok: true, json: async () => body });
const row = (sequence, overrides = {}) => ({
  id: `fixture-${sequence}`, sequence, method: "GET", host: "example.com", path: "/",
  status: 200, has_response: true, response_bytes: 0, note_count: 1,
  has_user_note: false, note_preview: "Streaming response capture is still in progress.",
  annotation_revision: 1, ...overrides,
});
function fixture(items = [row(1)]) {
  const state = {
    items: items.map((item) => ({ ...item })), selectedId: null, selectedRecord: null,
    sortKey: "index", sortDirection: "desc", filterSettings: { onlyNotes: true },
    _itemsVersion: 0, _pendingAnnotations: new Map(), _annotationInFlight: new Set(),
    historyPaging: {
      generation: 1, querySignature: "query-a", pageSize: 3, offset: items.length,
      total: items.length, filteredTotal: items.length, beforeSequence: items.at(-1)?.sequence ?? null,
      hasMore: false, fullyLoaded: true, loading: false, trimmedHeadCount: 0, trimmedTailCount: 0,
    },
  };
  const requests = [], detailRenders = [], renders = [], pending = [], refreshes = [];
  const c = loadFunctions([
    "flushTransactionDeltas", "applyLiveSummaryUpdate", "summaryMatchesActiveHistoryFilters",
    "canMergeRecentTransactions", "mergeHistoryItems", "trimHistoryCache", "keepLiveEvictedSelection",
    "reconcileHistorySelectionAfterTrim", "moveHistorySelectionIfMissing", "getVisibleEntries",
    "invalidateVisibleEntriesCache", "rebuildHistoryItemIndex", "getHistoryItem", "getHistoryItemIndex",
    "selectHistoryTransaction", "loadTransactionDetail", "canReuseSelectedHistoryRecord", "historyRecordSummarySignature",
    "clearHttpHistorySelectionPreview", "flushPendingAnnotations", "applyPendingAnnotationsToItems",
    "observeAnnotationRevision", "currentHistoryNoteEditWindow", "syncHistoryNoteEditWindow",
    "retainFilteredHistoryNote", "releaseHistoryNoteEdit", "adjustHistoryPagingAfterLocalRemoval",
    "refreshHistoryPagingCursorFromItems", "updateHistoryPagingCursor", "canUseSequenceCursorForHistoryPaging",
    "isKnownCount", "jsonArray", "loadMoreTransactions", "loadNewerTransactions",
  ], {
    state, session: "session-a", query: "query-a", annotationSaveVersion: 1,
    _historyDetailGeneration: 0, _historyFullLoadInFlight: 0, _searchActiveUntil: 0,
    _pendingTransactionSummaries: pending, HTTP_HISTORY_MAX_LOADED_ITEMS: 3,
    isHttpHistoryVisible: () => true, prepareHistoryItem: noop,
    scheduleTransactionDeltaFlush: noop, scheduleIncrementalRefresh: () => refreshes.push(true),
    scheduleHistoryBackfill: noop, adjustHistoryScrollAfterHeadTrim: noop,
    resortLoadedHistoryItemsForCurrentSort: () => false,
    summaryMatchesStatusFilter: () => true, summaryMatchesMimeFilter: () => true,
    summaryMatchesHiddenExtensions: () => true, summaryMatchesPortFilter: () => true,
    summaryMatchesColorTags: () => true, summaryMatchesAdvancedSearch: () => true, inferMimeType: () => "other",
    renderHistory: (options) => renders.push({ ids: state.items.map((item) => item.id), options }),
    renderDetail: (record) => detailRenders.push(structuredClone(record)), renderEmptyDetail: noop,
    updateHistorySelection: noop, scheduleHistoryDetailLoading: noop, scrollSelectedHistoryRowIntoView: noop,
    transactionPath: (id, session) => `fixture-only:${session}/${id}`,
    sessionWritePath: (path) => `fixture-only:${path}`,
    fetch(url, options) { const request = { url, options, ...deferred() }; requests.push(request); return request.promise; },
    showToast(message) { assert.fail(message); },
  });
  c.currentSessionId = () => c.session;
  c.createHistoryQueryState = () => ({ query: c.query });
  c.historyQuerySignature = (query = c.createHistoryQueryState()) => query.query;
  c.isCurrentHistoryQuerySignature = (signature) => signature === c.query;
  c.rebuildHistoryItemIndex();
  const deliver = (...summaries) => {
    for (const summary of summaries) pending.push({ sessionId: c.session, summary });
    c.flushTransactionDeltas();
  };
  return { c, state, requests, detailRenders, renders, pending, refreshes, deliver };
}
const complete = (item) => ({ ...item, note_count: 0, note_preview: undefined, response_bytes: 1226 });
const detailRequests = (f) => f.requests.filter((request) => !request.options);
async function save(f, payload, revision = 2) {
  const id = "fixture-1";
  const original = f.c.getHistoryItem(id) || row(1);
  f.state._pendingAnnotations.set(id, { sessionId: f.c.session, payload });
  const saving = f.c.flushPendingAnnotations(id);
  const summary = { ...original, annotation_revision: revision };
  if (Object.hasOwn(payload, "color_tag")) {
    delete summary.color_tag;
    if (payload.color_tag) summary.color_tag = payload.color_tag;
  }
  if (Object.hasOwn(payload, "user_note")) {
    summary.has_user_note = Boolean(payload.user_note);
    delete summary.note_preview;
    if (payload.user_note) summary.note_preview = payload.user_note.slice(0, 80);
  }
  f.requests.at(-1).resolve(response(summary));
  assert.equal(await saving, true);
  return summary;
}

test("a stream losing its only note leaves notes-only history and adjusts counts and cursor", () => {
  const f = fixture([row(2), row(1)]);
  f.deliver(complete(row(1)));
  assert.deepEqual(Array.from(f.c.getVisibleEntries(), (entry) => entry.item.id), ["fixture-2"]);
  assert.equal(f.c.getHistoryItem("fixture-1"), null);
  assert.equal(f.state.historyPaging.total, 2, "retained record still exists");
  assert.equal(f.state.historyPaging.filteredTotal, 1);
  assert.equal(f.state.historyPaging.offset, 1);
  assert.equal(f.state.historyPaging.beforeSequence, 2);
  assert.equal(f.state.historyPaging.localRemovalGeneration, 1);
});

test("a filtered-out selection rejects a late detail, while another selection stays unchanged", async () => {
  const f = fixture([row(2), row(1)]);
  const loading = f.c.selectHistoryTransaction("fixture-1");
  f.deliver(complete(row(1)));
  assert.equal(f.state.selectedId, null);
  detailRequests(f)[0].resolve(response(row(1)));
  assert.equal(await loading, null);
  assert.equal(f.state.selectedRecord, null);
  assert.deepEqual(f.detailRenders, []);

  f.state.items.push(row(1)); f.c.rebuildHistoryItemIndex();
  f.state.selectedId = "fixture-2";
  f.state.selectedRecord = { ...row(2), user_note: "Keep selected" };
  const selected = f.state.selectedRecord;
  f.deliver(complete(row(1)));
  assert.equal(f.state.selectedRecord, selected);
  assert.equal(f.state.selectedId, "fixture-2");
});

test("live completion keeps saved user annotations and stale-session events do nothing", () => {
  const f = fixture([row(1, { has_user_note: true, note_preview: "Saved", color_tag: "blue", annotation_revision: 9 })]);
  f.deliver(complete(row(1)));
  assert.equal(f.state.items.length, 1);
  assert.equal(f.state.items[0].note_preview, "Saved");
  assert.equal(f.state.items[0].color_tag, "blue");
  assert.equal(f.state.items[0].annotation_revision, 9);
  f.pending.push({ sessionId: "other-session", summary: row(1, { response_bytes: 99 }) });
  f.c.flushTransactionDeltas();
  assert.equal(f.state.items[0].response_bytes, 1226);
});

test("an existing-row removal is reconciled even when a fresh row needs a trimmed-window refresh", () => {
  const f = fixture();
  f.state.historyPaging.trimmedHeadCount = 3;
  f.deliver(complete(row(1)), row(5));
  assert.equal(f.state.items.length, 0);
  assert.equal(f.state.historyPaging.filteredTotal, 0);
  assert.equal(f.state.historyPaging.total, 1);
  assert.equal(f.refreshes.length, 1);
  assert.equal(f.renders.length, 1);
});

for (const direction of ["older", "newer"]) {
  test(`live filter removal invalidates a deferred ${direction} page and its counts`, async () => {
    const f = fixture([row(6), row(5), row(4)]);
    Object.assign(f.state.historyPaging, { total: 9, filteredTotal: 9, hasMore: true, fullyLoaded: false, trimmedHeadCount: 3 });
    let records = Array.from({ length: 9 }, (_, i) => row(9 - i));
    const pages = [];
    f.c.fetchTransactionPage = (options) => {
      const available = options.beforeSequence == null ? records : records.filter((item) => item.sequence < options.beforeSequence);
      const offset = options.offset || 0;
      const request = { options, page: { items: structuredClone(available.slice(offset, offset + 3)),
        total: 9, filtered_total: records.length, has_more: offset + 3 < available.length }, ...deferred() };
      pages.push(request); return request.promise;
    };
    const loading = direction === "older" ? f.c.loadMoreTransactions({ background: true }) : f.c.loadNewerTransactions({ background: true });
    records = records.filter((item) => item.id !== "fixture-4");
    f.deliver(complete(row(4)));
    pages[0].resolve(pages[0].page);
    await tick();
    if (pages[1]) pages[1].resolve(pages[1].page);
    await loading;
    assert.equal(pages.length, 2);
    assert.equal(f.state.historyPaging.filteredTotal, 8);
    assert.equal(f.c.getHistoryItem("fixture-4"), null);
    assert.equal(f.state.historyPaging.loading, false);
    if (direction === "older") assert.equal(pages[1].options.beforeSequence, 5);
  });
}

for (const field of ["user_note", "color_tag"]) {
  for (const clearing of [false, true]) {
    for (const duringParsing of [false, true]) {
      test(`late detail ${duringParsing ? "JSON" : "fetch"} cannot undo an acknowledged ${clearing ? "cleared" : "saved"} ${field}`, async () => {
        const f = fixture([row(1, { has_user_note: true, note_preview: "old note", color_tag: "red" })]);
        f.state.filterSettings.onlyNotes = false;
        const stale = { ...row(1), user_note: "old note", color_tag: "red", request: { body_preview: "original body" } };
        const loading = f.c.selectHistoryTransaction("fixture-1");
        const body = deferred();
        if (duringParsing) { detailRequests(f)[0].resolve({ ok: true, json: () => body.promise }); await tick(); }
        const value = clearing ? null : field === "user_note" ? "New note ".repeat(30) : "blue";
        await save(f, { [field]: value });
        if (duringParsing) body.resolve(stale);
        else detailRequests(f)[0].resolve(response(stale));
        await tick();
        const reads = detailRequests(f);
        const current = { ...stale, [field]: value, annotation_revision: 2 };
        if (reads[1]) reads[1].resolve(response(current));
        await loading;
        assert.equal(f.state.selectedRecord[field], value);
        assert.equal(f.state.selectedRecord.request.body_preview, "original body");
        assert.equal(f.state.selectedRecord.annotation_revision, 2);
        assert.equal(reads.length, 2, "reread full note rather than replacing it with its truncated summary preview");
        assert.equal(f.detailRenders.length, 1, "never render the obsolete annotations");
      });
    }
  }
}

for (const value of ["Unsaved note", null]) {
  test(`detail adoption overlays a pending note and tag, including ${value === null ? "clears" : "edits"}`, async () => {
    const f = fixture();
    f.state.filterSettings.onlyNotes = false;
    const loading = f.c.selectHistoryTransaction("fixture-1");
    f.state._pendingAnnotations.set("fixture-1", { sessionId: f.c.session,
      payload: { user_note: value, color_tag: null, client_id: "client", client_version: 99 } });
    detailRequests(f)[0].resolve(response({ ...row(1), user_note: "old", color_tag: "red" }));
    await loading;
    assert.equal(f.state.selectedRecord.user_note, value);
    assert.equal(f.state.selectedRecord.color_tag, null);
    assert.equal(f.state.selectedRecord.client_id, undefined);
    assert.equal(f.state.selectedRecord.annotation_revision, 1);
  });
}

test("a pending annotation from another session never overlays detail", async () => {
  const f = fixture();
  const loading = f.c.selectHistoryTransaction("fixture-1");
  f.state._pendingAnnotations.set("fixture-1", { sessionId: "other-session", payload: { user_note: "Other" } });
  detailRequests(f)[0].resolve(response({ ...row(1), user_note: "Current" }));
  await loading;
  assert.equal(f.state.selectedRecord.user_note, "Current");
});

test("repeated save acknowledgements invalidate repeated detail reads even after the paging window is replaced", async () => {
  const f = fixture();
  f.state.filterSettings.onlyNotes = false;
  const loading = f.c.selectHistoryTransaction("fixture-1");
  await save(f, { user_note: "First" }, 2);
  f.state.historyPaging = { ...f.state.historyPaging, annotationMutationGeneration: 0 };
  f.state.items = []; f.c.rebuildHistoryItemIndex();
  detailRequests(f)[0].resolve(response({ ...row(1), user_note: "Old" }));
  await tick();
  assert.equal(detailRequests(f).length, 2);
  await save(f, { user_note: "Second" }, 3);
  detailRequests(f)[1].resolve(response({ ...row(1), user_note: "First", annotation_revision: 2 }));
  await tick();
  assert.equal(detailRequests(f).length, 3);
  detailRequests(f)[2].resolve(response({ ...row(1), user_note: "Second", annotation_revision: 3 }));
  await loading;
  assert.equal(f.state.selectedRecord.user_note, "Second");
  assert.equal(f.detailRenders.length, 1);
});

for (const change of ["selection", "session"]) {
  test(`annotation-invalidated detail loses ownership after a ${change} change without retry`, async () => {
    const f = fixture([row(2), row(1)]);
    f.state.filterSettings.onlyNotes = false;
    const loading = f.c.selectHistoryTransaction("fixture-1");
    await save(f, { user_note: "Saved" });
    let newer;
    if (change === "selection") newer = f.c.selectHistoryTransaction("fixture-2");
    else f.c.session = "session-b";
    detailRequests(f)[0].resolve(response(row(1)));
    assert.equal(await loading, null);
    assert.equal(detailRequests(f).length, change === "selection" ? 2 : 1);
    assert.equal(f.detailRenders.length, 0);
    if (newer) { detailRequests(f)[1].resolve(response(row(2))); await newer; }
  });
}

test("a superseded save forces a fresh detail read while the newer pending edit stays visible", async () => {
  const f = fixture();
  f.state.filterSettings.onlyNotes = false;
  const loading = f.c.selectHistoryTransaction("fixture-1");
  f.state._pendingAnnotations.set("fixture-1", { sessionId: f.c.session, payload: { user_note: "First", color_tag: "blue" } });
  const saving = f.c.flushPendingAnnotations("fixture-1");
  const firstSave = f.requests.at(-1);
  f.state._pendingAnnotations.set("fixture-1", { sessionId: f.c.session, payload: { user_note: "Second", color_tag: "blue" } });
  firstSave.resolve(response({ ...row(1), has_user_note: true, note_preview: "First", color_tag: "blue", annotation_revision: 2 }));
  assert.equal(await saving, true);
  const secondSave = f.requests.at(-1);
  assert.equal(secondSave.options.method, "PATCH");
  detailRequests(f)[0].resolve(response({ ...row(1), user_note: "Old", color_tag: "red" }));
  await tick();
  assert.equal(detailRequests(f).length, 2);
  detailRequests(f)[1].resolve(response({ ...row(1), user_note: "First", color_tag: "blue", annotation_revision: 2 }));
  await loading;
  assert.equal(f.state.selectedRecord.user_note, "Second");
  assert.equal(f.state.selectedRecord.color_tag, "blue");
  assert.equal(f.detailRenders.length, 1);
  secondSave.resolve(response({ ...row(1), has_user_note: true, note_preview: "Second", color_tag: "blue", annotation_revision: 3 }));
  await tick();
  assert.equal(f.state.selectedRecord.user_note, "Second");
  assert.equal(f.state.selectedRecord.annotation_revision, 3);
  assert.equal(f.state._pendingAnnotations.size, 0);
});

test("a failed stale read cannot clear the selected record after a save acknowledgement", async () => {
  const f = fixture();
  f.state.filterSettings.onlyNotes = false;
  const loading = f.c.selectHistoryTransaction("fixture-1");
  await save(f, { user_note: "Saved" });
  detailRequests(f)[0].resolve({ ok: false, status: 500 });
  await tick();
  assert.equal(detailRequests(f).length, 2);
  assert.equal(f.state.selectedId, "fixture-1");
  detailRequests(f)[1].resolve(response({ ...row(1), user_note: "Saved", annotation_revision: 2 }));
  await loading;
  assert.equal(f.state.selectedRecord.user_note, "Saved");
});
