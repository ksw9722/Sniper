// Synthetic passive metadata and inert DOM; no bootstrap or proxy actions.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../web/service-map.js'), 'utf8');
const context = vm.createContext({ URL, URLSearchParams, AbortController, console });
vm.runInContext(source, context);
const map = context.SniperServiceMap;
const originA = 'https://app.example.com';
const originB = 'https://api.example.com';
const recordId = '11111111-1111-4111-8111-111111111111';
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function response(payload, status = 200) { return { ok: status < 400, status, json: async () => payload }; }
function endpoint(path = '/users', method = 'GET', id = 'evidence-one') { return { method, path, request_count: 2, statuses: [200], content_types: ['application/json'], evidence: [id] }; }
function origin(id, endpoints = [endpoint()]) { return { origin: id, in_scope: true, request_count: 2, endpoints, technology_clues: [], redirects: [], header_links: [] }; }
function snapshot(origins = [origin(originA), origin(originB)], session = 'session-one', extra = {}) {
  return { schema_version: 1, session_id: session, snapshot: { generation: 'generation-one', complete: true, stopped_by: [], limits: { records: 10000 }, considered_rows: 4, retained_rows: 4, included_rows: 4, excluded_rows: 0, omitted_rows: 0, oldest_considered_at: '2026-10-12T01:00:00Z', latest_considered_at: '2026-10-12T02:00:00Z', ...extra }, origins, unavailable: ['initiator', 'remote_ip', 'tls_certificate'], deferred: ['response_body_references'], warnings: [] };
}
function evidence(id = 'evidence-one', rowOrigin = originA, extra = {}) {
  return { schema_version: 1, session_id: 'session-one', evidence_id: id, available: true, source: 'captured_http_metadata', row: { evidence_id: id, origin: rowOrigin, method: 'GET', path: '/users', sequence: '9007199254740993', record_id: recordId, status: 200, started_at: '2026-10-12T01:00:00Z', metadata_complete: true, omitted: [] }, ...extra };
}
function node() {
  const handlers = new Map(), classes = new Set();
  let html = '', replacements = 0;
  return {
    value: '', checked: false, disabled: false, dataset: {}, attributes: {}, textContent: '', scrollTop: 0,
    classList: { toggle(name, value) { if (value) classes.add(name); else classes.delete(name); }, contains: name => classes.has(name) },
    setAttribute(name, value) { this.attributes[name] = value; },
    addEventListener(name, callback) { handlers.set(name, callback); },
    dispatch(name, event = {}) { return handlers.get(name)?.({ target: this, ...event }); },
    contains: () => true, querySelectorAll: () => [],
    get innerHTML() { return html; }, set innerHTML(value) { html = value; replacements += 1; },
    get replacements() { return replacements; },
  };
}
function fixture() {
  let session = 'session-one';
  const requests = [];
  const els = Object.fromEntries(['panel', 'status', 'refresh', 'search', 'scope', 'canvas', 'graphBounds', 'detail', 'content', 'catalog'].map(name => [name, node()]));
  const controller = map.createController({ elements: els, getSessionId: () => session, document: { activeElement: null, body: {}, querySelector: () => null }, request(url, options) { const pending = deferred(); requests.push({ url, options, ...pending }); return pending.promise; } });
  controller.setVisible(true);
  return { controller, els, requests, state: controller.getState(), setSession(value) { session = value; }, async load(payload = snapshot()) { const pending = controller.refresh(); requests.at(-1).resolve(response(payload)); await pending; } };
}

test('map is lazy: construction/hidden refresh cause no request', async () => {
  const f = fixture(); f.controller.setVisible(false); await f.controller.refresh();
  assert.equal(f.requests.length, 0); assert.equal(f.state.snapshot, null);
});
test('only passive local GET requests; map selection stays local and exact evidence uses opaque token', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA);
  assert.equal(f.requests.length, 1);
  const pending = f.controller.loadEvidence('evidence-one');
  assert.match(f.requests[1].url, /^\/api\/target\/service-map\/evidence\?/);
  assert.match(f.requests[1].url, /evidence_id=evidence-one/);
  assert.equal(f.requests[1].options.method, undefined);
  f.requests[1].resolve(response(evidence())); await pending;
  assert.equal(f.state.evidence.sequence, '9007199254740993');
  assert.match(f.els.detail.innerHTML, /Verified retained source/);
  assert.doesNotMatch(f.requests.map(request => request.url).join(' '), /\/api\/transactions\/|replay|scan|capture/);
});
test('selection survives endpoint insertion/reordering by case-sensitive method and exact path', async () => {
  const f = fixture(); await f.load(snapshot([origin(originA, [endpoint('/a'), endpoint('/z', 'get', 'z')])]));
  f.controller.selectOrigin(originA, 1);
  await f.load(snapshot([origin(originA, [endpoint('/new'), endpoint('/z', 'GET'), endpoint('/z', 'get', 'z'), endpoint('/a')])]));
  assert.equal(f.state.endpointIndex, 2);
  assert.match(f.els.detail.innerHTML, />get<\/span>/);
  await f.load(snapshot([origin(originA, [endpoint('/new'), endpoint('/z', 'GET')])]));
  assert.equal(f.state.endpointIndex, null); assert.equal(f.state.evidence, null);
});
test('removed selected origin clears selection without selecting a neighbor', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originB);
  await f.load(snapshot([origin(originA)]));
  assert.equal(f.state.selectedOrigin, null); assert.match(f.els.detail.innerHTML, /Select an origin/);
});
test('newer map request wins even if older request ignores abort and finishes last', async () => {
  const f = fixture(); const a = f.controller.refresh(); const b = f.controller.refresh({ force: true });
  f.requests[1].resolve(response(snapshot([origin(originB)]))); await b;
  f.requests[0].resolve(response(snapshot([origin(originA)]))); await a;
  assert.equal(f.state.snapshot.origins[0].origin, originB);
});
test('late map JSON parse cannot resurrect old session data', async () => {
  const f = fixture(), parse = deferred(); const pending = f.controller.refresh();
  f.requests[0].resolve({ ok: true, json: () => parse.promise }); await Promise.resolve();
  f.setSession('session-two'); f.controller.reset();
  parse.resolve(snapshot()); await pending;
  assert.equal(f.state.snapshot, null); assert.equal(f.state.selectedOrigin, null);
  assert.doesNotMatch(f.els.canvas.innerHTML, /app\.example/);
});
test('removal/gap invalidation clears counts and evidence immediately, rejects old map response', async () => {
  const f = fixture(); await f.load(); const old = f.controller.refresh();
  const current = f.controller.invalidate();
  assert.equal(f.state.snapshot, null); assert.doesNotMatch(f.els.status.textContent, /4 rows considered/);
  f.requests[1].resolve(response(snapshot())); await old;
  assert.equal(f.state.snapshot, null);
  f.requests[2].resolve(response(snapshot([]))); await current;
  assert.equal(f.state.snapshot.origins.length, 0);
});
test('live changes label stale counts and clear verified evidence; next stable read clears stale status', async () => {
  const f = fixture(); await f.load();
  f.controller.markDirty(); assert.match(f.els.status.textContent, /previous snapshot/);
  const pending = f.controller.refresh(); f.controller.markDirty();
  f.requests.at(-1).resolve(response(snapshot())); await pending;
  assert.equal(f.state.stale, true);
  await f.load(); assert.equal(f.state.stale, false);
});
test('scope filter forces new read; old-filter responses cannot replace it', async () => {
  const f = fixture(); const old = f.controller.refresh();
  f.els.scope.checked = true; f.els.scope.dispatch('change');
  assert.match(f.requests.at(-1).url, /in_scope_only=true/);
  f.requests[1].resolve(response(snapshot([origin(originB)]))); await new Promise(resolve => setImmediate(resolve));
  f.requests[0].resolve(response(snapshot([origin(originA)]))); await old;
  assert.equal(f.state.snapshot.origins[0].origin, originB);
});
test('late evidence after origin selection/session transition is discarded', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA);
  const pending = f.controller.loadEvidence('evidence-one'); f.controller.selectOrigin(originB);
  f.requests.at(-1).resolve(response(evidence())); await pending; assert.equal(f.state.evidence, null);
  const second = f.controller.loadEvidence('evidence-one'); f.setSession('session-two'); f.controller.reset();
  f.requests.at(-1).resolve(response(evidence('evidence-one', originB))); await second;
  assert.equal(f.state.evidence, null);
});
for (const [name, change] of [
  ['wrong schema', value => { value.schema_version = 2; }],
  ['wrong envelope token', value => { value.evidence_id = 'other'; }],
  ['wrong row token', value => { value.row.evidence_id = 'other'; }],
  ['wrong origin', value => { value.row.origin = originB; }],
  ['wrong method', value => { value.row.method = 'POST'; }],
  ['wrong path', value => { value.row.path = '/other'; }],
  ['wrong source type', value => { value.source = 'unverified'; }],
]) test(`evidence rejects ${name} without fallback to UUID detail`, async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA, 0);
  const pending = f.controller.loadEvidence('evidence-one'); const value = evidence(); change(value);
  f.requests.at(-1).resolve(response(value)); await pending;
  assert.equal(f.state.evidence, null); assert.match(f.els.detail.innerHTML, /Source unavailable/); assert.equal(f.requests.length, 2);
});
test('unavailable source is honest and repeated evidence clicks latest-win', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA);
  const a = f.controller.loadEvidence('evidence-one'), b = f.controller.loadEvidence('evidence-one');
  f.requests[2].resolve(response(evidence('evidence-one', originA, { available: false, row: null, reason: 'row_revision_changed' }))); await b;
  f.requests[1].resolve(response(evidence())); await a;
  assert.equal(f.state.evidence, null); assert.match(f.state.evidenceError, /Source unavailable/);
});
test('network errors keep prior map explicitly labelled; no payload text leaks', async () => {
  const f = fixture(); await f.load(); const pending = f.controller.refresh();
  f.requests.at(-1).reject(new Error('secret header body')); await pending;
  assert.match(f.els.status.textContent, /previous snapshot/); assert.match(f.els.status.textContent, /4 rows considered/);
  assert.doesNotMatch(f.els.status.textContent, /secret/); assert.equal(f.els.refresh.disabled, false);
});
test('incomplete bounds and latest window are visible, graph capped independently of snapshot', async () => {
  const f = fixture(); const origins = Array.from({ length: 80 }, (_, i) => origin(`https://node${i}.example.com`));
  await f.load(snapshot(origins, 'session-one', { complete: false, retained_rows: 20000, considered_rows: 10000, stopped_by: ['max_records'], omitted_rows: 2 }));
  assert.equal((f.els.canvas.innerHTML.match(/data-service-origin=/g) || []).length, 24);
  assert.match(f.els.graphBounds.textContent, /24 \/ 80/); assert.match(f.els.status.textContent, /max_records/);
  assert.match(f.els.status.textContent, /20,000 retained/); assert.match(f.els.status.textContent, /2026-10-12T01/);
  f.els.search.value = 'node79.'; f.els.search.dispatch('input'); assert.match(f.els.canvas.innerHTML, /node79/);
});
test('reference-only nodes carry no captured counts, no invented relationships; header direction is explicit', async () => {
  const f = fixture(), target = origin(originB); target.header_links = [{ source_origin: originA, relationship: 'origin_header', source: 'request_header_declaration', evidence: ['header'] }];
  await f.load(snapshot([target]));
  assert.match(f.els.canvas.innerHTML, /Referenced only in this snapshot/);
  assert.match(f.els.canvas.innerHTML, /app.example.com.*→.*api.example.com.*Origin header/s);
  f.controller.selectOrigin(originA); assert.match(f.els.detail.innerHTML, /No captured rows are shown/);
  assert.equal(f.requests.length, 1);
});
test('markup escapes captured strings and omits query/fragment/userinfo; no external links emitted', async () => {
  const f = fixture(), data = origin('https://user:password@example.com', [endpoint('/<img onerror="bad">?token=secret#frag', '<script>', 'x" onclick="bad')]);
  data.technology_clues = [{ name: '<img src=x onerror=bad>', source: 'response_header', confidence: 'observed_header', evidence: [] }];
  await f.load(snapshot([data]));
  const html = f.els.canvas.innerHTML + f.els.detail.innerHTML + f.els.catalog.innerHTML;
  assert.doesNotMatch(html, /<img|<script|token=secret|password|<a\b|href=/);
  assert.match(html, /&lt;img/); assert.match(html, /&lt;script&gt;/);
});
test('unchanged refresh preserves card DOM and stable order, with additions appended', async () => {
  const f = fixture(); await f.load(); const replacements = f.els.canvas.replacements;
  await f.load(); assert.equal(f.els.canvas.replacements, replacements);
  const order = [...f.state.order]; await f.load(snapshot([origin('https://aaa.example.com'), origin(originA), origin(originB)]));
  assert.deepEqual([...f.state.order].slice(0, 2), order);
});
test('API catalog and selected-origin lists are independently paginated at 50', async () => {
  const f = fixture(); await f.load(snapshot([origin(originA, Array.from({ length: 130 }, (_, i) => endpoint(`/path-${i}`)))]));
  assert.equal((f.els.catalog.innerHTML.match(/data-service-endpoint=/g) || []).length, 50);
  assert.equal((f.els.detail.innerHTML.match(/data-service-endpoint=/g) || []).length, 50);
});
test('graph layout has an explicit edge cap and only supported metadata relations', () => {
  const origins = Array.from({ length: 24 }, (_, i) => ({ ...origin(`https://node${i}.example.com`), captured: true }));
  origins.forEach(item => { item.redirects = origins.map(target => ({ target_origin: target.origin })); });
  const layout = map.graphLayout(origins, origins.map(item => item.origin));
  assert.equal(layout.edges.length, 40); assert.equal(layout.edgeCount, 24 * 23);
});

test('stable focus keys use exact origin/method/path rather than a mutable row index', async () => {
  const f = fixture(); await f.load(snapshot([origin(originA, [endpoint('/a'), endpoint('/z', 'get')])]));
  const before = f.els.catalog.innerHTML.match(/data-service-focus="([^"]+)"[^>]*><span class="service-map-method get">get/)[1];
  await f.load(snapshot([origin(originA, [endpoint('/new'), endpoint('/a'), endpoint('/z', 'get')])]));
  const after = f.els.catalog.innerHTML.match(/data-service-focus="([^"]+)"[^>]*><span class="service-map-method get">get/)[1];
  assert.equal(before, after); assert.match(after, /\/z/);
});
test('filtered/bounded referenced cards do not assert the origin was never captured', async () => {
  const f = fixture(), target = origin(originB); target.header_links = [{ source_origin: originA, relationship: 'referer_header', source: 'request_header_declaration', evidence: ['ref'] }];
  await f.load(snapshot([target], 'session-one', { complete: false, stopped_by: ['max_records'] }));
  f.controller.selectOrigin(originA);
  assert.match(f.els.detail.innerHTML, /Scope and projection bounds may omit captured history/);
  assert.doesNotMatch(f.els.detail.innerHTML + f.els.canvas.innerHTML, /not captured|uncaptured/i);
});
test('exact evidence includes source observations and any incomplete-metadata notice', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA);
  const pending = f.controller.loadEvidence('evidence-one'), value = evidence();
  Object.assign(value.row, { metadata_complete: false, omitted: ['header_limit'], header_links: [{ source_origin: originB, relationship: 'origin_header' }], redirect: { target_origin: originB, target_path: '/next?private=omitted' }, technology_clues: [{ name: 'nginx', header_name: 'server' }] });
  f.requests.at(-1).resolve(response(value)); await pending;
  assert.match(f.els.detail.innerHTML, /header_limit/); assert.match(f.els.detail.innerHTML, /Origin header:/);
  assert.match(f.els.detail.innerHTML, /Redirect instruction:/); assert.doesNotMatch(f.els.detail.innerHTML, /private=omitted/);
});

const { loadFunctions, appSource } = require('./frontend-test-helpers.cjs');
for (const view of ['service-map', 'apis', 'rules', 'paths']) test(`${view} loader only requests legacy paths when Paths is explicitly visible`, async () => {
  const requests = [], state = { activeTool: 'target', activeTargetView: view, targetScopeDraftVersion: 0, targetScopeSaveToken: 0, targetScopeDirty: false };
  const ctx = loadFunctions(['loadTargetSiteMap'], {
    state, currentSessionId: () => 'session-one', workspaceStateGeneration: 0,
    sessionQueryPath: (path, id) => `${path}?session_id=${id}`,
    fetch: async url => { requests.push(url); return response(url.startsWith('/api/runtime') ? {} : []); },
    requireOkResponse: async () => {}, jsonArray: value => value, syncTargetScopeDraft() {}, renderInterceptStatus() {}, renderProxySettings() {}, renderTarget() {},
  });
  await ctx.loadTargetSiteMap();
  assert.equal(requests.some(url => url.startsWith('/api/target/site-map')), view === 'paths');
});
test('map/APIs polling bypasses legacy runtime/path reads entirely', async () => {
  for (const view of ['service-map', 'apis']) {
    let refreshes = 0;
    const ctx = loadFunctions(['pollAuxiliaryData'], {
      state: { activeTool: 'target', activeTargetView: view },
      targetServiceMapController: { refresh: async () => { refreshes++; } },
      isWebsocketHistoryVisible: () => false, shouldPollFindingsBadge: () => false,
      loadTargetSiteMap() { assert.fail('Legacy path loader must not run in the bounded map'); },
    });
    await ctx.pollAuxiliaryData(); assert.equal(refreshes, 1);
  }
});
test('integration wires session, removal, gap and owned transaction invalidation', () => {
  assert.match(appSource, /function resetSessionScopedUiState\(\)[\s\S]*?targetServiceMapController\?\.reset\(\)/);
  assert.match(appSource, /function applyConfirmedHistoryRemoval\([^]*?targetServiceMapController\?\.invalidate\(\)/);
  assert.match(appSource, /function invalidateHistoryAfterEventGap\([^]*?targetServiceMapController\?\.invalidate\(\)/);
  assert.match(appSource, /addEventListener\("transaction", \(event\) => \{\s*if \(!ownsHistoryEvents\(\)\) \{\s*return;\s*\}\s*if \(typeof targetServiceMapController[^]*?markDirty\(\)/);
});

test('reference-card work is cached and capped independently of the 24-card viewport', async () => {
  const f = fixture(), target = origin(originB);
  target.header_links = Array.from({ length: 200 }, (_, i) => ({ source_origin: `https://source${i}.example.com`, relationship: 'origin_header', source: 'request_header_declaration', evidence: [] }));
  await f.load(snapshot([target]));
  assert.equal(f.state.graphOriginsCache.length, 101);
  assert.equal(f.state.omittedGraphReferences, 100);
  const cached = f.state.graphOriginsCache;
  f.controller.selectOrigin(originB); assert.equal(f.state.graphOriginsCache, cached);
  assert.match(f.els.graphBounds.textContent, /Reference-card cap 100/);
});

test('source-first layout puts declared sources left of destinations and preserves slots on refresh', async () => {
  const f = fixture(), app = origin(originA), api = origin(originB), auth = origin('https://auth.example.com');
  app.redirects = [{ target_origin: auth.origin, target_path: '/', relationship: 'redirect_instruction', evidence: ['evidence-one'] }];
  api.header_links = [{ source_origin: app.origin, relationship: 'origin_header', source: 'request_header_declaration', evidence: ['evidence-one'] }];
  await f.load(snapshot([api, app, auth]));
  assert.equal(f.state.layoutSlots.get(app.origin).column, 0);
  assert.equal(f.state.layoutSlots.get(api.origin).column, 1);
  assert.equal(f.state.layoutSlots.get(auth.origin).column, 1);
  const slots = JSON.stringify([...f.state.layoutSlots]);
  f.controller.selectOrigin(auth.origin); assert.equal(JSON.stringify([...f.state.layoutSlots]), slots);
  await f.load(snapshot([app, auth, api])); assert.equal(JSON.stringify([...f.state.layoutSlots]), slots);
});
test('individual relationship click reads the source token owned by the captured origin', async () => {
  const f = fixture(), target = origin(originB);
  target.header_links = [{ source_origin: originA, relationship: 'origin_header', source: 'request_header_declaration', evidence: ['declared-source'] }];
  await f.load(snapshot([target])); f.controller.selectOrigin(originA);
  assert.match(f.els.detail.innerHTML, /data-service-relation-evidence="declared-source"/);
  assert.match(f.els.detail.innerHTML, /Origin header declaration/);
  const button = { dataset: { serviceRelationOrigin: originB, serviceRelationEvidence: 'declared-source' } };
  f.els.panel.dispatch('click', { target: { closest: () => button } });
  assert.equal(f.state.selectedOrigin, originB); assert.equal(f.state.evidenceId, 'declared-source');
  f.requests.at(-1).resolve(response(evidence('declared-source', originB))); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.evidence.origin, originB);
});

for (const hidden of [false, true]) test(`a scope-rule save fences old map data even while ${hidden ? 'hidden' : 'a map read is running'}`, async () => {
  const f = fixture(); await f.load(); const oldMap = f.controller.refresh();
  if (hidden) f.controller.setVisible(false);
  const runtime = deferred();
  const appState = { runtime: { scope_patterns: ['*.example.com'], excluded_scope_patterns: [] }, targetScopeEditorSessionId: 'session-one', targetScopeDraft: 'api.example.com', targetScopeExcludeDraft: '', targetScopeDraftVersion: 0, targetScopeSaveToken: 0 };
  const ctx = loadFunctions(['saveTargetScope', 'formatScopePatternsText'], {
    state: appState, els: { targetScopeEditor: { value: 'api.example.com' }, targetScopeExcludeEditor: { value: '' } },
    currentSessionId: () => 'session-one', workspaceStateGeneration: 1, targetServiceMapController: f.controller,
    expectedActiveSessionIdForWrite: id => id, fetch: () => runtime.promise, showToast() {}, renderInterceptStatus() {}, renderProxySettings() {}, invalidateVisibleEntriesCache() {}, scheduleRefresh() {},
  });
  const save = ctx.saveTargetScope({ skipReload: true });
  runtime.resolve(response({ scope_patterns: ['api.example.com'], excluded_scope_patterns: [] }));
  await save;
  assert.equal(f.state.snapshot, null, 'old scope classifications clear immediately');
  assert.equal(f.requests[1].options.signal.aborted, true);
  f.requests[1].resolve(response(snapshot())); await oldMap;
  assert.equal(f.state.snapshot, null, 'old response cannot resurrect scope badges');
  if (hidden) { assert.equal(f.requests.length, 2); f.controller.setVisible(true); }
  const fresh = hidden ? f.controller.refresh() : null;
  const scoped = origin(originB); f.requests.at(-1).resolve(response(snapshot([scoped])));
  if (fresh) await fresh; else await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.snapshot.origins[0].origin, originB);
});
test('unchanged include/exclude rules do not invalidate an in-flight map', async () => {
  const f = fixture(); await f.load(); const pending = f.controller.refresh();
  assert.equal(f.controller.scopeChanged({ scope_patterns: ['example.com'] }, { scope_patterns: ['example.com'], excluded_scope_patterns: [], intercept_enabled: true }), false);
  assert.equal(f.requests.at(-1).options.signal.aborted, false);
  f.requests.at(-1).resolve(response(snapshot())); await pending;
  assert.notEqual(f.state.snapshot, null);
});
