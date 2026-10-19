// Synthetic Compare keyboard checks only. Production event callbacks run with
// inert navigation/action callbacks; no app bootstrap, server, or captured data.
const assert = require("node:assert/strict");
const test = require("node:test");
const vm = require("node:vm");
const { appSource, loadFunctions } = require("./frontend-test-helpers.cjs");

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

function fixture() {
  const document = { activeElement: null }, actions = [], pending = [], rendered = [];
  class HTMLElement {
    constructor(tagName = "DIV") {
      this.tagName = tagName;
      this.disabled = false;
      this.tabIndex = 0;
      this.children = [];
      this.isContentEditable = false;
      this.isConnected = true;
      this.handlers = new Map();
      const classes = new Set();
      this.classList = {
        add: name => classes.add(name), remove: name => classes.delete(name),
        contains: name => classes.has(name),
      };
    }
    focus() { document.activeElement = this; }
    addEventListener(type, callback) { this.handlers.set(type, callback); }
    click() { this.handlers.get("click")?.({ target: this }); }
    getClientRects() { return [1]; }
    closest() { return null; }
    querySelectorAll() { return this.children; }
  }
  const els = {};
  for (const name of ["displaySettingsModal", "filterModal", "curlImportModal", "compareModal"]) {
    els[name] = new HTMLElement();
    els[name].classList.add("hidden");
  }
  const requestTab = new HTMLElement("BUTTON"), responseTab = new HTMLElement("BUTTON");
  const diff = new HTMLElement("PRE"), close = new HTMLElement("BUTTON");
  requestTab.dataset = { compareTab: "request" };
  responseTab.dataset = { compareTab: "response" };
  els.compareModal.id = "compareModal";
  els.compareModal.children = [requestTab, responseTab, diff, close];
  els.trafficRegion = new HTMLElement();
  const opener = new HTMLElement("BUTTON");
  document.querySelector = () => els.trafficRegion;
  document.querySelectorAll = () => [requestTab, responseTab];
  document.getElementById = id => ({ compareModal: els.compareModal, closeCompareButton: close })[id] || null;
  document.activeElement = diff;
  const state = { activeTool: "proxy", activeProxyTab: "http-history", selectedId: "target" };
  const action = name => () => { actions.push(name); return Promise.resolve(); };
  const context = loadFunctions([
    "getActiveModalAction", "isModalVisible", "isEditableTarget", "trapModalFocus",
    "openCompareModal", "closeCompareModal", "restoreModalFocus",
  ], {
    state, els, document, HTMLElement, Element: HTMLElement, activeConfirmDialog: null,
    compareBaseId: "base", compareBaseSessionId: "session-a", compareLoadGeneration: 0,
    compareBaseRecord: null, compareTargetRecord: null, compareActiveTab: "request", compareReturnFocus: null,
    currentSessionId: () => "session-a", transactionPath: id => id,
    window: { getComputedStyle: () => ({ visibility: "visible" }) },
    closeDisplaySettingsModal: action("settings close"), closeFilterModal: action("filter close"),
    closeCertificateModal: action("certificate close"), closeCurlImportModal: action("curl close"),
    saveDisplaySettingsFromForm: action("settings apply"), applyFilterSettings: action("filter apply"),
    moveHistorySelection: action("history selection"),
    openReplayFromSelection: action("send to Replay"), openFuzzerFromSelection: action("send to Fuzzer"),
    handleSendActionError: error => { throw error; },
    fetch(path) {
      const response = deferred();
      pending.push({ path, ...response });
      return response.promise;
    },
  });
  context.renderCompareModal = () => rendered.push([context.compareBaseRecord.id, context.compareTargetRecord.id]);
  document.addEventListener = (type, callback) => { assert.equal(type, "keydown"); context.keydown = callback; };
  const start = appSource.indexOf('  document.addEventListener("keydown", (event) => {\n    const activeModalAction');
  const end = appSource.indexOf('\n\n  document.addEventListener("copy",', start);
  assert.ok(start >= 0 && end > start, "Missing production keyboard registration");
  vm.runInContext(appSource.slice(start, end), context);
  const compareStart = appSource.indexOf('  document.getElementById("closeCompareButton").addEventListener');
  const compareEnd = appSource.indexOf('\n  document.getElementById("closeCurlImportButton")', compareStart);
  assert.ok(compareStart >= 0 && compareEnd > compareStart);
  vm.runInContext(appSource.slice(compareStart, compareEnd), context);
  function key(key, target = document.activeElement, options = {}) {
    const event = {
      key, target, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
      defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...options,
    };
    context.keydown(event);
    if (!event.defaultPrevented && target?.tagName === "BUTTON" && ["Enter", " "].includes(key)) target.click();
    return event;
  }
  function resolvePair(offset, target) {
    pending[offset].resolve({ ok: true, json: async () => ({ id: "base" }) });
    pending[offset + 1].resolve({ ok: true, json: async () => ({ id: target }) });
  }
  return { context, els, document, actions, pending, rendered, requestTab, responseTab, diff, close, opener, key, resolvePair };
}

for (const key of ["ArrowUp", "ArrowDown"]) {
  test(`Compare ${key} leaves the underlying history selection alone`, () => {
    const f = fixture();
    f.els.compareModal.classList.remove("hidden");
    assert.equal(f.key(key).defaultPrevented, false, "Native diff scrolling should remain available");
    assert.deepEqual(f.actions, []);
  });
}

test("Escape closes Compare through its generation-aware dismissal", () => {
  const f = fixture();
  f.els.compareModal.classList.remove("hidden");
  assert.equal(f.key("Escape").defaultPrevented, true);
  assert.equal(f.els.compareModal.classList.contains("hidden"), true);
  assert.equal(f.context.compareLoadGeneration, 1);
  assert.deepEqual(f.actions, []);
});

for (const options of [{ isComposing: true }, { defaultPrevented: true }]) {
  test(`Compare preserves Escape when ${Object.keys(options)[0]}`, () => {
    const f = fixture();
    f.els.compareModal.classList.remove("hidden");
    f.key("Escape", f.diff, options);
    assert.equal(f.els.compareModal.classList.contains("hidden"), false);
    assert.equal(f.context.compareLoadGeneration, 0);
    assert.deepEqual(f.actions, []);
  });
}

for (const shiftKey of [false, true]) {
  test(`Compare confines ${shiftKey ? "Shift+Tab" : "Tab"} to its controls`, () => {
    const f = fixture();
    f.els.compareModal.classList.remove("hidden");
    (shiftKey ? f.requestTab : f.close).focus();
    assert.equal(f.key("Tab", f.document.activeElement, { shiftKey }).defaultPrevented, true);
    assert.equal(f.document.activeElement, shiftKey ? f.close : f.requestTab);
    assert.deepEqual(f.actions, []);
  });
}

for (const shortcut of ["r", "i"]) {
  for (const modifier of ["ctrlKey", "metaKey"]) {
    test(`Compare does not route ${modifier}+${shortcut} to the underlying transaction`, () => {
      const f = fixture();
      f.els.compareModal.classList.remove("hidden");
      f.key(shortcut, f.diff, { [modifier]: true });
      assert.deepEqual(f.actions, []);
    });
  }
}

test("Escape from a visible comparison cancels pending replacement; a later comparison can reopen", async () => {
  const f = fixture();
  const first = f.context.openCompareModal("first-target");
  f.resolvePair(0, "first-target");
  await first;
  const pending = f.context.openCompareModal("late-target");
  f.key("Escape");
  f.resolvePair(2, "late-target");
  await pending;
  assert.equal(f.els.compareModal.classList.contains("hidden"), true);
  assert.deepEqual(f.rendered, [["base", "first-target"]]);
  const reopened = f.context.openCompareModal("fresh-target");
  f.resolvePair(4, "fresh-target");
  await reopened;
  assert.equal(f.els.compareModal.classList.contains("hidden"), false);
  assert.deepEqual(f.rendered, [["base", "first-target"], ["base", "fresh-target"]]);
  assert.deepEqual(f.actions, []);
});

test("closed Compare leaves ordinary history navigation available", () => {
  const f = fixture();
  assert.equal(f.key("ArrowDown").defaultPrevented, true);
  assert.deepEqual(f.actions, ["history selection"]);
});


test("Compare opens with focus inside and Escape restores its invoker", async () => {
  const f = fixture();
  f.opener.focus();
  const load = f.context.openCompareModal("target");
  f.resolvePair(0, "target");
  await load;
  assert.equal(f.document.activeElement, f.close);
  f.key("Escape");
  assert.equal(f.document.activeElement, f.opener);
  f.diff.focus();
  f.context.closeCompareModal();
  assert.equal(f.document.activeElement, f.diff, "Closing an already hidden modal must not steal focus");
});

test("Compare falls back to the history region when its menu invoker is detached", async () => {
  const f = fixture();
  f.opener.focus();
  const load = f.context.openCompareModal("target");
  f.resolvePair(0, "target");
  await load;
  f.opener.isConnected = false;
  f.close.click();
  assert.equal(f.document.activeElement, f.els.trafficRegion);
});

for (const duringParsing of [false, true]) {
  test(`Escape ${duringParsing ? "during parsing" : "before the response"} cancels an initial hidden comparison`, async () => {
    const f = fixture(), body = deferred();
    f.opener.focus();
    const load = f.context.openCompareModal("target");
    if (duringParsing) {
      f.pending[0].resolve({ ok: true, json: async () => ({ id: "base" }) });
      f.pending[1].resolve({ ok: true, json: () => body.promise });
      await Promise.resolve();
    }
    f.key("Escape");
    if (duringParsing) body.resolve({ id: "target" });
    else f.resolvePair(0, "target");
    await load;
    assert.equal(f.els.compareModal.classList.contains("hidden"), true);
    assert.deepEqual(f.rendered, []);
    assert.equal(f.document.activeElement, f.opener);
  });
}

for (const key of ["Enter", " "]) {
  test(`native ${key === " " ? "Space" : key} activates Compare tabs and Close without workspace actions`, async () => {
    const f = fixture();
    const load = f.context.openCompareModal("target");
    f.resolvePair(0, "target");
    await load;
    assert.equal(f.key(key, f.responseTab).defaultPrevented, false);
    assert.equal(f.context.compareActiveTab, "response");
    assert.equal(f.key(key, f.requestTab).defaultPrevented, false);
    assert.equal(f.context.compareActiveTab, "request");
    assert.equal(f.key(key, f.close).defaultPrevented, false);
    assert.equal(f.els.compareModal.classList.contains("hidden"), true);
    assert.deepEqual(f.actions, []);
  });
}

for (const modal of ["confirm", "displaySettingsModal", "filterModal", "curlImportModal"]) {
  test(`${modal} retains keyboard priority over Compare`, () => {
    const f = fixture();
    f.els.compareModal.classList.remove("hidden");
    if (modal === "confirm") f.context.activeConfirmDialog = { close: () => f.actions.push("confirm close") };
    else f.els[modal].classList.remove("hidden");
    f.key("Escape");
    assert.equal(f.els.compareModal.classList.contains("hidden"), false);
    assert.equal(f.context.compareLoadGeneration, 0);
    assert.deepEqual(f.actions, [{ confirm: "confirm close", displaySettingsModal: "settings close", filterModal: "filter close", curlImportModal: "curl close" }[modal]]);
  });
}

for (const modal of ["confirm", "displaySettingsModal", "filterModal", "curlImportModal"]) {
  test(`a comparison finishing behind newer ${modal} does not cover it or steal focus`, async () => {
    const f = fixture();
    f.opener.focus();
    const load = f.context.openCompareModal("target");
    const newerControl = { marker: "newer dialog focus" };
    if (modal === "confirm") f.context.activeConfirmDialog = { close() {} };
    else f.els[modal].classList.remove("hidden");
    f.document.activeElement = newerControl;
    f.resolvePair(0, "target");
    await load;
    assert.equal(f.document.activeElement, newerControl);
    assert.equal(f.els.compareModal.classList.contains("hidden"), true);
    assert.deepEqual(f.rendered, []);
  });
}

test("replacing a visible comparison keeps its original focus-return target", async () => {
  const f = fixture();
  f.opener.focus();
  const first = f.context.openCompareModal("first-target");
  f.resolvePair(0, "first-target");
  await first;
  f.responseTab.focus();
  const second = f.context.openCompareModal("second-target");
  f.resolvePair(2, "second-target");
  await second;
  assert.equal(f.document.activeElement, f.responseTab, "A refresh must not reset focus inside the modal");
  f.key("Escape");
  assert.equal(f.document.activeElement, f.opener);
});

test("closing Compare beneath a newer dialog leaves that dialog's focus alone", async () => {
  const f = fixture();
  const load = f.context.openCompareModal("target");
  f.resolvePair(0, "target");
  await load;
  f.context.activeConfirmDialog = { close() {} };
  const newerControl = { marker: "newer dialog focus" };
  f.document.activeElement = newerControl;
  f.context.closeCompareModal();
  assert.equal(f.document.activeElement, newerControl);
  assert.equal(f.els.compareModal.classList.contains("hidden"), true);
});

test("clicking the Compare backdrop restores focus through the same dismissal", async () => {
  const f = fixture();
  f.opener.focus();
  const load = f.context.openCompareModal("target");
  f.resolvePair(0, "target");
  await load;
  f.els.compareModal.click();
  assert.equal(f.els.compareModal.classList.contains("hidden"), true);
  assert.equal(f.document.activeElement, f.opener);
});

for (const options of [{ isComposing: true }, { defaultPrevented: true }]) {
  test(`a pending initial comparison survives Escape when ${Object.keys(options)[0]}`, async () => {
    const f = fixture();
    const load = f.context.openCompareModal("target");
    f.key("Escape", f.diff, options);
    f.resolvePair(0, "target");
    await load;
    assert.equal(f.els.compareModal.classList.contains("hidden"), false);
    assert.deepEqual(f.rendered, [["base", "target"]]);
    assert.deepEqual(f.actions, []);
  });
}

test("closing Compare restores history focus when its context-menu invoker is hidden", async () => {
  const f = fixture();
  f.opener.focus();
  f.opener.getClientRects = () => [];
  const load = f.context.openCompareModal("target");
  f.resolvePair(0, "target");
  await load;
  f.key("Escape");
  assert.equal(f.document.activeElement, f.els.trafficRegion);
});
