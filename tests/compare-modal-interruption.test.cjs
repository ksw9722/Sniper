// Synthetic dialog interruption checks. Run production modal callbacks and
// keyboard listeners with deferred local data; never bootstrap the app or API.
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
  const nodes = {}, listeners = new Map(), pending = [], rendered = [], actions = [];
  const document = { activeElement: null, readyState: "loading" };
  class Element {
    constructor(id, tagName = "DIV", parent = null) {
      Object.assign(this, { id, tagName, parent, children: [], handlers: new Map(), dataset: {},
        attributes: {}, isConnected: true, disabled: false, tabIndex: tagName === "BUTTON" ? 0 : -1,
        visibility: "visible", isContentEditable: false });
      const classes = new Set();
      this.classList = {
        add: (...names) => names.forEach(name => classes.add(name)),
        remove: (...names) => names.forEach(name => classes.delete(name)),
        contains: name => classes.has(name),
        toggle: (name, on = !classes.has(name)) => on ? classes.add(name) : classes.delete(name),
      };
      parent?.children.push(this);
      nodes[id] = this;
    }
    getClientRects() {
      return this.isConnected && !this.classList.contains("hidden")
        && (!this.parent || this.parent.getClientRects().length) ? [1] : [];
    }
    focus() {
      if (!this.disabled && this.getClientRects().length && this.visibility !== "hidden") document.activeElement = this;
    }
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    remove() { this.isConnected = false; this.children.forEach(child => child.remove()); }
    addEventListener(type, handler) { this.handlers.set(type, handler); }
    click() { this.handlers.get("click")?.({ target: this }); }
    matches(selector) { return selector.split(", ").includes(this.tagName.toLowerCase()); }
    closest(selector) {
      if (selector.includes(".simple-code-view") && this === nodes.compareDiffView) return this;
      if (selector.includes("contenteditable") && this.isContentEditable) return this;
      return this.parent?.closest(selector) || null;
    }
    setAttribute(name, value) {
      this.attributes[name] = value;
      if (name === "contenteditable") this.isContentEditable = value === "true";
    }
    getAttribute(name) { return this.attributes[name]; }
    querySelectorAll(selector) {
      if (selector === ".code-line") return [];
      return this.children.flatMap(child => [child, ...child.querySelectorAll(selector)]);
    }
    querySelector() { return null; }
  }
  document.body = new Element("body", "BODY");
  document.body.appendChild = () => {};
  document.createElement = () => {
    const modal = new Element("confirm", "DIV", document.body);
    const cancel = new Element("cancel", "BUTTON", modal), ok = new Element("ok", "BUTTON", modal);
    modal.querySelector = selector => selector.includes("cancel") ? cancel : ok;
    return modal;
  };
  document.addEventListener = (type, handler) => {
    if (!listeners.has(type)) listeners.set(type, []);
    listeners.get(type).push(handler);
  };
  document.getElementById = id => nodes[id] || null;
  document.querySelector = () => nodes.mainTab;
  for (const id of ["displaySettingsModal", "filterModal", "curlImportModal", "scannerSettingsBackdrop", "compareModal"]) {
    new Element(id, "DIV", document.body).classList.add("hidden");
  }
  for (const id of ["opener", "mainTab", "openDisplaySettingsButton", "openFilterSettingsButton"]) new Element(id, "BUTTON", document.body);
  new Element("trafficRegion", "SECTION", document.body).tabIndex = 0;
  for (const [id, parent] of [
    ["closeDisplaySettingsButton", "displaySettingsModal"], ["closeFilterModalButton", "filterModal"],
    ["scannerSettingsClose", "scannerSettingsBackdrop"], ["scannerSettingsCancel", "scannerSettingsBackdrop"],
    ["scannerSettingsSave", "scannerSettingsBackdrop"], ["closeCompareButton", "compareModal"],
  ]) new Element(id, "BUTTON", nodes[parent]);
  new Element("scannerBuiltinRules", "DIV", nodes.scannerSettingsBackdrop);
  new Element("curlImportInput", "TEXTAREA", nodes.curlImportModal).tabIndex = 0;
  const request = new Element("request", "BUTTON", nodes.compareModal), response = new Element("response", "BUTTON", nodes.compareModal);
  request.dataset.compareTab = "request"; response.dataset.compareTab = "response";
  new Element("compareDiffView", "PRE", nodes.compareModal).tabIndex = 0;
  document.querySelectorAll = selector => selector === "[data-compare-tab]" ? [request, response] : [nodes.compareDiffView];
  document.activeElement = nodes.opener;
  const scannerLoad = deferred();
  const action = name => () => { actions.push(name); return Promise.resolve(); };
  const context = loadFunctions([
    "openCompareModal", "closeCompareModal", "getActiveModalAction", "isModalVisible", "restoreModalFocus", "trapModalFocus", "isEditableTarget",
    "openDisplaySettingsModal", "closeDisplaySettingsModal", "closeCertificateModal", "openFilterModal", "closeFilterModal", "showConfirmDialog",
    "openCurlImportModal", "closeCurlImportModal", "openScannerSettings", "closeScannerSettings",
  ], {
    document, els: nodes, HTMLElement: Element, Element,
    window: { getComputedStyle: node => ({ visibility: node.visibility }), getSelection: () => null },
    state: { activeTool: "proxy", activeProxyTab: "http-history", selectedId: "target" },
    compareBaseId: "base", compareBaseSessionId: "session-a", compareLoadGeneration: 0,
    compareBaseRecord: null, compareTargetRecord: null, compareActiveTab: "request", compareReturnFocus: null,
    currentSessionId: () => "session-a", transactionPath: id => id,
    activeConfirmDialog: null, displaySettingsPreviewActive: false, displaySettingsReturnFocus: null, filterSettingsReturnFocus: null,
    displaySettingsEditedControls: new Set(), filterSettingsEditedControls: new Set(),
    hydrateDisplaySettingsForm() {}, applyDisplaySettingsState() {}, renderShortcutReference() {}, selectSettingsTab() {}, hydrateFilterForm() {},
    saveDisplaySettingsFromForm: action("settings apply"), applyFilterSettings: action("filter apply"),
    scannerSettingsBaseline: null, scannerSettingsSessionId: null, scannerSettingsGeneration: 0,
    scannerSettingsSavePending: null, scannerConfigStateGeneration: 0, scannerConfigContextIsCurrent: () => true,
    scannerConfigSnapshot: config => config, loadScannerConfig: () => scannerLoad.promise, BUILTIN_RULE_LABELS: {}, renderCustomRulesEditor() {},
    saveScannerSettingsFromModal: action("scanner save"), showToast: action("toast"), escapeHtml: text => text,
    findingsArrowNav: action("findings"), moveHistorySelection: action("history"),
    openReplayFromSelection: action("replay"), openFuzzerFromSelection: action("fuzzer"), handleSendActionError: error => { throw error; },
    fetch(path) { const response = deferred(); pending.push({ path, ...response }); return response.promise; },
  });
  context.renderCompareModal = () => rendered.push(context.compareTargetRecord.id);
  nodes.scannerSettingsClose.addEventListener("click", context.closeScannerSettings);
  nodes.scannerSettingsCancel.addEventListener("click", context.closeScannerSettings);
  nodes.scannerSettingsSave.addEventListener("click", context.saveScannerSettingsFromModal);
  function register(startMarker, endMarker) {
    const start = appSource.indexOf(startMarker), end = appSource.indexOf(endMarker, start);
    assert.ok(start >= 0 && end > start, startMarker);
    vm.runInContext(appSource.slice(start, end), context);
  }
  // Preserve production listener order and the real read-only caret setup.
  register("(function initCodeViewLineNav() {", "\n\n// ─── CodeMirror 6 Integration");
  const findingsStart = appSource.indexOf("  // Arrow key navigation\n", appSource.indexOf("function bindFindingsEvents()"));
  const findingsEnd = appSource.indexOf("\n\n  // Findings detail search", findingsStart);
  assert.ok(findingsStart >= 0 && findingsEnd > findingsStart);
  vm.runInContext(appSource.slice(findingsStart, findingsEnd), context);
  register('  document.addEventListener("keydown", (event) => {\n    const activeModalAction', '\n\n  document.addEventListener("copy",');
  for (const handler of listeners.get("DOMContentLoaded") || []) handler();
  assert.equal(nodes.compareDiffView.getAttribute("data-readonly-editable"), "1");
  function key(key, target = document.activeElement, options = {}) {
    const event = { key, target, defaultPrevented: false, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false,
      preventDefault() { this.defaultPrevented = true; }, ...options };
    target?.focus?.();
    for (const handler of listeners.get("keydown") || []) handler(event);
    if (!event.defaultPrevented && target?.tagName === "BUTTON" && ["Enter", " "].includes(key)) target.click();
    return event;
  }
  function resolvePair(offset = 0, target = "target") {
    pending[offset].resolve({ ok: true, json: async () => ({ id: "base" }) });
    pending[offset + 1].resolve({ ok: true, json: async () => ({ id: target }) });
  }
  async function openDialog(name) {
    if (name === "settings") context.openDisplaySettingsModal();
    if (name === "filter") context.openFilterModal();
    if (name === "confirm") context.showConfirmDialog("Synthetic confirmation", action("confirmed"));
    if (name === "curl") context.openCurlImportModal();
    if (name === "scanner") {
      const opening = context.openScannerSettings();
      scannerLoad.resolve({ rules: {}, custom_rules: [] });
      await opening;
      nodes.scannerSettingsClose.focus();
    }
  }
  return { context, nodes, document, key, pending, rendered, actions, resolvePair, openDialog, scannerLoad,
    hidden: () => nodes.compareModal.classList.contains("hidden") };
}

for (const dialog of ["settings", "filter", "confirm", "curl", "scanner"]) {
  for (const dismissal of ["Escape", "close"]) {
    test(`newer ${dialog} opened and ${dismissal}-closed does not revive the pending comparison`, async () => {
      const f = fixture(), loading = f.context.openCompareModal("target");
      await f.openDialog(dialog);
      if (dismissal === "Escape") f.key("Escape");
      else ({ settings: f.context.closeDisplaySettingsModal, filter: f.context.closeFilterModal,
        confirm: () => f.context.activeConfirmDialog.close(), curl: f.context.closeCurlImportModal,
        scanner: f.context.closeScannerSettings })[dialog]();
      f.nodes.mainTab.focus();
      f.resolvePair(); await loading;
      assert.equal(f.hidden(), true);
      assert.deepEqual(f.rendered, []);
      assert.equal(f.document.activeElement, f.nodes.mainTab);
      assert.equal(f.context.compareBaseId, "base", "Cancel only the pending reveal, not the chosen base");
      const fresh = f.context.openCompareModal("fresh");
      f.resolvePair(2, "fresh"); await fresh;
      assert.equal(f.hidden(), false);
      assert.deepEqual(f.rendered, ["fresh"]);
    });
  }
}

test("opening async scanner settings cancels Compare before scanner configuration arrives", async () => {
  const f = fixture(), comparison = f.context.openCompareModal("target");
  const scanner = f.context.openScannerSettings();
  f.resolvePair(); await comparison;
  assert.equal(f.hidden(), true);
  assert.deepEqual(f.rendered, []);
  f.scannerLoad.resolve({ rules: {}, custom_rules: [] }); await scanner;
  assert.equal(f.nodes.scannerSettingsBackdrop.classList.contains("hidden"), false);
});

test("a visible scanner dialog joins modal precedence even when already open before the Compare request", async () => {
  const f = fixture(); await f.openDialog("scanner");
  const loading = f.context.openCompareModal("target");
  f.resolvePair(); await loading;
  assert.equal(f.hidden(), true);
  assert.equal(f.context.getActiveModalAction()?.modal, f.nodes.scannerSettingsBackdrop);
  assert.equal(f.document.activeElement, f.nodes.scannerSettingsClose);
});

for (const key of ["Escape", "Enter"]) {
  test(`a confirmation over scanner settings owns ${key} without closing or saving scanner`, async () => {
    const f = fixture(); await f.openDialog("scanner");
    f.context.showConfirmDialog("Synthetic confirmation", () => f.actions.push("confirmed"));
    f.key(key);
    assert.equal(f.nodes.scannerSettingsBackdrop.classList.contains("hidden"), false);
    assert.equal(f.context.activeConfirmDialog, null);
    assert.deepEqual(f.actions, []);
  });
}

for (const control of ["scannerSettingsClose", "scannerSettingsCancel", "scannerSettingsSave"]) {
  for (const key of ["Enter", " "]) {
    test(`scanner ${control} preserves native ${key === " " ? "Space" : key} activation`, async () => {
      const f = fixture(); await f.openDialog("scanner");
      f.key(key, f.nodes[control]);
      assert.deepEqual(f.actions, control === "scannerSettingsSave" ? ["scanner save"] : []);
      assert.equal(f.nodes.scannerSettingsBackdrop.classList.contains("hidden"), control !== "scannerSettingsSave");
    });
  }
}

for (const options of [{ isComposing: true }, { defaultPrevented: true }]) {
  test(`scanner leaves ${Object.keys(options)[0]} Escape alone`, async () => {
    const f = fixture(); await f.openDialog("scanner");
    f.key("Escape", f.nodes.scannerSettingsClose, options);
    assert.equal(f.nodes.scannerSettingsBackdrop.classList.contains("hidden"), false);
  });
}

for (const shiftKey of [false, true]) {
  test(`scanner confines ${shiftKey ? "Shift+Tab" : "Tab"} to enabled visible controls`, async () => {
    const f = fixture(); await f.openDialog("scanner");
    const target = shiftKey ? f.nodes.scannerSettingsClose : f.nodes.scannerSettingsSave;
    assert.equal(f.key("Tab", target, { shiftKey }).defaultPrevented, true);
    assert.equal(f.document.activeElement, shiftKey ? f.nodes.scannerSettingsSave : f.nodes.scannerSettingsClose);
  });
}

for (const dialog of ["settings", "filter", "confirm", "curl", "scanner"]) {
  test(`newer ${dialog} cancels only a pending replacement and preserves the visible comparison`, async () => {
    const f = fixture(), first = f.context.openCompareModal("first");
    f.resolvePair(0, "first"); await first;
    const parsing = deferred(), replacement = f.context.openCompareModal("late");
    f.pending[2].resolve({ ok: true, json: async () => ({ id: "base" }) });
    f.pending[3].resolve({ ok: true, json: () => parsing.promise });
    await Promise.resolve();
    await f.openDialog(dialog);
    f.key("Escape");
    parsing.resolve({ id: "late" }); await replacement;
    assert.equal(f.hidden(), false);
    assert.deepEqual(f.rendered, ["first"]);
    assert.equal(f.context.compareTargetRecord.id, "first");
    f.key("Escape");
    assert.equal(f.hidden(), true);
    assert.equal(f.document.activeElement, f.nodes.opener);
  });
}
