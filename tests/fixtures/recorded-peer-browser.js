/* Synthetic retained metadata only. Never load app.js or use any real API. */
(function () {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const SESSIONS = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222"];
  const GENERATIONS = ["33333333-3333-4333-8333-333333333333", "44444444-4444-4444-8444-444444444444"];
  const RECORD = "55555555-5555-4555-8555-555555555555";
  const REVISION = "66666666-6666-4666-8666-666666666666";
  const SNAPSHOT = "77777777-7777-4777-8777-777777777777";
  const SOURCE = "https://app.example.com", ABSENT = "https://archived.example.com";
  const BASE = 9007199254740993n;
  const LIMITS = { max_records: 10000, origin_groups: 100, samples_per_origin: 3, projection_bytes: 262144, host_bytes: 512, scheme_bytes: 8, scope_pattern_entries: 500, scope_pattern_bytes: 512, scope_work_units: 8388608 };
  const COUNT_KEYS = ["non_http_rows", "not_recorded_rows", "unavailable_observation_rows", "non_direct_rows", "invalid_observation_rows", "different_peer_rows", "invalid_metadata_rows", "scope_excluded_rows", "matching_rows", "returned_matching_rows", "omitted_matching_rows", "sample_rows", "omitted_sample_rows"];
  const stats = { map: 0, evidence: 0, matches: 0, unexpected: 0, networkAttempts: 0, cspBlocks: 0 };
  const requests = [], held = [];
  let sessionIndex = 0, generationIndex = 0, eventReady = true, visible = true, scenario = "normal", nextUnavailable = null, controller;
  const session = () => SESSIONS[sessionIndex], generation = () => GENERATIONS[generationIndex];
  const token = (sequence) => `v1.${session()}.${generation()}.${sequence}.${REVISION}.${RECORD}`;
  const at = (index, observed = false) => `2026-10-12T01:${String(Math.floor(index / 60)).padStart(2, "0")}:${String(index % 60).padStart(2, "0")}${observed ? ".123456789" : ""}Z`;
  const sourceToken = () => token(String(BASE));
  const peerObservation = (index) => ({ status: "observed", observation: { source: "http_response_remote_addr", route: "direct", observed_at: at(index, true), address: { ip: "192.0.2.44", port: 443 }, reason: null } });
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const emptyCounts = () => Object.fromEntries(COUNT_KEYS.map((key) => [key, 0]));

  function fixtureRows(kind = scenario) {
    const rows = [{ evidence_id: sourceToken(), sequence: String(BASE), origin: SOURCE, started_at: at(0), observed_at: at(0, true), in_scope: true, index: 0 }];
    let origins = [SOURCE, "https://api.example.com", "https://api.example.com", ABSENT, "https://cdn.example.com"];
    if (kind === "many") origins = origins.concat(Array.from({ length: 30 }, (_, i) => `https://peer-${String(i + 1).padStart(2, "0")}.example.com`));
    if (kind === "partial") origins = [SOURCE, SOURCE, SOURCE, SOURCE, "https://api.example.com", ABSENT];
    if (kind === "none") origins = [];
    for (const [i, origin] of origins.entries()) {
      const index = i + 1, sequence = String(BASE + BigInt(index));
      rows.push({ evidence_id: token(sequence), sequence, origin, started_at: at(index), observed_at: at(index, true), in_scope: origin !== "https://cdn.example.com", index });
    }
    return rows;
  }
  function mapPayload(scopeOnly) {
    const rows = fixtureRows(), source = rows[0];
    const origin = (id, idToken, count) => ({ origin: id, in_scope: true, request_count: count, first_seen: at(0), last_seen: at(50),
      endpoints: [{ method: "GET", path: "/synthetic", path_group: "/synthetic", request_count: count, statuses: [200], content_types: ["application/json"], evidence: idToken ? [idToken] : [] }],
      technology_clues: [], technology_observations: [], redirects: [], header_links: [] });
    return { schema_version: 1, session_id: session(), technology_catalog_version: 2,
      snapshot: { id: SNAPSHOT, generation: generation(), complete: true, stopped_by: [], limits: { records: 10000 }, considered_rows: 12, retained_rows: 12, included_rows: 12, excluded_rows: 0, omitted_rows: 0, oldest_considered_at: at(0), latest_considered_at: at(50), in_scope_only: scopeOnly },
      origins: [origin(SOURCE, source.evidence_id, 4), origin("https://api.example.com", rows.find((row) => row.origin === "https://api.example.com")?.evidence_id, 5), origin("https://independent.example.com", null, 3)],
      unavailable: ["initiator", "remote_ip", "tls_certificate"], deferred: ["response_body_references"], warnings: [] };
  }
  function evidencePayload(id) {
    const item = fixtureRows().find((row) => row.evidence_id === id);
    const available = !!item && (!$("fixtureMissingSample").checked || id === sourceToken());
    return { schema_version: 1, session_id: session(), evidence_id: id, available, technology_catalog_version: 2, source: "captured_http_metadata",
      row: available ? { evidence_id: id, origin: item.origin, method: "GET", path: "/synthetic", sequence: item.sequence, record_id: RECORD, status: 200, started_at: item.started_at, metadata_complete: true, omitted: [], technology_clues: [], technology_observations: [], header_links: [], redirect: null } : null,
      socket_peer: available ? peerObservation(item.index) : { status: "not_recorded", observation: null } };
  }
  function matchesPayload(id, scopeOnly, kind = scenario) {
    const rows = fixtureRows(kind), source = rows.find((row) => row.evidence_id === id), counts = emptyCounts();
    const payload = { schema_version: 1, session_id: session(), evidence_id: id, available: false, status: "unavailable", complete: false, reasons: ["record_missing"], in_scope_only: scopeOnly,
      coverage: "retained_direct_socket_peer_equality", peer: null, source: null, snapshot: null, counts, origins: [], limits: { ...LIMITS } };
    if (!source || kind === "unavailable") return payload;
    if (kind === "scope_work_limit") { payload.reasons = ["scope_work_limit"]; return payload; }
    if (scopeOnly && !source.in_scope) { payload.reasons = ["source_out_of_scope"]; return payload; }
    const limitedWindow = kind === "partial";
    const candidates = rows.filter((row) => row !== source && (!limitedWindow || BigInt(row.sequence) > BASE)), included = candidates.filter((row) => !scopeOnly || row.in_scope);
    const groups = new Map();
    for (const row of included) { if (!groups.has(row.origin)) groups.set(row.origin, []); groups.get(row.origin).push(row); }
    const origins = [...groups.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([origin, values]) => ({ origin, in_scope: values[0].in_scope, matching_rows: values.length,
      first_observed_at: values[0].observed_at, last_observed_at: values.at(-1).observed_at,
      samples: [...values].reverse().slice(0, 3).map(({ evidence_id, sequence, started_at, observed_at }) => ({ evidence_id, sequence, started_at, observed_at })), omitted_samples: Math.max(0, values.length - 3) }));
    Object.assign(counts, { non_http_rows: 1, not_recorded_rows: 1, unavailable_observation_rows: 1, non_direct_rows: 1, invalid_observation_rows: 1, different_peer_rows: 2,
      scope_excluded_rows: candidates.length - included.length, matching_rows: included.length, returned_matching_rows: included.length,
      sample_rows: origins.reduce((sum, group) => sum + group.samples.length, 0), omitted_sample_rows: origins.reduce((sum, group) => sum + group.omitted_samples, 0) });
    const inWindow = !limitedWindow || source.index > 0, considered = limitedWindow ? 10000 : candidates.length + 8, reasons = [];
    if (limitedWindow) {
      reasons.push("max_records");
      counts.different_peer_rows += considered - candidates.length - 7 - (inWindow ? 1 : 0);
    }
    if (!inWindow) reasons.push("source_outside_window");
    if (counts.omitted_sample_rows) reasons.push("sample_limit");
    Object.assign(payload, { available: true, status: included.length ? "matched" : "no_matches", complete: reasons.length === 0, reasons: reasons.sort(),
      peer: { source: "http_response_remote_addr", route: "direct", address_family: "ipv4", address: { ip: "192.0.2.44", port: 443 } },
      source: { evidence_id: id, sequence: source.sequence, origin: source.origin, started_at: source.started_at, observed_at: source.observed_at, in_scope: source.in_scope, in_window: inWindow },
      snapshot: { id: SNAPSHOT, generation: generation(), captured_at: at(59), window_order: "latest_retained_sequence", retained_rows: considered + (limitedWindow ? 1 : 0), considered_rows: considered,
        oldest_considered_sequence: String(BASE + (limitedWindow ? 1n : 0n)), latest_considered_sequence: String(BASE + BigInt(limitedWindow ? considered : candidates.length + 7)) }, origins });
    if (kind === "invalid") payload.origins[0].samples[0].sequence = Number(payload.origins[0].samples[0].sequence);
    return payload;
  }
  function updateCounters() {
    $("fixtureCounters").textContent = `Stub GETs ${requests.length} · map ${stats.map} · evidence ${stats.evidence} · peer matches ${stats.matches} · unexpected ${stats.unexpected} · blocked connections ${stats.networkAttempts} · CSP blocks ${stats.cspBlocks} · held ${held.length} · static assets ${window.performance?.getEntriesByType("resource").length || 0}`;
    const detailHTML = $("serviceMapDetail").innerHTML;
    $("fixtureEvidenceState").textContent = `Verified retained source: ${detailHTML.includes("Verified retained source") ? "present" : "absent"} · Upstream socket peer: ${detailHTML.includes("Upstream socket peer") ? "present" : "absent"} · next unavailable response: ${nextUnavailable || "none"}`;
    $("fixtureIdentity").textContent = `Source token: ${sourceToken()}. Every fixture row deliberately shares record UUID ${RECORD}; sequence strings remain distinct above 2^53. ${ABSENT} is absent from the default map. API and app remain separate origins despite equal address values.`;
  }
  function note(message) { $("fixtureResult").textContent = message; updateCounters(); }
  function blockedNetwork() { stats.networkAttempts++; updateCounters(); return Promise.reject(new Error("Fixture networking is disabled")); }
  // The CSP also denies connect-src; these guards make accidental attempts visible.
  window.fetch = blockedNetwork;
  window.XMLHttpRequest = class { constructor() { stats.networkAttempts++; updateCounters(); throw new Error("Fixture networking is disabled"); } };
  window.WebSocket = window.EventSource = class { constructor() { stats.networkAttempts++; updateCounters(); throw new Error("Fixture networking is disabled"); } };
  window.addEventListener?.("load", updateCounters);
  document.addEventListener("securitypolicyviolation", () => { stats.cspBlocks++; updateCounters(); });
  async function request(path, options = {}) {
    const parsed = new URL(path, "http://example.com");
    const expectedSession = parsed.searchParams.get("session_id");
    const category = parsed.pathname === "/api/target/service-map" ? "map" : parsed.pathname === "/api/target/service-map/evidence" ? "evidence" : parsed.pathname === "/api/target/service-map/socket-peer-matches" ? "matches" : "unexpected";
    if (!path.startsWith("/api/target/service-map") || parsed.origin !== "http://example.com" || options.method && options.method !== "GET" || expectedSession !== session() || options.redirect !== "error" || category === "unexpected") {
      stats.unexpected++; updateCounters(); throw new Error("Unexpected fixture request");
    }
    stats[category]++;
    const entry = { category, path, aborted: () => options.signal?.aborted === true };
    requests.push(entry);
    const li = document.createElement("li"); li.textContent = `${requests.length}. GET ${path}`; $("fixtureRequests").append(li);
    const scopeOnly = parsed.searchParams.get("in_scope_only") === "true", id = parsed.searchParams.get("evidence_id");
    const payload = clone(category === "map" ? mapPayload(scopeOnly) : category === "evidence" ? evidencePayload(id) : matchesPayload(id, scopeOnly, nextUnavailable ? "unavailable" : scenario));
    if (category === "matches" && nextUnavailable) { payload.reasons = [nextUnavailable]; nextUnavailable = null; }
    const response = { ok: true, status: 200, json: async () => clone(payload) };
    if (category === "matches" && $("fixtureDelay").checked) {
      $("fixtureDelay").checked = false;
      return new Promise((resolve) => { held.push(() => resolve(response)); updateCounters(); });
    }
    updateCounters(); return response;
  }
  const elementIds = { panel: "targetServiceMapPanel", status: "serviceMapStatus", refresh: "serviceMapRefresh", search: "serviceMapSearch", scope: "serviceMapScopeOnly", mapControls: "serviceMapViewControls", allOrigins: "serviceMapAllOrigins", directRelations: "serviceMapDirectRelations", focusSelection: "serviceMapFocusSelection", canvas: "serviceMapCanvas", viewport: "serviceMapViewport", zoomControls: "serviceMapZoomControls", zoomFit: "serviceMapZoomFit", zoomOut: "serviceMapZoomOut", zoomReset: "serviceMapZoomReset", zoomIn: "serviceMapZoomIn", graphBounds: "serviceMapGraphBounds", boundsDetails: "serviceMapBoundsDetails", graphDiagnostics: "serviceMapGraphDiagnostics", overlayStatus: "serviceMapBodyOverlay", detail: "serviceMapDetail", content: "serviceMapContent", catalog: "serviceMapApiCatalog" };
  const on = (id, action) => $(id).addEventListener("click", () => { Promise.resolve().then(action).catch((error) => note(`Fixture error: ${error.message}`)); });
  async function resetScenario() {
    scenario = $("fixtureScenario").value; eventReady = true; visible = true; nextUnavailable = null;
    $("targetServiceMapPanel").classList.remove("hidden"); $("fixtureHide").textContent = "Hide map";
    $("fixtureMissingSample").checked = false; $("fixtureDelay").checked = false;
    $("serviceMapSearch").value = ""; $("serviceMapSearch").dispatchEvent(new Event("input"));
    controller.reset(); controller.setVisible(true); await controller.refresh({ force: true }); controller.selectOrigin(SOURCE);
    note(`Loaded ${scenario}. Inspect source, find matches, then explicitly show on map. No peer read happens on load.`);
  }
  async function inspectSource() {
    controller.selectOrigin(SOURCE); const ok = await controller.loadEvidence(sourceToken()); note(ok ? "Exact source verified. No peer-match read yet." : "Source is not available in this view; load the scenario again."); return ok;
  }
  function makeNextMatchUnavailable() {
    nextUnavailable = "record_changed";
    note("Next explicit peer read will report record_changed. Click Find recorded matches before Show or while a group is shown; source, peer and same-source overlay must clear.");
  }
  function makeNextScopeWorkLimit() {
    nextUnavailable = "scope_work_limit";
    note("Next explicit peer read will report scope_work_limit. Verified source, peer and any current shown group must remain. Arming this control makes no read or settings change.");
  }
  function validateData() {
    const kinds = ["normal", "many", "partial", "none", "unavailable", "scope_work_limit"];
    const row = evidencePayload(sourceToken()).row, peer = window.SniperServiceMap.normalizeSocketPeer(peerObservation(0));
    for (const kind of kinds) for (const scope of [false, true]) window.SniperServiceMap.normalizePeerMatches(matchesPayload(sourceToken(), scope, kind), session(), sourceToken(), row, peer, scope, generation());
    let rejected = false;
    try { window.SniperServiceMap.normalizePeerMatches(matchesPayload(sourceToken(), false, "invalid"), session(), sourceToken(), row, peer, false, generation()); } catch (_) { rejected = true; }
    if (!rejected) throw new Error("Malformed fixture was accepted");
    note("PASS: 12 valid DTO variants accepted; malformed numeric sequence rejected. No requests added."); return true;
  }
  try {
    if (!window.SniperServiceMap) throw new Error("Production service-map.js did not load");
    controller = window.SniperServiceMap.createController({ elements: Object.fromEntries(Object.entries(elementIds).map(([key, id]) => [key, $(id)])), getSessionId: session, getEventStreamReady: () => eventReady, document, request });
    controller.setViewChange((view) => controller.setVisible(visible, view));
    on("fixtureReset", resetScenario); on("fixtureSource", inspectSource);
    on("fixtureMatches", async () => note(await controller.loadPeerMatches() ? "Peer inspection finished by one stub GET. Check the production inspector for matches or source-unavailable status." : "Peer matches were not loaded. Inspect the source and check the production inspector."));
    on("fixtureShow", () => note(controller.showPeerMatchesOnMap() ? "Temporary recorded-address group shown locally. No request added." : "No current matched source to show."));
    on("fixtureClear", () => { controller.clearBodyReferenceOverlay(); note("Temporary overlay cleared locally."); });
    on("fixtureAbsent", async () => { const row = fixtureRows().find((item) => item.origin === ABSENT); note(row && await controller.loadPeerEvidence(row.evidence_id, ABSENT) ? "Absent-origin exact sample verified; original address-group source remains pinned." : "Show a current address group first; unavailable exact samples must fail closed."); });
    on("fixtureValidate", validateData);
    on("fixtureChangedMatch", makeNextMatchUnavailable);
    on("fixtureScopeWorkLimit", makeNextScopeWorkLimit);
    on("fixtureDirty", () => { controller.markDirty(); note("Simulated retained row change. Evidence and temporary overlay must clear."); });
    on("fixtureInvalidate", async () => { await controller.invalidate(); note("Simulated history invalidation and fresh default-map read."); });
    on("fixtureGeneration", async () => { generationIndex = 1 - generationIndex; await controller.refresh(); note("Generation changed. Old exact sources and temporary overlay must clear."); });
    on("fixtureSession", async () => { sessionIndex = 1 - sessionIndex; controller.reset(); await controller.refresh(); note("Session changed. Old tokens and temporary overlay must clear."); });
    on("fixtureDisconnect", () => { eventReady = false; controller.disconnect(); note("Event stream disconnected. Evidence and temporary overlay must clear."); });
    on("fixtureReconnect", async () => { eventReady = true; await controller.refresh({ force: true }); note("Reconnected and refreshed. Explicitly inspect again."); });
    on("fixtureHide", () => { visible = !visible; controller.setVisible(visible); $("targetServiceMapPanel").classList.toggle("hidden", !visible); $("fixtureHide").textContent = visible ? "Hide map" : "Show map"; note(visible ? "Map visible. Cleared overlays must not resurrect." : "Map hidden; temporary overlay must clear."); });
    on("fixtureRelease", () => { const count = held.length; held.splice(0).forEach((resolve) => resolve()); note(`Released ${count} held response(s), even if aborted. Invalidated results must not resurrect.`); });
    window.recordedPeerFixture = { controller, stats, requests, resetScenario, inspectSource, validateData, makeNextMatchUnavailable, makeNextScopeWorkLimit, fixtureRows, matchesPayload, evidencePayload, mapPayload, sourceToken, get heldCount() { return held.length; } };
    if (window.MutationObserver) new window.MutationObserver(updateCounters).observe($("serviceMapDetail"), { childList: true, subtree: true });
    resetScenario().catch((error) => note(`Fixture error: ${error.message}`));
  } catch (error) { note(`Fixture cannot start: ${error.message}`); }
})();
