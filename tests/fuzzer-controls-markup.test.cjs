// Static markup contracts only. No application, network, payload execution or backend is loaded.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
const css = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
const fuzzer = html.slice(html.indexOf('<section class="fuzzer-shell'), html.indexOf('<section class="sequence-shell'));
const tags = [...fuzzer.matchAll(/<([a-z][\w-]*)\b([^>]*?)(\/?)>/gi)].map(([, tag, source]) => {
  const attrs = {};
  for (const [, key, value] of source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)) attrs[key] = value ?? '';
  return { tag, attrs };
});
function byId(id) {
  const matches = tags.filter(node => node.attrs.id === id);
  assert.equal(matches.length, 1, `Expected exactly one Fuzzer element #${id}`);
  return matches[0];
}
function values(id) {
  const contents = fuzzer.match(new RegExp(`<select[^>]*id="${id}"[^>]*>([\\s\\S]*?)<\\/select>`))?.[1];
  assert.ok(contents, `Missing select #${id}`);
  return [...contents.matchAll(/<option value="([^"]*)"/g)].map(([, value]) => value);
}

const buttons = ['resetFuzzerButton', 'startFuzzerButton', 'fuzzerMarkSelectionButton', 'fuzzerUndoMarkerButton',
  'fuzzerMarkCandidateButton', 'fuzzerImportPayloadButton', 'fuzzerApplyPresetButton', 'fuzzerGenerateRangeButton',
  'fuzzerCompareBaseButton', 'fuzzerCompareButton', 'fuzzerRefreshRunsButton', 'fuzzerViewRunButton', 'fuzzerReuseRunButton'];
const labeledControls = ['fuzzerCandidateSelect', 'fuzzerPayloadImportMode', 'fuzzerPayloadPreset',
  'fuzzerRangeStart', 'fuzzerRangeEnd', 'fuzzerRangeStep', 'fuzzerPayloadsEditor', 'fuzzerResultStatus',
  'fuzzerResultMinBytes', 'fuzzerResultMaxBytes', 'fuzzerResultMinMs', 'fuzzerResultMaxMs',
  'fuzzerResultSort', 'fuzzerResultDirection', 'fuzzerSavedRunSelect'];

test('Fuzzer controls retain unique IDs and keyboard-native button actions', () => {
  for (const id of buttons) {
    assert.equal(byId(id).tag, 'button');
    assert.equal(byId(id).attrs.type, 'button');
    assert.equal((html.match(new RegExp(`\\bid="${id}"`, 'g')) || []).length, 1);
  }
  for (const id of labeledControls) {
    byId(id);
    assert.ok(tags.some(node => node.tag === 'label' && node.attrs.for === id), `Missing label for #${id}`);
  }
  for (const node of tags) assert.notEqual(node.attrs.tabindex, '-1', `${node.attrs.id} cannot be removed from keyboard navigation`);
});

test('payload controls declare append default, exact presets and accessible file picker', () => {
  assert.deepEqual(values('fuzzerPayloadImportMode'), ['append', 'replace']);
  assert.match(fuzzer, /<option value="append" selected>Append to list<\/option>/);
  assert.deepEqual(values('fuzzerPayloadPreset'), ['text-boundaries', 'number-boundaries']);
  assert.equal(byId('fuzzerPayloadFile').attrs.type, 'file');
  assert.ok('hidden' in byId('fuzzerPayloadFile').attrs);
  assert.ok(byId('fuzzerPayloadFile').attrs['aria-label']);
  assert.equal(byId('fuzzerImportPayloadButton').attrs['aria-controls'], 'fuzzerPayloadFile');
  for (const id of ['fuzzerRangeStart', 'fuzzerRangeEnd', 'fuzzerRangeStep']) assert.equal(byId(id).attrs.type, 'number');
  assert.equal(byId('fuzzerPayloadsEditor').attrs['aria-describedby'], 'fuzzerPayloadHelp');
  assert.match(fuzzer, /Spaces are preserved\. Blank lines are empty payloads; a final line break is ignored\./);
});

test('preflight exposes replace-all and sequential semantics without fake run progress', () => {
  assert.match(fuzzer, /Each payload replaces every <code>\$payload\$<\/code> marker in one request/);
  assert.match(fuzzer, /Requests run sequentially; the server returns results when the run finishes/);
  for (const id of ['fuzzerPreflightSummary', 'fuzzerPreflightNotice']) assert.equal(byId(id).attrs.role, 'status');
  assert.equal(byId('fuzzerPreviewSamples').tag, 'div');
  assert.match(fuzzer, /<details[^>]*fuzzer-preview-disclosure[^>]*>\s*<summary>Preview generated requests<\/summary>\s*<div id="fuzzerPreviewSamples"/);
  assert.doesNotMatch(fuzzer, /<progress\b|fuzzerStop|>Stop<|live progress/i);
});

test('result filters, comparison and saved run controls match JS contracts', () => {
  assert.deepEqual(values('fuzzerResultStatus'), ['all', 'error', '2xx', '3xx', '4xx', '5xx']);
  assert.deepEqual(values('fuzzerResultSort'), ['index', 'status', 'bytes', 'duration']);
  assert.deepEqual(values('fuzzerResultDirection'), ['asc', 'desc']);
  for (const id of ['fuzzerResultMinBytes', 'fuzzerResultMaxBytes', 'fuzzerResultMinMs', 'fuzzerResultMaxMs']) {
    assert.equal(byId(id).attrs.type, 'number');
    assert.equal(byId(id).attrs.min, '0');
  }
  for (const id of ['fuzzerCompareBaseButton', 'fuzzerCompareButton', 'fuzzerViewRunButton', 'fuzzerReuseRunButton']) {
    assert.ok('disabled' in byId(id).attrs, `#${id} must wait for a selected result or run`);
  }
  assert.equal(byId('fuzzerResultFilterCount').attrs.role, 'status');
  assert.equal(byId('fuzzerSavedRunNotice').attrs.role, 'status');
  assert.match(fuzzer, /View results keeps your current draft\. Reuse setup asks before replacing it; the original target or source request may be unavailable\./);
});

test('helpers load before app and disclosures keep bounded responsive controls', () => {
  const helper = html.indexOf('<script src="/fuzzer-workflow.js" defer></script>');
  assert.ok(helper >= 0 && helper < html.indexOf('<script src="/app.js" defer></script>'));
  for (const node of tags.filter(node => node.tag === 'details')) assert.ok(!('open' in node.attrs));
  assert.match(css, /\.fuzzer-disclosure-body\s*\{[^}]*max-height:[^}]*overflow: auto;/s);
  assert.match(css, /\.fuzzer-top-row \.editor-panel\s*\{[^}]*min-height: 80px;/s);
  assert.match(css, /@media \(max-width: 620px\)\s*\{[\s\S]*\.fuzzer-filter-grid/);
  assert.match(css, /\.fuzzer-disclosure > summary:focus-visible/);
});
