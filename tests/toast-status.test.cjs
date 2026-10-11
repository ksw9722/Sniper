// Markup and synthetic DOM checks only: no app bootstrap or screen-reader claim.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { loadFunctions } = require("./frontend-test-helpers.cjs");

const html = fs.readFileSync(path.join(__dirname, "../web/index.html"), "utf8");
const containerMarkup = html.match(/<div\b([^>]*\bid="toastContainer"[^>]*)>([^]*?)<\/div>/);
assert.ok(containerMarkup, "Missing toast container");
const attributes = Object.fromEntries(
  [...containerMarkup[1].matchAll(/([\w-]+)="([^"]*)"/g)].map(([, name, value]) => [name, value]),
);

test("the empty toast status region exists before app initialization", () => {
  assert.equal((html.match(/\bid="toastContainer"/g) || []).length, 1);
  assert.equal(containerMarkup[2].trim(), "");
  const appScript = html.indexOf('<script src="/app.js"');
  assert.ok(appScript > containerMarkup.index);
  assert.equal(attributes.class, "toast-container");
  assert.equal(attributes.role, "status");
  // status supplies implicit polite semantics; an explicit override must agree.
  assert.ok(attributes["aria-live"] === undefined || attributes["aria-live"] === "polite");
  for (const name of ["tabindex", "autofocus", "hidden", "aria-hidden"]) {
    assert.doesNotMatch(containerMarkup[1], new RegExp(`\\b${name}\\b`));
  }
});

test("toast stack semantics target new content rather than older toasts or removals", () => {
  assert.equal(attributes["aria-atomic"], "false");
  assert.deepEqual(attributes["aria-relevant"]?.split(/\s+/).sort(), ["additions", "text"]);
});

function fixture({ missingContainer = false } = {}) {
  let now = 0, nextTimerId = 0;
  const timers = [], mutations = [], focusCalls = [], created = [];
  const opener = {};
  const document = { activeElement: opener };
  function element() {
    let content = "";
    const node = {
      className: "", style: {}, children: [], parentNode: null,
      get textContent() { return content; },
      set textContent(value) { content = String(value); mutations.push({ kind: "text", node, content }); },
      set innerHTML(_) { assert.fail("Toast messages must remain plain text"); },
      focus() { focusCalls.push(node); document.activeElement = node; },
      appendChild(child) {
        child.parentNode = node;
        node.children.push(child);
        mutations.push({ kind: "append", node: child, content: child.textContent });
        return child;
      },
      remove() {
        if (node.parentNode) {
          const siblings = node.parentNode.children;
          siblings.splice(siblings.indexOf(node), 1);
          node.parentNode = null;
        }
      },
    };
    return node;
  }
  const container = element();
  container.getAttribute = name => attributes[name] ?? null;
  document.getElementById = id => id === "toastContainer" && !missingContainer ? container : null;
  document.createElement = tag => {
    assert.equal(tag, "div");
    const node = element();
    created.push(node);
    return node;
  };
  const context = loadFunctions(["showToast"], {
    document,
    setTimeout(callback, delay) {
      timers.push({ id: ++nextTimerId, callback, due: now + delay });
      return nextTimerId;
    },
  });
  function advance(ms) {
    const target = now + ms;
    timers.sort((a, b) => a.due - b.due || a.id - b.id);
    while (timers.length && timers[0].due <= target) {
      const timer = timers.shift();
      now = timer.due;
      timer.callback();
      timers.sort((a, b) => a.due - b.due || a.id - b.id);
    }
    now = target;
  }
  return { context, container, document, opener, timers, mutations, focusCalls, created, advance };
}

test("showToast appends complete plain-text messages into the pre-existing region without focus", () => {
  const f = fixture();
  const messages = ["Synthetic saved", "Synthetic notice", "Synthetic <b>failure</b> & detail"];
  for (const [index, type] of ["success", "info", "error"].entries()) {
    f.context.showToast(messages[index], type);
    const toast = f.container.children[index];
    assert.equal(toast.className, `toast toast-${type}`);
    assert.equal(toast.textContent, messages[index]);
    assert.equal(toast.parentNode, f.container);
    assert.deepEqual(f.mutations.slice(index * 2, index * 2 + 2).map(({ kind, content }) => ({ kind, content })), [
      { kind: "text", content: messages[index] },
      { kind: "append", content: messages[index] },
    ]);
  }
  assert.equal(f.container.getAttribute("role"), "status");
  assert.equal(f.document.activeElement, f.opener);
  assert.deepEqual(f.focusCalls, []);
});

test("default and custom toast timers retain independent fade and removal timing without focus", () => {
  const f = fixture();
  f.context.showToast("Synthetic repeated notice");
  f.context.showToast("Synthetic repeated notice", "error", 4500);
  const [first, second] = f.container.children;
  assert.notEqual(first, second);
  assert.equal(first.className, "toast toast-success");
  f.advance(1999);
  assert.equal(first.style.opacity, undefined);
  f.advance(1);
  assert.equal(first.style.opacity, "0");
  assert.equal(second.style.opacity, undefined);
  f.advance(299);
  assert.deepEqual(f.container.children, [first, second]);
  f.advance(1);
  assert.deepEqual(f.container.children, [second]);
  f.advance(2199);
  assert.equal(second.style.opacity, undefined);
  f.advance(1);
  assert.equal(second.style.opacity, "0");
  f.advance(299);
  assert.deepEqual(f.container.children, [second]);
  f.advance(1);
  assert.deepEqual(f.container.children, []);
  assert.equal(f.timers.length, 0);
  assert.equal(f.container.getAttribute("role"), "status");
  assert.equal(f.document.activeElement, f.opener);
  assert.deepEqual(f.focusCalls, []);
});

test("showToast stays a no-op if its container is absent", () => {
  const f = fixture({ missingContainer: true });
  f.context.showToast("Synthetic unused notice");
  assert.deepEqual(f.created, []);
  assert.deepEqual(f.timers, []);
  assert.equal(f.document.activeElement, f.opener);
  assert.deepEqual(f.focusCalls, []);
});
