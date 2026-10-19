const assert = require("node:assert/strict");
const test = require("node:test");
const vm = require("node:vm");
const { appSource } = require("./frontend-test-helpers.cjs");
const { fixture, CM } = require("./encoded-preview-fixture.cjs");
const text = node => [node.textContent, ...node.children.map(text)].join("\n");
const visible = f => f.controller.tooltip;
const listenerCount = node => [...node.listeners.values()].reduce((total, list) => total + list.length, 0);

function open(f) { f.hover(2); f.run(300); assert.ok(visible(f)); }

test("production ViewPlugin callback delays hover, renders inert text, and leaves editor state intact", () => {
  const f = fixture("%3Cscript%3Eharmless%3C%2Fscript%3E");
  const original = f.view.state;
  f.hover(2); assert.equal(visible(f), null); assert.equal(f.timers.size, 1);
  f.run(300);
  assert.match(text(visible(f)), /<script>harmless<\/script>/);
  assert.equal(visible(f).getAttribute("role"), "tooltip");
  assert.equal(f.view.state, original);
  assert.equal(CM.undo(f.view), false, "Hover must not create an edit or undo entry");
  assert.equal(f.contentDOM.getAttribute("aria-describedby"), visible(f).id);
  assert.match(visible(f).style.left, /^\d+px$/);
});

test("mouse movement within a decoded token does not restart or replace an open tooltip", () => {
  const f = fixture(); open(f); const tooltip = visible(f);
  f.hover(4); f.hover(6);
  assert.equal(visible(f), tooltip); assert.equal(f.timers.size, 0);
});

test("hover tooltip can be reached across a gap and selected or scrolled without dismissal", () => {
  const f = fixture(); open(f); const tooltip = visible(f);
  f.hover(99); assert.equal(f.timers.size, 1);
  f.dom.fire("mousemove", { target: tooltip, buttons: 0 });
  f.run(150); assert.equal(visible(f), tooltip);
  f.document.fire("pointerdown", { target: tooltip });
  f.document.fire("scroll", { target: tooltip });
  assert.equal(visible(f), tooltip);
  f.dom.fire("mouseleave", { relatedTarget: null }); f.run(150); assert.equal(visible(f), null);
});

for (const event of ["pointerdown", "scroll", "Escape", "blur", "resize", "focusout"]) {
  test(`actual ${event} dismissal removes preview and temporary document/window listeners`, () => {
    const f = fixture(); open(f);
    let result;
    if (event === "Escape") result = f.document.fire("keydown", { key: "Escape" });
    else if (["blur", "resize"].includes(event)) f.window.fire(event);
    else if (event === "focusout") f.dom.fire(event, { relatedTarget: null });
    else f.document.fire(event);
    assert.equal(visible(f), null); assert.equal(f.timers.size, 0);
    assert.equal(f.contentDOM.getAttribute("aria-describedby"), null);
    assert.equal(listenerCount(f.window), 0);
    assert.equal(listenerCount(f.document), 0);
    if (result) assert.ok(result.defaultPrevented && result.propagationStopped, "Escape closes only the preview");
  });
}

test("ordinary keys and selected source text retain editor behavior", () => {
  const f = fixture(); open(f);
  const event = f.document.fire("keydown", { key: "ArrowRight" });
  assert.equal(event.defaultPrevented, false); assert.equal(event.propagationStopped, false);
  f.view.dispatch({ selection: { anchor: 1, head: 8 } });
  assert.equal(visible(f), null);
  f.hover(2); f.run(); assert.equal(visible(f), null);
  f.view.dispatch({ selection: { anchor: 1 } });
  f.hover(2, { buttons: 1 }); f.run(); assert.equal(visible(f), null);
});

test("keyboard-focusable Decode button previews cursor or exact selection without an app/browser shortcut", () => {
  const f = fixture("prefix aGVsbG8gd29ybGQ= end", { anchor: 7, head: 23 });
  const original = f.view.state;
  const button = f.controller.button;
  assert.equal(button.type, "button");
  assert.equal(button.getAttribute("aria-label"), "Preview encoded value at cursor or selection");
  button.fire("click");
  assert.match(text(visible(f)), /hello world/);
  assert.equal(visible(f).getAttribute("aria-live"), "polite");
  assert.equal(button.getAttribute("aria-describedby"), visible(f).id);
  assert.equal(f.view.state, original);
  f.dom.fire("focusout", { relatedTarget: button }); assert.ok(visible(f));
  f.dom.fire("focusout", { relatedTarget: null }); assert.equal(visible(f), null);
});

test("keyboard invocation gives bounded feedback for unsupported and oversized selections", () => {
  for (const [value, selection] of [["plain", 2], ["%41".repeat(2000), { anchor: 0, head: 6000 }]]) {
    const f = fixture(value, selection);
    f.controller.button.fire("click");
    assert.match(text(visible(f)), /No supported encoded text/);
    assert.ok(text(visible(f)).length < 150);
  }
});

test("pending hover is cancelled by text, selection, scroll, resize or destroy and never revives", () => {
  for (const change of ["document", "selection", "scroll", "resize", "destroy"]) {
    const f = fixture(); f.hover(2); const pending = [...f.timers.values()][0].callback;
    if (change === "document") f.view.dispatch({ changes: { from: 0, to: f.view.state.doc.length, insert: "new%20record" } });
    if (change === "selection") f.view.dispatch({ selection: { anchor: 4 } });
    if (change === "scroll") f.document.fire("scroll");
    if (change === "resize") f.window.fire("resize");
    if (change === "destroy") f.controller.destroy();
    assert.equal(f.timers.size, 0); f.run(); assert.equal(visible(f), null);
    pending(); assert.equal(visible(f), null);
  }
});

test("detached or hidden editors do not display a pending preview", () => {
  for (const change of ["detach", "hide"]) {
    const f = fixture(); f.hover(2);
    if (change === "detach") f.dom.remove(); else f.dom.visible = false;
    f.run(); assert.equal(visible(f), null); assert.equal(listenerCount(f.document), 0);
  }
});

test("destroy removes local controls and listeners, and keeps any previous accessible description", () => {
  const f = fixture(); f.contentDOM.setAttribute("aria-describedby", "existing-help"); open(f);
  f.controller.destroy();
  assert.equal(f.contentDOM.getAttribute("aria-describedby"), "existing-help");
  assert.equal(f.dom.children.length, 1);
  assert.equal(listenerCount(f.dom), 0); assert.equal(listenerCount(f.document), 0); assert.equal(listenerCount(f.window), 0);
  assert.equal(f.view.sniperDecodedPreview, undefined);
});

test("real wrapper redraw clears a preview even for identical bytes and replacement ownership", () => {
  const f = fixture();
  const start = appSource.indexOf("class SniperCodeView {");
  const end = appSource.indexOf("// CodeMirror-based code pane instances", start);
  f.context.cmProgrammaticViews = new WeakSet();
  vm.runInContext(appSource.slice(start, end) + "\nglobalThis.CodeView = SniperCodeView;", f.context);
  const wrapper = Object.create(f.context.CodeView.prototype); wrapper.view = f.view;
  for (const method of ["setContent", "setContentPreservingSelection"]) {
    open(f); const original = f.view.state;
    wrapper[method]("hello%20world");
    assert.equal(visible(f), null); assert.equal(f.view.state, original);
  }
  open(f); wrapper.setContent("new%20record"); assert.equal(visible(f), null);
  assert.equal(f.view.state.doc.toString(), "new%20record");
});

test("actual base extension factory installs the preview only on HTTP text surfaces", () => {
  const f = fixture();
  Object.assign(f.context, { sniperCMTheme: [], httpDecoPlugin: [], hexDecoPlugin: [], diffDecoPlugin: [], payloadDecoPlugin: [] });
  vm.runInContext(f.source("createBaseExtensions"), f.context);
  for (const options of [{ httpHighlight: true, readOnly: true }, { httpHighlight: true }]) {
    assert.ok(f.context.createBaseExtensions(options).includes(f.context.plugin));
  }
  for (const options of [{ hexHighlight: true }, { diffHighlight: true }, {}]) {
    assert.equal(f.context.createBaseExtensions(options).includes(f.context.plugin), false);
  }
});


test("keyboard preview scroll keys stay on the Decode button and never move the editor selection", () => {
  const f = fixture(); f.controller.button.fire("click");
  const original = f.view.state;
  for (const [key, expected] of [["ArrowDown", 40], ["ArrowDown", 80], ["ArrowUp", 40], ["PageDown", 176], ["PageUp", 40], ["End", 800], ["Home", 0]]) {
    const event = f.controller.button.fire("keydown", { key });
    assert.equal(visible(f).scrollTop, expected);
    assert.ok(event.defaultPrevented && event.propagationStopped);
    assert.equal(f.view.state, original);
  }
  assert.equal(f.controller.button.fire("keydown", { key: "ArrowDown", ctrlKey: true }).defaultPrevented, false);
  f.controller.hide();
  assert.equal(f.controller.button.fire("keydown", { key: "ArrowDown" }).defaultPrevented, false);
});


test("nearest-caret coordinates cover both halves of a token and its final document character", () => {
  const f = fixture();
  for (let pos = 0; pos < f.view.state.doc.length; pos += 1) {
    for (const offset of [1, 3, 5, 7]) {
      f.controller.hide(); f.hover(pos, { clientX: pos * 8 + offset }); f.run(300);
      assert.ok(visible(f), `character ${pos}, horizontal offset ${offset}`);
    }
  }
  for (const point of [{ clientX: -5 }, { clientX: 999 }, { clientY: 0 }, { clientY: 50 }]) {
    f.controller.hide(); f.hover(2, point); f.run(); assert.equal(visible(f), null);
  }
});

test("visual wrapping uses the previous side of the ending caret", () => {
  const f = fixture();
  f.view.coordsAtPos = (pos, side) => {
    const wrap = pos === 4 && side !== -1;
    return { left: wrap ? 0 : pos * 8, right: wrap ? 0 : pos * 8, top: wrap ? 30 : 10, bottom: wrap ? 46 : 26 };
  };
  f.hover(3, { clientX: 31 }); f.run(300); assert.ok(visible(f));
});
