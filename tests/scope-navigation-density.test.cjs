// Scope navigation uses the same density rules as existing Capture tabs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
const css = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
const app = fs.readFileSync(path.join(__dirname, '../web/app.js'), 'utf8');
const header = html.match(/<header class="target-workspace-header">([^]*?)<\/header>/)?.[1];
const rule = (selector, expectedDefinitions = 1) => {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const matches = [...css.matchAll(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`, 'g'))];
  assert.equal(matches.length, expectedDefinitions, `${selector} density definition count`);
  return matches[0][1];
};

test('Scope tabs share existing sub-tab styling without Capture handlers', () => {
  assert.ok(header);
  const tabs = [...header.matchAll(/<button\b([^>]+)>/g)].map(match => match[1]);
  assert.equal(tabs.length, 4);
  for (const tab of tabs) {
    assert.match(tab, /class="sub-tab target-tab(?: active)?"/);
    assert.match(tab, /data-target-view="(?:rules|service-map|apis|paths)"/);
    assert.match(tab, /role="tab"/);
    assert.doesNotMatch(tab, /data-proxy-tab/);
  }
  assert.match(app, /querySelectorAll\("\.sub-tab\[data-proxy-tab\]"\)/);
});

test('Scope strip omits the duplicate title and keeps accessible navigation labels', () => {
  assert.doesNotMatch(header, /<h[1-6]\b[^>]*>\s*Scope\s*<\/h[1-6]>/);
  assert.match(html, /<button class="main-tab" data-tool="target">Scope<\/button>/);
  assert.match(header, /role="tablist" aria-label="Scope views"/);
  assert.match(html, /id="targetRulesPanel" role="tabpanel" aria-labelledby="targetRulesTab"/);
  assert.match(html, /id="targetServiceMapPanel" role="tabpanel" aria-labelledby="targetServiceMapTab"/);
});

test('Scope strip keeps compact height and adds no vertical padding', () => {
  const strip = rule('.target-workspace-header');
  assert.match(strip, /padding:\s*0 8px;/);
  assert.match(strip, /min-height:\s*24px;/);
  assert.doesNotMatch(strip, /(?:^|;)\s*(?:height|padding-top|padding-bottom):/);
  assert.doesNotMatch(css, /\.target-tab(?:\.active)?\s*\{/);
});

test('Scope keeps existing responsive sub-tab padding and active indicator', () => {
  assert.match(css, /@media \(max-height: 920px\)\s*\{[^]*?\.sub-tab\s*\{\s*padding:\s*4px 8px 3px;/);
  assert.match(css, /\.sub-tab\.active::after\s*\{/);
  assert.match(rule('.target-tabs'), /overflow-x:\s*auto;/);
  assert.match(rule('.target-passive-label', 2), /margin-left:\s*auto;/);
});


test('Scrollable Scope tabs keep the keyboard focus ring inside their bounds', () => {
  assert.match(rule('.target-tab:focus-visible'), /outline-offset:\s*-2px;/);
});
