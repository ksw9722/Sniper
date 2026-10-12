/* Read-only retained metadata map with explicit, bounded single-source body
   references. No capture, replay, remote fetch, or UUID-only detail navigation. */
(function (root) {
  "use strict";
  const MAX_CARDS = 24;
  const MAX_EDGES = 40;
  const MAX_REFERENCE_CARDS = 100;
  const MAX_REFERENCE_SOURCES = 30;
  const API_PAGE_SIZE = 50;
  const GRAPH_GEOMETRY = Object.freeze({ cardWidth: 220, cardHeight: 104, columnPitch: 300, rowPitch: 124, padding: 24, loopInset: 8, lineHeight: 1.5 });
  const ZOOM_LIMITS = Object.freeze({ min: 0.1, max: 2, step: 0.1 });
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
    session_source_changed: "The active session source changed", metadata_unavailable: "Retained response metadata is unavailable",
  };
  const SOURCE_LIFETIME_MESSAGES = Object.freeze({
    invalid_evidence_id: "Source verification unavailable for this evidence token; verify again.",
    session_mismatch: "Source session does not match; verify again.",
    stale_generation: "Retained history changed; verify again.",
    record_missing: "The source row is no longer retained. Refresh the map to select current evidence.",
    record_changed: "The source row changed. Refresh the map to select current evidence.",
    session_source_changed: "Session context changed; verify again.",
  });
  const INACTIVE_SESSION_NOTICE = "This session must be active for supplementary inspection. Switch to it again, then verify the source.";
  const BODY_REFERENCE_PARTIAL_REASONS = new Set(["capture_preview_truncated", "scan_prefix_truncated", "candidate_limit", "candidate_bytes", "origin_limit", "offset_limit"]);
  const BODY_REFERENCE_UNSUPPORTED_REASONS = new Set(["unsupported_metadata", "metadata_limit", "unsupported_content_type", "unsupported_body_encoding", "unsupported_content_encoding"]);
  const BODY_REFERENCE_UNAVAILABLE_REASONS = new Set(Object.keys(BODY_REFERENCE_REASONS).filter((reason) => !BODY_REFERENCE_PARTIAL_REASONS.has(reason) && !BODY_REFERENCE_UNSUPPORTED_REASONS.has(reason)));
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
  const MAP_ERROR_HEADER = "x-sniper-service-map-error";
  const MAP_FAILURES = new Map([
    ["busy", { status: 429, message: "Service map is busy." }],
    ["scope_work_limit", { status: 422, message: "Scope matching exceeded the service map work limit. Simplify scope rules or use a smaller CLI --max-records window." }],
    ["session_changed", { status: 409, message: "The selected session source changed while the service map was being built." }],
    ["scope_changed", { status: 409, message: "Scope rules changed while the service map was being built." }],
    ["history_changed", { status: 409, message: "Retained history changed while the service map was being built." }],
    ["build_failed", { status: 500, message: "The service map could not be built." }],
  ]);
  function mapFailure(response) {
    // Fetch joins duplicate response headers with commas. Exact tokens reject
    // these, unfamiliar markers and status mismatches without reading the body.
    const token = response.headers?.get(MAP_ERROR_HEADER);
    const failure = MAP_FAILURES.get(token);
    return failure?.status === response.status ? failure : null;
  }
  // This closed catalog mirrors the passive header extractor. Display names and
  // categories never come from arbitrary header text or describe deployed roles.
  const TECHNOLOGY_CATALOG_VERSION = 2;
  const TECHNOLOGY_CATALOG_HEADER = "X-Sniper-Technology-Catalog-Version";
  const LEGACY_CATALOG_NOTICE = "Legacy server: catalog v1; additional declarations were not inspected.";
  const TECHNOLOGY_CATALOG_V1 = new Map([
    ["nginx", ["nginx", "web_server", "nginx", ["server"]]],
    ["apache_http_server", ["Apache", "web_server", "Apache", ["server"]]],
    ["caddy", ["Caddy", "web_server", "Caddy", ["server"]]],
    ["cloudflare", ["Cloudflare", "edge_service", "Cloudflare", ["server"]]],
    ["microsoft_iis", ["Microsoft IIS", "web_server", "Microsoft-IIS", ["server"]]],
    ["envoy", ["Envoy", "edge_service", "Envoy", ["server"]]],
    ["php", ["PHP", "language_runtime", "PHP", ["server", "x-powered-by"]]],
    ["jetty", ["Jetty", "application_server", "Jetty", ["server"]]],
    ["apache_coyote", ["Apache-Coyote", "connector", "Apache-Coyote", ["server"]]],
    ["apache_tomcat", ["Apache Tomcat", "application_server", "Apache Tomcat", ["server"]]],
    ["express", ["Express", "application_framework", "Express", ["x-powered-by"]]],
    ["asp_net", ["ASP.NET", "application_framework", "ASP.NET", ["x-powered-by"]]],
    ["wordpress", ["WordPress", "cms", "WordPress", ["x-generator"]]],
    ["drupal", ["Drupal", "cms", "Drupal", ["x-generator"]]],
  ]);
  const TECHNOLOGY_CATALOG_V2 = new Map([
    ...TECHNOLOGY_CATALOG_V1,
    ["gunicorn", ["Gunicorn", "application_server", "gunicorn", ["server"]]],
    ["uvicorn", ["Uvicorn", "application_server", "uvicorn", ["server"]]],
    ["waitress", ["Waitress", "application_server", "waitress", ["server"]]],
    ["openresty", ["OpenResty", "web_server", "openresty", ["server"]]],
    ["litespeed", ["LiteSpeed", "web_server", "LiteSpeed", ["server"]]],
    ["next_js", ["Next.js", "application_framework", "Next.js", ["x-powered-by"]]],
  ]);
  function technologyCatalogVersion(payload, requestedVersion = TECHNOLOGY_CATALOG_VERSION) {
    if (![1, 2].includes(requestedVersion)) throw new Error("Invalid requested technology catalog version");
    // Old servers ignore the version header. An absent marker permits only the
    // closed v1 catalog on this response, never a retry or cached negotiation.
    if (!Object.prototype.hasOwnProperty.call(payload, "technology_catalog_version")) return 1;
    if (payload.technology_catalog_version !== requestedVersion) throw new Error("Invalid retained technology catalog version");
    return requestedVersion;
  }
  function technologyCatalog(version) {
    if (version === 1) return TECHNOLOGY_CATALOG_V1;
    if (version === 2) return TECHNOLOGY_CATALOG_V2;
    throw new Error("Invalid retained technology catalog version");
  }
  const TECHNOLOGY_FIELDS = ["product_id", "name", "catalog_category", "reported_version", "version_status", "source", "strength", "header_name"];
  const TECHNOLOGY_CATEGORIES = { web_server: "Web server", edge_service: "Edge service", language_runtime: "Language runtime", application_server: "Application server", connector: "Connector", application_framework: "Application framework", cms: "CMS" };
  const technologyInteger = (value, max) => Number.isSafeInteger(value) && value >= 0 && value <= max;
  const exactFields = (value, fields) => value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).length === fields.length && fields.every((field) => Object.prototype.hasOwnProperty.call(value, field));
  const invalidTechnology = () => { throw new Error("Invalid retained technology observations"); };
  function observationTime(value) {
    const parts = typeof value === "string" && value.length <= 40
      && /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})$/.exec(value);
    if (!parts) return null;
    const [, year, month, day, hour, minute, second, fraction = ""] = parts;
    const leap = Number(year) % 4 === 0 && (Number(year) % 100 !== 0 || Number(year) % 400 === 0);
    const monthDays = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if (Number(month) < 1 || Number(month) > 12 || Number(day) < 1 || Number(day) > monthDays[Number(month) - 1]
      || Number(hour) > 23 || Number(minute) > 59 || Number(second) > 59) return null;
    const milliseconds = Date.parse(value.replace(/\.\d+(?=Z|[+-])/, ""));
    // Retained timestamps can carry nanoseconds. Keep that precision when
    // proving an observation stays inside its origin's retained time window.
    return Number.isFinite(milliseconds) ? BigInt(milliseconds) * 1000000n + BigInt(fraction.padEnd(9, "0")) : null;
  }
  function technologyBase(item, catalogVersion) {
    const catalog = technologyCatalog(catalogVersion).get(item?.product_id);
    if (!catalog || item.name !== catalog[0] || item.catalog_category !== catalog[1] || !catalog[3].includes(item.header_name)
      || item.source !== "response_header" || item.strength !== "literal_declaration"
      || !["reported", "not_reported", "unsupported_format", "non_product_version"].includes(item.version_status)
      || item.version_status === "non_product_version" && item.product_id !== "apache_coyote"
      || item.version_status === "reported" && (item.product_id === "apache_coyote" || typeof item.reported_version !== "string"
        || item.reported_version.length > 32 || !/^[0-9]+(?:\.[0-9]+)*$/.test(item.reported_version))
      || item.version_status !== "reported" && item.reported_version !== null) invalidTechnology();
    return Object.fromEntries(TECHNOLOGY_FIELDS.map((field) => [field, item[field]]));
  }
  function technologyList(value, max) {
    if (value === undefined) return [];
    if (!Array.isArray(value) || value.length > max) invalidTechnology();
    return value;
  }
  function normalizeRowTechnologies(value, catalogVersion = 1) {
    const positions = new Set(), headers = new Map(), headerSpans = new Map();
    return technologyList(value, 8).map((item) => {
      if (!exactFields(item, [...TECHNOLOGY_FIELDS, "header_index", "product_span", "version_span", "offset_basis", "rule_id"])) invalidTechnology();
      const base = technologyBase(item, catalogVersion), catalog = technologyCatalog(catalogVersion).get(item.product_id);
      const span = (value) => exactFields(value, ["start", "end"]) && technologyInteger(value.start, 4096)
        && technologyInteger(value.end, 4096) && value.start < value.end;
      const rule = `${item.header_name.replaceAll("-", "_")}.${item.product_id}.v1`;
      if (!technologyInteger(item.header_index, 127) || item.offset_basis !== "retained_header_value_utf8"
        || !span(item.product_span) || item.product_span.end - item.product_span.start !== catalog[2].length
        || item.rule_id !== rule && !(item.product_id === "jetty" && item.rule_id === "server.jetty_parenthesized.v1")
        || item.rule_id === "server.jetty_parenthesized.v1" && !["reported", "unsupported_format"].includes(item.version_status)
        || item.version_status === "reported" && (!span(item.version_span) || item.version_span.start <= item.product_span.end
          || item.version_span.end - item.version_span.start !== item.reported_version.length
          || item.header_name !== "x-generator" && item.version_span.start !== item.product_span.end + 1)
        || item.version_status !== "reported" && item.version_span !== null) invalidTechnology();
      const position = JSON.stringify([item.header_index, item.product_span.start, item.product_span.end]);
      if (positions.has(position) || headers.has(item.header_index) && headers.get(item.header_index) !== item.header_name) invalidTechnology();
      const spans = [item.product_span, ...(item.version_span ? [item.version_span] : [])];
      const previousSpans = headerSpans.get(item.header_index) || [];
      if (spans.some((span) => previousSpans.some((previous) => span.start < previous.end && previous.start < span.end))) invalidTechnology();
      positions.add(position);
      headers.set(item.header_index, item.header_name);
      headerSpans.set(item.header_index, [...previousSpans, ...spans]);
      return { ...base, header_index: item.header_index, product_span: { ...item.product_span },
        version_span: item.version_span ? { ...item.version_span } : null, offset_basis: item.offset_basis, rule_id: item.rule_id };
    });
  }
  function validTechnologyEvidence(value, sessionId, generation) {
    if (typeof value !== "string" || value.length > 200) return false;
    const fields = value.split("."), uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
    return fields.length === 6 && fields[0] === "v1" && fields[1] === sessionId && fields[2] === generation
      && [fields[1], fields[2], fields[4], fields[5]].every((field) => uuid.test(field))
      && /^(?:0|[1-9][0-9]{0,19})$/.test(fields[3]) && BigInt(fields[3]) <= 18446744073709551615n;
  }
  function normalizeOriginTechnologies(origin, sessionId, generation, catalogVersion) {
    const keys = new Set();
    return technologyList(origin?.technology_observations, 32).map((item) => {
      if (!exactFields(item, [...TECHNOLOGY_FIELDS, "row_count", "first_seen", "last_seen", "evidence"])) invalidTechnology();
      const base = technologyBase(item, catalogVersion), first = observationTime(item.first_seen), last = observationTime(item.last_seen);
      const originFirst = observationTime(origin.first_seen), originLast = observationTime(origin.last_seen);
      if (!technologyInteger(origin.request_count, 50000) || !technologyInteger(item.row_count, origin.request_count) || !item.row_count
        || first === null || last === null || first > last
        || originFirst === null || originLast === null || first < originFirst || last > originLast
        || !Array.isArray(item.evidence) || !item.evidence.length || item.evidence.length > Math.min(3, item.row_count)
        || new Set(item.evidence).size !== item.evidence.length
        || item.evidence.some((id) => !validTechnologyEvidence(id, sessionId, generation))
        || new Set(item.evidence.map((id) => id.split(".")[3])).size !== item.evidence.length) invalidTechnology();
      const key = JSON.stringify([item.product_id, item.header_name, item.reported_version, item.version_status]);
      if (keys.has(key)) invalidTechnology();
      keys.add(key);
      return { ...base, row_count: item.row_count, first_seen: item.first_seen, last_seen: item.last_seen, evidence: [...item.evidence] };
    });
  }
  function technologyVersion(item) {
    return item.version_status === "reported" ? `Reported version: ${item.reported_version}`
      : item.version_status === "unsupported_format" ? "Reported version: unknown (unsupported format)"
        : item.version_status === "non_product_version" ? "Reported version: unknown (non-product version)"
          : "Reported version: unknown (not reported)";
  }
  const PRODUCT_VERSION_OPTIONS = Object.freeze([
    ["multiple", "Multiple reported versions"],
    ["status:not_reported", "Unknown: not reported"],
    ["status:unsupported_format", "Unknown: unsupported format"],
    ["status:non_product_version", "Unknown: non-product version"],
  ]);
  function reportedProducts(origin) {
    // Only validated default-map aggregates participate. Missing metadata and
    // exact-source inspections must never turn into an origin-level claim.
    if (!origin?.technology_observations_present) return [];
    const products = new Map();
    for (const item of origin.technology_observations) {
      if (!products.has(item.product_id)) products.set(item.product_id, { id: item.product_id, name: item.name, versions: new Set(), statuses: new Set(), observations: [] });
      const product = products.get(item.product_id);
      if (item.version_status === "reported") product.versions.add(item.reported_version);
      else product.statuses.add(item.version_status);
      product.observations.push(item);
    }
    return [...products.values()].sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  }
  function productMatches(origin, productId = "", version = "") {
    if (!productId && !version) return true;
    return reportedProducts(origin).some((product) => (!productId || product.id === productId)
      && (!version || version === "multiple" && product.versions.size > 1
        || version.startsWith("reported:") && product.versions.has(version.slice(9))
        || version.startsWith("status:") && product.statuses.has(version.slice(7))));
  }
  function productSummaryMarkup(origin) {
    const products = reportedProducts(origin);
    if (!products.length) {
      const label = origin.peerInspectionOnly ? "Default product metadata unavailable"
        : origin.technology_observations_present ? "No loaded product declarations" : "Product/version metadata unavailable";
      return `<span class="service-map-clues service-map-products" title="${escape(label)}"><span>${escape(label)}</span></span>`;
    }
    const full = (product) => "Header-reported declarations; versions unverified.\n" + product.observations.map((item) => `${item.name} · ${technologyVersion(item)} · ${item.header_name} response header · ${item.row_count} retained row(s) · First: ${item.first_seen} · Last: ${item.last_seen}`).join("\n");
    const label = (product) => `${product.name}${product.versions.size > 1 ? ` · ${product.versions.size} versions` : product.versions.size ? ` ${[...product.versions][0]}` : " · version unknown"}${product.versions.size && product.statuses.size ? " + unknown" : ""}`;
    return `<span class="service-map-clues service-map-products" title="${escape(products.map(full).join("\n"))}">${products.slice(0, 2).map((product) => `<span title="${escape(full(product))}">${escape(label(product))}</span>`).join("")}${products.length > 2 ? `<span class="service-map-products-more" aria-label="${products.length - 2} additional products" title="${escape(products.slice(2).map(full).join("\n"))}">+${products.length - 2}</span>` : ""}</span>`;
  }
  function normalizeEvidenceObservations(row, catalogVersion) {
    const invalid = () => { throw new Error("Invalid retained source observations"); };
    const text = (value, limit) => typeof value === "string" && value.length > 0 && value.length <= limit;
    const origin = (value) => text(value, 1024) && !!safeOrigin(value);
    const list = (value, limit) => {
      if (value === undefined) return [];
      if (!Array.isArray(value) || value.length > limit) invalid();
      return value;
    };
    // Validate the small per-row projection before publishing it. A malformed
    // nested observation must not leave a row that crashes every inspector render.
    const technology_clues = list(row.technology_clues, 8).map((clue) => {
      if (!clue || !text(clue.name, 128) || !text(clue.header_name, 128)) invalid();
      return { name: clue.name, header_name: clue.header_name };
    });
    const header_links = list(row.header_links, 2).map((link) => {
      if (!link || !origin(link.source_origin) || !["origin_header", "referer_header"].includes(link.relationship)) invalid();
      return { source_origin: safeOrigin(link.source_origin), relationship: link.relationship };
    });
    const omitted = list(row.omitted, 32);
    if (omitted.some((reason) => !text(reason, 80))) invalid();
    let redirect = null;
    if (row.redirect !== undefined && row.redirect !== null) {
      if (!origin(row.redirect.target_origin) || !text(row.redirect.target_path, 2048)) invalid();
      redirect = { target_origin: safeOrigin(row.redirect.target_origin), target_path: safePath(row.redirect.target_path) };
    }
    const technology_observations = normalizeRowTechnologies(row.technology_observations, catalogVersion);
    if (row.technology_observations !== undefined && (row.metadata_complete !== (omitted.length === 0)
      || omitted.includes("technology_observations_limit") && technology_observations.length !== 8)
      || omitted.includes("technology_token_scan_limit") && row.metadata_complete !== false) invalidTechnology();
    return { ...row, technology_clues, technology_observations, technology_observations_present: Object.prototype.hasOwnProperty.call(row, "technology_observations"), header_links, redirect, omitted: [...omitted] };
  }
  function normalizeSocketPeer(payload) {
    // This extension is independently optional on older servers. Only typed
    // socket observations enter the inspector; arbitrary record fields never do.
    if (payload === undefined) return { status: "unsupported" };
    if (!payload || !["observed", "not_recorded", "unavailable"].includes(payload.status)) return { status: "invalid" };
    if (payload.status === "not_recorded") return payload.observation === null ? { status: "not_recorded" } : { status: "invalid" };
    const observation = payload.observation;
    if (!observation || observation.source !== "http_response_remote_addr"
      || !["direct", "http_proxy", "socks_proxy", "unknown"].includes(observation.route)
      || typeof observation.observed_at !== "string" || observation.observed_at.length > 40
      || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(observation.observed_at)
      || !Number.isFinite(Date.parse(observation.observed_at.replace(/\.(\d{1,9})(?=Z|[+-])/, (_, digits) => `.${digits.slice(0, 3).padEnd(3, "0")}`)))) return { status: "invalid" };
    const normalized = { status: payload.status, source: observation.source, observedAt: observation.observed_at, route: observation.route };
    if (payload.status === "unavailable") return observation.address === null && observation.reason === "remote_addr_unavailable"
      ? normalized : { status: "invalid" };
    const address = observation.address;
    if (observation.reason !== null || !address || typeof address.ip !== "string" || address.ip.length > 45
      || !Number.isInteger(address.port) || address.port < 0 || address.port > 65535) return { status: "invalid" };
    const ip = address.ip;
    let validIp = /^(0|[1-9]\d{0,2})(?:\.(0|[1-9]\d{0,2})){3}$/.test(ip) && ip.split(".").every((part) => Number(part) <= 255);
    if (!validIp && ip.includes(":") && /^[0-9a-fA-F:.]+$/.test(ip)) {
      try { validIp = new URL(`http://[${ip}]/`).hostname.startsWith("["); } catch (_) { /* Invalid IPv6 literal. */ }
    }
    return validIp ? { ...normalized, ip, port: address.port } : { status: "invalid" };
  }

  const PEER_MATCH_LIMITS = Object.freeze({ max_records: 10000, origin_groups: 100, samples_per_origin: 3, projection_bytes: 262144, host_bytes: 512, scheme_bytes: 8, scope_pattern_entries: 500, scope_pattern_bytes: 512, scope_work_units: 8388608 });
  const PEER_MATCH_REASONS = Object.freeze({
    invalid_evidence_id: "The exact evidence token is invalid", session_mismatch: "The source session changed", stale_generation: "Retained history changed",
    record_missing: "The source row is no longer retained", record_changed: "The source row changed", inactive_session: "Only the active session can be inspected",
    session_source_changed: "The active source changed", unsupported_source_context: "This source is not an ordinary captured HTTP row",
    observation_not_recorded: "No peer was recorded for this source", observation_unavailable: "The recorded source address is unavailable",
    non_direct_observation: "Proxy and unknown routes are diagnostics only", invalid_observation: "The recorded source address is not eligible for matching",
    metadata_unavailable: "The captured source origin could not be verified", source_out_of_scope: "The source is excluded by In scope only",
    scope_work_limit: "Scope matching reached the inspection work limit. Narrow the scope rules or use a smaller latest-row window in the CLI.",
    peer_inspection_failed: "The retained peer inspection is unavailable", max_records: "Newest-row window limit reached",
    source_outside_window: "The source is outside the considered candidate window", invalid_metadata: "Some matching rows have unusable origin metadata",
    origin_group_limit: "Origin-group result limit reached", projection_byte_limit: "Result byte limit reached", sample_limit: "Exact source samples were omitted",
  });
  const PEER_PARTIAL_REASONS = new Set(["max_records", "source_outside_window", "invalid_metadata", "origin_group_limit", "projection_byte_limit", "sample_limit"]);
  const PEER_COUNT_FIELDS = ["non_http_rows", "not_recorded_rows", "unavailable_observation_rows", "non_direct_rows", "invalid_observation_rows", "different_peer_rows", "invalid_metadata_rows", "scope_excluded_rows", "matching_rows", "returned_matching_rows", "omitted_matching_rows", "sample_rows", "omitted_sample_rows"];
  const PEER_PRIVACY = "Matches retained address values. Shared addresses and imported history do not establish the same physical endpoint or origin ownership. Addresses may reveal internal infrastructure.";
  const PEER_NODE_ID = "recorded-socket-peer";
  function peerAddressKey(ip) {
    if (typeof ip !== "string" || ip.length > 45) return null;
    if (/^(0|[1-9]\d{0,2})(?:\.(0|[1-9]\d{0,2})){3}$/.test(ip) && ip.split(".").every((part) => Number(part) <= 255)) return { family: "ipv4", ip, v4: ip.split(".").map(Number) };
    if (!ip.includes(":") || !/^[0-9a-fA-F:.]+$/.test(ip)) return null;
    try {
      const canonical = new URL(`http://[${ip}]/`).hostname.slice(1, -1);
      const parts = canonical.split("::"), left = parts[0] ? parts[0].split(":") : [], right = parts[1] ? parts[1].split(":") : [];
      const words = [...left, ...Array(8 - left.length - right.length).fill("0"), ...right].map((part) => parseInt(part, 16));
      if (words.length !== 8 || words.some((word) => !Number.isInteger(word))) return null;
      const mapped = words.slice(0, 5).every((word) => word === 0) && words[5] === 65535;
      const v4 = mapped ? [words[6] >> 8, words[6] & 255, words[7] >> 8, words[7] & 255] : null;
      return { family: "ipv6", ip: canonical, wire: mapped ? `::ffff:${v4.join(".")}` : canonical, words, v4 };
    } catch (_) { return null; }
  }
  function usableSocketPeer(peer) {
    if (peer?.status !== "observed" || peer.route !== "direct" || peer.source !== "http_response_remote_addr"
      || !Number.isInteger(peer.port) || peer.port < 1 || peer.port > 65535 || observationTime(peer.observedAt) === null) return false;
    const key = peerAddressKey(peer.ip);
    if (!key) return false;
    if (key.v4) return key.v4[0] !== 0 && key.v4[0] < 224 && !(key.v4[0] === 169 && key.v4[1] === 254);
    return key.words.some(Boolean) && (key.words[0] & 0xff00) !== 0xff00 && (key.words[0] & 0xffc0) !== 0xfe80;
  }
  function peerEquals(peer, recorded) {
    const left = peerAddressKey(peer?.address?.ip), right = peerAddressKey(recorded?.ip);
    return usableSocketPeer(recorded) && left && right && left.family === peer.address_family && left.family === right.family
      && left.ip === right.ip && peer.address.port === recorded.port && peer.route === recorded.route && peer.source === recorded.source;
  }
  const validPeerEvidence = (value, sessionId, generation) => validTechnologyEvidence(value, sessionId, generation) && value === value.toLowerCase();
  function normalizePeerMatches(payload, sessionId, evidenceId, row, recordedPeer, scopeOnly, generation) {
    const invalid = () => { throw new Error("Invalid recorded socket peer matches"); };
    const uuid = (value) => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
    const integer = (value, max = Number.MAX_SAFE_INTEGER) => Number.isSafeInteger(value) && value >= 0 && value <= max;
    const sequence = (value) => typeof value === "string" && /^(?:0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;
    const origin = (value) => typeof value === "string" && value.length <= 544 && value === safeOrigin(value);
    if (!exactFields(payload, ["schema_version", "session_id", "evidence_id", "available", "status", "complete", "reasons", "in_scope_only", "coverage", "peer", "source", "snapshot", "counts", "origins", "limits"])
      || payload.schema_version !== 1 || payload.session_id !== sessionId || payload.evidence_id !== evidenceId
      || !validPeerEvidence(evidenceId, sessionId, generation) || payload.in_scope_only !== scopeOnly
      || payload.coverage !== "retained_direct_socket_peer_equality" || typeof payload.available !== "boolean" || typeof payload.complete !== "boolean"
      || !["matched", "no_matches", "unavailable"].includes(payload.status)
      || !Array.isArray(payload.reasons) || new Set(payload.reasons).size !== payload.reasons.length
      || payload.reasons.length > 6 || payload.reasons.some((reason, index) => typeof reason !== "string" || !Object.hasOwn(PEER_MATCH_REASONS, reason) || index && payload.reasons[index - 1] >= reason)
      || !exactFields(payload.counts, PEER_COUNT_FIELDS) || PEER_COUNT_FIELDS.some((field) => !integer(payload.counts[field]))
      || !exactFields(payload.limits, Object.keys(PEER_MATCH_LIMITS)) || Object.entries(PEER_MATCH_LIMITS).some(([key, value]) => payload.limits[key] !== value)
      || !Array.isArray(payload.origins) || payload.origins.length > PEER_MATCH_LIMITS.origin_groups) invalid();
    if (!payload.available) {
      if (payload.status !== "unavailable" || payload.complete || payload.reasons.length !== 1 || payload.reasons.some((reason) => PEER_PARTIAL_REASONS.has(reason))
        || payload.reasons[0] === "source_out_of_scope" && !scopeOnly
        || payload.peer !== null || payload.source !== null || payload.snapshot !== null || payload.origins.length || PEER_COUNT_FIELDS.some((field) => payload.counts[field] !== 0)) invalid();
      return { ...payload, reasons: [...payload.reasons], origins: [], counts: { ...payload.counts }, limits: { ...payload.limits } };
    }
    const source = payload.source, snapshot = payload.snapshot, peer = payload.peer, counts = payload.counts;
    if (payload.status === "unavailable" || payload.complete !== (payload.reasons.length === 0) || payload.reasons.some((reason) => !PEER_PARTIAL_REASONS.has(reason))
      || !exactFields(peer, ["source", "route", "address_family", "address"]) || !exactFields(peer.address, ["ip", "port"])
      || peer.source !== "http_response_remote_addr" || peer.route !== "direct" || !peerEquals(peer, recordedPeer)
      || peer.address.ip !== (peerAddressKey(peer.address.ip).wire || peerAddressKey(peer.address.ip).ip)
      || !exactFields(source, ["evidence_id", "sequence", "origin", "started_at", "observed_at", "in_scope", "in_window"])
      || source.evidence_id !== evidenceId || source.sequence !== row.sequence || source.sequence !== evidenceId.split(".")[3]
      || source.origin !== row.origin || !origin(source.origin) || source.started_at !== row.started_at || observationTime(source.started_at) === null
      || source.observed_at !== recordedPeer.observedAt || typeof source.in_scope !== "boolean" || typeof source.in_window !== "boolean" || scopeOnly && !source.in_scope
      || !exactFields(snapshot, ["id", "generation", "captured_at", "window_order", "retained_rows", "considered_rows", "oldest_considered_sequence", "latest_considered_sequence"])
      || !uuid(snapshot.id) || snapshot.generation !== generation || !uuid(snapshot.generation) || observationTime(snapshot.captured_at) === null
      || snapshot.window_order !== "latest_retained_sequence" || !integer(snapshot.retained_rows) || !integer(snapshot.considered_rows, PEER_MATCH_LIMITS.max_records)
      || snapshot.considered_rows !== Math.min(snapshot.retained_rows, PEER_MATCH_LIMITS.max_records)
      || !snapshot.retained_rows || !snapshot.considered_rows
      || !sequence(snapshot.oldest_considered_sequence) || !sequence(snapshot.latest_considered_sequence)
      || BigInt(snapshot.oldest_considered_sequence) > BigInt(snapshot.latest_considered_sequence)
      || snapshot.considered_rows === 1 && snapshot.oldest_considered_sequence !== snapshot.latest_considered_sequence
      || BigInt(snapshot.considered_rows) > BigInt(snapshot.latest_considered_sequence) - BigInt(snapshot.oldest_considered_sequence) + 1n
      || BigInt(source.sequence) > BigInt(snapshot.latest_considered_sequence)
      || snapshot.considered_rows === snapshot.retained_rows && !source.in_window
      || source.in_window !== (BigInt(source.sequence) >= BigInt(snapshot.oldest_considered_sequence) && BigInt(source.sequence) <= BigInt(snapshot.latest_considered_sequence))
      || payload.reasons.includes("source_outside_window") === source.in_window
      || payload.reasons.includes("max_records") !== (snapshot.retained_rows > snapshot.considered_rows)
      || payload.reasons.includes("invalid_metadata") !== (counts.invalid_metadata_rows > 0)
      || PEER_COUNT_FIELDS.some((key) => counts[key] > snapshot.considered_rows)
      || counts.returned_matching_rows + counts.omitted_matching_rows !== counts.matching_rows
      || (payload.status === "matched") !== (counts.matching_rows > 0)
      || PEER_COUNT_FIELDS.slice(0, 9).reduce((sum, key) => sum + counts[key], 0) !== snapshot.considered_rows - (source.in_window ? 1 : 0)
      || scopeOnly === false && counts.scope_excluded_rows !== 0) invalid();
    const seenOrigins = new Set(), seenTokens = new Set([evidenceId]), seenSequences = new Set([source.sequence]);
    const origins = payload.origins.map((group, groupIndex) => {
      const first = observationTime(group?.first_observed_at), last = observationTime(group?.last_observed_at);
      if (!exactFields(group, ["origin", "in_scope", "matching_rows", "first_observed_at", "last_observed_at", "samples", "omitted_samples"])
        || !origin(group.origin) || seenOrigins.has(group.origin) || groupIndex && payload.origins[groupIndex - 1].origin >= group.origin || typeof group.in_scope !== "boolean" || scopeOnly && !group.in_scope
        || !integer(group.matching_rows, counts.matching_rows) || !group.matching_rows || first === null || last === null || first > last
        || !Array.isArray(group.samples) || group.samples.length > Math.min(3, group.matching_rows)
        || group.omitted_samples !== group.matching_rows - group.samples.length) invalid();
      seenOrigins.add(group.origin);
      const samples = group.samples.map((sample, sampleIndex) => {
        const at = observationTime(sample?.observed_at);
        if (!exactFields(sample, ["evidence_id", "sequence", "started_at", "observed_at"])
          || !validPeerEvidence(sample.evidence_id, sessionId, generation) || sample.sequence !== sample.evidence_id.split(".")[3]
          || seenTokens.has(sample.evidence_id) || seenSequences.has(sample.sequence)
          || sampleIndex && BigInt(group.samples[sampleIndex - 1].sequence) <= BigInt(sample.sequence) || observationTime(sample.started_at) === null || at === null || at < first || at > last
          || BigInt(sample.sequence) < BigInt(snapshot.oldest_considered_sequence) || BigInt(sample.sequence) > BigInt(snapshot.latest_considered_sequence)) invalid();
        seenTokens.add(sample.evidence_id); seenSequences.add(sample.sequence);
        return Object.freeze({ ...sample });
      });
      const knownTimes = new Set(samples.map((sample) => observationTime(sample.observed_at).toString()));
      const missingTimes = new Set([first.toString(), last.toString()].filter((value) => !knownTimes.has(value))).size;
      if (missingTimes > group.omitted_samples) invalid();
      if (!group.omitted_samples && (samples.reduce((min, item) => observationTime(item.observed_at) < min ? observationTime(item.observed_at) : min, last) !== first
        || samples.reduce((max, item) => observationTime(item.observed_at) > max ? observationTime(item.observed_at) : max, first) !== last)) invalid();
      return Object.freeze({ ...group, samples: Object.freeze(samples) });
    });
    const knownWindowRows = seenSequences.size - (source.in_window ? 0 : 1);
    const unknownWindowRows = snapshot.considered_rows - knownWindowRows;
    const missingEndpoints = new Set([snapshot.oldest_considered_sequence, snapshot.latest_considered_sequence].filter((value) => !seenSequences.has(value))).size;
    if (missingEndpoints > unknownWindowRows) invalid();
    const admittedBytes = 8192 + origins.reduce((sum, group) => sum + group.origin.length * 12 + 512, 0) + counts.sample_rows * 1024;
    const remainingBytes = PEER_MATCH_LIMITS.projection_bytes - admittedBytes;
    const byteRejectedSample = origins.some((group) => group.samples.length < 3 && group.omitted_samples > 0);
    if (remainingBytes < 0 || byteRejectedSample && remainingBytes >= 1024 || payload.reasons.includes("origin_group_limit") && (origins.length !== PEER_MATCH_LIMITS.origin_groups || !counts.omitted_matching_rows)
      || payload.reasons.includes("sample_limit") !== origins.some((group) => group.samples.length === 3 && group.omitted_samples > 0)
      || byteRejectedSample && !payload.reasons.includes("projection_byte_limit")
      || payload.reasons.includes("projection_byte_limit") && !(counts.omitted_matching_rows > 0 && remainingBytes < 7040 || byteRejectedSample && remainingBytes < 1024)
      || JSON.stringify(payload).length > PEER_MATCH_LIMITS.projection_bytes
      || origins.reduce((sum, group) => sum + group.matching_rows, 0) !== counts.returned_matching_rows
      || origins.reduce((sum, group) => sum + group.samples.length, 0) !== counts.sample_rows
      || origins.reduce((sum, group) => sum + group.omitted_samples, 0) !== counts.omitted_sample_rows
      || counts.omitted_matching_rows > 0 && !payload.reasons.some((reason) => ["origin_group_limit", "projection_byte_limit"].includes(reason))
      || counts.omitted_sample_rows > 0 && !payload.reasons.some((reason) => ["sample_limit", "projection_byte_limit"].includes(reason))) invalid();
    return Object.freeze({ ...payload, source: Object.freeze({ ...source }), peer: Object.freeze({ ...peer, address: Object.freeze({ ...peer.address }) }),
      snapshot: Object.freeze({ ...snapshot }), reasons: Object.freeze([...payload.reasons]), counts: Object.freeze({ ...counts }), origins: Object.freeze(origins), limits: Object.freeze({ ...payload.limits }) });
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
      || !Array.isArray(payload.reasons)
      || (scanned ? payload.reasons.length > BODY_REFERENCE_PARTIAL_REASONS.size
        || payload.reasons.some((reason, index, reasons) => !BODY_REFERENCE_PARTIAL_REASONS.has(reason) || index > 0 && reasons[index - 1] >= reason)
        : payload.reasons.length !== 1 || !(payload.status === "unsupported" ? BODY_REFERENCE_UNSUPPORTED_REASONS : BODY_REFERENCE_UNAVAILABLE_REASONS).has(payload.reasons[0]))
      || !Object.entries(BODY_REFERENCE_LIMITS).every(([key, value]) => payload.limits?.[key] === value)
      || !integer(payload.scanned_bytes, BODY_REFERENCE_LIMITS.scan_bytes)
      || !integer(payload.candidates_scanned, BODY_REFERENCE_LIMITS.candidates)
      || !integer(payload.omitted_reference_occurrences, payload.candidates_scanned)
      || typeof payload.capture_preview_truncated !== "boolean" || typeof payload.scan_prefix_truncated !== "boolean") invalid();
    if (scanned && (!canonical(payload.source_origin) || payload.source_origin !== row.origin)
      || scanned && (typeof payload.source_sequence !== "string" || payload.source_sequence !== row.sequence)
      || scanned && (typeof payload.source_started_at !== "string" || payload.source_started_at !== row.started_at)
      || scanned && (!integer(payload.retained_preview_bytes) || payload.scanned_bytes > payload.retained_preview_bytes)
      || !scanned && (payload.references.length || payload.scanned_bytes || payload.candidates_scanned || payload.omitted_reference_occurrences
        || payload.capture_preview_truncated || payload.scan_prefix_truncated
        || [payload.source_origin, payload.source_sequence, payload.source_started_at, payload.retained_preview_bytes].some((value) => value != null))
      || payload.status === "scanned" && (payload.reasons.length || payload.capture_preview_truncated || payload.scan_prefix_truncated || payload.omitted_reference_occurrences)
      || payload.status === "partial" && !payload.reasons.length) invalid();
    const seen = new Set();
    let occurrences = payload.omitted_reference_occurrences;
    const references = payload.references.map((item) => {
      if (!canonical(item?.target_origin) || seen.has(item.target_origin)
        || !integer(item.occurrences, payload.candidates_scanned) || !item.occurrences
        || !Array.isArray(item.preview_byte_offsets) || item.preview_byte_offsets.length !== Math.min(item.occurrences, BODY_REFERENCE_LIMITS.offsets_per_origin)
        || !integer(item.omitted_offsets, item.occurrences) || item.occurrences !== item.preview_byte_offsets.length + item.omitted_offsets
        || payload.status === "scanned" && item.omitted_offsets
        || item.preview_byte_offsets.some((offset, index, offsets) => !integer(offset) || offset >= payload.scanned_bytes || index > 0 && offset <= offsets[index - 1])) invalid();
      seen.add(item.target_origin);
      occurrences += item.occurrences;
      return { target_origin: item.target_origin, occurrences: item.occurrences, preview_byte_offsets: [...item.preview_byte_offsets], omitted_offsets: item.omitted_offsets };
    });
    if (occurrences > payload.candidates_scanned) invalid();
    const has = (reason) => payload.reasons.includes(reason);
    if (scanned && (has("capture_preview_truncated") !== payload.capture_preview_truncated
      || has("scan_prefix_truncated") !== payload.scan_prefix_truncated
      || payload.scan_prefix_truncated !== (payload.scanned_bytes < payload.retained_preview_bytes)
      || (payload.retained_preview_bytes <= BODY_REFERENCE_LIMITS.scan_bytes ? payload.scanned_bytes !== payload.retained_preview_bytes
        : payload.scanned_bytes < BODY_REFERENCE_LIMITS.scan_bytes - 3)
      || has("candidate_limit") && payload.candidates_scanned !== BODY_REFERENCE_LIMITS.candidates
      || has("candidate_bytes") && !payload.candidates_scanned
      || has("origin_limit") !== (payload.omitted_reference_occurrences > 0)
      || has("origin_limit") && references.length !== BODY_REFERENCE_LIMITS.origins
      || has("offset_limit") !== references.some((item) => item.omitted_offsets > 0))) invalid();
    // Keep a bounded allowlist. Unexpected payload fields, including body content,
    // never become inspector state or markup.
    return { status: payload.status, complete: payload.complete, reasons: [...payload.reasons], references,
      retained_preview_bytes: scanned ? payload.retained_preview_bytes : null, scanned_bytes: payload.scanned_bytes,
      candidates_scanned: payload.candidates_scanned, omitted_reference_occurrences: payload.omitted_reference_occurrences,
      capture_preview_truncated: payload.capture_preview_truncated, scan_prefix_truncated: payload.scan_prefix_truncated };
  }
  const LINK_DECLARATION_LIMITS = Object.freeze({ headers: 128, header_bytes: 32768, link_field_bytes: 4096, target_bytes: 4096, candidates: 128, parameters_per_member: 32, relation_tokens_per_member: 32, observations: 32, target_origin_bytes: 268 });
  const LINK_RELATIONS = Object.freeze({
    "service-doc": { type: "link_service_doc", label: "Link service-doc declaration", rule: "response_link.service_doc.v1" },
    "service-desc": { type: "link_service_desc", label: "Link service-desc declaration", rule: "response_link.service_desc.v1" },
    describedby: { type: "link_describedby", label: "Link describedby declaration", rule: "response_link.describedby.v1" },
    preconnect: { type: "link_preconnect", label: "Link preconnect hint", rule: "response_link.preconnect.v1" },
  });
  const LINK_ACQUISITION_REASONS = new Set(["invalid_evidence_id", "session_mismatch", "stale_generation", "record_missing", "record_changed", "response_unavailable", "unsupported_source_context", "inactive_session", "session_source_changed", "metadata_unavailable", "link_inspection_failed"]);
  const LINK_PARSER_REASONS = {
    header_count_limit: "Retained header count limit reached", header_byte_limit: "Retained header byte limit reached",
    link_field_byte_limit: "An oversized Link field was skipped", candidate_limit: "Link member candidate limit reached",
    parameter_limit: "A member parameter limit was reached", relation_token_limit: "A member relation-token limit was reached",
    observation_limit: "Declaration output limit reached", malformed_field: "A Link field has unsupported framing",
    unsupported_target: "A target is outside the supported absolute HTTP(S) grammar", anchor_present: "A member with an anchor was excluded",
    missing_relation: "A member has no rel parameter", unsupported_relation_syntax: "A rel value uses unsupported syntax",
  };
  const LINK_REASONS = { ...BODY_REFERENCE_REASONS, ...LINK_PARSER_REASONS, unsupported_source_context: "Only retained HTTP(S) GET or HEAD responses with status 200 are supported", session_source_changed: "The active session source changed", metadata_unavailable: "Retained response metadata is unavailable", link_inspection_failed: "Retained Link headers could not be inspected" };
  function normalizeLinkDeclarations(payload, sessionId, evidenceId, row) {
    const invalid = () => { throw new Error("Invalid retained Link-declaration response"); };
    const integer = (value, max = Number.MAX_SAFE_INTEGER) => Number.isSafeInteger(value) && value >= 0 && value <= max;
    const canonical = (value, max = LINK_DECLARATION_LIMITS.target_origin_bytes) => {
      if (typeof value !== "string" || !value || value.length > max || !/^[\x21-\x7e]+$/.test(value) || value !== safeOrigin(value)) return false;
      if (max !== LINK_DECLARATION_LIMITS.target_origin_bytes) return true;
      const host = new URL(value).hostname;
      if (host.startsWith("[")) return true;
      const dns = host.endsWith(".") ? host.slice(0, -1) : host;
      return dns.length > 0 && dns.length <= 253 && host.length <= 254 && dns.split(".").every((label) => label.length <= 63
        && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(label));
    };
    const sourceFields = ["source_origin", "source_sequence", "source_started_at", "source_method", "source_status", "retained_header_count"];
    const countFields = ["headers_inspected", "header_bytes_admitted", "link_fields_inspected", "candidates_inspected", "members_framed", "parameters_inspected", "relation_tokens_inspected", "unknown_relation_tokens_inspected"];
    if (!exactFields(payload, ["schema_version", "relation_catalog_version", "session_id", "evidence_id", "available", "status", "complete", "reasons", ...sourceFields, "coverage", "offset_basis", "counts", "observations", "limits"])
      || payload.schema_version !== 1 || payload.relation_catalog_version !== 1 || payload.session_id !== sessionId || payload.evidence_id !== evidenceId
      || !validHtmlGeneratorEvidence(evidenceId, sessionId, typeof evidenceId === "string" ? evidenceId.split(".")[2] : null)
      || !["scanned", "unsupported", "unavailable"].includes(payload.status) || payload.available !== (payload.status === "scanned")
      || typeof payload.complete !== "boolean" || payload.coverage !== "absolute_http_link_declarations" || payload.offset_basis !== "retained_header_value_utf8"
      || !exactFields(payload.limits, Object.keys(LINK_DECLARATION_LIMITS)) || !Object.entries(LINK_DECLARATION_LIMITS).every(([key, value]) => payload.limits[key] === value)
      || !exactFields(payload.counts, countFields) || countFields.some((key) => !integer(payload.counts[key]))
      || !Array.isArray(payload.observations) || payload.observations.length > LINK_DECLARATION_LIMITS.observations
      || !Array.isArray(payload.reasons) || payload.reasons.length > Object.keys(LINK_PARSER_REASONS).length
      || payload.reasons.some((reason, index) => typeof reason !== "string" || index > 0 && payload.reasons[index - 1] >= reason)) invalid();
    const c = payload.counts;
    if (!payload.available) {
      if (payload.complete || payload.reasons.length !== 1 || !LINK_ACQUISITION_REASONS.has(payload.reasons[0])
        || sourceFields.some((key) => payload[key] !== null) || countFields.some((key) => c[key] !== 0) || payload.observations.length) invalid();
    } else {
      if (!canonical(payload.source_origin, 4096) || payload.source_origin !== row.origin || payload.source_sequence !== row.sequence
        || payload.source_sequence !== evidenceId.split(".")[3] || payload.source_started_at !== row.started_at || observationTime(payload.source_started_at) === null
        || !["GET", "HEAD"].includes(payload.source_method) || payload.source_method !== row.method || payload.source_status !== 200 || row.status !== 200
        || !integer(payload.retained_header_count) || payload.complete !== (payload.reasons.length === 0)
        || payload.reasons.some((reason) => !Object.prototype.hasOwnProperty.call(LINK_PARSER_REASONS, reason))
        || !integer(c.headers_inspected, Math.min(payload.retained_header_count, 128)) || !integer(c.header_bytes_admitted, 32768)
        || c.headers_inspected === 0 && c.header_bytes_admitted !== 0 || c.header_bytes_admitted < c.link_fields_inspected * 4
        || !integer(c.link_fields_inspected, c.headers_inspected) || c.link_fields_inspected === 0 && c.candidates_inspected !== 0 || !integer(c.candidates_inspected, 128) || !integer(c.members_framed, c.candidates_inspected)
        || !integer(c.parameters_inspected, c.candidates_inspected * 32) || !integer(c.relation_tokens_inspected, c.members_framed * 32)
        || !integer(c.unknown_relation_tokens_inspected, c.relation_tokens_inspected) || payload.observations.length > c.relation_tokens_inspected - c.unknown_relation_tokens_inspected
        || c.headers_inspected < payload.retained_header_count && !["header_count_limit", "header_byte_limit", "candidate_limit"].some((reason) => payload.reasons.includes(reason))
        || payload.reasons.includes("header_byte_limit") && c.headers_inspected >= payload.retained_header_count
        || payload.reasons.includes("parameter_limit") && c.parameters_inspected < 32
        || payload.reasons.includes("relation_token_limit") && c.relation_tokens_inspected < 32
        || payload.reasons.includes("link_field_byte_limit") && (!c.link_fields_inspected || c.header_bytes_admitted < 4101)
        || payload.complete && (c.headers_inspected !== payload.retained_header_count || c.candidates_inspected !== c.members_framed || payload.observations.length !== c.relation_tokens_inspected - c.unknown_relation_tokens_inspected)
        || payload.reasons.includes("candidate_limit") && c.candidates_inspected !== 128
        || payload.reasons.includes("observation_limit") && (payload.observations.length !== 32 || c.relation_tokens_inspected - c.unknown_relation_tokens_inspected < 33)
        || payload.reasons.includes("header_count_limit") && (c.headers_inspected !== 128 || payload.retained_header_count <= 128)) invalid();
    }
    const headers = new Map(), members = new Set();
    let previous = null;
    const observations = payload.observations.map((item) => {
      const relation = Object.prototype.hasOwnProperty.call(LINK_RELATIONS, item?.relation) ? LINK_RELATIONS[item.relation] : null;
      const span = (value) => exactFields(value, ["start", "end"]) && integer(value.start, item.header_value_bytes) && integer(value.end, item.header_value_bytes) && value.start < value.end;
      if (!exactFields(item, ["relation", "target_origin", "source", "strength", "header_name", "header_index", "header_value_bytes", "link_value_span", "target_span", "relation_span", "rule_id", "offset_basis"])
        || !relation || !canonical(item.target_origin) || item.source !== "response_link_header" || item.strength !== "literal_declaration"
        || item.header_name !== "link" || item.rule_id !== relation.rule || item.offset_basis !== "retained_header_value_utf8"
        || !integer(item.header_index, c.headers_inspected - 1) || !integer(item.header_value_bytes, 4096)
        || !span(item.link_value_span) || !span(item.target_span) || !span(item.relation_span)
        || item.target_span.start !== item.link_value_span.start + 1 || item.target_span.end >= item.relation_span.start
        || item.target_span.end - item.target_span.start > 4096 || item.relation_span.end > item.link_value_span.end
        || item.relation_span.end - item.relation_span.start !== item.relation.length
        || headers.has(item.header_index) && headers.get(item.header_index) !== item.header_value_bytes) invalid();
      if (previous) {
        const sameMember = item.header_index === previous.header_index && item.link_value_span.start === previous.link_value_span.start;
        if (item.header_index < previous.header_index || item.header_index === previous.header_index && (sameMember
          ? item.link_value_span.end !== previous.link_value_span.end || item.target_span.start !== previous.target_span.start || item.target_span.end !== previous.target_span.end
            || item.target_origin !== previous.target_origin || item.relation_span.start <= previous.relation_span.end
          : item.link_value_span.start <= previous.link_value_span.end)) invalid();
      }
      headers.set(item.header_index, item.header_value_bytes);
      members.add(JSON.stringify([item.header_index, item.link_value_span.start])); previous = item;
      return { ...item, link_value_span: { ...item.link_value_span }, target_span: { ...item.target_span }, relation_span: { ...item.relation_span } };
    });
    if (headers.size > c.link_fields_inspected || members.size > c.members_framed || members.size > c.parameters_inspected
      || [...headers.values()].reduce((sum, size) => sum + size + 4, 0) > c.header_bytes_admitted) invalid();
    return { status: payload.status, complete: payload.complete, reasons: [...payload.reasons], observations, counts: { ...c }, retained_header_count: payload.retained_header_count };
  }
  const HTML_GENERATOR_LIMITS = Object.freeze({ scan_bytes: 262144, serialized_row_bytes: 2097152, tag_candidates: 4096, tag_bytes: 4096, attributes: 32, observations: 8, version_bytes: 32, version_separator_bytes: 32 });
  const HTML_GENERATOR_ACQUISITION_REASONS = new Set([
    "invalid_evidence_id", "session_mismatch", "stale_generation", "record_missing", "record_changed", "body_identity_unverifiable",
    "response_unavailable", "metadata_limit", "unsupported_content_type", "unsupported_body_encoding", "unsupported_content_encoding",
    "unsupported_metadata", "serialized_row_limit", "body_storage_unavailable", "body_read_failed", "malformed_retained_row",
    "retained_preview_unavailable", "body_scan_failed", "inactive_session", "session_source_changed", "metadata_unavailable",
  ]);
  const HTML_GENERATOR_STOP_REASONS = {
    end_of_preview: "Preview ended before a supported closing head", input_limit: "Input limit reached before a supported closing head",
    tag_candidate_limit: "Tag candidate limit reached", tag_byte_limit: "Tag byte limit reached", attribute_limit: "Attribute limit reached",
    observation_limit: "Generator declaration output limit reached", unsupported_document_context: "Document context is unsupported",
    unsupported_head_context: "Head context is unsupported", unsupported_doctype: "Doctype is unsupported",
    unsupported_comment: "Comment context is unsupported", unsupported_script_escape: "Script escape context is unsupported",
    unsupported_entity: "Entity spelling is unsupported", unsupported_self_closing: "Self-closing context is unsupported",
    malformed_tag: "Tag structure is unsupported", duplicate_attribute: "Duplicate attribute encountered", null_byte: "Null byte encountered",
  };
  const HTML_GENERATOR_REASONS = { ...BODY_REFERENCE_REASONS, ...HTML_GENERATOR_STOP_REASONS,
    unsupported_content_type: "Only retained HTML text previews are supported", session_source_changed: "The active session source changed",
    metadata_unavailable: "Retained metadata is unavailable" };
  function validHtmlGeneratorEvidence(value, sessionId, generation) {
    if (typeof value !== "string") return false;
    const fields = value.split("."), uuid = (part) => typeof part === "string" && part.length === 36
      && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(part);
    return fields.length === 6 && [fields[1], fields[2], fields[4], fields[5]].every(uuid)
      && !/[^0-9]/.test(fields[3]) && validTechnologyEvidence(value, sessionId, generation);
  }
  function normalizeHtmlGenerators(payload, sessionId, evidenceId, row) {
    const invalid = () => { throw new Error("Invalid retained HTML-generator response"); };
    const integer = (value, max = Number.MAX_SAFE_INTEGER) => Number.isSafeInteger(value) && value >= 0 && value <= max;
    const fields = ["schema_version", "generator_catalog_version", "session_id", "evidence_id", "available", "status", "complete", "reasons",
      "source_origin", "source_sequence", "source_started_at", "retained_preview_bytes", "scan_prefix_bytes", "capture_preview_truncated",
      "scan_prefix_truncated", "coverage", "head_closed", "parser_stop", "tag_candidates_inspected", "additional_tag_candidate_encountered",
      "additional_observation_encountered", "unknown_product_declarations", "observations", "limits", "offset_basis"];
    if (!exactFields(payload, fields) || payload.schema_version !== 1 || payload.generator_catalog_version !== 1
      || payload.session_id !== sessionId || payload.evidence_id !== evidenceId
      || !validHtmlGeneratorEvidence(evidenceId, sessionId, typeof evidenceId === "string" ? evidenceId.split(".")[2] : null)
      || !["scanned", "unsupported", "unavailable"].includes(payload.status) || payload.available !== (payload.status === "scanned")
      || ["complete", "capture_preview_truncated", "scan_prefix_truncated", "head_closed", "additional_tag_candidate_encountered", "additional_observation_encountered"].some((key) => typeof payload[key] !== "boolean")
      || payload.coverage !== "literal_html_generator_explicit_head" || payload.offset_basis !== "retained_utf8_preview"
      || !exactFields(payload.limits, Object.keys(HTML_GENERATOR_LIMITS))
      || !Object.entries(HTML_GENERATOR_LIMITS).every(([key, value]) => payload.limits[key] === value)
      || !integer(payload.scan_prefix_bytes, HTML_GENERATOR_LIMITS.scan_bytes)
      || !integer(payload.tag_candidates_inspected, HTML_GENERATOR_LIMITS.tag_candidates)
      || !integer(payload.unknown_product_declarations, payload.tag_candidates_inspected)
      || !Array.isArray(payload.observations) || payload.observations.length > HTML_GENERATOR_LIMITS.observations
      || !Array.isArray(payload.reasons) || payload.reasons.length > 3
      || payload.reasons.some((reason, index) => typeof reason !== "string" || index > 0 && payload.reasons[index - 1] >= reason)) invalid();
    const scanned = payload.available;
    if (!scanned) {
      if (payload.complete || payload.reasons.length !== 1 || payload.reasons.some((reason) => !HTML_GENERATOR_ACQUISITION_REASONS.has(reason))
        || ["source_origin", "source_sequence", "source_started_at", "retained_preview_bytes", "parser_stop"].some((key) => payload[key] !== null)
        || payload.scan_prefix_bytes || payload.tag_candidates_inspected || payload.unknown_product_declarations || payload.observations.length
        || payload.head_closed || payload.capture_preview_truncated || payload.scan_prefix_truncated
        || payload.additional_tag_candidate_encountered || payload.additional_observation_encountered) invalid();
    } else {
      if (typeof payload.source_origin !== "string" || payload.source_origin.length > 4096 || payload.source_origin !== safeOrigin(payload.source_origin)
        || payload.source_origin !== row.origin || payload.source_sequence !== row.sequence || payload.source_sequence !== evidenceId.split(".")[3]
        || payload.source_started_at !== row.started_at || observationTime(payload.source_started_at) === null
        || !integer(payload.retained_preview_bytes) || payload.scan_prefix_bytes > payload.retained_preview_bytes
        || (payload.retained_preview_bytes <= HTML_GENERATOR_LIMITS.scan_bytes ? payload.scan_prefix_bytes !== payload.retained_preview_bytes
          : payload.scan_prefix_bytes < HTML_GENERATOR_LIMITS.scan_bytes - 3)
        || payload.scan_prefix_truncated !== (payload.scan_prefix_bytes < payload.retained_preview_bytes)
        || payload.head_closed !== (payload.parser_stop === null)) invalid();
      const stop = payload.parser_stop;
      if (stop !== null && (!exactFields(stop, ["reason", "byte_cursor"]) || !Object.prototype.hasOwnProperty.call(HTML_GENERATOR_STOP_REASONS, stop.reason)
        || !integer(stop.byte_cursor, payload.scan_prefix_bytes)
        || stop.reason === "input_limit" && (!payload.scan_prefix_truncated || stop.byte_cursor !== payload.scan_prefix_bytes)
        || stop.reason === "end_of_preview" && (payload.scan_prefix_truncated || stop.byte_cursor !== payload.scan_prefix_bytes))) invalid();
      const declarations = payload.observations.length + payload.unknown_product_declarations + Number(payload.additional_observation_encountered);
      const minimumTags = declarations ? declarations + 2 + Number(payload.head_closed) : payload.head_closed ? 3 : 0;
      const reasons = [...(payload.capture_preview_truncated ? ["capture_preview_truncated"] : []),
        ...(payload.scan_prefix_truncated ? ["scan_prefix_truncated"] : []), ...(stop ? [stop.reason] : [])].sort();
      if (JSON.stringify(payload.reasons) !== JSON.stringify(reasons) || payload.complete !== (payload.head_closed && !reasons.length)
        || payload.additional_tag_candidate_encountered !== (stop?.reason === "tag_candidate_limit")
        || payload.additional_tag_candidate_encountered && payload.tag_candidates_inspected !== HTML_GENERATOR_LIMITS.tag_candidates
        || payload.additional_observation_encountered !== (stop?.reason === "observation_limit")
        || payload.additional_observation_encountered && payload.observations.length !== HTML_GENERATOR_LIMITS.observations
        || payload.tag_candidates_inspected < minimumTags || payload.tag_candidates_inspected > payload.scan_prefix_bytes
        || stop && payload.tag_candidates_inspected > stop.byte_cursor + 1) invalid();
    }
    let previousTagEnd = 0;
    const observations = payload.observations.map((item) => {
      const name = item?.product_id === "wordpress" ? "WordPress" : item?.product_id === "drupal" ? "Drupal" : null;
      const span = (value) => exactFields(value, ["start", "end"]) && integer(value.start, payload.scan_prefix_bytes)
        && integer(value.end, payload.scan_prefix_bytes) && value.start < value.end;
      if (!exactFields(item, ["product_id", "name", "catalog_category", "reported_version", "version_status", "source", "strength", "rule_id", "tag_span", "content_span", "product_span", "version_span", "offset_basis"])
        || !name || item.name !== name || item.catalog_category !== "cms" || item.source !== "response_html_meta_generator"
        || item.strength !== "literal_declaration" || item.rule_id !== `html_meta_generator.${item.product_id}.v1`
        || item.offset_basis !== "retained_utf8_preview" || !["reported", "not_reported", "unsupported_format"].includes(item.version_status)
        || !span(item.tag_span) || !span(item.content_span) || !span(item.product_span)
        || item.tag_span.start < previousTagEnd || item.tag_span.end - item.tag_span.start > HTML_GENERATOR_LIMITS.tag_bytes
        || item.content_span.start <= item.tag_span.start || item.content_span.end >= item.tag_span.end
        || item.product_span.start !== item.content_span.start || item.product_span.end > item.content_span.end
        || item.product_span.end - item.product_span.start !== name.length
        || payload.parser_stop && item.tag_span.end > payload.parser_stop.byte_cursor) invalid();
      if (item.version_status === "reported") {
        if (typeof item.reported_version !== "string" || item.reported_version.length > HTML_GENERATOR_LIMITS.version_bytes
          || !item.reported_version.split(".").every((part) => part.length > 0 && !/[^0-9]/.test(part)) || !span(item.version_span)
          || item.version_span.start <= item.product_span.end || item.version_span.start - item.product_span.end > HTML_GENERATOR_LIMITS.version_separator_bytes
          || item.version_span.end !== item.content_span.end || item.version_span.end - item.version_span.start !== item.reported_version.length) invalid();
      } else if (item.reported_version !== null || item.version_span !== null
        || item.version_status === "not_reported" && item.product_span.end !== item.content_span.end
        || item.version_status === "unsupported_format" && item.product_span.end === item.content_span.end) invalid();
      previousTagEnd = item.tag_span.end;
      return { product_id: item.product_id, name, catalog_category: "cms", reported_version: item.reported_version, version_status: item.version_status,
        source: item.source, strength: item.strength, rule_id: item.rule_id, tag_span: { ...item.tag_span }, content_span: { ...item.content_span },
        product_span: { ...item.product_span }, version_span: item.version_span ? { ...item.version_span } : null, offset_basis: item.offset_basis };
    });
    return { status: payload.status, complete: payload.complete, reasons: [...payload.reasons], source_origin: payload.source_origin,
      source_sequence: payload.source_sequence, source_started_at: payload.source_started_at, retained_preview_bytes: payload.retained_preview_bytes,
      scan_prefix_bytes: payload.scan_prefix_bytes, capture_preview_truncated: payload.capture_preview_truncated, scan_prefix_truncated: payload.scan_prefix_truncated,
      head_closed: payload.head_closed, parser_stop: payload.parser_stop ? { ...payload.parser_stop } : null,
      tag_candidates_inspected: payload.tag_candidates_inspected, additional_tag_candidate_encountered: payload.additional_tag_candidate_encountered,
      additional_observation_encountered: payload.additional_observation_encountered, unknown_product_declarations: payload.unknown_product_declarations, observations };
  }
  function normalizeSnapshot(payload, sessionId, requestedVersion = TECHNOLOGY_CATALOG_VERSION) {
    if (payload?.schema_version !== 1 || payload.session_id !== sessionId || !payload.snapshot || !Array.isArray(payload.origins)) {
      throw new Error("Invalid service map response");
    }
    const catalogVersion = technologyCatalogVersion(payload, requestedVersion);
    for (const [key, limit] of Object.entries({ technology_observations_per_row: 8, technology_observations_per_origin: 32, technology_observations: 2000, technology_version_bytes: 32, technology_token_candidates_per_row: 128 })) {
      if (payload.snapshot.limits?.[key] !== undefined && payload.snapshot.limits[key] !== limit) invalidTechnology();
    }
    if (array(payload.snapshot.stopped_by).includes("technology_token_scan_limit") && payload.snapshot.complete !== false) invalidTechnology();
    let technologyCount = 0;
    const technologySources = new Map();
    const technologies = payload.origins.map((item) => {
      const observations = normalizeOriginTechnologies(item, sessionId, payload.snapshot.generation, catalogVersion);
      technologyCount += observations.length;
      if (technologyCount > 2000) invalidTechnology();
      for (const observation of observations) for (const id of observation.evidence) {
        const sequence = id.split(".")[3], previous = technologySources.get(sequence);
        if (previous && (previous.origin !== item.origin || previous.id !== id)) invalidTechnology();
        technologySources.set(sequence, { origin: item.origin, id });
      }
      return observations;
    });
    // The same retained sequence must retain one complete token identity across
    // new observations and legacy evidence collections, including record revision.
    for (const item of payload.origins) {
      for (const source of [...array(item?.endpoints), ...array(item?.technology_clues), ...array(item?.redirects), ...array(item?.header_links)]) {
        for (const id of array(source?.evidence)) {
          const previous = typeof id === "string" && technologySources.get(id.split(".")[3]);
          if (previous && (previous.origin !== item.origin || previous.id !== id)) invalidTechnology();
        }
      }
    }
    const seen = new Set();
    const origins = payload.origins.flatMap((item, index) => {
      const origin = safeOrigin(item?.origin);
      if (!origin || seen.has(origin)) return [];
      seen.add(origin);
      return [{
        origin, captured: true, in_scope: item.in_scope === true, request_count: count(item.request_count),
        first_seen: item.first_seen, last_seen: item.last_seen, technology_observations: technologies[index],
        technology_observations_present: Object.prototype.hasOwnProperty.call(item, "technology_observations"),
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
    return { snapshot: payload.snapshot, origins, warningCount: array(payload.warnings).length, technologyCatalogVersion: catalogVersion, legacyCatalog: catalogVersion !== requestedVersion };
  }
  function stableOrder(previous, origins) {
    const ids = new Set(origins.map((origin) => origin.origin));
    const previousIds = new Set(previous);
    return [...previous.filter((id) => ids.has(id)), ...[...ids].filter((id) => !previousIds.has(id)).sort()];
  }
  function graphGeometry(textSize = 12) {
    const size = Math.max(8, Math.min(20, Number.isFinite(textSize) ? textSize : 12));
    // Four text rows, three gaps, two card paddings/borders, and clue borders.
    // Larger Display text must grow the graph as well as its text, otherwise
    // fixed compact rows clip glyphs and leave SVG arrows at the wrong height.
    const cardHeight = Math.max(GRAPH_GEOMETRY.cardHeight, Math.ceil((Math.max(10, size - 1) + 3 * Math.max(9, size - 2)) * GRAPH_GEOMETRY.lineHeight + 36));
    return { ...GRAPH_GEOMETRY, cardHeight, rowPitch: cardHeight + GRAPH_GEOMETRY.rowPitch - GRAPH_GEOMETRY.cardHeight };
  }
  function graphLayout(origins, order, previousSlots = new Map(), anchor = null, bodyEdges = [], geometry = GRAPH_GEOMETRY) {
    const width = geometry.padding * 2 + geometry.columnPitch + geometry.cardWidth;
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
    const visible = visibleOrigins.map((origin) => ({ ...origin, x: geometry.padding + slots.get(origin.origin).column * geometry.columnPitch, y: geometry.padding + slots.get(origin.origin).row * geometry.rowPitch }));
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
      // Ten typed/reverse metadata + Link arrows can share a pair. Keep
      // distinct attachment lanes inside the card with marker clearance.
      const lane = (geometry.cardHeight - 24) / 10;
      const offset = [0, -1, 1, -2, 2, -3, 3, -4, 4, -5][Math.min(pairIndex, 9)] * lane;
      const toRight = target.x > source.x;
      const sameColumn = target.x === source.x;
      const x1 = source.x + (sameColumn ? geometry.cardWidth : toRight ? geometry.cardWidth : 0);
      const x2 = target.x + (sameColumn ? geometry.cardWidth : toRight ? 0 : geometry.cardWidth);
      const y1 = source.y + geometry.cardHeight / 2 + offset;
      const y2 = target.y + geometry.cardHeight / 2 + offset;
      // Keep right-column loops inside the SVG, with room for stroke and markers.
      // Attachment offsets still distinguish typed links without widening cards.
      const bend = sameColumn ? Math.min(x1 + geometry.padding + Math.abs(offset), width - geometry.loopInset) : (x1 + x2) / 2;
      edges.push({ ...edge, path: `M ${x1} ${y1} C ${bend} ${y1}, ${bend} ${y2}, ${x2} ${y2}` });
    }
    return { nodes: visible, edges, edgeCount, slots, geometry, width, height: Math.max(geometry.padding * 2 + geometry.cardHeight, ...visible.map((origin) => origin.y + geometry.cardHeight + geometry.padding)) };
  }

  function peerGraphLayout(origins, sourceOrigin, geometry = GRAPH_GEOMETRY) {
    // This visualization contains observations only. No metadata or domain-to-
    // domain arrows are carried over from the normal service map.
    const ordered = [...origins.filter((item) => item.origin === sourceOrigin), ...origins.filter((item) => item.origin !== sourceOrigin)].slice(0, MAX_CARDS - 1);
    const width = geometry.padding * 2 + geometry.columnPitch + geometry.cardWidth;
    const endpoint = { origin: PEER_NODE_ID, endpointNode: true, x: geometry.padding, y: geometry.padding };
    const nodes = [endpoint, ...ordered.map((item, index) => ({ ...item, x: geometry.padding + geometry.columnPitch, y: geometry.padding + index * geometry.rowPitch }))];
    const x1 = endpoint.x + geometry.cardWidth, y1 = endpoint.y + geometry.cardHeight / 2;
    const edges = nodes.slice(1, MAX_EDGES + 1).map((item) => ({ source: item.origin, target: PEER_NODE_ID, relationship: "recorded_peer_observation", label: "Recorded address observation", undirected: true,
      path: `M ${x1} ${y1} C ${(x1 + item.x) / 2} ${y1}, ${(x1 + item.x) / 2} ${item.y + geometry.cardHeight / 2}, ${item.x} ${item.y + geometry.cardHeight / 2}` }));
    return { nodes, edges, edgeCount: ordered.length, slots: new Map(), geometry, width,
      height: Math.max(geometry.padding * 2 + geometry.cardHeight, ...nodes.map((item) => item.y + geometry.cardHeight + geometry.padding)) };
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
  const referenceOrigin = (id) => ({ origin: id, captured: false, in_scope: false, endpoints: [], technology_clues: [], technology_observations: [], technology_observations_present: false, redirects: [], header_links: [], references: [], referencesOmitted: 0 });
  function createController({ elements: els, getSessionId, getEventStreamReady = () => true, request: sendRequest, document: doc = root.document }) {
    // A local retained-data read must not turn an API redirect into another
    // request. Enforce this for every map/inspector read, preserving its signal.
    const request = (path, options) => sendRequest(path, { ...options, redirect: "error" });
    const productFilterContainer = els.productFilters || els.panel.querySelector?.("#serviceMapProductFilters");
    const state = {
      sessionId: null, snapshot: null, order: [], selectedOrigin: null, selectionInitialized: false,
      query: "", scopeOnly: false, view: "service-map", detailTab: "apis", endpointIndex: null, endpointSelection: null,
      endpointMethod: "", endpointPath: "", productId: "", productVersion: "", pageAnchor: null, detailPageAnchor: null,
      loading: false, error: "", refreshPaused: false, requestId: 0, evidenceRequestId: 0, evidence: null,
      evidenceLoading: false, evidenceError: "", evidenceId: null, evidenceCatalogVersion: null, evidenceLegacyCatalog: false, socketPeer: null, page: 0, detailPage: 0,
      peerMatches: null, peerMatchesSource: null, peerMatchesLoading: false, peerMatchesError: "", peerMatchesRequestId: 0, peerEvidenceBinding: null,
      bodyReferences: null, bodyReferencesLoading: false, bodyReferencesError: "", bodyReferencesRequestId: 0,
      bodyReferencesSource: null, bodyReferenceOverlay: null, bodyOverlayNotice: "",
      relationshipKind: "body", linkDeclarations: null, linkDeclarationsSource: null, linkDeclarationsError: "",
      htmlGenerators: null, htmlGeneratorsLoading: false, htmlGeneratorsError: "", htmlGeneratorsRequestId: 0, htmlGeneratorsSource: null,
      inactiveSessionId: null, historyVersion: 0, stale: false, anchorOrigin: null, modeNotice: "", zoomScale: 1,
    };
    let abort = null;
    let evidenceAbort = null;
    let bodyReferencesAbort = null;
    let peerMatchesAbort = null;
    let htmlGeneratorsAbort = null;
    let detailRender = null;
    let visible = false;
    let currentLayout = null;
    function viewportSize() {
      return { width: Number(els.viewport?.clientWidth) || 0, height: Number(els.viewport?.clientHeight) || 0 };
    }
    function zoomAvailable() {
      const size = viewportSize();
      return visible && state.view === "service-map" && !!currentLayout?.nodes.length && size.width > 0 && size.height > 0;
    }
    function updateZoomControls() {
      const available = zoomAvailable();
      els.zoomControls?.classList.toggle("hidden", state.view === "apis");
      if (els.zoomFit) els.zoomFit.disabled = !available;
      if (els.zoomOut) els.zoomOut.disabled = !available || state.zoomScale <= ZOOM_LIMITS.min;
      if (els.zoomIn) els.zoomIn.disabled = !available || state.zoomScale >= ZOOM_LIMITS.max;
      if (els.zoomReset) {
        els.zoomReset.disabled = !available;
        els.zoomReset.textContent = `${Math.round(state.zoomScale * 100)}%`;
        els.zoomReset.setAttribute("aria-label", `Zoom ${Math.round(state.zoomScale * 100)}%. Reset to 100%`);
      }
    }
    function applyGraphScale() {
      // Mutating the canvas variables leaves card DOM, focus, inspector and
      // in-flight exact-source reads untouched, including during polling.
      if (currentLayout && els.canvas.style) {
        els.canvas.style.setProperty("--service-map-scale", String(state.zoomScale));
        els.canvas.style.setProperty("--service-map-scaled-width", `${currentLayout.width * state.zoomScale}px`);
        els.canvas.style.setProperty("--service-map-scaled-height", `${currentLayout.height * state.zoomScale}px`);
      }
      updateZoomControls();
    }
    function setZoom(value, { fit = false } = {}) {
      if (!zoomAvailable() || !Number.isFinite(value)) return false;
      const viewport = els.viewport, size = viewportSize();
      const previous = state.zoomScale;
      const next = Math.max(ZOOM_LIMITS.min, Math.min(ZOOM_LIMITS.max, (fit ? Math.floor(value * 10000) : Math.round(value * 10000)) / 10000));
      // The canvas centers a small graph. Include that inset when preserving
      // the point under the viewport center across either scrollbar boundary.
      const centerX = (viewport.scrollLeft + size.width / 2 - Math.max(0, (size.width - currentLayout.width * previous) / 2)) / previous;
      const centerY = (viewport.scrollTop + size.height / 2 - Math.max(0, (size.height - currentLayout.height * previous) / 2)) / previous;
      state.zoomScale = next;
      applyGraphScale();
      const scaledWidth = currentLayout.width * next, scaledHeight = currentLayout.height * next;
      const nextSize = viewportSize();
      viewport.scrollLeft = fit ? 0 : Math.max(0, Math.min(scaledWidth - nextSize.width, centerX * next + Math.max(0, (nextSize.width - scaledWidth) / 2) - nextSize.width / 2));
      viewport.scrollTop = fit ? 0 : Math.max(0, Math.min(scaledHeight - nextSize.height, centerY * next + Math.max(0, (nextSize.height - scaledHeight) / 2) - nextSize.height / 2));
      return true;
    }
    function fitGraph() {
      if (!zoomAvailable()) return false;
      const size = viewportSize();
      return setZoom(Math.min(1, size.width / currentLayout.width, size.height / currentLayout.height), { fit: true });
    }
    function replace(element, html) {
      if (!element || element.innerHTML === html) return;
      const focused = doc?.activeElement;
      const key = focused?.dataset?.serviceFocus;
      const owned = key && element.contains(focused);
      const caret = owned && focused.dataset.serviceEndpointFilter === "path"
        ? [focused.selectionStart, focused.selectionEnd, focused.selectionDirection] : null;
      element.innerHTML = html;
      // Only restore the element that this replacement detached. A newly focused
      // dialog or control is never displaced by a background refresh.
      if (owned && visible && (!doc.activeElement || doc.activeElement === doc.body || doc.activeElement === focused && !focused.isConnected)
        && !doc.querySelector?.(".modal-backdrop:not(.hidden), dialog[open]")) {
        const next = [...element.querySelectorAll("[data-service-focus]")].find((candidate) => candidate.dataset.serviceFocus === key);
        if (next && !next.disabled && next.getClientRects().length) {
          next.focus({ preventScroll: true });
          if (caret && typeof caret[0] === "number") next.setSelectionRange?.(...caret);
        }
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
    function originFilterStages(origins) {
      const query = state.query.trim().toLowerCase();
      const searched = origins.filter((origin) => !query || origin.origin.toLowerCase().includes(query));
      const matching = searched.filter((origin) => productMatches(origin, state.productId, state.productVersion));
      return { loaded: origins, searched, matching, queryHidden: origins.length - searched.length, productHidden: searched.length - matching.length };
    }
    function filteredOrigins() {
      return originFilterStages(graphOrigins().filter((origin) => !state.scopeOnly || origin.in_scope || !origin.captured)).matching;
    }
    function setProductFilter(kind, value) {
      if (!["product", "version", "clear"].includes(kind)) return;
      if (kind === "clear") { state.productId = ""; state.productVersion = ""; }
      else if (kind === "product") {
        if (value && !TECHNOLOGY_CATALOG_V2.has(value)) return;
        state.productId = value || "";
      } else {
        if (value && !PRODUCT_VERSION_OPTIONS.some(([key]) => key === value)
          && !(typeof value === "string" && value.length <= 41 && /^reported:[0-9]+(?:\.[0-9]+)*$/.test(value))) return;
        state.productVersion = value || "";
      }
      // Product filters change membership, never source identity or pending
      // reads. Retain exact catalog anchors and the independent inspector page.
      if (state.snapshot) reconcileEndpointPages();
      render();
    }
    function renderProductFilters() {
      if (!productFilterContainer) return;
      const all = (state.snapshot?.origins || []).flatMap(reportedProducts);
      const products = new Map(all.map((product) => [product.id, product.name]));
      const selected = all.filter((product) => !state.productId || product.id === state.productId);
      const versions = new Set(selected.flatMap((product) => [...product.versions]));
      const options = new Map([["", "Any version/status"], ...PRODUCT_VERSION_OPTIONS,
        ...[...versions].sort().map((version) => [`reported:${version}`, `Reported: ${version}`])]);
      const unavailableProduct = !!state.productId && !products.has(state.productId);
      const unavailableVersion = !!state.productVersion && !(state.snapshot?.origins || []).some((origin) => productMatches(origin, state.productId, state.productVersion));
      if (state.productId) products.set(state.productId, TECHNOLOGY_CATALOG_V2.get(state.productId)[0]);
      if (state.productVersion.startsWith("reported:") && !options.has(state.productVersion)) options.set(state.productVersion, `Reported: ${state.productVersion.slice(9)} (unavailable)`);
      replace(productFilterContainer, `<div class="service-map-product-filter-controls"><label>Product (exact)<select data-service-product-filter="product" data-service-focus="products:product" aria-describedby="serviceMapProductHelp"><option value="">All products</option>${[...products].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([id, name]) => `<option value="${escape(id)}" ${state.productId === id ? "selected" : ""}>${escape(name)}${unavailableProduct && state.productId === id ? " (unavailable)" : ""}</option>`).join("")}</select></label><label>Version/status (exact)<select data-service-product-filter="version" data-service-focus="products:version" title="${escape(options.get(state.productVersion))}" aria-describedby="serviceMapProductHelp">${[...options].map(([id, name]) => `<option value="${escape(id)}" ${state.productVersion === id ? "selected" : ""}>${escape(name)}</option>`).join("")}</select></label><button class="secondary-action" type="button" data-service-product-filter-clear="true" data-service-focus="products:clear" ${!state.productId && !state.productVersion ? "disabled" : ""}>Clear</button></div><p id="serviceMapProductHelp" class="service-map-muted">Loaded response-header declarations only. Product and version/status match the same product. Versions are literal and unverified. Options are independent of origin search.</p>${unavailableProduct || unavailableVersion ? '<p class="service-map-filter-notice" role="status">Selected product/version is unavailable in the loaded snapshot. Filters retained; zero matching origins.</p>' : ""}`);
      productFilterContainer.parentElement?.classList.toggle("has-filter", !!state.productId || !!state.productVersion);
      const summary = productFilterContainer.parentElement?.querySelector("summary");
      if (summary) summary.textContent = state.productId || state.productVersion ? "Products / versions · active" : "Products / versions";
    }
    function findOrigin(id) {
      if (!id) return null;
      const existing = graphOrigins().find((origin) => origin.origin === id);
      const peerOrigin = peerOverlayOrigins().find((origin) => origin.origin === id);
      if (peerOrigin) return existing?.captured ? { ...existing, peerGroup: peerOrigin.peerGroup, peerSource: peerOrigin.peerSource } : peerOrigin;
      if (existing) return existing;
      if (state.extraOriginSnapshot === state.snapshot && state.extraOriginId === id && state.extraOriginOverlay === state.bodyReferenceOverlay) return state.extraOrigin;
      // A direct neighbor can be outside the All origins reference-card budget.
      // Its inspector still uses only loaded declarations and the usual pane caps.
      const item = referenceOrigin(id), group = { items: [], omitted: 0 }, seen = new Set();
      const bodyReference = overlayReferences().find((reference) => reference.target_origin === id);
      if (bodyReference) item.references.push({ origin: state.bodyReferenceOverlay.source.origin, label: state.bodyReferenceOverlay.kind === "link" ? "Retained Link declaration from" : "Literal body URL reference from" });
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
    function overlayReferences() {
      const overlay = state.bodyReferenceOverlay;
      return overlay && overlay.kind !== "peer" ? overlay.kind === "link" ? overlay.scan.observations : overlay.scan.references : [];
    }
    function overlayEdges() {
      const overlay = state.bodyReferenceOverlay, seen = new Set();
      return overlayReferences().filter((reference) => reference.target_origin !== overlay.source.origin).flatMap((reference) => {
        const descriptor = overlay.kind === "link" ? LINK_RELATIONS[reference.relation] : { type: "body_url_reference", label: "Literal body URL reference" };
        const key = JSON.stringify([reference.target_origin, descriptor.type]);
        if (seen.has(key)) return []; seen.add(key);
        return [{ source: overlay.source.origin, target: reference.target_origin, relationship: descriptor.type, label: descriptor.label }];
      });
    }
    function sourceIsRetained(source, snapshot = state.snapshot) {
      if (!source || source.sessionId !== getSessionId() || source.sessionId !== state.sessionId
        || typeof source.generation !== "string" || !source.generation || source.generation !== snapshot?.snapshot.generation) return false;
      const origin = snapshot.origins.find((item) => item.origin === source.origin);
      return !!origin && [...origin.endpoints, ...origin.technology_clues, ...origin.technology_observations, ...origin.redirects, ...origin.header_links]
        .some((item) => item.evidence.includes(source.evidenceId));
    }
    function clearOverlay(notice = "") {
      if (!state.bodyReferenceOverlay) return false;
      if (state.bodyReferenceOverlay.kind === "link") notice = notice.replace("body references", "Link references");
      if (state.bodyReferenceOverlay.kind === "peer") notice = notice.replace("body references", "recorded address group");
      state.bodyReferenceOverlay = null;
      if (state.peerEvidenceBinding) cancelEvidence();
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
      if (state.anchorOrigin && !findOrigin(state.anchorOrigin)) state.anchorOrigin = null;
      if (state.selectedOrigin && !findOrigin(state.selectedOrigin)) {
        cancelEvidence();
        state.selectedOrigin = null;
        state.endpointIndex = null;
        state.endpointSelection = null;
      }
      return true;
    }
    function clearBodyReferenceOverlay() {
      cancelPeerMatches();
      const cleared = clearOverlay(state.bodyReferenceOverlay?.kind === "link" ? "Temporary Link references cleared. Showing loaded metadata only." : "Temporary body references cleared. Showing loaded metadata only.");
      render();
      return cleared;
    }
    function showBodyReferencesOnMap() {
      if (state.sessionId !== getSessionId()) { reset(); return false; }
      const scan = state.bodyReferences, source = state.bodyReferencesSource;
      if (activeInspectionsBlocked() || state.relationshipKind !== "body" || !visible || !getEventStreamReady() || state.error || state.stale || !scan || !["scanned", "partial"].includes(scan.status) || !sourceIsRetained(source)
        || state.evidenceId !== source.evidenceId || state.selectedOrigin !== source.origin
        || !scan.references.some((item) => item.target_origin !== source.origin)) return false;
      clearOverlay();
      // Copy only already-validated fields, never the response row or raw body.
      // The overlay must survive an unrelated inspector selection unchanged.
      const references = Object.freeze(scan.references.map((item) => Object.freeze({
        target_origin: item.target_origin, occurrences: item.occurrences,
        preview_byte_offsets: Object.freeze([...item.preview_byte_offsets]), omitted_offsets: item.omitted_offsets,
      })));
      state.bodyReferenceOverlay = Object.freeze({ kind: "body", source: Object.freeze({ ...source }), scan: Object.freeze({
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
      if (state.bodyReferenceOverlay?.kind === "peer") return null;
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
        counts.outgoing += 1; counts[edge.relationship] = (counts[edge.relationship] || 0) + 1;
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
        cancelPeerMatches();
        clearOverlay("Temporary body references cleared by All origins.");
        state.anchorOrigin = null;
      }
      state.modeNotice = "";
      render();
    }
    function focusSelection() {
      if (state.sessionId !== getSessionId()) { reset(); return; }
      if (!state.anchorOrigin || !selectedOrigin()) return;
      if (state.anchorOrigin !== state.selectedOrigin) {
        cancelPeerMatches();
        clearOverlay("Temporary body references cleared when the pinned origin changed.");
      }
      if (!selectedOrigin()) { render(); return; }
      state.anchorOrigin = state.selectedOrigin;
      state.modeNotice = "";
      render();
    }

    function peerSnapshotKey(snapshot = state.snapshot) {
      if (!snapshot) return null;
      // Response IDs/timestamps change on every map poll. Freeze only retained
      // projection identity; event/history epochs cover edits at equal counts.
      const { id, captured_at, ...bounds } = snapshot.snapshot;
      return JSON.stringify([bounds, snapshot.origins]);
    }
    function peerSourceIsCurrent(source, snapshot = state.snapshot) {
      return !!source && source.sessionId === getSessionId() && source.sessionId === state.sessionId
        && source.generation === snapshot?.snapshot.generation && source.historyVersion === state.historyVersion
        && source.scopeOnly === state.scopeOnly && source.mapKey === peerSnapshotKey(snapshot);
    }
    function peerOverlayOrigins() {
      const overlay = state.bodyReferenceOverlay;
      if (overlay?.kind !== "peer") return [];
      const scan = overlay.scan, ids = [scan.source.origin, ...scan.origins.map((group) => group.origin)];
      return [...new Set(ids)].map((id) => {
        const group = scan.origins.find((item) => item.origin === id);
        const aggregate = state.snapshot?.origins.find((origin) => origin.origin === id);
        return { ...referenceOrigin(id), captured: true, peerInspectionOnly: !aggregate,
          technology_observations: aggregate?.technology_observations || [], technology_observations_present: aggregate?.technology_observations_present === true, in_scope: id === scan.source.origin ? scan.source.in_scope : group.in_scope,
          peerGroup: group || null, peerSource: id === scan.source.origin, request_count: 0 };
      });
    }
    function peerSampleBinding(id, origin) {
      const overlay = state.bodyReferenceOverlay;
      if (overlay?.kind !== "peer" || !peerSourceIsCurrent(overlay.source) || state.peerUnavailableTokens?.has(id)) return null;
      const scan = overlay.scan;
      const sample = scan.source.evidence_id === id && scan.source.origin === origin ? scan.source
        : scan.origins.find((group) => group.origin === origin)?.samples.find((item) => item.evidence_id === id);
      return sample ? Object.freeze({ overlay, snapshotId: scan.snapshot.id, seedId: scan.evidence_id, origin, sample }) : null;
    }
    function peerBindingIsCurrent(binding) {
      if (!binding || binding.overlay !== state.bodyReferenceOverlay || binding.overlay?.kind !== "peer"
        || !visible || !getEventStreamReady() || state.error || state.stale || !peerSourceIsCurrent(binding.overlay.source)
        || binding.snapshotId !== binding.overlay.scan.snapshot.id || binding.seedId !== binding.overlay.scan.evidence_id) return false;
      return peerSampleBinding(binding.sample.evidence_id, binding.origin)?.sample === binding.sample;
    }
    function handlePeerSampleUnavailable(binding) {
      if (state.bodyReferenceOverlay !== binding.overlay) return;
      if (binding.sample.evidence_id === binding.seedId) {
        clearOverlay("Temporary recorded address group cleared: the exact inspected source is unavailable or changed.");
        cancelPeerMatches();
      } else {
        if (!state.peerUnavailableTokens) state.peerUnavailableTokens = new Set();
        state.peerUnavailableTokens.add(binding.sample.evidence_id);
        state.peerEvidenceBinding = null;
        state.evidenceError = "This matched sample is unavailable or changed. The shown counts remain a point-in-time inspection; other exact samples can still be checked. Inspect again to obtain new samples.";
      }
    }
    function cancelPeerMatches() {
      state.peerMatchesRequestId += 1;
      peerMatchesAbort?.abort();
      peerMatchesAbort = null;
      state.peerMatches = null;
      state.peerMatchesSource = null;
      state.peerMatchesLoading = false;
      state.peerMatchesError = "";
    }
    async function loadPeerMatches() {
      const row = state.evidence, peer = state.socketPeer, sessionId = getSessionId(), generation = state.snapshot?.snapshot.generation;
      if (activeInspectionsBlocked() || !visible || state.view !== "service-map" || !getEventStreamReady() || state.error || state.stale || state.sessionId !== sessionId
        || !row || row.evidence_id !== state.evidenceId || state.evidenceLoading || state.peerMatchesLoading || !usableSocketPeer(peer)
        || !validTechnologyEvidence(row.evidence_id, sessionId, generation) || row.sequence !== row.evidence_id.split(".")[3]
        || row.record_id !== row.evidence_id.split(".")[5] || safeOrigin(row.origin) !== row.origin || observationTime(row.started_at) === null
        || !(state.peerEvidenceBinding ? peerBindingIsCurrent(state.peerEvidenceBinding) : evidenceItems(selectedOrigin()).some((item) => item.id === row.evidence_id))) return false;
      const source = Object.freeze({ sessionId, generation, evidenceId: row.evidence_id, origin: row.origin, sequence: row.sequence,
        method: row.method, path: safePath(row.path), startedAt: row.started_at, observedAt: peer.observedAt,
        historyVersion: state.historyVersion, scopeOnly: state.scopeOnly, mapKey: peerSnapshotKey() });
      cancelPeerMatches();
      const requestId = ++state.peerMatchesRequestId, selected = state.selectedOrigin;
      const current = () => requestId === state.peerMatchesRequestId && visible && state.view === "service-map" && getEventStreamReady()
        && !state.error && !state.stale && row === state.evidence && peer === state.socketPeer && state.evidenceId === source.evidenceId
        && selected === state.selectedOrigin && peerSourceIsCurrent(source);
      state.peerMatchesSource = source;
      state.peerMatchesLoading = true;
      peerMatchesAbort = new AbortController();
      renderDetail();
      let unsupportedServer = false;
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: source.evidenceId, in_scope_only: String(source.scopeOnly), max_records: String(PEER_MATCH_LIMITS.max_records) });
        const response = await request(`/api/target/service-map/socket-peer-matches?${params}`, { signal: peerMatchesAbort.signal });
        unsupportedServer = response.status === 404;
        if (!response.ok) throw new Error("Recorded peer matches unavailable");
        const payload = await response.json();
        if (!current()) return false;
        state.peerMatches = normalizePeerMatches(payload, sessionId, source.evidenceId, row, peer, source.scopeOnly, generation);
        state.peerMatchesLoading = false;
        peerMatchesAbort = null;
        if (!handleSupplementLifetime(state.peerMatches, source)) renderDetail();
        return true;
      } catch (_) {
        if (!current()) return false;
        state.peerMatchesLoading = false;
        peerMatchesAbort = null;
        state.peerMatches = null;
        state.peerMatchesError = unsupportedServer ? "This server does not support matching recorded socket peers. No matching inspection was performed."
          : "Could not verify matching recorded socket peers. Retry explicitly, or refresh the map and select the exact source again.";
        renderDetail();
        return false;
      }
    }
    function showPeerMatchesOnMap() {
      const scan = state.peerMatches, source = state.peerMatchesSource;
      if (activeInspectionsBlocked() || !visible || state.view !== "service-map" || !getEventStreamReady() || state.error || state.stale || !scan?.available
        || !peerSourceIsCurrent(source) || state.evidenceId !== source.evidenceId || state.selectedOrigin !== source.origin) return false;
      // Inspection and visualization are independent. Replacing the shown view
      // uses only this frozen response and never performs another read.
      clearOverlay();
      state.bodyReferenceOverlay = Object.freeze({ kind: "peer", source, scan });
      state.peerUnavailableTokens = new Set();
      state.selectedOrigin = source.origin;
      state.anchorOrigin = source.origin;
      state.endpointIndex = null;
      state.endpointSelection = null;
      state.bodyOverlayNotice = "";
      state.modeNotice = "";
      state.layoutSlots = new Map();
      render();
      return true;
    }
    function peerCountsMarkup(scan) {
      if (!scan?.available) return "";
      const counts = scan.counts;
      return `<p>${scan.complete ? "Complete considered-window equality inspection" : "Partial retained-history inspection"} · ${scan.snapshot.considered_rows.toLocaleString()} rows considered / ${scan.snapshot.retained_rows.toLocaleString()} retained · ${counts.matching_rows.toLocaleString()} other matching rows</p><p>${scan.origins.length} returned origin groups · ${counts.returned_matching_rows.toLocaleString()} represented matching rows · ${counts.omitted_matching_rows.toLocaleString()} matching rows omitted by result caps · ${counts.sample_rows} exact samples · ${counts.omitted_sample_rows} samples omitted within returned groups.</p><p>Candidate sequences ${escape(scan.snapshot.oldest_considered_sequence)}–${escape(scan.snapshot.latest_considered_sequence)}. Source ${scan.source.in_window ? "is inside" : "is outside"} this window and is counted separately.</p>${scan.reasons.length ? `<p>${scan.reasons.map((reason) => escape(PEER_MATCH_REASONS[reason])).join(" · ")}</p>` : ""}`;
    }
    function peerMatchesMarkup() {
      const scan = state.peerMatches, source = state.peerMatchesSource;
      const ready = usableSocketPeer(state.socketPeer);
      const disabled = activeInspectionsBlocked() || !ready || !visible || !getEventStreamReady() || !!state.error || state.stale || state.evidenceLoading || state.peerMatchesLoading;
      const showDisabled = !scan?.available || !peerSourceIsCurrent(source) || source.evidenceId !== state.evidenceId || disabled;
      return `<section class="service-map-body-references service-map-peer-matches" aria-label="Matching recorded socket peers" aria-busy="${state.peerMatchesLoading}"><h4>Matching recorded socket peers</h4><p class="service-map-muted">Explicit inspection of retained direct-route observations with equal source, address family, IP and port. No new capture, DNS, connection or remote request.</p><button class="secondary-action" type="button" data-service-peer-matches="inspect" data-service-focus="peer:inspect" ${disabled ? "disabled" : ""}>${state.peerMatchesLoading ? "Finding matching recorded peers…" : "Find matching recorded peer"}</button>${!ready ? '<p class="service-map-muted">Only a usable, recorded direct-route address can seed this inspection. Proxy, unknown, unavailable and excluded address observations remain diagnostics only.</p>' : ""}${state.peerMatchesError ? `<p class="service-map-error" role="status">${escape(state.peerMatchesError)}</p>` : ""}${scan ? `<div class="service-map-body-reference-result" role="status">${scan.available ? `<p>${scan.status === "no_matches" ? "No other matching rows in this considered, filtered window." : "Matching retained observations found."}</p>${peerCountsMarkup(scan)}` : `<p>Inspection unavailable. ${scan.reasons.map((reason) => escape(PEER_MATCH_REASONS[reason])).join(" · ")}</p>`}</div>` : ""}${scan?.available && scan.origins.length ? `<ul class="service-map-body-reference-list">${scan.origins.map((group) => `<li><span class="service-map-body-reference-origin">${escape(group.origin)}</span><span>${group.matching_rows} matching rows · ${group.samples.length} exact samples</span><span>Recorded first: ${escape(group.first_observed_at)} · last: ${escape(group.last_observed_at)}</span></li>`).join("")}</ul>` : ""}<button class="secondary-action" type="button" data-service-show-peer-matches="show" data-service-focus="peer:show" ${showDisabled ? "disabled" : ""}>Show recorded address group</button><p class="service-map-muted">Show replaces the one temporary visualization using this response, with no additional read. Observation timestamps are discrete recorded facts, not connection counts or continuous endpoint occupancy.</p><p class="service-map-muted">${PEER_PRIVACY}</p>${scan?.available ? `<details class="service-map-body-reference-limits"><summary data-service-focus="peer:limits">Coverage and omissions</summary><p>Other considered rows: ${scan.counts.non_http_rows} non-HTTP · ${scan.counts.not_recorded_rows} not recorded · ${scan.counts.unavailable_observation_rows} unavailable · ${scan.counts.non_direct_rows} non-direct · ${scan.counts.invalid_observation_rows} unusable observations · ${scan.counts.different_peer_rows} different peer · ${scan.counts.invalid_metadata_rows} invalid origin metadata · ${scan.counts.scope_excluded_rows} scope excluded.</p><p>Limits: ${PEER_MATCH_LIMITS.max_records.toLocaleString()} newest retained rows · 100 origin groups · 3 samples per origin · 256 KiB result. Unreturned distinct origins are not counted. Exact samples can change after this point-in-time inspection and are verified again when opened.</p></details>` : ""}</section>`;
    }
    function peerOriginMarkup(origin) {
      const overlay = state.bodyReferenceOverlay;
      if (overlay?.kind !== "peer" || !origin.peerSource && !origin.peerGroup) return "";
      const samples = [...(origin.peerSource ? [overlay.scan.source] : []), ...(origin.peerGroup?.samples || [])];
      return `<section class="service-map-peer-origin"><h4>${origin.peerInspectionOnly ? "Captured in this peer inspection" : "Recorded address observations"}</h4><p class="service-map-muted">${origin.peerInspectionOnly ? "This origin is outside the default metadata projection. " : ""}${origin.peerSource ? "The source observation is counted separately. " : ""}${origin.peerGroup?.matching_rows || 0} other matching rows in this inspection; these are not full-session traffic totals.</p>${origin.peerGroup ? `<p class="service-map-muted">First recorded: ${escape(origin.peerGroup.first_observed_at)} · Last recorded: ${escape(origin.peerGroup.last_observed_at)} · ${origin.peerGroup.omitted_samples} exact samples omitted. These times do not imply continuous occupancy.</p>` : ""}<div class="service-map-evidence-list">${samples.map((sample) => `<button class="service-map-evidence-button" type="button" data-service-peer-origin="${escape(origin.origin)}" data-service-peer-evidence="${escape(sample.evidence_id)}" data-service-focus="peer-sample:${escape(sample.evidence_id)}" ${state.peerUnavailableTokens?.has(sample.evidence_id) ? "disabled" : ""}><span>${sample.evidence_id === overlay.scan.evidence_id ? "Inspected source" : "Matched sample"} · sequence ${escape(sample.sequence)}<br>Recorded ${escape(sample.observed_at)}</span><span>${state.peerUnavailableTokens?.has(sample.evidence_id) ? "Unavailable" : "Verify source ›"}</span></button>`).join("") || '<p class="service-map-muted">No exact sample was returned for this origin. Its represented matching-row count remains available.</p>'}</div></section>`;
    }
    function renderPeerGraph() {
      const overlay = state.bodyReferenceOverlay, scan = overlay.scan, query = state.query.trim().toLowerCase();
      const allOrigins = peerOverlayOrigins(), source = allOrigins[0];
      const stages = originFilterStages(allOrigins.slice(1));
      const matching = [source, ...stages.matching];
      const textSize = doc?.documentElement && typeof root.getComputedStyle === "function" ? Number.parseFloat(root.getComputedStyle(doc.documentElement).getPropertyValue("--ui-root-size")) : 12;
      const layout = peerGraphLayout(matching, source.origin, graphGeometry(textSize));
      currentLayout = layout;
      applyGraphScale();
      els.boundsDetails?.classList.toggle("hidden", false);
      els.mapControls?.classList.toggle("hidden", false);
      els.allOrigins?.setAttribute("aria-pressed", "false");
      els.directRelations?.setAttribute("aria-pressed", "true");
      if (els.directRelations) els.directRelations.disabled = true;
      if (els.focusSelection) {
        els.focusSelection.classList.toggle("hidden", false);
        els.focusSelection.disabled = !selectedOrigin() || state.selectedOrigin === source.origin;
        els.focusSelection.title = "Clear this group and pin the selected origin in Direct relations";
      }
      const times = [scan.source.observed_at, ...scan.origins.flatMap((group) => [group.first_observed_at, group.last_observed_at])].sort((a, b) => observationTime(a) < observationTime(b) ? -1 : observationTime(a) > observationTime(b) ? 1 : 0);
      const address = `${scan.peer.address_family === "ipv6" ? `[${scan.peer.address.ip}]` : scan.peer.address.ip}:${scan.peer.address.port}`;
      const edges = layout.edges.map((edge) => `<path class="service-map-edge peer-observation ${edge.source === state.selectedOrigin ? "selected" : ""}" data-relationship="recorded_peer_observation" d="${edge.path}"><title>${escape(edge.source)} · Recorded direct address observation · ${escape(address)} · undirected</title></path>`).join("");
      const compactTime = (value) => value.replace("T", " ").replace(/\.\d+/, "").replace(/Z$/, " UTC");
      const cards = layout.nodes.map((item) => {
        if (item.endpointNode) return `<div class="service-map-card service-map-peer-address" role="group" aria-label="Recorded address match, direct route, ${escape(address)}" style="left:${item.x}px;top:${item.y}px"><span class="service-map-card-symbol" aria-hidden="true">◇</span><span class="service-map-card-copy"><strong title="${escape(address)}">${escape(address)}</strong><span class="service-map-card-scheme">Recorded address · direct · ${scan.peer.address_family.toUpperCase()}</span><span class="service-map-card-meta" title="${escape(times[0])}">First: ${escape(compactTime(times[0]))}</span><span class="service-map-card-meta" title="${escape(times.at(-1))}">Last: ${escape(compactTime(times.at(-1)))}</span></span></div>`;
        const selected = item.origin === state.selectedOrigin, pinned = item.origin === source.origin;
        const inDefault = state.snapshot.origins.some((origin) => origin.origin === item.origin);
        return `<button class="service-map-card peer-captured ${selected ? "selected" : ""} ${pinned ? "pinned" : ""}" type="button" data-service-origin="${escape(item.origin)}" data-service-focus="origin:${escape(item.origin)}" aria-current="${selected}" style="left:${item.x}px;top:${item.y}px"><span class="service-map-card-symbol" aria-hidden="true">${pinned ? "⊙" : "◎"}</span><span class="service-map-card-copy"><strong title="${escape(item.origin)}">${escape(new URL(item.origin).host)}</strong><span class="service-map-card-scheme" title="${inDefault ? "Also in the default metadata projection; products use this exact origin’s default aggregate" : "Captured in this peer inspection; default product metadata unavailable"}">${pinned ? "Pinned inspected source" : inDefault ? "Also in default projection" : "Captured in peer inspection"}</span>${productSummaryMarkup(item)}<span class="service-map-card-meta">${pinned ? "Source + " : ""}${item.peerGroup?.matching_rows || 0} ${pinned ? "other " : ""}matching rows</span></span><span class="service-map-scope-dot ${item.in_scope ? "in-scope" : ""}" title="${item.in_scope ? "In scope" : "Out of scope"}"></span></button>`;
      }).join("");
      replace(els.canvas, `<div class="service-map-scaled"><div class="service-map-layout" style="--service-map-card-width:${layout.geometry.cardWidth}px;--service-map-card-height:${layout.geometry.cardHeight}px;--service-map-line-height:${layout.geometry.lineHeight};width:${layout.width}px;height:${layout.height}px"><svg class="service-map-edges" width="${layout.width}" height="${layout.height}" aria-hidden="true">${edges}</svg>${cards}</div></div>`);
      const shown = new Set(layout.nodes.slice(1).map((item) => item.origin));
      const queryHidden = stages.queryHidden, productHidden = stages.productHidden, capHidden = matching.length - shown.size;
      const productRows = stages.searched.filter((item) => !stages.matching.includes(item)).reduce((sum, item) => sum + (item.peerGroup?.matching_rows || 0), 0);
      const queryRows = stages.loaded.filter((item) => !stages.searched.includes(item)).reduce((sum, item) => sum + (item.peerGroup?.matching_rows || 0), 0);
      const capRows = matching.filter((item) => !shown.has(item.origin)).reduce((sum, item) => sum + (item.peerGroup?.matching_rows || 0), 0);
      const shownRows = stages.matching.filter((item) => shown.has(item.origin)).reduce((sum, item) => sum + (item.peerGroup?.matching_rows || 0), 0);
      els.graphBounds.textContent = [
        `${shown.size} / ${allOrigins.length} returned origins shown + 1 recorded address node`,
        `${layout.edges.length} undirected observation spokes · ${shownRows} peer-match rows in shown matching origins`,
        source.peerGroup?.matching_rows ? `${source.peerGroup.matching_rows} peer-match rows in pinned context (separate)` : "",
        queryHidden ? `${queryHidden} origins / ${queryRows} rows hidden by query` : "",
        productHidden ? `${productHidden} origins / ${productRows} rows hidden by products/versions` : "",
        state.productId || state.productVersion ? `${stages.matching.length} matching origins (source excluded)` : "",
        capHidden ? `${capHidden} origins / ${capRows} rows omitted by 24-node display cap` : "",
        query && !source.origin.toLowerCase().includes(query) || !productMatches(source, state.productId, state.productVersion) ? "Pinned source remains visible outside filters (context only)" : "",
        state.selectedOrigin && !shown.has(state.selectedOrigin) ? "Selected origin is outside this display; inspector retained" : "",
      ].filter(Boolean).join(" · ");
      if (els.graphDiagnostics) els.graphDiagnostics.textContent = `Recorded address group only. ${stages.loaded.length} other loaded returned origins → ${queryHidden} query-hidden → ${productHidden} product/version-hidden → ${stages.matching.length} matching → ${capHidden} display-cap omitted. Pinned source is context, excluded from matching origins; its ${source.peerGroup?.matching_rows || 0} other matching rows are counted separately from filtered origin matches. Products use only an exact origin’s loaded default aggregate, not the peer source or samples. Source and address node are reserved within the 24-node cap (at most 23 origins); inclusive edge cap 40. ${scan.counts.omitted_matching_rows} matching rows omitted by API result caps; unreturned distinct origins are not counted. ${scan.complete ? "Complete" : "Partial"} inspection, ${scan.snapshot.considered_rows} / ${scan.snapshot.retained_rows} retained rows considered. Observation times describe recorded facts, not connections or continuous occupancy. ${PEER_PRIVACY}`;
    }

    function cancelBodyReferences() {
      state.bodyReferencesRequestId += 1;
      bodyReferencesAbort?.abort();
      bodyReferencesAbort = null;
      state.bodyReferences = null;
      state.bodyReferencesSource = null;
      state.linkDeclarations = null;
      state.linkDeclarationsSource = null;
      state.linkDeclarationsError = "";
      state.bodyReferencesLoading = false;
      state.bodyReferencesError = "";
    }
    function cancelHtmlGenerators() {
      state.htmlGeneratorsRequestId += 1;
      htmlGeneratorsAbort?.abort();
      htmlGeneratorsAbort = null;
      state.htmlGenerators = null;
      state.htmlGeneratorsSource = null;
      state.htmlGeneratorsLoading = false;
      state.htmlGeneratorsError = "";
    }
    function activeInspectionsBlocked() {
      return state.inactiveSessionId !== null && state.inactiveSessionId === getSessionId();
    }
    function handleSupplementLifetime(result, source) {
      // Called only after the request's identity fence and typed normalizer.
      // Eligibility/preview/parser failures cannot disprove ordinary metadata.
      if (result.status !== "unavailable") return false;
      const reason = result.reasons[0];
      const message = SOURCE_LIFETIME_MESSAGES[reason];
      if (message) {
        const overlay = state.bodyReferenceOverlay;
        const sameSession = overlay?.source.sessionId === source.sessionId;
        const affectedOverlay = sameSession && (reason === "session_source_changed"
          || reason === "stale_generation" && overlay.source.generation === source.generation
          || overlay.source.evidenceId === source.evidenceId);
        if (affectedOverlay) clearOverlay(`Temporary visualization cleared. ${message}`);
        cancelEvidence();
        state.evidenceError = `Source unavailable. ${message}`;
        render();
        return true;
      }
      if (reason !== "inactive_session") return false;
      state.inactiveSessionId = source.sessionId;
      // A peer overlay can also contain an independently allowlisted ordinary
      // row. Detach that binding before clearing, never restore orphan samples.
      const ordinaryOrigin = state.snapshot?.origins.find((origin) => origin.origin === state.selectedOrigin);
      if (state.evidence && evidenceItems(ordinaryOrigin).some((item) => item.id === state.evidence.evidence_id)) state.peerEvidenceBinding = null;
      clearOverlay(INACTIVE_SESSION_NOTICE);
      cancelPeerMatches();
      cancelBodyReferences();
      cancelHtmlGenerators();
      render();
      return true;
    }
    function cancelEvidence() {
      cancelPeerMatches();
      state.peerEvidenceBinding = null;
      cancelHtmlGenerators();
      cancelBodyReferences();
      state.evidenceRequestId += 1;
      evidenceAbort?.abort();
      evidenceAbort = null;
      state.evidence = null;
      state.evidenceCatalogVersion = null;
      state.evidenceLegacyCatalog = false;
      state.socketPeer = null;
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
        inactiveSessionId: null, selectionInitialized: false, endpointIndex: null, endpointSelection: null, endpointMethod: "", endpointPath: "", productId: "", productVersion: "", pageAnchor: null, detailPageAnchor: null, loading: false, error: "", refreshPaused: false, page: 0, detailPage: 0, stale: false, anchorOrigin: null, modeNotice: "", zoomScale: 1 });
      render();
    }
    function selectOrigin(origin, endpointIndex = null) {
      const item = findOrigin(origin);
      if (!item || endpointIndex !== null && (!Number.isSafeInteger(endpointIndex) || !item.endpoints[endpointIndex])) return;
      const changedOrigin = state.selectedOrigin !== origin;
      if (changedOrigin || state.endpointIndex !== endpointIndex) cancelEvidence();
      if (changedOrigin) { state.detailPage = 0; state.detailPageAnchor = null; }
      state.selectedOrigin = origin;
      state.selectionInitialized = true;
      state.endpointIndex = endpointIndex;
      const endpoint = endpointIndex === null ? null : item.endpoints[endpointIndex];
      // Keep only the exact identity when invalidation clears all displayed data.
      // An array index cannot safely survive scope filtering or history removal.
      state.endpointSelection = endpoint ? { method: endpoint.method, path: endpoint.path } : null;
      if (endpoint) {
        const entry = { origin: item, endpoint, index: endpointIndex };
        const index = loadedEndpointEntries([item]).findIndex((candidate) => sameEndpoint(candidate, endpointIdentity(entry)));
        if (index >= 0) { state.detailPage = Math.floor(index / API_PAGE_SIZE); state.detailPageAnchor = endpointIdentity(entry); }
      }
      state.detailTab = endpointIndex === null ? "apis" : "evidence";
      render();
    }
    async function refresh({ force = false, retry = false } = {}) {
      if (!visible) return false;
      const sessionId = getSessionId();
      if (state.sessionId !== sessionId) reset();
      if (!sessionId || state.refreshPaused && !retry || state.loading && !force) return false;
      // The app polls every 1.2 seconds. A same-source background snapshot must
      // not erase an explicit inspection or continually abort its pending read.
      if (force) {
        clearOverlay("Temporary body references cleared by Refresh. Inspect and show references again when ready.");
        cancelBodyReferences();
        cancelHtmlGenerators();
        cancelPeerMatches();
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
      let failure = null;
      try {
        const params = new URLSearchParams({ session_id: sessionId, in_scope_only: String(scopeOnly), max_records: "10000" });
        const response = await request(`/api/target/service-map?${params}`, { signal: abort.signal, headers: { [TECHNOLOGY_CATALOG_HEADER]: String(TECHNOLOGY_CATALOG_VERSION) } });
        if (!response.ok) {
          failure = mapFailure(response);
          throw new Error("Service map unavailable");
        }
        const snapshot = normalizeSnapshot(await response.json(), sessionId);
        if (requestId !== state.requestId || sessionId !== getSessionId() || scopeOnly !== state.scopeOnly) return false;
        if (state.bodyReferenceOverlay && !(state.bodyReferenceOverlay.kind === "peer" ? peerSourceIsCurrent(state.bodyReferenceOverlay.source, snapshot) : state.bodyReferenceOverlay.kind === "link" ? htmlGeneratorSourceIsRetained(state.bodyReferenceOverlay.source, snapshot) : sourceIsRetained(state.bodyReferenceOverlay.source, snapshot))) {
          clearOverlay("Temporary body references cleared: the exact source is absent or retained history changed.");
        }
        if (state.snapshot && (state.snapshot.snapshot.generation !== snapshot.snapshot.generation
          || state.snapshot.technologyCatalogVersion !== snapshot.technologyCatalogVersion)) cancelEvidence();
        if (state.peerMatchesSource && !peerSourceIsCurrent(state.peerMatchesSource, snapshot)) cancelPeerMatches();
        if (state.linkDeclarationsSource && !htmlGeneratorSourceIsRetained(state.linkDeclarationsSource, snapshot)) cancelBodyReferences();
        if (state.htmlGeneratorsSource && !htmlGeneratorSourceIsRetained(state.htmlGeneratorsSource, snapshot)) cancelHtmlGenerators();
        state.snapshot = snapshot;
        state.refreshPaused = false;
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
        if (state.evidenceId && !(state.peerEvidenceBinding ? peerBindingIsCurrent(state.peerEvidenceBinding) : evidenceItems(origin).some((item) => item.id === state.evidenceId))) cancelEvidence();
        reconcileEndpointPages();
        state.loading = false;
        render();
        return true;
      } catch (_) {
        if (requestId !== state.requestId || sessionId !== getSessionId() || scopeOnly !== state.scopeOnly) return false;
        clearOverlay("Temporary body references cleared because the current refresh failed.");
        cancelBodyReferences();
        cancelHtmlGenerators();
        state.loading = false;
        cancelPeerMatches();
        if (failure?.status === 409) cancelEvidence();
        // Polling, visibility and event reconnects must not turn a bounded
        // failure into an automatic retry loop, including with older servers.
        state.refreshPaused = true;
        state.stale = true;
        state.error = failure?.message || (state.snapshot ? "Refresh failed." : "Could not load the service map.");
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
        origin.technology_observations.forEach((item) => add(item.evidence, `${item.name} · ${technologyVersion(item)}`));
        origin.redirects.forEach((item) => add(item.evidence, `Redirect instruction → ${item.target_origin}`));
        origin.header_links.forEach((item) => add(item.evidence, `${item.relationship === "origin_header" ? "Origin" : "Referer"} header: ${item.source_origin}`));
      }
      return items;
    }
    async function loadEvidence(id) {
      // Preserve the ordinary evidence guard. When a default-map token is also
      // in the shown peer response, additionally revalidate its observation.
      if (!evidenceItems(selectedOrigin()).some((item) => item.id === id)) return false;
      return loadEvidenceInternal(id, peerSampleBinding(id, state.selectedOrigin));
    }
    async function loadPeerEvidence(id, originId) {
      const binding = peerSampleBinding(id, originId);
      if (!binding) return false;
      selectOrigin(originId);
      if (state.selectedOrigin !== originId) return false;
      return loadEvidenceInternal(id, binding);
    }
    async function loadEvidenceInternal(id, peerBinding) {
      const origin = selectedOrigin();
      if (peerBinding ? !peerBindingIsCurrent(peerBinding) || peerBinding.sample.evidence_id !== id || peerBinding.origin !== origin?.origin
        : !evidenceItems(origin).some((item) => item.id === id)) return false;
      cancelEvidence();
      state.peerEvidenceBinding = peerBinding;
      const requestId = ++state.evidenceRequestId;
      const sessionId = getSessionId();
      const selected = state.selectedOrigin;
      const overlaySource = () => state.bodyReferenceOverlay?.kind !== "peer" && state.bodyReferenceOverlay?.source.evidenceId === id
        && state.bodyReferenceOverlay.source.origin === selected ? state.bodyReferenceOverlay.source : null;
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      state.evidenceId = id;
      state.evidenceLoading = true;
      state.detailTab = "evidence";
      evidenceAbort = new AbortController();
      renderDetail();
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: id, include_socket_peer: "true" });
        const response = await request(`/api/target/service-map/evidence?${params}`, { signal: evidenceAbort.signal, headers: { [TECHNOLOGY_CATALOG_HEADER]: String(TECHNOLOGY_CATALOG_VERSION) } });
        if (!response.ok) throw new Error("Evidence unavailable");
        const result = await response.json();
        if (requestId !== state.evidenceRequestId || sessionId !== getSessionId() || selected !== state.selectedOrigin) return false;
        if (result.schema_version !== 1 || result.session_id !== sessionId || result.evidence_id !== id
          || result.source !== "captured_http_metadata" || typeof result.available !== "boolean") throw new Error("Evidence source changed");
        const catalogVersion = technologyCatalogVersion(result);
        const row = result.row;
        if (result.available && (!row || row.evidence_id !== id || safeOrigin(row.origin) !== selected
          || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(row.record_id || "") || typeof row.sequence !== "string" || !/^[0-9]+$/.test(row.sequence)
          || typeof row.method !== "string" || typeof row.path !== "string"
          || endpoint && (row.method !== endpoint.method || safePath(row.path) !== endpoint.path))) throw new Error("Evidence row changed");
        const normalizedPeer = result.available ? normalizeSocketPeer(result.socket_peer) : null;
        if (peerBinding && (!peerBindingIsCurrent(peerBinding) || result.available && (row.origin !== peerBinding.origin
          || row.sequence !== peerBinding.sample.sequence || row.sequence !== id.split(".")[3] || row.record_id !== id.split(".")[5]
          || row.started_at !== peerBinding.sample.started_at || normalizedPeer?.observedAt !== peerBinding.sample.observed_at
          || !peerEquals(peerBinding.overlay.scan.peer, normalizedPeer)))) throw new Error("Recorded peer source changed");
        const source = overlaySource();
        if (result.available && source && (row.method !== source.method || safePath(row.path) !== source.path
          || row.sequence !== source.sequence || row.started_at !== source.startedAt)) throw new Error("Overlay source changed");
        state.evidence = result.available === true ? normalizeEvidenceObservations(row, catalogVersion) : null;
        state.evidenceCatalogVersion = state.evidence ? catalogVersion : null;
        state.evidenceLegacyCatalog = !!state.evidence && catalogVersion !== TECHNOLOGY_CATALOG_VERSION;
        state.socketPeer = state.evidence ? normalizedPeer : null;
        state.evidenceError = state.evidence ? "" : "Source unavailable. The retained row may have changed or been removed. Refresh the map to select current evidence.";
        state.evidenceLoading = false;
        if (!state.evidence && peerBinding) {
          handlePeerSampleUnavailable(peerBinding);
          render();
        } else if (!state.evidence && source) {
          clearOverlay("Temporary body references cleared: the exact inspected source is no longer available.");
          render();
        } else renderDetail();
        return !!state.evidence;
      } catch (_) {
        if (requestId !== state.evidenceRequestId || sessionId !== getSessionId() || selected !== state.selectedOrigin) return false;
        state.evidence = null;
        state.evidenceCatalogVersion = null;
        state.evidenceLegacyCatalog = false;
        state.socketPeer = null;
        cancelBodyReferences();
        cancelHtmlGenerators();
        state.evidenceLoading = false;
        state.evidenceError = "Source unavailable. The exact retained row could not be verified. Refresh the map and try again.";
        if (peerBinding) {
          handlePeerSampleUnavailable(peerBinding);
          render();
        } else if (overlaySource()) {
          clearOverlay("Temporary body references cleared: the exact inspected source could not be verified.");
          render();
        } else renderDetail();
        return false;
      }
    }
    async function loadBodyReferences() {
      const row = state.evidence;
      const sessionId = getSessionId();
      if (activeInspectionsBlocked() || state.relationshipKind !== "body" || !visible || state.sessionId !== sessionId || !row || state.evidenceId !== row.evidence_id
        || state.evidenceLoading || state.bodyReferencesLoading
        || !evidenceItems(selectedOrigin()).some((item) => item.id === row.evidence_id)) return false;
      cancelBodyReferences();
      const requestId = ++state.bodyReferencesRequestId;
      const evidenceId = row.evidence_id;
      const selected = state.selectedOrigin;
      const generation = state.snapshot?.snapshot.generation;
      const current = () => requestId === state.bodyReferencesRequestId && state.relationshipKind === "body" && visible && sessionId === getSessionId()
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
        if (!handleSupplementLifetime(state.bodyReferences, state.bodyReferencesSource)) renderDetail();
        return true;
      } catch (_) {
        if (!current()) return false;
        state.bodyReferencesLoading = false;
        bodyReferencesAbort = null;
        state.bodyReferencesError = "Could not verify retained body references. Retry explicitly, or refresh the map and select the source again.";
        renderDetail();
        return false;
      }
    }
    function setRelationshipKind(kind) {
      if (!["body", "link"].includes(kind) || kind === state.relationshipKind) return false;
      cancelBodyReferences();
      state.relationshipKind = kind;
      renderDetail();
      return true;
    }
    async function loadLinkDeclarations() {
      const row = state.evidence, sessionId = getSessionId(), generation = state.snapshot?.snapshot.generation;
      if (activeInspectionsBlocked() || state.relationshipKind !== "link" || !visible || !getEventStreamReady() || state.error || state.stale
        || state.sessionId !== sessionId || !row || state.evidenceId !== row.evidence_id || state.evidenceLoading || state.bodyReferencesLoading
        || !validHtmlGeneratorEvidence(row.evidence_id, sessionId, generation) || row.evidence_id.split(".")[3] !== row.sequence
        || row.evidence_id.split(".")[5] !== row.record_id || safeOrigin(row.origin) !== row.origin || observationTime(row.started_at) === null
        || !evidenceItems(selectedOrigin()).some((item) => item.id === row.evidence_id)) return false;
      const source = Object.freeze({ sessionId, generation, evidenceId: row.evidence_id, origin: row.origin,
        method: row.method, path: safePath(row.path), sequence: row.sequence, startedAt: row.started_at });
      if (!htmlGeneratorSourceIsRetained(source)) return false;
      cancelBodyReferences();
      const requestId = ++state.bodyReferencesRequestId, selected = state.selectedOrigin;
      const current = () => requestId === state.bodyReferencesRequestId && state.relationshipKind === "link" && visible
        && getEventStreamReady() && !state.error && !state.stale && sessionId === getSessionId() && sessionId === state.sessionId
        && selected === state.selectedOrigin && row === state.evidence && source.evidenceId === state.evidenceId && htmlGeneratorSourceIsRetained(source);
      state.linkDeclarationsSource = source;
      state.bodyReferencesLoading = true;
      bodyReferencesAbort = new AbortController();
      renderDetail();
      let unsupportedServer = false;
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: source.evidenceId });
        const response = await request(`/api/target/service-map/link-declarations?${params}`, { signal: bodyReferencesAbort.signal });
        unsupportedServer = response.status === 404;
        if (!response.ok) throw new Error("Retained Link headers unavailable");
        const payload = await response.json();
        if (!current()) return false;
        state.linkDeclarations = normalizeLinkDeclarations(payload, sessionId, source.evidenceId, row);
        state.bodyReferencesLoading = false;
        bodyReferencesAbort = null;
        if (!handleSupplementLifetime(state.linkDeclarations, source)) renderDetail();
        return true;
      } catch (_) {
        if (!current()) return false;
        state.bodyReferencesLoading = false;
        bodyReferencesAbort = null;
        state.linkDeclarations = null;
        state.linkDeclarationsError = unsupportedServer
          ? "This server does not support retained Link header inspection. No Link declarations were inspected."
          : "Could not verify retained Link headers. Retry explicitly, or refresh the map and select the source again.";
        renderDetail();
        return false;
      }
    }
    function showLinkDeclarationsOnMap() {
      if (state.sessionId !== getSessionId()) { reset(); return false; }
      const scan = state.linkDeclarations, source = state.linkDeclarationsSource;
      if (activeInspectionsBlocked() || state.relationshipKind !== "link" || !visible || !getEventStreamReady() || state.error || state.stale
        || !scan || scan.status !== "scanned" || !htmlGeneratorSourceIsRetained(source)
        || state.evidenceId !== source.evidenceId || state.selectedOrigin !== source.origin
        || !scan.observations.some((item) => item.target_origin !== source.origin)) return false;
      const observations = Object.freeze(scan.observations.map((item) => Object.freeze({ ...item,
        link_value_span: Object.freeze({ ...item.link_value_span }), target_span: Object.freeze({ ...item.target_span }), relation_span: Object.freeze({ ...item.relation_span }) })));
      clearOverlay();
      state.bodyReferenceOverlay = Object.freeze({ kind: "link", source: Object.freeze({ ...source }), scan: Object.freeze({
        status: scan.status, complete: scan.complete, reasons: Object.freeze([...scan.reasons]), observations,
        counts: Object.freeze({ ...scan.counts }), retained_header_count: scan.retained_header_count,
      }) });
      state.bodyOverlayNotice = "";
      state.layoutSlots = new Map();
      state.modeNotice = "";
      state.anchorOrigin = source.origin;
      if (state.view !== "service-map") { state.view = "service-map"; onViewChange?.("service-map"); }
      render();
      return true;
    }
    function htmlGeneratorSourceIsRetained(source, snapshot = state.snapshot) {
      if (!sourceIsRetained(source, snapshot)) return false;
      const origin = snapshot.origins.find((item) => item.origin === source.origin);
      return origin.endpoints.filter((item) => item.evidence.includes(source.evidenceId))
        .every((item) => item.method === source.method && item.path === source.path);
    }
    async function loadHtmlGenerators() {
      const row = state.evidence, sessionId = getSessionId(), generation = state.snapshot?.snapshot.generation;
      if (activeInspectionsBlocked() || !visible || !getEventStreamReady() || state.error || state.stale || state.sessionId !== sessionId || !row
        || state.evidenceId !== row.evidence_id || state.evidenceLoading || state.htmlGeneratorsLoading
        || !validHtmlGeneratorEvidence(row.evidence_id, sessionId, generation)
        || row.evidence_id.split(".")[3] !== row.sequence || row.evidence_id.split(".")[5] !== row.record_id
        || safeOrigin(row.origin) !== row.origin || observationTime(row.started_at) === null
        || !evidenceItems(selectedOrigin()).some((item) => item.id === row.evidence_id)) return false;
      const source = Object.freeze({ sessionId, generation, evidenceId: row.evidence_id, origin: row.origin,
        method: row.method, path: safePath(row.path), sequence: row.sequence, startedAt: row.started_at });
      if (!htmlGeneratorSourceIsRetained(source)) return false;
      cancelHtmlGenerators();
      const requestId = ++state.htmlGeneratorsRequestId, selected = state.selectedOrigin;
      const current = () => requestId === state.htmlGeneratorsRequestId && visible && getEventStreamReady() && !state.error && !state.stale
        && sessionId === getSessionId() && sessionId === state.sessionId && selected === state.selectedOrigin
        && row === state.evidence && source.evidenceId === state.evidenceId && htmlGeneratorSourceIsRetained(source);
      state.htmlGeneratorsSource = source;
      state.htmlGeneratorsLoading = true;
      htmlGeneratorsAbort = new AbortController();
      renderDetail();
      let unsupportedServer = false;
      try {
        const params = new URLSearchParams({ session_id: sessionId, evidence_id: source.evidenceId });
        const response = await request(`/api/target/service-map/html-generators?${params}`, { signal: htmlGeneratorsAbort.signal });
        unsupportedServer = response.status === 404;
        if (!response.ok) throw new Error("Retained HTML generators unavailable");
        const payload = await response.json();
        if (!current()) return false;
        state.htmlGenerators = normalizeHtmlGenerators(payload, sessionId, source.evidenceId, row);
        state.htmlGeneratorsLoading = false;
        htmlGeneratorsAbort = null;
        if (!handleSupplementLifetime(state.htmlGenerators, source)) renderDetail();
        return true;
      } catch (_) {
        if (!current()) return false;
        state.htmlGeneratorsLoading = false;
        htmlGeneratorsAbort = null;
        state.htmlGenerators = null;
        state.htmlGeneratorsError = unsupportedServer
          ? "This server does not support retained HTML generator inspection. No generator declarations were inspected."
          : "Could not verify retained HTML generators. Retry explicitly, or refresh the map and select the source again.";
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
        if (array(snapshot.stopped_by).includes("technology_token_scan_limit")) status += " · Header technology scan partial: token candidate limit reached";
        if (state.snapshot.legacyCatalog) status += ` · ${LEGACY_CATALOG_NOTICE}`;
        if (state.scopeOnly) status += " · In scope only";
        if (state.stale) status += state.refreshPaused ? " · Stale snapshot: counts have not been revalidated" : " · History changed: counts are from the previous snapshot";
        if (state.snapshot.warningCount) status += ` · ${state.snapshot.warningCount} metadata notice(s)`;
      }
      // A deletion/gap can clear the retained snapshot while refresh is paused.
      // Derive this suffix now so it never claims cleared data is still shown.
      const error = state.refreshPaused && state.error
        ? `${state.error} Automatic refresh paused. ${snapshot ? "Showing the previous snapshot as stale; use Refresh to retry." : "Use Refresh to retry."}` : state.error;
      if (activeInspectionsBlocked()) status += ` · ${INACTIVE_SESSION_NOTICE}`;
      els.status.textContent = error ? `${error}${snapshot ? ` ${status}` : ""}` : status;
      els.status.classList.toggle("service-map-error", !!state.error);
      els.refresh.disabled = state.loading;
      els.refresh.textContent = state.loading ? "Refreshing…" : "Refresh";
      els.panel.setAttribute("aria-busy", String(state.loading));
    }
    function renderGraph() {
      if (state.bodyReferenceOverlay?.kind === "peer") { renderPeerGraph(); return; }
      const selected = selectedOrigin();
      const direct = directNeighborhood();
      const query = state.query.trim().toLowerCase();
      const stages = originFilterStages(direct ? direct.neighbors : graphOrigins().filter((origin) => !state.scopeOnly || origin.in_scope || !origin.captured));
      const matchingNeighbors = direct ? stages.matching : [];
      const origins = direct ? [direct.anchor, ...matchingNeighbors] : stages.matching;
      const neighborOrder = direct ? stableOrder(state.order, matchingNeighbors) : [];
      const order = direct ? [direct.anchor.origin, ...neighborOrder.filter((id) => direct.bodyIds.has(id)), ...neighborOrder.filter((id) => !direct.bodyIds.has(id))] : state.order;
      const filterKey = JSON.stringify([query, state.productId, state.productVersion, state.scopeOnly, state.sessionId, state.anchorOrigin]);
      if (state.layoutFilter !== filterKey) { state.layoutSlots = new Map(); state.layoutFilter = filterKey; }
      const textSize = doc?.documentElement && typeof root.getComputedStyle === "function"
        ? Number.parseFloat(root.getComputedStyle(doc.documentElement).getPropertyValue("--ui-root-size")) : 12;
      const layout = graphLayout(origins, order, state.layoutSlots, state.anchorOrigin, direct ? overlayEdges() : [], graphGeometry(textSize));
      state.layoutSlots = layout.slots;
      currentLayout = layout.nodes.length ? layout : null;
      applyGraphScale();
      els.boundsDetails?.classList.toggle("hidden", !state.snapshot);
      if (!direct && els.graphDiagnostics) els.graphDiagnostics.textContent = state.snapshot
        ? `${stages.loaded.length} loaded origins · ${stages.queryHidden} query-hidden · ${stages.productHidden} product/version-hidden · ${stages.matching.length} matching · ${layout.nodes.length} shown · ${Math.max(0, stages.matching.length - layout.nodes.length)} omitted by ${MAX_CARDS}-card cap. Counts describe only loaded origins and reference cards; server projection omissions and omitted reference declarations are separate. Unloaded products and distinct origins are not counted.` : "";
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
        replace(els.canvas, `<div class="service-map-empty"><span class="service-map-empty-symbol" aria-hidden="true">◎</span><h3>${state.loading ? "Loading service origins…" : state.error && !state.snapshot ? "Service map unavailable" : state.query || state.scopeOnly || state.productId || state.productVersion ? "No matching origins" : "No retained HTTP origins"}</h3><p>${state.query || state.scopeOnly || state.productId || state.productVersion ? "Change the origin search, product/version filters or In scope only." : "This view uses only HTTP history already retained in this session."}</p></div>`);
        els.graphBounds.textContent = ["0 matching origins", stages.queryHidden ? `${stages.queryHidden} hidden by query` : "", stages.productHidden ? `${stages.productHidden} hidden by products/versions` : "", state.anchorOrigin ? "Direct relations will resume after a fresh snapshot verifies the pinned origin." : state.modeNotice, selectionNotice].filter(Boolean).join(" · ");
        return;
      }
      const edges = layout.edges.map((edge) => `<path class="service-map-edge ${edge.relationship === "body_url_reference" ? "body-reference" : edge.relationship === "link_preconnect" ? "preconnect-hint" : edge.relationship.startsWith("link_") ? "link-declaration" : edge.relationship !== "redirect_instruction" ? "header-declaration" : ""} ${[edge.source, edge.target].includes(state.selectedOrigin) ? "selected" : ""}" data-relationship="${edge.relationship}" d="${edge.path}" marker-end="url(#service-map-arrow)"><title>${escape(edge.source)} → ${escape(edge.target)} · ${escape(edge.label)} · ${edge.relationship}</title></path>`).join("");
      const cards = layout.nodes.map((origin) => {
        const selected = origin.origin === state.selectedOrigin;
        const url = new URL(origin.origin);
        const pinned = origin.origin === state.anchorOrigin;
        return `<button class="service-map-card ${!origin.captured ? "referenced" : ""} ${selected ? "selected" : ""} ${pinned ? "pinned" : ""}" type="button" data-service-origin="${escape(origin.origin)}" data-service-focus="origin:${escape(origin.origin)}" aria-current="${selected}" style="left:${origin.x}px;top:${origin.y}px">
          <span class="service-map-card-symbol" aria-hidden="true">${pinned ? "⊙" : "◎"}</span><span class="service-map-card-copy"><strong title="${escape(origin.origin)}">${escape(url.host)}</strong><span class="service-map-card-scheme">${escape(url.protocol.slice(0, -1))} · ${pinned ? "pinned anchor" : origin.captured ? "captured origin" : "referenced origin"}</span>${productSummaryMarkup(origin)}<span class="service-map-card-meta">${origin.captured ? `${origin.endpoints.length} API paths · ${origin.request_count.toLocaleString()} sampled rows` : direct?.bodyIds.has(origin.origin) ? state.bodyReferenceOverlay?.kind === "link" ? "Link declaration · no captured rows" : "Body URL reference · no captured rows" : "Referenced only in this snapshot"}</span></span>
          <span class="service-map-scope-dot ${!origin.captured ? "hidden" : ""} ${origin.in_scope ? "in-scope" : ""}" title="${origin.in_scope ? "In scope" : "Out of scope"}"></span>
        </button>`;
      }).join("");
      replace(els.canvas, `<div class="service-map-scaled"><div class="service-map-layout" style="--service-map-card-width:${layout.geometry.cardWidth}px;--service-map-card-height:${layout.geometry.cardHeight}px;--service-map-line-height:${layout.geometry.lineHeight};width:${layout.width}px;height:${layout.height}px"><svg class="service-map-edges" width="${layout.width}" height="${layout.height}" aria-hidden="true"><defs><marker id="service-map-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M1,1 L7,4 L1,7" fill="none" stroke="currentColor" stroke-width="1.5" /></marker></defs>${edges}</svg>${cards}</div></div>`);
      if (direct) {
        const counts = direct.counts;
        const linkOverlay = state.bodyReferenceOverlay?.kind === "link";
        const overlayCount = linkOverlay ? Object.values(LINK_RELATIONS).reduce((sum, item) => sum + (counts[item.type] || 0), 0) : counts.body_url_reference;
        const overlayName = linkOverlay ? "Link declaration" : "Body URL";
        const shownBodyArrows = layout.edges.filter((edge) => edge.relationship === "body_url_reference" || edge.relationship.startsWith("link_")).length;
        const shownMetadataArrows = layout.edges.length - shownBodyArrows;
        const metadataArrows = direct.edgeCount - overlayCount;
        const visibleIds = new Set(layout.nodes.map((origin) => origin.origin));
        const availableBodyArrows = overlayEdges().filter((edge) => visibleIds.has(edge.source) && visibleIds.has(edge.target)).length;
        const availableMetadataArrows = layout.edgeCount - availableBodyArrows;
        const queryHidden = stages.queryHidden, productHidden = stages.productHidden;
        const capOmitted = Math.max(0, matchingNeighbors.length - (MAX_CARDS - 1));
        const incomplete = state.snapshot.snapshot.complete !== true || array(state.snapshot.snapshot.stopped_by).length > 0;
        const outsideQuery = query && !direct.anchor.origin.toLowerCase().includes(query);
        const summary = [
          `${layout.nodes.length - 1} / ${direct.neighbors.length} known direct neighbors shown`,
          `${counts.incoming} incoming / ${counts.outgoing} outgoing links`,
          state.bodyReferenceOverlay ? `${metadataArrows} metadata / ${overlayCount} ${linkOverlay ? "Link" : "body URL"} links` : "",
          state.bodyReferenceOverlay ? `${shownBodyArrows} / ${overlayCount} ${linkOverlay ? "Link" : "body"} arrows shown` : "",
          queryHidden ? `${queryHidden} hidden by query` : "",
          productHidden ? `${productHidden} hidden by products/versions` : "",
          state.productId || state.productVersion ? `${matchingNeighbors.length} matching neighbors · pinned anchor is context only` : "",
          capOmitted ? `${capOmitted} omitted by ${MAX_CARDS}-card cap` : "",
          layout.edgeCount > MAX_EDGES ? `${layout.edgeCount - MAX_EDGES} arrows omitted by ${MAX_EDGES}-arrow cap` : "",
          outsideQuery ? "Pinned anchor is outside the search query and remains visible." : "",
          !productMatches(direct.anchor, state.productId, state.productVersion) ? "Pinned anchor remains visible outside product/version filters (context only)." : "",
          !direct.anchor.captured ? "Reference-only anchor; no captured rows shown." : "",
          !direct.neighbors.length ? "No direct neighbors appear in loaded metadata; others may exist." : "",
          incomplete ? "Known neighborhood is incomplete; see snapshot bounds above." : "",
          selectionNotice,
        ];
        const diagnostics = [
          `Direct relations · ${direct.neighbors.length} known direct neighbors in loaded metadata${state.bodyReferenceOverlay ? linkOverlay ? " and temporary Link declarations (unique union)" : " and temporary body references (unique union)" : ""}`,
          `${direct.neighbors.length} loaded neighbors · ${queryHidden} query-hidden · ${productHidden} product/version-hidden · ${matchingNeighbors.length} matching · ${layout.nodes.length - 1} shown · ${capOmitted} omitted by ${MAX_CARDS}-card cap (anchor included). Pinned anchor is context, excluded from matching counts`,
          `Non-self distinct typed links: ${counts.incoming} incoming / ${counts.outgoing} outgoing · ${counts.redirect_instruction} redirect / ${counts.origin_header} Origin / ${counts.referer_header} Referer${state.bodyReferenceOverlay ? linkOverlay ? Object.values(LINK_RELATIONS).map((item) => ` / ${counts[item.type] || 0} ${item.type}`).join("") : ` / ${counts.body_url_reference} body_url_reference` : ""}`,
          `${layout.edges.length} / ${layout.edgeCount} arrows between displayed cards (cap ${MAX_EDGES}) · ${direct.edgeCount - layout.edgeCount} links outside displayed cards`,
          ...(state.bodyReferenceOverlay ? [
            `${overlayName} typed arrows: ${shownBodyArrows} / ${overlayCount} shown · ${overlayCount - availableBodyArrows} outside displayed cards · ${availableBodyArrows - shownBodyArrows} omitted by arrow cap`,
            `Metadata typed arrows: ${shownMetadataArrows} / ${metadataArrows} shown · ${metadataArrows - availableMetadataArrows} outside displayed cards · ${availableMetadataArrows - shownMetadataArrows} omitted by arrow cap`,
            `${linkOverlay ? "Link" : "Body"} targets and arrows take priority within the same 24-card and 40-arrow limits.`,
          ] : []),
          "Links count distinct origin pairs and relationship types, not requests, URL occurrences or redirect paths. Incoming and outgoing are relative to the pinned anchor; self-links are excluded.",
          state.bodyReferenceOverlay ? linkOverlay ? "Loaded metadata and one explicit retained-header inspection only; no transitive links or confirmed calls." : "Loaded metadata and one explicit retained-preview scan only; no transitive links or confirmed calls." : "Loaded metadata only; no transitive links or confirmed calls.",
        ];
        els.graphBounds.textContent = summary.filter(Boolean).join(" · ");
        if (els.graphDiagnostics) els.graphDiagnostics.textContent = diagnostics.join(" · ");
      } else els.graphBounds.textContent = `${layout.nodes.length} / ${origins.length} matching origins${stages.queryHidden ? ` · ${stages.queryHidden} hidden by query` : ""}${stages.productHidden ? ` · ${stages.productHidden} hidden by products/versions` : ""}${origins.length > layout.nodes.length ? ` · ${origins.length - layout.nodes.length} omitted by ${MAX_CARDS}-card cap` : ""}${state.omittedGraphReferences ? ` · Reference-card cap ${MAX_REFERENCE_CARDS}; ${state.omittedGraphReferences} reference declarations omitted` : ""}${origins.length > MAX_CARDS ? " · Filter to find more" : ""}${layout.edgeCount > MAX_EDGES ? ` · ${MAX_EDGES} / ${layout.edgeCount} arrows shown` : ""}${selectionNotice ? ` · ${selectionNotice}` : ""}${state.modeNotice ? ` · ${state.modeNotice}` : ""}`;
    }
    function endpointMatches(endpoint) {
      return (!state.endpointMethod || endpoint.method === state.endpointMethod) && endpoint.path.includes(state.endpointPath);
    }
    function loadedEndpointEntries(origins = filteredOrigins(), filtered = true) {
      return origins.flatMap((origin) => origin.endpoints.map((endpoint, index) => ({ origin, endpoint, index })))
        .filter((entry) => !filtered || endpointMatches(entry.endpoint));
    }
    const endpointIdentity = ({ origin, endpoint }) => ({ origin: origin.origin, method: endpoint.method, path: endpoint.path });
    const sameEndpoint = (entry, identity) => !!identity && entry.origin.origin === identity.origin
      && entry.endpoint.method === identity.method && entry.endpoint.path === identity.path;
    function reconcileEndpointPages() {
      // Selection and pagination use exact retained identities, never old array
      // offsets. Missing anchors clamp the previous page without choosing a row.
      for (const [pageKey, anchorKey, entries] of [
        ["page", "pageAnchor", loadedEndpointEntries()],
        ["detailPage", "detailPageAnchor", loadedEndpointEntries(selectedOrigin() ? [selectedOrigin()] : [])],
      ]) {
        const index = entries.findIndex((entry) => sameEndpoint(entry, state[anchorKey]));
        state[pageKey] = Math.min(index < 0 ? state[pageKey] : Math.floor(index / API_PAGE_SIZE), Math.max(0, Math.ceil(entries.length / API_PAGE_SIZE) - 1));
        // A local origin filter can hide an otherwise retained catalog anchor.
        // Keep it so clearing the filter returns to that exact endpoint; a
        // genuinely removed endpoint still adopts the clamped fallback page.
        const hiddenLoadedAnchor = pageKey === "page" && loadedEndpointEntries((state.snapshot?.origins || []).filter((origin) => !state.scopeOnly || origin.in_scope), false)
          .some((entry) => sameEndpoint(entry, state[anchorKey]));
        if (index < 0 && !hiddenLoadedAnchor) state[anchorKey] = entries[state[pageKey] * API_PAGE_SIZE] ? endpointIdentity(entries[state[pageKey] * API_PAGE_SIZE]) : null;
      }
    }
    function setEndpointFilter(kind, value) {
      if (!["method", "path", "clear"].includes(kind)) return;
      if (kind === "clear") { state.endpointMethod = ""; state.endpointPath = ""; }
      else state[kind === "method" ? "endpointMethod" : "endpointPath"] = String(value || "");
      state.page = 0;
      state.detailPage = 0;
      state.pageAnchor = null;
      state.detailPageAnchor = null;
      // Local filters never change source identity, cancel pending inspections,
      // alter graph search/zoom, or issue additional retained-data reads.
      renderDetail();
      renderCatalog();
    }
    function endpointFiltersMarkup(place) {
      const methods = new Set(loadedEndpointEntries(state.snapshot?.origins || [], false).map(({ endpoint }) => endpoint.method));
      const missingMethod = !!state.endpointMethod && !methods.has(state.endpointMethod);
      if (state.endpointMethod) methods.add(state.endpointMethod);
      return `<fieldset class="service-map-endpoint-filters"><legend>Loaded endpoint filters</legend><div class="service-map-endpoint-filter-controls"><label>Method (exact)<select data-service-endpoint-filter="method" data-service-focus="${place}:method" title="${escape(state.endpointMethod || "All loaded methods")}" aria-describedby="${place}FilterHelp"><option value="">All methods</option>${[...methods].sort().map((method) => `<option value="${escape(method)}" ${method === state.endpointMethod ? "selected" : ""}>${escape(method)}</option>`).join("")}</select></label><label>Path contains<input type="search" value="${escape(state.endpointPath)}" data-service-endpoint-filter="path" data-service-focus="${place}:path" aria-describedby="${place}FilterHelp" placeholder="Path substring" autocomplete="off" spellcheck="false" /></label><button class="secondary-action" type="button" data-service-endpoint-filter-clear="true" data-service-focus="${place}:clear" ${!state.endpointMethod && !state.endpointPath ? "disabled" : ""}>Clear</button></div><p id="${place}FilterHelp" class="service-map-muted">Both case-sensitive · shared by both API lists · loaded snapshot only.</p>${missingMethod ? '<p class="service-map-filter-notice" role="status">Selected method is absent from the loaded snapshot. Filter retained; no loaded endpoints match.</p>' : ""}</fieldset>`;
    }
    function endpointSelectionNotice(origin = selectedOrigin()) {
      if (!origin) return "";
      const selected = state.endpointSelection || (state.evidence?.origin === origin.origin ? state.evidence : null);
      const notices = [];
      if (selected && !endpointMatches(selected)) notices.push("Selected endpoint is outside the loaded endpoint filters. Its inspector selection and evidence are preserved.");
      if (!productMatches(origin, state.productId, state.productVersion)) notices.push("Selected origin is outside the product/version filters. Its inspector and exact evidence remain open.");
      const query = state.query.trim().toLowerCase();
      if (query && !origin.origin.toLowerCase().includes(query)) notices.push("Selected origin is outside the origin search. Its inspector remains open.");
      return notices.length ? `<p class="service-map-filter-notice" role="status">${notices.join(" ")}</p>` : "";
    }
    function endpointCoverageMarkup(matching, loaded, start, hiddenOrigins = 0, hiddenProducts = 0) {
      return `<p class="service-map-muted service-map-endpoint-counts" role="status">${matching ? start + 1 : 0}–${Math.min(start + API_PAGE_SIZE, matching)} / ${matching.toLocaleString()} matching · ${loaded.toLocaleString()} loaded · ${(loaded - hiddenOrigins - hiddenProducts - matching).toLocaleString()} hidden by endpoint filters${hiddenProducts ? ` · ${hiddenProducts.toLocaleString()} hidden by origin products/versions` : ""}${hiddenOrigins ? ` · ${hiddenOrigins.toLocaleString()} hidden by origin search` : ""}.</p><p class="service-map-muted">Counts cover loaded method/path combinations only. Server scope and projection omissions are separate in the snapshot summary above; this is not an exhaustive endpoint inventory.</p>`;
    }
    function endpointButton(endpoint, origin, index, catalog = false) {
      const method = endpoint.method;
      const tone = method.toUpperCase() === "GET" ? "get" : method.toUpperCase() === "POST" ? "post" : "other";
      return `<button class="service-map-api-row ${state.selectedOrigin === origin.origin && state.endpointIndex === index ? "selected" : ""}" type="button" data-service-endpoint="${index}" data-service-origin="${escape(origin.origin)}" data-service-focus="api:${escape(JSON.stringify([origin.origin, endpoint.method, endpoint.path]))}"><span class="service-map-method ${tone}" title="${escape(method)}">${escape(method)}</span><span class="service-map-api-copy">${catalog ? `<small title="${escape(origin.origin)}">${escape(origin.origin)}</small>` : ""}<span title="${escape(endpoint.path)}">${escape(endpoint.path)}</span></span><span class="service-map-api-count">${endpoint.request_count.toLocaleString()}<span class="service-map-chevron" aria-hidden="true">›</span></span></button>`;
    }
    function relationshipMarkup(origin) {
      const group = state.relationsByOrigin?.get(origin.origin);
      const overlay = state.bodyReferenceOverlay;
      const links = overlay?.kind === "link" ? overlay.scan.observations.filter((item) => origin.origin === overlay.source.origin || item.target_origin === origin.origin) : [];
      if (!group?.items.length && !links.length) return "";
      const metadata = (group?.items || []).slice(0, MAX_REFERENCE_SOURCES);
      const shownLinks = links.slice(0, Math.max(0, MAX_REFERENCE_SOURCES - metadata.length));
      const omitted = (group?.omitted || 0) + links.length - shownLinks.length;
      return `<h4>${links.length ? "Shown relationships" : "Metadata relationships"}</h4><p class="service-map-muted">Header declarations and redirect instructions do not prove service calls. Select a relationship to verify its exact source.</p><div class="service-map-evidence-list">${metadata.map((item) => `<button class="service-map-evidence-button service-map-relation" type="button" data-service-relation-origin="${escape(item.capturedOrigin)}" data-service-relation-evidence="${escape(item.evidence)}" data-service-focus="relation:${escape(JSON.stringify([item.sourceOrigin, item.targetOrigin, item.targetPath || "", item.label]))}"><span><strong>${escape(item.label)}</strong>${escape(item.sourceOrigin)} → ${escape(item.targetOrigin)}${escape(item.targetPath || "")}</span><span>${item.evidence ? "View source ›" : "Inspect ›"}</span></button>`).join("")}</div>${shownLinks.length ? `${overlayProvenanceMarkup()}${linkObservationsMarkup(shownLinks)}` : ""}${omitted ? `<p class="service-map-muted">${omitted} additional relationship declarations omitted from this pane.</p>` : ""}`;
    }
    function bodyReferencesMarkup() {
      const result = state.bodyReferences;
      const scanned = result?.status === "scanned" || result?.status === "partial";
      const hasNeighbors = scanned && result.references.some((item) => item.target_origin !== state.bodyReferencesSource?.origin);
      const showing = state.bodyReferenceOverlay?.kind === "body" && state.bodyReferenceOverlay?.source.evidenceId === state.bodyReferencesSource?.evidenceId
        && state.bodyReferenceOverlay?.source.origin === state.bodyReferencesSource?.origin && !!state.bodyReferenceOverlay;
      const showDisabled = activeInspectionsBlocked() || !scanned || !hasNeighbors || !getEventStreamReady() || !!state.error || state.stale || !sourceIsRetained(state.bodyReferencesSource);
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
      return `<div aria-busy="${state.bodyReferencesLoading}"><p class="service-map-muted">Optional inspection of this exact response’s retained HTML or JavaScript preview. Active session only. A literal URL reference does not prove a request, call, or service ownership.</p><button class="secondary-action" type="button" data-service-body-references="inspect" data-service-focus="body-references:inspect" ${activeInspectionsBlocked() || state.bodyReferencesLoading ? "disabled" : ""}>${buttonLabel}</button>${state.bodyReferencesLoading ? '<p class="service-map-muted" role="status">Inspecting one retained preview locally…</p>' : ""}${state.bodyReferencesError ? `<p class="service-map-error" role="status">${escape(state.bodyReferencesError)}</p>` : ""}${summary}${state.bodyReferencesSource && scanned ? relationshipSourceMarkup(state.bodyReferencesSource, result, "body") : ""}<button class="secondary-action" type="button" data-service-show-body-references="show" data-service-focus="body-references:show" aria-pressed="${showing}" aria-describedby="serviceMapBodyShowHint" ${showDisabled ? "disabled" : ""}>${state.bodyReferenceOverlay ? "Replace shown references" : "Show references on map"}</button><p class="service-map-muted" id="serviceMapBodyShowHint">${showHint}</p><details class="service-map-body-reference-limits"><summary data-service-focus="body-references:limits">Coverage and limits</summary><p>Literal absolute HTTP(S) URLs only. Origins are text, never fetched. No script execution or relative URL resolution. Only Show references on map adds temporary Direct relations. Offsets are zero-based bytes relative to the retained UTF-8 preview, not the original response.</p><p>Scan: ${BODY_REFERENCE_LIMITS.scan_bytes.toLocaleString()} bytes · Serialized row read: ${BODY_REFERENCE_LIMITS.serialized_row_bytes.toLocaleString()} bytes · Origins: ${BODY_REFERENCE_LIMITS.origins} · URL candidates: ${BODY_REFERENCE_LIMITS.candidates.toLocaleString()} · Candidate: ${BODY_REFERENCE_LIMITS.candidate_bytes.toLocaleString()} bytes · Offsets per origin: ${BODY_REFERENCE_LIMITS.offsets_per_origin}.</p>${result ? `<p>${result.candidates_scanned.toLocaleString()} candidates inspected · Capture preview truncated: ${result.capture_preview_truncated ? "yes" : "no"} · Scan prefix truncated: ${result.scan_prefix_truncated ? "yes" : "no"}.</p>` : ""}</details></div>`;
    }
    function relationshipSourceMarkup(source, result, kind) {
      return `<p class="service-map-overlay-source">${escape(source.method)} ${escape(source.origin)}${escape(source.path)}</p><p class="service-map-muted">Sequence ${escape(source.sequence)} · Captured ${escape(source.startedAt)} · ${kind === "link" ? "Response Link headers" : "Body URL references"} · ${result.complete ? "Complete" : "Partial"} inspection</p>`;
    }
    function relationshipsMarkup() {
      return `<section class="service-map-body-references service-map-retained-relationships" aria-label="Retained relationships"><h4>Retained relationships</h4><label class="service-map-relationship-kind">Inspect source<select data-service-relationship-kind="select" data-service-focus="relationships:kind" aria-label="Retained relationship source"><option value="body" ${state.relationshipKind === "body" ? "selected" : ""}>Body URL references</option><option value="link" ${state.relationshipKind === "link" ? "selected" : ""}>Response Link headers</option></select></label>${state.relationshipKind === "link" ? linkDeclarationsMarkup() : bodyReferencesMarkup()}</section>`;
    }
    function linkObservationsMarkup(observations) {
      return `<ul class="service-map-body-reference-list" aria-label="Retained Link declarations">${observations.map((item) => `<li><span class="service-map-body-reference-origin">${escape(item.target_origin)}</span><span>${escape(LINK_RELATIONS[item.relation].label)} · Response Link header index ${item.header_index}</span><span>Retained header value UTF-8 bytes: member ${item.link_value_span.start}–${item.link_value_span.end} · target ${item.target_span.start}–${item.target_span.end} · relation ${item.relation_span.start}–${item.relation_span.end}. Zero-based, end exclusive; not original wire offsets.</span></li>`).join("")}</ul>`;
    }
    function linkDeclarationsMarkup() {
      const result = state.linkDeclarations, source = state.linkDeclarationsSource, scanned = result?.status === "scanned";
      const hasNeighbors = scanned && result.observations.some((item) => item.target_origin !== source?.origin);
      const showing = state.bodyReferenceOverlay?.kind === "link" && state.bodyReferenceOverlay.source.evidenceId === source?.evidenceId;
      const disabled = activeInspectionsBlocked() || state.bodyReferencesLoading || !getEventStreamReady() || !!state.error || state.stale;
      const showDisabled = !scanned || !hasNeighbors || disabled || !htmlGeneratorSourceIsRetained(source);
      const label = state.bodyReferencesLoading ? "Inspecting retained Link headers…" : state.linkDeclarationsError || result?.status === "unavailable"
        ? "Retry retained Link headers" : result ? "Inspect retained Link headers again" : "Inspect retained Link headers";
      const message = !result ? "" : result.status === "unsupported" ? "This retained source is unsupported. No Link declarations were inspected."
        : result.status === "unavailable" ? "Retained response metadata is unavailable. No Link declarations were inspected."
          : result.observations.length ? `${result.observations.length} retained Link declaration${result.observations.length === 1 ? "" : "s"} · ${new Set(result.observations.map((item) => item.target_origin)).size} target origins`
            : result.complete ? "No supported Link declarations found in the inspected retained headers." : "No supported Link declarations are available from the inspected prefix; coverage is partial.";
      return `<div aria-busy="${state.bodyReferencesLoading}"><p class="service-map-muted">Optional inspection of this exact response’s retained Link headers. GET or HEAD with status 200 only. Declarations and preconnect hints do not prove requests, calls, ownership or deployed services.</p><button class="secondary-action" type="button" data-service-link-declarations="inspect" data-service-focus="relationships:inspect" ${disabled ? "disabled" : ""}>${label}</button>${state.bodyReferencesLoading ? '<p class="service-map-muted" role="status">Inspecting retained response headers locally…</p>' : ""}${state.linkDeclarationsError ? `<p class="service-map-error" role="status">${escape(state.linkDeclarationsError)}</p>` : ""}${result ? `<div class="service-map-body-reference-result" role="status"><p>${escape(message)}</p>${scanned ? `<p>${result.complete ? "Complete supported-grammar inspection" : "Partial retained-header inspection"} · ${result.counts.headers_inspected} / ${result.retained_header_count} retained headers inspected · ${result.counts.header_bytes_admitted} name/value bytes admitted</p>` : ""}${result.reasons.length ? `<p>${result.reasons.map((reason) => escape(LINK_REASONS[reason])).join(" · ")}</p>` : ""}</div>` : ""}${result?.observations.length ? linkObservationsMarkup(result.observations) : ""}${scanned && source ? relationshipSourceMarkup(source, result, "link") : ""}<button class="secondary-action" type="button" data-service-show-link-declarations="show" data-service-focus="relationships:show" aria-pressed="${showing}" ${showDisabled ? "disabled" : ""}>${state.bodyReferenceOverlay ? "Replace shown references" : "Show references on map"}</button><p class="service-map-muted">${scanned && !hasNeighbors ? result.observations.length ? "Only the source origin was declared; self-links are not drawn." : "There are no declared target origins to show on the map." : "Show pins this exact source in Direct relations. One temporary visualization at a time; no additional read."}</p><details class="service-map-body-reference-limits"><summary data-service-focus="relationships:limits">Coverage and limits</summary><p>Literal absolute HTTP(S) targets for service-doc, service-desc, describedby and preconnect only. Complete means this bounded grammar, not every relationship or dependency. Origins are text, never fetched; no URL resolution, DNS, probes or body reads. Source spans refer to retained header value UTF-8 bytes, not original wire offsets.</p><p>Headers: 128 · Name/value bytes: 32,768 · Link field: 4,096 bytes · Target: 4,096 bytes · Candidates: 128 · Parameters and relation tokens per member: 32 each · Observations: 32 · Target origin: 268 bytes.</p>${scanned ? `<p>${result.counts.link_fields_inspected} Link fields · ${result.counts.candidates_inspected} candidates · ${result.counts.members_framed} framed members · ${result.counts.parameters_inspected} parameters · ${result.counts.relation_tokens_inspected} relation tokens · ${result.counts.unknown_relation_tokens_inspected} unknown relation tokens inspected. Counts describe inspected input only; uninspected declarations are not counted.</p>` : ""}</details></div>`;
    }
    function htmlGeneratorsMarkup() {
      const result = state.htmlGenerators, scanned = result?.status === "scanned";
      const disabled = activeInspectionsBlocked() || state.htmlGeneratorsLoading || !getEventStreamReady() || !!state.error || state.stale;
      const buttonLabel = state.htmlGeneratorsLoading ? "Inspecting retained HTML generators…"
        : state.htmlGeneratorsError || result?.status === "unavailable" ? "Retry retained HTML generators"
          : result ? "Inspect retained HTML generators again" : "Inspect retained HTML generators";
      let summary = "";
      if (result) {
        const message = result.status === "unsupported" ? "Retained preview is unsupported. Generator declarations were not inspected."
          : result.status === "unavailable" ? "Retained preview is unavailable. Generator declarations were not inspected."
            : result.observations.length ? `${result.observations.length} reported HTML generator declaration${result.observations.length === 1 ? "" : "s"}`
              : result.complete ? "No supported WordPress or Drupal generator declarations in the inspected head."
                : "No supported generator declarations are available from the inspected prefix; coverage is partial.";
        summary = `<div class="service-map-html-generator-result" role="status"><p>${escape(message)}</p>${scanned ? `<p class="service-map-muted">${result.complete ? "Complete supported-head inspection" : "Partial retained-preview coverage"} · Supported explicit head closed: ${result.head_closed ? "yes" : "no"}. The body after the head is not parsed.</p><p class="service-map-muted">${result.scan_prefix_bytes.toLocaleString()} / ${result.retained_preview_bytes.toLocaleString()} retained UTF-8 preview bytes offered to the parser; this is not a count of bytes fully parsed.</p><p class="service-map-muted">Capture preview truncated: ${result.capture_preview_truncated ? "yes" : "no"} · Scan prefix truncated: ${result.scan_prefix_truncated ? "yes" : "no"}.</p><p class="service-map-muted">Source: ${escape(result.source_origin)} · Sequence ${escape(result.source_sequence)} · Captured ${escape(result.source_started_at)}</p>` : ""}${result.reasons.length ? `<p class="service-map-muted">${result.reasons.map((reason) => escape(HTML_GENERATOR_REASONS[reason])).join(" · ")}</p>` : ""}</div>`;
      }
      const observations = result?.observations.length ? `<ul class="service-map-references service-map-html-generator-list" aria-label="Reported HTML generator declarations">${result.observations.map((item) => `<li><strong>${escape(item.name)}</strong> · ${escape(technologyVersion(item))}<p class="service-map-muted">Catalog: CMS · Source: response HTML meta generator · Literal declaration</p><p class="service-map-muted">Retained UTF-8 preview bytes: tag ${item.tag_span.start}–${item.tag_span.end} · content ${item.content_span.start}–${item.content_span.end} · product ${item.product_span.start}–${item.product_span.end}${item.version_span ? ` · version ${item.version_span.start}–${item.version_span.end}` : ""} (zero-based, end exclusive).</p></li>`).join("")}</ul>` : "";
      return `<section class="service-map-body-references service-map-html-generators" aria-label="Retained HTML generators" aria-busy="${state.htmlGeneratorsLoading}"><h4>Retained HTML generators</h4><p class="service-map-muted">Optional, independent inspection of this exact response’s retained HTML preview. These are reported generators, not proven backend/runtime technologies or installed versions.</p><button class="secondary-action" type="button" data-service-html-generators="inspect" data-service-focus="html-generators:inspect" ${disabled ? "disabled" : ""}>${buttonLabel}</button>${state.htmlGeneratorsLoading ? '<p class="service-map-muted" role="status">Inspecting one retained HTML preview locally…</p>' : ""}${state.htmlGeneratorsError ? `<p class="service-map-error" role="status">${escape(state.htmlGeneratorsError)}</p>` : ""}${summary}${observations}<details class="service-map-html-generator-limits"><summary data-service-focus="html-generators:limits">Generator coverage and limits</summary><p>Literal WordPress and Drupal declarations in a supported explicit HTML head only. Numeric or dotted reported versions only. No HTML recovery, entity decoding, script execution, fetching or map aggregation. A closed head can coexist with a truncated, unparsed body; overall coverage remains partial.</p><p>Prefix: ${HTML_GENERATOR_LIMITS.scan_bytes.toLocaleString()} bytes · Serialized row read: ${HTML_GENERATOR_LIMITS.serialized_row_bytes.toLocaleString()} bytes · Tag candidates: ${HTML_GENERATOR_LIMITS.tag_candidates.toLocaleString()} · Tag: ${HTML_GENERATOR_LIMITS.tag_bytes.toLocaleString()} bytes · Attributes per tag: ${HTML_GENERATOR_LIMITS.attributes} · Declarations: ${HTML_GENERATOR_LIMITS.observations} · Version and separator: ${HTML_GENERATOR_LIMITS.version_bytes} bytes each.</p>${scanned ? `<p>${result.tag_candidates_inspected.toLocaleString()} tag candidates inspected · Additional tag candidate encountered: ${result.additional_tag_candidate_encountered ? "yes" : "no"} · Additional supported declaration encountered: ${result.additional_observation_encountered ? "yes" : "no"} · Unrecognized product declarations among inspected candidates: ${result.unknown_product_declarations.toLocaleString()}. Uninspected declarations are not counted.</p>${result.parser_stop ? `<p>Parser stop: ${escape(HTML_GENERATOR_STOP_REASONS[result.parser_stop.reason])} · Raw preview byte cursor: ${result.parser_stop.byte_cursor.toLocaleString()} (not a text-slicing boundary).</p>` : ""}` : ""}</details></section>`;
    }
    function overlayProvenanceMarkup() {
      const overlay = state.bodyReferenceOverlay;
      if (!overlay) return "";
      const { source, scan } = overlay;
      if (overlay.kind === "peer") return `<p class="service-map-overlay-source" title="${escape(overlay.scan.source.origin)} · Source sequence ${escape(overlay.scan.source.sequence)}">${escape(overlay.scan.source.origin)} · Source sequence ${escape(overlay.scan.source.sequence)}</p><p>${overlay.scan.complete ? "Complete considered window" : "Partial inspection"} · ${overlay.scan.snapshot.considered_rows} / ${overlay.scan.snapshot.retained_rows} retained rows considered · ${overlay.scan.counts.matching_rows} other matching rows</p><details class="service-map-peer-coverage"><summary data-service-focus="peer:coverage">Inspection coverage and privacy</summary>${peerCountsMarkup(overlay.scan)}<p>${PEER_PRIVACY}</p><p>Recorded timestamps do not imply connection counts or continuous endpoint occupancy.</p></details>`;
      if (overlay.kind === "link") return `${relationshipSourceMarkup(source, scan, "link")}${scan.reasons.length ? `<p class="service-map-muted">${scan.reasons.map((reason) => escape(LINK_REASONS[reason])).join(" · ")}</p>` : ""}`;
      return `<p class="service-map-overlay-source" title="${escape(`${source.method} ${source.origin}${source.path}`)}">${escape(source.method)} ${escape(source.origin)}${escape(source.path)}</p><p>Sequence ${escape(source.sequence)} · ${escape(source.startedAt)} · ${scan.complete ? "Complete" : "Partial"} retained-preview scan</p>${!scan.complete ? `<p class="service-map-muted">${scan.reasons.map((reason) => escape(BODY_REFERENCE_REASONS[reason] || reason.replace(/_/g, " "))).join(" · ")}${scan.omitted_reference_occurrences ? ` · ${scan.omitted_reference_occurrences} reference occurrence(s) omitted` : ""}</p>` : ""}`;
    }
    function renderOverlayStatus() {
      if (!els.overlayStatus) return;
      const overlay = state.bodyReferenceOverlay;
      els.overlayStatus.classList.toggle("hidden", !overlay && !state.bodyOverlayNotice);
      if (overlay?.kind === "peer") {
        replace(els.overlayStatus, `<div><strong>Temporary recorded address group</strong>${overlayProvenanceMarkup()}<p>Undirected observation spokes only. Changes to retained history clear this view.</p></div><div class="service-map-overlay-actions"><button class="secondary-action" type="button" data-service-peer-origin="${escape(overlay.scan.source.origin)}" data-service-peer-evidence="${escape(overlay.scan.evidence_id)}" data-service-focus="peer:source">View inspected source</button><button class="secondary-action" type="button" data-service-clear-body-references="clear" data-service-focus="peer:clear">Clear recorded address group</button></div>`);
        return;
      }
      replace(els.overlayStatus, overlay ? `<div><strong>${overlay.kind === "link" ? "Temporary Response Link references" : "Temporary body URL references"}</strong>${overlayProvenanceMarkup()}<p>Literal references only; no confirmed requests. Changes to history clear this view.</p></div><div class="service-map-overlay-actions"><button class="secondary-action" type="button" data-service-relation-origin="${escape(overlay.source.origin)}" data-service-relation-evidence="${escape(overlay.source.evidenceId)}" data-service-focus="body-references:source">View inspected source</button><button class="secondary-action" type="button" data-service-clear-body-references="clear" data-service-focus="body-references:clear">Clear ${overlay.kind === "link" ? "Link" : "body"} references</button></div>` : `<p role="status">${escape(state.bodyOverlayNotice)}</p>`);
    }
    function socketPeerMarkup() {
      const peer = state.socketPeer;
      if (!peer) return "";
      const heading = '<h4>Upstream socket peer</h4>';
      let content;
      if (peer.status === "not_recorded") content = '<p class="service-map-muted">Not recorded for this row.</p>';
      else if (peer.status === "unsupported") content = '<p class="service-map-muted">Socket peer details are not available from this server.</p>';
      else if (peer.status === "invalid") content = '<p class="service-map-error">Socket peer details could not be verified for this read.</p>';
      else {
        const route = { direct: "Direct", http_proxy: "HTTP proxy", socks_proxy: "SOCKS proxy", unknown: "Unknown" }[peer.route];
        const address = peer.status === "observed" ? `${peer.ip.includes(":") ? `[${peer.ip}]` : peer.ip}:${peer.port}` : "Unavailable at response headers";
        content = `<dl><dt>Observed socket peer</dt><dd>${escape(address)}</dd><dt>Route used</dt><dd>${escape(route)}</dd><dt>Response headers observed at</dt><dd>${escape(peer.observedAt)}</dd><dt>Source</dt><dd>HTTP client response remote address</dd></dl><p class="service-map-muted">This may identify an upstream proxy or internal address. It does not establish the origin’s IP address, DNS identity, or TLS certificate identity.</p>`;
      }
      return `<section class="service-map-socket-peer" aria-label="Upstream socket peer">${heading}${content}</section>${peerMatchesMarkup()}`;
    }
    function technologyMarkup(items, origin = null, sourceRow = null) {
      if (!items.length && sourceRow) return `<p class="service-map-muted service-map-product-coverage">${sourceRow.technology_observations_present ? "No header-reported product declarations in the inspected source metadata." : "Product/version metadata unavailable for this source."} Missing or empty observations do not prove that a product is absent.</p>`;
      if (!items.length) return origin ? `<p class="service-map-muted service-map-product-coverage">${origin.technology_observations_present ? "No loaded product declarations." : origin.peerInspectionOnly ? "Default product metadata unavailable. Exact peer samples do not provide a default origin aggregate." : "Product/version metadata unavailable for this origin."} Missing or empty observations do not prove that a product is absent.</p>` : "";
      const rows = items.map((item) => {
        const detail = origin
          ? `${item.row_count} retained row${item.row_count === 1 ? "" : "s"} · First: ${item.first_seen} · Last: ${item.last_seen}`
          : `Response header index ${item.header_index} (zero-based) · Retained header value: UTF-8 product bytes ${item.product_span.start}–${item.product_span.end}${item.version_span ? ` · Version bytes ${item.version_span.start}–${item.version_span.end}` : ""} (end exclusive)`;
        const sources = origin ? `<div class="service-map-evidence-list">${item.evidence.map((id, index) => `<button class="service-map-evidence-button" type="button" data-service-relation-origin="${escape(origin.origin)}" data-service-relation-evidence="${escape(id)}" data-service-focus="technology:${escape(JSON.stringify([item.product_id, item.header_name, item.reported_version, item.version_status, id]))}"><span>${escape(item.name)} · ${escape(technologyVersion(item))}</span><span>View source${item.evidence.length > 1 ? ` ${index + 1}` : ""} ›</span></button>`).join("")}</div>` : "";
        return `<li><strong>${escape(item.name)}</strong> · ${escape(technologyVersion(item))}<p class="service-map-muted">Catalog: ${escape(TECHNOLOGY_CATEGORIES[item.catalog_category])} · Source: ${escape(item.header_name)} response header · Literal declaration</p><p class="service-map-muted">${escape(detail)}</p>${sources}</li>`;
      }).join("");
      return `<section class="service-map-technologies" aria-label="Header-reported technologies"><h4>Header-reported technologies</h4><p class="service-map-muted">Literal declarations from retained response headers. Versions are reported, not verified. Catalog categories describe product types only; deployment roles, ownership and vulnerabilities are unverified.</p><ul class="service-map-references">${rows}</ul></section>`;
    }
    function evidenceMarkup(origin) {
      const items = evidenceItems(origin);
      const endpoint = state.endpointIndex === null ? null : origin.endpoints[state.endpointIndex];
      const row = state.evidence;
      const peerOnlySource = !!row && !!state.peerEvidenceBinding && !items.some((item) => item.id === row.evidence_id);
      const supplementaryMarkup = peerOnlySource
        ? '<p class="service-map-muted">Body URL, response Link and retained HTML generator inspections are unavailable for this source outside the default metadata projection. Its exact source metadata, recorded header observations and socket peer remain available here.</p>'
        : htmlGeneratorsMarkup() + relationshipsMarkup();
      const meta = row ? `<div class="service-map-verified"><h4>Verified retained source</h4><dl><dt>Method</dt><dd>${escape(row.method)}</dd><dt>Origin</dt><dd>${escape(safeOrigin(row.origin))}</dd><dt>Path</dt><dd>${escape(safePath(row.path))}</dd><dt>Status</dt><dd>${escape(row.status ?? "No response")}</dd><dt>Sequence</dt><dd>${escape(row.sequence ?? "Unavailable")}</dd><dt>Captured</dt><dd>${escape(row.started_at || "Unavailable")}</dd><dt>Record ID</dt><dd>${escape(row.record_id || "Unavailable")}</dd></dl>${row.metadata_complete === false ? `<p class="service-map-error">Source metadata is incomplete: ${escape(array(row.omitted).join(", ") || "bounded metadata")}</p>` : ""}${array(row.omitted).includes("technology_token_scan_limit") ? '<p class="service-map-error">Header technology scan is partial: token candidate limit reached. Additional declarations may be omitted.</p>' : ""}<h4>Source observations</h4><ul class="service-map-references">${array(row.header_links).filter((link) => ["origin_header", "referer_header"].includes(link.relationship)).map((link) => `<li>${link.relationship === "origin_header" ? "Origin" : "Referer"} header: ${escape(safeOrigin(link.source_origin))} (declaration)</li>`).join("")}${row.redirect ? `<li>Redirect instruction: ${escape(safeOrigin(row.redirect.target_origin))}${escape(safePath(row.redirect.target_path))}</li>` : ""}${array(row.technology_clues).map((clue) => `<li>${escape(clue.name)} · ${escape(clue.header_name)} header clue</li>`).join("")}</ul>${state.evidenceLegacyCatalog ? `<p class="service-map-muted">${LEGACY_CATALOG_NOTICE}</p>` : ""}${technologyMarkup(row.technology_observations, null, row)}<p>Exact retained source verified for this read.</p></div>` : "";
      return `${endpoint ? `<div class="service-map-endpoint-summary"><span class="service-map-method" title="${escape(endpoint.method)}">${escape(endpoint.method)}</span><strong>${escape(endpoint.path)}</strong><p>${endpoint.request_count} sampled rows · Statuses: ${escape(endpoint.statuses.join(", ") || "No response")}</p></div>` : ""}
        <h4>Source evidence</h4><p class="service-map-muted">Read the exact retained source metadata. Bodies, query strings and private headers are not included.</p>
        ${activeInspectionsBlocked() ? `<p class="service-map-error" role="status">${INACTIVE_SESSION_NOTICE}</p>` : ""}${state.evidenceLoading ? '<p class="service-map-muted" role="status">Verifying source…</p>' : ""}${state.evidenceError ? `<p class="service-map-error" role="status">${escape(state.evidenceError)}</p>` : ""}${meta}${peerOriginMarkup(origin)}${row ? socketPeerMarkup() + supplementaryMarkup : ""}
        <div class="service-map-evidence-list">${items.slice(0, 30).map((item) => `<button class="service-map-evidence-button ${state.evidenceId === item.id ? "selected" : ""}" type="button" data-service-evidence="${escape(item.id)}" data-service-focus="evidence:${escape(item.id)}"><span>${escape(item.label)}</span><span>View source ›</span></button>`).join("") || `<p class="service-map-muted">${origin.peerInspectionOnly ? "No default-map samples are loaded for this origin; use the exact peer-inspection samples above." : "No source samples are available in this bounded snapshot."}</p>`}</div>${items.length > 30 ? `<p class="service-map-muted">Showing 30 of ${items.length} source samples. Select an API path to narrow the evidence.</p>` : ""}
        ${!endpoint ? technologyMarkup(origin.technology_observations, origin) + relationshipMarkup(origin) : ""}
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
      const generatorLimitsOpen = els.detail.querySelector?.(".service-map-html-generator-limits")?.open;
      replace(els.detail, html);
      detailRender = { identity, evidence: state.evidence, html };
      if (sameSource) {
        const body = els.detail.querySelector?.(".service-map-detail-body");
        const list = els.detail.querySelector?.(".service-map-body-reference-list");
        const limits = els.detail.querySelector?.(".service-map-body-reference-limits");
        if (limits && limitsOpen !== undefined) limits.open = limitsOpen;
        const generatorLimits = els.detail.querySelector?.(".service-map-html-generator-limits");
        if (generatorLimits && generatorLimitsOpen !== undefined) generatorLimits.open = generatorLimitsOpen;
        if (body && bodyScroll !== undefined) body.scrollTop = bodyScroll;
        if (list && listScroll !== undefined) list.scrollTop = listScroll;
      }
    }
    function renderDetail() {
      const origin = selectedOrigin();
      if (!origin) { replaceDetail('<div class="service-map-empty"><h3>Select an origin</h3><p>Choose a card to inspect its API paths and retained evidence.</p></div>'); return; }
      if (origin.peerInspectionOnly) {
        replaceDetail(`<header class="service-map-detail-header"><div><h3 title="${escape(origin.origin)}">${escape(new URL(origin.origin).host)}</h3><p>${escape(origin.origin)}</p></div><span class="detail-chip ${origin.in_scope ? "ok" : "none"}">${origin.in_scope ? "In scope" : "Out of scope"}</span></header><div class="service-map-detail-body">${endpointSelectionNotice(origin)}${evidenceMarkup(origin)}</div>`);
        return;
      }
      if (!origin.captured) {
        const bodyReference = overlayReferences().some((item) => item.target_origin === origin.origin);
        replaceDetail(`<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div></header><div class="service-map-detail-body">${endpointSelectionNotice(origin)}<h4>Referenced only in this snapshot</h4><p class="service-map-muted">This origin appears only in loaded declarations${bodyReference ? state.bodyReferenceOverlay.kind === "link" ? " or the temporary Link declarations" : " or the temporary body URL references" : ""}. No captured rows are shown for this origin. Scope and projection bounds may omit captured history; no service role or request count is established here.</p>${bodyReference ? `<h4>${state.bodyReferenceOverlay.kind === "link" ? "Retained Link declaration" : "Literal body URL reference"}</h4>${overlayProvenanceMarkup()}<p class="service-map-muted">This reference does not prove a request, call, or service ownership.</p>` : ""}${relationshipMarkup(origin)}<h4>Captured origins</h4><div class="service-map-evidence-list">${origin.references.map((ref) => `<button class="service-map-evidence-button" type="button" data-service-origin="${escape(ref.origin)}" data-service-focus="reference:${escape(ref.origin)}"><span>${escape(ref.label)} ${escape(ref.origin)}</span><span>Inspect source ›</span></button>`).join("")}</div>${origin.referencesOmitted ? `<p class="service-map-muted">${origin.referencesOmitted} additional source references omitted from this pane.</p>` : ""}</div>`);
        return;
      }
      const entries = loadedEndpointEntries([origin]);
      const pageCount = Math.max(1, Math.ceil(entries.length / API_PAGE_SIZE));
      state.detailPage = Math.min(state.detailPage, pageCount - 1);
      const start = state.detailPage * API_PAGE_SIZE;
      if (!state.detailPageAnchor && entries[start]) state.detailPageAnchor = endpointIdentity(entries[start]);
      replaceDetail(`<header class="service-map-detail-header"><div><h3 title="${escape(new URL(origin.origin).host)}">${escape(new URL(origin.origin).host)}</h3><p title="${escape(origin.origin)}">${escape(origin.origin)}</p></div><span class="detail-chip ${origin.in_scope ? "ok" : "none"}">${origin.in_scope ? "In scope" : "Out of scope"}</span></header>
        <div class="service-map-detail-tabs" role="tablist" aria-label="Origin details"><button type="button" role="tab" id="serviceMapDetailApisTab" aria-controls="serviceMapDetailBody" aria-selected="${state.detailTab === "apis"}" tabindex="${state.detailTab === "apis" ? 0 : -1}" data-service-detail-tab="apis" data-service-focus="detail:apis">APIs</button><button type="button" role="tab" id="serviceMapDetailEvidenceTab" aria-controls="serviceMapDetailBody" aria-selected="${state.detailTab === "evidence"}" tabindex="${state.detailTab === "evidence" ? 0 : -1}" data-service-detail-tab="evidence" data-service-focus="detail:evidence">Evidence</button></div>
        <div class="service-map-detail-body" id="serviceMapDetailBody" role="tabpanel" aria-labelledby="${state.detailTab === "apis" ? "serviceMapDetailApisTab" : "serviceMapDetailEvidenceTab"}">${endpointSelectionNotice(origin)}${state.detailTab === "evidence" ? evidenceMarkup(origin) : `${endpointFiltersMarkup("serviceMapDetail")}${endpointCoverageMarkup(entries.length, origin.endpoints.length, start)}<div class="service-map-api-list">${entries.slice(start, start + API_PAGE_SIZE).map(({ endpoint, index }) => endpointButton(endpoint, origin, index)).join("") || '<p class="empty-copy">No loaded endpoints match these filters for this origin.</p>'}</div>${pageCount > 1 ? `<div class="service-map-pagination"><button class="secondary-action" type="button" data-service-detail-page="${state.detailPage - 1}" ${state.detailPage === 0 ? "disabled" : ""}>Previous</button><span>${state.detailPage + 1} / ${pageCount}</span><button class="secondary-action" type="button" data-service-detail-page="${state.detailPage + 1}" ${state.detailPage + 1 >= pageCount ? "disabled" : ""}>Next</button></div>` : ""}${peerOriginMarkup(origin)}${technologyMarkup(origin.technology_observations, origin)}${relationshipMarkup(origin)}<div class="service-map-evidence-summary"><h4>Evidence summary</h4><p>${origin.request_count.toLocaleString()} sampled HTTP rows in this snapshot. ${origin.redirects.length} redirect instruction(s).</p><p class="service-map-muted">${origin.technology_clues.length ? `${origin.technology_clues.length} response-header clue(s). ` : ""}No captured browser initiator, origin-IP association or certificate identity is available. Optional socket peers start in exact source evidence; explicit recorded matches can be shown as a temporary group.</p><button class="secondary-action" type="button" data-service-detail-tab="evidence" data-service-focus="view:evidence">${state.endpointSelection ? "View selected endpoint evidence" : "View evidence"}</button></div>`}</div>`);
    }
    function renderCatalog() {
      const loaded = loadedEndpointEntries((state.snapshot?.origins || []).filter((origin) => !state.scopeOnly || origin.in_scope), false);
      const stages = originFilterStages((state.snapshot?.origins || []).filter((origin) => !state.scopeOnly || origin.in_scope));
      const searchedEntries = loadedEndpointEntries(stages.searched, false);
      const originEntries = loadedEndpointEntries(stages.matching, false);
      const entries = originEntries.filter(({ endpoint }) => endpointMatches(endpoint));
      const pages = Math.max(1, Math.ceil(entries.length / API_PAGE_SIZE));
      // Invalidation temporarily removes the snapshot. Keep the previous page
      // as the fallback if its identity anchor is absent from the next result.
      if (state.snapshot) state.page = Math.min(state.page, pages - 1);
      const start = state.page * API_PAGE_SIZE;
      if (!state.pageAnchor && entries[start]) state.pageAnchor = endpointIdentity(entries[start]);
      replace(els.catalog, `<header class="service-map-catalog-header"><div><h3>Observed API paths</h3><p>Method/path combinations belonging to matching origins. Origin products/versions do not prove that each endpoint reported them. Query strings are omitted; no endpoint is probed.</p></div><span>${entries.length.toLocaleString()} matching loaded combinations</span></header>${endpointFiltersMarkup("serviceMapCatalog")}${endpointSelectionNotice()}${endpointCoverageMarkup(entries.length, loaded.length, start, loaded.length - searchedEntries.length, searchedEntries.length - originEntries.length)}<div class="service-map-api-list">${entries.slice(start, start + API_PAGE_SIZE).map(({ origin, endpoint, index }) => endpointButton(endpoint, origin, index, true)).join("") || `<p class="empty-copy">${state.loading ? "Loading retained API paths…" : "No loaded endpoints match these filters, origin search and origin products/versions."}</p>`}</div><div class="service-map-pagination"><button class="secondary-action" type="button" data-service-page="${state.page - 1}" ${!state.snapshot || state.page === 0 ? "disabled" : ""}>Previous</button><span>${entries.length ? start + 1 : 0}–${Math.min(start + API_PAGE_SIZE, entries.length)} / ${entries.length}</span><button class="secondary-action" type="button" data-service-page="${state.page + 1}" ${state.page + 1 >= pages ? "disabled" : ""}>Next</button></div>`);
    }
    function render() {
      renderStatus();
      renderProductFilters();
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
      if (data.servicePeerEvidence && data.servicePeerOrigin) loadPeerEvidence(data.servicePeerEvidence, data.servicePeerOrigin);
      else if (data.servicePeerMatches) loadPeerMatches();
      else if (data.serviceShowPeerMatches) showPeerMatchesOnMap();
      else if (data.serviceRelationOrigin) {
        selectOrigin(data.serviceRelationOrigin);
        state.detailTab = "evidence";
        if (data.serviceRelationEvidence) loadEvidence(data.serviceRelationEvidence);
        else renderDetail();
      } else if (data.serviceEndpoint !== undefined) {
        if (state.view === "apis") {
          const origin = findOrigin(data.serviceOrigin), endpoint = origin?.endpoints[Number(data.serviceEndpoint)];
          if (endpoint) state.pageAnchor = endpointIdentity({ origin, endpoint });
        }
        selectOrigin(data.serviceOrigin, Number(data.serviceEndpoint));
        if (state.view === "apis") { state.view = "service-map"; onViewChange?.("service-map"); render(); }
      } else if (data.serviceOrigin) selectOrigin(data.serviceOrigin);
      else if (data.serviceDetailTab) { state.detailTab = data.serviceDetailTab; renderDetail(); }
      else if (data.serviceEndpointFilterClear) setEndpointFilter("clear");
      else if (data.serviceProductFilterClear) setProductFilter("clear");
      else if (data.serviceEvidence) loadEvidence(data.serviceEvidence);
      else if (data.serviceBodyReferences) loadBodyReferences();
      else if (data.serviceLinkDeclarations) loadLinkDeclarations();
      else if (data.serviceShowLinkDeclarations) showLinkDeclarationsOnMap();
      else if (data.serviceHtmlGenerators) loadHtmlGenerators();
      else if (data.serviceShowBodyReferences) showBodyReferencesOnMap();
      else if (data.serviceClearBodyReferences) clearBodyReferenceOverlay();
      else if (data.servicePage !== undefined) { state.page = Math.max(0, Number(data.servicePage) || 0); state.pageAnchor = null; renderCatalog(); els.catalog.scrollTop = 0; }
      else if (data.serviceDetailPage !== undefined) { state.detailPage = Math.max(0, Number(data.serviceDetailPage) || 0); state.detailPageAnchor = null; renderDetail(); }
    }
    let onViewChange = null;
    els.panel.addEventListener("click", handleClick);
    els.panel.addEventListener("change", (event) => {
      if (event.target?.dataset?.serviceProductFilter) setProductFilter(event.target.dataset.serviceProductFilter, event.target.value);
      else if (event.target?.dataset?.serviceRelationshipKind) setRelationshipKind(event.target.value);
      else if (event.target?.dataset?.serviceEndpointFilter === "method") setEndpointFilter("method", event.target.value);
    });
    els.panel.addEventListener("input", (event) => {
      if (event.target?.dataset?.serviceEndpointFilter === "path") setEndpointFilter("path", event.target.value);
    });
    els.panel.addEventListener("keydown", (event) => {
      const tab = event.target.closest('[role="tab"][data-service-detail-tab]');
      if (!tab || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      state.detailTab = event.key === "Home" ? "apis" : event.key === "End" ? "evidence" : state.detailTab === "apis" ? "evidence" : "apis";
      renderDetail();
      const next = [...els.detail.querySelectorAll('[role="tab"][data-service-detail-tab]')].find((button) => button.dataset.serviceDetailTab === state.detailTab);
      next?.focus();
    });
    els.zoomFit?.addEventListener("click", fitGraph);
    els.zoomOut?.addEventListener("click", () => setZoom(state.zoomScale - ZOOM_LIMITS.step));
    els.zoomReset?.addEventListener("click", () => setZoom(1));
    els.zoomIn?.addEventListener("click", () => setZoom(state.zoomScale + ZOOM_LIMITS.step));
    els.viewport?.addEventListener("keydown", (event) => {
      if (event.target !== els.viewport || event.ctrlKey || event.metaKey || event.altKey) return;
      const handled = event.key === "+" || event.key === "=" ? setZoom(state.zoomScale + ZOOM_LIMITS.step)
        : event.key === "-" ? setZoom(state.zoomScale - ZOOM_LIMITS.step)
        : event.key === "0" ? setZoom(1) : event.key.toLowerCase() === "f" ? fitGraph() : false;
      if (handled) event.preventDefault();
    });
    // Resize only refreshes control availability; Fit is an explicit one-shot
    // action, so resizing or polling cannot silently change the user's zoom.
    if (els.viewport && typeof root.ResizeObserver === "function") new root.ResizeObserver(updateZoomControls).observe(els.viewport);
    els.refresh.addEventListener("click", () => refresh({ force: true, retry: true }));
    els.allOrigins?.addEventListener("click", () => setMapMode(false));
    els.directRelations?.addEventListener("click", () => setMapMode(true));
    els.focusSelection?.addEventListener("click", focusSelection);
    els.search.addEventListener("input", () => { state.query = els.search.value; state.page = 0; state.pageAnchor = null; render(); });
    els.scope.addEventListener("change", () => {
      if (state.scopeOnly === els.scope.checked) return;
      state.refreshPaused = false;
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
      refresh, reset, selectOrigin, setProductFilter, setZoom, fitGraph, loadEvidence, loadPeerEvidence, loadPeerMatches, showPeerMatchesOnMap, loadBodyReferences, loadLinkDeclarations, loadHtmlGenerators, setRelationshipKind, showBodyReferencesOnMap, showLinkDeclarationsOnMap, clearBodyReferenceOverlay,
      displayChanged() { renderGraph(); },
      scopeChanged(previous, next) {
        const signature = (runtime) => JSON.stringify([array(runtime?.scope_patterns), array(runtime?.excluded_scope_patterns)]);
        if (signature(previous) === signature(next)) return false;
        state.refreshPaused = false;
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
          cancelHtmlGenerators();
          cancelPeerMatches();
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
        return refresh();
      },
      // State access makes offline lifecycle tests use the same real controller.
      getState: () => state,
    };
  }
  root.SniperServiceMap = { createController, reportedProducts, productMatches, productSummaryMarkup, normalizeSnapshot, normalizeRowTechnologies, normalizeBodyReferences, normalizeHtmlGenerators, normalizeLinkDeclarations, normalizeSocketPeer, normalizePeerMatches, usableSocketPeer, peerGraphLayout, PEER_MATCH_LIMITS, LINK_DECLARATION_LIMITS, BODY_REFERENCE_LIMITS, HTML_GENERATOR_LIMITS, stableOrder, graphLayout, graphGeometry, safeOrigin, safePath, MAX_CARDS, MAX_EDGES, MAX_REFERENCE_CARDS, GRAPH_GEOMETRY, ZOOM_LIMITS };
})(typeof window === "undefined" ? globalThis : window);
