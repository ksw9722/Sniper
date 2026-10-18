/* Synthetic passive source-lifetime QA. No app bootstrap, live API, or external data. */
(function (root) {
  "use strict";
  const SESSIONS = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222"];
  const GENERATIONS = ["33333333-3333-4333-8333-333333333333", "44444444-4444-4444-8444-444444444444"];
  const RECORD = "55555555-5555-4555-8555-555555555555", REVISION = "66666666-6666-4666-8666-666666666666", SNAPSHOT = "77777777-7777-4777-8777-777777777777";
  const A = "https://source-a.example.com", B = "https://source-b.example.com", PEER = "https://peer.example.com";
  const BODY_TARGET = "https://body-reference.example.com", LINK_TARGET = "https://link-reference.example.com";
  const START = "2026-10-12T18:00:00Z", OBSERVED = "2026-10-12T18:00:00.123456789Z", END = "2026-10-12T18:01:00Z";
  const BASE = 9007199254740993n;
  const AUTHORITATIVE = ["invalid_evidence_id", "session_mismatch", "stale_generation", "record_missing", "record_changed", "session_source_changed"];
  const COMMON_REASONS = [...AUTHORITATIVE, "inactive_session"];
  const OPTIONAL_REASONS = { body: ["response_unavailable", "body_identity_unverifiable", "body_read_failed", "metadata_unavailable"], link: ["response_unavailable", "metadata_unavailable", "link_inspection_failed"], html: ["response_unavailable", "body_identity_unverifiable", "body_read_failed", "metadata_unavailable"], peer: ["observation_unavailable", "scope_work_limit", "peer_inspection_failed", "metadata_unavailable"] };
  const BODY_LIMITS = { scan_bytes: 262144, serialized_row_bytes: 2097152, origins: 100, candidates: 4096, candidate_bytes: 4096, offsets_per_origin: 5 };
  const LINK_LIMITS = { headers: 128, header_bytes: 32768, link_field_bytes: 4096, target_bytes: 4096, candidates: 128, parameters_per_member: 32, relation_tokens_per_member: 32, observations: 32, target_origin_bytes: 268 };
  const HTML_LIMITS = { scan_bytes: 262144, serialized_row_bytes: 2097152, tag_candidates: 4096, tag_bytes: 4096, attributes: 32, observations: 8, version_bytes: 32, version_separator_bytes: 32 };
  const PEER_LIMITS = { max_records: 10000, origin_groups: 100, samples_per_origin: 3, projection_bytes: 262144, host_bytes: 512, scheme_bytes: 8, scope_pattern_entries: 500, scope_pattern_bytes: 512, scope_work_units: 8388608 };
  const PEER_COUNT_KEYS = ["non_http_rows", "not_recorded_rows", "unavailable_observation_rows", "non_direct_rows", "invalid_observation_rows", "different_peer_rows", "invalid_metadata_rows", "scope_excluded_rows", "matching_rows", "returned_matching_rows", "omitted_matching_rows", "sample_rows", "omitted_sample_rows"];
  const LINK_COUNT_KEYS = ["headers_inspected", "header_bytes_admitted", "link_fields_inspected", "candidates_inspected", "members_framed", "parameters_inspected", "relation_tokens_inspected", "unknown_relation_tokens_inspected"];
  const defaults = { session: SESSIONS[0], generation: GENERATIONS[0], technology: "declared" };
  const clone = value => JSON.parse(JSON.stringify(value));
  const zero = keys => Object.fromEntries(keys.map(key => [key, 0]));
  const token = (index = 0, context = defaults) => `v1.${context.session}.${context.generation}.${BASE + BigInt(index)}.${REVISION}.${RECORD}`;
  const rows = (context = defaults) => [A, B, PEER].map((origin, index) => ({ evidence_id: token(index, context), sequence: String(BASE + BigInt(index)), origin, started_at: START, observed_at: OBSERVED, in_scope: true }));
  const peerObservation = () => ({ status: "observed", observation: { source: "http_response_remote_addr", route: "direct", observed_at: OBSERVED, address: { ip: "192.0.2.44", port: 443 }, reason: null } });
  const rowTechnology = () => ({ product_id: "nginx", name: "nginx", catalog_category: "web_server", reported_version: "1.26.2", version_status: "reported", header_name: "server", source: "response_header", strength: "literal_declaration", header_index: 0, product_span: { start: 0, end: 5 }, version_span: { start: 6, end: 12 }, offset_basis: "retained_header_value_utf8", rule_id: "server.nginx.v1" });
  function mapPayload(scopeOnly = false, context = defaults) {
    return { schema_version: 1, session_id: context.session, technology_catalog_version: 2,
      snapshot: { id: SNAPSHOT, generation: context.generation, complete: true, stopped_by: [], limits: { records: 10000 }, considered_rows: 3, retained_rows: 3, included_rows: 3, excluded_rows: 0, omitted_rows: 0, oldest_considered_at: START, latest_considered_at: END, in_scope_only: scopeOnly },
      origins: rows(context).map(row => ({ origin: row.origin, in_scope: true, request_count: 1, first_seen: START, last_seen: START, endpoints: [{ method: "GET", path: "/synthetic-source", path_group: "/synthetic-source", request_count: 1, statuses: [200], content_types: ["text/html"], evidence: [row.evidence_id] }], technology_clues: [], technology_observations: [], redirects: [], header_links: [] })), unavailable: ["initiator", "remote_ip", "tls_certificate"], deferred: ["response_body_references"], warnings: [] };
  }
  function evidencePayload(id = token(), context = defaults) {
    const item = rows(context).find(row => row.evidence_id === id);
    const row = item ? { evidence_id: id, origin: item.origin, method: "GET", path: "/synthetic-source", sequence: item.sequence, record_id: RECORD, status: 200, started_at: START, metadata_complete: true, omitted: [], technology_clues: [], technology_observations: context.technology === "declared" ? [rowTechnology()] : [], header_links: [], redirect: null } : null;
    if (row && context.technology === "absent") delete row.technology_observations;
    return { schema_version: 1, session_id: context.session, evidence_id: id, available: !!row, technology_catalog_version: 2, source: "captured_http_metadata", row, socket_peer: row ? peerObservation() : { status: "not_recorded", observation: null } };
  }
  function bodyPayload(id = token(), reason = null, context = defaults) {
    const row = evidencePayload(id, context).row;
    if (!row) throw new Error("Unknown synthetic source token");
    return { schema_version: 1, session_id: context.session, evidence_id: id, available: !reason, status: reason ? "unavailable" : "scanned", complete: !reason, reasons: reason ? [reason] : [], source_origin: reason ? null : row.origin, source_sequence: reason ? null : row.sequence, source_started_at: reason ? null : START,
      retained_preview_bytes: reason ? null : 500, scanned_bytes: reason ? 0 : 500, capture_preview_truncated: false, scan_prefix_truncated: false, candidates_scanned: reason ? 0 : 1, omitted_reference_occurrences: 0,
      references: reason ? [] : [{ target_origin: BODY_TARGET, occurrences: 1, preview_byte_offsets: [40], omitted_offsets: 0 }], limits: { ...BODY_LIMITS }, coverage: "literal_absolute_http_urls", offset_basis: "retained_utf8_preview" };
  }
  function linkPayload(id = token(), reason = null, context = defaults) {
    const row = evidencePayload(id, context).row, target = `${LINK_TARGET}/synthetic-docs`, prefix = `<${target}>; rel="`, raw = `${prefix}service-doc"`;
    if (!row) throw new Error("Unknown synthetic source token");
    return { schema_version: 1, relation_catalog_version: 1, session_id: context.session, evidence_id: id, available: !reason, status: reason ? "unavailable" : "scanned", complete: !reason, reasons: reason ? [reason] : [], source_origin: reason ? null : row.origin, source_sequence: reason ? null : row.sequence, source_started_at: reason ? null : START, source_method: reason ? null : "GET", source_status: reason ? null : 200, retained_header_count: reason ? null : 1,
      coverage: "absolute_http_link_declarations", offset_basis: "retained_header_value_utf8", counts: reason ? zero(LINK_COUNT_KEYS) : { headers_inspected: 1, header_bytes_admitted: raw.length + 4, link_fields_inspected: 1, candidates_inspected: 1, members_framed: 1, parameters_inspected: 1, relation_tokens_inspected: 1, unknown_relation_tokens_inspected: 0 },
      observations: reason ? [] : [{ relation: "service-doc", target_origin: LINK_TARGET, source: "response_link_header", strength: "literal_declaration", header_name: "link", header_index: 0, header_value_bytes: raw.length, link_value_span: { start: 0, end: raw.length }, target_span: { start: 1, end: target.length + 1 }, relation_span: { start: prefix.length, end: prefix.length + 11 }, rule_id: "response_link.service_doc.v1", offset_basis: "retained_header_value_utf8" }], limits: { ...LINK_LIMITS } };
  }
  function htmlPayload(id = token(), reason = null, context = defaults) {
    const row = evidencePayload(id, context).row;
    if (!row) throw new Error("Unknown synthetic source token");
    return { schema_version: 1, generator_catalog_version: 1, session_id: context.session, evidence_id: id, available: !reason, status: reason ? "unavailable" : "scanned", complete: !reason, reasons: reason ? [reason] : [], source_origin: reason ? null : row.origin, source_sequence: reason ? null : row.sequence, source_started_at: reason ? null : START, retained_preview_bytes: reason ? null : 100, scan_prefix_bytes: reason ? 0 : 100, capture_preview_truncated: false, scan_prefix_truncated: false,
      coverage: "literal_html_generator_explicit_head", head_closed: !reason, parser_stop: null, tag_candidates_inspected: reason ? 0 : 4, additional_tag_candidate_encountered: false, additional_observation_encountered: false, unknown_product_declarations: 0,
      observations: reason ? [] : [{ product_id: "wordpress", name: "WordPress", catalog_category: "cms", reported_version: "6.8.1", version_status: "reported", source: "response_html_meta_generator", strength: "literal_declaration", rule_id: "html_meta_generator.wordpress.v1", tag_span: { start: 12, end: 61 }, content_span: { start: 44, end: 59 }, product_span: { start: 44, end: 53 }, version_span: { start: 54, end: 59 }, offset_basis: "retained_utf8_preview" }], limits: { ...HTML_LIMITS }, offset_basis: "retained_utf8_preview" };
  }
  function peerPayload(id = token(), reason = null, context = defaults, scopeOnly = false) {
    const source = rows(context).find(row => row.evidence_id === id), others = rows(context).filter(row => row.evidence_id !== id);
    if (!source) throw new Error("Unknown synthetic source token");
    return { schema_version: 1, session_id: context.session, evidence_id: id, available: !reason, status: reason ? "unavailable" : "matched", complete: !reason, reasons: reason ? [reason] : [], in_scope_only: scopeOnly, coverage: "retained_direct_socket_peer_equality",
      peer: reason ? null : { source: "http_response_remote_addr", route: "direct", address_family: "ipv4", address: { ip: "192.0.2.44", port: 443 } }, source: reason ? null : { ...source, in_window: true },
      snapshot: reason ? null : { id: SNAPSHOT, generation: context.generation, captured_at: END, window_order: "latest_retained_sequence", retained_rows: 3, considered_rows: 3, oldest_considered_sequence: String(BASE), latest_considered_sequence: String(BASE + 2n) },
      counts: reason ? zero(PEER_COUNT_KEYS) : { ...zero(PEER_COUNT_KEYS), matching_rows: 2, returned_matching_rows: 2, sample_rows: 2 },
      origins: reason ? [] : others.sort((a, b) => a.origin < b.origin ? -1 : 1).map(row => ({ origin: row.origin, in_scope: true, matching_rows: 1, first_observed_at: OBSERVED, last_observed_at: OBSERVED, samples: [{ evidence_id: row.evidence_id, sequence: row.sequence, started_at: START, observed_at: OBSERVED }], omitted_samples: 0 })), limits: { ...PEER_LIMITS } };
  }
  const factories = { body: bodyPayload, link: linkPayload, html: htmlPayload, peer: peerPayload };
  function validateData(map) {
    let accepted = 0;
    for (const session of SESSIONS) for (const generation of GENERATIONS) for (const index of [0, 1]) {
      const context = { ...defaults, session, generation }, id = token(index, context), row = evidencePayload(id, context).row;
      map.normalizeSnapshot(mapPayload(false, context), session); map.normalizeRowTechnologies(row.technology_observations, 2);
      for (const kind of Object.keys(factories)) for (const reason of [null, ...COMMON_REASONS, ...OPTIONAL_REASONS[kind]]) {
        const payload = factories[kind](id, reason, context, false);
        if (kind === "peer") map.normalizePeerMatches(payload, session, id, row, map.normalizeSocketPeer(peerObservation()), false, generation);
        else map[{ body: "normalizeBodyReferences", link: "normalizeLinkDeclarations", html: "normalizeHtmlGenerators" }[kind]](payload, session, id, row);
        accepted++;
      }
    }
    return accepted;
  }
  const data = { SESSIONS, GENERATIONS, AUTHORITATIVE, COMMON_REASONS, OPTIONAL_REASONS, A, B, PEER, BODY_TARGET, LINK_TARGET, defaults, token, rows, mapPayload, evidencePayload, bodyPayload, linkPayload, htmlPayload, peerPayload, peerObservation, factories, validateData };
  if (typeof document === "undefined") { if (typeof module !== "undefined") module.exports = data; return; }

  const $ = id => document.getElementById(id);
  const requests = [], held = [], nextReasons = new Map();
  const stats = { map: 0, evidence: 0, body: 0, link: 0, html: 0, peer: 0, unexpected: 0, blockedConnections: 0, cspBlocks: 0, expectedFontBlocks: 0, unexpectedCspBlocks: 0 };
  let controller, sessionIndex = 0, generationIndex = 0, eventReady = true, heldSequence = 0;
  const context = () => ({ session: SESSIONS[sessionIndex], generation: GENERATIONS[generationIndex], technology: $("fixtureTechnology").value });
  const currentToken = index => token(index, context());
  function observations() {
    const state = controller?.getState();
    if (!state) return {};
    const actions = [...$("serviceMapDetail").querySelectorAll("button[data-service-body-references], button[data-service-link-declarations], button[data-service-html-generators], button[data-service-peer-matches], button[data-service-show-body-references], button[data-service-show-link-declarations], button[data-service-show-peer-matches]")].map(button => ({ text: button.textContent.trim(), disabled: button.disabled }));
    return { session: state.sessionId, generation: state.snapshot?.snapshot.generation, selectedOrigin: state.selectedOrigin, evidence: state.evidence?.origin || null, evidenceId: state.evidenceId, socketPeer: state.socketPeer?.status || null,
      inactive: state.inactiveSessionId === context().session, inactiveSessionId: state.inactiveSessionId || null, overlay: state.bodyReferenceOverlay?.kind || null, overlaySource: state.bodyReferenceOverlay?.source.origin || null,
      body: state.bodyReferences?.status || null, link: state.linkDeclarations?.status || null, html: state.htmlGenerators?.status || null, peer: state.peerMatches?.status || null,
      loading: { body: state.bodyReferencesLoading, html: state.htmlGeneratorsLoading, peer: state.peerMatchesLoading }, evidenceError: state.evidenceError, actions,
      detailText: $("serviceMapDetail").textContent, held: held.map(item => ({ id: item.id, kind: item.kind, reason: item.reason, source: item.source, aborted: !!item.signal?.aborted })) };
  }
  function diagnostics() {
    if (!controller) return;
    const seen = observations();
    $("fixtureCounters").textContent = `Stub GETs ${requests.length} · map ${stats.map} · exact ${stats.evidence} · body ${stats.body} · Link ${stats.link} · HTML ${stats.html} · peer ${stats.peer} · unexpected ${stats.unexpected} · connections ${stats.blockedConnections} · CSP ${stats.cspBlocks} (${stats.expectedFontBlocks} expected font, ${stats.unexpectedCspBlocks} unexpected)`;
    $("fixtureEvidenceState").textContent = `Verified source: ${seen.evidence || "none"} · socket peer: ${seen.socketPeer || "none"} · overlay: ${seen.overlay || "none"} (${seen.overlaySource || "no source"}) · inactive-session latch: ${seen.inactive ? "BLOCKED" : "clear"}`;
    $("fixtureSupplementState").textContent = `Supplement results · body: ${seen.body || "none"} · Link: ${seen.link || "none"} · HTML: ${seen.html || "none"} · peer: ${seen.peer || "none"} · pending: ${Object.entries(seen.loading).filter(([, yes]) => yes).map(([kind]) => kind).join(", ") || "none"} · armed: ${[...nextReasons].map(([kind, reason]) => `${kind}/${reason}`).join(", ") || "none"}`;
    $("fixtureActionState").textContent = `Production buttons: ${seen.actions.map(item => `${item.disabled ? "disabled" : "enabled"}: ${item.text}`).join(" · ") || "no active-only buttons shown"}`;
    $("fixtureHeldState").textContent = `Held responses: ${seen.held.map(item => `#${item.id} ${item.kind}/${item.reason || "success"} from ${item.source}${item.aborted ? " [aborted]" : ""}`).join(" · ") || "none"}`;
    $("fixtureIdentity").textContent = `Current session ${context().session}; generation ${context().generation}. Source A token ${currentToken(0)}. Source B token ${currentToken(1)}. The three rows deliberately share one UUID with distinct string sequences above 2^53.`;
  }
  function note(text) { $("fixtureResult").textContent = text; diagnostics(); }
  function rejectConnection() { stats.blockedConnections++; diagnostics(); throw new Error("Network disabled in this synthetic fixture"); }
  root.fetch = rejectConnection;
  root.XMLHttpRequest = class { constructor() { rejectConnection(); } };
  root.WebSocket = root.EventSource = class { constructor() { rejectConnection(); } };
  if (navigator.sendBeacon) navigator.sendBeacon = rejectConnection;
  document.addEventListener("securitypolicyviolation", event => { stats.cspBlocks++; if (event.effectiveDirective === "font-src" && event.blockedURI === new URL("/fonts/Bungee-Regular.ttf", root.location.href).href) stats.expectedFontBlocks++; else stats.unexpectedCspBlocks++; diagnostics(); });
  function request(path, options = {}) {
    const parsed = new URL(path, "http://example.com"), suffix = parsed.pathname.replace("/api/target/service-map", "");
    const kind = { "": "map", "/evidence": "evidence", "/body-references": "body", "/link-declarations": "link", "/html-generators": "html", "/socket-peer-matches": "peer" }[suffix];
    if (!path.startsWith("/api/target/service-map") || !kind || parsed.origin !== "http://example.com" || parsed.searchParams.get("session_id") !== context().session || options.redirect !== "error" || options.method && options.method !== "GET") { stats.unexpected++; diagnostics(); throw new Error("Unexpected fixture request blocked"); }
    const id = parsed.searchParams.get("evidence_id"), scope = parsed.searchParams.get("in_scope_only") === "true", reason = nextReasons.get(kind) || null;
    nextReasons.delete(kind); stats[kind]++;
    const payload = clone(kind === "map" ? mapPayload(scope, context()) : kind === "evidence" ? evidencePayload(id, context()) : factories[kind](id, reason, context(), scope));
    const entry = { kind, path, reason, source: id ? rows(context()).find(row => row.evidence_id === id)?.origin : null, signal: options.signal }; requests.push(entry);
    const line = document.createElement("li"); line.textContent = `${requests.length}. ${kind} ${reason || "success"}: GET ${path}`; $("fixtureRequests").append(line);
    const response = { ok: true, status: 200, json: async () => clone(payload) };
    if ($("fixtureHold").checked && $("fixtureHoldKind").value === kind) {
      $("fixtureHold").checked = false;
      // Ignore abort only in this stub: the production current-source guard must
      // reject an old result even when a transport completes after cancellation.
      return new Promise(resolve => { held.push({ ...entry, id: ++heldSequence, release: () => { entry.releasedAfterAbort = !!entry.signal?.aborted; resolve(response); } }); diagnostics(); });
    }
    diagnostics(); return Promise.resolve(response);
  }
  const ids = { panel: "targetServiceMapPanel", status: "serviceMapStatus", refresh: "serviceMapRefresh", search: "serviceMapSearch", scope: "serviceMapScopeOnly", productFilters: "serviceMapProductFilters", mapControls: "serviceMapViewControls", allOrigins: "serviceMapAllOrigins", directRelations: "serviceMapDirectRelations", focusSelection: "serviceMapFocusSelection", canvas: "serviceMapCanvas", viewport: "serviceMapViewport", zoomControls: "serviceMapZoomControls", zoomFit: "serviceMapZoomFit", zoomOut: "serviceMapZoomOut", zoomReset: "serviceMapZoomReset", zoomIn: "serviceMapZoomIn", graphBounds: "serviceMapGraphBounds", boundsDetails: "serviceMapBoundsDetails", graphDiagnostics: "serviceMapGraphDiagnostics", overlayStatus: "serviceMapBodyOverlay", detail: "serviceMapDetail", content: "serviceMapContent", catalog: "serviceMapApiCatalog" };
  const on = (id, action) => $(id).addEventListener("click", () => Promise.resolve().then(action).then(diagnostics).catch(error => note(`Fixture error: ${error.message}`)));
  const releaseHeld = (kind = null) => { const released = held.filter(item => !kind || item.kind === kind); for (const item of released) { held.splice(held.indexOf(item), 1); item.release(); } diagnostics(); return released.length; };
  async function inspectSource(index = 0) { controller.selectOrigin([A, B, PEER][index]); const ok = await controller.loadEvidence(currentToken(index)); diagnostics(); return ok; }
  function arm(kind = $("fixtureKind").value, reason = $("fixtureReason").value) {
    if (![...COMMON_REASONS, ...OPTIONAL_REASONS[kind]].includes(reason)) throw new Error("Reason is not valid for this inspection type");
    nextReasons.set(kind, reason); note(`Armed ${kind}: ${reason}. No read was added. Run the selected inspection explicitly.`);
  }
  async function inspect(kind = $("fixtureKind").value) {
    if (kind === "body" || kind === "link") controller.setRelationshipKind(kind);
    const before = requests.length, result = await controller[{ body: "loadBodyReferences", link: "loadLinkDeclarations", html: "loadHtmlGenerators", peer: "loadPeerMatches" }[kind]]();
    note(`${kind} inspection ${requests.length === before ? "was blocked before any stub read" : "completed one explicit stub read"}. Check the source, overlay, and production button diagnostics.`); return result;
  }
  function show(kind = $("fixtureOverlay").value) {
    const result = controller[{ body: "showBodyReferencesOnMap", link: "showLinkDeclarationsOnMap", peer: "showPeerMatchesOnMap" }[kind]]();
    note(result ? `Shown ${kind} overlay locally; no request added.` : `${kind} overlay was not shown; inspect it first and check active-only availability.`); return result;
  }
  async function switchSession() { sessionIndex = 1 - sessionIndex; controller.reset(); await controller.refresh({ force: true }); controller.selectOrigin(A); diagnostics(); }
  async function switchAwayBack() { await switchSession(); await switchSession(); note("Switched away and back through actual controller.reset calls. Explicit source inspection is available again."); }
  async function fresh() {
    nextReasons.clear(); $("fixtureHold").checked = false; eventReady = true;
    // These are genuine selected-session changes, not a test-only latch override.
    await switchAwayBack(); releaseHeld(); await Promise.resolve();
    $("serviceMapSearch").value = ""; $("serviceMapSearch").dispatchEvent(new Event("input"));
    note("Fresh session context. Inspect source A or B, then load and explicitly show a supplement.");
  }
  async function prepare(overlayKind = $("fixtureOverlay").value) {
    await fresh(); await inspectSource(0); await inspect(overlayKind); if (!show(overlayKind)) throw new Error("Could not prepare the requested overlay");
    if (overlayKind !== "peer") await inspect("peer");
    await inspect("html"); note(`Source A verified with a shown ${overlayKind} overlay and supplemental results. Arm one unavailable reason, then run its inspection.`);
  }
  async function delayedSourceA() {
    await prepare(); const kind = $("fixtureKind").value, reason = $("fixtureReason").value;
    arm(kind, reason); $("fixtureHoldKind").value = kind; $("fixtureHold").checked = true;
    const task = inspect(kind); await Promise.resolve(); await inspectSource(1);
    note("Source A's result is held; source B is now verified. Release held responses: B and unrelated pinned A overlay must remain."); return { task, kind };
  }
  async function siblingRace() {
    await prepare(); const failureKind = $("fixtureKind").value, reason = $("fixtureReason").value, siblingKind = failureKind === "html" ? "peer" : "html";
    $("fixtureHoldKind").value = siblingKind; $("fixtureHold").checked = true;
    const sibling = inspect(siblingKind); await Promise.resolve(); arm(failureKind, reason); await inspect(failureKind);
    note(`Held ${siblingKind} success is a concurrent same-source sibling of ${failureKind}/${reason}. Release it now; a revoked source or inactive session must not regain supplements.`); return { sibling, siblingKind };
  }
  async function changeGeneration() { generationIndex = 1 - generationIndex; await controller.refresh({ force: true }); note("Refreshed a new retained generation without resetting the selected session. An inactive-session latch must remain."); }
  async function reconnect() { eventReady = false; controller.disconnect(); eventReady = true; await controller.refresh({ force: true }); note("Disconnected and reconnected with a map refresh. This does not reset the inactive-session latch."); }
  function updateReasons() {
    const kind = $("fixtureKind").value, previous = $("fixtureReason").value;
    $("fixtureReason").replaceChildren(...[...COMMON_REASONS, ...OPTIONAL_REASONS[kind]].map(reason => { const option = document.createElement("option"); option.value = reason; option.textContent = reason; return option; }));
    if ([...COMMON_REASONS, ...OPTIONAL_REASONS[kind]].includes(previous)) $("fixtureReason").value = previous;
  }
  try {
    controller = root.SniperServiceMap.createController({ elements: Object.fromEntries(Object.entries(ids).map(([key, id]) => [key, $(id)])), getSessionId: () => context().session, getEventStreamReady: () => eventReady, document, request });
    controller.setViewChange(view => controller.setVisible(true, view));
    on("fixtureFresh", fresh); on("fixtureSourceA", () => inspectSource(0)); on("fixtureSourceB", () => inspectSource(1));
    on("fixturePrepare", () => prepare()); on("fixtureArm", () => arm()); on("fixtureInspect", () => inspect()); on("fixtureShow", () => show());
    on("fixtureClear", () => { controller.clearBodyReferenceOverlay(); note("Temporary overlay cleared locally."); });
    on("fixtureRelease", () => note(`Released ${releaseHeld()} held responses, including aborted ones.`));
    on("fixtureDelayedA", delayedSourceA); on("fixtureSibling", siblingRace); on("fixtureGeneration", changeGeneration); on("fixtureReconnect", reconnect);
    on("fixtureSession", async () => { await switchSession(); note("Selected session changed through controller.reset. Inspect an ordinary source before requesting supplements."); });
    on("fixtureAwayBack", switchAwayBack);
    on("fixtureRefresh", async () => { await controller.refresh({ force: true }); note("Same-session refresh completed. It must not release an inactive-session latch."); });
    on("fixtureSelection", async () => { await inspectSource(1); await inspectSource(0); note("Selected and verified B, then A. Ordinary evidence remains usable; inactive-only supplementary actions must stay blocked."); });
    on("fixtureScope", async () => { await controller.scopeChanged({ scope_patterns: ["*.example.com"] }, { scope_patterns: ["source-a.example.com"] }); note("Simulated a scope-rule revision and map reload. No real settings changed; an inactive-session latch must remain."); });
    on("fixtureValidate", () => note(`PASS: ${validateData(root.SniperServiceMap)} valid supplementary DTO variants normalized across two sessions, two generations, and sources A/B. No reads added.`));
    $("fixtureKind").addEventListener("change", updateReasons); $("fixtureTechnology").addEventListener("change", () => note("Exact-row technology field mode changed for the next explicit evidence read. Reinspect A or B; map aggregates stay independently empty."));
    new MutationObserver(diagnostics).observe($("targetServiceMapPanel"), { childList: true, subtree: true });
    root.sourceLifetimeFixture = { ...data, controller, stats, requests, context, currentToken, observations, diagnostics, arm, inspect, inspectSource, show, prepare, fresh, switchSession, switchAwayBack, changeGeneration, reconnect, delayedSourceA, siblingRace, releaseHeld, get heldCount() { return held.length; } };
    updateReasons(); controller.reset(); controller.setVisible(true); controller.refresh({ force: true }).then(() => { controller.selectOrigin(A); note("Ready. All data is synthetic; only the production controller and styles run. Prepare a shown overlay to begin."); }).catch(error => note(`Fixture error: ${error.message}`));
  } catch (error) { note(`Fixture cannot start: ${error.message}`); }
})(typeof globalThis !== "undefined" ? globalThis : this);
