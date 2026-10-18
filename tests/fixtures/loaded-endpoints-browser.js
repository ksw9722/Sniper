/* Synthetic passive browser fixture. Actual production controller; no app.js. */
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const A = 'https://app.example.com', B = 'https://api.example.com';
  const SESSION = '11111111-1111-4111-8111-111111111111', GENERATION = '22222222-2222-4222-8222-222222222222';
  const RECORD = '33333333-3333-4333-8333-333333333333', REVISION = '44444444-4444-4444-8444-444444444444';
  const LONG_METHOD = 'CUSTOM-' + 'LONG-UNKNOWN-METHOD-'.repeat(8);
  const token = number => `v1.${SESSION}.${GENERATION}.${9007199254740993n + BigInt(number)}.${REVISION}.${RECORD}`;
  const clone = value => JSON.parse(JSON.stringify(value));
  const requests = [], held = [], stats = { map: 0, evidence: 0, unexpected: 0, blockedConnections: 0, cspBlocks: 0, expectedFontBlocks: 0, unexpectedCspBlocks: 0 };
  let controller, scenario = 'base', view = 'service-map';
  function endpoints() {
    const rows = Array.from({ length: 130 }, (_, index) => ({ method: index === 111 || index === 129 ? 'get' : index === 112 ? LONG_METHOD : 'GET',
      path: index === 113 ? '/long/' + 'bounded-path-'.repeat(35) : index === 128 ? '/CaseSensitive/[literal]' : index === 129 ? '/casesensitive/[literal]' : `/path-${index}`,
      request_count: 1, statuses: [200], content_types: ['application/json'], evidence: [token(index)] }));
    if (scenario === 'reordered') return [rows[110], ...rows.filter((_, index) => index !== 110).reverse()];
    if (scenario === 'missing-method') return rows.filter(row => row.method !== LONG_METHOD && row.method !== 'get');
    if (scenario === 'short') return rows.slice(0, 12);
    if (scenario === 'empty') return [];
    return rows;
  }
  function snapshot() {
    const app = endpoints(), api = Array.from({ length: 8 }, (_, index) => ({ method: index % 2 ? 'POST' : 'GET', path: `/api/path-${index}`, request_count: 1, statuses: [200], content_types: ['application/json'], evidence: [token(1000 + index)] }));
    const origin = (id, rows) => ({ origin: id, in_scope: true, request_count: rows.length, endpoints: rows, first_seen: '2026-10-12T00:00:00Z', last_seen: '2026-10-12T01:00:00Z', technology_clues: [], technology_observations: [], header_links: [], redirects: [] });
    return { schema_version: 1, session_id: SESSION, technology_catalog_version: 2,
      snapshot: { generation: GENERATION, complete: scenario !== 'partial', stopped_by: scenario === 'partial' ? ['max_records', 'endpoint_limit'] : [],
        limits: { records: 10000 }, retained_rows: scenario === 'partial' ? 20000 : app.length + api.length, considered_rows: scenario === 'partial' ? 10000 : app.length + api.length,
        included_rows: app.length + api.length, excluded_rows: 0, omitted_rows: scenario === 'partial' ? 9862 : 0, oldest_considered_at: '2026-10-12T00:00:00Z', latest_considered_at: '2026-10-12T01:00:00Z' },
      origins: [origin(A, app), origin(B, api)], unavailable: [], deferred: [], warnings: [] };
  }
  function evidence(id) {
    const owner = snapshot().origins.find(origin => origin.endpoints.some(row => row.evidence.includes(id)));
    const endpoint = owner?.endpoints.find(row => row.evidence.includes(id));
    return { schema_version: 1, session_id: SESSION, technology_catalog_version: 2, evidence_id: id, available: !!endpoint,
      source: 'captured_http_metadata', row: endpoint ? { evidence_id: id, origin: owner.origin, method: endpoint.method, path: endpoint.path,
        sequence: id.split('.')[3], record_id: RECORD, status: 200, started_at: '2026-10-12T00:00:00Z', metadata_complete: true, omitted: [], technology_clues: [], technology_observations: [], header_links: [], redirect: null } : null,
      socket_peer: { status: 'not_recorded', observation: null } };
  }
  function diagnostics() {
    const state = controller?.getState();
    if (!state) return;
    $('fixtureCounters').textContent = `Stub map reads: ${stats.map} · evidence reads: ${stats.evidence} · unexpected: ${stats.unexpected} · external connection attempts: ${stats.blockedConnections} · CSP blocks: ${stats.cspBlocks} (${stats.expectedFontBlocks} expected local font; ${stats.unexpectedCspBlocks} unexpected) · held: ${held.length}`;
    $('fixtureSelection').textContent = `View: ${state.view} · inspector APIs page: ${state.detailPage + 1} · catalog page: ${state.page + 1} · endpoint: ${state.endpointSelection ? state.endpointSelection.method + ' ' + state.endpointSelection.path : 'none'} · method filter: ${state.endpointMethod || 'All'} · path substring: ${state.endpointPath || '(empty)'} · verified evidence: ${state.evidence ? 'yes' : 'no'} · zoom: ${Math.round(state.zoomScale * 100)}%`;
  }
  function request(path, options = {}) {
    const url = new URL(path, 'http://example.com');
    const kind = url.pathname === '/api/target/service-map' ? 'map' : url.pathname === '/api/target/service-map/evidence' ? 'evidence' : 'unexpected';
    if (kind === 'unexpected' || url.origin !== 'http://example.com' || url.searchParams.get('session_id') !== SESSION || options.redirect !== 'error' || options.method && options.method !== 'GET') {
      stats.unexpected++; diagnostics(); throw new Error('Unexpected stub request blocked');
    }
    stats[kind]++; requests.push({ kind, path, signal: options.signal });
    const li = document.createElement('li'); li.textContent = `${kind}: GET ${path}`; $('fixtureRequests').append(li);
    const payload = clone(kind === 'map' ? snapshot() : evidence(url.searchParams.get('evidence_id')));
    const response = { ok: true, status: 200, json: async () => clone(payload) };
    if (kind === 'evidence' && $('fixtureHold').checked) {
      $('fixtureHold').checked = false;
      return new Promise(resolve => { held.push(() => resolve(response)); diagnostics(); });
    }
    diagnostics(); return Promise.resolve(response);
  }
  const rejectConnection = () => { stats.blockedConnections++; diagnostics(); throw new Error('Network disabled in this synthetic fixture'); };
  window.fetch = rejectConnection;
  window.XMLHttpRequest = class { constructor() { rejectConnection(); } };
  window.WebSocket = class { constructor() { rejectConnection(); } };
  window.EventSource = class { constructor() { rejectConnection(); } };
  document.addEventListener('securitypolicyviolation', event => {
    stats.cspBlocks++;
    const expectedFont = event.effectiveDirective === 'font-src'
      && event.blockedURI === new URL('/fonts/Bungee-Regular.ttf', window.location.href).href;
    if (expectedFont) stats.expectedFontBlocks++;
    else stats.unexpectedCspBlocks++;
    diagnostics();
  });
  const ids = { panel: 'targetServiceMapPanel', status: 'serviceMapStatus', refresh: 'serviceMapRefresh', search: 'serviceMapSearch', scope: 'serviceMapScopeOnly',
    mapControls: 'serviceMapViewControls', allOrigins: 'serviceMapAllOrigins', directRelations: 'serviceMapDirectRelations', focusSelection: 'serviceMapFocusSelection',
    canvas: 'serviceMapCanvas', viewport: 'serviceMapViewport', zoomControls: 'serviceMapZoomControls', zoomFit: 'serviceMapZoomFit', zoomOut: 'serviceMapZoomOut', zoomReset: 'serviceMapZoomReset', zoomIn: 'serviceMapZoomIn',
    graphBounds: 'serviceMapGraphBounds', boundsDetails: 'serviceMapBoundsDetails', graphDiagnostics: 'serviceMapGraphDiagnostics', overlayStatus: 'serviceMapBodyOverlay', detail: 'serviceMapDetail', content: 'serviceMapContent', catalog: 'serviceMapApiCatalog' };
  const on = (id, fn) => $(id).addEventListener('click', () => Promise.resolve().then(fn).then(diagnostics).catch(error => { $('fixtureMessage').textContent = error.message; }));
  function setView(next) { view = next; controller.setVisible(true, view); diagnostics(); }
  async function refreshScenario() { scenario = $('fixtureScenario').value; await controller.refresh({ force: true }); diagnostics(); }
  async function reset() {
    scenario = 'base'; $('fixtureScenario').value = scenario; $('serviceMapSearch').value = ''; $('serviceMapSearch').dispatchEvent(new Event('input'));
    controller.reset(); setView('service-map'); await controller.refresh(); controller.selectOrigin(A); diagnostics();
  }
  controller = window.SniperServiceMap.createController({ elements: Object.fromEntries(Object.entries(ids).map(([key, id]) => [key, $(id)])), getSessionId: () => SESSION, document, request });
  controller.setViewChange(setView);
  on('fixtureMap', () => setView('service-map')); on('fixtureCatalog', () => setView('apis'));
  on('fixtureRefresh', refreshScenario); on('fixtureReset', reset);
  on('fixtureRelease', () => { held.splice(0).forEach(resolve => resolve()); });
  on('fixtureInvalidate', () => controller.invalidate());
  $('fixtureTextSize').addEventListener('change', () => { document.documentElement.style.setProperty('--ui-root-size', $('fixtureTextSize').value + 'px'); controller.displayChanged(); diagnostics(); });
  const observer = new MutationObserver(diagnostics); observer.observe($('targetServiceMapPanel'), { childList: true, subtree: true });
  $('targetServiceMapPanel').addEventListener('input', diagnostics); $('targetServiceMapPanel').addEventListener('change', diagnostics);
  window.loadedEndpointsFixture = { controller, stats, requests, snapshot, evidence, setView, reset, refreshScenario, longMethod: LONG_METHOD, get heldCount() { return held.length; } };
  reset().catch(error => { $('fixtureMessage').textContent = error.message; });
})();
