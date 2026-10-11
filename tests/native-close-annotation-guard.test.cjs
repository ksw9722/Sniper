// Saved note/settings metadata only. Actual frontend handlers and the native
// wrapper run in a VM; fetch, timers, IPC and other lifecycle steps are inert.
// This covers queued annotations, not an editor's still-unfired debounce.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const { appSource, loadFunctions } = require("./frontend-test-helpers.cjs");

const desktopSource = fs.readFileSync(path.join(__dirname, "../src/bin/sniper-desktop.rs"), "utf8")
  .replace(/\r\n/g, "\n");
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function nativeCloseScript(generation) {
  const template = desktopSource.match(/fn desktop_close_flush_script\(generation: u64\)[^]*?r#"([^]*?)"#/);
  assert.ok(template, "Missing native close wrapper");
  let script = template[1].replaceAll("{{", "{").replaceAll("}}", "}");
  for (const name of ["DESKTOP_CLOSE_FLUSH_OK_PREFIX", "DESKTOP_CLOSE_FLUSH_ERROR_PREFIX"]) {
    const prefix = desktopSource.match(new RegExp(`const ${name}: &str = "([^"]+)";`));
    assert.ok(prefix, `Missing ${name}`);
    script = script.replaceAll(`{${name}}`, prefix[1]);
  }
  return script.replaceAll("{generation}", String(generation));
}

function eventTarget() {
  const listeners = new Map();
  return {
    addEventListener(type, callback) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(callback);
    },
    removeEventListener(type, callback) { listeners.get(type)?.delete(callback); },
    dispatch(type) {
      const event = { target: this, stopPropagation() {}, preventDefault() {} };
      for (const callback of listeners.get(type) || []) callback(event);
    },
  };
}

function fixture() {
  const requests = [], messages = [], timers = new Map(), lifecycleCalls = [], toasts = [];
  let timerId = 0, annotationVersion = 0, input = null;
  const state = {
    activeSession: { id: "synthetic-session-a" }, items: [], selectedRecord: null, historyPaging: {},
    _pendingAnnotations: new Map(), _annotationInFlight: new Set(),
    displaySettings: { sizePx: 12, theme: "paper", uiFont: "plex", monoFont: "jetbrains" },
    historyColumnWidths: {}, historyColumnOrder: [], wsColumnWidths: {},
  };
  const cell = {
    isConnected: true, dataset: {}, innerHTML: "Saved synthetic note",
    querySelector: () => input, appendChild(value) { input = value; },
    contains(value) { return value === this || value === input; },
  };
  const document = {
    ...eventTarget(), activeElement: {}, visibilityState: "visible",
    createElement(tag) {
      assert.equal(tag, "input");
      return { ...eventTarget(), dataset: {}, value: "", select() {},
        focus() { document.activeElement = this; } };
    },
  };
  const lifecycle = name => () => { lifecycleCalls.push(name); };
  const context = loadFunctions([
    "flushBeforeNativeClose", "flushAllPendingAnnotations", "updateAnnotations", "flushPendingAnnotations",
    "beginNoteEdit", "releaseHistoryNoteEdit", "inheritPendingHistoryNoteEdit", "requireOkResponse",
    "truncateUtf8", "truncateUtf8Preview", "utf8ByteLength",
    "currentSessionId", "sessionWritePath", "expectedActiveSessionIdForWrite",
    "persistUiSettings", "snapshotUiSettings", "nextUiSettingsSnapshot", "scheduleUiSettingsRetry",
    "updateUiSettingsServerRevision",
  ], {
    state, document, URLSearchParams, TextEncoder, workspaceStateGeneration: 0,
    annotationClientId: "synthetic-note-client", nextAnnotationClientVersion: () => ++annotationVersion,
    pendingHistoryNoteRead: null, MAX_ANNOTATION_NOTE_BYTES: 32 * 1024,
    uiSettingsClientId: "synthetic-settings-client", uiSettingsSaveVersion: 0, uiSettingsServerRevision: 3,
    uiSettingsSaveTimer: null, uiSettingsDirty: true, uiSettingsInFlight: false, uiSettingsSavePromise: null,
    lastUiSettingsPayload: null, displaySettingsReady: true, pendingDisplaySettingsApply: null,
    console: { error() {} }, showToast: (...args) => toasts.push(args),
    window: {
      ...eventTarget(), ipc: { postMessage: message => messages.push(message) },
      setTimeout(callback, delay) { timers.set(++timerId, { callback, delay }); return timerId; },
      clearTimeout(id) { timers.delete(id); },
    },
    fetch(url, options) {
      assert.ok((url === "fixture-only:note-read" && !options)
        || (url === "/api/ui-settings" && options?.method === "POST")
        || (url.startsWith("/api/transactions/synthetic-note/annotations?") && options?.method === "PATCH"),
      `Unexpected fixture fetch: ${url}`);
      const request = { url, options, ...deferred() };
      requests.push(request);
      return request.promise;
    },
    // No request, proxy, replay, WebSocket, sequence, scanner or native action
    // implementation is loaded. The native close function sees inert phases.
    flushProxySettingsSaveBeforeClose: lifecycle("proxy-settings"),
    flushSequenceDraft: lifecycle("sequence"),
    syncReplayDraftsBeforeWorkspaceClose: lifecycle("replay-drafts"),
    flushWsReplayTabsBeforeClose: lifecycle("websocket"),
    flushWorkspaceState: lifecycle("workspace"),
    transactionPath: () => "fixture-only:note-read", isHttpHistoryVisible: () => true,
    isModalVisible: () => false, closeContextMenu() {}, els: {},
    currentHistoryNoteEditWindow: () => null, getHistoryItemIndex: () => -1,
    observeAnnotationRevision() {}, loadTransactions: async () => {},
    sanitizeActiveTool: () => "proxy", sanitizeActiveProxyTab: () => "http-history",
    sanitizeHttpQuery: () => "", sanitizeHttpMethod: () => "", sanitizeHttpSortKey: () => "index",
    sanitizeHttpSortDirection: () => "desc", serializeHttpFilterSettings: () => ({}),
    serializeWorkbenchPaneWidths: () => ({}), sanitizeWebsocketQuery: () => "",
    sanitizeWebsocketSortKey: () => "started_at", sanitizeWebsocketSortDirection: () => "desc",
    updateDisplaySettingsReadiness() {},
    applyUiSettingsSnapshot() { assert.fail("No conflict snapshot is expected"); },
  });
  const hook = appSource.match(/^window\.__sniperFlushBeforeNativeClose = flushBeforeNativeClose;$/m);
  assert.ok(hook, "Missing native-close frontend hook");
  vm.runInContext(hook[0], context);

  const close = (generation = 42) => vm.runInContext(nativeCloseScript(generation), context);
  const settingsRequests = () => requests.filter(request => request.url === "/api/ui-settings");
  const annotationRequests = () => requests.filter(request => request.options?.method === "PATCH");
  const runTimer = delay => {
    const entry = [...timers].find(([, timer]) => timer.delay === delay);
    assert.ok(entry, `Missing ${delay} ms timer`);
    timers.delete(entry[0]); entry[1].callback();
  };
  const typeNote = async (value = "Synthetic late note") => {
    const loading = context.beginNoteEdit(cell, "synthetic-note");
    requests.at(-1).resolve({ ok: true, json: async () => ({ user_note: "Saved synthetic note" }) });
    await loading;
    assert.ok(input);
    input.value = value; input.dispatch("input");
  };
  const completeSettings = () => {
    const request = settingsRequests().at(-1), payload = JSON.parse(request.options.body);
    request.resolve({ ok: true, json: async () => ({ ...payload, server_revision: 4 }) });
  };
  const completeNote = async (ok = true) => {
    annotationRequests().at(-1).resolve(ok
      ? { ok: true, json: async () => ({ id: "synthetic-note", annotation_revision: 1 }) }
      : { ok: false, text: async () => "Synthetic note save failure" });
    await tick();
  };
  const beginLateNote = async () => {
    await typeNote();
    assert.equal(state._pendingAnnotations.size, 0, "Editor debounce has not queued its note yet");
    const closing = close(); await tick();
    assert.equal(settingsRequests().length, 1);
    runTimer(500);
    assert.equal(annotationRequests().length, 1, "Actual note timer queues a PATCH during settings save");
    return { closing, entry: state._pendingAnnotations.get("synthetic-note") };
  };
  return { context, state, requests, messages, timers, lifecycleCalls, toasts, close, typeNote, runTimer,
    settingsRequests, annotationRequests, completeSettings, completeNote, beginLateNote };
}

test("completed note PATCH and settings save permit native success", async () => {
  const f = fixture(); await f.typeNote(); f.runTimer(500); await f.completeNote();
  const closing = f.close(); await tick(); f.completeSettings(); await closing;
  assert.deepEqual(f.messages, ["sniper:native-close-flush-ok:42"]);
  assert.equal(f.state._pendingAnnotations.size, 0);
  assert.equal(f.state._annotationInFlight.size, 0);
  assert.equal(f.lifecycleCalls.length, 5);
});

for (const outcome of ["in-flight", "failed"]) {
  test(`late ${outcome} note refuses native success without retrying or consuming the edit`, async () => {
    const f = fixture(), { closing, entry } = await f.beginLateNote();
    if (outcome === "failed") await f.completeNote(false);
    f.completeSettings(); await closing;
    assert.match(f.messages[0], /^sniper:native-close-flush-error:42:/);
    assert.equal(f.messages.length, 1);
    assert.equal(f.state._pendingAnnotations.get("synthetic-note"), entry);
    assert.equal(entry.payload.user_note, "Synthetic late note");
    assert.equal(f.state._annotationInFlight.size, outcome === "in-flight" ? 1 : 0);
    assert.equal(f.annotationRequests().length, 1, "Final guard does not start another save");
    assert.equal(f.timers.size, 0, "Final guard does not add retry work");
  });
}

test("late acknowledged PATCH permits success without a redundant annotation write", async () => {
  const f = fixture(), { closing } = await f.beginLateNote();
  await f.completeNote(); f.completeSettings(); await closing;
  assert.deepEqual(f.messages, ["sniper:native-close-flush-ok:42"]);
  assert.equal(f.annotationRequests().length, 1);
  assert.equal(f.state._pendingAnnotations.size, 0);
});

test("subsequent close retries the preserved failed note and waits for its acknowledgement", async () => {
  const f = fixture(), { closing, entry } = await f.beginLateNote();
  await f.completeNote(false); f.completeSettings(); await closing;
  assert.match(f.messages[0], /^sniper:native-close-flush-error:42:/);
  const retry = f.close(43); await tick();
  assert.equal(f.messages.length, 1);
  assert.equal(f.annotationRequests().length, 2);
  assert.equal(f.annotationRequests()[1].options.body, JSON.stringify(entry.payload));
  await f.completeNote(); await retry;
  assert.equal(f.messages[1], "sniper:native-close-flush-ok:43");
  assert.equal(f.state._pendingAnnotations.size, 0);
  assert.equal(f.settingsRequests().length, 1, "Acknowledged settings are not saved again");
});

test("settings failure still refuses close and preserves its existing retry", async () => {
  const f = fixture(), closing = f.close(); await tick();
  f.settingsRequests()[0].resolve({ ok: false, text: async () => "Synthetic settings failure" });
  await closing;
  assert.deepEqual(f.messages, ["sniper:native-close-flush-error:42:Synthetic settings failure"]);
  assert.equal(f.context.uiSettingsDirty, true);
  assert.equal(f.context.uiSettingsInFlight, false);
  assert.equal([...f.timers.values()].filter(timer => timer.delay === 1000).length, 1);
});

for (const sameId of [false, true]) {
  test(`final guard preserves a new note after a ${sameId ? "generation" : "session"} change`, async () => {
    const f = fixture(), closing = f.close(); await tick();
    f.state.activeSession = { id: sameId ? "synthetic-session-a" : "synthetic-session-b" };
    f.context.workspaceStateGeneration++;
    await f.typeNote("Synthetic replacement note"); f.runTimer(500);
    const entry = f.state._pendingAnnotations.get("synthetic-note");
    const url = new URL(f.annotationRequests()[0].url, "http://example.com");
    assert.equal(url.searchParams.get("session_id"), f.state.activeSession.id);
    assert.equal(url.searchParams.get("expected_active_session_id"), f.state.activeSession.id);
    f.completeSettings(); await closing;
    assert.match(f.messages[0], /^sniper:native-close-flush-error:42:/);
    assert.equal(f.state._pendingAnnotations.get("synthetic-note"), entry);
    assert.equal(entry.payload.user_note, "Synthetic replacement note");
    assert.equal(f.annotationRequests().length, 1);
  });
}

test("native rejection source returns to Wait before session persistence or teardown", () => {
  // Source contract only: never start the Rust executable, window or event loop.
  const start = desktopSource.indexOf("Event::UserEvent(DesktopUserEvent::WebviewCloseFlushFinished {");
  const end = desktopSource.indexOf("Event::UserEvent(DesktopUserEvent::WebviewCloseFlushTimedOut", start);
  assert.ok(start >= 0 && end > start);
  const handler = desktopSource.slice(start, end);
  assert.match(handler, /if !close_flush_in_progress \|\| generation != close_flush_generation \{\s+return;/);
  assert.match(handler, /close_flush_in_progress = false;\s+if !ok \{\s+\*control_flow = block_desktop_shutdown\([^]*?\);\s+return;\s+\}\s+match persist_desktop_session_state/);
  const blocker = desktopSource.match(/fn block_desktop_shutdown\([^]*?^\}/m)?.[0];
  assert.ok(blocker);
  assert.match(blocker, /ControlFlow::Wait\s+\}/);
  assert.doesNotMatch(blocker, /ControlFlow::Exit|teardown_once|persist_desktop_session_state/);
  const parser = desktopSource.match(/fn desktop_user_event_from_ipc\([^]*?^\}/m)?.[0];
  assert.match(parser, /strip_prefix\(DESKTOP_CLOSE_FLUSH_ERROR_PREFIX\)[^]*?ok: false/);
});
