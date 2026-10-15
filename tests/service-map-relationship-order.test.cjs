// The map draws at most MAX_CARDS cards and an arrow only between two shown
// cards. Picked by name, related origins fell on either side of the cap, and a
// real session with 67 relationships among 86 origins drew no arrow at all.
// Linked cards also lead their column, so the arrows start on screen.
// Trackpad pinch zooms the graph around the fingers.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const source = fs.readFileSync(path.join(__dirname, '../web/service-map.js'), 'utf8');
const context = vm.createContext({ URL, URLSearchParams, AbortController, console, getComputedStyle: element => ({ getPropertyValue: name => element.style.values[name] || '' }), ResizeObserver: class { observe() {} } });
vm.runInContext(source, context);
const map = context.SniperServiceMap;

function deferred() { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; }
function response(payload) { return { ok: true, status: 200, json: async () => payload }; }
function origin(id, extra = {}) {
  return { origin: id, in_scope: true, request_count: 1, endpoints: [{ method: 'GET', path: '/', request_count: 1, statuses: [200], content_types: ['text/html'], evidence: [`evidence-${id}`] }], technology_clues: [], redirects: [], header_links: [], ...extra };
}
const referer = (sourceOrigin) => ({ source: 'request_header_declaration', source_origin: sourceOrigin, relationship: 'referer_header', evidence: ['evidence-header'] });
const redirect = (target) => ({ target_origin: target, target_path: '/', relationship: 'redirect_instruction', evidence: ['evidence-redirect'] });
function snapshot(origins) {
  return { schema_version: 1, session_id: 'session-one', snapshot: { generation: 'generation-one', complete: true, stopped_by: [], limits: { records: 10000 }, considered_rows: origins.length, retained_rows: origins.length, included_rows: origins.length, excluded_rows: 0, omitted_rows: 0, oldest_considered_at: '2026-10-12T01:00:00Z', latest_considered_at: '2026-10-12T02:00:00Z' }, origins, unavailable: ['initiator', 'remote_ip', 'tls_certificate'], deferred: ['response_body_references'], warnings: [] };
}
function node() {
  const handlers = new Map(), classes = new Set();
  let html = '';
  return {
    value: '', checked: false, disabled: false, dataset: {}, attributes: {}, textContent: '', scrollTop: 0, scrollLeft: 0, clientWidth: 0, clientHeight: 0,
    style: { values: {}, setProperty(name, value) { this.values[name] = value; } },
    classList: { toggle(name, value) { if (value) classes.add(name); else classes.delete(name); }, contains: name => classes.has(name) },
    setAttribute(name, value) { this.attributes[name] = value; },
    addEventListener(name, callback) { handlers.set(name, callback); },
    dispatch(name, event = {}) { return handlers.get(name)?.({ target: this, preventDefault() {}, ...event }); },
    contains: () => true, querySelectorAll: () => [],
    get innerHTML() { return html; }, set innerHTML(value) { html = value; },
  };
}
function fixture() {
  const requests = [];
  const els = Object.fromEntries(['panel', 'status', 'refresh', 'search', 'scope', 'canvas', 'graphBounds', 'detail', 'content', 'catalog', 'mapControls', 'allOrigins', 'directRelations', 'focusSelection', 'boundsDetails', 'graphDiagnostics', 'overlayStatus', 'viewport', 'zoomControls', 'zoomFit', 'zoomOut', 'zoomReset', 'zoomIn'].map(name => [name, node()]));
  els.viewport.clientWidth = 300; els.viewport.clientHeight = 300;
  const document = { activeElement: null, body: {}, documentElement: node(), querySelector: () => null };
  document.documentElement.style.setProperty('--ui-root-size', '12px');
  const controller = map.createController({ elements: els, getSessionId: () => 'session-one', document, request() { const pending = deferred(); requests.push(pending); return pending.promise; } });
  controller.setVisible(true);
  return { controller, els, requests, state: controller.getState(), async load(payload) { const pending = controller.refresh(); requests.at(-1).resolve(response(payload)); await pending; } };
}
const arrows = (html) => (html.match(/data-relationship=/g) || []).length;

// 24 unrelated origins sort first by name; the related ones sort last.
const unrelated = Array.from({ length: 24 }, (_, i) => `https://a${String(i).padStart(2, '0')}.example.com`);
const app = 'https://x-app.example.com', api = 'https://x-api.example.com', cdn = 'https://x-cdn.example.com', login = 'https://y-login.example.com';
function relatedOrigins() {
  return [...unrelated.map(id => origin(id)),
    origin(app, { redirects: [] }),
    origin(api, { header_links: [referer(app)] }),
    origin(cdn, { header_links: [referer(app)] }),
    origin(login, { redirects: [redirect(app)] })];
}

test('related origins lead the order, best connected first, so their arrows fit under the card cap', () => {
  const origins = relatedOrigins();
  const byName = origins.map(item => item.origin).sort();
  assert.equal(map.graphLayout(origins, byName).edges.length, 0, 'by name, every related origin is past the cap');
  const order = map.relationshipOrder(origins);
  assert.deepEqual([...order.slice(0, 4)], [app, api, cdn, login], 'the hub, then its neighbours');
  assert.deepEqual([...order.slice(4)], unrelated, 'unrelated origins follow by name');
  const layout = map.graphLayout(origins, order);
  assert.equal(layout.edges.length, 3);
  assert.deepEqual([api, app, cdn].map(id => layout.slots.get(id).row), [0, 1, 2], 'linked targets lead their column, ahead of unrelated names');
});

test('a self declaration links nothing and reference-only origins stay after captured ones', () => {
  const origins = [origin('https://b.example.com', { header_links: [referer('https://b.example.com')] }), origin('https://a.example.com'),
    { ...origin('https://z.example.com'), captured: false }, { ...origin('https://c.example.com'), captured: false }];
  assert.deepEqual([...map.relationshipOrder(origins)], ['https://a.example.com', 'https://b.example.com', 'https://c.example.com', 'https://z.example.com']);
});

test('the first map shows related origins with their arrows, and Reload re-ranks later relationships', async () => {
  const f = fixture();
  await f.load(snapshot(relatedOrigins()));
  assert.equal(arrows(f.els.canvas.innerHTML), 3);

  // Polling keeps the order it opened with, so relationships that appear later
  // between cards past the cap stay hidden until an explicit Reload.
  const g = fixture();
  await g.load(snapshot([...unrelated.map(id => origin(id)), origin(app), origin(api)]));
  await g.load(snapshot(relatedOrigins()));
  assert.equal(arrows(g.els.canvas.innerHTML), 0);
  const reload = g.els.refresh.dispatch('click');
  g.requests.at(-1).resolve(response(snapshot(relatedOrigins()))); await reload;
  assert.equal(arrows(g.els.canvas.innerHTML), 3);
});

test('an origin naming itself in Referer is not listed as a relationship of that origin', async () => {
  const f = fixture();
  await f.load(snapshot([origin(app, { header_links: [referer(app), referer(api)] }), origin(api)]));
  const labels = (f.state.relationsByOrigin.get(app)?.items || []).map(item => `${item.sourceOrigin} -> ${item.targetOrigin}`);
  assert.deepEqual([...labels], [`${api} -> ${app}`]);
});

test('ctrl+wheel and WebKit pinch gestures zoom around the fingers; a plain wheel only scrolls', async () => {
  const f = fixture();
  await f.load(snapshot(unrelated.map(id => origin(id))));
  f.els.viewport.getBoundingClientRect = () => ({ left: 100, top: 50 });
  f.els.viewport.scrollLeft = 120; f.els.viewport.scrollTop = 480;
  let prevented = 0;
  const wheel = (extra) => f.els.viewport.dispatch('wheel', { deltaY: -10, deltaMode: 0, clientX: 100, clientY: 50, preventDefault() { prevented += 1; }, ...extra });

  wheel({ ctrlKey: false });
  assert.equal(f.state.zoomScale, 1); assert.equal(prevented, 0, 'a plain wheel is left to scroll the map');

  wheel({ ctrlKey: true });
  assert.ok(Math.abs(f.state.zoomScale - Math.exp(0.1)) < 1e-4); assert.equal(prevented, 1);
  // The fingers are at the viewport's top-left corner, so that graph point stays put.
  assert.ok(Math.abs(f.els.viewport.scrollLeft - 120 * f.state.zoomScale) < 0.01);
  assert.ok(Math.abs(f.els.viewport.scrollTop - 480 * f.state.zoomScale) < 0.01);

  wheel({ ctrlKey: true, deltaY: 1000 });
  assert.ok(Math.abs(f.state.zoomScale - Math.round(Math.exp(0.1) * Math.exp(-0.2) * 10000) / 10000) < 1e-4, 'one mouse notch is capped near a button step');

  f.controller.setZoom(1);
  f.els.viewport.dispatch('gesturestart', { preventDefault() { prevented += 1; } });
  f.els.viewport.dispatch('gesturechange', { scale: 1.5, clientX: 250, clientY: 200, preventDefault() { prevented += 1; } });
  assert.equal(f.state.zoomScale, 1.5);
  wheel({ ctrlKey: true });
  assert.equal(f.state.zoomScale, 1.5, 'a ctrl+wheel arriving during a gesture does not zoom twice');
  f.els.viewport.dispatch('gesturechange', { scale: 9, preventDefault() {} });
  assert.equal(f.state.zoomScale, 2, 'pinching stays within the zoom limits');
  f.els.viewport.dispatch('gestureend', { preventDefault() {} });
  f.els.viewport.dispatch('gesturechange', { scale: 0.5, preventDefault() {} });
  assert.equal(f.state.zoomScale, 2, 'a change after the gesture ended is ignored');
  assert.equal(f.requests.length, 1, 'zooming reads nothing');
});
