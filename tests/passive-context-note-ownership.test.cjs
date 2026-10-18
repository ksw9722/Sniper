// Recorded-note presentation only. Extract production row/dismiss handlers and
// menu helpers; every metadata read is synthetic, with no app or live requests.
const assert = require("node:assert/strict");
const test = require("node:test");
const vm = require("node:vm");
const { appSource, loadFunctions } = require("./frontend-test-helpers.cjs");
const tick = () => new Promise(resolve => setImmediate(resolve));

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function element() {
  const classes = new Set(["hidden"]);
  return {
    isConnected: true, innerHTML: "", style: {}, listeners: {},
    offsetWidth: 180, offsetHeight: 200,
    classList: {
      add: name => classes.add(name), remove: name => classes.delete(name),
      contains: name => classes.has(name),
      toggle(name, enabled) { if (enabled) classes.add(name); else classes.delete(name); },
    },
    querySelectorAll: () => [], contains(value) { return value === this; },
    addEventListener(name, callback) { this.listeners[name] = callback; },
  };
}

function fixture() {
  const requests = [], reads = [], presentations = [], errors = [];
  const els = Object.fromEntries([
    "historyTableBody", "contextMenu", "contextMenuNotes", "contextMenuNotesSection", "contextMenuNotesDivider",
  ].map(name => [name, element()]));
  const document = element();
  const state = { activeSession: { id: "session-a" }, _itemById: new Map() };
  const context = loadFunctions([
    "currentSessionId", "sessionQueryPath", "transactionPath", "getHistoryItem", "escapeHtml",
    "createTransactionRecordMenuTarget", "openContextMenu", "closeContextMenu", "loadUserNote",
    "renderContextMenuNotes", "positionContextMenu",
  ], {
    state, els, document,
    window: { innerWidth: 800, innerHeight: 600 },
    console: { error: (...args) => errors.push(args) },
    workspaceStateGeneration: 0, workspaceLoaded: true, clearReplaySendInFlight() {},
    // Selection is outside this passive surface. Keep a synthetic cached record
    // so the actual menu-target helper does not start its unrelated action read.
    async selectHistoryTransaction(id) { state.selectedRecord = { id }; },
    fetch(url, options) {
      assert.equal(options, undefined, "recorded-note reads cannot write or dispatch actions");
      assert.match(url, /^\/api\/transactions\/[ab]\?session_id=session-[ab]$/);
      const request = { url, ...deferred() }; requests.push(request); return request.promise;
    },
  });
  const declarations = appSource.indexOf("let contextMenuTargetId = null;");
  const open = appSource.indexOf("function openContextMenu(", declarations);
  assert.ok(declarations >= 0 && open > declarations);
  vm.runInContext(appSource.slice(declarations, open), context);
  const read = name => vm.runInContext(name, context);
  const load = context.loadUserNote;
  context.loadUserNote = id => { const promise = load(id); reads.push(promise); return promise; };
  for (const name of ["renderContextMenuNotes", "positionContextMenu"]) {
    const original = context[name];
    context[name] = (...args) => { presentations.push(name); return original(...args); };
  }
  function bind(marker, endMarker) {
    const start = appSource.indexOf(marker), end = appSource.indexOf(endMarker, start);
    assert.ok(start >= 0 && end > start, `Missing production binding: ${marker}`);
    vm.runInContext(appSource.slice(start, end), context);
  }
  bind('  els.historyTableBody.addEventListener("contextmenu", (event) => {', "\n\n  let _searchDebounce");
  bind('document.addEventListener("click", (event) => {\n  if (!els.contextMenu.contains(event.target))',
    '\n\nels.interceptContextMenu?.querySelectorAll');
  bind('document.addEventListener("keydown", (event) => {\n  if (event.key === "Escape" && !els.contextMenu.classList.contains("hidden"))',
    '\n\nels.contextMenu.querySelectorAll');
  // Run the exact reset entry through closeContextMenu; unrelated panel resets
  // and timers are omitted rather than starting the rest of the application.
  const reset = appSource.indexOf("function resetSessionScopedUiState() {");
  const resetEnd = appSource.indexOf("  window.clearTimeout(refreshTimer);", reset);
  assert.ok(reset >= 0 && resetEnd > reset);
  vm.runInContext(`${appSource.slice(reset, resetEnd)}\n}`, context);

  function show(id = "a", x = 780, y = 580) {
    const row = { dataset: { id } };
    let prevented = false;
    els.historyTableBody.listeners.contextmenu({ clientX: x, clientY: y,
      target: { closest: selector => selector === ".history-row" ? row : null },
      preventDefault() { prevented = true; },
    });
    assert.equal(prevented, true);
    return reads.at(-1);
  }
  const response = notes => ({ ok: true, json: async () => ({ notes }) });
  async function finishLatest(notes = ["Final recorded note"]) {
    requests.at(-1).resolve(response(notes));
    await reads.at(-1);
  }
  const close = () => document.listeners.keydown({ key: "Escape" });
  const outside = () => document.listeners.click({ target: {} });
  function resetSession(id) {
    state.activeSession = { id };
    context.resetSessionScopedUiState();
  }
  function snapshot() {
    return { target: read("contextMenuTargetId"), session: read("contextMenuSessionId"),
      notes: els.contextMenuNotes.innerHTML, hidden: els.contextMenu.classList.contains("hidden"),
      sectionHidden: els.contextMenuNotesSection.classList.contains("hidden"),
      dividerHidden: els.contextMenuNotesDivider.classList.contains("hidden"),
      position: { ...els.contextMenu.style }, presentations: presentations.length };
  }
  return { context, state, els, requests, reads, errors, show, finishLatest, response, close, outside, resetSession, snapshot };
}

async function pauseRead(f, stage) {
  const reading = f.show(), request = f.requests.at(-1);
  let pending = request, decoded = 0;
  if (stage === "JSON") {
    pending = deferred();
    request.resolve({ ok: true, json: () => { decoded++; return pending.promise; } });
    await tick();
    assert.equal(decoded, 1);
  }
  async function settle(outcome) {
    if (outcome === "rejection") pending.reject(new Error("Synthetic superseded note read failed"));
    else if (outcome === "non-OK") pending.resolve({ ok: false,
      json() { assert.fail("Non-OK note metadata must not be decoded"); } });
    else if (stage === "fetch") pending.resolve({ ok: true, json: async () => {
      decoded++; return { notes: ["Streaming response capture is still in progress."] };
    } });
    else pending.resolve({ notes: ["Streaming response capture is still in progress."] });
    await reading;
  }
  return { settle, decoded: () => decoded };
}

const transitions = {
  "same-row reopen": async f => { f.show(); await f.finishLatest(); },
  "close and same-row reopen": async f => { f.close(); f.show(); await f.finishLatest(); },
  "A→B→A": async f => { f.show("b"); await f.finishLatest(["Other row"]); f.show(); await f.finishLatest(); },
  "new target": async f => { f.show("b"); await f.finishLatest(); },
  "Escape close": async f => f.close(),
  "outside click close": async f => f.outside(),
  "session reset": async f => f.resetSession("session-b"),
  "session roundtrip and same-row reopen": async f => {
    f.resetSession("session-b"); f.resetSession("session-a"); f.show(); await f.finishLatest();
  },
  "detached menu": async f => { f.els.contextMenu.isConnected = false; },
  "replaced menu": async f => { f.els.contextMenu = element(); f.els.contextMenu.classList.remove("hidden"); },
};

for (const stage of ["fetch", "JSON"]) {
  for (const outcome of ["success", "rejection", ...(stage === "fetch" ? ["non-OK"] : [])]) {
    for (const [name, transition] of Object.entries(transitions)) {
      test(`actual ${name} ignores stale recorded-note ${stage} ${outcome}`, async () => {
        const f = fixture(), old = await pauseRead(f, stage);
        await transition(f);
        const before = f.snapshot(), decoded = old.decoded(), count = f.requests.length;
        await old.settle(outcome);
        assert.deepEqual(f.snapshot(), before, "stale reads cannot render or reposition any menu");
        assert.equal(old.decoded(), decoded, "stale fetch completion cannot begin decoding");
        assert.equal(f.requests.length, count);
        assert.deepEqual(f.errors, []);
      });
    }
  }
}

test("current recorded notes render escaped text, exclude invalid entries and reposition normally", async () => {
  const f = fixture(); f.show();
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].url, "/api/transactions/a?session_id=session-a");
  await f.finishLatest(["Final <recorded> note", "", "  ", null, 2, "Second & final"]);
  assert.deepEqual(f.snapshot(), { target: "a", session: "session-a",
    notes: "<p>Final &lt;recorded&gt; note</p><p>Second &amp; final</p>",
    hidden: false, sectionHidden: false, dividerHidden: false,
    position: { left: "612px", top: "392px" }, presentations: 4 });
  assert.deepEqual(f.errors, []);
});

for (const notes of [[], undefined, null, "not a notes array"]) {
  test(`current ${JSON.stringify(notes)} recorded notes keep an empty section hidden`, async () => {
    const f = fixture(); f.show(); await f.finishLatest(["Initial recorded note"]);
    f.show("b");
    f.requests.at(-1).resolve(f.response(notes)); await f.reads.at(-1);
    assert.equal(f.snapshot().notes, "");
    assert.equal(f.snapshot().sectionHidden, true);
    assert.equal(f.snapshot().dividerHidden, true);
    assert.equal(f.snapshot().hidden, false);
    assert.deepEqual(f.errors, []);
  });
}

for (const outcome of ["non-OK", "fetch rejection", "JSON rejection"]) {
  test(`current ${outcome} keeps the cleared menu open without disturbing it`, async () => {
    const f = fixture(); f.show(); await f.finishLatest(["Initial recorded note"]);
    const reading = f.show("b"), request = f.requests.at(-1), before = f.snapshot();
    if (outcome === "fetch rejection") request.reject(new Error("Synthetic current note read failed"));
    else if (outcome === "JSON rejection") request.resolve({ ok: true,
      json: async () => { throw new Error("Synthetic current note decode failed"); } });
    else request.resolve({ ok: false, json() { assert.fail("Non-OK metadata is not decoded"); } });
    await reading;
    assert.deepEqual(f.snapshot(), before);
    assert.equal(before.hidden, false); assert.equal(before.notes, "");
    assert.deepEqual(f.errors, []);
  });
}

for (const latest of ["pending", "failed"]) {
  test(`stale success cannot fill a ${latest} same-row reopen`, async () => {
    const f = fixture(), old = await pauseRead(f, "JSON");
    const reading = f.show();
    if (latest === "failed") {
      f.requests.at(-1).reject(new Error("Synthetic newest note read failed")); await reading;
    }
    const before = f.snapshot(); await old.settle("success");
    assert.deepEqual(f.snapshot(), before); assert.equal(before.notes, "");
    if (latest === "pending") { await f.finishLatest(); assert.match(f.snapshot().notes, /Final recorded note/); }
    assert.deepEqual(f.errors, []);
  });
}
