// Synthetic clipboard APIs only; these tests never access a real clipboard.
const assert = require("node:assert/strict");
const test = require("node:test");
const { loadFunctions } = require("./frontend-test-helpers.cjs");

function fixture({ modern, selectError, copyError, copied = true } = {}) {
  const attached = new Set(), writes = [], calls = [];
  const document = {
    body: { appendChild(node) { attached.add(node); } },
    createElement(tag) {
      assert.equal(tag, "textarea");
      return {
        value: "", style: {}, setAttribute() {},
        select() {
          calls.push("select");
          if (selectError) throw selectError;
        },
        remove() { calls.push("remove"); attached.delete(this); },
      };
    },
    execCommand(command) {
      assert.equal(command, "copy");
      calls.push("copy");
      assert.equal(attached.size, 1);
      writes.push([...attached][0].value);
      if (copyError) throw copyError;
      return copied;
    },
  };
  const navigator = {};
  if (modern) navigator.clipboard = { writeText: modern };
  const { copyTextToClipboard } = loadFunctions(["copyTextToClipboard"], { document, navigator });
  return { copyTextToClipboard, attached, writes, calls };
}

test("modern clipboard success does not create a fallback node", async () => {
  const values = [];
  const f = fixture({ modern: async text => values.push(text) });
  await f.copyTextToClipboard("plain synthetic text");
  assert.deepEqual(values, ["plain synthetic text"]);
  assert.equal(f.attached.size, 0);
  assert.deepEqual(f.calls, []);
});

for (const modernRejected of [false, true]) {
  test(`fallback success cleans up (modern rejected: ${modernRejected})`, async () => {
    const f = fixture({ modern: modernRejected ? async () => { throw new Error("unavailable"); } : undefined });
    await f.copyTextToClipboard("synthetic 日本語 😀\nsecond line");
    assert.deepEqual(f.writes, ["synthetic 日本語 😀\nsecond line"]);
    assert.deepEqual(f.calls, ["select", "copy", "remove"]);
    assert.equal(f.attached.size, 0);
  });
}

test("a false fallback result rejects after removing its temporary node", async () => {
  const f = fixture({ copied: false });
  await assert.rejects(f.copyTextToClipboard("synthetic text"), /Clipboard copy failed/);
  assert.equal(f.attached.size, 0);
  assert.deepEqual(f.calls, ["select", "copy", "remove"]);
});

for (const operation of ["select", "copy"]) {
  test(`a throwing ${operation} removes its temporary node and preserves the error`, async () => {
    const error = new Error(`synthetic ${operation} failure`);
    const f = fixture(operation === "select" ? { selectError: error } : { copyError: error });
    for (let i = 0; i < 5; i++) {
      await assert.rejects(f.copyTextToClipboard(`synthetic attempt ${i}`), actual => actual === error);
      assert.equal(f.attached.size, 0, "failed attempts must not accumulate temporary textareas");
    }
    assert.equal(f.calls.filter(call => call === "remove").length, 5);
    if (operation === "select") assert.deepEqual(f.writes, []);
  });
}

test("nullish input stays a no-op and other values retain string conversion", async () => {
  const f = fixture();
  await f.copyTextToClipboard(null);
  await f.copyTextToClipboard(undefined);
  assert.deepEqual(f.calls, []);
  await f.copyTextToClipboard(0);
  await f.copyTextToClipboard("");
  assert.deepEqual(f.writes, ["0", ""]);
  assert.equal(f.attached.size, 0);
});
