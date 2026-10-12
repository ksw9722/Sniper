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
  const els = Object.fromEntries(['panel', 'status', 'refresh', 'search', 'scope', 'canvas', 'graphBounds', 'detail', 'content', 'catalog', 'mapControls', 'allOrigins', 'directRelations', 'focusSelection', 'boundsDetails', 'graphDiagnostics'].map(name => [name, node()]));
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
  ['numeric sequence without exact decimal string', value => { value.row.sequence = 9007199254740993; }],
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
  assert.match(boundsText(f), /24 \/ 80/); assert.match(f.els.status.textContent, /max_records/);
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
  const before = f.els.catalog.innerHTML.match(/data-service-focus="([^"]+)"[^>]*><span class="service-map-method get"[^>]*>get/)[1];
  await f.load(snapshot([origin(originA, [endpoint('/new'), endpoint('/a'), endpoint('/z', 'get')])]));
  const after = f.els.catalog.innerHTML.match(/data-service-focus="([^"]+)"[^>]*><span class="service-map-method get"[^>]*>get/)[1];
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
  assert.match(boundsText(f), /Reference-card cap 100/);
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

for (const transition of ['scope filter', 'history invalidation']) test(`${transition} preserves exact endpoint identity through an empty snapshot`, async () => {
  const f = fixture();
  await f.load(snapshot([origin(originA, [endpoint('/first'), endpoint('/selected', 'get', 'selected')])]));
  f.controller.selectOrigin(originA, 1);
  const oldEvidence = f.controller.loadEvidence('selected');
  let refresh;
  if (transition === 'scope filter') {
    f.els.scope.checked = true;
    f.els.scope.dispatch('change');
  } else refresh = f.controller.invalidate();
  assert.equal(f.state.snapshot, null);
  assert.equal(f.state.evidence, null);
  f.requests[1].resolve(response(evidence('selected', originA, { row: { ...evidence().row, evidence_id: 'selected', method: 'get', path: '/selected' } })));
  await oldEvidence;
  assert.equal(f.state.evidence, null, 'old source cannot return during the invalidated snapshot');
  f.requests.at(-1).resolve(response(snapshot([origin(originA, [endpoint('/selected', 'GET'), endpoint('/other'), endpoint('/selected', 'get', 'selected')])])));
  if (refresh) await refresh; else await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.endpointIndex, 2);
  assert.match(f.els.detail.innerHTML, /service-map-endpoint-summary[^]*?>get<\/span><strong>\/selected<\/strong>/);
  assert.equal(f.state.evidence, null, 'a refreshed selection does not claim a newly verified source');
});

test('invalidating a removed endpoint clears its selection instead of selecting the replacement index', async () => {
  const f = fixture(); await f.load(snapshot([origin(originA, [endpoint('/selected')])]));
  f.controller.selectOrigin(originA, 0);
  const refresh = f.controller.invalidate();
  f.requests.at(-1).resolve(response(snapshot([origin(originA, [endpoint('/replacement')])])));
  await refresh;
  assert.equal(f.state.endpointIndex, null);
  assert.doesNotMatch(f.els.detail.innerHTML, /service-map-endpoint-summary/);
});

test('origin search keeps selected exact evidence and explains when its card is filtered out', async () => {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA, 0);
  const pending = f.controller.loadEvidence('evidence-one');
  f.requests.at(-1).resolve(response(evidence())); await pending;
  const verified = f.state.evidence;
  for (const query of ['api.example.com', 'no-matching.example.com']) {
    f.els.search.value = query; f.els.search.dispatch('input');
    assert.equal(f.state.selectedOrigin, originA);
    assert.equal(f.state.evidence, verified);
    assert.match(f.els.detail.innerHTML, /Verified retained source/);
    assert.match(boundsText(f), /Selected origin is outside this filter/);
  }
  f.els.search.value = ''; f.els.search.dispatch('input');
  assert.doesNotMatch(boundsText(f), /outside this filter/);
  assert.equal(f.requests.length, 2, 'origin search never fetches new metadata');
});

test('selection beyond the card cap remains inspectable and explains the display limit', async () => {
  const f = fixture(), origins = Array.from({ length: 30 }, (_, i) => origin(`https://node${String(i).padStart(2, '0')}.example.com`));
  await f.load(snapshot(origins));
  f.controller.selectOrigin(origins.at(-1).origin, 0);
  assert.equal((f.els.canvas.innerHTML.match(/data-service-origin=/g) || []).length, 24);
  assert.match(boundsText(f), /Selected origin is outside the 24-card display/);
  assert.match(f.els.detail.innerHTML, /node29.example.com/);
});

test('redirect relationships distinguish exact target paths and have distinct stable focus keys', async () => {
  const f = fixture(), source = origin(originA);
  source.redirects = ['/first?secret=omitted', '/second'].map((target_path, i) => ({ target_origin: originB, target_path, relationship: 'redirect_instruction', evidence: [`redirect-${i}`] }));
  await f.load(snapshot([source]));
  f.controller.selectOrigin(originA);
  assert.match(f.els.detail.innerHTML, /api.example.com\/first/);
  assert.match(f.els.detail.innerHTML, /api.example.com\/second/);
  assert.doesNotMatch(f.els.detail.innerHTML, /secret=omitted/);
  const keys = [...f.els.detail.innerHTML.matchAll(/data-service-focus="(relation:[^"]+)"/g)].map(match => match[1]);
  assert.equal(keys.length, 2);
  assert.equal(new Set(keys).size, 2);
});

test('long captured and reference-only origins retain full identities behind bounded header previews', async () => {
  const host = `${['a'.repeat(50), 'b'.repeat(50), 'c'.repeat(50), 'd'.repeat(50)].join('.')}.example.com`;
  const fullOrigin = `https://${host}`;
  for (const captured of [true, false]) {
    const f = fixture(), owner = origin(captured ? fullOrigin : originA);
    if (!captured) owner.header_links = [{ source_origin: fullOrigin, relationship: 'origin_header', source: 'request_header_declaration', evidence: [] }];
    await f.load(snapshot([owner])); f.controller.selectOrigin(fullOrigin);
    assert.ok(f.els.detail.innerHTML.includes(`<h3 title="${host}">${host}</h3>`));
    assert.ok(f.els.detail.innerHTML.includes(`<p title="${fullOrigin}">${fullOrigin}</p>`));
  }
});

test('bounded API previews preserve full escaped method and exact path in text and titles', async () => {
  const f = fixture(), method = 'EXPERIMENTAL_METHOD_WITH_32_CHARS!'.slice(0, 32);
  assert.equal(method.length, 32);
  const exactPath = `/catalog/${'long-segment-'.repeat(22)}"quoted"&<safe>`;
  const escapedPath = exactPath.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  await f.load(snapshot([origin(originA, [endpoint(exactPath, method)])]));
  for (const html of [f.els.detail.innerHTML, f.els.catalog.innerHTML]) {
    assert.ok(html.includes(`title="${method}">${method}</span>`));
    assert.ok(html.includes(`<span title="${escapedPath}">${escapedPath}</span>`));
    assert.doesNotMatch(html, /<safe>|title="[^"]*"quoted"/);
  }
  f.controller.selectOrigin(originA, 0);
  assert.ok(f.els.detail.innerHTML.includes(`<strong>${escapedPath}</strong>`), 'the selected endpoint body keeps the complete path');
});

test('map preview styles bound headers and long method/path rows without removing their text', () => {
  const styles = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
  const header = styles.match(/\.service-map-detail-header h3\s*\{([^}]+)\}/)[1];
  assert.match(header, /-webkit-line-clamp:\s*2/);
  assert.match(header, /max-height:\s*2\.6em/);
  const method = styles.match(/\.service-map-api-row \.service-map-method\s*\{([^}]+)\}/)[1];
  assert.match(method, /max-width:\s*min\(8em,\s*32%\)/);
  const pathPreview = styles.match(/\.service-map-api-copy > span\s*\{([^}]+)\}/)[1];
  assert.match(pathPreview, /-webkit-line-clamp:\s*2/);
  assert.match(pathPreview, /max-height:\s*2\.8em/);
  assert.match(styles, /\.service-map-detail-header p, \.service-map-api-copy > small\s*\{[^}]*text-overflow:\s*ellipsis;[^}]*white-space:\s*nowrap/);
});

function redirect(target, target_path = '/', id = 'evidence-one') { return { target_origin: target, target_path, relationship: 'redirect_instruction', evidence: [id] }; }
function header(source, relationship = 'origin_header', id = 'evidence-one') { return { source_origin: source, relationship, source: 'request_header_declaration', evidence: [id] }; }
function boundsText(f) { return `${f.els.graphBounds.textContent} ${f.els.graphDiagnostics.textContent}`; }
function cardIds(f) { return [...f.els.canvas.innerHTML.matchAll(/data-service-origin="([^"]+)"/g)].map(match => match[1]); }
function pin(f, id) { f.controller.selectOrigin(id); f.els.directRelations.dispatch('click'); }

test('All origins is default; direct controls are local, optional and hidden in standalone APIs', async () => {
  const f = fixture();
  assert.equal(f.els.allOrigins.attributes['aria-pressed'], 'true');
  assert.equal(f.els.directRelations.disabled, true);
  f.els.directRelations.dispatch('click'); assert.equal(f.state.anchorOrigin, null);
  await f.load(); pin(f, originA);
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(f.els.directRelations.attributes['aria-pressed'], 'true');
  const catalog = f.els.catalog.innerHTML;
  f.controller.setVisible(true, 'apis');
  assert.equal(f.els.mapControls.classList.contains('hidden'), true);
  assert.equal(f.els.catalog.innerHTML, catalog);
  f.controller.setVisible(true, 'service-map');
  assert.equal(f.els.mapControls.classList.contains('hidden'), false);
  f.els.allOrigins.dispatch('click');
  assert.equal(f.state.anchorOrigin, null);
  assert.deepEqual(new Set(cardIds(f)), new Set([originA, originB]));
  assert.equal(f.requests.length, 1);
});

test('direct mode pins beyond-card-cap anchors and never expands transitively', async () => {
  const f = fixture(), rows = Array.from({ length: 30 }, (_, i) => origin(`https://node${String(i).padStart(2, '0')}.example.com`));
  rows[29].redirects = [redirect(rows[0].origin)];
  rows[0].redirects = [redirect(rows[1].origin)];
  await f.load(snapshot(rows)); pin(f, rows[29].origin);
  assert.deepEqual(new Set(cardIds(f)), new Set([rows[29].origin, rows[0].origin]));
  assert.match(f.els.canvas.innerHTML, /pinned anchor/);
  assert.match(boundsText(f), /1 known direct neighbors/);
  assert.equal(f.requests.length, 1);
});

test('card and relationship selections never move a pinned anchor; Focus selection preserves exact evidence', async () => {
  const f = fixture(), app = origin(originA), api = origin(originB);
  app.redirects = [redirect(originB)]; await f.load(snapshot([app, api])); pin(f, originA);
  const slots = JSON.stringify([...f.state.layoutSlots]);
  f.els.panel.dispatch('click', { target: { closest: () => ({ dataset: { serviceOrigin: originB } }) } });
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(JSON.stringify([...f.state.layoutSlots]), slots);
  f.els.directRelations.dispatch('click'); assert.equal(f.state.anchorOrigin, originA, 'active mode does not repin');
  f.controller.selectOrigin(originB, 0);
  const pending = f.controller.loadEvidence('evidence-one');
  f.requests.at(-1).resolve(response(evidence('evidence-one', originB))); await pending;
  const selectedEvidence = f.state.evidence, endpointIdentity = f.state.endpointSelection, detail = f.els.detail.innerHTML;
  f.els.focusSelection.dispatch('click');
  assert.equal(f.state.anchorOrigin, originB);
  assert.equal(f.state.endpointIndex, 0);
  assert.equal(f.state.endpointSelection, endpointIdentity);
  assert.equal(f.state.evidence, selectedEvidence);
  assert.equal(f.els.detail.innerHTML, detail);
  assert.equal(f.state.detailTab, 'evidence');
  f.els.allOrigins.dispatch('click');
  assert.equal(f.state.evidence, selectedEvidence);
  assert.equal(f.state.endpointSelection, endpointIdentity);
  assert.equal(f.requests.length, 2, 'only map load and explicit source verification fetch');
});

test('search filters direct neighbors without hiding the anchor, with separate query/card omissions', async () => {
  const f = fixture(), app = origin(originA);
  app.redirects = Array.from({ length: 30 }, (_, i) => redirect(`https://neighbor${String(i).padStart(2, '0')}.example.com`));
  await f.load(snapshot([app])); pin(f, originA);
  assert.equal(cardIds(f).length, 24);
  assert.equal(cardIds(f)[0], originA);
  assert.match(boundsText(f), /30 known direct neighbors/);
  assert.match(boundsText(f), /23 shown · 0 query-hidden · 7 omitted by 24-card cap/);
  f.els.search.value = 'neighbor2'; f.els.search.dispatch('input');
  assert.equal(cardIds(f).length, 11);
  assert.equal(cardIds(f)[0], originA);
  assert.match(boundsText(f), /30 known direct neighbors/);
  assert.match(boundsText(f), /10 shown · 20 query-hidden · 0 omitted/);
  assert.match(boundsText(f), /Pinned anchor is outside the search query/);
  f.els.search.value = 'missing'; f.els.search.dispatch('input');
  assert.deepEqual(cardIds(f), [originA]);
  assert.match(boundsText(f), /0 shown · 30 query-hidden/);
  assert.doesNotMatch(boundsText(f), /No direct neighbors appear/);
  assert.equal(f.requests.length, 1);
});

test('in-contract direct neighbors remain available beyond the global 100 and pane 30 caps', async () => {
  const f = fixture(), app = origin(originA), budgetOwner = origin(originB);
  budgetOwner.redirects = Array.from({ length: 100 }, (_, i) => redirect(`https://reference${String(i).padStart(3, '0')}.example.com`, `/target-${i}`));
  app.redirects = Array.from({ length: 100 }, (_, i) => redirect(`https://reference${String(i + 100).padStart(3, '0')}.example.com`, `/target-${i + 100}`));
  await f.load(snapshot([budgetOwner, app])); pin(f, originA);
  assert.equal(f.state.graphOriginsCache.length, 102);
  assert.equal(f.state.relationsByOrigin.get(originA).items.length, 30);
  assert.match(boundsText(f), /100 known direct neighbors/);
  assert.match(boundsText(f), /23 shown · 0 query-hidden · 77 omitted/);
  f.els.search.value = 'reference199'; f.els.search.dispatch('input');
  const extra = 'https://reference199.example.com';
  assert.deepEqual(cardIds(f), [originA, extra]);
  f.controller.selectOrigin(extra);
  assert.match(f.els.detail.innerHTML, /Referenced only in this snapshot/);
  assert.match(f.els.detail.innerHTML, /reference199.example.com\/target-199/);
  assert.equal(f.state.anchorOrigin, originA);
  f.els.focusSelection.dispatch('click'); assert.equal(f.state.anchorOrigin, extra);
  assert.match(boundsText(f), /Reference-only anchor/);
  assert.match(boundsText(f), /1 known direct neighbors/);
  f.els.allOrigins.dispatch('click');
  assert.equal(f.state.selectedOrigin, extra);
  assert.match(f.els.detail.innerHTML, /reference199.example.com/);
  assert.match(boundsText(f), /100-reference-card limit/);
  f.els.search.value = ''; f.els.search.dispatch('input');
  await f.load(snapshot([budgetOwner, app]));
  assert.equal(f.state.selectedOrigin, extra, 'refresh retains a still-loaded reference beyond the All origins budget');
  assert.match(boundsText(f), /100-reference-card limit/);
});

test('direct arrows include only anchor-incident metadata, count distinct type/directions and exclude self links', async () => {
  const f = fixture(), a = origin(originA), b = origin(originB), c = origin('https://third.example.com');
  a.redirects = [redirect(originB, '/one'), redirect(originB, '/two'), redirect(originA)];
  b.redirects = [redirect(c.origin)];
  b.header_links = [header(originA), header(originA, 'referer_header')];
  a.header_links = [header(c.origin, 'referer_header')];
  await f.load(snapshot([a, b, c])); pin(f, originA);
  assert.match(boundsText(f), /2 known direct neighbors/);
  assert.match(boundsText(f), /Non-self distinct typed links: 1 incoming \/ 3 outgoing · 1 redirect \/ 1 Origin \/ 2 Referer/);
  assert.match(boundsText(f), /4 \/ 4 arrows between displayed cards \(cap 40\)/);
  assert.doesNotMatch(f.els.canvas.innerHTML, /api.example.com → https:\/\/third.example.com/);
  assert.match(f.els.detail.innerHTML, /api.example.com\/one/);
  assert.match(f.els.detail.innerHTML, /api.example.com\/two/);
});

test('direct arrow cap reports visible-card eligible links separately from off-card links', async () => {
  const f = fixture(), app = origin(originA), peers = Array.from({ length: 30 }, (_, i) => origin(`https://peer${i}.example.com`));
  peers.forEach(peer => { peer.header_links = [header(originA), header(originA, 'referer_header')]; app.redirects.push(redirect(peer.origin)); });
  await f.load(snapshot([app, ...peers])); pin(f, originA);
  assert.match(boundsText(f), /40 \/ 69 arrows between displayed cards \(cap 40\) · 21 links outside displayed cards/);
  assert.equal((f.els.canvas.innerHTML.match(/marker-end=/g) || []).length, 40);
});

test('reference-only anchors preserve metadata direction and exact captured-owner evidence routing', async () => {
  const f = fixture(), api = origin(originB); api.header_links = [header(originA, 'referer_header', 'source')];
  await f.load(snapshot([api])); pin(f, originA);
  assert.match(boundsText(f), /0 incoming \/ 1 outgoing/);
  assert.match(boundsText(f), /Reference-only anchor/);
  const button = { dataset: { serviceRelationOrigin: originB, serviceRelationEvidence: 'source' } };
  f.els.panel.dispatch('click', { target: { closest: () => button } });
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(f.state.selectedOrigin, originB);
  assert.equal(f.state.evidenceId, 'source');
  f.requests.at(-1).resolve(response(evidence('source', originB))); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.evidence.origin, originB);
});

test('inspector notices distinguish outside-neighborhood, search, card cap and reference cap', async () => {
  const f = fixture(), app = origin(originA), unrelated = origin('https://unrelated.example.com');
  app.redirects = Array.from({ length: 30 }, (_, i) => redirect(`https://peer${String(i).padStart(2, '0')}.example.com`));
  await f.load(snapshot([app, unrelated])); pin(f, originA);
  f.controller.selectOrigin(unrelated.origin, 0);
  assert.match(boundsText(f), /outside this direct neighborhood/);
  assert.doesNotMatch(boundsText(f), /outside this filter/);
  f.controller.selectOrigin('https://peer29.example.com');
  assert.match(boundsText(f), /outside the 24-card display/);
  f.els.search.value = 'peer00'; f.els.search.dispatch('input');
  assert.match(boundsText(f), /outside this filter/);
  f.els.focusSelection.dispatch('click');
  assert.equal(f.state.anchorOrigin, 'https://peer29.example.com');
  assert.doesNotMatch(boundsText(f), /Selected origin is outside/);
});

test('no-neighbor and incomplete snapshots retain the pinned origin with bounded claims', async () => {
  const f = fixture(); await f.load(snapshot([origin(originA)], 'session-one', { complete: false, stopped_by: ['max_records'] })); pin(f, originA);
  assert.deepEqual(cardIds(f), [originA]);
  assert.match(boundsText(f), /0 known direct neighbors/);
  assert.match(boundsText(f), /No direct neighbors appear in loaded metadata/);
  assert.match(boundsText(f), /Known neighborhood is incomplete/);
  assert.doesNotMatch(boundsText(f), /complete architecture|service calls observed/);
});

for (const transition of ['history', 'scope']) test(`direct anchor survives transient ${transition} invalidation and exact endpoint identity survives reorder`, async () => {
  const f = fixture(), app = origin(originA, [endpoint('/first'), endpoint('/selected', 'get', 'selected')]);
  app.redirects = [redirect(originB)]; await f.load(snapshot([app, origin(originB)])); pin(f, originA);
  f.controller.selectOrigin(originA, 1);
  const oldEvidence = f.controller.loadEvidence('selected'), oldMap = f.controller.refresh();
  let current;
  if (transition === 'history') current = f.controller.invalidate();
  else { f.els.scope.checked = true; f.els.scope.dispatch('change'); }
  assert.equal(f.state.snapshot, null);
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(f.state.neighborhood, null);
  assert.doesNotMatch(boundsText(f), /known direct neighbors/);
  f.requests[1].resolve(response(evidence('selected'))); await oldEvidence;
  f.requests[2].resolve(response(snapshot())); await oldMap;
  assert.equal(f.state.snapshot, null);
  const fresh = origin(originA, [endpoint('/selected', 'GET'), endpoint('/selected', 'get', 'selected')]);
  fresh.redirects = [redirect(originB)]; fresh.in_scope = false;
  f.requests.at(-1).resolve(response(snapshot([fresh, origin(originB)])));
  if (current) await current; else await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(f.state.endpointIndex, 1);
  assert.equal(f.state.endpointSelection.method, 'get');
  assert.equal(f.state.evidence, null);
  assert.match(f.els.detail.innerHTML, /Out of scope/);
  assert.equal(cardIds(f)[0], originA, 'anchor visibility is independent of new classification');
});

test('missing anchors clear mode only on current successful snapshot; stale responses cannot resurrect it', async () => {
  const f = fixture(); await f.load(); pin(f, originA);
  const old = f.controller.refresh(), current = f.controller.invalidate();
  assert.equal(f.state.anchorOrigin, originA);
  f.requests.at(-1).resolve(response(snapshot([origin(originB)]))); await current;
  assert.equal(f.state.anchorOrigin, null);
  assert.equal(f.els.allOrigins.attributes['aria-pressed'], 'true');
  assert.match(boundsText(f), /pinned origin is absent/);
  f.requests[1].resolve(response(snapshot())); await old;
  assert.equal(f.state.anchorOrigin, null);
  assert.doesNotMatch(f.els.canvas.innerHTML, /app.example.com/);
});

test('session reset clears direct mode immediately and pending parse cannot restore it', async () => {
  const f = fixture(); await f.load(); pin(f, originA);
  const parse = deferred(), pending = f.controller.refresh();
  f.requests.at(-1).resolve({ ok: true, json: () => parse.promise }); await Promise.resolve();
  f.setSession('session-two'); f.controller.reset();
  assert.equal(f.state.anchorOrigin, null);
  assert.equal(f.state.neighborhood, null);
  parse.resolve(snapshot()); await pending;
  assert.equal(f.state.snapshot, null);
  assert.equal(f.state.anchorOrigin, null);
});

for (const action of ['directRelations', 'focusSelection']) test(`${action} rejects a session switch before a UI reset without fetching`, async () => {
  const f = fixture(); await f.load(); if (action === 'focusSelection') pin(f, originA);
  f.setSession('session-two'); f.els[action].dispatch('click');
  assert.equal(f.state.anchorOrigin, null);
  assert.equal(f.state.snapshot, null);
  assert.equal(f.requests.length, 1);
});

test('dirty and refresh-failure states keep the stable anchor and label previous data without re-verifying evidence', async () => {
  const f = fixture(), app = origin(originA); app.redirects = [redirect(originB)];
  await f.load(snapshot([app, origin(originB)])); pin(f, originA);
  const slots = JSON.stringify([...f.state.layoutSlots]); f.controller.markDirty();
  assert.equal(f.state.anchorOrigin, originA);
  assert.match(f.els.status.textContent, /previous snapshot/);
  const pending = f.controller.refresh(); f.requests.at(-1).reject(new Error('fixture failure')); await pending;
  assert.equal(f.state.anchorOrigin, originA);
  assert.equal(JSON.stringify([...f.state.layoutSlots]), slots);
  assert.match(f.els.status.textContent, /Refresh failed/);
  assert.equal(f.requests.length, 2);
});

test('direct mode changes preserve an in-flight exact source read and its identity', async () => {
  const f = fixture(), app = origin(originA); app.redirects = [redirect(originB)];
  await f.load(snapshot([app, origin(originB)]));
  f.controller.selectOrigin(originA, 0);
  const pending = f.controller.loadEvidence('evidence-one'), sourceRequest = f.requests.at(-1);
  f.els.directRelations.dispatch('click');
  f.els.allOrigins.dispatch('click');
  f.els.directRelations.dispatch('click');
  assert.equal(sourceRequest.options.signal.aborted, false);
  sourceRequest.resolve(response(evidence())); await pending;
  assert.equal(f.state.evidence.origin, originA);
  assert.equal(f.state.endpointSelection.path, '/users');
  assert.equal(f.state.endpointIndex, 0);
  assert.equal(f.requests.length, 2);
});

test('Focus selection can repin while exact evidence is loading without aborting the read', async () => {
  const f = fixture(), app = origin(originA); app.redirects = [redirect(originB)];
  await f.load(snapshot([app, origin(originB)])); pin(f, originA);
  f.controller.selectOrigin(originB, 0);
  const pending = f.controller.loadEvidence('evidence-one'), sourceRequest = f.requests.at(-1);
  f.els.focusSelection.dispatch('click');
  assert.equal(sourceRequest.options.signal.aborted, false);
  sourceRequest.resolve(response(evidence('evidence-one', originB))); await pending;
  assert.equal(f.state.anchorOrigin, originB);
  assert.equal(f.state.evidence.origin, originB);
  assert.equal(f.requests.length, 2);
});

test('direct neighborhood cache ignores inspector changes and rebuilds from fresh metadata while keeping slots', async () => {
  const f = fixture(), app = origin(originA), api = origin(originB); app.redirects = [redirect(originB)];
  await f.load(snapshot([app, api])); pin(f, originA);
  const cached = f.state.neighborhood, slots = JSON.stringify([...f.state.layoutSlots]);
  f.controller.selectOrigin(originB);
  assert.equal(f.state.neighborhood, cached);
  f.controller.selectOrigin(originA);
  const replacements = f.els.canvas.replacements;
  await f.load(snapshot([api, app]));
  assert.notEqual(f.state.neighborhood, cached);
  assert.equal(JSON.stringify([...f.state.layoutSlots]), slots);
  assert.equal(f.els.canvas.replacements, replacements);
  const updated = origin(originA); updated.redirects = [redirect('https://changed.example.com')];
  await f.load(snapshot([updated, api]));
  assert.equal(f.state.anchorOrigin, originA);
  assert.deepEqual(new Set(cardIds(f)), new Set([originA, 'https://changed.example.com']));
  assert.doesNotMatch(f.els.canvas.innerHTML, /api.example.com/);
});

test('a current empty snapshot clears anchor mode and its count notices', async () => {
  const f = fixture(); await f.load(); pin(f, originA);
  await f.load(snapshot([]));
  assert.equal(f.state.anchorOrigin, null);
  assert.equal(f.state.neighborhood, null);
  assert.equal(f.els.directRelations.disabled, true);
  assert.match(f.els.canvas.innerHTML, /No retained HTTP origins/);
  assert.match(boundsText(f), /pinned origin is absent/);
  assert.doesNotMatch(boundsText(f), /known direct neighbors/);
});

test('standalone APIs remain query-filtered across pinning, including unrelated origins', async () => {
  const f = fixture(), app = origin(originA), unrelated = origin('https://unrelated.example.com');
  app.redirects = [redirect(originB)];
  await f.load(snapshot([app, origin(originB), unrelated])); pin(f, originA);
  f.els.search.value = 'unrelated'; f.els.search.dispatch('input');
  f.controller.setVisible(true, 'apis');
  assert.match(f.els.catalog.innerHTML, /unrelated.example.com/);
  assert.doesNotMatch(f.els.catalog.innerHTML, /app.example.com|api.example.com/);
  const button = { dataset: { serviceOrigin: unrelated.origin, serviceEndpoint: '0' } };
  f.els.panel.dispatch('click', { target: { closest: () => button } });
  assert.equal(f.state.view, 'service-map');
  assert.equal(f.state.selectedOrigin, unrelated.origin);
  assert.equal(f.state.anchorOrigin, originA);
  assert.match(boundsText(f), /outside this direct neighborhood/);
  assert.equal(f.requests.length, 1);
});

test('routine projection advisories do not imply an incomplete direct neighborhood', async () => {
  const f = fixture(), data = snapshot([origin(originA)]);
  data.warnings = ['Retained rows only.', 'Header declarations are not initiator proof.', 'Technology clues are not verified architecture.'];
  await f.load(data); pin(f, originA);
  assert.match(f.els.status.textContent, /3 metadata notice/);
  assert.match(boundsText(f), /Loaded metadata only; no transitive links or confirmed calls/);
  assert.doesNotMatch(boundsText(f), /Known neighborhood is incomplete/);
  data.snapshot.complete = false; data.snapshot.stopped_by = ['max_records'];
  await f.load(data);
  assert.match(boundsText(f), /Known neighborhood is incomplete/);
});

test('direct primary summary stays compact while native disclosure retains all count and limit details', async () => {
  const f = fixture(), app = origin(originA); app.redirects = [redirect(originB)];
  await f.load(snapshot([app, origin(originB)])); pin(f, originA);
  assert.equal(f.els.boundsDetails.classList.contains('hidden'), false);
  assert.match(f.els.graphBounds.textContent, /^1 \/ 1 known direct neighbors shown · 0 incoming \/ 1 outgoing links$/);
  assert.doesNotMatch(f.els.graphBounds.textContent, /Non-self|typed|0 query-hidden|0 omitted|cap 40/);
  assert.match(f.els.graphDiagnostics.textContent, /1 redirect \/ 0 Origin \/ 0 Referer/);
  assert.match(f.els.graphDiagnostics.textContent, /1 \/ 1 arrows between displayed cards \(cap 40\)/);
  assert.match(f.els.graphDiagnostics.textContent, /self-links are excluded/);
  f.els.allOrigins.dispatch('click');
  assert.equal(f.els.boundsDetails.classList.contains('hidden'), true);
  assert.equal(f.els.graphDiagnostics.textContent, '');
  const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
  assert.match(html, /<details[^>]*id="serviceMapBoundsDetails"><summary>Relation details<\/summary><p id="serviceMapGraphDiagnostics"><\/p><\/details>/);
});

test('primary direct summary keeps meaningful omissions visible without opening details', async () => {
  const f = fixture(), app = origin(originA), peers = Array.from({ length: 30 }, (_, i) => origin(`https://peer${String(i).padStart(2, '0')}.example.com`));
  peers.forEach(peer => { peer.header_links = [header(originA), header(originA, 'referer_header')]; app.redirects.push(redirect(peer.origin)); });
  await f.load(snapshot([app, ...peers])); pin(f, originA);
  assert.match(f.els.graphBounds.textContent, /7 omitted by 24-card cap/);
  assert.match(f.els.graphBounds.textContent, /29 arrows omitted by 40-arrow cap/);
  f.els.search.value = 'peer2'; f.els.search.dispatch('input');
  assert.match(f.els.graphBounds.textContent, /20 hidden by query/);
  assert.match(f.els.graphBounds.textContent, /Pinned anchor is outside the search query/);
  assert.doesNotMatch(f.els.graphBounds.textContent, /\b0 omitted|\b0 hidden/);
});
