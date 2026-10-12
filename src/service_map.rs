//! Bounded, passive projections of retained HTTP metadata. No body reads, probes,
//! persistent indexes, or claims about unobserved upstream infrastructure.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{model::TransactionRecord, scope};

pub const DEFAULT_MAX_RECORDS: usize = 10_000;
pub const MAX_RECORDS: usize = 50_000;
pub const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ORIGINS: usize = 500;
pub const MAX_ENDPOINTS: usize = 2_000;
pub const MAX_ENDPOINTS_PER_ORIGIN: usize = 500;
pub const MAX_EVIDENCE: usize = 3;
pub const MAX_REDIRECTS_PER_ORIGIN: usize = 100;
pub const MAX_HEADER_LINKS_PER_ORIGIN: usize = 100;
pub const MAX_TECHNOLOGY_CLUES_PER_ROW: usize = 8;
pub const MAX_PATH_BYTES: usize = 2_048;
pub const MAX_HOST_BYTES: usize = 512;
pub const MAX_METHOD_BYTES: usize = 32;
pub const MAX_HEADERS: usize = 128;
pub const MAX_HEADER_BYTES: usize = 32 * 1024;
pub const MAX_HEADER_VALUE_BYTES: usize = 4_096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapLimits {
    pub max_records: usize,
    pub metadata_bytes: usize,
    pub origins: usize,
    pub endpoints: usize,
    pub endpoints_per_origin: usize,
    pub evidence_per_item: usize,
    pub redirects_per_origin: usize,
    pub header_links_per_origin: usize,
    pub technology_clues_per_row: usize,
    pub statuses_per_endpoint: usize,
    pub content_types_per_endpoint: usize,
    pub path_bytes: usize,
    pub host_bytes: usize,
    pub method_bytes: usize,
    pub headers_per_row: usize,
    pub header_bytes_per_row: usize,
    pub header_value_bytes: usize,
}

impl ServiceMapLimits {
    fn new(max_records: usize) -> Self {
        Self {
            max_records,
            metadata_bytes: MAX_METADATA_BYTES,
            origins: MAX_ORIGINS,
            endpoints: MAX_ENDPOINTS,
            endpoints_per_origin: MAX_ENDPOINTS_PER_ORIGIN,
            evidence_per_item: MAX_EVIDENCE,
            redirects_per_origin: MAX_REDIRECTS_PER_ORIGIN,
            header_links_per_origin: MAX_HEADER_LINKS_PER_ORIGIN,
            technology_clues_per_row: MAX_TECHNOLOGY_CLUES_PER_ROW,
            statuses_per_endpoint: 32,
            content_types_per_endpoint: 16,
            path_bytes: MAX_PATH_BYTES,
            host_bytes: MAX_HOST_BYTES,
            method_bytes: MAX_METHOD_BYTES,
            headers_per_row: MAX_HEADERS,
            header_bytes_per_row: MAX_HEADER_BYTES,
            header_value_bytes: MAX_HEADER_VALUE_BYTES,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapSnapshot {
    pub window_order: String,
    /// A fresh response ID; this is not a persistent cache or replayable cursor.
    pub id: Uuid,
    /// Store incarnation. Reopening an inactive session may invalidate evidence.
    pub generation: Uuid,
    pub captured_at: DateTime<Utc>,
    pub first_retained_started_at: Option<DateTime<Utc>>,
    pub last_retained_started_at: Option<DateTime<Utc>>,
    pub oldest_considered_at: Option<DateTime<Utc>>,
    pub latest_considered_at: Option<DateTime<Utc>>,
    pub retained_rows: usize,
    pub considered_rows: usize,
    pub non_http_rows: usize,
    pub projected_rows: usize,
    pub included_rows: usize,
    pub excluded_rows: usize,
    pub omitted_rows: usize,
    pub origin_count: usize,
    pub endpoint_count: usize,
    pub complete: bool,
    pub stopped_by: Vec<String>,
    pub limits: ServiceMapLimits,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMap {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub snapshot: ServiceMapSnapshot,
    pub in_scope_only: bool,
    pub origin_filter: Option<String>,
    pub origins: Vec<ServiceOrigin>,
    pub unavailable: Vec<String>,
    pub deferred: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceOrigin {
    pub origin: String,
    pub host: String,
    pub scheme: String,
    pub port: u16,
    pub in_scope: bool,
    /// Captured rows, not distinct UUIDs or inferred calls.
    pub request_count: usize,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub endpoints: Vec<ServiceEndpoint>,
    pub technology_clues: Vec<TechnologyClue>,
    pub redirects: Vec<RedirectInstruction>,
    pub header_links: Vec<HeaderLink>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    pub method: String,
    /// Exact captured path without query or fragment. Never a guessed route.
    pub path: String,
    /// A display hint only; exact endpoints are never merged by this value.
    pub path_group: Option<String>,
    pub request_count: usize,
    pub statuses: Vec<u16>,
    pub content_types: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TechnologyClue {
    pub name: String,
    pub source: String,
    pub header_name: String,
    pub confidence: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedirectInstruction {
    pub target_origin: String,
    pub target_path: String,
    pub relationship: String,
    pub source: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderLink {
    pub source_origin: String,
    pub relationship: String,
    pub source: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderLinkSource {
    pub source_origin: String,
    pub relationship: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderClue {
    pub name: String,
    pub header_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedirectTarget {
    pub target_origin: String,
    pub target_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapRow {
    // URL canonicalization can compress IPv6 or encode IDNs differently from
    // the shared history matcher. Retain the bounded captured authority only
    // for internal scope decisions; never serialize potential userinfo.
    #[serde(skip)]
    scope_host: String,
    pub evidence_id: String,
    pub record_id: Uuid,
    /// String preserves row identity in JavaScript beyond 2^53.
    pub sequence: String,
    pub started_at: DateTime<Utc>,
    pub method: String,
    pub origin: String,
    pub host: String,
    pub scheme: String,
    pub port: u16,
    pub path: String,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub is_websocket: bool,
    pub technology_clues: Vec<HeaderClue>,
    pub redirect: Option<RedirectTarget>,
    pub header_links: Vec<HeaderLinkSource>,
    pub metadata_complete: bool,
    pub omitted: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapEvidence {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub evidence_id: String,
    pub available: bool,
    pub reason: Option<String>,
    pub source: String,
    pub row: Option<ServiceMapRow>,
    /// Explicit exact-source extension. Omitted from the default v1 response and
    /// never part of the aggregate metadata projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_peer: Option<ServiceMapSocketPeerEvidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceMapSocketPeerStatus {
    Observed,
    NotRecorded,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceMapSocketPeerEvidence {
    pub status: ServiceMapSocketPeerStatus,
    pub observation: Option<crate::model::ObservedSocketPeer>,
}

impl ServiceMapSocketPeerEvidence {
    pub(crate) fn from_record(record: &TransactionRecord) -> Self {
        let observation = record
            .observed_socket_peer
            .as_ref()
            .filter(|observation| observation.is_valid())
            .cloned();
        let status = match observation.as_ref() {
            Some(observation) if observation.address.is_some() => {
                ServiceMapSocketPeerStatus::Observed
            }
            Some(_) => ServiceMapSocketPeerStatus::Unavailable,
            None => ServiceMapSocketPeerStatus::NotRecorded,
        };
        Self {
            status,
            observation,
        }
    }
}

impl ServiceMapEvidence {
    pub(crate) fn missing(session_id: Uuid, evidence_id: String, reason: &str) -> Self {
        Self {
            schema_version: 1,
            session_id,
            evidence_id,
            available: false,
            reason: Some(reason.into()),
            source: "captured_http_metadata".into(),
            row: None,
            socket_peer: None,
        }
    }

    pub(crate) fn found(session_id: Uuid, row: ServiceMapRow) -> Self {
        Self {
            schema_version: 1,
            session_id,
            evidence_id: row.evidence_id.clone(),
            available: true,
            reason: None,
            source: "captured_http_metadata".into(),
            row: Some(row),
            socket_peer: None,
        }
    }
}

pub(crate) struct MetadataSnapshot {
    pub generation: Uuid,
    pub captured_at: DateTime<Utc>,
    pub first_retained_started_at: Option<DateTime<Utc>>,
    pub last_retained_started_at: Option<DateTime<Utc>>,
    pub oldest_considered_at: Option<DateTime<Utc>>,
    pub latest_considered_at: Option<DateTime<Utc>>,
    pub retained_rows: usize,
    pub considered_rows: usize,
    pub non_http_rows: usize,
    pub omitted_rows: usize,
    pub stopped_by: BTreeSet<String>,
    pub rows: Vec<ServiceMapRow>,
}

#[derive(Debug)]
pub(crate) struct EvidenceKey {
    pub session_id: Uuid,
    pub generation: Uuid,
    pub sequence: u64,
    pub revision: Uuid,
    pub record_id: Uuid,
}

impl EvidenceKey {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.len() > 200 {
            return None;
        }
        let fields: Vec<_> = value.split('.').collect();
        if fields.len() != 6 || fields[0] != "v1" {
            return None;
        }
        let sequence = fields[3].parse::<u64>().ok()?;
        if sequence.to_string() != fields[3] {
            return None;
        }
        Some(Self {
            session_id: fields[1].parse().ok()?,
            generation: fields[2].parse().ok()?,
            sequence,
            revision: fields[4].parse().ok()?,
            record_id: fields[5].parse().ok()?,
        })
    }
}

pub(crate) fn evidence_id(
    session_id: Uuid,
    generation: Uuid,
    revision: Uuid,
    record: &TransactionRecord,
) -> String {
    format!(
        "v1.{session_id}.{generation}.{}.{revision}.{}",
        record.sequence, record.id
    )
}

/// Normalize an HTTP(S) URL to a safe origin. Filter callers also require exact
/// equality with the input so credentials, paths and queries are not accepted.
/// URL's origin serialization never includes userinfo.
pub fn canonical_origin(value: &str) -> Option<String> {
    if value.len() > MAX_HOST_BYTES + 32 {
        return None;
    }
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

fn request_target(record: &TransactionRecord) -> Option<(Url, String)> {
    if record.scheme.len() > 8
        || record.host.len() > MAX_HOST_BYTES
        || record.path.len() > MAX_PATH_BYTES + MAX_HEADER_VALUE_BYTES
        || record.method.is_empty()
        || record.method.len() > MAX_METHOD_BYTES
        || !record
            .method
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        || record.host.contains(['/', '?', '#', '\\'])
        || record
            .host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let scheme = record.scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return None;
    }
    let base = Url::parse(&format!("{scheme}://{}/", record.host)).ok()?;
    if base.host_str()?.len() > MAX_HOST_BYTES {
        return None;
    }
    let path = record.path.split(['?', '#']).next().unwrap_or("");
    if !path.starts_with('/') || path.len() > MAX_PATH_BYTES || path.chars().any(char::is_control) {
        return None;
    }
    Some((base, path.to_string()))
}

fn mime(value: &str) -> Option<String> {
    if value.len() > MAX_HEADER_VALUE_BYTES {
        return None;
    }
    let value = value.split(';').next()?.trim();
    if value.len() > 128
        || !value.contains('/')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'+' | b'_'))
    {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn product(value: &str, expected: &str) -> bool {
    value
        .get(..expected.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected))
        && value
            .as_bytes()
            .get(expected.len())
            .is_none_or(|b| matches!(b, b'/' | b' ' | b';' | b','))
}

pub(crate) fn project_record(record: &TransactionRecord, id: String) -> Option<ServiceMapRow> {
    if record.method == "CONNECT" {
        return None;
    }
    let (base, path) = request_target(record)?;
    let origin = base.origin().ascii_serialization();
    let host = base.host_str()?.trim_matches(['[', ']']).to_string();
    let mut row = ServiceMapRow {
        scope_host: record.host.clone(),
        evidence_id: id,
        record_id: record.id,
        sequence: record.sequence.to_string(),
        started_at: record.started_at,
        method: record.method.clone(),
        origin,
        host,
        scheme: base.scheme().into(),
        port: base.port_or_known_default()?,
        path,
        status: record.status,
        content_type: record
            .response
            .as_ref()
            .and_then(|m| m.content_type.as_deref())
            .and_then(mime),
        is_websocket: false,
        technology_clues: vec![],
        redirect: None,
        header_links: vec![],
        metadata_complete: true,
        omitted: vec![],
    };
    if record
        .response
        .as_ref()
        .and_then(|m| m.content_type.as_ref())
        .is_some()
        && row.content_type.is_none()
    {
        row.omitted.push("unsupported_content_type".into());
    }
    let mut request_header_bytes = 0usize;
    let mut request_headers_scanned = 0usize;
    let mut websocket_upgrade = false;
    let mut connection_upgrade = false;
    let mut origin_values = Vec::new();
    let mut referer_values = Vec::new();
    for (index, header) in record.request.headers.iter().enumerate() {
        request_header_bytes = request_header_bytes
            .saturating_add(header.name.len())
            .saturating_add(header.value.len());
        if index >= MAX_HEADERS || request_header_bytes > MAX_HEADER_BYTES {
            row.omitted.push("request_header_scan_limit".into());
            break;
        }
        request_headers_scanned += 1;
        if header.value.len() > MAX_HEADER_VALUE_BYTES {
            row.omitted.push("request_header_value_limit".into());
            continue;
        }
        if header.name.eq_ignore_ascii_case("upgrade") {
            websocket_upgrade |= header
                .value
                .split(',')
                .any(|value| value.trim().eq_ignore_ascii_case("websocket"));
        }
        if header.name.eq_ignore_ascii_case("connection") {
            connection_upgrade |= header
                .value
                .split(',')
                .any(|value| value.trim().eq_ignore_ascii_case("upgrade"));
        }
        if header.name.eq_ignore_ascii_case("origin") {
            origin_values.push(header.value.as_str());
        }
        if header.name.eq_ignore_ascii_case("referer") {
            referer_values.push(header.value.as_str());
        }
    }
    row.is_websocket = record.status == Some(101) && websocket_upgrade && connection_upgrade;
    if !row
        .omitted
        .iter()
        .any(|reason| reason.starts_with("request_header_"))
    {
        for (values, relationship) in [
            (&origin_values, "origin_header"),
            (&referer_values, "referer_header"),
        ] {
            if values.len() > 1 {
                row.omitted.push(format!("ambiguous_{relationship}"));
            } else if let Some(value) = values.first() {
                if let Some(source_origin) =
                    declared_header_origin(value, relationship == "origin_header")
                {
                    row.header_links.push(HeaderLinkSource {
                        source_origin,
                        relationship: relationship.into(),
                    });
                }
            }
        }
    }
    let Some(response) = &record.response else {
        row.metadata_complete = row.omitted.is_empty();
        return Some(row);
    };
    let mut header_bytes = request_header_bytes;
    let mut locations = Vec::new();
    for (index, header) in response.headers.iter().enumerate() {
        header_bytes = header_bytes
            .saturating_add(header.name.len())
            .saturating_add(header.value.len());
        if index.saturating_add(request_headers_scanned) >= MAX_HEADERS
            || header_bytes > MAX_HEADER_BYTES
        {
            row.omitted.push("header_scan_limit".into());
            break;
        }
        if header.value.len() > MAX_HEADER_VALUE_BYTES {
            row.omitted.push("header_value_limit".into());
            continue;
        }
        let header_name = header.name.to_ascii_lowercase();
        if header_name == "location" {
            locations.push(header.value.as_str());
        }
        let products: &[(&str, &str)] = match header_name.as_str() {
            "server" => &[
                ("nginx", "nginx"),
                ("apache", "Apache"),
                ("caddy", "Caddy"),
                ("cloudflare", "Cloudflare"),
                ("microsoft-iis", "Microsoft IIS"),
                ("envoy", "Envoy"),
            ],
            "x-powered-by" => &[
                ("php", "PHP"),
                ("express", "Express"),
                ("asp.net", "ASP.NET"),
            ],
            "x-generator" => &[("wordpress", "WordPress"), ("drupal", "Drupal")],
            _ => &[],
        };
        for (prefix, name) in products {
            if product(header.value.trim(), prefix)
                && !row
                    .technology_clues
                    .iter()
                    .any(|clue| clue.name == *name && clue.header_name == header_name)
            {
                if row.technology_clues.len() < MAX_TECHNOLOGY_CLUES_PER_ROW {
                    row.technology_clues.push(HeaderClue {
                        name: (*name).into(),
                        header_name: header_name.clone(),
                    });
                } else {
                    row.omitted.push("technology_clues_limit".into());
                }
            }
        }
    }
    if record
        .status
        .is_some_and(|status| matches!(status, 300 | 301 | 302 | 303 | 307 | 308))
    {
        if locations.len() > 1 {
            row.omitted.push("ambiguous_location_headers".into());
        } else if let Some(location) = locations.first() {
            // A partial header scan cannot prove there was only one Location.
            if !row
                .omitted
                .iter()
                .any(|reason| reason == "header_scan_limit" || reason == "header_value_limit")
            {
                let request_url = Url::parse(&format!("{}{}", row.origin, row.path)).ok();
                let target = request_url.and_then(|url| url.join(location.trim()).ok());
                match target.filter(|url| {
                    matches!(url.scheme(), "http" | "https")
                        && url.path().len() <= MAX_PATH_BYTES
                        && url
                            .host_str()
                            .is_some_and(|host| host.len() <= MAX_HOST_BYTES)
                }) {
                    Some(url) => {
                        row.redirect = Some(RedirectTarget {
                            target_origin: url.origin().ascii_serialization(),
                            target_path: url.path().into(),
                        })
                    }
                    None => row.omitted.push("unsupported_location".into()),
                }
            }
        }
    }
    row.omitted.sort();
    row.omitted.dedup();
    row.metadata_complete = row.omitted.is_empty();
    Some(row)
}

fn declared_header_origin(value: &str, origin_header: bool) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_HEADER_VALUE_BYTES
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return None;
    }
    if !(value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://")))
        || value.contains('\\')
    {
        return None;
    }
    if origin_header {
        // URL parsing repairs extra authority slashes and removes dot segments,
        // so validate the declared structure before normalization. Keep the
        // existing allowance for one trailing slash, but never raw userinfo.
        let (_, authority) = value.split_once("://")?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority.is_empty() || authority.contains(['/', '?', '#', '@', ',']) {
            return None;
        }
    }
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str()?.len() > MAX_HOST_BYTES {
        return None;
    }
    // Origin is one origin, not an arbitrary URL or an origin-list. Reject
    // credentials there; Referer may contain userinfo, which is never returned.
    if origin_header
        && (url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some())
    {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

pub fn path_group(path: &str) -> Option<String> {
    let mut changed = false;
    let grouped = path
        .split('/')
        .map(|part| {
            if !part.is_empty() && part.len() <= 18 && part.bytes().all(|b| b.is_ascii_digit()) {
                changed = true;
                "{number}"
            } else if part.len() == 36
                && Uuid::parse_str(part).is_ok()
                && part.as_bytes()[8] == b'-'
                && part.as_bytes()[13] == b'-'
                && part.as_bytes()[18] == b'-'
                && part.as_bytes()[23] == b'-'
            {
                changed = true;
                "{uuid}"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    changed.then_some(grouped)
}

fn sample(values: &mut Vec<String>, id: &str) {
    if values.len() < MAX_EVIDENCE {
        values.push(id.into());
    }
}

pub(crate) fn build(
    session_id: Uuid,
    snapshot: MetadataSnapshot,
    max_records: usize,
    in_scope_only: bool,
    origin_filter: Option<String>,
    include: &[String],
    exclude: &[String],
) -> ServiceMap {
    let projected_rows = snapshot.rows.len();
    let mut stopped_by = snapshot.stopped_by;
    let mut origins = BTreeMap::<String, ServiceOrigin>::new();
    let mut endpoint_indexes = BTreeMap::<(String, String, String), usize>::new();
    let mut included_rows = 0;
    let mut excluded_rows = 0;
    let mut omitted_rows = snapshot.omitted_rows;
    for row in snapshot.rows {
        let in_scope = scope::is_host_in_scope(&row.scope_host, include, exclude);
        if (in_scope_only && !in_scope)
            || origin_filter
                .as_ref()
                .is_some_and(|origin| origin != &row.origin)
        {
            excluded_rows += 1;
            continue;
        }
        for reason in &row.omitted {
            stopped_by.insert(reason.clone());
        }
        if !origins.contains_key(&row.origin) && origins.len() == MAX_ORIGINS {
            stopped_by.insert("origins_limit".into());
            omitted_rows += 1;
            continue;
        }
        let origin = origins
            .entry(row.origin.clone())
            .or_insert_with(|| ServiceOrigin {
                origin: row.origin.clone(),
                host: row.host.clone(),
                scheme: row.scheme.clone(),
                port: row.port,
                in_scope,
                request_count: 0,
                first_seen: row.started_at,
                last_seen: row.started_at,
                endpoints: vec![],
                technology_clues: vec![],
                redirects: vec![],
                header_links: vec![],
            });
        origin.in_scope |= in_scope;
        origin.request_count += 1;
        origin.first_seen = origin.first_seen.min(row.started_at);
        origin.last_seen = origin.last_seen.max(row.started_at);
        included_rows += 1;
        let key = (row.origin.clone(), row.method.clone(), row.path.clone());
        let endpoint_index = if let Some(index) = endpoint_indexes.get(&key) {
            Some(*index)
        } else if endpoint_indexes.len() >= MAX_ENDPOINTS
            || origin.endpoints.len() >= MAX_ENDPOINTS_PER_ORIGIN
        {
            stopped_by.insert("endpoints_limit".into());
            None
        } else {
            let index = origin.endpoints.len();
            endpoint_indexes.insert(key, index);
            origin.endpoints.push(ServiceEndpoint {
                method: row.method.clone(),
                path: row.path.clone(),
                path_group: path_group(&row.path),
                request_count: 0,
                statuses: vec![],
                content_types: vec![],
                evidence: vec![],
            });
            Some(index)
        };
        if let Some(index) = endpoint_index {
            let endpoint = &mut origin.endpoints[index];
            endpoint.request_count += 1;
            if let Some(status) = row.status {
                if !endpoint.statuses.contains(&status) {
                    if endpoint.statuses.len() < 32 {
                        endpoint.statuses.push(status);
                    } else {
                        stopped_by.insert("statuses_limit".into());
                    }
                }
            }
            if let Some(mime) = &row.content_type {
                if !endpoint.content_types.contains(mime) {
                    if endpoint.content_types.len() < 16 {
                        endpoint.content_types.push(mime.clone());
                    } else {
                        stopped_by.insert("content_types_limit".into());
                    }
                }
            }
            sample(&mut endpoint.evidence, &row.evidence_id);
        }
        for clue in row.technology_clues {
            if let Some(existing) = origin
                .technology_clues
                .iter_mut()
                .find(|c| c.name == clue.name && c.header_name == clue.header_name)
            {
                sample(&mut existing.evidence, &row.evidence_id);
            } else {
                origin.technology_clues.push(TechnologyClue {
                    name: clue.name,
                    header_name: clue.header_name,
                    source: "response_header".into(),
                    confidence: "observed_header".into(),
                    evidence: vec![row.evidence_id.clone()],
                });
            }
        }
        for link in row.header_links {
            if let Some(existing) = origin.header_links.iter_mut().find(|r| {
                r.source_origin == link.source_origin && r.relationship == link.relationship
            }) {
                sample(&mut existing.evidence, &row.evidence_id);
            } else if origin.header_links.len() < MAX_HEADER_LINKS_PER_ORIGIN {
                origin.header_links.push(HeaderLink {
                    source_origin: link.source_origin,
                    relationship: link.relationship,
                    source: "request_header_declaration".into(),
                    evidence: vec![row.evidence_id.clone()],
                });
            } else {
                stopped_by.insert("header_links_limit".into());
            }
        }
        if let Some(target) = row.redirect {
            if let Some(existing) = origin.redirects.iter_mut().find(|r| {
                r.target_origin == target.target_origin && r.target_path == target.target_path
            }) {
                sample(&mut existing.evidence, &row.evidence_id);
            } else if origin.redirects.len() < MAX_REDIRECTS_PER_ORIGIN {
                origin.redirects.push(RedirectInstruction {
                    target_origin: target.target_origin,
                    target_path: target.target_path,
                    relationship: "redirect_instruction".into(),
                    source: "response_location_header".into(),
                    evidence: vec![row.evidence_id.clone()],
                });
            } else {
                stopped_by.insert("redirects_limit".into());
            }
        }
    }
    let mut origins: Vec<_> = origins.into_values().collect();
    for origin in &mut origins {
        origin
            .endpoints
            .sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
        for endpoint in &mut origin.endpoints {
            endpoint.statuses.sort_unstable();
            endpoint.content_types.sort();
        }
        origin
            .technology_clues
            .sort_by(|a, b| (&a.name, &a.header_name).cmp(&(&b.name, &b.header_name)));
        origin.header_links.sort_by(|a, b| {
            (&a.source_origin, &a.relationship).cmp(&(&b.source_origin, &b.relationship))
        });
        origin.redirects.sort_by(|a, b| {
            (&a.target_origin, &a.target_path).cmp(&(&b.target_origin, &b.target_path))
        });
    }
    ServiceMap { schema_version: 1, session_id,
        snapshot: ServiceMapSnapshot {
            window_order: "newest_capture_sequence".into(), id: Uuid::new_v4(), generation: snapshot.generation,
            captured_at: snapshot.captured_at,
            first_retained_started_at: snapshot.first_retained_started_at, last_retained_started_at: snapshot.last_retained_started_at,
            oldest_considered_at: snapshot.oldest_considered_at, latest_considered_at: snapshot.latest_considered_at,
            retained_rows: snapshot.retained_rows,
            considered_rows: snapshot.considered_rows,
            non_http_rows: snapshot.non_http_rows, projected_rows, included_rows, excluded_rows, omitted_rows,
            origin_count: origins.len(), endpoint_count: endpoint_indexes.len(), complete: stopped_by.is_empty(),
            stopped_by: stopped_by.into_iter().collect(), limits: ServiceMapLimits::new(max_records) },
        in_scope_only, origin_filter, origins,
        unavailable: vec!["initiator".into(), "remote_ip".into(), "tls_certificate".into()],
        deferred: vec!["response_body_references".into()],
        warnings: vec!["Counts describe analyzed rows in a newest-capture-sequence window, not all-time traffic. Removed or uncaptured traffic is unavailable; retained first/last timestamps are capture-order boundaries.".into(),
            "Header clues do not prove software identity or a backend chain. Redirect instructions and Origin/Referer declarations do not prove a follow-up or initiating call.".into(),
            "Evidence samples are bounded. Queries, fragments, userinfo, raw headers, and bodies are omitted. Origin in_scope means at least one analyzed row matches scope using its captured host.".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{HeaderRecord, MessageRecord},
        store::TransactionStore,
    };

    fn record(path: &str) -> TransactionRecord {
        let message =
            MessageRecord::from_headers_and_body(&http::HeaderMap::new(), b"BODY_SENTINEL", 1024);
        TransactionRecord::http(
            Utc::now(),
            "GET".into(),
            "https".into(),
            "example.com".into(),
            path.into(),
            Some(200),
            1,
            message.clone(),
            Some(message),
            vec![],
            None,
            None,
        )
    }

    fn headers(values: &[(&str, &str)]) -> Vec<HeaderRecord> {
        values
            .iter()
            .map(|(name, value)| HeaderRecord {
                name: (*name).into(),
                value: (*value).into(),
            })
            .collect()
    }

    async fn map(records: Vec<TransactionRecord>) -> ServiceMap {
        let id = Uuid::new_v4();
        let store = TransactionStore::from_records(records);
        build(
            id,
            store.service_map_snapshot(id, DEFAULT_MAX_RECORDS).await,
            DEFAULT_MAX_RECORDS,
            false,
            None,
            &[],
            &[],
        )
    }

    #[tokio::test]
    async fn service_map_never_serializes_credentials_bodies_or_arbitrary_header_values() {
        let mut row = record("/login?password=QUERY_SENTINEL#FRAGMENT_SENTINEL");
        row.host = "USER_SENTINEL:PASS_SENTINEL@example.com:443".into();
        row.status = Some(302);
        row.request.headers = headers(&[("Authorization", "Bearer AUTH_SENTINEL"),
            ("Referer", "https://REFUSER_SENTINEL:REFPASS_SENTINEL@source.example.com/private?token=REFQUERY_SENTINEL#REFHASH_SENTINEL")]);
        row.response.as_mut().unwrap().headers = headers(&[("Server", "nginx/SECRET_VERSION_SENTINEL"),
            ("Set-Cookie", "session=COOKIE_SENTINEL"),
            ("Location", "https://REDIRUSER_SENTINEL:REDIRPASS_SENTINEL@other.example.com/next?token=REDIRQUERY_SENTINEL#REDIRHASH_SENTINEL")]);
        let result = map(vec![row]).await;
        let json = serde_json::to_string(&result).unwrap();
        assert!(
            !json.contains("SENTINEL"),
            "safe projection leaked synthetic sensitive data"
        );
        let origin = &result.origins[0];
        assert_eq!(origin.origin, "https://example.com");
        assert_eq!(origin.endpoints[0].path, "/login");
        assert_eq!(origin.technology_clues[0].name, "nginx");
        assert_eq!(
            origin.redirects[0].target_origin,
            "https://other.example.com"
        );
        assert_eq!(origin.redirects[0].target_path, "/next");
        assert_eq!(origin.redirects[0].relationship, "redirect_instruction");
        assert_eq!(
            origin.header_links[0].source_origin,
            "https://source.example.com"
        );
        assert_eq!(result.snapshot.included_rows, 1);
        assert_eq!(result.snapshot.origin_count, 1);
    }

    #[tokio::test]
    async fn service_map_preserves_exact_methods_paths_and_only_hints_at_groups() {
        let mut lower = record("/users/12?x=1");
        lower.method = "get".into();
        let mut custom = record("/users/12?x=2");
        custom.method = "M-SEARCH2!".into();
        let result = map(vec![
            record("/users/12"),
            record("/users/34"),
            lower,
            custom,
            record("/users/550e8400-e29b-41d4-a716-446655440000"),
            record("/static/v2"),
        ])
        .await;
        let endpoints = &result.origins[0].endpoints;
        assert_eq!(endpoints.len(), 6);
        assert!(endpoints.iter().any(|e| e.method == "get"));
        assert!(endpoints.iter().any(|e| e.method == "M-SEARCH2!"));
        assert_eq!(path_group("/users/12"), Some("/users/{number}".into()));
        assert_eq!(
            path_group("/users/550e8400-e29b-41d4-a716-446655440000"),
            Some("/users/{uuid}".into())
        );
        assert_eq!(path_group("/static/v2/0x10/1234567890123456789"), None);
    }

    #[tokio::test]
    async fn service_map_duplicate_uuid_evidence_resolves_exact_retained_row() {
        let session = Uuid::new_v4();
        let old = record("/old");
        let mut new = old.clone();
        new.path = "/new".into();
        let store = TransactionStore::from_records(vec![new.clone(), old]);
        let snapshot = store.service_map_snapshot(session, 10).await;
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(store.get(new.id).await.unwrap().path, "/new");
        for row in snapshot.rows {
            let evidence = store.service_map_evidence(session, &row.evidence_id).await;
            assert!(evidence.available);
            assert_eq!(evidence.row.unwrap().path, row.path);
        }
        let result = build(
            session,
            store.service_map_snapshot(session, 10).await,
            10,
            false,
            None,
            &[],
            &[],
        );
        assert_eq!(result.origins[0].request_count, 2);
        assert_eq!(result.origins[0].endpoints.len(), 2);
    }

    #[tokio::test]
    async fn socket_peer_extension_is_explicit_exact_and_never_part_of_map_projection() {
        use crate::model::{ObservedSocketPeer, ObservedSocketPeerRoute};
        let session = Uuid::new_v4();
        let mut observed = record("/observed");
        observed.observed_socket_peer = Some(ObservedSocketPeer::from_remote_addr(
            observed.started_at,
            ObservedSocketPeerRoute::SocksProxy,
            Some("[2001:db8::1]:1080".parse().unwrap()),
        ));
        let mut unavailable = record("/unavailable");
        unavailable.observed_socket_peer = Some(ObservedSocketPeer::from_remote_addr(
            unavailable.started_at,
            ObservedSocketPeerRoute::Unknown,
            None,
        ));
        let mut invalid = observed.clone();
        invalid.id = Uuid::new_v4();
        invalid.path = "/invalid".into();
        invalid.observed_socket_peer.as_mut().unwrap().reason =
            Some(crate::model::ObservedSocketPeerReason::RemoteAddrUnavailable);
        let store =
            TransactionStore::from_records(vec![observed, unavailable, invalid, record("/legacy")]);
        let snapshot = store.service_map_snapshot(session, 10).await;
        for row in &snapshot.rows {
            assert!(!serde_json::to_string(row).unwrap().contains("socket_peer"));
            let default = store.service_map_evidence(session, &row.evidence_id).await;
            assert!(default.socket_peer.is_none());
            assert!(serde_json::to_value(default)
                .unwrap()
                .get("socket_peer")
                .is_none());
            let explicit = store
                .service_map_evidence_with_socket_peer(session, &row.evidence_id, true)
                .await;
            let extension = explicit.socket_peer.unwrap();
            match row.path.as_str() {
                "/observed" => {
                    assert_eq!(extension.status, ServiceMapSocketPeerStatus::Observed);
                    let observation = extension.observation.unwrap();
                    assert_eq!(observation.route, ObservedSocketPeerRoute::SocksProxy);
                    assert_eq!(observation.address.unwrap().ip.to_string(), "2001:db8::1");
                }
                "/unavailable" => {
                    assert_eq!(extension.status, ServiceMapSocketPeerStatus::Unavailable);
                    assert!(extension.observation.unwrap().address.is_none());
                }
                _ => {
                    assert_eq!(extension.status, ServiceMapSocketPeerStatus::NotRecorded);
                    assert!(extension.observation.is_none());
                }
            }
            let wrong_session = store
                .service_map_evidence_with_socket_peer(Uuid::new_v4(), &row.evidence_id, true)
                .await;
            assert!(!wrong_session.available);
            assert!(wrong_session.socket_peer.is_none());
        }
        let projection = build(session, snapshot, 10, false, None, &[], &[]);
        let json = serde_json::to_string(&projection).unwrap();
        assert!(!json.contains("socket_peer"));
        assert!(!json.contains("2001:db8::1"));
        assert!(projection
            .unavailable
            .iter()
            .any(|field| field == "remote_ip"));
    }

    #[tokio::test]
    async fn service_map_evidence_invalidates_updated_trimmed_and_reconstructed_rows() {
        let session = Uuid::new_v4();
        let row = record("/before");
        let id = row.id;
        let store = TransactionStore::from_records_with_max_entries(vec![row.clone()], Some(2));
        let token = store.service_map_snapshot(session, 10).await.rows[0]
            .evidence_id
            .clone();
        assert_eq!(
            store
                .service_map_evidence(Uuid::new_v4(), &token)
                .await
                .reason
                .as_deref(),
            Some("session_mismatch")
        );
        store.insert(record("/other")).await;
        assert!(store.service_map_evidence(session, &token).await.available);
        store
            .update_record(id, |row| row.path = "/after".into())
            .await;
        assert_eq!(
            store
                .service_map_evidence(session, &token)
                .await
                .reason
                .as_deref(),
            Some("record_changed")
        );
        let current = store
            .service_map_snapshot(session, 10)
            .await
            .rows
            .into_iter()
            .find(|r| r.record_id == id)
            .unwrap()
            .evidence_id;
        store.insert(record("/trim")).await;
        assert_eq!(
            store
                .service_map_evidence(session, &current)
                .await
                .reason
                .as_deref(),
            Some("record_missing")
        );
        let reconstructed = TransactionStore::from_records(vec![row]);
        assert_eq!(
            reconstructed
                .service_map_evidence(session, &token)
                .await
                .reason
                .as_deref(),
            Some("stale_generation")
        );
        assert_eq!(
            store
                .service_map_evidence(session, "bad")
                .await
                .reason
                .as_deref(),
            Some("invalid_evidence_id")
        );
    }

    #[tokio::test]
    async fn service_map_durable_deletion_removes_exact_evidence_without_uuid_fallback() {
        let dir =
            std::env::temp_dir().join(format!("sniper-service-map-delete-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let row = record("/gone");
        let id = row.id;
        let session = Uuid::new_v4();
        let store = TransactionStore::from_records_with_journal(
            vec![row],
            dir.join("transactions.journal"),
            None,
        );
        let token = store.service_map_snapshot(session, 10).await.rows[0]
            .evidence_id
            .clone();
        store.delete_ids(&[id]).await.unwrap();
        assert_eq!(
            store
                .service_map_evidence(session, &token)
                .await
                .reason
                .as_deref(),
            Some("record_missing")
        );
        assert_eq!(
            store.service_map_snapshot(session, 10).await.retained_rows,
            0
        );
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn service_map_scope_uses_shared_include_exclude_and_origin_filter() {
        let mut a = record("/a");
        a.host = "API.Example.COM.:443".into();
        let mut b = record("/b");
        b.host = "excluded.example.com".into();
        let mut c = record("/c");
        c.host = "[::1]:8443".into();
        let store = TransactionStore::from_records(vec![a, b, c]);
        let id = Uuid::new_v4();
        let result = build(
            id,
            store.service_map_snapshot(id, 100).await,
            100,
            true,
            None,
            &["*.example.com".into()],
            &["excluded.example.com".into()],
        );
        assert_eq!(result.origins.len(), 1);
        assert!(result.origins[0].in_scope);
        assert_eq!(result.snapshot.excluded_rows, 2);
        let filtered = build(
            id,
            store.service_map_snapshot(id, 100).await,
            100,
            false,
            Some("https://[::1]:8443".into()),
            &[],
            &[],
        );
        assert_eq!(filtered.origins.len(), 1);
        assert_eq!(filtered.origins[0].port, 8443);
    }

    #[tokio::test]
    async fn service_map_header_declarations_work_without_responses_and_reject_ambiguity() {
        let mut row = record("/");
        row.response = None;
        row.status = None;
        row.request.headers = headers(&[
            ("Origin", "https://source.example.com"),
            ("Referer", "https://source.example.com/path?q=secret"),
        ]);
        let projected = project_record(&row, "test".into()).unwrap();
        assert_eq!(projected.header_links.len(), 2);
        for value in [
            "null",
            "https://a.example.com https://b.example.com",
            "javascript:alert(1)",
            "/relative",
            "https://a.example.com/path",
            "https://user:secret@a.example.com",
        ] {
            assert!(declared_header_origin(value, true).is_none());
        }
        row.request.headers.push(HeaderRecord {
            name: "Origin".into(),
            value: "https://other.example.com".into(),
        });
        let projected = project_record(&row, "test".into()).unwrap();
        assert_eq!(projected.header_links.len(), 1);
        assert!(projected
            .omitted
            .contains(&"ambiguous_origin_header".into()));
    }

    fn header_declaration_cases() -> Vec<(&'static str, &'static str, Option<&'static str>)> {
        let mut cases = vec![
            (
                "Origin",
                "https://source.example.com",
                Some("https://source.example.com"),
            ),
            (
                "Origin",
                "HTTPS://SOURCE.Example.COM:443",
                Some("https://source.example.com"),
            ),
            (
                "Origin",
                "http://source.example.com:80",
                Some("http://source.example.com"),
            ),
            (
                "Origin",
                "https://source.example.com:8443",
                Some("https://source.example.com:8443"),
            ),
            (
                "Origin",
                "https://source.example.com/",
                Some("https://source.example.com"),
            ),
            (
                "Origin",
                " https://source.example.com:443/ ",
                Some("https://source.example.com"),
            ),
            (
                "Origin",
                "https://[2001:db8:0:0:0:0:0:1]:443",
                Some("https://[2001:db8::1]"),
            ),
            (
                "Origin",
                "http://[2001:db8::1]:8080/",
                Some("http://[2001:db8::1]:8080"),
            ),
            (
                "Referer",
                "https://source.example.com/path?q=QUERY_SENTINEL#FRAGMENT_SENTINEL",
                Some("https://source.example.com"),
            ),
            (
                "Referer",
                "https://source.example.com/a/..",
                Some("https://source.example.com"),
            ),
            (
                "Referer",
                "https://source.example.com/%2e",
                Some("https://source.example.com"),
            ),
            (
                "Referer",
                "https://USER_SENTINEL:PASS_SENTINEL@source.example.com/path",
                Some("https://source.example.com"),
            ),
        ];
        cases.extend(
            [
                "",
                "null",
                "/relative",
                "javascript:alert(1)",
                "https://",
                "https:///source.example.com",
                "https:////source.example.com",
                "http://///source.example.com/",
                "https://source.example.com//",
                "https://source.example.com/path",
                "https://source.example.com/.",
                "https://source.example.com/..",
                "https://source.example.com/a/..",
                "https://source.example.com/a/../",
                "https://source.example.com/%2e",
                "https://source.example.com/%2E/",
                "https://source.example.com/a/%2e%2e",
                "https://source.example.com/a/.%2e",
                "https://source.example.com/a/%2E.",
                "https://source.example.com/a/%2e%2E/",
                "https://@source.example.com",
                "https://:@source.example.com",
                "https://user@source.example.com",
                "https://user:secret@source.example.com",
                "https://source.example.com?",
                "https://source.example.com/?q=secret",
                "https://source.example.com#",
                "https://source.example.com/#fragment",
                "https://source.example.com https://other.example.com",
                "https://source.example.com,https://other.example.com",
                "https://source.example.com, null",
                "https://source.example.com,null",
                "https://source.example.com,",
                "https://source.example.com\\path",
            ]
            .into_iter()
            .map(|value| ("Origin", value, None)),
        );
        cases
    }

    #[test]
    fn service_map_header_declaration_rows_validate_raw_origin_structure() {
        for (header_name, value, expected) in header_declaration_cases() {
            for with_response in [false, true] {
                let mut row = record("/captured");
                row.request.headers = headers(&[(header_name, value)]);
                if !with_response {
                    row.response = None;
                    row.status = None;
                }
                let projected = project_record(&row, "test".into()).unwrap();
                assert_eq!(
                    projected.header_links.len(),
                    usize::from(expected.is_some()),
                    "{header_name}: {value:?}; with_response={with_response}"
                );
                if let Some(expected) = expected {
                    let link = &projected.header_links[0];
                    assert_eq!(link.source_origin, expected, "{header_name}: {value:?}");
                    assert_eq!(
                        link.relationship,
                        format!("{}_header", header_name.to_ascii_lowercase())
                    );
                }
                assert_eq!(projected.origin, "https://example.com");
                assert_eq!(projected.path, "/captured");
                assert!(projected.metadata_complete);
                assert!(!serde_json::to_string(&projected)
                    .unwrap()
                    .contains("SENTINEL"));
            }
        }
    }

    #[tokio::test]
    async fn service_map_header_declaration_aggregates_validate_raw_origin_structure() {
        for (header_name, value, expected) in header_declaration_cases() {
            let mut row = record("/captured");
            row.request.headers = headers(&[(header_name, value)]);
            let result = map(vec![row]).await;
            assert!(result.snapshot.complete);
            assert_eq!(result.snapshot.included_rows, 1);
            assert_eq!(result.snapshot.origin_count, 1);
            assert_eq!(result.snapshot.endpoint_count, 1);
            let origin = &result.origins[0];
            assert_eq!(origin.origin, "https://example.com");
            assert_eq!(origin.request_count, 1);
            assert_eq!(origin.endpoints[0].path, "/captured");
            assert_eq!(
                origin.header_links.len(),
                usize::from(expected.is_some()),
                "{header_name}: {value:?}"
            );
            if let Some(expected) = expected {
                let link = &origin.header_links[0];
                assert_eq!(link.source_origin, expected, "{header_name}: {value:?}");
                assert_eq!(
                    link.relationship,
                    format!("{}_header", header_name.to_ascii_lowercase())
                );
                assert_eq!(link.source, "request_header_declaration");
                assert_eq!(link.evidence, origin.endpoints[0].evidence);
            }
            assert!(!serde_json::to_string(&result).unwrap().contains("SENTINEL"));
        }
    }

    #[tokio::test]
    async fn service_map_redirect_is_only_an_unambiguous_3xx_instruction() {
        let mut row = record("/a/b");
        row.response.as_mut().unwrap().headers = headers(&[("Location", "../next?secret=gone")]);
        assert!(project_record(&row, "test".into())
            .unwrap()
            .redirect
            .is_none());
        row.status = Some(302);
        let projected = project_record(&row, "test".into()).unwrap();
        assert_eq!(projected.redirect.unwrap().target_path, "/next");
        row.response.as_mut().unwrap().headers.push(HeaderRecord {
            name: "Location".into(),
            value: "/other".into(),
        });
        let projected = project_record(&row, "test".into()).unwrap();
        assert!(projected.redirect.is_none());
        assert!(!projected.metadata_complete);
        assert!(projected
            .omitted
            .contains(&"ambiguous_location_headers".into()));
    }

    #[tokio::test]
    async fn service_map_header_limits_and_exact_path_limits_are_explicit() {
        let mut row = record("/");
        row.status = Some(302);
        row.response.as_mut().unwrap().headers = (0..=MAX_HEADERS)
            .map(|_| HeaderRecord {
                name: "x-test".into(),
                value: "a".into(),
            })
            .collect();
        row.response.as_mut().unwrap().headers[0] = HeaderRecord {
            name: "location".into(),
            value: "/next".into(),
        };
        let result = map(vec![
            row,
            record(&format!("/{}", "x".repeat(MAX_PATH_BYTES))),
        ])
        .await;
        assert!(!result.snapshot.complete);
        assert!(result
            .snapshot
            .stopped_by
            .contains(&"header_scan_limit".into()));
        assert!(result
            .snapshot
            .stopped_by
            .contains(&"unsupported_or_oversized_metadata".into()));
        assert!(result.origins[0].redirects.is_empty());
    }

    #[tokio::test]
    async fn service_map_newest_window_and_evidence_samples_are_bounded() {
        let store = TransactionStore::from_records(
            (0..10)
                .rev()
                .map(|index| {
                    let mut r = record("/same");
                    r.status = Some(200 + index);
                    r
                })
                .collect(),
        );
        let session = Uuid::new_v4();
        let result = build(
            session,
            store.service_map_snapshot(session, 5).await,
            5,
            false,
            None,
            &[],
            &[],
        );
        assert_eq!(result.snapshot.retained_rows, 10);
        assert_eq!(result.snapshot.considered_rows, 5);
        assert_eq!(result.snapshot.included_rows, 5);
        assert_eq!(result.origins[0].request_count, 5);
        assert_eq!(result.origins[0].endpoints[0].evidence.len(), MAX_EVIDENCE);
        assert!(!result.snapshot.complete);
        assert_eq!(result.snapshot.stopped_by, vec!["max_records"]);
    }

    #[tokio::test]
    async fn service_map_high_cardinality_bounds_and_origin_expansion() {
        let records = (0..MAX_ORIGINS + 1)
            .map(|index| {
                let mut r = record("/");
                r.host = format!("{index}.example.com");
                r
            })
            .collect();
        let store = TransactionStore::from_records(records);
        let session = Uuid::new_v4();
        let result = build(
            session,
            store.service_map_snapshot(session, MAX_RECORDS).await,
            MAX_RECORDS,
            false,
            None,
            &[],
            &[],
        );
        assert_eq!(result.origins.len(), MAX_ORIGINS);
        assert!(!result.snapshot.complete);
        assert!(result.snapshot.stopped_by.contains(&"origins_limit".into()));
        let expanded = build(
            session,
            store.service_map_snapshot(session, MAX_RECORDS).await,
            MAX_RECORDS,
            false,
            Some("https://500.example.com".into()),
            &[],
            &[],
        );
        assert_eq!(expanded.origins.len(), 1);
        assert!(expanded.snapshot.complete);
        let endpoints = map((0..MAX_ENDPOINTS_PER_ORIGIN + 1)
            .map(|i| record(&format!("/item/{i}")))
            .collect())
        .await;
        assert_eq!(
            endpoints.origins[0].endpoints.len(),
            MAX_ENDPOINTS_PER_ORIGIN
        );
        assert_eq!(
            endpoints.origins[0].request_count,
            MAX_ENDPOINTS_PER_ORIGIN + 1
        );
        assert!(endpoints
            .snapshot
            .stopped_by
            .contains(&"endpoints_limit".into()));
    }

    #[tokio::test]
    async fn service_map_projection_byte_budget_bounds_long_paths() {
        let suffix = "x".repeat(1900);
        let store = TransactionStore::from_records(
            (0..5000)
                .map(|i| record(&format!("/{i}/{suffix}")))
                .collect(),
        );
        let session = Uuid::new_v4();
        let start = std::time::Instant::now();
        let snapshot = store.service_map_snapshot(session, MAX_RECORDS).await;
        eprintln!(
            "service-map bounded 5000-row projection: {:?}, {} considered",
            start.elapsed(),
            snapshot.considered_rows
        );
        assert!(snapshot.considered_rows < 5000);
        assert!(snapshot.stopped_by.contains("metadata_bytes"));
        assert!(
            snapshot
                .rows
                .iter()
                .map(|r| serde_json::to_vec(r).unwrap().len())
                .sum::<usize>()
                <= MAX_METADATA_BYTES
        );
    }
    #[tokio::test]
    async fn service_map_missing_body_file_does_not_trigger_body_rehydration() {
        let dir =
            std::env::temp_dir().join(format!("sniper-service-map-metadata-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut row = record("/metadata-only");
        row.request.body_preview.clear();
        row.response.as_mut().unwrap().body_preview.clear();
        let id = row.id;
        let store = TransactionStore::from_located_records_with_journal(
            vec![row],
            std::collections::HashMap::from([(
                id,
                crate::session::BodyLocator {
                    offset: 999,
                    len: 999,
                },
            )]),
            &std::collections::HashSet::new(),
            dir.join("transactions.journal"),
            0,
        );
        let session = Uuid::new_v4();
        let snapshot = store.service_map_snapshot(session, 10).await;
        assert_eq!(snapshot.rows.len(), 1);
        let evidence = store
            .service_map_evidence(session, &snapshot.rows[0].evidence_id)
            .await;
        assert!(evidence.available);
        assert_eq!(evidence.row.unwrap().path, "/metadata-only");
        assert!(!dir.join("transactions.ndjson").exists());
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn service_map_considered_times_use_actual_extrema_with_out_of_order_completion() {
        let mut early = record("/early");
        let mut late = record("/late");
        early.started_at = Utc::now() - chrono::Duration::hours(1);
        late.started_at = Utc::now();
        let first = late.started_at;
        let last = early.started_at;
        // Newest capture completed last, but began first.
        let result = map(vec![early, late]).await;
        assert_eq!(result.snapshot.first_retained_started_at, Some(first));
        assert_eq!(result.snapshot.last_retained_started_at, Some(last));
        assert_eq!(result.snapshot.oldest_considered_at, Some(last));
        assert_eq!(result.snapshot.latest_considered_at, Some(first));
    }
    #[tokio::test]
    async fn service_map_connect_is_case_sensitive_and_nonredirect_3xx_has_no_edge() {
        let mut lower = record("/custom");
        lower.method = "connect".into();
        let mut upper = record("/tunnel");
        upper.method = "CONNECT".into();
        let result = map(vec![lower, upper]).await;
        assert_eq!(result.snapshot.non_http_rows, 1);
        assert_eq!(result.origins[0].endpoints[0].method, "connect");
        for status in [304, 305, 306, 309, 399] {
            let mut row = record("/");
            row.status = Some(status);
            row.response.as_mut().unwrap().headers = headers(&[("Location", "/next")]);
            assert!(project_record(&row, "test".into())
                .unwrap()
                .redirect
                .is_none());
        }
        for host in [
            " example.com",
            "example.com\n",
            "example.com\t",
            "example.com/path",
            "example.com?token=secret",
        ] {
            let mut row = record("/");
            row.host = host.into();
            assert!(project_record(&row, "test".into()).is_none());
        }
    }
    #[test]
    fn service_map_websocket_and_mime_checks_obey_header_field_bounds() {
        let mut row = record("/websocket");
        row.status = Some(101);
        row.request.headers = (0..MAX_HEADERS)
            .map(|_| HeaderRecord {
                name: "x-test".into(),
                value: "a".into(),
            })
            .collect();
        row.request.headers.extend(headers(&[
            ("Connection", "upgrade"),
            ("Upgrade", "websocket"),
        ]));
        row.response.as_mut().unwrap().content_type = Some("x".repeat(MAX_HEADER_VALUE_BYTES + 1));
        let projected = project_record(&row, "test".into()).unwrap();
        assert!(!projected.is_websocket);
        assert!(projected.content_type.is_none());
        assert!(projected
            .omitted
            .contains(&"request_header_scan_limit".into()));
        assert!(projected
            .omitted
            .contains(&"unsupported_content_type".into()));
        row.request.headers = headers(&[
            ("Connection", "keep-alive, Upgrade"),
            ("Upgrade", "websocket"),
        ]);
        assert!(project_record(&row, "test".into()).unwrap().is_websocket);
    }
    #[tokio::test]
    async fn service_map_scope_matches_original_expanded_ipv6_and_alias_rows() {
        let mut expanded = record("/expanded");
        expanded.host = "[0:0:0:0:0:0:0:1]:8443".into();
        let mut compressed = record("/compressed");
        compressed.host = "[::1]:8443".into();
        let store = TransactionStore::from_records(vec![expanded, compressed]);
        let session = Uuid::new_v4();
        let include = vec!["0:0:0:0:0:0:0:1".into()];
        let filtered = build(
            session,
            store.service_map_snapshot(session, 10).await,
            10,
            true,
            None,
            &include,
            &[],
        );
        assert_eq!(filtered.origins.len(), 1);
        assert_eq!(filtered.origins[0].origin, "https://[::1]:8443");
        assert_eq!(filtered.origins[0].request_count, 1);
        assert_eq!(filtered.origins[0].endpoints[0].path, "/expanded");
        assert_eq!(filtered.snapshot.excluded_rows, 1);
        let all = build(
            session,
            store.service_map_snapshot(session, 10).await,
            10,
            false,
            None,
            &include,
            &[],
        );
        assert_eq!(all.origins[0].request_count, 2);
        assert!(all.origins[0].in_scope);
        assert!(!serde_json::to_string(&all).unwrap().contains("scope_host"));
    }
}
