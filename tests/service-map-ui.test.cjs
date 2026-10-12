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
  const document = { activeElement: null, body: {}, querySelector: () => null };
  const controller = map.createController({ elements: els, getSessionId: () => session, document, request(url, options) { const pending = deferred(); requests.push({ url, options, ...pending }); return pending.promise; } });
  controller.setVisible(true);
  return { controller, els, requests, document, state: controller.getState(), setSession(value) { session = value; }, async load(payload = snapshot()) { const pending = controller.refresh(); requests.at(-1).resolve(response(payload)); await pending; } };
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

function bodyReferences(overrides = {}) {
  return { schema_version: 1, session_id: 'session-one', evidence_id: 'evidence-one', available: true,
    status: 'scanned', complete: true, reasons: [], source_origin: originA, source_sequence: '9007199254740993',
    source_started_at: '2026-10-12T01:00:00Z', retained_preview_bytes: 300, scanned_bytes: 300,
    capture_preview_truncated: false, scan_prefix_truncated: false, candidates_scanned: 2,
    omitted_reference_occurrences: 0, references: [{ target_origin: 'https://assets.example.com', occurrences: 2, preview_byte_offsets: [24, 148], omitted_offsets: 0 }],
    limits: { ...map.BODY_REFERENCE_LIMITS }, coverage: 'literal_absolute_http_urls', offset_basis: 'retained_utf8_preview', ...overrides };
}
async function verifiedBodyFixture() {
  const f = fixture(); await f.load(); f.controller.selectOrigin(originA, 0);
  const pending = f.controller.loadEvidence('evidence-one'); f.requests.at(-1).resolve(response(evidence())); await pending;
  return f;
}
async function resolveBodyReferences(f, payload = bodyReferences()) {
  const pending = f.controller.loadBodyReferences(); f.requests.at(-1).resolve(response(payload)); return pending;
}

test('body inspection is opt-in: map, source verification, tabs, filters and refresh never request bodies', async () => {
  const f = fixture(); await f.controller.loadBodyReferences(); assert.equal(f.requests.length, 0);
  await f.load(); await f.controller.loadBodyReferences(); assert.equal(f.requests.length, 1);
  assert.doesNotMatch(f.els.detail.innerHTML, /data-service-body-references/);
  f.controller.selectOrigin(originA, 0);
  const sourceRead = f.controller.loadEvidence('evidence-one');
  assert.doesNotMatch(f.els.detail.innerHTML, /data-service-body-references/);
  f.requests.at(-1).resolve(response(evidence())); await sourceRead;
  assert.match(f.els.detail.innerHTML, /Inspect retained body references/);
  assert.match(f.els.detail.innerHTML, /Active session only/);
  f.els.search.value = 'app'; f.els.search.dispatch('input');
  f.controller.setVisible(false); f.controller.setVisible(true); await f.load();
  assert.equal(f.requests.length, 3);
  assert.ok(f.requests.every(item => !item.url.includes('/body-references')));
  f.els.panel.dispatch('click', { target: { closest: () => ({ dataset: { serviceBodyReferences: 'inspect' } }) } });
  assert.equal(f.requests.length, 4);
  const read = f.requests.at(-1), url = new URL(read.url, 'http://localhost');
  assert.equal(url.pathname, '/api/target/service-map/body-references');
  assert.deepEqual(Object.fromEntries(url.searchParams), { session_id: 'session-one', evidence_id: 'evidence-one' });
  assert.equal(read.options.method, undefined);
  read.resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.bodyReferences.status, 'scanned');
});

test('body references remain supplemental plaintext evidence with exact UTF-8 preview offsets', async () => {
  const f = await verifiedBodyFixture(), graph = f.els.canvas.innerHTML, graphBounds = boundsText(f);
  const payload = bodyReferences({ raw_body: 'private synthetic content', response: { body_preview: 'not for the inspector' } });
  await resolveBodyReferences(f, payload);
  assert.match(f.els.detail.innerHTML, /2 literal URL occurrences/);
  assert.match(f.els.detail.innerHTML, /https:\/\/assets\.example\.com/);
  assert.match(f.els.detail.innerHTML, /Preview byte offsets: 24, 148/);
  assert.match(f.els.detail.innerHTML, /Complete retained-preview scan/);
  assert.match(f.els.detail.innerHTML, /zero-based bytes relative to the retained UTF-8 preview/);
  assert.match(f.els.detail.innerHTML, /does not prove a request, call, or service ownership/);
  assert.doesNotMatch(f.els.detail.innerHTML, /href=|src=|private synthetic|not for the inspector/);
  assert.doesNotMatch(JSON.stringify(f.state.bodyReferences), /raw_body|response|private synthetic/);
  assert.equal(f.els.canvas.innerHTML, graph); assert.equal(boundsText(f), graphBounds);
  assert.equal(f.requests.length, 3);
});

test('body inspection deduplicates loading clicks and retries only on an explicit request', async () => {
  const f = await verifiedBodyFixture(); const first = f.controller.loadBodyReferences();
  assert.equal(f.state.bodyReferencesLoading, true);
  assert.match(f.els.detail.innerHTML, /data-service-body-references="inspect"[^>]*disabled/);
  await f.controller.loadBodyReferences(); await f.controller.loadBodyReferences();
  assert.equal(f.requests.length, 3);
  f.requests.at(-1).reject(new Error('private synthetic body')); await first;
  assert.match(f.els.detail.innerHTML, /Could not verify retained body references/);
  assert.match(f.els.detail.innerHTML, /Retry retained body references/);
  assert.doesNotMatch(f.els.detail.innerHTML, /private synthetic body/);
  f.els.search.dispatch('input'); f.controller.setVisible(true);
  assert.equal(f.requests.length, 3);
  await resolveBodyReferences(f); assert.equal(f.requests.length, 4);
  assert.equal(f.state.bodyReferencesError, '');
});

for (const [name, change] of [
  ['wrong schema', p => { p.schema_version = 2; }],
  ['wrong session', p => { p.session_id = 'session-two'; }],
  ['wrong exact token', p => { p.evidence_id = 'other-evidence'; }],
  ['wrong source origin', p => { p.source_origin = originB; }],
  ['missing source origin', p => { delete p.source_origin; }],
  ['wrong exact sequence', p => { p.source_sequence = '9007199254740992'; }],
  ['numeric sequence', p => { p.source_sequence = 9007199254740993; }],
  ['changed capture time', p => { p.source_started_at = '2026-10-12T02:00:00Z'; }],
  ['unknown status', p => { p.status = 'complete'; }],
  ['wrong available flag', p => { p.available = false; }],
  ['wrong complete flag', p => { p.complete = false; }],
  ['wrong coverage', p => { p.coverage = 'inferred_urls'; }],
  ['wrong offset basis', p => { p.offset_basis = 'raw_response'; }],
  ['noncanonical target with credentials', p => { p.references[0].target_origin = 'https://user:pass@assets.example.com'; }],
  ['noncanonical target path', p => { p.references[0].target_origin = 'https://assets.example.com/path?private=value'; }],
  ['noncanonical target case', p => { p.references[0].target_origin = 'https://ASSETS.example.com'; }],
  ['non-HTTP target', p => { p.references[0].target_origin = 'javascript:alert(1)'; }],
  ['duplicate origins', p => { p.references.push({ ...p.references[0] }); }],
  ['too many origins', p => { p.references = Array.from({ length: 101 }, (_, i) => ({ ...p.references[0], target_origin: `https://r${i}.example.com` })); }],
  ['too many offsets', p => { p.references[0].preview_byte_offsets = [1, 2, 3, 4, 5, 6]; }],
  ['negative offset', p => { p.references[0].preview_byte_offsets[0] = -1; }],
  ['out-of-range offset', p => { p.references[0].preview_byte_offsets[1] = 300; }],
  ['unordered offsets', p => { p.references[0].preview_byte_offsets = [148, 24]; }],
  ['unsafe occurrence count', p => { p.references[0].occurrences = Number.MAX_SAFE_INTEGER + 1; }],
  ['wrong omitted offset count', p => { p.references[0].omitted_offsets = 1; }],
  ['wrong scan limit', p => { p.limits.scan_bytes += 1; }],
  ['oversized scan', p => { p.scanned_bytes = 262145; }],
  ['unsafe retained preview count', p => { p.retained_preview_bytes = Number.MAX_SAFE_INTEGER + 1; }],
  ['scan beyond retained preview', p => { p.retained_preview_bytes = 299; }],
  ['too many candidates', p => { p.candidates_scanned = 4097; }],
  ['too many omitted references', p => { p.omitted_reference_occurrences = 100; }],
  ['reference count exceeds candidates', p => { p.candidates_scanned = 1; }],
  ['raw content in reason', p => { p.reasons = ['https://private.example.com/raw']; }],
  ['truncated scan called complete', p => { p.capture_preview_truncated = true; }],
  ['partial without reason', p => { p.status = 'partial'; p.complete = false; }],
  ['unsupported with references', p => { p.status = 'unsupported'; p.available = false; p.complete = false; }],
]) test(`retained body-reference validation rejects ${name}`, async () => {
  const f = await verifiedBodyFixture(), p = bodyReferences(); change(p);
  await resolveBodyReferences(f, p);
  assert.equal(f.state.bodyReferences, null);
  assert.match(f.state.bodyReferencesError, /Could not verify/);
  assert.doesNotMatch(f.els.detail.innerHTML, /user:pass|private\.example|alert\(1\)/);
  assert.equal(f.requests.length, 3);
});

for (const [name, change] of [
  ['origin selection', f => f.controller.selectOrigin(originB)],
  ['endpoint selection', f => f.controller.selectOrigin(originA)],
  ['another evidence request', f => f.controller.loadEvidence('evidence-one')],
  ['session reset', f => { f.setSession('session-two'); f.controller.reset(); }],
  ['unannounced session transition', f => f.setSession('session-two')],
  ['history changes', f => f.controller.markDirty()],
  ['retention invalidation', f => f.controller.invalidate()],
  ['scope change', f => { f.els.scope.checked = true; f.els.scope.dispatch('change'); }],
  ['map refresh', f => f.controller.refresh({ force: true })],
  ['hidden map', f => f.controller.setVisible(false)],
]) test(`late body-reference responses cannot publish after ${name}`, async () => {
  const f = await verifiedBodyFixture(), parsing = deferred();
  const pending = f.controller.loadBodyReferences(), request = f.requests.at(-1);
  request.resolve({ ok: true, json: () => parsing.promise }); await Promise.resolve();
  change(f); parsing.resolve(bodyReferences()); await pending;
  assert.equal(f.state.bodyReferences, null);
  assert.doesNotMatch(f.els.detail.innerHTML, /assets\.example\.com/);
  if (name !== 'unannounced session transition') assert.equal(request.options.signal.aborted, true);
});

test('previous body-reference result clears immediately when exact source is invalidated', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  f.controller.markDirty();
  assert.equal(f.state.bodyReferences, null); assert.equal(f.state.evidence, null);
  assert.doesNotMatch(f.els.detail.innerHTML, /data-service-body-references|assets\.example\.com/);
});

test('a newer body-reference read wins even if the cancelled read finishes last', async () => {
  const f = await verifiedBodyFixture(), first = f.controller.loadBodyReferences(), old = f.requests.at(-1);
  const sourceRead = f.controller.loadEvidence('evidence-one'); f.requests.at(-1).resolve(response(evidence())); await sourceRead;
  await resolveBodyReferences(f, bodyReferences({ candidates_scanned: 0, references: [] }));
  old.resolve(response(bodyReferences())); await first;
  assert.equal(f.state.bodyReferences.references.length, 0);
  assert.match(f.els.detail.innerHTML, /No literal absolute HTTP\(S\) URL references found/);
});

for (const status of ['unsupported', 'unavailable']) test(`${status} retained previews are distinct from a successful zero-reference scan`, async () => {
  const f = await verifiedBodyFixture();
  const p = bodyReferences({ status, available: false, complete: false, reasons: [status === 'unsupported' ? 'unsupported_content_type' : 'inactive_session'],
    references: [], scanned_bytes: 0, candidates_scanned: 0 });
  delete p.retained_preview_bytes; delete p.source_origin; delete p.source_sequence; delete p.source_started_at;
  await resolveBodyReferences(f, p);
  assert.equal(f.state.bodyReferences.status, status);
  assert.match(f.els.detail.innerHTML, new RegExp(`Retained preview is ${status}`));
  assert.doesNotMatch(f.els.detail.innerHTML, /No literal absolute HTTP\(S\) URL references found|Complete retained-preview scan/);
});

test('partial output exposes truncation, omissions, long origins and full independent limits', async () => {
  const f = await verifiedBodyFixture(), longOrigin = `https://${['a'.repeat(50), 'b'.repeat(50), 'c'.repeat(50), 'd'.repeat(50)].join('.')}.example.com`;
  const refs = Array.from({ length: 100 }, (_, i) => ({ target_origin: i ? `https://reference${i}.example.com` : longOrigin, occurrences: 7, preview_byte_offsets: [1, 25, 55, 80, 140], omitted_offsets: 2 }));
  await resolveBodyReferences(f, bodyReferences({ status: 'partial', complete: false,
    reasons: ['capture_preview_truncated', 'scan_prefix_truncated', 'candidate_limit', 'origin_limit', 'offset_limit'],
    retained_preview_bytes: 300000, scanned_bytes: 262144, candidates_scanned: 4096, omitted_reference_occurrences: 30,
    capture_preview_truncated: true, scan_prefix_truncated: true, references: refs }));
  assert.equal(f.state.bodyReferences.references.length, 100);
  assert.match(f.els.detail.innerHTML, /100 referenced origins · 700 literal URL occurrences/);
  assert.match(f.els.detail.innerHTML, /Partial retained-preview scan/);
  assert.match(f.els.detail.innerHTML, /30 reference occurrence\(s\) omitted/);
  assert.match(f.els.detail.innerHTML, /2 more offset\(s\) omitted/);
  assert.match(f.els.detail.innerHTML, /Scan: 262,144 bytes · Serialized row read: 2,097,152 bytes · Origins: 100 · URL candidates: 4,096 · Candidate: 4,096 bytes · Offsets per origin: 5/);
  assert.ok(f.els.detail.innerHTML.includes(longOrigin));
  assert.equal((f.els.detail.innerHTML.match(/class="service-map-body-reference-origin"/g) || []).length, 100);
  assert.equal(f.state.snapshot.origins.length, 2);
  const styles = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
  assert.match(styles, /\.service-map-body-reference-list \{[^}]*max-height: 300px;[^}]*overflow: auto;/);
  assert.match(styles, /\.service-map-body-reference-list span \{[^}]*overflow-wrap: anywhere;/);
});


test('resident previews can exceed the serialized disk-read ceiling while scanning remains bounded', async () => {
  const f = await verifiedBodyFixture();
  await resolveBodyReferences(f, bodyReferences({ status: 'partial', complete: false, reasons: ['scan_prefix_truncated'],
    retained_preview_bytes: 3000000, scanned_bytes: 262144, scan_prefix_truncated: true }));
  assert.equal(f.state.bodyReferences.retained_preview_bytes, 3000000);
  assert.match(f.els.detail.innerHTML, /262,144 \/ 3,000,000 UTF-8 preview bytes inspected/);
});

test('canonical omitted references cannot exceed the inspected candidate budget', async () => {
  const f = await verifiedBodyFixture();
  await resolveBodyReferences(f, bodyReferences({ status: 'partial', complete: false, reasons: ['origin_limit'],
    retained_preview_bytes: 100000, scanned_bytes: 100000, candidates_scanned: 4096, omitted_reference_occurrences: 4095 }));
  assert.equal(f.state.bodyReferences, null);
  assert.match(f.state.bodyReferencesError, /Could not verify/);
});

function pollRealTargetMap(f) {
  return loadFunctions(['pollAuxiliaryData'], {
    state: { activeTool: 'target', activeTargetView: 'service-map' }, targetServiceMapController: f.controller,
    isWebsocketHistoryVisible: () => false, shouldPollFindingsBadge: () => false,
    loadTargetSiteMap() { assert.fail('No legacy map read'); },
  }).pollAuxiliaryData();
}
function clickBodyInspector(f) {
  f.els.panel.dispatch('click', { target: { closest: () => ({ dataset: { serviceBodyReferences: 'inspect' } }) } });
}
test('real app background polling preserves body results produced by the delegated Inspect click', async () => {
  const f = await verifiedBodyFixture(); clickBodyInspector(f);
  f.requests.at(-1).resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.bodyReferences.status, 'scanned');
  const replacements = f.els.detail.replacements;
  for (let i = 0; i < 4; i++) {
    const poll = pollRealTargetMap(f);
    f.requests.at(-1).resolve(response(snapshot())); await poll;
    assert.equal(f.state.bodyReferences?.status, 'scanned');
    assert.equal(f.els.detail.replacements, replacements);
  }
  assert.match(f.els.detail.innerHTML, /https:\/\/assets\.example\.com/);
  assert.equal(f.requests.filter(item => item.url.includes('/body-references')).length, 1);
});


test('delegated Inspect during a background poll stays enabled and can finish after repeated unchanged polls', async () => {
  const f = await verifiedBodyFixture(); const firstPoll = pollRealTargetMap(f), mapRead = f.requests.at(-1);
  assert.equal(f.state.loading, true);
  assert.doesNotMatch(f.els.detail.innerHTML, /data-service-body-references="inspect"[^>]*disabled/);
  clickBodyInspector(f); const bodyRead = f.requests.at(-1);
  assert.match(bodyRead.url, /body-references/); assert.equal(f.state.bodyReferencesLoading, true);
  mapRead.resolve(response(snapshot())); await firstPoll;
  for (let i = 0; i < 3; i++) {
    const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(snapshot())); await poll;
    assert.equal(bodyRead.options.signal.aborted, false); assert.equal(f.state.bodyReferencesLoading, true);
  }
  bodyRead.resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.bodyReferences.status, 'scanned');
  assert.equal(f.requests.filter(item => item.url.includes('/body-references')).length, 1);
});

test('explicit force refresh clears a completed inspection without starting a body read', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  const refresh = f.controller.refresh({ force: true });
  assert.equal(f.state.bodyReferences, null); assert.doesNotMatch(f.els.detail.innerHTML, /assets\.example\.com/);
  f.requests.at(-1).resolve(response(snapshot())); await refresh;
  assert.equal(f.requests.filter(item => item.url.includes('/body-references')).length, 1);
});

for (const change of ['generation', 'exact token']) test(`background poll changing ${change} clears the source and fences pending body inspection`, async () => {
  const f = await verifiedBodyFixture(); clickBodyInspector(f); const bodyRead = f.requests.at(-1);
  const value = change === 'generation' ? snapshot(undefined, 'session-one', { generation: 'generation-two' })
    : snapshot([origin(originA, [endpoint('/users', 'GET', 'new-exact-token')]), origin(originB)]);
  const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(value)); await poll;
  assert.equal(f.state.evidence, null); assert.equal(bodyRead.options.signal.aborted, true);
  bodyRead.resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.bodyReferences, null); assert.doesNotMatch(f.els.detail.innerHTML, /assets\.example\.com/);
});

test('newly re-verified exact source can be inspected while only the map counts remain stale', async () => {
  const f = await verifiedBodyFixture(); f.controller.markDirty();
  assert.equal(f.state.evidence, null); assert.equal(f.state.stale, true);
  const verify = f.controller.loadEvidence('evidence-one'); f.requests.at(-1).resolve(response(evidence())); await verify;
  clickBodyInspector(f); const bodyRead = f.requests.at(-1);
  assert.match(bodyRead.url, /body-references/);
  bodyRead.resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.bodyReferences.status, 'scanned');
  assert.equal(f.state.stale, true);
});

test('native Coverage open attribute survives unchanged polling without replacing inspector DOM', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  const nativeOpenHtml = f.els.detail.innerHTML.replace('<details class="service-map-body-reference-limits">', '<details class="service-map-body-reference-limits" open="">');
  f.els.detail.innerHTML = nativeOpenHtml;
  const replacements = f.els.detail.replacements;
  for (let i = 0; i < 3; i++) {
    const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(snapshot())); await poll;
    assert.equal(f.els.detail.innerHTML, nativeOpenHtml);
    assert.equal(f.els.detail.replacements, replacements);
  }
});

// Model native DOM-owned state separately from generated markup. Replacing
// innerHTML creates fresh elements and loses focus/scroll/open, as a browser does.
function nativeInspectorModel(f) {
  const descriptor = Object.getOwnPropertyDescriptor(f.els.detail, 'innerHTML');
  const fresh = html => ({
    body: html.includes('service-map-detail-body') ? { scrollTop: 0 } : null,
    list: html.includes('class="service-map-body-reference-list"') ? { scrollTop: 0 } : null,
    limits: html.includes('class="service-map-body-reference-limits"') ? { open: false } : null,
    summary: html.includes('data-service-focus="body-references:limits"') ? {
      dataset: { serviceFocus: 'body-references:limits' }, isConnected: true,
      getClientRects: () => [{}], focus() { f.document.activeElement = this; },
    } : null,
  });
  let current = fresh(f.els.detail.innerHTML);
  f.els.detail.querySelector = selector => current[{
    '.service-map-detail-body': 'body', '.service-map-body-reference-list': 'list', '.service-map-body-reference-limits': 'limits',
  }[selector]] || null;
  f.els.detail.querySelectorAll = () => current.summary ? [current.summary] : [];
  Object.defineProperty(f.els.detail, 'innerHTML', {
    get: descriptor.get,
    set(html) {
      if (current.summary) current.summary.isConnected = false;
      if (f.document.activeElement === current.summary) f.document.activeElement = f.document.body;
      descriptor.set.call(this, html); current = fresh(html);
    },
  });
  return () => current;
}

test('unchanged polls preserve nested scroll and native-normalized loading markup', async () => {
  const f = await verifiedBodyFixture(); clickBodyInspector(f); const bodyRead = f.requests.at(-1);
  const view = nativeInspectorModel(f); view().limits.open = true; view().body.scrollTop = 240;
  // Boolean attributes and insignificant spaces normalize when assigned to DOM.
  f.els.detail.innerHTML = f.els.detail.innerHTML.replace(/ disabled>/g, ' disabled="">').replace(/inspect" >/g, 'inspect">');
  view().limits.open = true; view().body.scrollTop = 240;
  const before = f.els.detail.replacements;
  const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(snapshot())); await poll;
  assert.equal(f.els.detail.replacements, before);
  assert.equal(view().limits.open, true); assert.equal(view().body.scrollTop, 240);
  bodyRead.resolve(response(bodyReferences())); await new Promise(resolve => setImmediate(resolve));
  assert.equal(view().limits.open, true); assert.equal(view().body.scrollTop, 240);
});

test('same-source output changes retain Coverage keyboard focus, disclosure and both scroll positions', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  const view = nativeInspectorModel(f), previous = view();
  previous.limits.open = true; previous.body.scrollTop = 350; previous.list.scrollTop = 110;
  f.document.activeElement = previous.summary;
  const data = snapshot(); data.origins[0].endpoints[0].request_count = 3;
  const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(data)); await poll;
  assert.notEqual(view(), previous);
  assert.equal(view().limits.open, true);
  assert.equal(view().body.scrollTop, 350); assert.equal(view().list.scrollTop, 110);
  assert.equal(f.document.activeElement, view().summary);
  assert.match(f.els.detail.innerHTML, /data-service-focus="body-references:limits"/);
  assert.match(f.els.detail.innerHTML, /3 sampled rows/);
});

test('source invalidation resets Coverage and nested scroll instead of transferring native state', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  const view = nativeInspectorModel(f);
  view().limits.open = true; view().list.scrollTop = 110; view().body.scrollTop = 350;
  f.controller.markDirty(); assert.equal(view().limits, null);
  const verify = f.controller.loadEvidence('evidence-one'); f.requests.at(-1).resolve(response(evidence())); await verify;
  assert.equal(view().limits.open, false); assert.equal(view().body.scrollTop, 0);
  await resolveBodyReferences(f);
  assert.equal(view().limits.open, false); assert.equal(view().list.scrollTop, 0);
});

test('the existing native Relation details footer stays open through metadata polling', async () => {
  const f = await verifiedBodyFixture(); pin(f, originA);
  f.els.boundsDetails.open = true;
  const before = f.els.boundsDetails.replacements;
  const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(snapshot())); await poll;
  assert.equal(f.els.boundsDetails.open, true); assert.equal(f.els.boundsDetails.replacements, before);
  assert.equal(f.els.boundsDetails.classList.contains('hidden'), false);
});

test('same exact source keeps native inspector state when its selected endpoint moves to another index', async () => {
  const f = await verifiedBodyFixture(); await resolveBodyReferences(f);
  const view = nativeInspectorModel(f);
  f.els.detail.innerHTML = f.els.detail.innerHTML.replace('<details class="service-map-body-reference-limits">', '<details class="service-map-body-reference-limits" open="">');
  const previous = view(); previous.limits.open = true; previous.list.scrollTop = 110; previous.body.scrollTop = 350;
  f.document.activeElement = previous.summary;
  const replacements = f.els.detail.replacements;
  const data = snapshot([origin(originA, [endpoint('/inserted', 'GET', 'inserted-source'), endpoint()]), origin(originB)]);
  const poll = pollRealTargetMap(f); f.requests.at(-1).resolve(response(data)); await poll;
  assert.equal(f.state.endpointIndex, 1); assert.equal(f.state.endpointSelection.path, '/users');
  assert.equal(f.state.evidenceId, 'evidence-one'); assert.equal(f.state.bodyReferences.status, 'scanned');
  assert.equal(f.els.detail.replacements, replacements); assert.equal(view(), previous);
  assert.equal(view().limits.open, true); assert.equal(view().list.scrollTop, 110); assert.equal(view().body.scrollTop, 350);
  assert.equal(f.document.activeElement, previous.summary);
  assert.equal(f.requests.filter(item => item.url.includes('/body-references')).length, 1);
});


test('metadata map footer distinguishes explicit body inspection from map-wide extraction', () => {
  const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
  assert.match(html, /Body references require explicit inspection of one retained response; they are not added to this map\./);
  assert.doesNotMatch(html, /Response-body reference extraction is deferred/);
});
