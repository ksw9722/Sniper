/* Offline fixture DTO, asset, and production-DOM checks. No app or network. */
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const ROOT = path.resolve(__dirname, '../..');
const fixture = require('./source-lifetime-browser.js');
const context = vm.createContext({ URL, URLSearchParams, AbortController, console });
vm.runInContext(fs.readFileSync(path.join(ROOT, 'web/service-map.js'), 'utf8'), context);
const map = context.SniperServiceMap;
const accepted = fixture.validateData(map);
assert.equal(accepted, 376);
for (const technology of ['absent', 'empty', 'declared']) {
  const row = fixture.evidencePayload(fixture.token(), { ...fixture.defaults, technology }).row;
  assert.equal(Object.hasOwn(row, 'technology_observations'), technology !== 'absent');
  assert.equal(map.normalizeRowTechnologies(row.technology_observations, 2).length, technology === 'declared' ? 1 : 0);
}
for (const scope of [false, true]) {
  const id = fixture.token(), row = fixture.evidencePayload(id).row;
  assert.equal(map.normalizePeerMatches(fixture.peerPayload(id, null, fixture.defaults, scope), fixture.defaults.session, id, row, map.normalizeSocketPeer(fixture.peerObservation()), scope, fixture.defaults.generation).origins.length, 2);
}
const valid = fixture.htmlPayload(), invalid = structuredClone(valid);
invalid.reasons = ['synthetic_unknown_reason'];
assert.throws(() => map.normalizeHtmlGenerators(invalid, fixture.defaults.session, fixture.token(), fixture.evidencePayload().row));
const wrongSource = fixture.bodyPayload(); wrongSource.source_sequence = '1';
assert.throws(() => map.normalizeBodyReferences(wrongSource, fixture.defaults.session, fixture.token(), fixture.evidencePayload().row));
const html = fs.readFileSync(path.join(__dirname, 'source-lifetime-browser.html'), 'utf8');
const production = fs.readFileSync(path.join(ROOT, 'web/index.html'), 'utf8');
const sectionStart = '          <section class="service-map-workspace"';
assert.equal(html.slice(html.indexOf(sectionStart), html.indexOf('    <details class="fixture-log"')), production.slice(production.indexOf(sectionStart), production.indexOf('          <section class="panel-card hidden" id="targetPathsPanel"', production.indexOf(sectionStart))));
assert.match(html, /img-src data:;/); assert.match(html, /connect-src 'none'/); assert.match(html, /form-action 'none'/); assert.match(html, /worker-src 'none'/);
assert.doesNotMatch(html, /(?:src|href)=["']https?:\/\//); assert.doesNotMatch(html, /app\.js/);
assert.deepEqual([...html.matchAll(/<script src="([^"]+)"/g)].map(match => match[1]), ['../../web/service-map.js', 'source-lifetime-browser.js']);
const identifiers = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
assert.equal(new Set(identifiers).size, identifiers.length, 'Fixture DOM ids must be unique');
const script = fs.readFileSync(path.join(__dirname, 'source-lifetime-browser.js'), 'utf8');
for (const [, id] of script.matchAll(/\$\("([^"]+)"\)/g)) assert.ok(identifiers.includes(id), `Missing fixture control ${id}`);
for (const [, id] of script.matchAll(/on\("([^"]+)"/g)) assert.ok(identifiers.includes(id), `Missing click control ${id}`);
for (const host of [...JSON.stringify(fixture.mapPayload()).matchAll(/https:\/\/([^/"\s]+)/g)].map(match => match[1])) assert.match(host, /\.example\.com$/);
const manifestPath = path.join(__dirname, 'source-lifetime-browser.manifest.json');
assert.ok(fs.existsSync(manifestPath), 'Refresh the fixture manifest before checking it');
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
for (const [file, expected] of Object.entries(manifest.sha256)) assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(ROOT, file))).digest('hex'), expected, `Stale fixture provenance for ${file}; rerun source-lifetime-browser.manifest.py after production edits`);
console.log(`PASS: ${accepted} valid supplementary DTOs; technology absent/empty/declared; scoped peers; malformed DTO rejection; production DOM; CSP/assets/control ids; provenance hashes.`);
