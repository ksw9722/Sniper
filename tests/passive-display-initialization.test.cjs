// Display initialization only: actual handlers, synthetic DOM/settings, and
// deferred fake requests. No app bootstrap, browser, server, or resource fetches.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const { appSource, loadFunctions } = require("./frontend-test-helpers.cjs");
const nextTurn = () => new Promise(resolve => setImmediate(resolve));
const plain = value => JSON.parse(JSON.stringify(value));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const defaults = { size_px: 12, theme: "charcoal", ui_font: "plex", mono_font: "jetbrains", render_resources: false };
function snapshot(revision, display = {}) {
  return {
    ...(revision === undefined ? {} : { server_revision: revision }),
    display_settings: { ...defaults, ...display },
    active_tool: "proxy", active_proxy_tab: "http-history",
    history_column_widths: { host: 276 }, history_column_order: ["host"],
    workbench_height: 550, workbench_pane_widths: { request_percent: 42 },
    http_query: "", http_method: "", http_sort_key: "index", http_sort_direction: "desc",
    http_filter_settings: {
      in_scope_only: false, hide_without_responses: false, only_parameterized: false, only_notes: true,
      search_term: "", regex: false, case_sensitive: false, negative_search: false,
      mime: { html: true, script: true, json: true, css: true, image: true, websocket: true, other: true },
      status: { success: true, redirect: true, client_error: true, server_error: true, other: true },
      hidden_extensions: "png,ico,css,woff,woff2,ttf,svg,jpg,jpeg,gif", port: "", color_tags: [],
    },
    websocket_pane_width: null, websocket_query: "", websocket_sort_key: "started_at",
    websocket_sort_direction: "desc", websocket_in_scope_only: false, websocket_live_only: false,
    websocket_stack_height: null, ws_replay_left_width: null, ws_replay_frame_detail_height: null,
  };
}
const nondefault = { size_px: 16, theme: "paper", ui_font: "notokr", mono_font: "plexmono", render_resources: true };

function fixture() {
  const requests = [], timers = new Map(), errors = [], styles = new Map();
  let timerId = 0, redraws = 0;
  const document = {
    activeElement: null, getElementById: () => null,
    body: { dataset: {} }, documentElement: { style: { setProperty: (key, value) => styles.set(key, value) } },
  };
  function node(tagName = "BUTTON") {
    const handlers = new Map(), classes = new Set();
    const control = {
      tagName, type: "button", checked: false, value: "", disabled: false, textContent: "",
      children: [], tabIndex: 0, isConnected: true,
      classList: {
        add: value => classes.add(value), remove: value => classes.delete(value), contains: value => classes.has(value),
        toggle(value, force = !classes.has(value)) { if (force) classes.add(value); else classes.delete(value); },
      },
      addEventListener(type, callback) {
        if (!handlers.has(type)) handlers.set(type, []);
        handlers.get(type).push(callback);
      },
      dispatch(type) { handlers.get(type)?.forEach(callback => callback({ target: control })); },
      click() { if (!control.disabled) control.dispatch("click"); },
      focus() { document.activeElement = control; },
      contains: target => control === target || control.children.includes(target),
    };
    return control;
  }
  const els = {};
  for (const name of ["openDisplaySettingsButton", "closeDisplaySettingsButton", "applyDisplaySettingsButton", "resetDisplaySettingsButton", "retryDisplaySettingsButton", "displaySettingsLoadStatus"]) els[name] = node();
  for (const name of ["displayThemeSelect", "displayUiFontSelect", "displayMonoFontSelect"]) els[name] = node("SELECT");
  els.displaySizeInput = node("INPUT"); els.displaySizeInput.type = "number";
  els.displayRenderResourcesInput = node("INPUT"); els.displayRenderResourcesInput.type = "checkbox";
  for (const name of ["displaySettingsModal", "filterModal", "curlImportModal"]) {
    els[name] = node("DIV"); els[name].classList.add("hidden");
  }
  els.displaySettingsModal.children = Object.values(els);
  document.activeElement = els.openDisplaySettingsButton;
  const state = {
    displaySettings: { sizePx: 12, theme: "charcoal", uiFont: "plex", monoFont: "jetbrains", renderResources: false },
    historyColumnWidths: {}, historyColumnOrder: [], wsColumnWidths: {}, workbenchPaneWidths: {},
  };
  const globals = {
    state, els, document, displaySettingsPreviewActive: false, displaySettingsReturnFocus: null,
    displaySettingsEditedControls: new Set(), displaySettingsReady: false,
    uiSettingsLoadState: "loading", uiSettingsLoadPromise: null,
    uiSettingsInitialPostGeneration: 0, uiSettingsInitialPostRevision: 0,
    uiSettingsClientId: "fixture-local", uiSettingsSaveVersion: 0, uiSettingsServerRevision: 0,
    uiSettingsSaveTimer: null, uiSettingsDirty: false, uiSettingsInFlight: false,
    uiSettingsSavePromise: null, lastUiSettingsPayload: null, activeConfirmDialog: null,
    WS_COLUMN_RULES: {}, console: { error: error => errors.push(error) },
    DISPLAY_THEME_OPTIONS: new Set(["charcoal", "paper", "ivory"]),
    DISPLAY_UI_FONT_OPTIONS: new Set(["plex", "system", "notokr"]),
    DISPLAY_MONO_FONT_OPTIONS: new Set(["jetbrains", "sfmono", "plexmono"]),
    clamp: (value, min, max) => Math.min(max, Math.max(min, value)),
    window: {
      clearTimeout: id => timers.delete(id),
      setTimeout(callback, delay) { const id = ++timerId; timers.set(id, { callback, delay }); return id; },
    },
    fetch(url, options) {
      assert.equal(url, "/api/ui-settings");
      assert.ok(!options || options.method === "POST");
      const request = { ...deferred(), options }; requests.push(request); return request.promise;
    },
    restoreModalFocus: target => target?.focus(), renderShortcutReference() {}, selectSettingsTab() {},
    redrawResponsePreviews: () => redraws++,
    serializeHttpFilterSettings: () => state.filterSettings || {},
    serializeWorkbenchPaneWidths: () => state.workbenchPaneWidths,
  };
  for (const name of [
    "sanitizeActiveTool", "sanitizeActiveProxyTab", "sanitizeHttpQuery", "sanitizeHttpMethod",
    "sanitizeHttpSortKey", "sanitizeHttpSortDirection", "sanitizeHttpFilterSettings",
    "sanitizeHistoryColumnWidths", "sanitizeHistoryColumnOrder", "sanitizeWorkbenchHeight",
    "sanitizeWorkbenchPaneWidths", "sanitizeWebsocketPaneWidth", "sanitizeWebsocketQuery",
    "sanitizeWebsocketSortKey", "sanitizeWebsocketSortDirection", "sanitizeWebsocketStackHeight",
    "sanitizeWsReplayLeftWidth", "sanitizeWsReplayFrameDetailHeight",
  ]) globals[name] = value => value;
  for (const name of [
    "hydrateFilterForm", "syncColorTagFilterUI", "syncHttpInScopePill", "syncHttpCapturePill",
    "renderHistoryHeader", "applyHistoryColumnWidths", "applyWsColumnWidths", "updateWebsocketSortIndicators",
    "applySavedWorkbenchPaneWidths", "applySavedWebsocketPaneWidth", "applySavedWebsocketStackHeight",
    "applySavedWsReplayLayout", "applyWorkbenchStackHeight", "closeFilterModal", "applyFilterSettings", "closeCurlImportModal",
  ]) globals[name] = () => {};
  const names = [
    "createDefaultDisplaySettings", "sanitizeDisplaySettings", "openDisplaySettingsModal", "closeDisplaySettingsModal",
    "hydrateDisplaySettingsForm", "collectDisplaySettingsFormValues", "previewDisplaySettingsFromForm",
    "saveDisplaySettingsFromForm", "applyDisplaySettingsState", "applyUiSettingsSnapshot", "loadUiSettings",
    "updateUiSettingsServerRevision", "isModalVisible", "getActiveModalAction", "persistUiSettings",
    "snapshotUiSettings", "nextUiSettingsSnapshot", "scheduleUiSettingsSave", "scheduleUiSettingsRetry",
  ];
  names.push("displaySettingsFormControls", "updateDisplaySettingsReadiness", "isUiSettingsLoadSnapshot");
  const context = loadFunctions(names, globals);
  const start = appSource.indexOf('  els.applyDisplaySettingsButton.addEventListener("click",');
  const end = appSource.indexOf('\n  onClickWithProgress(els.openCertFolderButton', start);
  assert.ok(start >= 0 && end > start);
  vm.runInContext(appSource.slice(start, end), context);
  document.addEventListener = (_, handler) => { context.keydown = handler; };
  const keyStart = appSource.indexOf('  document.addEventListener("keydown", (event) => {\n    const activeModalAction');
  const keyEnd = appSource.indexOf('    if (\n      !event.defaultPrevented', keyStart);
  vm.runInContext(appSource.slice(keyStart, keyEnd) + "  });", context);
  context.applyDisplaySettingsState();
  const key = (key, target = els.displaySizeInput) => context.keydown({
    key, target, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; },
  });
  const payload = index => JSON.parse(requests[index].options.body);
  const respond = (index, body) => requests[index].resolve({ ok: true, json: async () => body });
  const succeed = (index, revision = 8) => respond(index, { ...payload(index), server_revision: revision });
  function edit(name, value, event = "change") {
    const control = els[name]; control[name === "displayRenderResourcesInput" ? "checked" : "value"] = value; control.dispatch(event);
  }
  function runTimers() { for (const [id, timer] of [...timers]) { timers.delete(id); timer.callback(); } }
  async function ready(body = snapshot(7, nondefault)) { const loading = context.loadUiSettings(); respond(requests.length - 1, body); await loading; }
  return { context, state, els, document, requests, timers, errors, styles, key, payload, respond, succeed, edit, runTimers, ready, get redraws() { return redraws; } };
}

for (const [label, initial] of [
  ["omitted-zero defaults", snapshot(undefined)], ["existing revision", snapshot(7, nondefault)],
  ["nondefault legacy revision zero", snapshot(undefined, nondefault)],
]) {
  for (const action of ["click", "keyboard", "handler"]) {
    test(`${label}: pending GET blocks ${action} Apply without changing committed settings`, async () => {
      const f = fixture(), loading = f.context.loadUiSettings();
      f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
      const before = plain(f.state.displaySettings);
      if (action === "click") f.els.applyDisplaySettingsButton.click();
      if (action === "keyboard") f.key("Enter");
      if (action === "handler") f.context.saveDisplaySettingsFromForm();
      assert.equal(f.requests.length, 1, "Only the initial GET may run");
      assert.equal(f.context.uiSettingsSaveVersion, 0);
      assert.equal(f.context.uiSettingsDirty, false);
      assert.deepEqual(plain(f.state.displaySettings), before);
      assert.equal(f.context.isModalVisible(f.els.displaySettingsModal), true);
      assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
      f.respond(0, initial); await loading;
      assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
      assert.equal(f.els.displayThemeSelect.value, "ivory");
      assert.equal(f.els.displaySizeInput.value, String(initial.display_settings.size_px));
      assert.equal(f.els.displayUiFontSelect.value, initial.display_settings.ui_font);
      assert.equal(f.els.displayMonoFontSelect.value, initial.display_settings.mono_font);
      assert.equal(f.els.displayRenderResourcesInput.checked, initial.display_settings.render_resources);
      f.els.applyDisplaySettingsButton.click();
      assert.equal(f.requests.length, 2); assert.equal(f.context.uiSettingsSaveVersion, 1);
      assert.deepEqual(f.payload(1).display_settings, { ...initial.display_settings, theme: "ivory" });
      assert.equal(f.payload(1).server_revision, initial.server_revision || 0);
      assert.deepEqual(f.payload(1).history_column_widths, initial.history_column_widths);
      assert.deepEqual(f.payload(1).http_filter_settings, initial.http_filter_settings);
      assert.equal(f.payload(1).workbench_height, 550);
      f.succeed(1); await nextTurn();
    });
  }
}

test("omitted optional layout fields and sanitized legacy Display fields remain compatible", async () => {
  const f = fixture(), initial = snapshot(undefined);
  delete initial.history_column_order; delete initial.workbench_pane_widths;
  // JSON-safe unknown Display values still use the existing sanitization.
  initial.display_settings = { theme: "old-theme", ui_font: "old-font", size_px: 25 };
  await f.ready(initial); f.context.openDisplaySettingsModal();
  assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
  assert.equal(f.els.displayThemeSelect.value, "charcoal"); assert.equal(f.els.displaySizeInput.value, "20");
  assert.equal(f.els.displayUiFontSelect.value, "plex"); assert.equal(f.els.displayMonoFontSelect.value, "jetbrains");
  assert.equal(f.els.displayRenderResourcesInput.checked, false); assert.equal(f.requests.length, 1);
});

for (const [control, away, back, field] of [
  ["displayThemeSelect", "ivory", "charcoal", "theme"], ["displaySizeInput", "18", "12", "size_px"],
  ["displayUiFontSelect", "system", "plex", "ui_font"], ["displayMonoFontSelect", "sfmono", "jetbrains", "mono_font"],
  ["displayRenderResourcesInput", true, false, "render_resources"],
]) {
  test(`initial hydration preserves an explicit ${control} edit back to default`, async () => {
    const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal();
    f.edit(control, away, "input"); f.edit(control, back, "change");
    f.respond(0, snapshot(7, nondefault)); await loading;
    const values = plain(f.context.collectDisplaySettingsFormValues());
    const wire = { size_px: values.sizePx, theme: values.theme, ui_font: values.uiFont, mono_font: values.monoFont, render_resources: values.renderResources };
    assert.deepEqual(wire, { ...nondefault, [field]: defaults[field] });
    assert.equal(f.requests.length, 1); assert.equal(f.redraws, 0);
  });
}

test("Reset marks all Display fields without applying or loading resources", async () => {
  const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal();
  f.els.resetDisplaySettingsButton.click();
  f.respond(0, snapshot(7, nondefault)); await loading;
  assert.deepEqual(plain(f.context.collectDisplaySettingsFormValues()), { sizePx: 12, theme: "charcoal", uiFont: "plex", monoFont: "jetbrains", renderResources: false });
  assert.equal(f.document.body.dataset.theme, "charcoal"); assert.equal(f.styles.get("--ui-root-size"), "16px");
  assert.equal(f.requests.length, 1); assert.equal(f.redraws, 0);
  f.key("Escape");
  assert.equal(f.context.displaySettingsEditedControls.size, 0);
  assert.equal(f.document.body.dataset.theme, "paper");
  f.context.openDisplaySettingsModal(); assert.equal(f.els.displayRenderResourcesInput.checked, true);
});

test("size-only draft remains unpreviewed through hydration and redundant open", async () => {
  const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal();
  f.edit("displaySizeInput", "1", "input"); f.els.displaySizeInput.focus();
  f.context.openDisplaySettingsModal();
  assert.equal(f.document.activeElement, f.els.displaySizeInput);
  f.respond(0, snapshot(7, nondefault)); await loading;
  assert.equal(f.els.displaySizeInput.value, "1"); assert.equal(f.context.displaySettingsPreviewActive, false);
  assert.equal(f.styles.get("--ui-root-size"), "16px"); assert.equal(f.document.body.dataset.theme, "paper");
  f.context.closeDisplaySettingsModal(); f.context.openDisplaySettingsModal();
  assert.equal(f.els.displaySizeInput.value, "16"); assert.equal(f.context.displaySettingsEditedControls.size, 0);
});

test("Cancel before hydration clears edits; a reopened untouched form receives saved values", async () => {
  const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal();
  f.edit("displayThemeSelect", "ivory"); f.context.closeDisplaySettingsModal();
  f.context.openDisplaySettingsModal(); f.respond(0, snapshot(7, nondefault)); await loading;
  assert.equal(f.els.displayThemeSelect.value, "paper"); assert.equal(f.els.displaySizeInput.value, "16");
  assert.equal(f.context.displaySettingsEditedControls.size, 0); assert.equal(f.requests.length, 1);
});

const failures = [
  ["network", null], ["HTTP", null], ["JSON", null],
  ...[null, [], "settings", 42, false, {}, { display_settings: null }, { display_settings: [] }, { display_settings: "settings" }].map((body, i) => [`shape ${i}`, body]),
  ...[{ server_revision: null }, { server_revision: "7" }, { server_revision: -1 }, { server_revision: 1.5 }, { server_revision: Number.MAX_SAFE_INTEGER + 1 }, { server_revision: true },
    { client_version: null }, { client_version: -1 }, { client_version: "2" }, { client_version: 1.5 },
    { client_version: Number.MAX_SAFE_INTEGER + 1 }, { client_id: null }, { client_id: 42 }].map((meta, i) => [`metadata ${i}`, { ...snapshot(7), ...meta }]),
];
for (const [kind, body] of failures) {
  test(`initial ${kind} failure retains the draft and offers a single-flight, read-only Retry`, async () => {
    const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal();
    f.edit("displayThemeSelect", "ivory");
    const before = plain(f.state);
    if (kind === "network") f.requests[0].reject(new Error("synthetic unavailable"));
    else if (kind === "HTTP") f.requests[0].resolve({ ok: false, text: async () => "synthetic unavailable" });
    else if (kind === "JSON") f.requests[0].resolve({ ok: true, json: async () => { throw new Error("synthetic JSON"); } });
    else f.respond(0, body);
    await loading;
    assert.deepEqual(plain(f.state), before, "An invalid GET must not change committed preferences");
    assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
    assert.match(f.els.displaySettingsLoadStatus.textContent, /load|retry/i);
    assert.equal(f.els.retryDisplaySettingsButton.disabled, false);
    assert.equal(f.els.retryDisplaySettingsButton.classList.contains("hidden"), false);
    f.key("Enter"); assert.equal(f.requests.length, 1);
    f.els.retryDisplaySettingsButton.click(); f.els.retryDisplaySettingsButton.click();
    const retry = f.context.loadUiSettings();
    assert.equal(f.requests.length, 2); assert.equal(f.els.retryDisplaySettingsButton.disabled, true);
    assert.equal(f.els.displayThemeSelect.value, "ivory");
    f.respond(1, snapshot(undefined, nondefault)); await retry;
    assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
    assert.equal(f.els.displayThemeSelect.value, "ivory"); assert.equal(f.els.displayUiFontSelect.value, "notokr");
    assert.equal(f.requests.length, 2, "Retry must never submit the draft");
    assert.equal(f.context.uiSettingsSaveVersion, 0); assert.equal(f.context.uiSettingsDirty, false);
  });
}

test("repeated initial reads share one pending request; repeated failures preserve the draft", async () => {
  const f = fixture(), first = f.context.loadUiSettings(), second = f.context.loadUiSettings();
  assert.equal(f.requests.length, 1); f.context.openDisplaySettingsModal(); f.edit("displaySizeInput", "18", "input");
  f.requests[0].reject(new Error("synthetic first failure")); await Promise.all([first, second]);
  f.els.retryDisplaySettingsButton.click(); f.requests[1].reject(new Error("synthetic second failure")); await nextTurn();
  assert.equal(f.els.displaySizeInput.value, "18"); assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
  f.els.retryDisplaySettingsButton.click(); f.respond(2, snapshot(7, nondefault)); await nextTurn();
  assert.equal(f.els.displaySizeInput.value, "18"); assert.equal(f.requests.length, 3);
});

for (const response of ["accepted", "conflict"]) {
  for (const first of ["GET", "POST"]) {
    test(`preexisting ${response} POST finishing ${first === "POST" ? "before" : "after"} GET cannot consume a Display Apply`, async () => {
      const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
      f.state.workbenchHeight = 600; f.context.uiSettingsDirty = true;
      const saving = f.context.persistUiSettings();
      const winner = response === "accepted" ? { ...f.payload(1), server_revision: 8 } : { ...snapshot(8, nondefault), client_id: "fixture-other", client_version: 3 };
      if (first === "GET") {
        f.respond(0, snapshot(7, nondefault)); await nextTurn();
        assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
        assert.equal(f.context.uiSettingsLoadState, "waiting");
        f.key("Enter"); assert.equal(f.requests.length, 2); assert.equal(f.context.uiSettingsSaveVersion, 1);
        assert.equal(f.context.isModalVisible(f.els.displaySettingsModal), true);
        f.respond(1, winner); await saving; await loading;
      } else {
        f.respond(1, winner); await saving;
        assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
        // Real default responses omit revision 0, rather than encoding it.
        f.respond(0, snapshot(undefined)); await loading;
        assert.equal(f.context.uiSettingsServerRevision, 8);
        assert.equal(f.state.workbenchHeight, winner.workbench_height);
      }
      assert.equal(f.context.uiSettingsLoadState, "failed");
      assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
      assert.equal(f.els.displayThemeSelect.value, "ivory");
      const retry = f.context.loadUiSettings(); f.respond(2, winner); await retry;
      assert.equal(f.requests.length, 3, "A fresh read must not submit the draft");
      assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
      assert.equal(f.els.displayThemeSelect.value, "ivory");
      f.context.saveDisplaySettingsFromForm();
      assert.equal(f.payload(3).display_settings.theme, "ivory");
      assert.equal(f.payload(3).server_revision, 8);
      assert.equal(f.payload(3).display_settings.ui_font, winner.display_settings.ui_font);
      f.succeed(3, 9); await nextTurn();
    });
  }
}

for (const getFirst of [false, true]) {
  test(`preexisting POST failure ${getFirst ? "after" : "before"} GET requires a fresh read before Display can replace its retry`, async () => {
    const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
    f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings();
    const rejected = assert.rejects(saving, /synthetic/);
    if (getFirst) { f.respond(0, snapshot(7, nondefault)); await nextTurn(); assert.equal(f.els.applyDisplaySettingsButton.disabled, true); }
    f.requests[1].reject(new Error("synthetic unavailable")); await rejected;
    if (!getFirst) f.respond(0, snapshot(7, nondefault));
    await loading;
    assert.equal(f.timers.size, 1); assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
    assert.equal(f.context.uiSettingsLoadState, "failed");
    const retry = f.context.loadUiSettings(); f.respond(2, snapshot(7, nondefault)); await retry;
    assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
    f.context.saveDisplaySettingsFromForm(); assert.equal(f.timers.size, 0);
    assert.equal(f.payload(3).display_settings.theme, "ivory"); assert.equal(f.payload(3).display_settings.ui_font, "notokr");
    f.succeed(3); await nextTurn(); assert.equal(f.context.uiSettingsDirty, false);
  });
}

test("an already-running retry also holds the initialization gate until its conflict settles", async () => {
  const f = fixture(), loading = f.context.loadUiSettings(); f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
  f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings(); const rejected = assert.rejects(saving);
  f.requests[1].reject(new Error("synthetic unavailable")); await rejected; f.runTimers();
  f.respond(0, snapshot(7, nondefault)); await nextTurn();
  assert.equal(f.els.applyDisplaySettingsButton.disabled, true); f.key("Enter"); assert.equal(f.requests.length, 3);
  f.respond(2, { ...snapshot(8, nondefault), client_id: "fixture-other", client_version: 4 }); await loading;
  assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
  const retry = f.context.loadUiSettings(); f.respond(3, snapshot(8, nondefault)); await retry;
  assert.equal(f.els.applyDisplaySettingsButton.disabled, false); assert.equal(f.els.displayThemeSelect.value, "ivory");
  assert.equal(f.els.displayMonoFontSelect.value, "plexmono");
});

for (const failure of ["HTTP", "JSON", "shape", "metadata"]) {
  test(`a pre-initialization POST ${failure} failure makes the overlapping GET ambiguous`, async () => {
    const f = fixture(), loading = f.context.loadUiSettings();
    f.context.openDisplaySettingsModal(); f.edit("displaySizeInput", "18", "input");
    f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings();
    const rejected = assert.rejects(saving);
    if (failure === "HTTP") f.requests[1].resolve({ ok: false, text: async () => "synthetic unavailable" });
    if (failure === "JSON") f.requests[1].resolve({ ok: true, json: async () => { throw new Error("synthetic unavailable"); } });
    if (failure === "shape") f.respond(1, []);
    if (failure === "metadata") f.respond(1, { server_revision: "8" });
    await rejected; f.respond(0, snapshot(undefined)); await loading;
    assert.equal(f.context.uiSettingsLoadState, "failed");
    assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
    assert.equal(f.context.uiSettingsInitialPostRevision, 0);
    assert.equal(f.els.displaySizeInput.value, "18");
    const retry = f.context.loadUiSettings(); f.respond(2, snapshot(8, nondefault)); await retry;
    assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
    assert.equal(f.els.displaySizeInput.value, "18");
    assert.equal(f.els.displayUiFontSelect.value, "notokr"); assert.equal(f.requests.length, 3);
  });
}

for (const shape of ["compatible", "incompatible", "partial"]) {
  test(`an equal-revision ${shape} POST overlap also requires a clean GET`, async () => {
    const f = fixture(), loading = f.context.loadUiSettings();
    f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
    f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings();
    const response = shape === "partial" ? { server_revision: 0 }
      : { ...snapshot(0, shape === "incompatible" ? nondefault : {}), client_id: "fixture-other", client_version: 1 };
    f.respond(1, response); await saving;
    const before = plain(f.state);
    f.respond(0, snapshot(undefined)); await loading;
    assert.equal(f.context.uiSettingsLoadState, "failed");
    assert.deepEqual(plain(f.state), before, "An ambiguous GET must not mutate any committed baseline");
    const retry = f.context.loadUiSettings(); f.respond(2, snapshot(undefined, nondefault)); await retry;
    assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
    assert.equal(f.els.displayThemeSelect.value, "ivory"); assert.equal(f.els.displayUiFontSelect.value, "notokr");
    assert.equal(f.state.workbenchHeight, 550); assert.equal(f.requests.length, 3);
  });
}

test("the valid POST revision floor survives failed reads and retries", async () => {
  const f = fixture(); f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings();
  f.succeed(0, 8); await saving;
  for (const revision of [undefined, 7]) {
    const loading = f.context.loadUiSettings(); f.respond(f.requests.length - 1, snapshot(revision, nondefault)); await loading;
    assert.equal(f.context.uiSettingsLoadState, "failed"); assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
    assert.equal(f.context.uiSettingsInitialPostRevision, 8);
  }
  const retry = f.context.loadUiSettings(); f.respond(3, snapshot(8, nondefault)); await retry;
  assert.equal(f.els.applyDisplaySettingsButton.disabled, false); assert.equal(f.context.uiSettingsServerRevision, 8);
  assert.equal(f.context.uiSettingsSaveVersion, 1); assert.equal(f.requests.length, 4);
});

test("an unsafe POST revision cannot become an initialization revision floor", async () => {
  const f = fixture(), loading = f.context.loadUiSettings();
  f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings();
  f.succeed(1, Number.MAX_SAFE_INTEGER + 1); await saving;
  f.respond(0, snapshot(undefined)); await loading;
  assert.equal(f.context.uiSettingsLoadState, "failed");
  assert.equal(f.context.uiSettingsInitialPostRevision, 0);
});

test("POST completion during GET JSON parsing is also detected", async () => {
  const f = fixture(), body = deferred(), loading = f.context.loadUiSettings();
  f.requests[0].resolve({ ok: true, json: () => body.promise }); await nextTurn();
  f.context.uiSettingsDirty = true; const saving = f.context.persistUiSettings(); f.succeed(1, 8); await saving;
  body.resolve(snapshot(7, nondefault)); await loading;
  assert.equal(f.context.uiSettingsLoadState, "failed"); assert.equal(f.els.applyDisplaySettingsButton.disabled, true);
});

test("ready Display still queues repeated Apply in order and preserves normal server-wins conflicts", async () => {
  const f = fixture(); await f.ready(); f.context.openDisplaySettingsModal(); f.edit("displayThemeSelect", "ivory");
  f.context.saveDisplaySettingsFromForm(); assert.equal(f.context.displaySettingsEditedControls.size, 0);
  f.context.openDisplaySettingsModal(); f.edit("displaySizeInput", "18", "input"); f.context.saveDisplaySettingsFromForm();
  assert.equal(f.requests.length, 2); assert.equal(f.els.applyDisplaySettingsButton.disabled, false);
  f.succeed(1, 8); await nextTurn();
  assert.equal(f.payload(2).display_settings.theme, "ivory"); assert.equal(f.payload(2).display_settings.size_px, 18);
  assert.equal(f.payload(2).client_version, 2); assert.equal(f.payload(2).server_revision, 8);
  f.respond(2, { ...snapshot(9, nondefault), client_id: "fixture-other", client_version: 4 }); await nextTurn();
  assert.equal(f.state.displaySettings.theme, "paper"); assert.equal(f.state.displaySettings.sizePx, 16);
  assert.equal(f.context.uiSettingsDirty, false); assert.equal(f.context.uiSettingsSavePromise, null);
});

test("Display loading and Retry are accessible, and Apply starts disabled in markup", () => {
  const html = fs.readFileSync(path.join(__dirname, "../web/index.html"), "utf8");
  assert.match(html, /<[^>]+id="displaySettingsLoadStatus"[^>]+role="status"/);
  assert.match(html, /<button[^>]+id="retryDisplaySettingsButton"[^>]+type="button"/);
  assert.match(html, /<button[^>]+id="applyDisplaySettingsButton"[^>]+disabled/);
});
