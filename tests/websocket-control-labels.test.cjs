// Static markup checks only; no app code, browser, or WebSocket operation runs.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const html = fs.readFileSync(path.join(__dirname, "../web/index.html"), "utf8");
const css = fs.readFileSync(path.join(__dirname, "../web/styles.css"), "utf8");

// These controls use quoted attributes and explicit closing tags in index.html.
function elementById(id) {
  const matches = [...html.matchAll(new RegExp(`<([a-z][\\w-]*)\\b([^>]*\\bid="${id}"[^>]*)>([^]*?)<\\/\\1>`, "g"))];
  assert.equal(matches.length, 1, `Expected one element with id ${id}`);
  const [, tag, source, content] = matches[0];
  const attrs = Object.fromEntries([...source.matchAll(/([^\s=]+)(?:="([^"]*)")?/g)]
    .map(([, name, value]) => [name, value ?? ""]));
  return { tag, attrs, content };
}

for (const [id, name, className] of [
  ["wsSchemeSelect", "WebSocket scheme", "compact-field-inline"],
  ["wsMessageType", "WebSocket message type", "compact-field-inline ws-type-field"],
]) {
  test(`${id} has an explicit name and retains its native select semantics`, () => {
    const control = elementById(id);
    assert.equal(control.tag, "select");
    assert.deepEqual(control.attrs, { id, class: className, "aria-label": name });
  });
}

test("the named selectors retain their existing option order and implicit defaults", () => {
  for (const [id, expected] of [
    ["wsSchemeSelect", [["wss", "WSS"], ["ws", "WS"]]],
    ["wsMessageType", [["text", "Text"], ["binary", "Binary"], ["ping", "Ping"], ["pong", "Pong"]]],
  ]) {
    const { content } = elementById(id);
    const options = [...content.matchAll(/<option value="([^"]+)">([^<]+)<\/option>/g)];
    assert.deepEqual(options.map(([, value, text]) => [value, text]), expected);
    assert.equal(content.replace(/<option value="[^"]+">[^<]+<\/option>/g, "").trim(), "");
  }
});

test("the editable message is named by its existing visible MESSAGE caption", () => {
  const editor = elementById("wsMessageHighlight");
  assert.equal(editor.attrs["aria-labelledby"], "wsMessageLabel");
  const label = elementById(editor.attrs["aria-labelledby"]);
  assert.equal(label.tag, "label");
  assert.deepEqual(label.attrs, { id: "wsMessageLabel", class: "ws-editor-label" });
  assert.equal(label.content.trim(), "MESSAGE");
  assert.equal(editor.attrs["aria-label"], undefined);
  const rule = css.match(/\.ws-editor-label\s*\{([^}]+)\}/);
  assert.ok(rule, "The existing caption styles remain present");
  assert.doesNotMatch(rule[1], /(?:display:\s*none|visibility:\s*hidden)/);
});

test("textbox semantics belong to the existing plaintext-only multiline editor", () => {
  const editor = elementById("wsMessageHighlight");
  assert.equal(editor.tag, "pre");
  assert.deepEqual(editor.attrs, {
    id: "wsMessageHighlight",
    class: "ws-message-highlight",
    contenteditable: "plaintext-only",
    role: "textbox",
    "aria-multiline": "true",
    "aria-labelledby": "wsMessageLabel",
    spellcheck: "false",
    "data-placeholder": "Type a message...",
  });
  assert.equal(editor.content, "");
});

test("the hidden backing textarea remains separate from the named editable region", () => {
  const proxy = elementById("wsMessageEditor");
  assert.equal(proxy.tag, "textarea");
  assert.deepEqual(proxy.attrs, {
    id: "wsMessageEditor", class: "ws-message-editor-hidden", rows: "6", spellcheck: "false",
  });
  const rule = css.match(/\.ws-message-editor-hidden\s*\{([^}]+)\}/);
  assert.ok(rule, "The backing textarea retains its hidden styles");
  assert.match(rule[1], /display:\s*none\s*!important;/);
});

test("the new label reference resolves uniquely without dangling or duplicate IDs", () => {
  const editor = elementById("wsMessageHighlight");
  const references = (editor.attrs["aria-labelledby"] || "").trim().split(/\s+/).filter(Boolean);
  assert.deepEqual(references, ["wsMessageLabel"]);
  for (const id of [...references, "wsMessageHighlight", "wsMessageEditor", "wsSchemeSelect", "wsMessageType"]) {
    const occurrences = [...html.matchAll(new RegExp(`\\bid="${id}"`, "g"))];
    assert.equal(occurrences.length, 1, `${id} must identify exactly one element`);
    elementById(id);
  }
});
