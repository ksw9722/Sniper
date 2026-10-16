// Synthetic retained metadata with the production controller. No app, sockets or capture.
'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const context = vm.createContext({ URL, URLSearchParams, AbortController, console });
vm.runInContext(fs.readFileSync(path.join(__dirname, '../web/service-map.js'), 'utf8'), context);
const map = context.SniperServiceMap;
const A = 'https://app.example.com', B = 'https://api.example.com', REF = 'https://reference.example.com', PEER = 'https://peer-only.example.com';
const SESSION = '11111111-1111-4111-8111-111111111111', GENERATION = '22222222-2222-4222-8222-222222222222';
const RECORD = '33333333-3333-4333-8333-333333333333', REVISION = '44444444-4444-4444-8444-444444444444', SNAPSHOT = '55555555-5555-4555-8555-555555555555';
const START = '2026-10-12T01:00:00Z', END = '2026-10-12T02:00:00Z', OBSERVED = '2026-10-12T01:00:01Z';
const token = (seq = 1, generation = GENERATION) => `v1.${SESSION}.${generation}.${seq}.${REVISION}.${RECORD}`;
const products = { nginx: ['nginx', 'web_server', 'server'], php: ['PHP', 'language_runtime', 'server'], express: ['Express', 'application_framework', 'x-powered-by'],
  apache_coyote: ['Apache-Coyote', 'connector', 'server'], gunicorn: ['Gunicorn', 'application_server', 'server'] };
function observation(id = 'nginx', version = '1.26.2', seq = 1, patch = {}) {
  return { product_id: id, name: products[id][0], catalog_category: products[id][1], header_name: products[id][2], source: 'response_header', strength: 'literal_declaration',
    reported_version: version, version_status: version === null ? 'not_reported' : 'reported', row_count: 1, first_seen: START, last_seen: START, evidence: [token(seq)], ...patch };
}
function endpoint(index = 0, offset = 1) { return { method: 'GET', path: `/path-${index}`, request_count: 1, statuses: [200], content_types: ['text/html'], evidence: [token(offset + index)] }; }
function origin(id = A, observations = [observation()], size = 1, offset = 1) {
  const value = { origin: id, in_scope: true, first_seen: START, last_seen: END, request_count: Math.max(10, size), endpoints: Array.from({ length: size }, (_, i) => endpoint(i, offset)), technology_clues: [], header_links: [], redirects: [] };
  if (observations !== undefined) value.technology_observations = observations;
  return value;
}
function snapshot(origins = [origin()], extra = {}) { return { schema_version: 1, session_id: SESSION, technology_catalog_version: 2,
  snapshot: { id: SNAPSHOT, generation: GENERATION, captured_at: END, complete: true, stopped_by: [], considered_rows: 400, retained_rows: 400, included_rows: 400, excluded_rows: 0, omitted_rows: 0, ...extra }, origins, warnings: [] }; }
const clone = value => JSON.parse(JSON.stringify(value));
const normalized = observations => map.normalizeSnapshot(snapshot([origin(A, observations)]), SESSION).origins[0];
function element() {
  const handlers = new Map(), classes = new Set();
  return { value: '', innerHTML: '', textContent: '', dataset: {}, checked: false, attributes: {}, clientWidth: 800, clientHeight: 500, scrollTop: 0, scrollLeft: 0,
    style: { setProperty() {} }, classList: { toggle(key, yes) { yes ? classes.add(key) : classes.delete(key); }, contains: key => classes.has(key) },
    setAttribute(key, value) { this.attributes[key] = value; }, addEventListener(key, fn) { handlers.set(key, fn); },
    dispatch(key, extra = {}) { return handlers.get(key)?.({ target: this, ...extra }); }, contains: () => false, querySelectorAll: () => [] };
}
const response = payload => ({ ok: true, status: 200, json: async () => payload });
function fixture() {
  let session = SESSION;
  const els = Object.fromEntries(['panel', 'status', 'refresh', 'search', 'scope', 'canvas', 'graphBounds', 'detail', 'content', 'catalog', 'viewport', 'productFilters', 'mapControls', 'allOrigins', 'directRelations', 'focusSelection', 'boundsDetails', 'graphDiagnostics', 'overlayStatus', 'zoomControls', 'zoomFit', 'zoomOut', 'zoomReset', 'zoomIn'].map(key => [key, element()]));
  els.panel.contains = () => true;
  const requests = [], document = { activeElement: null, body: {}, querySelector: () => null };
  // Opening an endpoint reads its parameters; these tests are about the other
  // reads, so that one is kept apart and left pending.
  const controller = map.createController({ elements: els, document, getSessionId: () => session, request(url, options) {
    let resolve; const promise = new Promise(done => { resolve = done; });
    if (!url.startsWith('/api/target/service-map/parameters?')) requests.push({ url, options, resolve });
    return promise;
  } });
  controller.setVisible(true);
  const state = controller.getState();
  function click(dataset) { const target = { dataset, closest: () => target }; els.panel.dispatch('click', { target }); }
  function filter(kind, value) { els.panel.dispatch('change', { target: { value, dataset: { serviceProductFilter: kind } } }); }
  function search(value) { els.search.value = value; els.search.dispatch('input'); }
  return { controller, state, els, requests, document, click, filter, search, setSession(value) { session = value; },
    async load(value = snapshot()) { const task = controller.refresh(); requests.at(-1).resolve(response(value)); return task; },
    async verify(index = 0, id = A, seq = index + 1) { controller.selectOrigin(id, index); const task = controller.loadEvidence(token(seq)); requests.at(-1).resolve(response(evidence(seq, id, index))); return task; } };
}
// A focused native select owns its popup independently of serialized markup.
// Model browser boolean-attribute normalization and detachment on innerHTML,
// which the ordinary inert fixture intentionally does not do.
function nativeControlModel(f, container) {
  const normalize = markup => markup.replace(/<[^>]+>/g, tag => tag.replace(/\b(selected|disabled)(?=[ >])/g, '$1=""').replace(/ +>/g, '>'));
  let html = normalize(container.innerHTML), current = [], replacements = 0;
  const install = markup => [...markup.matchAll(/<(?:select|input|button)\b[^>]*data-service-focus="([^"]+)"[^>]*>/g)].map(match => ({
    dataset: { serviceFocus: match[1] }, isConnected: true, popupOpen: false,
    getClientRects: () => [{}], focus() { f.document.activeElement = this; },
  }));
  current = install(html);
  container.contains = control => current.includes(control);
  container.querySelectorAll = () => current;
  Object.defineProperty(container, 'innerHTML', {
    get: () => html,
    set(markup) {
      for (const control of current) { control.isConnected = false; control.popupOpen = false; }
      if (current.includes(f.document.activeElement)) f.document.activeElement = f.document.body;
      html = normalize(markup);
      current = install(html); replacements += 1;
    },
  });
  return { controls: () => current, replacements: () => replacements };
}
function evidence(seq = 1, id = A, index = 0) { return { schema_version: 1, session_id: SESSION, technology_catalog_version: 2, evidence_id: token(seq), available: true, source: 'captured_http_metadata',
  row: { evidence_id: token(seq), sequence: String(seq), record_id: RECORD, origin: id, method: 'GET', path: `/path-${index}`, started_at: START, status: 200, metadata_complete: true, omitted: [] },
  socket_peer: { status: 'observed', observation: { source: 'http_response_remote_addr', route: 'direct', observed_at: OBSERVED, address: { ip: '192.0.2.10', port: 443 }, reason: null } } }; }
function bodyScan() { return { schema_version: 1, session_id: SESSION, evidence_id: token(), available: true, status: 'scanned', complete: true, reasons: [],
  source_origin: A, source_sequence: '1', source_started_at: START, retained_preview_bytes: 300, scanned_bytes: 300, capture_preview_truncated: false, scan_prefix_truncated: false,
  candidates_scanned: 1, omitted_reference_occurrences: 0, references: [{ target_origin: REF, occurrences: 1, preview_byte_offsets: [10], omitted_offsets: 0 }],
  limits: { ...map.BODY_REFERENCE_LIMITS }, coverage: 'literal_absolute_http_urls', offset_basis: 'retained_utf8_preview' }; }
function linkScan() {
  const target = `${REF}/path`, prefix = `<${target}>; rel="`, raw = `${prefix}service-doc"`;
  return { schema_version: 1, relation_catalog_version: 1, session_id: SESSION, evidence_id: token(), available: true, status: 'scanned', complete: true, reasons: [],
    source_origin: A, source_sequence: '1', source_started_at: START, source_method: 'GET', source_status: 200, retained_header_count: 1,
    coverage: 'absolute_http_link_declarations', offset_basis: 'retained_header_value_utf8',
    counts: { headers_inspected: 1, header_bytes_admitted: raw.length + 4, link_fields_inspected: 1, candidates_inspected: 1, members_framed: 1, parameters_inspected: 1, relation_tokens_inspected: 1, unknown_relation_tokens_inspected: 0 },
    observations: [{ relation: 'service-doc', target_origin: REF, source: 'response_link_header', strength: 'literal_declaration', header_name: 'link', header_index: 0, header_value_bytes: raw.length,
      link_value_span: { start: 0, end: raw.length }, target_span: { start: 1, end: target.length + 1 }, relation_span: { start: prefix.length, end: prefix.length + 11 },
      rule_id: 'response_link.service_doc.v1', offset_basis: 'retained_header_value_utf8' }], limits: { ...map.LINK_DECLARATION_LIMITS } };
}
function htmlScan() { return { schema_version: 1, generator_catalog_version: 1, session_id: SESSION, evidence_id: token(), available: true, status: 'scanned', complete: true, reasons: [],
  source_origin: A, source_sequence: '1', source_started_at: START, retained_preview_bytes: 100, scan_prefix_bytes: 100, capture_preview_truncated: false, scan_prefix_truncated: false,
  coverage: 'literal_html_generator_explicit_head', head_closed: true, parser_stop: null, tag_candidates_inspected: 3, additional_tag_candidate_encountered: false,
  additional_observation_encountered: false, unknown_product_declarations: 0, observations: [], limits: { ...map.HTML_GENERATOR_LIMITS }, offset_basis: 'retained_utf8_preview' }; }
function peerScan() { return { schema_version: 1, session_id: SESSION, evidence_id: token(), available: true, status: 'matched', complete: true, reasons: [], in_scope_only: false,
  coverage: 'retained_direct_socket_peer_equality', peer: { source: 'http_response_remote_addr', route: 'direct', address_family: 'ipv4', address: { ip: '192.0.2.10', port: 443 } },
  source: { evidence_id: token(), sequence: '1', origin: A, started_at: START, observed_at: OBSERVED, in_scope: true, in_window: true },
  snapshot: { id: SNAPSHOT, generation: GENERATION, captured_at: END, window_order: 'latest_retained_sequence', retained_rows: 3, considered_rows: 3, oldest_considered_sequence: '1', latest_considered_sequence: '3' },
  counts: { non_http_rows: 0, not_recorded_rows: 0, unavailable_observation_rows: 0, non_direct_rows: 0, invalid_observation_rows: 0, different_peer_rows: 0, invalid_metadata_rows: 0, scope_excluded_rows: 0, matching_rows: 2, returned_matching_rows: 2, omitted_matching_rows: 0, sample_rows: 2, omitted_sample_rows: 0 },
  origins: [B, PEER].map((id, i) => ({ origin: id, in_scope: true, matching_rows: 1, first_observed_at: OBSERVED, last_observed_at: OBSERVED,
    samples: [{ evidence_id: token(i + 2), sequence: String(i + 2), started_at: START, observed_at: OBSERVED }], omitted_samples: 0 })), limits: { ...map.PEER_MATCH_LIMITS } }; }
const cards = f => [...f.els.canvas.innerHTML.matchAll(/data-service-origin="([^"]+)"/g)].map(m => m[1]);
const rows = element => [...element.innerHTML.matchAll(/data-service-endpoint="(\d+)"/g)].map(m => Number(m[1]));
const plain = value => JSON.parse(JSON.stringify(value));

test('missing, present-empty and reference-only are distinct from an unknown version observation', () => {
  const missing = origin(); delete missing.technology_observations;
  missing.technology_clues = [{ name: 'nginx/9.9', source: 'response_header', confidence: 'observed_header', evidence: [] }];
  const a = map.normalizeSnapshot(snapshot([missing]), SESSION).origins[0], b = normalized([]), c = normalized([observation('nginx', null)]);
  assert.equal(a.technology_observations_present, false); assert.equal(b.technology_observations_present, true);
  assert.equal(map.productSummaryMarkup(a), '', 'a card shows products only when declared'); assert.equal(map.productSummaryMarkup(b), '');
  for (const item of [a, b, { captured: false, technology_observations: [] }]) {
    assert.equal(map.productMatches(item), true); assert.equal(map.productMatches(item, 'nginx'), false); assert.equal(map.productMatches(item, '', 'status:not_reported'), false);
  }
  assert.equal(map.productMatches(c, 'nginx', 'status:not_reported'), true); assert.equal(map.productMatches(c, 'nginx', 'multiple'), false);
});

test('only same-product distinct literal reported strings conflict; an unknown version stays in the tooltip, not on the chip', () => {
  const item = normalized([observation('php', '8.3'), observation('php', '8.3', 1, { header_name: 'x-powered-by' }), observation('nginx', '1.26')]);
  assert.equal(map.productMatches(item, '', 'multiple'), false); assert.equal(map.productMatches(item, 'nginx', 'reported:8.3'), false);
  assert.equal(map.productMatches(item, 'php', 'reported:8.3'), true);
  const mixed = normalized([observation('nginx', '1'), observation('nginx', null)]);
  assert.match(map.productSummaryMarkup(mixed), />nginx 1<\/span>/); assert.match(map.productSummaryMarkup(mixed), /Reported version: unknown/); assert.equal(map.productMatches(mixed, 'nginx', 'multiple'), false);
  const conflict = normalized([observation('nginx', '1'), observation('nginx', '01'), observation('nginx', '1.0'), observation('nginx', null)]);
  assert.equal(map.productMatches(conflict, 'nginx', 'multiple'), true); assert.match(map.productSummaryMarkup(conflict), />nginx · 3 versions<\/span>/);
  for (const v of ['1', '01', '1.0']) assert.equal(map.productMatches(conflict, 'nginx', `reported:${v}`), true);
  assert.equal(map.productMatches(conflict, 'nginx', 'reported:1.00'), false);
});

for (const [status, product] of [['not_reported', 'nginx'], ['unsupported_format', 'nginx'], ['non_product_version', 'apache_coyote']]) test(`unknown filter requires explicit ${status} observation`, () => {
  const item = normalized([observation(product, null, 1, { version_status: status })]);
  assert.equal(map.productMatches(item, product, `status:${status}`), true);
  // A version not found is not shown: the chip is the name alone.
  assert.match(map.productSummaryMarkup(item), />(nginx|Apache-Coyote)<\/span>/); assert.doesNotMatch(map.productSummaryMarkup(item), />[^<]*unknown[^<]*</);
  for (const other of ['not_reported', 'unsupported_format', 'non_product_version'].filter(s => s !== status)) assert.equal(map.productMatches(item, '', `status:${other}`), false);
});

test('summary is bounded, deterministic and preserves separate exact source tuples in escaped tooltips', () => {
  const observations = [observation('php', '8.3'), observation('express', null), observation('nginx', '00000000000000000000000000000001'), observation('php', '8.3', 1, { header_name: 'x-powered-by' })];
  const html = map.productSummaryMarkup(normalized(observations));
  assert.match(html, /\+1<\/span>/); assert.equal((html.match(/<span title=/g) || []).length, 2);
  assert.match(html, /class="service-map-products-more"[^>]*data-more="PHP 8\.3" title="">\+1</, '+N names what it stands for, with no second tooltip');
  assert.match(html, /server response header/); assert.match(html, /x-powered-by response header/); assert.match(html, /00000000000000000000000000000001/);
  assert.equal(html, map.productSummaryMarkup(normalized([...observations].reverse().sort((a, b) => a.header_name.localeCompare(b.header_name)))));
  const malicious = normalized([observation()]); malicious.technology_observations[0].name = '<img onerror="bad">';
  const escaped = map.productSummaryMarkup(malicious); assert.doesNotMatch(escaped, /<img|title="[^>]*"bad"/); assert.match(escaped, /&lt;img onerror=&quot;bad&quot;&gt;/);
});

test('v1 observations still work with the reduced coverage warning and no legacy clue parsing', async () => {
  const f = fixture(), value = snapshot(); delete value.technology_catalog_version; await f.load(value);
  f.filter('product', 'nginx'); assert.deepEqual(cards(f), [A]); assert.match(f.els.status.textContent, /Legacy server: catalog v1/);
  const missing = origin(); delete missing.technology_observations; missing.technology_clues = [{ name: 'nginx/1.26.2', source: 'response_header', confidence: 'observed_header', evidence: [] }];
  value.origins = [missing]; await f.load(value); assert.deepEqual(cards(f), []); assert.equal(f.state.productId, 'nginx');
  assert.match(f.els.productFilters.innerHTML, /nginx \(unavailable\)/); assert.match(f.els.productFilters.innerHTML, /zero matching origins/);
});

test('product conjunction, independent search options and disappeared selections never broaden filters', async () => {
  const f = fixture(); await f.load(snapshot([origin(A, [observation(), observation('php', '8.3')]), origin(B, [observation('php', '7.4', 2)], 1, 2)]));
  f.search('app'); const options = f.els.productFilters.innerHTML; f.search('none'); assert.equal(f.els.productFilters.innerHTML, options);
  f.search(''); f.filter('product', 'nginx'); f.filter('version', 'reported:8.3'); assert.deepEqual(cards(f), []);
  assert.match(f.els.productFilters.innerHTML, /Reported: 8.3 \(unavailable\)/); assert.match(f.els.productFilters.innerHTML, /zero matching origins/);
  f.filter('product', 'php'); assert.equal(f.state.productVersion, 'reported:8.3'); assert.deepEqual(cards(f), [A]);
  await f.load(snapshot([origin(B, [observation('nginx', '9', 2)], 1, 2)]));
  assert.equal(f.state.productId, 'php'); assert.equal(f.state.productVersion, 'reported:8.3'); assert.deepEqual(cards(f), []);
  assert.match(f.els.productFilters.innerHTML, /php" selected>PHP \(unavailable\)/);
  f.click({ serviceProductFilterClear: 'true' }); assert.equal(f.state.productId, ''); assert.equal(f.state.productVersion, ''); assert.deepEqual(cards(f), [B]); assert.equal(f.requests.length, 2);
});

test('all-origin and direct-neighbor counts partition query and products before the display cap; pin is context', async () => {
  const f = fixture(), main = origin(A, [observation('php', '8.3')]);
  const neighbors = Array.from({ length: 30 }, (_, i) => origin(`https://neighbor-${String(i).padStart(2, '0')}.example.com`, [observation(i < 27 ? 'nginx' : 'php', '1', i + 2)], 1, i + 2));
  main.redirects = [...neighbors.map(item => ({ target_origin: item.origin, target_path: '/', relationship: 'redirect_instruction', evidence: [] })), { target_origin: REF, target_path: '/', relationship: 'redirect_instruction', evidence: [] }];
  await f.load(snapshot([main, ...neighbors])); f.controller.selectOrigin(A); f.els.directRelations.dispatch('click'); f.filter('product', 'nginx');
  assert.equal(cards(f).length, 24); assert.equal(cards(f)[0], A); assert.match(f.els.graphDiagnostics.textContent, /31 loaded neighbors · 0 query-hidden · 4 product\/version-hidden · 27 matching · 23 shown · 4 omitted/);
  assert.match(f.els.graphBounds.textContent, /Pinned anchor remains visible outside product\/version filters/); assert.match(f.els.detail.innerHTML, /outside the product\/version filters/);
  f.search('neighbor-2'); assert.equal(cards(f).length, 8); assert.match(f.els.graphDiagnostics.textContent, /31 loaded neighbors · 21 query-hidden · 3 product\/version-hidden · 7 matching · 7 shown · 0 omitted/);
  f.els.allOrigins.dispatch('click'); assert.equal(cards(f).length, 7); assert.match(f.els.graphBounds.textContent, /7 \/ 7 matching origins · 22 hidden by query · 3 hidden by products\/versions/); assert.match(f.els.graphDiagnostics.textContent, /32 loaded origins · 22 query-hidden · 3 product\/version-hidden · 7 matching · 7 shown · 0 omitted/);
  assert.equal(f.requests.length, 1);
});

test('catalog membership is origin-level; selected endpoint inspector keeps independent filters and page', async () => {
  const f = fixture(); await f.load(snapshot([origin(A, [observation()], 130), origin(B, [observation('php', '8.3', 131)], 2, 131)]));
  f.controller.selectOrigin(A, 110); await f.verify(110); f.click({ serviceDetailTab: 'apis' }); f.click({ servicePage: '2' });
  const pageAnchor = plain(f.state.pageAnchor), evidenceBefore = f.state.evidence;
  f.filter('product', 'nginx'); assert.deepEqual(plain(f.state.pageAnchor), pageAnchor); assert.equal(f.state.page, 2); assert.equal(f.state.detailPage, 2);
  f.filter('product', 'php'); assert.equal(f.state.detailPage, 2); assert.equal(f.state.endpointSelection.path, '/path-110'); assert.equal(f.state.evidence, evidenceBefore);
  assert.equal(rows(f.els.detail).length, 30); assert.equal(rows(f.els.catalog).length, 2);
  assert.match(f.els.catalog.innerHTML, /2 matching · 132 loaded · 0 hidden by endpoint filters · 130 hidden by origin products\/versions/);
  f.search('app'); assert.equal(rows(f.els.catalog).length, 0); assert.match(f.els.catalog.innerHTML, /0 matching · 132 loaded · 0 hidden by endpoint filters · 130 hidden by origin products\/versions · 2 hidden by origin search/);
  f.els.panel.dispatch('input', { target: { value: '/path-11', dataset: { serviceEndpointFilter: 'path' } } }); assert.equal(rows(f.els.detail).length, 11);
  assert.equal(f.state.productId, 'php'); assert.equal(f.requests.length, 2);
});

for (const kind of ['evidence', 'body', 'link', 'html', 'peer']) test(`local filters preserve pending ${kind} read, source, mode and zoom with zero extra requests`, async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(A, 0); if (kind !== 'evidence') assert.equal(await f.verify(), true);
  if (kind === 'link') f.controller.setRelationshipKind('link');
  f.controller.setZoom(0.6); f.els.directRelations.dispatch('click');
  const run = kind === 'evidence' ? () => f.controller.loadEvidence(token()) : kind === 'body' ? () => f.controller.loadBodyReferences() : kind === 'link' ? () => f.controller.loadLinkDeclarations() : kind === 'html' ? () => f.controller.loadHtmlGenerators() : () => f.controller.loadPeerMatches();
  const task = run(), read = f.requests.at(-1), count = f.requests.length, sourceBefore = f.state.evidence, anchor = f.state.anchorOrigin;
  f.filter('product', 'php'); f.filter('version', 'status:unsupported_format');
  assert.equal(f.requests.length, count); assert.equal(read.options.signal.aborted, false); assert.equal(f.state.evidence, sourceBefore); assert.equal(f.state.anchorOrigin, anchor); assert.equal(f.state.zoomScale, 0.6); assert.equal(f.state.endpointSelection.path, '/path-0');
  read.resolve(response(({ evidence, body: bodyScan, link: linkScan, html: htmlScan, peer: peerScan })[kind]())); assert.equal(await task, true);
  assert.ok(f.state.evidence); assert.equal(f.state.productId, 'php'); assert.match(f.els.detail.innerHTML, /outside the product\/version filters/);
  assert.equal(f.requests.length, count); assert.equal(read.options.redirect, 'error');
});

for (const kind of ['body', 'link', 'peer']) test(`shown ${kind} visualization survives product changes and retains the exact source`, async () => {
  const f = fixture(); await f.load(snapshot([origin(), origin(B, [observation('php', '8.3', 2)], 1, 2)])); await f.verify();
  if (kind === 'link') f.controller.setRelationshipKind('link');
  const task = kind === 'body' ? f.controller.loadBodyReferences() : kind === 'link' ? f.controller.loadLinkDeclarations() : f.controller.loadPeerMatches();
  f.requests.at(-1).resolve(response(kind === 'body' ? bodyScan() : kind === 'link' ? linkScan() : peerScan())); assert.equal(await task, true);
  const show = kind === 'body' ? 'showBodyReferencesOnMap' : kind === 'link' ? 'showLinkDeclarationsOnMap' : 'showPeerMatchesOnMap'; assert.equal(f.controller[show](), true);
  const overlay = f.state.bodyReferenceOverlay, count = f.requests.length; f.filter('product', 'php');
  assert.equal(f.state.bodyReferenceOverlay, overlay); assert.equal(f.state.anchorOrigin, kind === 'peer' ? null : A); assert.equal(f.state.evidence.origin, A); assert.equal(f.requests.length, count);
  assert.deepEqual(cards(f), kind === 'peer' ? [B] : [A]);
  if (kind === 'peer') {
    assert.match(f.els.canvas.innerHTML, /PHP 8.3/); assert.doesNotMatch(f.els.canvas.innerHTML, /data-service-origin="https:\/\/peer-only/);
    assert.match(f.els.graphBounds.textContent, /2 hidden by products\/versions/);
    f.controller.selectOrigin(PEER); assert.match(f.els.detail.innerHTML, /outside the product\/version filters/);
    f.click({ serviceProductFilterClear: 'true' }); assert.match(f.els.canvas.innerHTML, /peer-only\.example\.com/);
  } else assert.match(f.els.graphDiagnostics.textContent, /1 loaded neighbors · 0 query-hidden · 1 product\/version-hidden · 0 matching/);
});

for (const kind of ['evidence', 'body', 'link', 'html', 'peer']) for (const transition of ['generation', 'session']) test(`${transition} fence remains authoritative for pending ${kind} after local filtering`, async () => {
  const f = fixture(); await f.load(); if (kind !== 'evidence') await f.verify(); else f.controller.selectOrigin(A, 0);
  if (kind === 'link') f.controller.setRelationshipKind('link');
  const task = kind === 'evidence' ? f.controller.loadEvidence(token()) : kind === 'body' ? f.controller.loadBodyReferences() : kind === 'link' ? f.controller.loadLinkDeclarations() : kind === 'html' ? f.controller.loadHtmlGenerators() : f.controller.loadPeerMatches();
  const pending = f.requests.at(-1); f.filter('product', 'nginx');
  if (transition === 'session') { f.setSession(RECORD); f.controller.reset(); assert.equal(f.state.productId, ''); }
  else {
    const value = snapshot([origin(A, [], 0)], { generation: REVISION }); await f.load(value); assert.equal(f.state.productId, 'nginx');
  }
  assert.equal(pending.options.signal.aborted, true); pending.resolve(response(({ evidence, body: bodyScan, link: linkScan, html: htmlScan, peer: peerScan })[kind]())); assert.equal(await task, false); assert.equal(f.state.evidence, null);
});

test('same-session refresh, invalidation and scope preserve filters while exact pages reconcile and session reset clears', async () => {
  const f = fixture(), value = snapshot([origin(A, [observation()], 130)]); await f.load(value);
  f.filter('product', 'nginx'); f.filter('version', 'reported:1.26.2'); f.controller.selectOrigin(A, 110); f.click({ servicePage: '2' });
  const invalidation = f.controller.invalidate(); assert.equal(f.state.page, 2); assert.equal(f.state.productId, 'nginx'); assert.equal(f.state.productVersion, 'reported:1.26.2');
  const reordered = clone(value); reordered.origins[0].endpoints.reverse(); f.requests.at(-1).resolve(response(reordered)); await invalidation;
  assert.equal(f.state.endpointIndex, 19); assert.equal(f.state.detailPage, 0); assert.equal(f.state.page, 0); assert.equal(f.state.pageAnchor.path, '/path-100');
  f.els.scope.dispatch('click'); assert.equal(f.state.productId, 'nginx'); assert.equal(f.state.productVersion, 'reported:1.26.2');
  f.requests.at(-1).resolve(response(value)); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.productId, 'nginx'); f.controller.reset(); assert.equal(f.state.productId, ''); assert.equal(f.state.productVersion, '');
});

test('loaded product controls are a collapsed bounded toolbar disclosure with unchanged card geometry', () => {
  const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8'), css = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
  assert.match(html, /<details class="service-map-product-filters"><summary>Products \/ versions<\/summary>/);
  assert.match(css, /\.service-map-product-filter-panel \{ position: absolute/); assert.match(css, /width: min\(560px, calc\(100% - 6px\)\)/);
  assert.match(css, /service-map-product-filter-controls select \{ width: 100%; min-width: 0; max-width: 100%/);
  for (const size of [8, 12, 20]) { const geometry = map.graphGeometry(size); assert.equal(geometry.cardWidth, 220); assert.ok(geometry.cardHeight >= 4 * size); }
});

test('catalog anchor hidden by products is retained across same-session refresh and recovered on Clear', async () => {
  const f = fixture(), value = snapshot([origin(A, [observation()], 130), origin(B, [observation('php', '8.3', 131)], 1, 131)]); await f.load(value);
  f.click({ servicePage: '2' }); assert.equal(f.state.pageAnchor.path, '/path-100');
  f.filter('product', 'php'); assert.equal(f.state.page, 0); assert.equal(f.state.pageAnchor.path, '/path-100');
  await f.load(value); assert.equal(f.state.page, 0); assert.equal(f.state.pageAnchor.path, '/path-100');
  f.click({ serviceProductFilterClear: 'true' }); assert.equal(f.state.page, 2); assert.equal(f.state.pageAnchor.path, '/path-100');
  f.filter('product', 'php'); f.click({ servicePage: '0' }); assert.equal(f.state.pageAnchor.origin, B);
  f.click({ serviceProductFilterClear: 'true' }); assert.equal(f.state.pageAnchor.origin, B); assert.equal(f.state.page, 2);
});

test('catalog rendering and page reconciliation consistently exclude returned out-of-scope origins', async () => {
  const f = fixture(), outside = origin(B, [observation('php', '8.3', 131)], 60, 131); outside.in_scope = false;
  const value = snapshot([outside, origin(A, [observation()], 130)]); await f.load(value);
  f.els.scope.dispatch('click'); f.requests.at(-1).resolve(response(value)); await new Promise(resolve => setImmediate(resolve));
  f.click({ servicePage: '2' }); assert.equal(f.state.pageAnchor.origin, A); assert.equal(f.state.pageAnchor.path, '/path-100');
  f.filter('product', 'nginx'); assert.equal(f.state.page, 2); assert.equal(rows(f.els.catalog).length, 30); assert.match(f.els.catalog.innerHTML, /130 loaded/);
  f.filter('product', 'php'); assert.equal(rows(f.els.catalog).length, 0); assert.equal(f.state.page, 0);
});

for (const kind of ['body', 'link', 'html', 'peer']) test(`same-snapshot poll preserves pending ${kind} inspection while product filter hides source`, async () => {
  const f = fixture(); await f.load(); await f.verify(); if (kind === 'link') f.controller.setRelationshipKind('link');
  const task = kind === 'body' ? f.controller.loadBodyReferences() : kind === 'link' ? f.controller.loadLinkDeclarations() : kind === 'html' ? f.controller.loadHtmlGenerators() : f.controller.loadPeerMatches();
  const pending = f.requests.at(-1); f.filter('product', 'php'); await f.load(); assert.equal(pending.options.signal.aborted, false);
  pending.resolve(response(({ body: bodyScan, link: linkScan, html: htmlScan, peer: peerScan })[kind]())); assert.equal(await task, true); assert.equal(f.state.productId, 'php'); assert.equal(f.requests.length, 4);
});

test('invalid local choices cannot create arbitrary products or oversized version options', async () => {
  const f = fixture(); await f.load();
  for (const [kind, value] of [['product', '<script>'], ['version', 'reported:<script>'], ['version', 'reported:' + '1'.repeat(33)], ['version', 'semver:>=1']]) f.filter(kind, value);
  assert.equal(f.state.productId, ''); assert.equal(f.state.productVersion, ''); assert.equal(f.requests.length, 1);
});

for (const [containerName, key] of [['productFilters', 'products:product'], ['productFilters', 'products:version'], ['catalog', 'serviceMapCatalog:method']]) {
  test(`native-normalized ${key} retains its exact node and open popup through unchanged polling`, async () => {
    const f = fixture(), container = f.els[containerName], view = nativeControlModel(f, container);
    const disclosure = element(); disclosure.open = true; disclosure.querySelector = () => null;
    f.els.productFilters.parentElement = disclosure;
    await f.load(); f.filter('product', 'nginx'); f.filter('version', 'reported:1.26.2');
    f.els.panel.dispatch('change', { target: { value: 'GET', dataset: { serviceEndpointFilter: 'method' } } });
    const control = view.controls().find(control => control.dataset.serviceFocus === key), count = view.replacements();
    control.popupOpen = true; control.focus(); container.scrollTop = 125;
    assert.match(container.innerHTML, /selected=""/);
    for (let index = 0; index < 3; index += 1) await f.load();
    assert.equal(view.replacements(), count); assert.equal(control.isConnected, true); assert.equal(control.popupOpen, true);
    assert.equal(f.document.activeElement, control); assert.equal(container.scrollTop, 125); assert.equal(disclosure.open, true);
    assert.equal(f.state.productId, 'nginx'); assert.equal(f.state.productVersion, 'reported:1.26.2'); assert.equal(f.state.endpointMethod, 'GET');
  });
}

test('native markup cache still commits filter changes, refreshes options and fences session reset', async () => {
  const f = fixture(), view = nativeControlModel(f, f.els.productFilters); await f.load();
  f.filter('product', 'nginx'); f.filter('version', 'reported:1.26.2');
  const selected = view.controls()[0], before = view.replacements(); selected.focus();
  f.filter('product', 'php');
  assert.ok(view.replacements() > before); assert.equal(selected.isConnected, false);
  assert.equal(f.document.activeElement, view.controls()[0]); assert.match(f.els.productFilters.innerHTML, /php" selected="">PHP \(unavailable\)/);
  await f.load(snapshot([origin(A, [observation('php', '8.3')])]));
  assert.match(f.els.productFilters.innerHTML, /php" selected="">PHP<\/option>/); assert.match(f.els.productFilters.innerHTML, /Reported: 8.3/);
  const pending = f.controller.refresh(), oldRequest = f.requests.at(-1);
  f.setSession('new-synthetic-session'); f.controller.reset();
  assert.equal(oldRequest.options.signal.aborted, true); assert.equal(f.state.productId, ''); assert.equal(f.state.productVersion, '');
  assert.doesNotMatch(f.els.productFilters.innerHTML, /<option value="php"|Reported: 8.3/);
  oldRequest.resolve(response(snapshot())); await pending;
  assert.equal(f.state.snapshot, null); assert.doesNotMatch(f.els.productFilters.innerHTML, /<option value="php"|Reported: 8.3/);
});

test('session changes detach old native controls even when generated filter markup is identical', async () => {
  const f = fixture(), view = nativeControlModel(f, f.els.productFilters);
  await f.load(snapshot([origin(A, [])]));
  const previous = view.controls()[0], html = f.els.productFilters.innerHTML;
  previous.popupOpen = true; previous.focus();
  f.setSession('new-synthetic-session'); f.controller.reset();
  assert.equal(f.els.productFilters.innerHTML, html);
  assert.equal(previous.isConnected, false); assert.equal(previous.popupOpen, false);
  assert.notEqual(view.controls()[0], previous); assert.equal(f.state.snapshot, null);
});

test('product and API controls use scoped theme colors, sizing, placeholders and keyboard focus', () => {
  const css = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
  const controls = css.match(/\.service-map-product-filter-controls select, \.service-map-endpoint-filter-controls input, \.service-map-endpoint-filter-controls select \{([^}]+)\}/)?.[1];
  assert.ok(controls);
  for (const rule of ['box-sizing: border-box', 'color: var(--text)', 'background-color: var(--panel-code)', 'border: 1px solid var(--line-strong)', 'font: inherit']) assert.ok(controls.includes(rule), rule);
  // Native appearance is disabled globally; its shared SVG chevron must survive
  // the themed background rather than being erased by a background shorthand.
  assert.match(css, /select:not\(\[multiple\]\) \{[^}]*background-image: url\("data:image\/svg\+xml/);
  assert.doesNotMatch(controls, /(?:^|;)\s*background\s*:|background-image\s*:/);
  assert.match(css, /service-map-product-filter-controls select:focus-visible[^}]+outline: 2px solid var\(--point\)/);
  assert.match(css, /service-map-endpoint-filter-controls input::placeholder \{ color: var\(--text-dim\)/);
});

test('a hidden catalog anchor is replaced only when its exact loaded endpoint disappears', async () => {
  const f = fixture(), value = snapshot([origin(A, [observation()], 130), origin(B, [observation('php', '8.3', 131)], 1, 131)]); await f.load(value);
  f.click({ servicePage: '2' }); f.filter('product', 'php'); assert.equal(f.state.pageAnchor.path, '/path-100');
  value.origins[0].endpoints = value.origins[0].endpoints.filter(item => item.path !== '/path-100'); await f.load(value);
  assert.equal(f.state.pageAnchor.origin, B); assert.equal(f.state.pageAnchor.path, '/path-0');
  f.click({ serviceProductFilterClear: 'true' }); assert.equal(f.state.pageAnchor.origin, B); assert.equal(f.state.page, 2);
});

test('peer source-origin extra matches remain separate per-inspection facts, without invented product metadata', async () => {
  const f = fixture(); await f.load(); await f.verify();
  const scan = peerScan(); scan.origins[0].origin = A; scan.origins.sort((a, b) => a.origin.localeCompare(b.origin));
  const task = f.controller.loadPeerMatches(); f.requests.at(-1).resolve(response(scan)); assert.equal(await task, true); assert.equal(f.controller.showPeerMatchesOnMap(), true);
  f.filter('product', 'php'); assert.deepEqual(cards(f), []);
  assert.match(f.els.detail.innerHTML, /1 other matching rows in this inspection/);
  assert.equal(f.state.relationLayers.length, 1);
});
