//! Explicit equality inspection over retained direct socket observations. This
//! never establishes origin ownership, shared deployment, or service calls.
use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    model::{
        ObservedSocketPeer, ObservedSocketPeerAddress, ObservedSocketPeerRoute,
        ObservedSocketPeerSource, TrafficKind, TransactionRecord,
    },
    scope,
    service_map::{self, EvidenceKey},
};

/// This supplementary contract accepts only unchanged canonical row tokens.
/// Legacy evidence parsing remains unchanged for its existing callers.
pub fn is_canonical_evidence_id(value: &str) -> bool {
    canonical_evidence_key(value).is_some()
}

pub(crate) fn canonical_evidence_key(value: &str) -> Option<EvidenceKey> {
    let key = EvidenceKey::parse(value)?;
    let canonical = format!(
        "v1.{}.{}.{}.{}.{}",
        key.session_id, key.generation, key.sequence, key.revision, key.record_id
    );
    (canonical == value).then_some(key)
}

pub const MAX_ORIGIN_GROUPS: usize = 100;
pub const MAX_SAMPLES_PER_ORIGIN: usize = 3;
pub const MAX_PROJECTION_BYTES: usize = 256 * 1024;
pub const MAX_HOST_BYTES: usize = 512;
pub const MAX_SCHEME_BYTES: usize = 8;
pub const MAX_SCOPE_PATTERN_ENTRIES: usize = 500;
pub const MAX_SCOPE_PATTERN_BYTES: usize = 512;
pub const MAX_SCOPE_WORK_UNITS: usize = 8 * 1024 * 1024;
const MAX_ORIGIN_BYTES: usize = MAX_HOST_BYTES + 32;
// Conservative JSON worst-case charges include punctuation, escaped strings,
// decimal counters and all fixed metadata. Admission precedes token copies.
const BASE_BYTES: usize = 8 * 1024;
const SAMPLE_BYTES: usize = 1024;

pub const ACQUISITION_REASONS: &[&str] = &[
    "invalid_evidence_id",
    "session_mismatch",
    "stale_generation",
    "record_missing",
    "record_changed",
    "inactive_session",
    "session_source_changed",
    "unsupported_source_context",
    "observation_not_recorded",
    "observation_unavailable",
    "non_direct_observation",
    "invalid_observation",
    "metadata_unavailable",
    "source_out_of_scope",
    "peer_inspection_failed",
    "scope_work_limit",
];
pub const PARTIAL_REASONS: &[&str] = &[
    "max_records",
    "source_outside_window",
    "invalid_metadata",
    "origin_group_limit",
    "projection_byte_limit",
    "sample_limit",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchLimits {
    pub max_records: usize,
    pub origin_groups: usize,
    pub samples_per_origin: usize,
    pub projection_bytes: usize,
    pub host_bytes: usize,
    pub scheme_bytes: usize,
    pub scope_pattern_entries: usize,
    pub scope_pattern_bytes: usize,
    pub scope_work_units: usize,
}
impl SocketPeerMatchLimits {
    pub fn new(max_records: usize) -> Self {
        Self {
            max_records,
            origin_groups: MAX_ORIGIN_GROUPS,
            samples_per_origin: MAX_SAMPLES_PER_ORIGIN,
            projection_bytes: MAX_PROJECTION_BYTES,
            host_bytes: MAX_HOST_BYTES,
            scheme_bytes: MAX_SCHEME_BYTES,
            scope_pattern_entries: MAX_SCOPE_PATTERN_ENTRIES,
            scope_pattern_bytes: MAX_SCOPE_PATTERN_BYTES,
            scope_work_units: MAX_SCOPE_WORK_UNITS,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SocketPeerAddressFamily {
    Ipv4,
    Ipv6,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchKey {
    pub source: ObservedSocketPeerSource,
    pub route: ObservedSocketPeerRoute,
    pub address_family: SocketPeerAddressFamily,
    pub address: ObservedSocketPeerAddress,
}
impl SocketPeerMatchKey {
    fn from_observation(observation: &ObservedSocketPeer) -> Result<Self, &'static str> {
        // The extension is already typed and bounded: do not serialize it or
        // consult headers merely to compare fixed-size metadata.
        if observation.address.is_some() == observation.reason.is_some() {
            return Err("invalid_observation");
        }
        if observation.route != ObservedSocketPeerRoute::Direct {
            return Err("non_direct_observation");
        }
        let address = observation.address.ok_or("observation_unavailable")?;
        if !usable_address(address) {
            return Err("invalid_observation");
        }
        Ok(Self {
            source: observation.source,
            route: observation.route,
            address_family: if address.ip.is_ipv4() {
                SocketPeerAddressFamily::Ipv4
            } else {
                SocketPeerAddressFamily::Ipv6
            },
            address,
        })
    }
}

pub fn usable_address(address: ObservedSocketPeerAddress) -> bool {
    if address.port == 0 {
        return false;
    }
    fn usable_v4(ip: std::net::Ipv4Addr) -> bool {
        let octets = ip.octets();
        octets[0] != 0 && octets[0] < 224 && !ip.is_link_local()
    }
    match address.ip {
        IpAddr::V4(ip) => usable_v4(ip),
        IpAddr::V6(ip) => {
            !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
                && ip.to_ipv4_mapped().is_none_or(usable_v4)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchSource {
    pub evidence_id: String,
    pub sequence: String,
    pub origin: String,
    pub started_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub in_scope: bool,
    pub in_window: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchSample {
    pub evidence_id: String,
    pub sequence: String,
    pub started_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchOrigin {
    pub origin: String,
    pub in_scope: bool,
    pub matching_rows: usize,
    pub first_observed_at: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
    pub samples: Vec<SocketPeerMatchSample>,
    pub omitted_samples: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchSnapshot {
    pub id: Uuid,
    pub generation: Uuid,
    pub captured_at: DateTime<Utc>,
    pub window_order: String,
    pub retained_rows: usize,
    pub considered_rows: usize,
    pub oldest_considered_sequence: Option<String>,
    pub latest_considered_sequence: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SocketPeerMatchCounts {
    pub non_http_rows: usize,
    pub not_recorded_rows: usize,
    pub unavailable_observation_rows: usize,
    pub non_direct_rows: usize,
    pub invalid_observation_rows: usize,
    pub different_peer_rows: usize,
    pub invalid_metadata_rows: usize,
    pub scope_excluded_rows: usize,
    pub matching_rows: usize,
    pub returned_matching_rows: usize,
    pub omitted_matching_rows: usize,
    pub sample_rows: usize,
    pub omitted_sample_rows: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceMapSocketPeerMatches {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub evidence_id: String,
    pub available: bool,
    pub status: String,
    pub complete: bool,
    pub reasons: Vec<String>,
    pub in_scope_only: bool,
    pub coverage: String,
    pub peer: Option<SocketPeerMatchKey>,
    pub source: Option<SocketPeerMatchSource>,
    pub snapshot: Option<SocketPeerMatchSnapshot>,
    pub counts: SocketPeerMatchCounts,
    pub origins: Vec<SocketPeerMatchOrigin>,
    pub limits: SocketPeerMatchLimits,
}
impl ServiceMapSocketPeerMatches {
    pub fn missing(
        session_id: Uuid,
        evidence_id: String,
        in_scope_only: bool,
        max_records: usize,
        reason: &str,
    ) -> Self {
        let reason = if ACQUISITION_REASONS.contains(&reason) {
            reason
        } else {
            "peer_inspection_failed"
        };
        Self {
            schema_version: 1,
            session_id,
            evidence_id,
            available: false,
            status: "unavailable".into(),
            complete: false,
            reasons: vec![reason.into()],
            in_scope_only,
            coverage: "retained_direct_socket_peer_equality".into(),
            peer: None,
            source: None,
            snapshot: None,
            counts: SocketPeerMatchCounts::default(),
            origins: Vec::new(),
            limits: SocketPeerMatchLimits::new(max_records),
        }
    }

    /// Validate an untrusted server response before exposing its peer metadata.
    pub fn validate_response(
        &self,
        session_id: Uuid,
        evidence_id: &str,
        in_scope_only: bool,
        max_records: usize,
    ) -> bool {
        let Some(seed) = canonical_evidence_key(evidence_id) else {
            return false;
        };
        if self.schema_version != 1
            || self.session_id != session_id
            || self.evidence_id != evidence_id
            || self.in_scope_only != in_scope_only
            || self.coverage != "retained_direct_socket_peer_equality"
            || self.limits != SocketPeerMatchLimits::new(max_records)
            || !(1..=service_map::MAX_RECORDS).contains(&max_records)
            || self.reasons.len() > PARTIAL_REASONS.len().max(1)
            || !self.reasons.windows(2).all(|pair| pair[0] < pair[1])
        {
            return false;
        }
        if !self.available {
            return !self.complete
                && self.status == "unavailable"
                && self.reasons.len() == 1
                && ACQUISITION_REASONS.contains(&self.reasons[0].as_str())
                && self.peer.is_none()
                && self.source.is_none()
                && self.snapshot.is_none()
                && self.origins.is_empty()
                && self.counts == SocketPeerMatchCounts::default()
                && (self.reasons[0] != "source_out_of_scope" || in_scope_only);
        }
        let (Some(peer), Some(source), Some(snapshot)) = (&self.peer, &self.source, &self.snapshot)
        else {
            return false;
        };
        let expected_family = if peer.address.ip.is_ipv4() {
            SocketPeerAddressFamily::Ipv4
        } else {
            SocketPeerAddressFamily::Ipv6
        };
        if seed.session_id != session_id
            || peer.route != ObservedSocketPeerRoute::Direct
            || peer.address_family != expected_family
            || !usable_address(peer.address)
            || source.evidence_id != evidence_id
            || source.sequence != seed.sequence.to_string()
            || !safe_origin(&source.origin)
            || (in_scope_only && !source.in_scope)
            || snapshot.generation != seed.generation
            || snapshot.window_order != "latest_retained_sequence"
            || snapshot.retained_rows == 0
            || snapshot.considered_rows != snapshot.retained_rows.min(max_records)
            || self.complete != self.reasons.is_empty()
            || !self
                .reasons
                .iter()
                .all(|r| PARTIAL_REASONS.contains(&r.as_str()))
            || self.origins.len() > MAX_ORIGIN_GROUPS
            || snapshot.retained_rows as u128 > 9_007_199_254_740_991u128
            || !self
                .origins
                .windows(2)
                .all(|pair| pair[0].origin < pair[1].origin)
        {
            return false;
        }
        let parse_sequence = |value: &Option<String>| {
            value.as_ref().and_then(|value| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|number| number.to_string() == *value)
            })
        };
        let (Some(oldest), Some(latest)) = (
            parse_sequence(&snapshot.oldest_considered_sequence),
            parse_sequence(&snapshot.latest_considered_sequence),
        ) else {
            return false;
        };
        if oldest > latest
            || (snapshot.considered_rows == 1 && oldest != latest)
            || seed.sequence > latest
            || (snapshot.considered_rows as u128) > (latest as u128 - oldest as u128 + 1)
            || (snapshot.considered_rows == snapshot.retained_rows && !source.in_window)
            || source.in_window != (oldest..=latest).contains(&seed.sequence)
            || self.reasons.iter().any(|r| r == "max_records")
                != (snapshot.retained_rows > snapshot.considered_rows)
            || self.reasons.iter().any(|r| r == "source_outside_window") == source.in_window
        {
            return false;
        }
        let mut origins = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        let mut sequences = BTreeSet::new();
        let mut returned = 0usize;
        let mut samples = 0usize;
        let mut omitted_samples = 0usize;
        for origin in &self.origins {
            if !safe_origin(&origin.origin)
                || !origins.insert(&origin.origin)
                || (in_scope_only && !origin.in_scope)
                || origin.matching_rows == 0
                || origin.matching_rows > snapshot.considered_rows
                || origin.samples.len() > MAX_SAMPLES_PER_ORIGIN
                || origin.samples.len() > origin.matching_rows
                || origin.omitted_samples != origin.matching_rows - origin.samples.len()
                || origin.first_observed_at > origin.last_observed_at
            {
                return false;
            }
            returned = returned.saturating_add(origin.matching_rows);
            samples += origin.samples.len();
            omitted_samples = omitted_samples.saturating_add(origin.omitted_samples);
            if origin.samples.len() == origin.matching_rows
                && (origin.samples.iter().map(|s| s.observed_at).min()
                    != Some(origin.first_observed_at)
                    || origin.samples.iter().map(|s| s.observed_at).max()
                        != Some(origin.last_observed_at))
            {
                return false;
            }
            let unrepresented_time_extrema = usize::from(
                !origin
                    .samples
                    .iter()
                    .any(|sample| sample.observed_at == origin.first_observed_at),
            ) + usize::from(
                origin.last_observed_at != origin.first_observed_at
                    && !origin
                        .samples
                        .iter()
                        .any(|sample| sample.observed_at == origin.last_observed_at),
            );
            if unrepresented_time_extrema > origin.omitted_samples {
                return false;
            }
            let mut previous_sequence = None;
            for sample in &origin.samples {
                let Some(key) = canonical_evidence_key(&sample.evidence_id) else {
                    return false;
                };
                if key.session_id != session_id
                    || key.generation != snapshot.generation
                    || sample.sequence != key.sequence.to_string()
                    || key.sequence == seed.sequence
                    || !(oldest..=latest).contains(&key.sequence)
                    || !tokens.insert(&sample.evidence_id)
                    || !sequences.insert(key.sequence)
                    || previous_sequence.is_some_and(|previous| previous <= key.sequence)
                    || sample.observed_at < origin.first_observed_at
                    || sample.observed_at > origin.last_observed_at
                {
                    return false;
                }
                previous_sequence = Some(key.sequence);
            }
        }
        if source.in_window {
            sequences.insert(seed.sequence);
        }
        // Exact samples and the source occupy known rows. Each distinct window
        // endpoint not among them must be backed by another considered row.
        let unknown_rows = snapshot.considered_rows.saturating_sub(sequences.len());
        let unknown_extrema = usize::from(!sequences.contains(&oldest))
            + usize::from(latest != oldest && !sequences.contains(&latest));
        if sequences.len() > snapshot.considered_rows || unknown_extrema > unknown_rows {
            return false;
        }
        // These charges are the same conservative admission costs used by the
        // producer. A claimed byte stop must at least be possible at the final
        // remaining budget; unknown omitted origin sizes are never invented.
        let admitted_bytes = self.origins.iter().fold(BASE_BYTES, |bytes, origin| {
            bytes + origin.origin.len() * 12 + 512 + origin.samples.len() * SAMPLE_BYTES
        });
        if admitted_bytes > MAX_PROJECTION_BYTES {
            return false;
        }
        let remaining_bytes = MAX_PROJECTION_BYTES - admitted_bytes;
        let has_sample_limit = self.origins.iter().any(|origin| {
            origin.samples.len() == MAX_SAMPLES_PER_ORIGIN && origin.omitted_samples > 0
        });
        let byte_rejected_sample = self.origins.iter().any(|origin| {
            origin.samples.len() < MAX_SAMPLES_PER_ORIGIN && origin.omitted_samples > 0
        });
        let c = &self.counts;
        let byte_limit_possible = (c.omitted_matching_rows > 0
            && remaining_bytes < MAX_ORIGIN_BYTES * 12 + 512)
            || (byte_rejected_sample && remaining_bytes < SAMPLE_BYTES);
        let classified = [
            c.non_http_rows,
            c.not_recorded_rows,
            c.unavailable_observation_rows,
            c.non_direct_rows,
            c.invalid_observation_rows,
            c.different_peer_rows,
            c.invalid_metadata_rows,
            c.scope_excluded_rows,
            c.matching_rows,
        ]
        .into_iter()
        .fold(0usize, usize::saturating_add);
        [
            c.non_http_rows,
            c.not_recorded_rows,
            c.unavailable_observation_rows,
            c.non_direct_rows,
            c.invalid_observation_rows,
            c.different_peer_rows,
            c.invalid_metadata_rows,
            c.scope_excluded_rows,
            c.matching_rows,
            c.returned_matching_rows,
            c.omitted_matching_rows,
            c.sample_rows,
            c.omitted_sample_rows,
        ]
        .into_iter()
        .all(|count| count <= snapshot.considered_rows)
            && classified == snapshot.considered_rows - usize::from(source.in_window)
            && (in_scope_only || c.scope_excluded_rows == 0)
            && (!self.reasons.iter().any(|r| r == "origin_group_limit")
                || (self.origins.len() == MAX_ORIGIN_GROUPS && c.omitted_matching_rows > 0))
            && self.reasons.iter().any(|r| r == "sample_limit") == has_sample_limit
            && (!self.reasons.iter().any(|r| r == "projection_byte_limit") || byte_limit_possible)
            && (!byte_rejected_sample
                || (remaining_bytes < SAMPLE_BYTES
                    && self.reasons.iter().any(|r| r == "projection_byte_limit")))
            && c.returned_matching_rows == returned
            && c.sample_rows == samples
            && c.omitted_sample_rows == omitted_samples
            && c.matching_rows
                == c.returned_matching_rows
                    .saturating_add(c.omitted_matching_rows)
            && self.status
                == if c.matching_rows > 0 {
                    "matched"
                } else {
                    "no_matches"
                }
            && (c.invalid_metadata_rows > 0) == self.reasons.iter().any(|r| r == "invalid_metadata")
            && (c.omitted_sample_rows == 0
                || self
                    .reasons
                    .iter()
                    .any(|r| r == "sample_limit" || r == "projection_byte_limit"))
            && (c.omitted_matching_rows == 0
                || self
                    .reasons
                    .iter()
                    .any(|r| r == "origin_group_limit" || r == "projection_byte_limit"))
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= MAX_PROJECTION_BYTES)
    }
}

fn ordinary_http(record: &TransactionRecord) -> bool {
    matches!(record.kind, TrafficKind::Http)
        && !record.method.eq_ignore_ascii_case("CONNECT")
        && record.status != Some(101)
}

fn origin(record: &TransactionRecord) -> Option<String> {
    // Only authority fields are relevant. Never walk or copy raw path, query,
    // credentials, request/response headers or resident body strings.
    if record.scheme.len() > MAX_SCHEME_BYTES
        || record.host.len() > MAX_HOST_BYTES
        || !(record.scheme.eq_ignore_ascii_case("http")
            || record.scheme.eq_ignore_ascii_case("https"))
        || record.host.contains(['/', '?', '#', '\\', '@'])
        || record
            .host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let url = url::Url::parse(&format!("{}://{}/", record.scheme, record.host)).ok()?;
    let host = url.host_str()?.trim_matches(['[', ']']);
    let origin = url.origin().ascii_serialization();
    if host.len() > MAX_HOST_BYTES || origin.len() > MAX_ORIGIN_BYTES {
        return None;
    }
    Some(origin)
}
fn safe_origin(value: &str) -> bool {
    value.len() <= MAX_ORIGIN_BYTES
        && service_map::canonical_origin(value).as_deref() == Some(value)
}

/// Called under the store's membership/revision lock. Only typed fixed-size peer
/// comparisons happen for nonmatches; allocation is bounded before copying.
pub(crate) fn inspect_retained<'a>(
    session_id: Uuid,
    generation: Uuid,
    evidence_id: &str,
    source: &TransactionRecord,
    rows: impl ExactSizeIterator<Item = (&'a TransactionRecord, Uuid)>,
    max_records: usize,
    in_scope_only: bool,
    prepared_scope: &scope::PreparedScope,
) -> ServiceMapSocketPeerMatches {
    let missing = |reason| {
        ServiceMapSocketPeerMatches::missing(
            session_id,
            evidence_id.into(),
            in_scope_only,
            max_records,
            reason,
        )
    };
    if !ordinary_http(source) {
        return missing("unsupported_source_context");
    }
    let Some(observation) = &source.observed_socket_peer else {
        return missing("observation_not_recorded");
    };
    let peer = match SocketPeerMatchKey::from_observation(observation) {
        Ok(peer) => peer,
        Err(reason) => return missing(reason),
    };
    let Some(source_origin) = origin(source) else {
        return missing("metadata_unavailable");
    };
    let mut scope_work_remaining = MAX_SCOPE_WORK_UNITS;
    let Ok(source_in_scope) =
        prepared_scope.contains_bounded(&source.host, &mut scope_work_remaining)
    else {
        return missing("scope_work_limit");
    };
    if in_scope_only && !source_in_scope {
        return missing("source_out_of_scope");
    }
    let retained_rows = rows.len();
    let mut result = missing("peer_inspection_failed");
    result.available = true;
    result.reasons.clear();
    result.peer = Some(peer.clone());
    result.source = Some(SocketPeerMatchSource {
        evidence_id: evidence_id.into(),
        sequence: source.sequence.to_string(),
        origin: source_origin,
        started_at: source.started_at,
        observed_at: observation.observed_at,
        in_scope: source_in_scope,
        in_window: false,
    });
    let mut snapshot = SocketPeerMatchSnapshot {
        id: Uuid::new_v4(),
        generation,
        captured_at: Utc::now(),
        window_order: "latest_retained_sequence".into(),
        retained_rows,
        considered_rows: 0,
        oldest_considered_sequence: None,
        latest_considered_sequence: None,
    };
    let mut reasons = BTreeSet::new();
    let mut groups: BTreeMap<String, SocketPeerMatchOrigin> = BTreeMap::new();
    let mut bytes = BASE_BYTES;
    let mut oldest_sequence = None;
    let mut latest_sequence = None;
    for (row, revision) in rows.take(max_records) {
        snapshot.considered_rows += 1;
        oldest_sequence = Some(row.sequence);
        latest_sequence.get_or_insert(row.sequence);
        if row.sequence == source.sequence {
            result.source.as_mut().unwrap().in_window = true;
            continue;
        }
        let counts = &mut result.counts;
        if !ordinary_http(row) {
            counts.non_http_rows += 1;
            continue;
        }
        let Some(observation) = &row.observed_socket_peer else {
            counts.not_recorded_rows += 1;
            continue;
        };
        let candidate = match SocketPeerMatchKey::from_observation(observation) {
            Ok(candidate) => candidate,
            Err("non_direct_observation") => {
                counts.non_direct_rows += 1;
                continue;
            }
            Err("observation_unavailable") => {
                counts.unavailable_observation_rows += 1;
                continue;
            }
            Err(_) => {
                counts.invalid_observation_rows += 1;
                continue;
            }
        };
        if candidate != peer {
            counts.different_peer_rows += 1;
            continue;
        }
        let Some(origin) = origin(row) else {
            counts.invalid_metadata_rows += 1;
            reasons.insert("invalid_metadata");
            continue;
        };
        let Ok(in_scope) = prepared_scope.contains_bounded(&row.host, &mut scope_work_remaining)
        else {
            // Never reinterpret an incomplete scope decision as false, and
            // never publish partial counts from an aborted equality inspection.
            return missing("scope_work_limit");
        };
        if in_scope_only && !in_scope {
            counts.scope_excluded_rows += 1;
            continue;
        }
        counts.matching_rows += 1;
        if !groups.contains_key(&origin) {
            if groups.len() == MAX_ORIGIN_GROUPS {
                counts.omitted_matching_rows += 1;
                reasons.insert("origin_group_limit");
                continue;
            }
            // Two owned origin strings (map key and DTO) plus fixed group data.
            let charge = origin.len() * 12 + 512;
            if charge > MAX_PROJECTION_BYTES - bytes {
                counts.omitted_matching_rows += 1;
                reasons.insert("projection_byte_limit");
                continue;
            }
            bytes += charge;
            groups.insert(
                origin.clone(),
                SocketPeerMatchOrigin {
                    origin: origin.clone(),
                    in_scope,
                    matching_rows: 0,
                    first_observed_at: observation.observed_at,
                    last_observed_at: observation.observed_at,
                    samples: Vec::new(),
                    omitted_samples: 0,
                },
            );
        }
        let group = groups.get_mut(&origin).unwrap();
        group.matching_rows += 1;
        group.in_scope |= in_scope;
        counts.returned_matching_rows += 1;
        group.first_observed_at = group.first_observed_at.min(observation.observed_at);
        group.last_observed_at = group.last_observed_at.max(observation.observed_at);
        if group.samples.len() < MAX_SAMPLES_PER_ORIGIN
            && SAMPLE_BYTES <= MAX_PROJECTION_BYTES - bytes
        {
            bytes += SAMPLE_BYTES;
            group.samples.push(SocketPeerMatchSample {
                evidence_id: service_map::evidence_id(session_id, generation, revision, row),
                sequence: row.sequence.to_string(),
                started_at: row.started_at,
                observed_at: observation.observed_at,
            });
            counts.sample_rows += 1;
        } else {
            reasons.insert(if group.samples.len() == MAX_SAMPLES_PER_ORIGIN {
                "sample_limit"
            } else {
                "projection_byte_limit"
            });
            group.omitted_samples += 1;
            counts.omitted_sample_rows += 1;
        }
    }
    if snapshot.considered_rows < snapshot.retained_rows {
        reasons.insert("max_records");
    }
    if !result.source.as_ref().unwrap().in_window {
        reasons.insert("source_outside_window");
    }
    result.complete = reasons.is_empty();
    result.status = if result.counts.matching_rows > 0 {
        "matched"
    } else {
        "no_matches"
    }
    .into();
    result.reasons = reasons.into_iter().map(str::to_owned).collect();
    snapshot.oldest_considered_sequence = oldest_sequence.map(|sequence| sequence.to_string());
    snapshot.latest_considered_sequence = latest_sequence.map(|sequence| sequence.to_string());
    result.snapshot = Some(snapshot);
    result.origins = groups.into_values().collect();
    result
}
