/* Inert synthetic product/version fixture. Actual production controller; no app.js. */
(function () {
  'use strict';
  const A = 'https://app.example.com';
  const LONG_ORIGIN = 'https://zz-long-retained-hostname-for-layout-checks.' + 'long-retained-catalog-declarations.'.repeat(3) + 'example.com';
  const LONG_VERSION = '12345678901234567890123456789012';
  const LONG_METHOD = 'CUSTOM-' + 'LONG-UNKNOWN-METHOD-'.repeat(8);
  const SESSION = '11111111-1111-4111-8111-111111111111', GENERATION = '22222222-2222-4222-8222-222222222222';
  const RECORD = '33333333-3333-4333-8333-333333333333', REVISION = '44444444-4444-4444-8444-444444444444';
  const FIRST = '2026-10-12T00:00:00Z', LAST = '2026-10-12T01:00:00Z';
  const REFERENCE_ONLY = 'https://reference-only.example.com', PEER_ONLY = 'https://peer-only.example.com';
  const PEER_SNAPSHOT = '55555555-5555-4555-8555-555555555555';
  const PEER_LIMITS = { max_records: 10000, origin_groups: 100, samples_per_origin: 3, projection_bytes: 262144, host_bytes: 512, scheme_bytes: 8, scope_pattern_entries: 500, scope_pattern_bytes: 512, scope_work_units: 8388608 };
  const PEER_COUNT_KEYS = ['non_http_rows', 'not_recorded_rows', 'unavailable_observation_rows', 'non_direct_rows', 'invalid_observation_rows', 'different_peer_rows', 'invalid_metadata_rows', 'scope_excluded_rows', 'matching_rows', 'returned_matching_rows', 'omitted_matching_rows', 'sample_rows', 'omitted_sample_rows'];
  const CATALOG = {
    nginx: ['nginx', 'web_server', 'nginx', 'server'], apache_http_server: ['Apache', 'web_server', 'Apache', 'server'],
    caddy: ['Caddy', 'web_server', 'Caddy', 'server'], cloudflare: ['Cloudflare', 'edge_service', 'Cloudflare', 'server'],
    microsoft_iis: ['Microsoft IIS', 'web_server', 'Microsoft-IIS', 'server'], envoy: ['Envoy', 'edge_service', 'Envoy', 'server'],
    php: ['PHP', 'language_runtime', 'PHP', 'server'], jetty: ['Jetty', 'application_server', 'Jetty', 'server'],
    apache_coyote: ['Apache-Coyote', 'connector', 'Apache-Coyote', 'server'], apache_tomcat: ['Apache Tomcat', 'application_server', 'Apache Tomcat', 'server'],
    express: ['Express', 'application_framework', 'Express', 'x-powered-by'], asp_net: ['ASP.NET', 'application_framework', 'ASP.NET', 'x-powered-by'],
    wordpress: ['WordPress', 'cms', 'WordPress', 'x-generator'], drupal: ['Drupal', 'cms', 'Drupal', 'x-generator'],
    gunicorn: ['Gunicorn', 'application_server', 'gunicorn', 'server'], uvicorn: ['Uvicorn', 'application_server', 'uvicorn', 'server'],
    waitress: ['Waitress', 'application_server', 'waitress', 'server'], openresty: ['OpenResty', 'web_server', 'openresty', 'server'],
    litespeed: ['LiteSpeed', 'web_server', 'LiteSpeed', 'server'], next_js: ['Next.js', 'application_framework', 'Next.js', 'x-powered-by'],
  };
  const V2_ONLY = new Set(['gunicorn', 'uvicorn', 'waitress', 'openresty', 'litespeed', 'next_js']);
  const SCENARIOS = ['base', 'reordered', 'disappeared-product', 'disappeared-version', 'partial', 'legacy', 'present-empty', 'field-missing', 'empty'];
  // Every retained sequence belongs to one origin. Aggregates refer to exactly
  // the same complete token as their source endpoint, including record revision.
  const token = (originIndex, endpointIndex) => `v1.${SESSION}.${GENERATION}.${9007199254740993n + BigInt(originIndex * 1000 + endpointIndex)}.${REVISION}.${RECORD}`;
  const clone = value => JSON.parse(JSON.stringify(value));
  const declaration = (product, version, status = version === null ? 'not_reported' : 'reported', header) => ({ product, version, status, header });
  const DEFINITIONS = [
    { origin: A, declarations: [declaration('nginx', '1.24.0'), declaration('nginx', '1.26.2'), declaration('nginx', null)] },
    { origin: 'https://api.example.com', declarations: [declaration('gunicorn', '22.0.0'), declaration('gunicorn', '23.0.0')] },
    { origin: 'https://all-unknown.example.com', declarations: [declaration('nginx', null), declaration('nginx', null, 'unsupported_format'), declaration('apache_coyote', null, 'non_product_version')] },
    { origin: 'https://empty-extension.example.com', declarations: [], legacyClue: true },
    { origin: 'https://missing-extension.example.com', declarations: [], legacyClue: true, absent: true },
    { origin: 'https://same-version-two-headers.example.com', declarations: [declaration('php', '8.3.12'), declaration('php', '8.3.12', 'reported', 'x-powered-by')] },
    { origin: 'https://literal-leading-zero.example.com', declarations: [declaration('nginx', '01.2'), declaration('nginx', '1.2')] },
    { origin: 'https://different-products.example.com', declarations: [declaration('nginx', '1.24.0'), declaration('caddy', '2.8.4')] },
    { origin: 'https://node-08.example.com', declarations: [declaration('uvicorn', '0.32.1')] },
    { origin: 'https://node-09.example.com', declarations: [declaration('waitress', '3.0.2')] },
    { origin: 'https://node-10.example.com', declarations: [declaration('openresty', '1.27.1')] },
    { origin: 'https://node-11.example.com', declarations: [declaration('litespeed', '6.3')] },
    { origin: 'https://node-12.example.com', declarations: [declaration('next_js', '15.0.0')] },
    { origin: 'https://node-13.example.com', declarations: [declaration('apache_http_server', '2.4.62')] },
    { origin: 'https://node-14.example.com', declarations: [declaration('cloudflare', null)] },
    { origin: 'https://node-15.example.com', declarations: [declaration('microsoft_iis', '10.0')] },
    { origin: 'https://node-16.example.com', declarations: [declaration('envoy', '1.31.0')] },
    { origin: 'https://node-17.example.com', declarations: [declaration('jetty', '12.0.9')] },
    { origin: 'https://node-18.example.com', declarations: [declaration('apache_tomcat', '10.1.30')] },
    { origin: 'https://node-19.example.com', declarations: [declaration('express', '4.21.0')] },
    { origin: 'https://node-20.example.com', declarations: [declaration('asp_net', '8.0')] },
    { origin: 'https://node-21.example.com', declarations: [declaration('wordpress', '6.6.2')] },
    { origin: 'https://node-22.example.com', declarations: [declaration('drupal', '10.3.5')] },
    { origin: 'https://node-23.example.com', declarations: [declaration('nginx', '1.24.0')] },
    { origin: 'https://zz-beyond-card-cap-24.example.com', declarations: [declaration('nginx', '1.26.2')] },
    { origin: 'https://zz-beyond-card-cap-25.example.com', declarations: [declaration('nginx', '1.26.2')] },
    { origin: LONG_ORIGIN, declarations: [declaration('nginx', LONG_VERSION)] },
    { origin: 'https://unknown-format.example.com', declarations: [declaration('php', null, 'unsupported_format')] },
    { origin: 'https://known-and-unknown.example.com', declarations: [declaration('caddy', '2.8.4'), declaration('caddy', null)] },
  ];
  function rowObservation(item) {
    const [name, catalog_category, literal, defaultHeader] = CATALOG[item.product], header_name = item.header || defaultHeader;
    return { product_id: item.product, name, catalog_category, reported_version: item.version, version_status: item.status,
      source: 'response_header', strength: 'literal_declaration', header_name, header_index: 0,
      product_span: { start: 0, end: literal.length }, version_span: item.status === 'reported' ? { start: literal.length + 1, end: literal.length + 1 + item.version.length } : null,
      offset_basis: 'retained_header_value_utf8', rule_id: `${header_name.replaceAll('-', '_')}.${item.product}.v1` };
  }
  function activeDeclarations(definition, scenario) {
    if (['present-empty', 'field-missing'].includes(scenario)) return [];
    return definition.declarations.flatMap((item, index) => {
      if (scenario === 'legacy' && V2_ONLY.has(item.product)
        || scenario === 'disappeared-product' && item.product === 'gunicorn'
        || scenario === 'disappeared-version' && item.product === 'nginx' && item.version === '1.26.2') return [];
      return [{ item, index }];
    });
  }
  function endpoints(originIndex, scenario) {
    const rows = Array.from({ length: originIndex === 0 ? 130 : 8 }, (_, index) => ({
      method: originIndex === 0 && (index === 111 || index === 129) ? 'get' : originIndex === 0 && index === 112 ? LONG_METHOD : index % 5 === 4 ? 'POST' : 'GET',
      path: originIndex === 0 ? index === 113 ? '/long/' + 'bounded-path-'.repeat(35) : index === 128 ? '/CaseSensitive/[literal]' : index === 129 ? '/casesensitive/[literal]' : `/path-${index}` : `/api/path-${index}`,
      request_count: 1, statuses: [200], content_types: ['application/json'], evidence: [token(originIndex, index)] }));
    return scenario === 'reordered' && originIndex === 0 ? [rows[110], ...rows.filter((_, index) => index !== 110).reverse()] : rows;
  }
  function redirects() {
    return [...DEFINITIONS.slice(1).map(item => item.origin), REFERENCE_ONLY].map((origin, index) => ({
      relationship: 'redirect_instruction', target_origin: origin, target_path: '/declared-destination', evidence: [token(0, 50 + index)] }));
  }
  function buildSnapshot(scenario = 'base', inScopeOnly = false) {
    if (!SCENARIOS.includes(scenario)) throw new Error('Unknown synthetic snapshot');
    const metadataRedirects = redirects(), redirectEvidence = new Set(metadataRedirects.map(item => item.evidence[0]));
    const origins = scenario === 'empty' ? [] : DEFINITIONS.map((definition, originIndex) => {
      const rows = endpoints(originIndex, scenario);
      if (originIndex === 0) for (const row of rows) if (redirectEvidence.has(row.evidence[0])) row.statuses = [302];
      const observations = activeDeclarations(definition, scenario).map(({ item, index }) => {
        const { header_index, product_span, version_span, offset_basis, rule_id, ...base } = rowObservation(item);
        return { ...base, row_count: 1, first_seen: FIRST, last_seen: FIRST, evidence: [token(originIndex, index)] };
      });
      const origin = { origin: definition.origin, in_scope: originIndex % 4 !== 3, request_count: rows.length, endpoints: rows,
        first_seen: FIRST, last_seen: LAST, technology_clues: definition.legacyClue ? [{ name: 'nginx', header_name: 'server', source: 'response_header', confidence: 'observed_header', evidence: [token(originIndex, 0)] }] : [],
        technology_observations: observations, header_links: [], redirects: originIndex === 0 ? metadataRedirects : [] };
      if (definition.absent && scenario !== 'present-empty' || scenario === 'field-missing') delete origin.technology_observations;
      return origin;
    });
    const selectedOrigins = inScopeOnly ? origins.filter(origin => origin.in_scope) : origins;
    const full = origins.reduce((sum, origin) => sum + origin.request_count, 0);
    const included = selectedOrigins.reduce((sum, origin) => sum + origin.request_count, 0), excluded = full - included, partial = scenario === 'partial';
    const result = { schema_version: 1, session_id: SESSION, technology_catalog_version: 2,
      snapshot: { generation: GENERATION, complete: !partial, stopped_by: partial ? ['max_records', 'endpoint_limit'] : [],
        limits: { records: 10000, technology_observations_per_row: 8, technology_observations_per_origin: 32, technology_observations: 2000, technology_version_bytes: 32, technology_token_candidates_per_row: 128 },
        retained_rows: partial ? 20000 : full, considered_rows: partial ? 10000 : full, included_rows: included,
        excluded_rows: excluded, omitted_rows: partial ? 10000 - full : 0, oldest_considered_at: FIRST, latest_considered_at: LAST },
      origins: selectedOrigins, unavailable: [], deferred: [], warnings: [] };
    if (scenario === 'legacy') delete result.technology_catalog_version;
    return result;
  }
  function buildEvidence(id, scenario = 'base') {
    const payload = buildSnapshot(scenario), owner = payload.origins.find(origin => origin.endpoints.some(row => row.evidence.includes(id)));
    const peerOnly = scenario !== 'empty' && id === token(DEFINITIONS.length, 0);
    const endpoint = owner?.endpoints.find(row => row.evidence.includes(id)), originIndex = DEFINITIONS.findIndex(item => item.origin === owner?.origin);
    const definition = DEFINITIONS[originIndex], matching = definition ? activeDeclarations(definition, scenario).filter(({ index }) => token(originIndex, index) === id) : [];
    let row = endpoint ? { evidence_id: id, origin: owner.origin, method: endpoint.method, path: endpoint.path, sequence: id.split('.')[3], record_id: RECORD,
      status: endpoint.statuses[0], started_at: FIRST, metadata_complete: true, omitted: [], technology_clues: definition.legacyClue && id === token(originIndex, 0) ? [{ name: 'nginx', header_name: 'server' }] : [],
      technology_observations: matching.map(({ item }) => rowObservation(item)), header_links: [], redirect: null } : null;
    const redirect = owner?.redirects.find(item => item.evidence.includes(id));
    if (row && redirect) row.redirect = { target_origin: redirect.target_origin, target_path: redirect.target_path };
    if (peerOnly) row = { evidence_id: id, origin: PEER_ONLY, method: 'GET', path: '/peer-only-retained-source', sequence: id.split('.')[3], record_id: RECORD, status: 200, started_at: FIRST, metadata_complete: true, omitted: [], technology_clues: [], technology_observations: [], header_links: [], redirect: null };
    if (row && (definition?.absent && scenario !== 'present-empty' || scenario === 'field-missing')) delete row.technology_observations;
    const result = { schema_version: 1, session_id: SESSION, technology_catalog_version: 2, evidence_id: id, available: !!row,
      source: 'captured_http_metadata', row, socket_peer: row && peerRows(scenario).some(item => item.evidence_id === id) ? peerObservation() : { status: 'not_recorded', observation: null } };
    if (scenario === 'legacy') delete result.technology_catalog_version;
    return result;
  }
  const peerObservation = () => ({ status: 'observed', observation: { source: 'http_response_remote_addr', route: 'direct', observed_at: FIRST, address: { ip: '192.0.2.10', port: 443 }, reason: null } });
  function peerRows(scenario = 'base') {
    if (scenario === 'empty') return [];
    return [{ origin: A, id: token(0, 0), in_scope: true }, { origin: A, id: token(0, 1), in_scope: true },
      ...DEFINITIONS.slice(1).map((item, index) => ({ origin: item.origin, id: token(index + 1, 0), in_scope: (index + 1) % 4 !== 3 })),
      { origin: PEER_ONLY, id: token(DEFINITIONS.length, 0), in_scope: true }].map(item => ({ origin: item.origin, evidence_id: item.id, sequence: item.id.split('.')[3], started_at: FIRST, observed_at: FIRST, in_scope: item.in_scope }));
  }
  function buildPeerMatches(id, scenario = 'base', inScopeOnly = false) {
    const rows = peerRows(scenario), source = rows.find(item => item.evidence_id === id);
    const counts = Object.fromEntries(PEER_COUNT_KEYS.map(key => [key, 0]));
    const payload = { schema_version: 1, session_id: SESSION, evidence_id: id, available: false, status: 'unavailable', complete: false, reasons: ['record_missing'], in_scope_only: inScopeOnly,
      coverage: 'retained_direct_socket_peer_equality', peer: null, source: null, snapshot: null, counts, origins: [], limits: { ...PEER_LIMITS } };
    if (!source) return payload;
    if (inScopeOnly && !source.in_scope) { payload.reasons = ['source_out_of_scope']; return payload; }
    const partial = scenario === 'partial', oldest = token(partial ? 1 : 0, 0).split('.')[3], latest = token(partial ? 30 : DEFINITIONS.length, 0).split('.')[3];
    const inWindow = BigInt(source.sequence) >= BigInt(oldest) && BigInt(source.sequence) <= BigInt(latest);
    const candidates = rows.filter(item => item !== source && BigInt(item.sequence) >= BigInt(oldest) && BigInt(item.sequence) <= BigInt(latest));
    const included = candidates.filter(item => !inScopeOnly || item.in_scope), groups = new Map();
    for (const row of included) { if (!groups.has(row.origin)) groups.set(row.origin, []); groups.get(row.origin).push(row); }
    const origins = [...groups.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([origin, values]) => ({ origin, in_scope: values[0].in_scope, matching_rows: values.length,
      first_observed_at: FIRST, last_observed_at: FIRST, samples: values.slice().sort((a, b) => BigInt(a.sequence) < BigInt(b.sequence) ? 1 : -1).map(({ evidence_id, sequence, started_at, observed_at }) => ({ evidence_id, sequence, started_at, observed_at })), omitted_samples: 0 }));
    // Peer inspection is an independent retained-metadata projection. Its extra
    // source is deliberately absent from default aggregates, so product summary
    // joins must use exact loaded origins and cannot invent a peer-only product.
    const retained = partial ? 20000 : 355, considered = Math.min(retained, PEER_LIMITS.max_records);
    Object.assign(counts, { not_recorded_rows: considered - (inWindow ? 1 : 0) - candidates.length, scope_excluded_rows: candidates.length - included.length,
      matching_rows: included.length, returned_matching_rows: included.length, sample_rows: included.length });
    const reasons = partial ? ['max_records', ...(!inWindow ? ['source_outside_window'] : [])] : [];
    return { ...payload, available: true, status: included.length ? 'matched' : 'no_matches', complete: !reasons.length, reasons,
      peer: { source: 'http_response_remote_addr', route: 'direct', address_family: 'ipv4', address: { ip: '192.0.2.10', port: 443 } },
      source: { evidence_id: id, sequence: source.sequence, origin: source.origin, started_at: source.started_at, observed_at: source.observed_at, in_scope: source.in_scope, in_window: inWindow },
      snapshot: { id: PEER_SNAPSHOT, generation: GENERATION, captured_at: LAST, window_order: 'latest_retained_sequence', retained_rows: retained, considered_rows: considered,
        oldest_considered_sequence: oldest, latest_considered_sequence: latest }, origins };
  }
  // Data factories also run offline in Node for normalizer checks, without a DOM,
  // network adapter, browser globals, server or application bootstrap.
  const data = { buildSnapshot, buildEvidence, buildPeerMatches, peerRows, peerObservation, rowObservation, token, SESSION, GENERATION, A, REFERENCE_ONLY, PEER_ONLY, LONG_ORIGIN, LONG_VERSION, LONG_METHOD, SCENARIOS };
  if (typeof document === 'undefined') { if (typeof module !== 'undefined') module.exports = data; return; }

  const $ = id => document.getElementById(id);
  const requests = [], held = [], stats = { map: 0, evidence: 0, matches: 0, unexpected: 0, blockedConnections: 0, cspBlocks: 0, expectedFontBlocks: 0, unexpectedCspBlocks: 0 };
  let controller, scenario = 'base', view = 'service-map';
  const snapshot = () => buildSnapshot(scenario, controller?.getState().scopeOnly === true), evidence = id => buildEvidence(id, scenario);
  function diagnostics() {
    const state = controller?.getState();
    if (!state) return;
    $('fixtureCounters').textContent = `Stub map reads: ${stats.map} · evidence reads: ${stats.evidence} · peer reads: ${stats.matches} · unexpected: ${stats.unexpected} · connection attempts: ${stats.blockedConnections} · CSP blocks: ${stats.cspBlocks} (${stats.expectedFontBlocks} expected local font; ${stats.unexpectedCspBlocks} unexpected) · held: ${held.length}`;
    $('fixtureSelection').textContent = `Snapshot: ${scenario} · view: ${state.view} · product: ${state.productId || 'Any'} · version: ${state.productVersion || 'Any'} · origin: ${state.selectedOrigin || 'none'} · inspector page: ${state.detailPage + 1} · catalog page: ${state.page + 1} · endpoint: ${state.endpointSelection ? state.endpointSelection.method + ' ' + state.endpointSelection.path : 'none'} · exact evidence: ${state.evidence ? state.evidence.sequence : 'none'}`;
  }
  function request(path, options = {}) {
    const url = new URL(path, 'http://example.com');
    const kind = url.pathname === '/api/target/service-map' ? 'map' : url.pathname === '/api/target/service-map/evidence' ? 'evidence' : url.pathname === '/api/target/service-map/socket-peer-matches' ? 'matches' : 'unexpected';
    if (kind === 'unexpected' || url.origin !== 'http://example.com' || url.searchParams.get('session_id') !== SESSION || options.redirect !== 'error' || options.method && options.method !== 'GET') {
      stats.unexpected++; diagnostics(); throw new Error('Unexpected stub request blocked');
    }
    stats[kind]++;
    const entry = { kind, path, scenario, signal: options.signal }; requests.push(entry);
    const li = document.createElement('li'); li.textContent = `${kind}: GET ${path} (${scenario})`; $('fixtureRequests').append(li);
    const payload = clone(kind === 'map' ? buildSnapshot(scenario, url.searchParams.get('in_scope_only') === 'true') : kind === 'evidence' ? evidence(url.searchParams.get('evidence_id')) : buildPeerMatches(url.searchParams.get('evidence_id'), scenario, url.searchParams.get('in_scope_only') === 'true'));
    const response = { ok: true, status: 200, json: async () => clone(payload) };
    if ($('fixtureHold').checked && $('fixtureHoldKind').value === kind) {
      $('fixtureHold').checked = false;
      // Intentionally release even an aborted read: stale-result guards belong
      // to the real controller and need to be exercised by this inert fixture.
      return new Promise(resolve => { held.push(() => { entry.releasedAfterAbort = !!entry.signal?.aborted; resolve(response); }); diagnostics(); });
    }
    diagnostics(); return Promise.resolve(response);
  }
  const rejectConnection = () => { stats.blockedConnections++; diagnostics(); throw new Error('Network disabled in this synthetic fixture'); };
  window.fetch = rejectConnection;
  window.XMLHttpRequest = class { constructor() { rejectConnection(); } };
  window.WebSocket = class { constructor() { rejectConnection(); } };
  window.EventSource = class { constructor() { rejectConnection(); } };
  if (navigator.sendBeacon) navigator.sendBeacon = rejectConnection;
  document.addEventListener('securitypolicyviolation', event => {
    stats.cspBlocks++;
    const expectedFont = event.effectiveDirective === 'font-src' && event.blockedURI === new URL('/fonts/Bungee-Regular.ttf', window.location.href).href;
    if (expectedFont) stats.expectedFontBlocks++; else stats.unexpectedCspBlocks++;
    diagnostics();
  });
  const ids = { panel: 'targetServiceMapPanel', status: 'serviceMapStatus', refresh: 'serviceMapRefresh', search: 'serviceMapSearch', scope: 'serviceMapScopeOnly', productFilters: 'serviceMapProductFilters',
    mapControls: 'serviceMapViewControls', allOrigins: 'serviceMapAllOrigins', directRelations: 'serviceMapDirectRelations', focusSelection: 'serviceMapFocusSelection',
    canvas: 'serviceMapCanvas', viewport: 'serviceMapViewport', zoomControls: 'serviceMapZoomControls', zoomFit: 'serviceMapZoomFit', zoomOut: 'serviceMapZoomOut', zoomReset: 'serviceMapZoomReset', zoomIn: 'serviceMapZoomIn',
    graphBounds: 'serviceMapGraphBounds', boundsDetails: 'serviceMapBoundsDetails', graphDiagnostics: 'serviceMapGraphDiagnostics', overlayStatus: 'serviceMapBodyOverlay', detail: 'serviceMapDetail', content: 'serviceMapContent', catalog: 'serviceMapApiCatalog' };
  const on = (id, fn) => $(id).addEventListener('click', () => Promise.resolve().then(fn).then(diagnostics).catch(error => { $('fixtureMessage').textContent = error.message; }));
  function setView(next) { view = next; controller.setVisible(true, view); diagnostics(); }
  async function refreshScenario() { scenario = $('fixtureScenario').value; await controller.refresh({ force: true }); diagnostics(); }
  const releaseHeld = () => { held.splice(0).forEach(resolve => resolve()); diagnostics(); };
  async function reset() {
    releaseHeld(); scenario = 'base'; $('fixtureScenario').value = scenario; $('fixtureHold').checked = false;
    controller.setVisible(false, view);
    $('serviceMapSearch').value = ''; $('serviceMapSearch').dispatchEvent(new Event('input'));
    $('serviceMapScopeOnly').checked = false; $('serviceMapScopeOnly').dispatchEvent(new Event('change'));
    controller.reset(); setView('service-map'); await controller.refresh(); controller.selectOrigin(A); diagnostics();
  }
  controller = window.SniperServiceMap.createController({ elements: Object.fromEntries(Object.entries(ids).map(([key, id]) => [key, $(id)])), getSessionId: () => SESSION, document, request });
  controller.setViewChange(setView);
  on('fixtureMap', () => setView('service-map')); on('fixtureCatalog', () => setView('apis'));
  on('fixtureRefresh', refreshScenario); on('fixtureReset', reset); on('fixtureRelease', releaseHeld); on('fixtureInvalidate', () => controller.invalidate());
  on('fixtureLongOrigin', () => { controller.selectOrigin(LONG_ORIGIN); diagnostics(); });
  $('fixtureTextSize').addEventListener('change', () => { document.documentElement.style.setProperty('--ui-root-size', $('fixtureTextSize').value + 'px'); controller.displayChanged(); diagnostics(); });
  const observer = new MutationObserver(diagnostics); observer.observe($('targetServiceMapPanel'), { childList: true, subtree: true });
  $('targetServiceMapPanel').addEventListener('input', diagnostics); $('targetServiceMapPanel').addEventListener('change', diagnostics);
  window.reportedProductsFixture = { ...data, controller, stats, requests, snapshot, evidence, setView, reset, refreshScenario, releaseHeld, get heldCount() { return held.length; }, get scenario() { return scenario; } };
  reset().catch(error => { $('fixtureMessage').textContent = error.message; });
})();
