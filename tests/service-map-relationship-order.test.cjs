// The map draws at most MAX_CARDS cards and an arrow only between two shown
// cards. Picked by name, related origins fell on either side of the cap, and a
// real session with 67 relationships among 86 origins drew no arrow at all.
// Cards are laid out by link depth, so links run between columns, not inside one.
// Trackpad pinch zooms the graph around the fingers. An Origin/Referer link is
// drawn for the kinds of request behind it, so image loads can stay hidden.
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
function fixture(options = {}) {
  const requests = [];
  const els = Object.fromEntries(['panel', 'status', 'filterSummary', 'refresh', 'search', 'scope', 'canvas', 'graphBounds', 'detail', 'content', 'catalog', 'mapControls', 'allOrigins', 'directRelations', 'focusSelection', 'boundsDetails', 'graphDiagnostics', 'overlayStatus', 'viewport', 'zoomControls', 'zoomFit', 'zoomOut', 'zoomReset', 'zoomIn', 'linkKinds', 'siteControls'].map(name => [name, node()]));
  els.viewport.clientWidth = 300; els.viewport.clientHeight = 300;
  const document = { activeElement: null, body: {}, documentElement: node(), querySelector: () => null };
  document.documentElement.style.setProperty('--ui-root-size', '12px');
  const controller = map.createController({ ...options, elements: els, getSessionId: () => 'session-one', document, request(path) { const pending = deferred(); pending.path = path; requests.push(pending); return pending.promise; } });
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
  const at = id => [layout.slots.get(id).column, layout.slots.get(id).row];
  assert.deepEqual([login, app, api, cdn].map(at), [[0, 0], [1, 0], [2, 0], [2, 1]], 'each link moves one column right');
  assert.ok(unrelated.slice(0, 20).every(id => layout.slots.get(id).row >= 2), 'unlinked cards sit below the linked ones');
});

const columnOf = (layout, id) => layout.slots.get(id).column;
const sameColumnArrows = layout => layout.edges.filter(edge => columnOf(layout, edge.source) === columnOf(layout, edge.target)).length;

test('a hub whose targets also link to each other spreads over columns without same-column loops', () => {
  // The real session's shape: one page, its targets, and two of those targets
  // calling further origins and one shared target.
  const id = name => `https://${name}.example.com`;
  const hub = origin(id('hub')), targets = Array.from({ length: 6 }, (_, i) => origin(id(`t${i}`), { header_links: [referer(hub.origin)] }));
  const shared = origin(id('shared'), { header_links: [referer(hub.origin), referer(targets[0].origin), referer(targets[1].origin)] });
  const deep = origin(id('deep'), { header_links: [referer(targets[0].origin)] });
  const origins = [hub, ...targets, shared, deep];
  const layout = map.graphLayout(origins, map.relationshipOrder(origins));
  assert.equal(columnOf(layout, hub.origin), 0);
  assert.ok(targets.every(item => columnOf(layout, item.origin) === 1));
  assert.deepEqual([shared, deep].map(item => columnOf(layout, item.origin)), [2, 2], 'a target of the hub and of its targets sits past both');
  assert.equal(sameColumnArrows(layout), 0);
  assert.equal(layout.slots.get(hub.origin).row, 2, 'a short column is centered on the tallest');
});

test('a cycle turns one link around, and a long chain stops at five columns', () => {
  const a = origin('https://a.example.com', { redirects: [redirect('https://b.example.com')] });
  const b = origin('https://b.example.com', { redirects: [redirect(a.origin)] });
  const loop = map.graphLayout([a, b], [a.origin, b.origin]);
  assert.deepEqual([a, b].map(item => columnOf(loop, item.origin)), [0, 1]);
  assert.equal(loop.edges.length, 2, 'both directions keep their arrow');
  assert.equal(sameColumnArrows(loop), 0);

  const chain = Array.from({ length: 8 }, (_, i) => origin(`https://c${i}.example.com`));
  chain.forEach((item, i) => { if (chain[i + 1]) item.redirects = [redirect(chain[i + 1].origin)]; });
  const long = map.graphLayout(chain, chain.map(item => item.origin));
  assert.deepEqual(chain.map(item => columnOf(long, item.origin)), [0, 1, 2, 3, 4, 4, 4, 4]);
  assert.deepEqual(chain.slice(4).map(item => long.slots.get(item.origin).row), [0, 1, 2, 3], 'the shared last column keeps chain order');
  assert.equal(long.width, 2 * map.GRAPH_GEOMETRY.padding + 4 * map.GRAPH_GEOMETRY.columnPitch + map.GRAPH_GEOMETRY.cardWidth);
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

// Points along an SVG path of M, L and C commands, in layout pixels.
function pathPoints(d, steps = 40) {
  const tokens = d.match(/[MLC]|-?\d+(?:\.\d+)?/g), points = [];
  let i = 0, x = 0, y = 0, command = 'M';
  const num = () => Number(tokens[i++]);
  while (i < tokens.length) {
    if (/[MLC]/.test(tokens[i])) command = tokens[i++];
    if (command === 'M') { x = num(); y = num(); points.push([x, y]); command = 'L'; continue; }
    const [ax, ay, bx, by, nx, ny] = command === 'L' ? [x, y, x, y, num(), num()] : [num(), num(), num(), num(), num(), num()];
    const lerp = command === 'L';
    for (let s = 1; s <= steps; s++) {
      const t = s / steps, u = 1 - t;
      points.push(lerp ? [x + (nx - x) * t, y + (ny - y) * t]
        : [u ** 3 * x + 3 * u * u * t * ax + 3 * u * t * t * bx + t ** 3 * nx, u ** 3 * y + 3 * u * u * t * ay + 3 * u * t * t * by + t ** 3 * ny]);
    }
    x = nx; y = ny;
  }
  return points;
}
function cardsCrossed(layout, edge) {
  const { cardWidth, cardHeight } = layout.geometry, points = pathPoints(edge.path);
  return Array.from(layout.nodes).filter(node => node.origin !== edge.source && node.origin !== edge.target
    && points.some(([x, y]) => x > node.x + 1 && x < node.x + cardWidth - 1 && y > node.y + 1 && y < node.y + cardHeight - 1)).map(node => node.origin);
}

test('an arrow that skips a column runs between rows instead of behind the cards it passes', () => {
  // A sign-in chain that returns to its start: every card is on one row, so a
  // straight return arrow would run through both middle cards and under the
  // forward arrows.
  const chain = Array.from({ length: 4 }, (_, i) => origin(`https://s${i}.example.com`));
  chain.forEach((item, i) => { item.redirects = [redirect(chain[(i + 1) % 4].origin)]; });
  const layout = map.graphLayout(chain, chain.map(item => item.origin));
  assert.deepEqual(chain.map(item => columnOf(layout, item.origin)), [0, 1, 2, 3]);
  const back = layout.edges.find(edge => edge.source === chain[3].origin);
  assert.ok(back.path.includes(' L '), 'the return arrow follows a lane');
  const { padding, cardHeight, rowPitch } = layout.geometry;
  const laneY = Number(back.path.split(' L ')[1].split(' ')[1]);
  assert.ok(laneY > padding + cardHeight && laneY < padding + rowPitch, 'the lane is the gap below the row');
  for (const edge of layout.edges) assert.deepEqual(cardsCrossed(layout, edge), [], edge.path);

  // The real session's shape: the hub also declares the targets of its targets.
  const id = name => `https://${name}.example.com`;
  const hub = origin(id('hub')), mid = Array.from({ length: 5 }, (_, i) => origin(id(`m${i}`), { header_links: [referer(hub.origin)] }));
  const far = Array.from({ length: 4 }, (_, i) => origin(id(`f${i}`), { header_links: [referer(hub.origin), referer(mid[i].origin)] }));
  const shaped = map.graphLayout([hub, ...mid, ...far], map.relationshipOrder([hub, ...mid, ...far]));
  assert.equal(shaped.edges.filter(edge => Math.abs(columnOf(shaped, edge.target) - columnOf(shaped, edge.source)) > 1).length, 4);
  for (const edge of shaped.edges) assert.deepEqual(cardsCrossed(shaped, edge), [], edge.path);
});

test('arrows between neighbouring columns turn at their own points, the longest nearest the source', () => {
  const hub = origin('https://hub.example.com');
  const targets = Array.from({ length: 5 }, (_, i) => origin(`https://t${i}.example.com`, { header_links: [referer(hub.origin)] }));
  const layout = map.graphLayout([hub, ...targets], [hub.origin, ...targets.map(item => item.origin)]);
  const bend = edge => Number(edge.path.match(/C (-?[\d.]+)/)[1]);
  const span = edge => Math.abs(layout.slots.get(edge.target).row - layout.slots.get(edge.source).row);
  const bent = layout.edges.filter(edge => span(edge) > 0);
  assert.equal(new Set(bent.map(bend)).size, bent.length, 'no two vertical runs share a track');
  const [longest] = [...bent].sort((a, b) => span(b) - span(a));
  assert.equal(bend(longest), Math.min(...bent.map(bend)));
});

test('cards whose arrows all fall past the arrow cap sit with the unlinked cards', () => {
  const hub = origin('https://hub.example.com');
  const targets = Array.from({ length: 20 }, (_, i) => origin(`https://t${String(i).padStart(2, '0')}.example.com`, { header_links: [referer(hub.origin), { ...referer(hub.origin), relationship: 'origin_header' }] }));
  const p = origin('https://p.example.com'), q = origin('https://q.example.com', { header_links: [referer('https://p.example.com')] });
  const origins = [hub, ...targets, p, q];
  const layout = map.graphLayout(origins, map.relationshipOrder(origins));
  assert.equal(layout.edgeCount, 41); assert.equal(layout.edges.length, 40);
  assert.ok([p, q].every(item => layout.slots.get(item.origin).row >= 20), 'below the 20 linked targets, not beside them');
});

test('a pinned star centers its source on its neighbors', () => {
  const source = origin('https://source.example.com'), neighbors = Array.from({ length: 5 }, (_, i) => origin(`https://n${i}.example.com`));
  const bodyEdges = neighbors.map(item => ({ source: source.origin, target: item.origin, relationship: 'body_url_reference', label: 'Literal body URL reference' }));
  const layout = map.graphLayout([source, ...neighbors], [source.origin, ...neighbors.map(item => item.origin)], new Map(), source.origin, bodyEdges);
  assert.deepEqual([layout.slots.get(source.origin).column, layout.slots.get(source.origin).row], [0, 2]);
  for (const edge of layout.edges) assert.deepEqual(cardsCrossed(layout, edge), [], edge.path);
});

test('the Service Map filter rides on every read; changing it re-reads, re-ranks and reports hidden rows', async () => {
  let filter = { hidden_extensions: 'png,css' };
  const f = fixture({ getFilterQuery: () => filter, getFilterSummary: () => ({ text: 'hiding png, css', detail: 'Service Map filter: hiding png, css' }) });
  await f.load(snapshot(relatedOrigins()));
  assert.match(f.requests.at(-1).path, /hidden_extensions=png%2Ccss/);
  assert.match(f.requests.at(-1).path, /session_id=session-one/);
  assert.equal(f.els.filterSummary.textContent, 'hiding png, css');

  // A filter restored from settings, with no dialog: the next poll re-ranks.
  filter = { hidden_extensions: 'png' };
  f.state.order = ['https://stale.example.com'];
  await f.load(snapshot(relatedOrigins()));
  assert.equal(f.state.order[0], app);

  filter = { mime_types: 'html,json' };
  f.state.order = ['https://stale.example.com'];
  const reading = f.controller.filterChanged();
  assert.match(f.requests.at(-1).path, /mime_types=html%2Cjson/);
  assert.doesNotMatch(f.requests.at(-1).path, /hidden_extensions/);
  const payload = snapshot(relatedOrigins());
  payload.snapshot.filtered_rows = 7;
  f.requests.at(-1).resolve(response(payload)); await reading;
  assert.deepEqual([...f.state.order.slice(0, 4)], [app, api, cdn, login], 'cards are ranked again for the rows that remain');
  assert.match(f.els.status.textContent, /7 hidden by the Service Map filter/);
});

test("the selected card's own gold arrows flow toward their targets, and only when motion is allowed", async () => {
  const f = fixture();
  await f.load(snapshot(relatedOrigins()));
  const selected = () => (f.els.canvas.innerHTML.match(/class="service-map-edge [^"]*\bselected\b/g) || []).length;
  f.controller.selectOrigin(app);
  assert.equal(selected(), 3, 'the hub: two Referer targets and the redirect into it');
  f.controller.selectOrigin(api);
  assert.equal(selected(), 1);
  assert.doesNotMatch(f.els.canvas.innerHTML, /service-map-flow/, 'no second line is drawn over the arrow');
  const css = fs.readFileSync(path.join(__dirname, '../web/styles.css'), 'utf8');
  const motion = css.match(/@media \(prefers-reduced-motion: no-preference\) \{([^]*?)\n\}/)?.[1] || '';
  assert.match(motion, /\.service-map-edge\.selected:not\(\.peer-observation\) \{ animation: service-map-flow-18/);
  assert.match(motion, /data-relationship="redirect_instruction"\] \{ stroke-dasharray: 14 4; \}/, 'a solid redirect takes dashes so it can flow');
  assert.match(css, /@keyframes service-map-flow-11 \{ to \{ stroke-dashoffset: -11; \} \}/, 'one Referer dash period per cycle');
  assert.doesNotMatch(css.replace(motion, ''), /\.service-map-edge\.selected[^{]*\{[^}]*animation/, 'no animation outside the motion-allowed block');
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

// What each Origin/Referer link stood for: an API call, a page load, a frame,
// or a script or static file the page pulled in.
const kinded = (sourceOrigin, kinds) => ({ ...referer(sourceOrigin), request_count: Object.values(kinds).reduce((sum, rows) => sum + rows, 0), request_kinds: kinds });
function kindedOrigins() {
  // The page's own reloads name itself; that is no link and counts in no toggle.
  return [origin(app, { header_links: [kinded(app, { navigation: 2 })] }),
    origin(api, { header_links: [kinded(app, { api: 3 })] }),
    origin(cdn, { header_links: [kinded(app, { script: 1, static: 4 })] }),
    origin(login, { header_links: [kinded(app, { navigation: 1, frame: 2 })] })];
}

test('a link is drawn for the request kinds behind it: hidden kinds drop it, the leading kind colors it, rows weigh it', () => {
  const normalized = map.normalizeSnapshot(snapshot([origin(app), origin(api, { header_links: [{ ...kinded(app, { api: 2 }), request_kinds: { api: 2, bogus: 5, static: 0 } }] })]), 'session-one');
  const link = normalized.origins.find(item => item.origin === api).header_links[0];
  assert.deepEqual({ ...link.request_kinds }, { api: 2 }, 'unknown and empty kinds are dropped');
  assert.equal(link.request_count, 2);

  const origins = kindedOrigins();
  const edge = (layout, id) => layout.edges.find(item => item.target === id);
  const resourcesHidden = map.graphLayout(origins, map.relationshipOrder(origins, ['script', 'static']), new Map(), null, [], undefined, ['script', 'static']);
  assert.equal(resourcesHidden.edges.length, 2, 'the CDN link stood only for a script and images');
  assert.equal(edge(resourcesHidden, cdn), undefined);
  assert.deepEqual([edge(resourcesHidden, api).kind, edge(resourcesHidden, api).count], ['api', 3]);
  assert.deepEqual([edge(resourcesHidden, login).kind, edge(resourcesHidden, login).count], ['navigation', 3], 'a page load leads its frames, however many');
  assert.equal(map.relationshipOrder(origins, ['script', 'static']).indexOf(cdn), 3, 'a hidden link ranks nothing');

  const everything = map.graphLayout(origins, map.relationshipOrder(origins), new Map(), null, [], undefined, []);
  assert.deepEqual([edge(everything, cdn).kind, edge(everything, cdn).count, { ...edge(everything, cdn).kinds }], ['script', 5, { script: 1, static: 4 }]);
  const mixed = [origin(app), origin(api, { header_links: [kinded(app, { api: 1, static: 9 })] })];
  assert.equal(map.graphLayout(mixed, [app, api], new Map(), null, [], undefined, []).edges[0].kind, 'api', 'one API call among nine image loads still reads as an API link');
  const noFrames = map.graphLayout(origins, map.relationshipOrder(origins), new Map(), null, [], undefined, ['frame']);
  assert.deepEqual([edge(noFrames, login).kind, edge(noFrames, login).count], ['navigation', 1], 'the rows of a hidden kind no longer count');
  const older = map.graphLayout([origin(app), origin(api, { header_links: [referer(app)] })], [app, api], new Map(), null, [], undefined, ['script', 'static']);
  assert.deepEqual([older.edges[0].kind, older.edges[0].count], ['other', 1], 'a link from an older server is drawn as other requests');
});

test('the legend toggles links by request kind: the map redraws, ranks again and saves the choice', async () => {
  let hidden = ['script', 'static'];
  const saves = [];
  const f = fixture({ getHiddenLinkKinds: () => hidden, setHiddenLinkKinds: kinds => { hidden = kinds; saves.push([...kinds]); } });
  await f.load(snapshot(kindedOrigins()));
  const toggles = () => Object.fromEntries([...f.els.linkKinds.innerHTML.matchAll(/data-link-kind="(\w+)"[^>]*aria-pressed="(\w+)"[^>]*><span class="service-map-kind-swatch"[^>]*><\/span>([^<]*)</g)].map(([, kind, pressed, label]) => [kind, `${pressed} ${label}`]));
  assert.deepEqual(toggles(), { api: 'true API calls 1', navigation: 'true Page navigations 1', frame: 'true Frames 1', script: 'false Scripts 1', static: 'false Static files 1' });
  assert.equal(arrows(f.els.canvas.innerHTML), 2);
  assert.match(f.els.canvas.innerHTML, /class="service-map-edge header-declaration kind-api [^"]*" data-relationship="referer_header" style="--edge-weight:1.76px"/, 'three rows draw a slightly heavier line');
  assert.match(f.els.canvas.innerHTML, /3 requests \(API calls 3\)<\/title>/);

  const click = kind => f.els.linkKinds.dispatch('click', { target: { closest: () => ({ dataset: { linkKind: kind } }) } });
  click('static');
  assert.deepEqual(saves, [['script']]);
  assert.equal(toggles().static, 'true Static files 1');
  assert.equal(arrows(f.els.canvas.innerHTML), 3, 'the CDN link returns for its images');
  assert.match(f.els.canvas.innerHTML, /kind-static [^"]*" data-relationship="referer_header" style="--edge-weight:1.83px"/);
  assert.match(f.els.canvas.innerHTML, /4 requests \(Static files 4\)<\/title>/, 'the hidden script row is not counted');
  click('bogus');
  assert.equal(saves.length, 1, 'only a known kind toggles');

  // A choice made in another window arrives with settings: the next read ranks again.
  hidden = ['api', 'navigation', 'frame', 'script', 'static', 'other'];
  f.state.order = ['https://stale.example.com'];
  await f.load(snapshot(kindedOrigins()));
  assert.equal(arrows(f.els.canvas.innerHTML), 0);
  assert.notEqual(f.state.order[0], 'https://stale.example.com');
});

test('an arrow names its CORS rows, and selecting it marks its entry in the inspector', async () => {
  const f = fixture();
  const cors = { ...kinded(app, { api: 3 }), cors_allowed_count: 2, cors_credentials_count: 1 };
  await f.load(snapshot([origin(app), origin(api, { header_links: [cors] }), origin(login, { header_links: [kinded(app, { frame: 2 })] })]));
  assert.equal(arrows(f.els.canvas.innerHTML), 1, 'frames, scripts and static files are hidden by default');
  assert.match(f.els.canvas.innerHTML, /3 requests \(API calls 3\) · CORS allowed 2 of 3 rows, 1 with credentials<\/title>/);
  const hits = [...f.els.canvas.innerHTML.matchAll(/<path class="service-map-hit" data-service-edge="(\d+)"[^>]*><title>([^<]*)<\/title>/g)];
  assert.equal(hits.length, 1, 'one pointer target per recorded arrow');

  f.controller.selectOrigin(login);
  assert.match(f.els.canvas.innerHTML, /markerUnits="userSpaceOnUse"/, 'an arrowhead keeps its size on a heavy line');
  assert.match(f.els.canvas.innerHTML, /kind-api "[^>]*marker-end="url\(#service-map-arrow-api\)"/, 'an API arrow ends in a blue head');
  f.els.panel.dispatch('click', { target: { closest: selector => selector === '[data-service-edge]' ? { dataset: { serviceEdge: hits[0][1] } } : null } });
  assert.equal(f.state.selectedOrigin, api, 'the Referer link is recorded on the origin it was sent to');
  assert.match(f.els.canvas.innerHTML, /kind-api selected"[^>]*marker-end="url\(#service-map-arrow-selected\)"/, "the selected card's arrows end in gold heads");
  assert.match(f.els.detail.innerHTML, /service-map-relation highlighted"[^]*?API calls 3 · CORS allowed 2 of 3 rows, 1 with credentials/);
  assert.equal((f.els.detail.innerHTML.match(/highlighted/g) || []).length, 1);
  f.controller.selectOrigin(app);
  assert.doesNotMatch(f.els.detail.innerHTML, /highlighted/, 'another selection clears the mark');
});

// Two sites: the leading origin's example.com, and example.net with two origins.
const imgCdn = 'https://img.example.net', cssCdn = 'https://css.example.net';
function siteOrigins() {
  return [origin(app),
    origin(api, { header_links: [kinded(app, { api: 2 })] }),
    origin(imgCdn, { header_links: [kinded(app, { api: 1, static: 3 })] }),
    origin(cssCdn, { header_links: [kinded(app, { api: 2 })], redirects: [redirect(imgCdn)] })];
}
const press = (f, dataset) => f.els.panel.dispatch('click', { target: { closest: selector => selector === 'button' ? { dataset } : null } });

test('site keys take the last two host labels, three under listed suffixes, and never an address', () => {
  assert.deepEqual(['https://shop-phinf.pstatic.net', 'https://www.example.co.kr', 'https://example.com', 'http://127.0.0.1:8080', 'http://[::1]:80', 'http://localhost:3000'].map(map.siteKey),
    ['pstatic.net', 'example.co.kr', 'example.com', null, null, null]);
});

test("a site's origins share one card and one arrow, and the leading origin's site keeps a card per origin", async () => {
  let display = { groupSites: true, expandedSites: [], hidden: [] };
  const saves = [];
  const f = fixture({ getSiteDisplay: () => display, setSiteDisplay: next => { display = next; saves.push(next); } });
  await f.load(snapshot(siteOrigins()));
  const html = () => f.els.canvas.innerHTML;
  assert.match(html(), /data-service-site="example\.net"/);
  assert.doesNotMatch(html(), /data-service-origin="https:\/\/img\.example\.net"/);
  assert.match(html(), /data-service-origin="https:\/\/x-api\.example\.com"/, "the leading origin's site is not grouped");
  assert.equal(arrows(html()), 2, 'the redirect inside the site is gone');
  assert.match(html(), /→ \*\.example\.net · Referer header · referer_header · 3 requests \(API calls 3\)<\/title>/, 'both links share the arrow and add up');

  press(f, { serviceSite: 'example.net' });
  assert.match(f.els.detail.innerHTML, /\*\.example\.net[^]*2 origins grouped by site/);
  assert.match(html(), /service-map-site\s+selected/);
  press(f, { serviceOrigin: imgCdn });
  assert.equal(f.state.selectedOrigin, imgCdn);
  assert.match(html(), /service-map-site\s+selected/, 'a member keeps its site card selected');

  press(f, { serviceSiteExpand: 'example.net' });
  assert.deepEqual([...display.expandedSites], ['example.net']);
  assert.equal(arrows(html()), 4, 'each origin has its card and arrows again');
  assert.match(f.els.detail.innerHTML, /data-service-site-collapse="example\.net"/, 'an expanded site can be grouped again');

  press(f, { serviceHide: 'example.net' });
  assert.deepEqual([...display.hidden], ['example.net']);
  assert.doesNotMatch(html(), /example\.net/);
  assert.match(f.els.siteControls.innerHTML, /Hidden 1[^]*data-service-unhide="example\.net"/);
  press(f, { serviceUnhideAll: 'all' });
  assert.match(html(), /img\.example\.net/);

  press(f, { serviceGroupSites: 'toggle' });
  assert.equal(display.groupSites, false);
  assert.match(f.els.siteControls.innerHTML, /data-service-group-sites="toggle"[^>]*aria-pressed="false"/);
  assert.equal(saves.length, 4, 'expand, hide, show all and the toggle each save once');
});
