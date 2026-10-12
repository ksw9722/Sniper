/* Read-only projection of retained HTTP metadata. Deliberately no capture,
   replay, remote resources, or UUID-only detail navigation in this module. */
(function (root) {
  "use strict";
  const MAX_CARDS = 24;
  const MAX_EDGES = 40;
  const MAX_REFERENCE_CARDS = 100;
  const MAX_REFERENCE_SOURCES = 30;
  const API_PAGE_SIZE = 50;
  const escape = (value) => String(value ?? "").replace(/[&<>"']/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;" })[char]);
  const count = (value) => Number.isSafeInteger(value) && value >= 0 ? value : 0;
  const array = (value) => Array.isArray(value) ? value : [];
  const safePath = (value) => String(value || "/").split(/[?#]/, 1)[0];
  function safeOrigin(value) {
    try {
      const url = new URL(value);
      return ["http:", "https:"].includes(url.protocol) ? url.origin : "";
    } catch (_) { return ""; }
  }
  function normalizeSnapshot(payload, sessionId) {
    if (payload?.schema_version !== 1 || payload.session_id !== sessionId || !payload.snapshot || !Array.isArray(payload.origins)) {
      throw new Error("Invalid service map response");
    }
    const seen = new Set();
    const origins = payload.origins.flatMap((item) => {
      const origin = safeOrigin(item?.origin);
      if (!origin || seen.has(origin)) return [];
      seen.add(origin);
      return [{
        origin, captured: true, in_scope: item.in_scope === true, request_count: count(item.request_count),
        first_seen: item.first_seen, last_seen: item.last_seen,
        endpoints: array(item.endpoints).map((endpoint) => ({
          method: String(endpoint.method || "?"), path: safePath(endpoint.path), path_group: safePath(endpoint.path_group || endpoint.path),
          request_count: count(endpoint.request_count), statuses: array(endpoint.statuses), content_types: array(endpoint.content_types),
          evidence: array(endpoint.evidence).filter((id) => typeof id === "string" && id.length > 0),
        })),
        technology_clues: array(item.technology_clues).filter((clue) => clue.source === "response_header" && clue.confidence === "observed_header").map((clue) => ({
          name: String(clue.name || "Header clue"), evidence: array(clue.evidence).filter((id) => typeof id === "string" && id.length > 0),
        })),
        header_links: array(item.header_links).filter((link) => link.source === "request_header_declaration" && ["origin_header", "referer_header"].includes(link.relationship) && safeOrigin(link.source_origin)).map((link) => ({
          source_origin: safeOrigin(link.source_origin), relationship: link.relationship,
          evidence: array(link.evidence).filter((id) => typeof id === "string" && id.length > 0),
        })),
        redirects: array(item.redirects).filter((redirect) => redirect.relationship === "redirect_instruction" && safeOrigin(redirect.target_origin)).map((redirect) => ({
          target_origin: safeOrigin(redirect.target_origin), target_path: safePath(redirect.target_path),
          evidence: array(redirect.evidence).filter((id) => typeof id === "string" && id.length > 0),
        })),
      }];
    });
    return { snapshot: payload.snapshot, origins, warningCount: array(payload.warnings).length };
  }
  function stableOrder(previous, origins) {
    const ids = new Set(origins.map((origin) => origin.origin));
    const previousIds = new Set(previous);
    return [...previous.filter((id) => ids.has(id)), ...[...ids].filter((id) => !previousIds.has(id)).sort()];
  }
  function graphLayout(origins, order, previousSlots = new Map()) {
    const byOrigin = new Map(origins.map((origin) => [origin.origin, origin]));
    const visibleOrigins = order.filter((id) => byOrigin.has(id)).slice(0, MAX_CARDS).map((id) => byOrigin.get(id));
    const visibleIds = new Set(visibleOrigins.map((origin) => origin.origin));
    const connections = visibleOrigins.flatMap((origin) => [
      ...origin.redirects.map((redirect) => [origin.origin, redirect.target_origin]),
      ...origin.header_links.map((link) => [link.source_origin, origin.origin]),
    ]).filter(([source, target]) => source !== target && visibleIds.has(source) && visibleIds.has(target));
    const incoming = new Set(connections.map(([, target]) => target));
    const outgoing = new Set(connections.map(([source]) => source));
    let left = visibleOrigins.filter((origin) => outgoing.has(origin.origin) && !incoming.has(origin.origin));
    let right = visibleOrigins.filter((origin) => !left.includes(origin));
    if (!left.length) {
      left = visibleOrigins.filter((_, index) => index % 2 === 0);
      right = visibleOrigins.filter((_, index) => index % 2 !== 0);
    } else {
      right.sort((a, b) => a.origin.localeCompare(b.origin));
      const targetRank = new Map(right.map((origin, index) => [origin.origin, index]));
      const averageTarget = (origin) => {
        const ranks = connections.filter(([source, target]) => source === origin.origin && targetRank.has(target)).map(([, target]) => targetRank.get(target));
        return ranks.length ? ranks.reduce((sum, rank) => sum + rank, 0) / ranks.length : 0;
      };
      left.sort((a, b) => averageTarget(a) - averageTarget(b) || a.origin.localeCompare(b.origin));
    }
    const preferred = new Map([...left.map((origin, row) => [origin.origin, { column: 0, row }]), ...right.map((origin, row) => [origin.origin, { column: 1, row }])]);
    const slots = new Map(visibleOrigins.filter((origin) => previousSlots.has(origin.origin)).map((origin) => [origin.origin, previousSlots.get(origin.origin)]));
    for (const origin of visibleOrigins) {
      if (slots.has(origin.origin)) continue;
      const column = preferred.get(origin.origin).column;
      let row = previousSlots.size ? 0 : preferred.get(origin.origin).row;
      while ([...slots.values()].some((slot) => slot.column === column && slot.row === row)) row += 1;
      slots.set(origin.origin, { column, row });
    }
    const visible = visibleOrigins.map((origin) => ({ ...origin, x: 32 + slots.get(origin.origin).column * 380, y: 36 + slots.get(origin.origin).row * 184 }));
    const positions = new Map(visible.map((origin) => [origin.origin, origin]));
    const edges = [];
    const seen = new Set();
    let edgeCount = 0;
    const declaredEdges = visible.flatMap((origin) => [
      ...origin.redirects.map((redirect) => ({ source: origin.origin, target: redirect.target_origin, relationship: "redirect_instruction", label: "Redirect instruction" })),
      ...origin.header_links.map((link) => ({ source: link.source_origin, target: origin.origin, relationship: link.relationship, label: link.relationship === "origin_header" ? "Origin header" : "Referer header" })),
    ]);
    for (const edge of declaredEdges) {
      const source = positions.get(edge.source);
      const target = positions.get(edge.target);
      const key = JSON.stringify([edge.source, edge.target, edge.relationship]);
      if (!source || !target || target === source || seen.has(key)) continue;
      seen.add(key);
      edgeCount += 1;
      if (edges.length >= MAX_EDGES) continue;
      const toRight = target.x > source.x;
      const sameColumn = target.x === source.x;
      const x1 = source.x + (sameColumn ? 276 : toRight ? 276 : 0);
      const x2 = target.x + (sameColumn ? 276 : toRight ? 0 : 276);
      const y1 = source.y + 72;
      const y2 = target.y + 72;
      const bend = sameColumn ? x1 + 32 : (x1 + x2) / 2;
      edges.push({ ...edge, path: `M ${x1} ${y1} C ${bend} ${y1}, ${bend} ${y2}, ${x2} ${y2}` });
    }
    return { nodes: visible, edges, edgeCount, slots, width: 720, height: Math.max(400, ...visible.map((origin) => origin.y + 174)) };
  }
  function createController({ elements: els, getSessionId, request, document: doc = root.document }) {
    const state = {
      sessionId: null, snapshot: null, order: [], selectedOrigin: null, selectionInitialized: false,
      query: "", scopeOnly: false, view: "service-map", detailTab: "apis", endpointIndex: null, endpointSelection: null,
      loading: false, error: "", requestId: 0, evidenceRequestId: 0, evidence: null,
      evidenceLoading: false, evidenceError: "", evidenceId: null, page: 0, detailPage: 0,
      historyVersion: 0, stale: false,
    };
    let abort = null;
    let evidenceAbort = null;
    let visible = false;
    function replace(element, html) {
      if (!element || element.innerHTML === html) return;
      const focused = doc?.activeElement;
      const key = focused?.dataset?.serviceFocus;
      const owned = key && element.contains(focused);
      element.innerHTML = html;
      // Only restore the element that this replacement detached. A newly focused
      // dialog or control is never displaced by a background refresh.
      if (owned && visible && (!doc.activeElement || doc.activeElement === doc.body || doc.activeElement === focused && !focused.isConnected)
        && !doc.querySelector?.(".modal-backdrop:not(.hidden), dialog[open]")) {
        const next = [...element.querySelectorAll("[data-service-focus]")].find((candidate) => candidate.dataset.serviceFocus === key);
        if (next && !next.disabled && next.getClientRects().length) next.focus({ preventScroll: true });
      }
    }
    function graphOrigins() {
      if (state.graphSnapshot === state.snapshot) return state.graphOriginsCache || [];
      const captured = state.snapshot?.origins || [];
      let referenceCards = 0;
      state.omittedGraphReferences = 0;
      state.relationsByOrigin = new Map();
      const origins = new Map(captured.map((origin) => [origin.origin, origin]));
      const reference = (id, source, label) => {
        if (!origins.has(id)) {
          if (referenceCards >= MAX_REFERENCE_CARDS) { state.omittedGraphReferences += 1; return; }
          referenceCards += 1;
          origins.set(id, { origin: id, captured: false, in_scope: false, endpoints: [], technology_clues: [], redirects: [], header_links: [], references: [], referencesOmitted: 0 });
        }
        const item = origins.get(id);
        if (!item.captured && !item.references.some((ref) => ref.origin === source.origin && ref.label === label)) {
          if (item.references.length >= MAX_REFERENCE_SOURCES) item.referencesOmitted += 1;
          else item.references.push({ origin: source.origin, label });
        }
      };
      const relation = (item) => {
        for (const id of new Set([item.sourceOrigin, item.targetOrigin])) {
          if (!origins.has(id)) continue;
          if (!state.relationsByOrigin.has(id)) state.relationsByOrigin.set(id, { items: [], omitted: 0 });
          const group = state.relationsByOrigin.get(id);
          if (group.items.length < MAX_REFERENCE_SOURCES) group.items.push(item);
          else group.omitted += 1;
        }
      };
      captured.forEach((origin) => {
        origin.header_links.forEach((link) => {
          reference(link.source_origin, origin, `${link.relationship === "origin_header" ? "Origin" : "Referer"} header on`);
          relation({ sourceOrigin: link.source_origin, targetOrigin: origin.origin, capturedOrigin: origin.origin, label: `${link.relationship === "origin_header" ? "Origin" : "Referer"} header declaration`, evidence: link.evidence[0] || "" });
        });
        origin.redirects.forEach((redirect) => {
          reference(redirect.target_origin, origin, "Redirect instruction from");
          relation({ sourceOrigin: origin.origin, targetOrigin: redirect.target_origin, targetPath: redirect.target_path, capturedOrigin: origin.origin, label: "Redirect instruction", evidence: redirect.evidence[0] || "" });
        });
      });
      state.graphSnapshot = state.snapshot;
      state.graphOriginsCache = [...origins.values()];
      return state.graphOriginsCache;
    }
    function filteredOrigins() {
      const query = state.query.trim().toLowerCase();
      return graphOrigins().filter((origin) => (!state.scopeOnly || origin.in_scope || !origin.captured) && (!query || origin.origin.toLowerCase().includes(query)));
    }
    function selectedOrigin() {
      return graphOrigins().find((origin) => origin.origin === state.selectedOrigin) || null;
    }
    function cancelEvidence() {
      state.evidenceRequestId += 1;
      evidenceAbort?.abort();
      evidenceAbort = null;
      state.evidence = null;
      state.evidenceLoading = false;
      state.evidenceError = "";
      state.evidenceId = null;
    }
    function reset() {
      state.requestId += 1;
      abort?.abort();
      abort = null;
      cancelEvidence();
      Object.assign(state, { sessionId: getSessionId(), snapshot: null, order: [], selectedOrigin: null,
        selectionInitialized: false, endpointIndex: null, endpointSelection: null, loading: false, error: "", page: 0, detailPage: 0, stale: false });
      render();
    }
    function selectOrigin(origin, endpointIndex = null) {
      const item = graphOrigins().find((item) => item.origin === origin);
      if (!item || endpointIndex !== null && (!Number.isSafeInteger(endpointIndex) || !item.endpoints[endpointIndex])) return;
      if (state.selectedOrigin !== origin || state.endpointIndex !== endpointIndex) cancelEvidence();
      state.selectedOrigin = origin;
      state.selectionInitialized = true;
      state.endpointIndex = endpointIndex;
      const endpoint = endpointIndex === null ? null : item.endpoints[endpointIndex];
      // Keep only the exact identity when invalidation clears all displayed data.
      // An array index cannot safely survive scope filtering or history removal.
      state.endpointSelection = endpoint ? { method: endpoint.method, path: endpoint.path } : null;
      state.detailPage = 0;
      state.detailTab = endpointIndex === null ? "apis" : "evidence";
      render();
    }
    async function refresh({ force = false } = {}) {
      if (!visible) return false;
      const sessionId = getSessionId();
      if (state.sessionId !== sessionId) reset();
      if (!sessionId || state.loading && !force) return false;
      abort?.abort();
      abort = new AbortController();
      const requestId = ++state.requestId;
      const scopeOnly = state.scopeOnly;
      const historyVersion = state.historyVersion;
      state.loading = true;
      state.error = "";
      renderStatus();
      if (!state.snapshot) render();
      try {
        const params = new URLSearchParams({ session_id: sessionId, in_scope_only: String(scopeOnly), max_records: "10000" });
        const response = await request(`/api/target/service-map?${params}`, { signal: abort.signal });
        if (!response.ok) throw new Error("Service map unavailable");
        const snapshot = normalizeSnapshot(await response.json(), sessionId);
        if (requestId !== state.requestId || sessionId !== getSessionId() || scopeOnly !== state.scopeOnly) return false;
        state.snapshot = snapshot;
        state.stale = historyVersion !== state.historyVersion;
        const allOrigins = graphOrigins();
        state.order = state.order.length ? stableOrder(state.order, allOrigins) : [
          ...snapshot.origins.map((origin) => origin.origin).sort(),
          ...allOrigins.filter((origin) => !origin.captured).map((origin) => origin.origin).sort(),
        ];
        if (!allOrigins.some((origin) => origin.origin === state.selectedOrigin)) {
          cancelEvidence();
          state.selectedOrigin = state.selectionInitialized ? null : state.order[0] || null;
          state.endpointIndex = null;
          state.endpointSelection = null;
        }
        if (state.selectedOrigin) state.selectionInitialized = true;
        const origin = selectedOrigin();
        if (state.endpointSelection) {
          const index = origin?.endpoints.findIndex((endpoint) => endpoint.method === state.endpointSelection.method && endpoint.path === state.endpointSelection.path) ?? -1;
          state.endpointIndex = index < 0 ? null : index;
          if (index < 0) { state.endpointSelection = null; cancelEvidence(); }
        }
        if (state.evidenceId && !evidenceItems(origin).some((item) => item.id === state.evidenceId)) cancelEvidence();
        state.loading = false;
        render();
        return true;
      } catch (_) {
        if (requestId !== state.requestId || sessionId !== getSessionId()) return false;
        state.loading = false;
        state.error = state.snapshot ? "Refresh failed. Showing the previous snapshot; use Refresh to retry." : "Could not load the service map. Use Refresh to retry.";
        render();
        return false;
      }
    }
    function evidenceItems(origin) {
      if (!origin || !origin.captured) return [];
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      const items = [];
      const seen = new Set();
      const add = (ids, label) => ids.forEach((id) => { if (!seen.has(id)) { seen.add(id); items.push({ id, label }); } });
      if (endpoint) add(endpoint.evidence, `${endpoint.method} ${endpoint.path}`);
      else {
        origin.endpoints.forEach((item) => add(item.evidence, `${item.method} ${item.path}`));
        origin.technology_clues.forEach((item) => add(item.evidence, `${item.name} · header clue`));
        origin.redirects.forEach((item) => add(item.evidence, `Redirect instruction → ${item.target_origin}`));
        origin.header_links.forEach((item) => add(item.evidence, `${item.relationship === "origin_header" ? "Origin" : "Referer"} header: ${item.source_origin}`));
      }
      return items;
    }
    async function loadEvidence(id) {
      const origin = selectedOrigin();
      if (!evidenceItems(origin).some((item) => item.id === id)) return false;
      cancelEvidence();
      const requestId = ++state.evidenceRequestId;
      const sessionId = getSessionId();
      const selected = state.selectedOrigin;
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      state.evidenceId = id;
      state.evidenceLoading = true;
      state.detailTab = "evidence";
      evidenceAbort = new AbortController();
      renderDetail();
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: id });
        const response = await request(`/api/target/service-map/evidence?${params}`, { signal: evidenceAbort.signal });
        if (!response.ok) throw new Error("Evidence unavailable");
        const result = await response.json();
        if (requestId !== state.evidenceRequestId || sessionId !== getSessionId() || selected !== state.selectedOrigin) return false;
        if (result.schema_version !== 1 || result.session_id !== sessionId || result.evidence_id !== id
          || result.source !== "captured_http_metadata" || typeof result.available !== "boolean") throw new Error("Evidence source changed");
        const row = result.row;
        if (result.available && (!row || row.evidence_id !== id || safeOrigin(row.origin) !== selected
          || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(row.record_id || "") || typeof row.sequence !== "string" || !/^[0-9]+$/.test(row.sequence)
          || typeof row.method !== "string" || typeof row.path !== "string"
          || endpoint && (row.method !== endpoint.method || safePath(row.path) !== endpoint.path))) throw new Error("Evidence row changed");
        state.evidence = result.available === true ? row : null;
        state.evidenceError = state.evidence ? "" : "Source unavailable. The retained row may have changed or been removed. Refresh the map to select current evidence.";
        state.evidenceLoading = false;
        renderDetail();
        return !!state.evidence;
      } catch (_) {
        if (requestId !== state.evidenceRequestId || sessionId !== getSessionId() || selected !== state.selectedOrigin) return false;
        state.evidenceLoading = false;
        state.evidenceError = "Source unavailable. The exact retained row could not be verified. Refresh the map and try again.";
        renderDetail();
        return false;
      }
    }
    function renderStatus() {
      if (!els.status) return;
      const snapshot = state.snapshot?.snapshot;
      let status = "Open a session to view its retained history.";
      if (state.loading && !snapshot) status = "Loading retained HTTP metadata…";
      if (snapshot) {
        status = `${count(snapshot.considered_rows).toLocaleString()} rows considered / ${count(snapshot.retained_rows).toLocaleString()} retained · ${state.snapshot.origins.length.toLocaleString()} origins`;
        status += snapshot.complete === true ? " · Complete retained snapshot" : ` · Incomplete: ${array(snapshot.stopped_by).join(", ") || "projection bounds reached"}`;
        status += ` · ${count(snapshot.included_rows)} included / ${count(snapshot.excluded_rows)} excluded / ${count(snapshot.omitted_rows)} omitted among analyzed`;
        status += ` · Newest capture-sequence window: ${snapshot.oldest_considered_at || "unknown"} to ${snapshot.latest_considered_at || "unknown"}`;
        if (state.scopeOnly) status += " · In scope only";
        if (state.stale) status += " · History changed: counts are from the previous snapshot";
        if (state.loading) status += " · Refreshing…";
        if (state.snapshot.warningCount) status += ` · ${state.snapshot.warningCount} projection warning(s)`;
      }
      els.status.textContent = state.error ? `${state.error}${snapshot ? ` ${status}` : ""}` : status;
      els.status.classList.toggle("service-map-error", !!state.error);
      els.refresh.disabled = state.loading;
      els.refresh.textContent = state.loading ? "Refreshing…" : "Refresh";
      els.panel.setAttribute("aria-busy", String(state.loading));
    }
    function renderGraph() {
      const origins = filteredOrigins();
      const filterKey = JSON.stringify([state.query.trim().toLowerCase(), state.scopeOnly, state.sessionId]);
      if (state.layoutFilter !== filterKey) { state.layoutSlots = new Map(); state.layoutFilter = filterKey; }
      const layout = graphLayout(origins, state.order, state.layoutSlots);
      state.layoutSlots = layout.slots;
      const selected = selectedOrigin();
      const selectionNotice = !selected || layout.nodes.some((origin) => origin.origin === selected.origin) ? "" : origins.some((origin) => origin.origin === selected.origin)
        ? `Selected origin is outside the ${MAX_CARDS}-card display; inspector retained.`
        : "Selected origin is outside this filter; inspector retained.";
      if (!origins.length) {
        replace(els.canvas, `<div class="service-map-empty"><span class="service-map-empty-symbol" aria-hidden="true">◎</span><h3>${state.loading ? "Loading service origins…" : state.error && !state.snapshot ? "Service map unavailable" : state.query || state.scopeOnly ? "No matching origins" : "No retained HTTP origins"}</h3><p>${state.query || state.scopeOnly ? "Change the origin filter or turn off In scope only." : "This view uses only HTTP history already retained in this session."}</p></div>`);
        els.graphBounds.textContent = selectionNotice;
        return;
      }
      const edges = layout.edges.map((edge) => `<path class="service-map-edge ${edge.relationship !== "redirect_instruction" ? "header-declaration" : ""} ${[edge.source, edge.target].includes(state.selectedOrigin) ? "selected" : ""}" d="${edge.path}" marker-end="url(#service-map-arrow)"><title>${escape(edge.source)} → ${escape(edge.target)} · ${escape(edge.label)} declaration</title></path>`).join("");
      const cards = layout.nodes.map((origin) => {
        const selected = origin.origin === state.selectedOrigin;
        const url = new URL(origin.origin);
        return `<button class="service-map-card ${!origin.captured ? "referenced" : ""} ${selected ? "selected" : ""}" type="button" data-service-origin="${escape(origin.origin)}" data-service-focus="origin:${escape(origin.origin)}" aria-current="${selected}" style="left:${origin.x}px;top:${origin.y}px">
          <span class="service-map-card-symbol" aria-hidden="true">◎</span><span class="service-map-card-copy"><strong title="${escape(origin.origin)}">${escape(url.host)}</strong><span class="service-map-card-scheme">${escape(url.protocol.slice(0, -1))} · ${origin.captured ? "captured origin" : "referenced origin"}</span><span class="service-map-clues">${origin.technology_clues.slice(0, 2).map((clue) => `<span title="Observed response header; not verified architecture">${escape(clue.name)} · header clue</span>`).join("")}</span><span class="service-map-card-meta">${origin.captured ? `${origin.endpoints.length} API paths · ${origin.request_count.toLocaleString()} sampled rows` : "Referenced only in this snapshot"}</span></span>
          <span class="service-map-scope-dot ${!origin.captured ? "hidden" : ""} ${origin.in_scope ? "in-scope" : ""}" title="${origin.in_scope ? "In scope" : "Out of scope"}"></span>
        </button>`;
      }).join("");
      replace(els.canvas, `<div class="service-map-layout" style="width:${layout.width}px;height:${layout.height}px"><svg class="service-map-edges" width="${layout.width}" height="${layout.height}" aria-hidden="true"><defs><marker id="service-map-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M1,1 L7,4 L1,7" fill="none" stroke="currentColor" stroke-width="1.5" /></marker></defs>${edges}</svg>${cards}</div>`);
      els.graphBounds.textContent = `${layout.nodes.length} / ${origins.length} matching origins${state.omittedGraphReferences ? ` · Reference-card cap ${MAX_REFERENCE_CARDS}; ${state.omittedGraphReferences} reference declarations omitted` : ""}${origins.length > MAX_CARDS ? " · Filter to find more" : ""}${layout.edgeCount > MAX_EDGES ? ` · ${MAX_EDGES} / ${layout.edgeCount} arrows shown` : ""}${selectionNotice ? ` · ${selectionNotice}` : ""}`;
    }
    function endpointButton(endpoint, origin, index, catalog = false) {
      const method = endpoint.method;
      const tone = method.toUpperCase() === "GET" ? "get" : method.toUpperCase() === "POST" ? "post" : "other";
      return `<button class="service-map-api-row ${state.selectedOrigin === origin.origin && state.endpointIndex === index ? "selected" : ""}" type="button" data-service-endpoint="${index}" data-service-origin="${escape(origin.origin)}" data-service-focus="api:${escape(JSON.stringify([origin.origin, endpoint.method, endpoint.path]))}"><span class="service-map-method ${tone}" title="${escape(method)}">${escape(method)}</span><span class="service-map-api-copy">${catalog ? `<small title="${escape(origin.origin)}">${escape(origin.origin)}</small>` : ""}<span title="${escape(endpoint.path)}">${escape(endpoint.path)}</span></span><span class="service-map-api-count">${endpoint.request_count.toLocaleString()}<span class="service-map-chevron" aria-hidden="true">›</span></span></button>`;
    }
    function relationshipMarkup(origin) {
      const group = state.relationsByOrigin?.get(origin.origin);
      if (!group?.items.length) return "";
      return `<h4>Metadata relationships</h4><p class="service-map-muted">Header declarations and redirect instructions do not prove service calls. Select a relationship to verify its exact source.</p><div class="service-map-evidence-list">${group.items.map((item) => `<button class="service-map-evidence-button service-map-relation" type="button" data-service-relation-origin="${escape(item.capturedOrigin)}" data-service-relation-evidence="${escape(item.evidence)}" data-service-focus="relation:${escape(JSON.stringify([item.sourceOrigin, item.targetOrigin, item.targetPath || "", item.label]))}"><span><strong>${escape(item.label)}</strong>${escape(item.sourceOrigin)} → ${escape(item.targetOrigin)}${escape(item.targetPath || "")}</span><span>${item.evidence ? "View source ›" : "Inspect ›"}</span></button>`).join("")}</div>${group.omitted ? `<p class="service-map-muted">${group.omitted} additional relationship declarations omitted from this pane.</p>` : ""}`;
    }
    function evidenceMarkup(origin) {
      const items = evidenceItems(origin);
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      const row = state.evidence;
      const meta = row ? `<div class="service-map-verified"><h4>Verified retained source</h4><dl><dt>Method</dt><dd>${escape(row.method)}</dd><dt>Origin</dt><dd>${escape(safeOrigin(row.origin))}</dd><dt>Path</dt><dd>${escape(safePath(row.path))}</dd><dt>Status</dt><dd>${escape(row.status ?? "No response")}</dd><dt>Sequence</dt><dd>${escape(row.sequence ?? "Unavailable")}</dd><dt>Captured</dt><dd>${escape(row.started_at || "Unavailable")}</dd><dt>Record ID</dt><dd>${escape(row.record_id || "Unavailable")}</dd></dl>${row.metadata_complete === false ? `<p class="service-map-error">Source metadata is incomplete: ${escape(array(row.omitted).join(", ") || "bounded metadata")}</p>` : ""}<h4>Source observations</h4><ul class="service-map-references">${array(row.header_links).filter((link) => ["origin_header", "referer_header"].includes(link.relationship)).map((link) => `<li>${link.relationship === "origin_header" ? "Origin" : "Referer"} header: ${escape(safeOrigin(link.source_origin))} (declaration)</li>`).join("")}${row.redirect ? `<li>Redirect instruction: ${escape(safeOrigin(row.redirect.target_origin))}${escape(safePath(row.redirect.target_path))}</li>` : ""}${array(row.technology_clues).map((clue) => `<li>${escape(clue.name)} · ${escape(clue.header_name)} header clue</li>`).join("")}</ul><p>Exact retained source verified for this read.</p></div>` : "";
      return `${endpoint ? `<div class="service-map-endpoint-summary"><span class="service-map-method" title="${escape(endpoint.method)}">${escape(endpoint.method)}</span><strong>${escape(endpoint.path)}</strong><p>${endpoint.request_count} sampled rows · Statuses: ${escape(endpoint.statuses.join(", ") || "No response")}</p></div>` : ""}
        <h4>Source evidence</h4><p class="service-map-muted">Read the exact retained source metadata. Bodies, query strings and private headers are not included.</p>
        ${state.evidenceLoading ? '<p class="service-map-muted" role="status">Verifying source…</p>' : ""}${state.evidenceError ? `<p class="service-map-error" role="status">${escape(state.evidenceError)}</p>` : ""}${meta}
        <div class="service-map-evidence-list">${items.slice(0, 30).map((item) => `<button class="service-map-evidence-button ${state.evidenceId === item.id ? "selected" : ""}" type="button" data-service-evidence="${escape(item.id)}" data-service-focus="evidence:${escape(item.id)}"><span>${escape(item.label)}</span><span>View source ›</span></button>`).join("") || '<p class="service-map-muted">No source samples are available in this bounded snapshot.</p>'}</div>${items.length > 30 ? `<p class="service-map-muted">Showing 30 of ${items.length} source samples. Select an API path to narrow the evidence.</p>` : ""}
        ${!endpoint ? relationshipMarkup(origin) : ""}
        ${!endpoint && origin.technology_clues.length ? `<h4>Observed header clues</h4><p class="service-map-muted">${origin.technology_clues.map((clue) => escape(clue.name)).join(", ")} · response headers only</p>` : ""}`;
    }
    function renderDetail() {
      const origin = selectedOrigin();
      if (!origin) { replace(els.detail, '<div class="service-map-empty"><h3>Select an origin</h3><p>Choose a card to inspect its API paths and retained evidence.</p></div>'); return; }
      if (!origin.captured) {
        replace(els.detail, `<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div></header><div class="service-map-detail-body"><h4>Referenced only in this snapshot</h4><p class="service-map-muted">This origin appears only in captured metadata within this snapshot. No captured rows are shown for this origin. Scope and projection bounds may omit captured history; no service role or request count is established here.</p>${relationshipMarkup(origin)}<h4>Captured origins</h4><div class="service-map-evidence-list">${origin.references.map((ref) => `<button class="service-map-evidence-button" type="button" data-service-origin="${escape(ref.origin)}" data-service-focus="reference:${escape(ref.origin)}"><span>${escape(ref.label)} ${escape(ref.origin)}</span><span>Inspect source ›</span></button>`).join("")}</div>${origin.referencesOmitted ? `<p class="service-map-muted">${origin.referencesOmitted} additional source references omitted from this pane.</p>` : ""}</div>`);
        return;
      }
      const pageCount = Math.max(1, Math.ceil(origin.endpoints.length / API_PAGE_SIZE));
      state.detailPage = Math.min(state.detailPage, pageCount - 1);
      const start = state.detailPage * API_PAGE_SIZE;
      replace(els.detail, `<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div><span class="detail-chip ${origin.in_scope ? "ok" : "none"}">${origin.in_scope ? "In scope" : "Out of scope"}</span></header>
        <div class="service-map-detail-tabs" role="tablist" aria-label="Origin details"><button type="button" role="tab" id="serviceMapDetailApisTab" aria-controls="serviceMapDetailBody" aria-selected="${state.detailTab === "apis"}" tabindex="${state.detailTab === "apis" ? 0 : -1}" data-service-detail-tab="apis" data-service-focus="detail:apis">APIs</button><button type="button" role="tab" id="serviceMapDetailEvidenceTab" aria-controls="serviceMapDetailBody" aria-selected="${state.detailTab === "evidence"}" tabindex="${state.detailTab === "evidence" ? 0 : -1}" data-service-detail-tab="evidence" data-service-focus="detail:evidence">Evidence</button></div>
        <div class="service-map-detail-body" id="serviceMapDetailBody" role="tabpanel" aria-labelledby="${state.detailTab === "apis" ? "serviceMapDetailApisTab" : "serviceMapDetailEvidenceTab"}">${state.detailTab === "evidence" ? evidenceMarkup(origin) : `<p class="service-map-muted">${origin.endpoints.length ? `${start + 1}–${Math.min(start + API_PAGE_SIZE, origin.endpoints.length)} of ${origin.endpoints.length}` : "No"} observed method/path combinations. Paths are not inferred API definitions.</p><div class="service-map-api-list">${origin.endpoints.slice(start, start + API_PAGE_SIZE).map((endpoint, index) => endpointButton(endpoint, origin, start + index)).join("")}</div>${pageCount > 1 ? `<div class="service-map-pagination"><button class="secondary-action" type="button" data-service-detail-page="${state.detailPage - 1}" ${state.detailPage === 0 ? "disabled" : ""}>Previous</button><span>${state.detailPage + 1} / ${pageCount}</span><button class="secondary-action" type="button" data-service-detail-page="${state.detailPage + 1}" ${state.detailPage + 1 >= pageCount ? "disabled" : ""}>Next</button></div>` : ""}${relationshipMarkup(origin)}<div class="service-map-evidence-summary"><h4>Evidence summary</h4><p>${origin.request_count.toLocaleString()} sampled HTTP rows in this snapshot. ${origin.redirects.length} redirect instruction(s).</p><p class="service-map-muted">${origin.technology_clues.length ? `${origin.technology_clues.length} response-header clue(s). ` : ""}No captured browser initiator, IP or certificate association is available.</p><button class="secondary-action" type="button" data-service-detail-tab="evidence" data-service-focus="view:evidence">View evidence</button></div>`}</div>`);
    }
    function renderCatalog() {
      const entries = filteredOrigins().flatMap((origin) => origin.endpoints.map((endpoint, index) => ({ origin, endpoint, index })));
      const pages = Math.max(1, Math.ceil(entries.length / API_PAGE_SIZE));
      state.page = Math.min(state.page, pages - 1);
      const start = state.page * API_PAGE_SIZE;
      replace(els.catalog, `<header class="service-map-catalog-header"><div><h3>Observed API paths</h3><p>Method/path combinations from retained HTTP history. Query strings are omitted; no endpoint is probed.</p></div><span>${entries.length.toLocaleString()} combinations</span></header><div class="service-map-api-list">${entries.slice(start, start + API_PAGE_SIZE).map(({ origin, endpoint, index }) => endpointButton(endpoint, origin, index, true)).join("") || `<p class="empty-copy">${state.loading ? "Loading retained API paths…" : "No API paths match this view."}</p>`}</div><div class="service-map-pagination"><button class="secondary-action" type="button" data-service-page="${state.page - 1}" ${state.page === 0 ? "disabled" : ""}>Previous</button><span>${entries.length ? start + 1 : 0}–${Math.min(start + API_PAGE_SIZE, entries.length)} / ${entries.length}</span><button class="secondary-action" type="button" data-service-page="${state.page + 1}" ${state.page + 1 >= pages ? "disabled" : ""}>Next</button></div>`);
    }
    function render() {
      renderStatus();
      renderGraph();
      renderDetail();
      renderCatalog();
      els.content.classList.toggle("hidden", state.view === "apis");
      els.catalog.classList.toggle("hidden", state.view !== "apis");
    }
    function handleClick(event) {
      const button = event.target.closest("button");
      if (!button || !els.panel.contains(button)) return;
      const data = button.dataset;
      if (data.serviceRelationOrigin) {
        selectOrigin(data.serviceRelationOrigin);
        state.detailTab = "evidence";
        if (data.serviceRelationEvidence) loadEvidence(data.serviceRelationEvidence);
        else renderDetail();
      } else if (data.serviceEndpoint !== undefined) {
        selectOrigin(data.serviceOrigin, Number(data.serviceEndpoint));
        if (state.view === "apis") { state.view = "service-map"; onViewChange?.("service-map"); render(); }
      } else if (data.serviceOrigin) selectOrigin(data.serviceOrigin);
      else if (data.serviceDetailTab) { state.detailTab = data.serviceDetailTab; if (state.detailTab === "apis") { state.endpointIndex = null; state.endpointSelection = null; } renderDetail(); }
      else if (data.serviceEvidence) loadEvidence(data.serviceEvidence);
      else if (data.servicePage !== undefined) { state.page = Math.max(0, Number(data.servicePage) || 0); renderCatalog(); els.catalog.scrollTop = 0; }
      else if (data.serviceDetailPage !== undefined) { state.detailPage = Math.max(0, Number(data.serviceDetailPage) || 0); renderDetail(); }
    }
    let onViewChange = null;
    els.panel.addEventListener("click", handleClick);
    els.panel.addEventListener("keydown", (event) => {
      const tab = event.target.closest('[role="tab"][data-service-detail-tab]');
      if (!tab || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      state.detailTab = event.key === "Home" ? "apis" : event.key === "End" ? "evidence" : state.detailTab === "apis" ? "evidence" : "apis";
      if (state.detailTab === "apis") { state.endpointIndex = null; state.endpointSelection = null; }
      renderDetail();
      const next = [...els.detail.querySelectorAll('[role="tab"][data-service-detail-tab]')].find((button) => button.dataset.serviceDetailTab === state.detailTab);
      next?.focus();
    });
    els.refresh.addEventListener("click", () => refresh({ force: true }));
    els.search.addEventListener("input", () => { state.query = els.search.value; state.page = 0; render(); });
    els.scope.addEventListener("change", () => {
      state.scopeOnly = els.scope.checked;
      state.page = 0;
      state.snapshot = null;
      state.endpointIndex = null;
      cancelEvidence();
      refresh({ force: true });
    });
    render();
    return {
      refresh, reset, selectOrigin, loadEvidence,
      scopeChanged(previous, next) {
        const signature = (runtime) => JSON.stringify([array(runtime?.scope_patterns), array(runtime?.excluded_scope_patterns)]);
        if (signature(previous) === signature(next)) return false;
        return this.invalidate();
      },
      markDirty() {
        state.historyVersion += 1;
        state.stale = true;
        cancelEvidence();
        renderStatus();
        if (state.detailTab === "evidence") renderDetail();
      },
      setVisible(value, view = state.view) {
        visible = value;
        state.view = view;
        if (state.sessionId !== getSessionId()) reset();
        render();
      },
      setViewChange(callback) { onViewChange = callback; },
      invalidate() {
        state.requestId += 1;
        state.historyVersion += 1;
        abort?.abort();
        state.loading = false;
        state.snapshot = null;
        state.endpointIndex = null;
        cancelEvidence();
        render();
        return refresh({ force: true });
      },
      // State access makes offline lifecycle tests use the same real controller.
      getState: () => state,
    };
  }
  root.SniperServiceMap = { createController, normalizeSnapshot, stableOrder, graphLayout, safeOrigin, safePath, MAX_CARDS, MAX_EDGES, MAX_REFERENCE_CARDS };
})(typeof window === "undefined" ? globalThis : window);
