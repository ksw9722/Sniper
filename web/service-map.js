/* Read-only retained metadata map with explicit, bounded single-source body
   references. No capture, replay, remote fetch, or UUID-only detail navigation. */
(function (root) {
  "use strict";
  const MAX_CARDS = 24;
  const MAX_EDGES = 40;
  const MAX_REFERENCE_CARDS = 100;
  const MAX_REFERENCE_SOURCES = 30;
  const API_PAGE_SIZE = 50;
  const BODY_REFERENCE_LIMITS = Object.freeze({ scan_bytes: 262144, serialized_row_bytes: 2097152, origins: 100, candidates: 4096, candidate_bytes: 4096, offsets_per_origin: 5 });
  const BODY_REFERENCE_REASONS = {
    capture_preview_truncated: "Capture preview is truncated", scan_prefix_truncated: "Scan prefix limit reached",
    candidate_limit: "URL candidate limit reached", candidate_bytes: "An oversized URL candidate was skipped",
    origin_limit: "Origin output limit reached", offset_limit: "Offset output limit reached",
    inactive_session: "Only the active session can be inspected", invalid_evidence_id: "Evidence token is invalid",
    session_mismatch: "The source session changed", stale_generation: "Retained history changed",
    record_missing: "The source row is no longer retained", record_changed: "The source row changed",
    body_identity_unverifiable: "The retained body identity could not be verified",
    body_storage_unavailable: "Retained body storage is unavailable", body_read_failed: "The retained body could not be read",
    serialized_row_limit: "The retained row exceeds the read limit", malformed_retained_row: "The retained row could not be verified",
    response_unavailable: "No retained response is available", retained_preview_unavailable: "No retained response preview is available",
    unsupported_metadata: "Source metadata is unsupported", metadata_limit: "Source metadata exceeds its limit",
    unsupported_content_type: "Only retained HTML and JavaScript text previews are supported",
    unsupported_body_encoding: "The retained preview is not UTF-8 text",
    unsupported_content_encoding: "The retained response encoding is unsupported", body_scan_failed: "The retained preview could not be inspected",
  };
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
  function normalizeBodyReferences(payload, sessionId, evidenceId, row) {
    const invalid = () => { throw new Error("Invalid retained body-reference response"); };
    const integer = (value, max = Number.MAX_SAFE_INTEGER) => Number.isSafeInteger(value) && value >= 0 && value <= max;
    const canonical = (value) => typeof value === "string" && value.length <= BODY_REFERENCE_LIMITS.candidate_bytes && value === safeOrigin(value) && !!value;
    const scanned = payload?.status === "scanned" || payload?.status === "partial";
    if (payload?.schema_version !== 1 || payload.session_id !== sessionId || payload.evidence_id !== evidenceId
      || !["scanned", "partial", "unsupported", "unavailable"].includes(payload.status)
      || payload.available !== scanned || payload.complete !== (payload.status === "scanned")
      || payload.coverage !== "literal_absolute_http_urls" || payload.offset_basis !== "retained_utf8_preview"
      || !Array.isArray(payload.references) || payload.references.length > BODY_REFERENCE_LIMITS.origins
      || !Array.isArray(payload.reasons) || payload.reasons.length > 32
      || payload.reasons.some((reason) => typeof reason !== "string" || !/^[a-z_]{1,80}$/.test(reason))
      || !Object.entries(BODY_REFERENCE_LIMITS).every(([key, value]) => payload.limits?.[key] === value)
      || !integer(payload.scanned_bytes, BODY_REFERENCE_LIMITS.scan_bytes)
      || !integer(payload.candidates_scanned, BODY_REFERENCE_LIMITS.candidates)
      || !integer(payload.omitted_reference_occurrences, payload.candidates_scanned)
      || typeof payload.capture_preview_truncated !== "boolean" || typeof payload.scan_prefix_truncated !== "boolean") invalid();
    if ((scanned || payload.source_origin !== undefined) && (!canonical(payload.source_origin) || payload.source_origin !== row.origin)
      || (scanned || payload.source_sequence !== undefined) && (typeof payload.source_sequence !== "string" || payload.source_sequence !== row.sequence)
      || (scanned || payload.source_started_at !== undefined) && (typeof payload.source_started_at !== "string" || payload.source_started_at !== row.started_at)
      || scanned && (!integer(payload.retained_preview_bytes) || payload.scanned_bytes > payload.retained_preview_bytes)
      || !scanned && (payload.references.length || payload.scanned_bytes || payload.candidates_scanned || payload.omitted_reference_occurrences)
      || payload.status === "scanned" && (payload.reasons.length || payload.capture_preview_truncated || payload.scan_prefix_truncated || payload.omitted_reference_occurrences)
      || payload.status === "partial" && !payload.reasons.length) invalid();
    const seen = new Set();
    let occurrences = payload.omitted_reference_occurrences;
    const references = payload.references.map((item) => {
      if (!canonical(item?.target_origin) || seen.has(item.target_origin)
        || !integer(item.occurrences, payload.candidates_scanned) || !item.occurrences
        || !Array.isArray(item.preview_byte_offsets) || !item.preview_byte_offsets.length || item.preview_byte_offsets.length > BODY_REFERENCE_LIMITS.offsets_per_origin
        || !integer(item.omitted_offsets, item.occurrences) || item.occurrences !== item.preview_byte_offsets.length + item.omitted_offsets
        || payload.status === "scanned" && item.omitted_offsets
        || item.preview_byte_offsets.some((offset, index, offsets) => !integer(offset) || offset >= payload.scanned_bytes || index > 0 && offset <= offsets[index - 1])) invalid();
      seen.add(item.target_origin);
      occurrences += item.occurrences;
      return { target_origin: item.target_origin, occurrences: item.occurrences, preview_byte_offsets: [...item.preview_byte_offsets], omitted_offsets: item.omitted_offsets };
    });
    if (occurrences > payload.candidates_scanned) invalid();
    // Keep a bounded allowlist. Unexpected payload fields, including body content,
    // never become inspector state or markup.
    return { status: payload.status, complete: payload.complete, reasons: [...payload.reasons], references,
      retained_preview_bytes: scanned ? payload.retained_preview_bytes : null, scanned_bytes: payload.scanned_bytes,
      candidates_scanned: payload.candidates_scanned, omitted_reference_occurrences: payload.omitted_reference_occurrences,
      capture_preview_truncated: payload.capture_preview_truncated, scan_prefix_truncated: payload.scan_prefix_truncated };
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
  function graphLayout(origins, order, previousSlots = new Map(), anchor = null, bodyEdges = []) {
    const byOrigin = new Map(origins.map((origin) => [origin.origin, origin]));
    const visibleOrigins = order.filter((id) => byOrigin.has(id)).slice(0, MAX_CARDS).map((id) => byOrigin.get(id));
    const visibleIds = new Set(visibleOrigins.map((origin) => origin.origin));
    const connections = [...bodyEdges.map((edge) => [edge.source, edge.target]), ...visibleOrigins.flatMap((origin) => [
      ...origin.redirects.map((redirect) => [origin.origin, redirect.target_origin]),
      ...origin.header_links.map((link) => [link.source_origin, origin.origin]),
    ])].filter(([source, target]) => source !== target && visibleIds.has(source) && visibleIds.has(target) && (!anchor || source === anchor || target === anchor));
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
    if (anchor && bodyEdges.length) {
      // A temporary single-source overlay is a direct star. Keep every neighbor
      // opposite the source so mixed incoming/outgoing declarations cannot
      // become clipped same-column loops at the fixed canvas edge.
      left = visibleOrigins.filter((origin) => origin.origin === anchor);
      right = visibleOrigins.filter((origin) => origin.origin !== anchor);
    }
    if (anchor) {
      const pin = (items) => [...items.filter((origin) => origin.origin === anchor), ...items.filter((origin) => origin.origin !== anchor)];
      left = pin(left); right = pin(right);
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
    // The explicit overlay gets the first arrow slots. Metadata remains distinct
    // even when the same pair also occurs in a retained response body.
    const declaredEdges = [...bodyEdges, ...visible.flatMap((origin) => [
      ...origin.redirects.map((redirect) => ({ source: origin.origin, target: redirect.target_origin, relationship: "redirect_instruction", label: "Redirect instruction" })),
      ...origin.header_links.map((link) => ({ source: link.source_origin, target: origin.origin, relationship: link.relationship, label: link.relationship === "origin_header" ? "Origin header" : "Referer header" })),
    ])];
    const pairCounts = new Map();
    for (const edge of declaredEdges) {
      const source = positions.get(edge.source);
      const target = positions.get(edge.target);
      const key = JSON.stringify([edge.source, edge.target, edge.relationship]);
      if (!source || !target || target === source || seen.has(key) || anchor && edge.source !== anchor && edge.target !== anchor) continue;
      seen.add(key);
      edgeCount += 1;
      if (edges.length >= MAX_EDGES) continue;
      const pair = JSON.stringify([edge.source, edge.target].sort());
      const pairIndex = pairCounts.get(pair) || 0;
      pairCounts.set(pair, pairIndex + 1);
      const offset = [0, -12, 12, -24, 24, -36, 36][Math.min(pairIndex, 6)];
      const toRight = target.x > source.x;
      const sameColumn = target.x === source.x;
      const x1 = source.x + (sameColumn ? 276 : toRight ? 276 : 0);
      const x2 = target.x + (sameColumn ? 276 : toRight ? 0 : 276);
      const y1 = source.y + 72 + offset;
      const y2 = target.y + 72 + offset;
      const bend = sameColumn ? x1 + 32 + Math.abs(offset) : (x1 + x2) / 2;
      edges.push({ ...edge, path: `M ${x1} ${y1} C ${bend} ${y1}, ${bend} ${y2}, ${x2} ${y2}` });
    }
    return { nodes: visible, edges, edgeCount, slots, width: 720, height: Math.max(400, ...visible.map((origin) => origin.y + 174)) };
  }
  function* metadataRelations(origins) {
    for (const origin of origins) {
      for (const link of origin.header_links) yield {
        sourceOrigin: link.source_origin, targetOrigin: origin.origin, capturedOrigin: origin.origin,
        relationship: link.relationship, label: `${link.relationship === "origin_header" ? "Origin" : "Referer"} header declaration`, evidence: link.evidence[0] || "",
      };
      for (const redirect of origin.redirects) yield {
        sourceOrigin: origin.origin, targetOrigin: redirect.target_origin, targetPath: redirect.target_path, capturedOrigin: origin.origin,
        relationship: "redirect_instruction", label: "Redirect instruction", evidence: redirect.evidence[0] || "",
      };
    }
  }
  const referenceOrigin = (id) => ({ origin: id, captured: false, in_scope: false, endpoints: [], technology_clues: [], redirects: [], header_links: [], references: [], referencesOmitted: 0 });
  function createController({ elements: els, getSessionId, getEventStreamReady = () => true, request, document: doc = root.document }) {
    const state = {
      sessionId: null, snapshot: null, order: [], selectedOrigin: null, selectionInitialized: false,
      query: "", scopeOnly: false, view: "service-map", detailTab: "apis", endpointIndex: null, endpointSelection: null,
      loading: false, error: "", requestId: 0, evidenceRequestId: 0, evidence: null,
      evidenceLoading: false, evidenceError: "", evidenceId: null, page: 0, detailPage: 0,
      bodyReferences: null, bodyReferencesLoading: false, bodyReferencesError: "", bodyReferencesRequestId: 0,
      bodyReferencesSource: null, bodyReferenceOverlay: null, bodyOverlayNotice: "",
      historyVersion: 0, stale: false, anchorOrigin: null, modeNotice: "",
    };
    let abort = null;
    let evidenceAbort = null;
    let bodyReferencesAbort = null;
    let detailRender = null;
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
          origins.set(id, referenceOrigin(id));
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
    function findOrigin(id) {
      if (!id) return null;
      const existing = graphOrigins().find((origin) => origin.origin === id);
      if (existing) return existing;
      if (state.extraOriginSnapshot === state.snapshot && state.extraOriginId === id && state.extraOriginOverlay === state.bodyReferenceOverlay) return state.extraOrigin;
      // A direct neighbor can be outside the All origins reference-card budget.
      // Its inspector still uses only loaded declarations and the usual pane caps.
      const item = referenceOrigin(id), group = { items: [], omitted: 0 }, seen = new Set();
      const bodyReference = state.bodyReferenceOverlay?.scan.references.find((reference) => reference.target_origin === id);
      if (bodyReference) item.references.push({ origin: state.bodyReferenceOverlay.source.origin, label: "Literal body URL reference from" });
      for (const relation of metadataRelations(state.snapshot?.origins || [])) {
        if (relation.sourceOrigin !== id && relation.targetOrigin !== id) continue;
        if (group.items.length < MAX_REFERENCE_SOURCES) group.items.push(relation); else group.omitted += 1;
        const label = relation.relationship === "redirect_instruction" ? "Redirect instruction from" : `${relation.relationship === "origin_header" ? "Origin" : "Referer"} header on`;
        const key = JSON.stringify([relation.capturedOrigin, label]);
        if (seen.has(key)) continue;
        seen.add(key);
        if (item.references.length < MAX_REFERENCE_SOURCES) item.references.push({ origin: relation.capturedOrigin, label }); else item.referencesOmitted += 1;
      }
      state.extraOriginSnapshot = state.snapshot; state.extraOriginId = id;
      state.extraOriginOverlay = state.bodyReferenceOverlay;
      state.extraOrigin = group.items.length || bodyReference ? item : null;
      if (group.items.length) state.relationsByOrigin.set(id, group);
      return state.extraOrigin;
    }
    function selectedOrigin() {
      return findOrigin(state.selectedOrigin);
    }
    function overlayEdges() {
      const overlay = state.bodyReferenceOverlay;
      return overlay ? overlay.scan.references.filter((reference) => reference.target_origin !== overlay.source.origin).map((reference) => ({
        source: overlay.source.origin, target: reference.target_origin, relationship: "body_url_reference", label: "Literal body URL reference",
      })) : [];
    }
    function sourceIsRetained(source, snapshot = state.snapshot) {
      if (!source || source.sessionId !== getSessionId() || source.sessionId !== state.sessionId
        || typeof source.generation !== "string" || !source.generation || source.generation !== snapshot?.snapshot.generation) return false;
      const origin = snapshot.origins.find((item) => item.origin === source.origin);
      return !!origin && [...origin.endpoints, ...origin.technology_clues, ...origin.redirects, ...origin.header_links]
        .some((item) => item.evidence.includes(source.evidenceId));
    }
    function clearOverlay(notice = "") {
      if (!state.bodyReferenceOverlay) return false;
      state.bodyReferenceOverlay = null;
      state.neighborhoodOverlay = null;
      state.neighborhoodSnapshot = null;
      state.neighborhoodAnchor = null;
      state.neighborhood = null;
      state.extraOriginOverlay = null;
      state.extraOriginSnapshot = null;
      state.extraOriginId = null;
      state.extraOrigin = null;
      state.layoutSlots = new Map();
      state.bodyOverlayNotice = notice;
      if (state.selectedOrigin && !findOrigin(state.selectedOrigin)) {
        cancelEvidence();
        state.selectedOrigin = null;
        state.endpointIndex = null;
        state.endpointSelection = null;
      }
      return true;
    }
    function clearBodyReferenceOverlay() {
      const cleared = clearOverlay("Temporary body references cleared. Showing loaded metadata only.");
      render();
      return cleared;
    }
    function showBodyReferencesOnMap() {
      if (state.sessionId !== getSessionId()) { reset(); return false; }
      const scan = state.bodyReferences, source = state.bodyReferencesSource;
      if (!visible || !getEventStreamReady() || state.error || state.stale || !scan || !["scanned", "partial"].includes(scan.status) || !sourceIsRetained(source)
        || state.evidenceId !== source.evidenceId || state.selectedOrigin !== source.origin
        || !scan.references.some((item) => item.target_origin !== source.origin)) return false;
      clearOverlay();
      // Copy only already-validated fields, never the response row or raw body.
      // The overlay must survive an unrelated inspector selection unchanged.
      const references = Object.freeze(scan.references.map((item) => Object.freeze({
        target_origin: item.target_origin, occurrences: item.occurrences,
        preview_byte_offsets: Object.freeze([...item.preview_byte_offsets]), omitted_offsets: item.omitted_offsets,
      })));
      state.bodyReferenceOverlay = Object.freeze({ source: Object.freeze({ ...source }), scan: Object.freeze({
        status: scan.status, complete: scan.complete, reasons: Object.freeze([...scan.reasons]), references,
        retained_preview_bytes: scan.retained_preview_bytes, scanned_bytes: scan.scanned_bytes,
        candidates_scanned: scan.candidates_scanned, omitted_reference_occurrences: scan.omitted_reference_occurrences,
        capture_preview_truncated: scan.capture_preview_truncated, scan_prefix_truncated: scan.scan_prefix_truncated,
      }) });
      state.bodyOverlayNotice = "";
      state.layoutSlots = new Map();
      state.modeNotice = "";
      state.anchorOrigin = source.origin;
      if (state.view !== "service-map") {
        state.view = "service-map";
        onViewChange?.("service-map");
      }
      render();
      return true;
    }
    function directNeighborhood() {
      if (!state.anchorOrigin || !state.snapshot) {
        state.neighborhoodSnapshot = null; state.neighborhoodAnchor = null; state.neighborhood = null;
        return null;
      }
      if (state.neighborhoodSnapshot === state.snapshot && state.neighborhoodAnchor === state.anchorOrigin && state.neighborhoodOverlay === state.bodyReferenceOverlay) return state.neighborhood;
      const anchor = findOrigin(state.anchorOrigin);
      if (!anchor) return null;
      const ids = new Set(), seen = new Set();
      const counts = { incoming: 0, outgoing: 0, redirect_instruction: 0, origin_header: 0, referer_header: 0, body_url_reference: 0 };
      for (const relation of metadataRelations(state.snapshot.origins)) {
        const { sourceOrigin, targetOrigin, relationship } = relation;
        if (sourceOrigin === targetOrigin || sourceOrigin !== anchor.origin && targetOrigin !== anchor.origin) continue;
        ids.add(sourceOrigin === anchor.origin ? targetOrigin : sourceOrigin);
        const key = JSON.stringify([sourceOrigin, targetOrigin, relationship]);
        if (seen.has(key)) continue;
        seen.add(key);
        counts[sourceOrigin === anchor.origin ? "outgoing" : "incoming"] += 1;
        counts[relationship] += 1;
      }
      const bodyIds = new Set();
      for (const edge of overlayEdges()) {
        if (edge.source !== anchor.origin) continue;
        ids.add(edge.target); bodyIds.add(edge.target);
        seen.add(JSON.stringify([edge.source, edge.target, edge.relationship]));
        counts.outgoing += 1; counts.body_url_reference += 1;
      }
      // Count the full loaded snapshot, not the capped relationship inspector or
      // All origins reference cache. Only rendering is clipped to 24 cards/40 arrows.
      const byOrigin = new Map(graphOrigins().map((origin) => [origin.origin, origin]));
      const neighbors = [...ids].sort().map((id) => byOrigin.get(id) || referenceOrigin(id));
      state.neighborhoodSnapshot = state.snapshot; state.neighborhoodAnchor = state.anchorOrigin;
      state.neighborhoodOverlay = state.bodyReferenceOverlay;
      state.neighborhood = { anchor, neighbors, counts, bodyIds, edgeCount: seen.size };
      return state.neighborhood;
    }
    function setMapMode(direct) {
      if (state.sessionId !== getSessionId()) { reset(); return; }
      if (direct) {
        if (state.anchorOrigin || !selectedOrigin()) return;
        state.anchorOrigin = state.selectedOrigin;
      } else {
        clearOverlay("Temporary body references cleared by All origins.");
        state.anchorOrigin = null;
      }
      state.modeNotice = "";
      render();
    }
    function focusSelection() {
      if (state.sessionId !== getSessionId()) { reset(); return; }
      if (!state.anchorOrigin || !selectedOrigin()) return;
      if (state.anchorOrigin !== state.selectedOrigin) clearOverlay("Temporary body references cleared when the pinned origin changed.");
      if (!selectedOrigin()) { render(); return; }
      state.anchorOrigin = state.selectedOrigin;
      state.modeNotice = "";
      render();
    }
    function cancelBodyReferences() {
      state.bodyReferencesRequestId += 1;
      bodyReferencesAbort?.abort();
      bodyReferencesAbort = null;
      state.bodyReferences = null;
      state.bodyReferencesSource = null;
      state.bodyReferencesLoading = false;
      state.bodyReferencesError = "";
    }
    function cancelEvidence() {
      cancelBodyReferences();
      state.evidenceRequestId += 1;
      evidenceAbort?.abort();
      evidenceAbort = null;
      state.evidence = null;
      state.evidenceLoading = false;
      state.evidenceError = "";
      state.evidenceId = null;
    }
    function reset() {
      clearOverlay("Temporary body references cleared because the session changed.");
      state.requestId += 1;
      abort?.abort();
      abort = null;
      cancelEvidence();
      Object.assign(state, { sessionId: getSessionId(), snapshot: null, order: [], selectedOrigin: null,
        selectionInitialized: false, endpointIndex: null, endpointSelection: null, loading: false, error: "", page: 0, detailPage: 0, stale: false, anchorOrigin: null, modeNotice: "" });
      render();
    }
    function selectOrigin(origin, endpointIndex = null) {
      const item = findOrigin(origin);
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
      // The app polls every 1.2 seconds. A same-source background snapshot must
      // not erase an explicit inspection or continually abort its pending read.
      if (force) {
        clearOverlay("Temporary body references cleared by Refresh. Inspect and show references again when ready.");
        cancelBodyReferences();
      }
      abort?.abort();
      abort = new AbortController();
      const requestId = ++state.requestId;
      const scopeOnly = state.scopeOnly;
      const historyVersion = state.historyVersion;
      state.loading = true;
      state.error = "";
      renderStatus();
      if (!state.snapshot || force) render();
      try {
        const params = new URLSearchParams({ session_id: sessionId, in_scope_only: String(scopeOnly), max_records: "10000" });
        const response = await request(`/api/target/service-map?${params}`, { signal: abort.signal });
        if (!response.ok) throw new Error("Service map unavailable");
        const snapshot = normalizeSnapshot(await response.json(), sessionId);
        if (requestId !== state.requestId || sessionId !== getSessionId() || scopeOnly !== state.scopeOnly) return false;
        if (state.bodyReferenceOverlay && !sourceIsRetained(state.bodyReferenceOverlay.source, snapshot)) {
          clearOverlay("Temporary body references cleared: the exact source is absent or retained history changed.");
        }
        if (state.snapshot && state.snapshot.snapshot.generation !== snapshot.snapshot.generation) cancelEvidence();
        state.snapshot = snapshot;
        state.stale = historyVersion !== state.historyVersion;
        const allOrigins = graphOrigins();
        state.order = state.order.length ? stableOrder(state.order, allOrigins) : [
          ...snapshot.origins.map((origin) => origin.origin).sort(),
          ...allOrigins.filter((origin) => !origin.captured).map((origin) => origin.origin).sort(),
        ];
        if (state.anchorOrigin && !findOrigin(state.anchorOrigin)) {
          state.anchorOrigin = null;
          state.modeNotice = "Direct relations cleared: the pinned origin is absent from this snapshot.";
        }
        if (!findOrigin(state.selectedOrigin)) {
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
        clearOverlay("Temporary body references cleared because the current refresh failed.");
        cancelBodyReferences();
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
      const overlaySource = () => state.bodyReferenceOverlay?.source.evidenceId === id
        && state.bodyReferenceOverlay.source.origin === selected ? state.bodyReferenceOverlay.source : null;
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
        const source = overlaySource();
        if (result.available && source && (row.method !== source.method || safePath(row.path) !== source.path
          || row.sequence !== source.sequence || row.started_at !== source.startedAt)) throw new Error("Overlay source changed");
        state.evidence = result.available === true ? row : null;
        state.evidenceError = state.evidence ? "" : "Source unavailable. The retained row may have changed or been removed. Refresh the map to select current evidence.";
        state.evidenceLoading = false;
        if (!state.evidence && source) {
          clearOverlay("Temporary body references cleared: the exact inspected source is no longer available.");
          render();
        } else renderDetail();
        return !!state.evidence;
      } catch (_) {
        if (requestId !== state.evidenceRequestId || sessionId !== getSessionId() || selected !== state.selectedOrigin) return false;
        state.evidenceLoading = false;
        state.evidenceError = "Source unavailable. The exact retained row could not be verified. Refresh the map and try again.";
        if (overlaySource()) {
          clearOverlay("Temporary body references cleared: the exact inspected source could not be verified.");
          render();
        } else renderDetail();
        return false;
      }
    }
    async function loadBodyReferences() {
      const row = state.evidence;
      const sessionId = getSessionId();
      if (!visible || state.sessionId !== sessionId || !row || state.evidenceId !== row.evidence_id
        || state.evidenceLoading || state.bodyReferencesLoading
        || !evidenceItems(selectedOrigin()).some((item) => item.id === row.evidence_id)) return false;
      cancelBodyReferences();
      const requestId = ++state.bodyReferencesRequestId;
      const evidenceId = row.evidence_id;
      const selected = state.selectedOrigin;
      const generation = state.snapshot?.snapshot.generation;
      const readsOverlaySource = () => state.bodyReferenceOverlay?.source.evidenceId === evidenceId
        && state.bodyReferenceOverlay.source.origin === selected;
      const current = () => requestId === state.bodyReferencesRequestId && visible && sessionId === getSessionId()
        && sessionId === state.sessionId && selected === state.selectedOrigin && row === state.evidence && evidenceId === state.evidenceId
        && generation === state.snapshot?.snapshot.generation;
      state.bodyReferencesLoading = true;
      bodyReferencesAbort = new AbortController();
      renderDetail();
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: evidenceId });
        const response = await request(`/api/target/service-map/body-references?${params}`, { signal: bodyReferencesAbort.signal });
        if (!response.ok) throw new Error("Retained body references unavailable");
        const payload = await response.json();
        if (!current()) return false;
        state.bodyReferences = normalizeBodyReferences(payload, sessionId, evidenceId, row);
        state.bodyReferencesSource = Object.freeze({ sessionId, generation, evidenceId, origin: safeOrigin(row.origin),
          method: row.method, path: safePath(row.path), sequence: row.sequence, startedAt: row.started_at });
        state.bodyReferencesLoading = false;
        bodyReferencesAbort = null;
        if (!["scanned", "partial"].includes(state.bodyReferences.status) && readsOverlaySource()) {
          clearOverlay("Temporary body references cleared: the inspected source preview is no longer available or supported.");
          render();
        } else renderDetail();
        return true;
      } catch (_) {
        if (!current()) return false;
        state.bodyReferencesLoading = false;
        bodyReferencesAbort = null;
        state.bodyReferencesError = "Could not verify retained body references. Retry explicitly, or refresh the map and select the source again.";
        if (readsOverlaySource()) {
          clearOverlay("Temporary body references cleared: the inspected source preview could not be verified.");
          render();
        } else renderDetail();
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
        if (state.snapshot.warningCount) status += ` · ${state.snapshot.warningCount} metadata notice(s)`;
      }
      els.status.textContent = state.error ? `${state.error}${snapshot ? ` ${status}` : ""}` : status;
      els.status.classList.toggle("service-map-error", !!state.error);
      els.refresh.disabled = state.loading;
      els.refresh.textContent = state.loading ? "Refreshing…" : "Refresh";
      els.panel.setAttribute("aria-busy", String(state.loading));
    }
    function renderGraph() {
      const selected = selectedOrigin();
      const direct = directNeighborhood();
      const query = state.query.trim().toLowerCase();
      const matchingNeighbors = direct?.neighbors.filter((origin) => !query || origin.origin.toLowerCase().includes(query)) || [];
      const origins = direct ? [direct.anchor, ...matchingNeighbors] : filteredOrigins();
      const neighborOrder = direct ? stableOrder(state.order, matchingNeighbors) : [];
      const order = direct ? [direct.anchor.origin, ...neighborOrder.filter((id) => direct.bodyIds.has(id)), ...neighborOrder.filter((id) => !direct.bodyIds.has(id))] : state.order;
      const filterKey = JSON.stringify([query, state.scopeOnly, state.sessionId, state.anchorOrigin]);
      if (state.layoutFilter !== filterKey) { state.layoutSlots = new Map(); state.layoutFilter = filterKey; }
      const layout = graphLayout(origins, order, state.layoutSlots, state.anchorOrigin, direct ? overlayEdges() : []);
      state.layoutSlots = layout.slots;
      els.boundsDetails?.classList.toggle("hidden", !direct);
      if (!direct && els.graphDiagnostics) els.graphDiagnostics.textContent = "";
      els.mapControls?.classList.toggle("hidden", state.view === "apis");
      els.allOrigins?.setAttribute("aria-pressed", String(!state.anchorOrigin));
      els.directRelations?.setAttribute("aria-pressed", String(!!state.anchorOrigin));
      if (els.directRelations) els.directRelations.disabled = !state.snapshot || !state.anchorOrigin && !selected;
      if (els.focusSelection) {
        els.focusSelection.classList.toggle("hidden", !state.anchorOrigin);
        els.focusSelection.disabled = !selected || selected.origin === state.anchorOrigin;
        els.focusSelection.title = selected ? `Pin ${selected.origin} without changing its inspector` : "Select an origin to pin it";
      }
      const selectionNotice = !selected || layout.nodes.some((origin) => origin.origin === selected.origin) ? ""
        : direct && !direct.neighbors.some((origin) => origin.origin === selected.origin) ? "Selected origin is outside this direct neighborhood; inspector retained."
        : origins.some((origin) => origin.origin === selected.origin)
        ? `Selected origin is outside the ${MAX_CARDS}-card display; inspector retained.`
        : !direct && !graphOrigins().some((origin) => origin.origin === selected.origin) && (!query || selected.origin.toLowerCase().includes(query))
          ? `Selected origin is outside the ${MAX_REFERENCE_CARDS}-reference-card limit; inspector retained.`
          : "Selected origin is outside this filter; inspector retained.";
      if (!origins.length) {
        replace(els.canvas, `<div class="service-map-empty"><span class="service-map-empty-symbol" aria-hidden="true">◎</span><h3>${state.loading ? "Loading service origins…" : state.error && !state.snapshot ? "Service map unavailable" : state.query || state.scopeOnly ? "No matching origins" : "No retained HTTP origins"}</h3><p>${state.query || state.scopeOnly ? "Change the origin filter or turn off In scope only." : "This view uses only HTTP history already retained in this session."}</p></div>`);
        els.graphBounds.textContent = [state.anchorOrigin ? "Direct relations will resume after a fresh snapshot verifies the pinned origin." : state.modeNotice, selectionNotice].filter(Boolean).join(" · ");
        return;
      }
      const edges = layout.edges.map((edge) => `<path class="service-map-edge ${edge.relationship === "body_url_reference" ? "body-reference" : edge.relationship !== "redirect_instruction" ? "header-declaration" : ""} ${[edge.source, edge.target].includes(state.selectedOrigin) ? "selected" : ""}" data-relationship="${edge.relationship}" d="${edge.path}" marker-end="url(#service-map-arrow)"><title>${escape(edge.source)} → ${escape(edge.target)} · ${escape(edge.label)} · ${edge.relationship}</title></path>`).join("");
      const cards = layout.nodes.map((origin) => {
        const selected = origin.origin === state.selectedOrigin;
        const url = new URL(origin.origin);
        const pinned = origin.origin === state.anchorOrigin;
        return `<button class="service-map-card ${!origin.captured ? "referenced" : ""} ${selected ? "selected" : ""} ${pinned ? "pinned" : ""}" type="button" data-service-origin="${escape(origin.origin)}" data-service-focus="origin:${escape(origin.origin)}" aria-current="${selected}" style="left:${origin.x}px;top:${origin.y}px">
          <span class="service-map-card-symbol" aria-hidden="true">${pinned ? "⊙" : "◎"}</span><span class="service-map-card-copy"><strong title="${escape(origin.origin)}">${escape(url.host)}</strong><span class="service-map-card-scheme">${escape(url.protocol.slice(0, -1))} · ${pinned ? "pinned anchor" : origin.captured ? "captured origin" : "referenced origin"}</span><span class="service-map-clues">${origin.technology_clues.slice(0, 2).map((clue) => `<span title="Observed response header; not verified architecture">${escape(clue.name)} · header clue</span>`).join("")}</span><span class="service-map-card-meta">${origin.captured ? `${origin.endpoints.length} API paths · ${origin.request_count.toLocaleString()} sampled rows` : direct?.bodyIds.has(origin.origin) ? "Body URL reference · no captured rows" : "Referenced only in this snapshot"}</span></span>
          <span class="service-map-scope-dot ${!origin.captured ? "hidden" : ""} ${origin.in_scope ? "in-scope" : ""}" title="${origin.in_scope ? "In scope" : "Out of scope"}"></span>
        </button>`;
      }).join("");
      replace(els.canvas, `<div class="service-map-layout" style="width:${layout.width}px;height:${layout.height}px"><svg class="service-map-edges" width="${layout.width}" height="${layout.height}" aria-hidden="true"><defs><marker id="service-map-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M1,1 L7,4 L1,7" fill="none" stroke="currentColor" stroke-width="1.5" /></marker></defs>${edges}</svg>${cards}</div>`);
      if (direct) {
        const counts = direct.counts;
        const shownBodyArrows = layout.edges.filter((edge) => edge.relationship === "body_url_reference").length;
        const shownMetadataArrows = layout.edges.length - shownBodyArrows;
        const metadataArrows = direct.edgeCount - counts.body_url_reference;
        const visibleIds = new Set(layout.nodes.map((origin) => origin.origin));
        const availableBodyArrows = overlayEdges().filter((edge) => visibleIds.has(edge.source) && visibleIds.has(edge.target)).length;
        const availableMetadataArrows = layout.edgeCount - availableBodyArrows;
        const queryHidden = direct.neighbors.length - matchingNeighbors.length;
        const capOmitted = Math.max(0, matchingNeighbors.length - (MAX_CARDS - 1));
        const incomplete = state.snapshot.snapshot.complete !== true || array(state.snapshot.snapshot.stopped_by).length > 0;
        const outsideQuery = query && !direct.anchor.origin.toLowerCase().includes(query);
        const summary = [
          `${layout.nodes.length - 1} / ${direct.neighbors.length} known direct neighbors shown`,
          `${counts.incoming} incoming / ${counts.outgoing} outgoing links`,
          state.bodyReferenceOverlay ? `${direct.edgeCount - counts.body_url_reference} metadata / ${counts.body_url_reference} body URL links` : "",
          state.bodyReferenceOverlay ? `${shownBodyArrows} / ${counts.body_url_reference} body arrows shown` : "",
          queryHidden ? `${queryHidden} hidden by query` : "",
          capOmitted ? `${capOmitted} omitted by ${MAX_CARDS}-card cap` : "",
          layout.edgeCount > MAX_EDGES ? `${layout.edgeCount - MAX_EDGES} arrows omitted by ${MAX_EDGES}-arrow cap` : "",
          outsideQuery ? "Pinned anchor is outside the search query and remains visible." : "",
          !direct.anchor.captured ? "Reference-only anchor; no captured rows shown." : "",
          !direct.neighbors.length ? "No direct neighbors appear in loaded metadata; others may exist." : "",
          incomplete ? "Known neighborhood is incomplete; see snapshot bounds above." : "",
          selectionNotice,
        ];
        const diagnostics = [
          `Direct relations · ${direct.neighbors.length} known direct neighbors in loaded metadata${state.bodyReferenceOverlay ? " and temporary body references (unique union)" : ""}`,
          `${layout.nodes.length - 1} shown · ${queryHidden} query-hidden · ${capOmitted} omitted by ${MAX_CARDS}-card cap (anchor included)`,
          `Non-self distinct typed links: ${counts.incoming} incoming / ${counts.outgoing} outgoing · ${counts.redirect_instruction} redirect / ${counts.origin_header} Origin / ${counts.referer_header} Referer${state.bodyReferenceOverlay ? ` / ${counts.body_url_reference} body_url_reference` : ""}`,
          `${layout.edges.length} / ${layout.edgeCount} arrows between displayed cards (cap ${MAX_EDGES}) · ${direct.edgeCount - layout.edgeCount} links outside displayed cards`,
          ...(state.bodyReferenceOverlay ? [
            `Body URL typed arrows: ${shownBodyArrows} / ${counts.body_url_reference} shown · ${counts.body_url_reference - availableBodyArrows} outside displayed cards · ${availableBodyArrows - shownBodyArrows} omitted by arrow cap`,
            `Metadata typed arrows: ${shownMetadataArrows} / ${metadataArrows} shown · ${metadataArrows - availableMetadataArrows} outside displayed cards · ${availableMetadataArrows - shownMetadataArrows} omitted by arrow cap`,
            "Body targets and arrows take priority within the same 24-card and 40-arrow limits.",
          ] : []),
          "Links count distinct origin pairs and relationship types, not requests, URL occurrences or redirect paths. Incoming and outgoing are relative to the pinned anchor; self-links are excluded.",
          state.bodyReferenceOverlay ? "Loaded metadata and one explicit retained-preview scan only; no transitive links or confirmed calls." : "Loaded metadata only; no transitive links or confirmed calls.",
        ];
        els.graphBounds.textContent = summary.filter(Boolean).join(" · ");
        if (els.graphDiagnostics) els.graphDiagnostics.textContent = diagnostics.join(" · ");
      } else els.graphBounds.textContent = `${layout.nodes.length} / ${origins.length} matching origins${state.omittedGraphReferences ? ` · Reference-card cap ${MAX_REFERENCE_CARDS}; ${state.omittedGraphReferences} reference declarations omitted` : ""}${origins.length > MAX_CARDS ? " · Filter to find more" : ""}${layout.edgeCount > MAX_EDGES ? ` · ${MAX_EDGES} / ${layout.edgeCount} arrows shown` : ""}${selectionNotice ? ` · ${selectionNotice}` : ""}${state.modeNotice ? ` · ${state.modeNotice}` : ""}`;
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
    function bodyReferencesMarkup() {
      const result = state.bodyReferences;
      const scanned = result?.status === "scanned" || result?.status === "partial";
      const hasNeighbors = scanned && result.references.some((item) => item.target_origin !== state.bodyReferencesSource?.origin);
      const showing = state.bodyReferenceOverlay?.source.evidenceId === state.bodyReferencesSource?.evidenceId
        && state.bodyReferenceOverlay?.source.origin === state.bodyReferencesSource?.origin && !!state.bodyReferenceOverlay;
      const showDisabled = !scanned || !hasNeighbors || !getEventStreamReady() || !!state.error || state.stale || !sourceIsRetained(state.bodyReferencesSource);
      const showHint = !getEventStreamReady() ? "Wait for the session event connection before showing temporary references."
        : state.error || state.stale ? "Wait for a current metadata snapshot, or use Refresh, before showing temporary references."
        : scanned && !hasNeighbors ? result.references.length ? "Only the source origin was referenced; self-links are not drawn." : "There are no referenced origins to show on the map."
        : scanned ? "Show pins this exact source in Direct relations. One temporary overlay at a time; no additional read."
        : "Inspect a supported retained preview successfully before showing references on the map.";
      const buttonLabel = state.bodyReferencesLoading ? "Inspecting retained body references…" : state.bodyReferencesError || result?.status === "unavailable" ? "Retry retained body references" : result ? "Inspect retained body references again" : "Inspect retained body references";
      let summary = "";
      if (result) {
        const scanned = result.status === "scanned" || result.status === "partial";
        const occurrences = result.references.reduce((total, item) => total + item.occurrences, 0);
        const message = result.status === "unsupported" ? "Retained preview is unsupported. No references were inspected."
          : result.status === "unavailable" ? "Retained preview is unavailable. No references were inspected."
          : result.references.length ? `${result.references.length} referenced origin${result.references.length === 1 ? "" : "s"} · ${occurrences.toLocaleString()} literal URL occurrence${occurrences === 1 ? "" : "s"}`
          : "No literal absolute HTTP(S) URL references found in the inspected preview.";
        summary = `<div class="service-map-body-reference-result" role="status"><p>${escape(message)}</p>${scanned ? `<p class="service-map-muted">${result.complete ? "Complete retained-preview scan" : "Partial retained-preview scan"} · ${result.scanned_bytes.toLocaleString()} / ${result.retained_preview_bytes.toLocaleString()} UTF-8 preview bytes inspected${result.omitted_reference_occurrences ? ` · ${result.omitted_reference_occurrences.toLocaleString()} reference occurrence(s) omitted` : ""}</p>` : ""}${result.reasons.length ? `<p class="service-map-muted">${result.reasons.map((reason) => escape(BODY_REFERENCE_REASONS[reason] || reason.replace(/_/g, " "))).join(" · ")}</p>` : ""}</div>
          ${result.references.length ? `<ul class="service-map-body-reference-list" aria-label="Literal URL reference origins">${result.references.map((item) => `<li><span class="service-map-body-reference-origin">${escape(item.target_origin)}</span><span>${item.occurrences.toLocaleString()} occurrence${item.occurrences === 1 ? "" : "s"} · Preview byte offsets: ${item.preview_byte_offsets.join(", ")}${item.omitted_offsets ? ` · ${item.omitted_offsets.toLocaleString()} more offset(s) omitted` : ""}</span></li>`).join("")}</ul>` : ""}`;
      }
      return `<section class="service-map-body-references" aria-label="Retained body references" aria-busy="${state.bodyReferencesLoading}"><h4>Retained body references</h4><p class="service-map-muted">Optional inspection of this exact response’s retained HTML or JavaScript preview. Active session only. A literal URL reference does not prove a request, call, or service ownership.</p><button class="secondary-action" type="button" data-service-body-references="inspect" data-service-focus="body-references:inspect" ${state.bodyReferencesLoading ? "disabled" : ""}>${buttonLabel}</button>${state.bodyReferencesLoading ? '<p class="service-map-muted" role="status">Inspecting one retained preview locally…</p>' : ""}${state.bodyReferencesError ? `<p class="service-map-error" role="status">${escape(state.bodyReferencesError)}</p>` : ""}${summary}<button class="secondary-action" type="button" data-service-show-body-references="show" data-service-focus="body-references:show" aria-pressed="${showing}" aria-describedby="serviceMapBodyShowHint" ${showDisabled ? "disabled" : ""}>Show references on map</button><p class="service-map-muted" id="serviceMapBodyShowHint">${showHint}</p><details class="service-map-body-reference-limits"><summary data-service-focus="body-references:limits">Coverage and limits</summary><p>Literal absolute HTTP(S) URLs only. Origins are text, never fetched. No script execution or relative URL resolution. Only Show references on map adds temporary Direct relations. Offsets are zero-based bytes relative to the retained UTF-8 preview, not the original response.</p><p>Scan: ${BODY_REFERENCE_LIMITS.scan_bytes.toLocaleString()} bytes · Serialized row read: ${BODY_REFERENCE_LIMITS.serialized_row_bytes.toLocaleString()} bytes · Origins: ${BODY_REFERENCE_LIMITS.origins} · URL candidates: ${BODY_REFERENCE_LIMITS.candidates.toLocaleString()} · Candidate: ${BODY_REFERENCE_LIMITS.candidate_bytes.toLocaleString()} bytes · Offsets per origin: ${BODY_REFERENCE_LIMITS.offsets_per_origin}.</p>${result ? `<p>${result.candidates_scanned.toLocaleString()} candidates inspected · Capture preview truncated: ${result.capture_preview_truncated ? "yes" : "no"} · Scan prefix truncated: ${result.scan_prefix_truncated ? "yes" : "no"}.</p>` : ""}</details></section>`;
    }
    function overlayProvenanceMarkup() {
      const overlay = state.bodyReferenceOverlay;
      if (!overlay) return "";
      const { source, scan } = overlay;
      return `<p class="service-map-overlay-source" title="${escape(`${source.method} ${source.origin}${source.path}`)}">${escape(source.method)} ${escape(source.origin)}${escape(source.path)}</p><p>Sequence ${escape(source.sequence)} · ${escape(source.startedAt)} · ${scan.complete ? "Complete" : "Partial"} retained-preview scan</p>${!scan.complete ? `<p class="service-map-muted">${scan.reasons.map((reason) => escape(BODY_REFERENCE_REASONS[reason] || reason.replace(/_/g, " "))).join(" · ")}${scan.omitted_reference_occurrences ? ` · ${scan.omitted_reference_occurrences} reference occurrence(s) omitted` : ""}</p>` : ""}`;
    }
    function renderOverlayStatus() {
      if (!els.overlayStatus) return;
      const overlay = state.bodyReferenceOverlay;
      els.overlayStatus.classList.toggle("hidden", !overlay && !state.bodyOverlayNotice);
      replace(els.overlayStatus, overlay ? `<div><strong>Temporary body URL references</strong>${overlayProvenanceMarkup()}<p>Literal references only; no confirmed requests. Changes to history clear this view.</p></div><div class="service-map-overlay-actions"><button class="secondary-action" type="button" data-service-relation-origin="${escape(overlay.source.origin)}" data-service-relation-evidence="${escape(overlay.source.evidenceId)}" data-service-focus="body-references:source">View inspected source</button><button class="secondary-action" type="button" data-service-clear-body-references="clear" data-service-focus="body-references:clear">Clear body references</button></div>` : `<p role="status">${escape(state.bodyOverlayNotice)}</p>`);
    }
    function evidenceMarkup(origin) {
      const items = evidenceItems(origin);
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      const row = state.evidence;
      const meta = row ? `<div class="service-map-verified"><h4>Verified retained source</h4><dl><dt>Method</dt><dd>${escape(row.method)}</dd><dt>Origin</dt><dd>${escape(safeOrigin(row.origin))}</dd><dt>Path</dt><dd>${escape(safePath(row.path))}</dd><dt>Status</dt><dd>${escape(row.status ?? "No response")}</dd><dt>Sequence</dt><dd>${escape(row.sequence ?? "Unavailable")}</dd><dt>Captured</dt><dd>${escape(row.started_at || "Unavailable")}</dd><dt>Record ID</dt><dd>${escape(row.record_id || "Unavailable")}</dd></dl>${row.metadata_complete === false ? `<p class="service-map-error">Source metadata is incomplete: ${escape(array(row.omitted).join(", ") || "bounded metadata")}</p>` : ""}<h4>Source observations</h4><ul class="service-map-references">${array(row.header_links).filter((link) => ["origin_header", "referer_header"].includes(link.relationship)).map((link) => `<li>${link.relationship === "origin_header" ? "Origin" : "Referer"} header: ${escape(safeOrigin(link.source_origin))} (declaration)</li>`).join("")}${row.redirect ? `<li>Redirect instruction: ${escape(safeOrigin(row.redirect.target_origin))}${escape(safePath(row.redirect.target_path))}</li>` : ""}${array(row.technology_clues).map((clue) => `<li>${escape(clue.name)} · ${escape(clue.header_name)} header clue</li>`).join("")}</ul><p>Exact retained source verified for this read.</p></div>` : "";
      return `${endpoint ? `<div class="service-map-endpoint-summary"><span class="service-map-method" title="${escape(endpoint.method)}">${escape(endpoint.method)}</span><strong>${escape(endpoint.path)}</strong><p>${endpoint.request_count} sampled rows · Statuses: ${escape(endpoint.statuses.join(", ") || "No response")}</p></div>` : ""}
        <h4>Source evidence</h4><p class="service-map-muted">Read the exact retained source metadata. Bodies, query strings and private headers are not included.</p>
        ${state.evidenceLoading ? '<p class="service-map-muted" role="status">Verifying source…</p>' : ""}${state.evidenceError ? `<p class="service-map-error" role="status">${escape(state.evidenceError)}</p>` : ""}${meta}${row ? bodyReferencesMarkup() : ""}
        <div class="service-map-evidence-list">${items.slice(0, 30).map((item) => `<button class="service-map-evidence-button ${state.evidenceId === item.id ? "selected" : ""}" type="button" data-service-evidence="${escape(item.id)}" data-service-focus="evidence:${escape(item.id)}"><span>${escape(item.label)}</span><span>View source ›</span></button>`).join("") || '<p class="service-map-muted">No source samples are available in this bounded snapshot.</p>'}</div>${items.length > 30 ? `<p class="service-map-muted">Showing 30 of ${items.length} source samples. Select an API path to narrow the evidence.</p>` : ""}
        ${!endpoint ? relationshipMarkup(origin) : ""}
        ${!endpoint && origin.technology_clues.length ? `<h4>Observed header clues</h4><p class="service-map-muted">${origin.technology_clues.map((clue) => escape(clue.name)).join(", ")} · response headers only</p>` : ""}`;
    }
    function replaceDetail(html) {
      const identity = JSON.stringify([state.sessionId, state.selectedOrigin, state.endpointSelection?.method, state.endpointSelection?.path, state.detailTab, state.evidenceId]);
      const sameSource = detailRender?.identity === identity && detailRender.evidence === state.evidence;
      // Native disclosures and browser HTML serialization change innerHTML.
      // Compare our generated markup locally so unchanged polls leave the
      // inspector DOM, keyboard focus, and nested scroll positions untouched.
      if (sameSource && detailRender.html === html) return;
      const previousBody = els.detail.querySelector?.(".service-map-detail-body");
      const previousList = els.detail.querySelector?.(".service-map-body-reference-list");
      const previousLimits = els.detail.querySelector?.(".service-map-body-reference-limits");
      const bodyScroll = previousBody?.scrollTop;
      const listScroll = previousList?.scrollTop;
      const limitsOpen = previousLimits?.open;
      replace(els.detail, html);
      detailRender = { identity, evidence: state.evidence, html };
      if (sameSource) {
        const body = els.detail.querySelector?.(".service-map-detail-body");
        const list = els.detail.querySelector?.(".service-map-body-reference-list");
        const limits = els.detail.querySelector?.(".service-map-body-reference-limits");
        if (limits && limitsOpen !== undefined) limits.open = limitsOpen;
        if (body && bodyScroll !== undefined) body.scrollTop = bodyScroll;
        if (list && listScroll !== undefined) list.scrollTop = listScroll;
      }
    }
    function renderDetail() {
      const origin = selectedOrigin();
      if (!origin) { replaceDetail('<div class="service-map-empty"><h3>Select an origin</h3><p>Choose a card to inspect its API paths and retained evidence.</p></div>'); return; }
      if (!origin.captured) {
        const bodyReference = state.bodyReferenceOverlay?.scan.references.some((item) => item.target_origin === origin.origin);
        replaceDetail(`<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div></header><div class="service-map-detail-body"><h4>Referenced only in this snapshot</h4><p class="service-map-muted">This origin appears only in loaded declarations${bodyReference ? " or the temporary body URL references" : ""}. No captured rows are shown for this origin. Scope and projection bounds may omit captured history; no service role or request count is established here.</p>${bodyReference ? `<h4>Literal body URL reference</h4>${overlayProvenanceMarkup()}<p class="service-map-muted">This reference does not prove a request, call, or service ownership.</p>` : ""}${relationshipMarkup(origin)}<h4>Captured origins</h4><div class="service-map-evidence-list">${origin.references.map((ref) => `<button class="service-map-evidence-button" type="button" data-service-origin="${escape(ref.origin)}" data-service-focus="reference:${escape(ref.origin)}"><span>${escape(ref.label)} ${escape(ref.origin)}</span><span>Inspect source ›</span></button>`).join("")}</div>${origin.referencesOmitted ? `<p class="service-map-muted">${origin.referencesOmitted} additional source references omitted from this pane.</p>` : ""}</div>`);
        return;
      }
      const pageCount = Math.max(1, Math.ceil(origin.endpoints.length / API_PAGE_SIZE));
      state.detailPage = Math.min(state.detailPage, pageCount - 1);
      const start = state.detailPage * API_PAGE_SIZE;
      replaceDetail(`<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div><span class="detail-chip ${origin.in_scope ? "ok" : "none"}">${origin.in_scope ? "In scope" : "Out of scope"}</span></header>
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
      renderOverlayStatus();
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
      else if (data.serviceBodyReferences) loadBodyReferences();
      else if (data.serviceShowBodyReferences) showBodyReferencesOnMap();
      else if (data.serviceClearBodyReferences) clearBodyReferenceOverlay();
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
    els.allOrigins?.addEventListener("click", () => setMapMode(false));
    els.directRelations?.addEventListener("click", () => setMapMode(true));
    els.focusSelection?.addEventListener("click", focusSelection);
    els.search.addEventListener("input", () => { state.query = els.search.value; state.page = 0; render(); });
    els.scope.addEventListener("change", () => {
      clearOverlay("Temporary body references cleared because the scope filter changed.");
      state.scopeOnly = els.scope.checked;
      state.page = 0;
      state.snapshot = null;
      state.endpointIndex = null;
      cancelEvidence();
      refresh({ force: true });
    });
    render();
    return {
      refresh, reset, selectOrigin, loadEvidence, loadBodyReferences, showBodyReferencesOnMap, clearBodyReferenceOverlay,
      scopeChanged(previous, next) {
        const signature = (runtime) => JSON.stringify([array(runtime?.scope_patterns), array(runtime?.excluded_scope_patterns)]);
        if (signature(previous) === signature(next)) return false;
        return this.invalidate();
      },
      markDirty() {
        clearOverlay("Temporary body references cleared because retained history changed. Inspect and show references again when ready.");
        state.historyVersion += 1;
        state.stale = true;
        cancelEvidence();
        render();
      },
      setVisible(value, view = state.view) {
        if (!value || view !== "service-map") {
          clearOverlay("Temporary body references cleared because the map was hidden.");
          cancelBodyReferences();
        }
        visible = value;
        state.view = view;
        if (state.sessionId !== getSessionId()) reset();
        render();
      },
      setViewChange(callback) { onViewChange = callback; },
      disconnect() {
        clearOverlay("Temporary body references cleared because the session event connection was lost.");
        state.requestId += 1;
        state.historyVersion += 1;
        abort?.abort();
        abort = null;
        state.loading = false;
        state.stale = true;
        cancelEvidence();
        render();
      },
      invalidate() {
        clearOverlay("Temporary body references cleared because history or scope was invalidated.");
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
  root.SniperServiceMap = { createController, normalizeSnapshot, normalizeBodyReferences, BODY_REFERENCE_LIMITS, stableOrder, graphLayout, safeOrigin, safePath, MAX_CARDS, MAX_EDGES, MAX_REFERENCE_CARDS };
})(typeof window === "undefined" ? globalThis : window);
