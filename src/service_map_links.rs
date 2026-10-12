//! Explicit metadata-only declarations from one exactly identified retained response.
//! These literals do not establish service ownership, deployment, or actual calls.
use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    model::{HeaderRecord, TrafficKind, TransactionRecord},
    service_map::request_target,
    service_map_link_parser as parser,
};

pub const ACQUISITION_REASONS: &[&str] = &[
    "invalid_evidence_id",
    "session_mismatch",
    "stale_generation",
    "record_missing",
    "record_changed",
    "response_unavailable",
    "unsupported_source_context",
    "inactive_session",
    "session_source_changed",
    "metadata_unavailable",
    "link_inspection_failed",
];
pub const PARSER_REASONS: &[&str] = &[
    "header_count_limit",
    "header_byte_limit",
    "link_field_byte_limit",
    "candidate_limit",
    "parameter_limit",
    "relation_token_limit",
    "observation_limit",
    "malformed_field",
    "unsupported_target",
    "anchor_present",
    "missing_relation",
    "unsupported_relation_syntax",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceMapLinkLimits {
    pub headers: usize,
    pub header_bytes: usize,
    pub link_field_bytes: usize,
    pub target_bytes: usize,
    pub candidates: usize,
    pub parameters_per_member: usize,
    pub relation_tokens_per_member: usize,
    pub observations: usize,
    pub target_origin_bytes: usize,
}

impl Default for ServiceMapLinkLimits {
    fn default() -> Self {
        Self {
            headers: parser::MAX_HEADERS,
            header_bytes: parser::MAX_HEADER_BYTES,
            link_field_bytes: parser::MAX_LINK_FIELD_BYTES,
            target_bytes: parser::MAX_TARGET_BYTES,
            candidates: parser::MAX_CANDIDATES,
            parameters_per_member: parser::MAX_PARAMETERS,
            relation_tokens_per_member: parser::MAX_RELATION_TOKENS,
            observations: parser::MAX_OBSERVATIONS,
            target_origin_bytes: parser::MAX_TARGET_ORIGIN_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl From<parser::Span> for ByteSpan {
    fn from(span: parser::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceMapLinkCounts {
    pub headers_inspected: usize,
    pub header_bytes_admitted: usize,
    pub link_fields_inspected: usize,
    pub candidates_inspected: usize,
    pub members_framed: usize,
    pub parameters_inspected: usize,
    pub relation_tokens_inspected: usize,
    pub unknown_relation_tokens_inspected: usize,
}

impl From<parser::Counts> for ServiceMapLinkCounts {
    fn from(counts: parser::Counts) -> Self {
        Self {
            headers_inspected: counts.headers_inspected,
            header_bytes_admitted: counts.header_bytes_admitted,
            link_fields_inspected: counts.link_fields_inspected,
            candidates_inspected: counts.candidates_inspected,
            members_framed: counts.members_framed,
            parameters_inspected: counts.parameters_inspected,
            relation_tokens_inspected: counts.relation_tokens_inspected,
            unknown_relation_tokens_inspected: counts.unknown_relation_tokens_inspected,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkDeclarationObservation {
    pub relation: String,
    pub target_origin: String,
    pub source: String,
    pub strength: String,
    pub header_name: String,
    pub header_index: usize,
    pub header_value_bytes: usize,
    pub link_value_span: ByteSpan,
    pub target_span: ByteSpan,
    pub relation_span: ByteSpan,
    pub rule_id: String,
    pub offset_basis: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceMapLinkDeclarations {
    pub schema_version: u32,
    pub relation_catalog_version: u32,
    pub session_id: Uuid,
    pub evidence_id: String,
    pub available: bool,
    pub status: String,
    /// Completeness covers this bounded literal grammar only, not dependencies.
    pub complete: bool,
    pub reasons: Vec<String>,
    pub source_origin: Option<String>,
    pub source_sequence: Option<String>,
    pub source_started_at: Option<DateTime<Utc>>,
    pub source_method: Option<String>,
    pub source_status: Option<u16>,
    pub retained_header_count: Option<usize>,
    pub coverage: String,
    pub offset_basis: String,
    pub counts: ServiceMapLinkCounts,
    pub observations: Vec<LinkDeclarationObservation>,
    pub limits: ServiceMapLinkLimits,
}

impl ServiceMapLinkDeclarations {
    pub fn missing(session_id: Uuid, evidence_id: String, reason: &str) -> Self {
        let reason = if ACQUISITION_REASONS.contains(&reason) {
            reason
        } else {
            "link_inspection_failed"
        };
        Self {
            schema_version: 1,
            relation_catalog_version: 1,
            session_id,
            evidence_id,
            available: false,
            status: "unavailable".into(),
            complete: false,
            reasons: vec![reason.into()],
            source_origin: None,
            source_sequence: None,
            source_started_at: None,
            source_method: None,
            source_status: None,
            retained_header_count: None,
            coverage: "absolute_http_link_declarations".into(),
            offset_basis: "retained_header_value_utf8".into(),
            counts: ServiceMapLinkCounts::default(),
            observations: Vec::new(),
            limits: ServiceMapLinkLimits::default(),
        }
    }

    pub(crate) fn unsupported(session_id: Uuid, evidence_id: String) -> Self {
        let mut result = Self::missing(session_id, evidence_id, "unsupported_source_context");
        result.status = "unsupported".into();
        result
    }
}

/// Reuse the accepted parser's target grammar without broadening its public API.
/// The full-span check prevents delimiter injection into the synthetic member.
pub fn is_canonical_target_origin(value: &str) -> bool {
    if value.len() > parser::MAX_TARGET_ORIGIN_BYTES {
        return false;
    }
    let member = format!("<{value}>; rel=preconnect");
    let report = parser::inspect(&[parser::RetainedHeader {
        name: "link",
        value: &member,
    }]);
    report.coverage == parser::Coverage::CompleteWithinFixedGrammar
        && report.observations.len() == 1
        && report.observations[0].target_origin == value
        && report.observations[0].target_span
            == (parser::Span {
                start: 1,
                end: value.len() + 1,
            })
}

/// Only this bounded prefix is copied under the store lock. No body metadata,
/// locator, raw request headers, or skipped response-header tail is inspected.
pub(crate) struct LinkSource {
    origin: String,
    sequence: u64,
    started_at: DateTime<Utc>,
    method: String,
    retained_header_count: usize,
    headers: Vec<HeaderRecord>,
    admission_stop: Option<parser::Reason>,
}

impl LinkSource {
    pub(crate) fn from_record(record: &TransactionRecord) -> Result<Self, &'static str> {
        if !matches!(record.kind, TrafficKind::Http)
            || !matches!(record.method.as_str(), "GET" | "HEAD")
            || !(record.scheme.eq_ignore_ascii_case("http")
                || record.scheme.eq_ignore_ascii_case("https"))
            || record.status != Some(200)
        {
            return Err("unsupported_source_context");
        }
        let response = record.response.as_ref().ok_or("response_unavailable")?;
        let (base, _) = request_target(record).ok_or("metadata_unavailable")?;
        let mut bytes = 0;
        let mut headers = Vec::with_capacity(response.headers.len().min(parser::MAX_HEADERS));
        let mut admission_stop = None;
        for (index, header) in response.headers.iter().enumerate() {
            if index == parser::MAX_HEADERS {
                admission_stop = Some(parser::Reason::HeaderCountLimit);
                break;
            }
            let remaining = parser::MAX_HEADER_BYTES - bytes;
            if header.name.len() > remaining || header.value.len() > remaining - header.name.len() {
                admission_stop = Some(parser::Reason::HeaderByteLimit);
                break;
            }
            bytes += header.name.len() + header.value.len();
            headers.push(header.clone());
        }
        Ok(Self {
            origin: base.origin().ascii_serialization(),
            sequence: record.sequence,
            started_at: record.started_at,
            method: record.method.clone(),
            retained_header_count: response.headers.len(),
            headers,
            admission_stop,
        })
    }
}

fn reason_code(reason: parser::Reason) -> &'static str {
    use parser::Reason::*;
    match reason {
        HeaderCountLimit => "header_count_limit",
        HeaderByteLimit => "header_byte_limit",
        LinkFieldByteLimit => "link_field_byte_limit",
        CandidateLimit => "candidate_limit",
        ParameterLimit => "parameter_limit",
        RelationTokenLimit => "relation_token_limit",
        ObservationLimit => "observation_limit",
        MalformedField => "malformed_field",
        UnsupportedTarget => "unsupported_target",
        AnchorPresent => "anchor_present",
        MissingRelation => "missing_relation",
        UnsupportedRelationSyntax => "unsupported_relation_syntax",
    }
}

pub(crate) fn inspect_source(
    session_id: Uuid,
    evidence_id: String,
    source: LinkSource,
) -> ServiceMapLinkDeclarations {
    let headers: Vec<_> = source
        .headers
        .iter()
        .map(|header| parser::RetainedHeader {
            name: &header.name,
            value: &header.value,
        })
        .collect();
    let report = parser::inspect(&headers);
    let mut reasons: BTreeSet<_> = report.reasons.iter().copied().map(reason_code).collect();
    // Prefix acquisition omits the first header rejected by its admission cap.
    // Restore that cap only when parsing reached the prefix end. CandidateLimit
    // stops the entire row first, even if it occurred inside the last copied field.
    if !report.reasons.contains(&parser::Reason::CandidateLimit) {
        if let Some(reason) = source.admission_stop {
            reasons.insert(reason_code(reason));
        }
    }
    let mut result =
        ServiceMapLinkDeclarations::missing(session_id, evidence_id, "link_inspection_failed");
    result.available = true;
    result.status = "scanned".into();
    result.complete = reasons.is_empty();
    result.reasons = reasons.into_iter().map(str::to_owned).collect();
    result.source_origin = Some(source.origin);
    result.source_sequence = Some(source.sequence.to_string());
    result.source_started_at = Some(source.started_at);
    result.source_method = Some(source.method);
    result.source_status = Some(200);
    result.retained_header_count = Some(source.retained_header_count);
    result.counts = report.counts.into();
    result.observations = report
        .observations
        .into_iter()
        .map(|observation| LinkDeclarationObservation {
            relation: observation.relation.as_str().into(),
            target_origin: observation.target_origin,
            source: observation.source.into(),
            strength: observation.strength.into(),
            header_name: observation.header_name.into(),
            header_index: observation.header_index,
            header_value_bytes: observation.header_value_bytes,
            link_value_span: observation.link_value_span.into(),
            target_span: observation.target_span.into(),
            relation_span: observation.relation_span.into(),
            rule_id: observation.rule_id.into(),
            offset_basis: "retained_header_value_utf8".into(),
        })
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BodyEncoding, MessageRecord};

    fn header(name: &str, value: &str) -> HeaderRecord {
        HeaderRecord {
            name: name.into(),
            value: value.into(),
        }
    }

    fn record(headers: Vec<HeaderRecord>) -> TransactionRecord {
        let message = MessageRecord {
            headers,
            body_preview: "SYNTHETIC_PRIVATE_BODY".into(),
            body_encoding: BodyEncoding::Base64,
            body_size: usize::MAX,
            decoded_body_size: None,
            preview_truncated: true,
            content_type: Some("UNSUPPORTED_PRIVATE_MIME".into()),
            content_decoded: false,
        };
        let mut record = TransactionRecord::http(
            "2026-10-12T00:00:00Z".parse().unwrap(),
            "GET".into(),
            "https".into(),
            "example.com".into(),
            "/private?token=HIDDEN".into(),
            Some(200),
            1,
            message.clone(),
            Some(message),
            Vec::new(),
            None,
            None,
        );
        record.sequence = 42;
        record
    }

    fn scan(record: &TransactionRecord) -> ServiceMapLinkDeclarations {
        inspect_source(
            Uuid::nil(),
            "synthetic-evidence".into(),
            LinkSource::from_record(record).unwrap(),
        )
    }

    #[test]
    fn link_declarations_preserve_exact_header_provenance_without_private_values() {
        let value = "<https://docs.example.com/private?token=HIDDEN>; rel=\"service-doc preconnect\"; title=\"PRIVATE_TITLE\"";
        let row = record(vec![
            header("Content-Type", "application/octet-stream"),
            header("LiNk", value),
        ]);
        let result = scan(&row);
        assert!(result.available && result.complete);
        assert_eq!(result.status, "scanned");
        assert_eq!(result.source_origin.as_deref(), Some("https://example.com"));
        assert_eq!(result.source_sequence.as_deref(), Some("42"));
        assert_eq!(result.source_method.as_deref(), Some("GET"));
        assert_eq!(result.source_status, Some(200));
        assert_eq!(result.retained_header_count, Some(2));
        assert_eq!(
            result.counts,
            ServiceMapLinkCounts {
                headers_inspected: 2,
                header_bytes_admitted: 140,
                link_fields_inspected: 1,
                candidates_inspected: 1,
                members_framed: 1,
                parameters_inspected: 2,
                relation_tokens_inspected: 2,
                unknown_relation_tokens_inspected: 0,
            }
        );
        assert_eq!(result.observations.len(), 2);
        for observation in &result.observations {
            assert_eq!(observation.header_index, 1);
            assert_eq!(observation.header_name, "link");
            assert_eq!(observation.header_value_bytes, value.len());
            assert_eq!(
                observation.link_value_span,
                ByteSpan {
                    start: 0,
                    end: value.len()
                }
            );
            assert_eq!(
                &value[observation.target_span.start..observation.target_span.end],
                "https://docs.example.com/private?token=HIDDEN"
            );
            assert_eq!(
                &value[observation.relation_span.start..observation.relation_span.end],
                observation.relation
            );
            assert_eq!(observation.target_origin, "https://docs.example.com");
        }
        assert_eq!(
            result.observations[0].target_span,
            result.observations[1].target_span
        );
        assert!(
            result.observations[0].relation_span.end < result.observations[1].relation_span.start
        );
        let json = serde_json::to_string(&result).unwrap();
        for private in [
            "PRIVATE",
            "HIDDEN",
            "token=",
            "/private",
            "<https",
            "application/octet-stream",
        ] {
            assert!(!json.contains(private), "leaked {private}");
        }
        let mut maximum = row;
        maximum.sequence = u64::MAX;
        assert_eq!(
            scan(&maximum).source_sequence.as_deref(),
            Some("18446744073709551615")
        );
    }

    #[test]
    fn link_declarations_source_policy_is_independent_of_bodies_and_mime() {
        let mut row = record(vec![header(
            "Link",
            "<https://example.com>; rel=preconnect",
        )]);
        for method in ["GET", "HEAD"] {
            row.method = method.into();
            assert!(scan(&row).complete);
        }
        for (method, status, scheme) in [
            ("POST", 200, "https"),
            ("CONNECT", 200, "https"),
            ("GET", 101, "https"),
            ("GET", 201, "https"),
            ("GET", 204, "https"),
            ("GET", 206, "https"),
            ("GET", 304, "https"),
            ("GET", 200, "wss"),
        ] {
            row.method = method.into();
            row.status = Some(status);
            row.scheme = scheme.into();
            assert_eq!(
                LinkSource::from_record(&row).err(),
                Some("unsupported_source_context")
            );
        }
        row.method = "GET".into();
        row.status = Some(200);
        row.scheme = "HTTPS".into();
        row.kind = TrafficKind::Tunnel;
        assert_eq!(
            LinkSource::from_record(&row).err(),
            Some("unsupported_source_context")
        );
        row.kind = TrafficKind::Http;
        row.host = "invalid/path".into();
        assert_eq!(
            LinkSource::from_record(&row).err(),
            Some("metadata_unavailable")
        );
        row.host = "example.com".into();
        row.response = None;
        assert_eq!(
            LinkSource::from_record(&row).err(),
            Some("response_unavailable")
        );
    }

    #[test]
    fn link_declarations_acquisition_envelopes_are_null_zero_and_allowlisted() {
        for reason in ACQUISITION_REASONS
            .iter()
            .copied()
            .chain(["SYNTHETIC_PRIVATE_ERROR"])
        {
            let result = ServiceMapLinkDeclarations::missing(Uuid::nil(), "token".into(), reason);
            assert!(!result.available && !result.complete);
            assert_eq!(result.status, "unavailable");
            assert_eq!(
                result.reasons,
                [if ACQUISITION_REASONS.contains(&reason) {
                    reason
                } else {
                    "link_inspection_failed"
                }]
            );
            assert_eq!(result.counts, ServiceMapLinkCounts::default());
            assert!(result.observations.is_empty());
            let json = serde_json::to_value(result).unwrap();
            for field in [
                "source_origin",
                "source_sequence",
                "source_started_at",
                "source_method",
                "source_status",
                "retained_header_count",
            ] {
                assert!(json.as_object().unwrap().contains_key(field));
                assert!(json[field].is_null());
            }
            assert!(!json.to_string().contains("SYNTHETIC_PRIVATE_ERROR"));
        }
        let unsupported = ServiceMapLinkDeclarations::unsupported(Uuid::nil(), "token".into());
        assert_eq!(unsupported.status, "unsupported");
        assert_eq!(unsupported.reasons, ["unsupported_source_context"]);
        let empty = scan(&record(vec![]));
        assert!(empty.complete && empty.available);
        assert_eq!(empty.retained_header_count, Some(0));
        assert_eq!(empty.counts, ServiceMapLinkCounts::default());
    }

    fn assert_admission_parity(headers: Vec<HeaderRecord>) {
        let full: Vec<_> = headers
            .iter()
            .map(|header| parser::RetainedHeader {
                name: &header.name,
                value: &header.value,
            })
            .collect();
        let expected = parser::inspect(&full);
        let result = scan(&record(headers));
        let reasons: BTreeSet<_> = expected.reasons.iter().copied().map(reason_code).collect();
        assert_eq!(result.reasons, reasons.into_iter().collect::<Vec<_>>());
        assert_eq!(
            result.complete,
            expected.coverage == parser::Coverage::CompleteWithinFixedGrammar
        );
        assert_eq!(result.counts, expected.counts.into());
        assert_eq!(result.observations.len(), expected.observations.len());
        for (actual, expected) in result.observations.iter().zip(expected.observations) {
            assert_eq!(actual.target_origin, expected.target_origin);
            assert_eq!(actual.header_index, expected.header_index);
            assert_eq!(actual.link_value_span, expected.link_value_span.into());
            assert_eq!(actual.target_span, expected.target_span.into());
            assert_eq!(actual.relation_span, expected.relation_span.into());
        }
    }

    #[test]
    fn link_declarations_copied_prefix_matches_direct_parser_at_all_admission_caps() {
        let member = "<https://a>; rel=x";
        for candidates in [0, 1, 127, 128, 129] {
            for padding in [0, 126, 127, 128, 129] {
                for tail_bytes in [
                    0,
                    parser::MAX_HEADER_BYTES - 1,
                    parser::MAX_HEADER_BYTES,
                    parser::MAX_HEADER_BYTES + 1,
                ] {
                    let mut headers = vec![header("x", ""); padding];
                    headers.push(header("Link", &vec![member; candidates].join(",")));
                    headers.push(header("x", &"a".repeat(tail_bytes)));
                    headers.push(header("Link", "<https://example.com>; rel=service-doc"));
                    assert_admission_parity(headers);
                }
            }
        }
        // Candidate exhaustion before a later byte/count admission stop must
        // not claim inspection of that later cap or copied header tail.
        for cap in ["bytes", "count"] {
            let mut headers = vec![header("link", &vec![member; 129].join(","))];
            if cap == "bytes" {
                headers.push(header("x", &"a".repeat(parser::MAX_HEADER_BYTES)));
            } else {
                headers.extend(vec![header("x", ""); 128]);
            }
            let result = scan(&record(headers.clone()));
            assert_eq!(result.reasons, ["candidate_limit"]);
            assert_eq!(result.counts.headers_inspected, 1);
            assert_admission_parity(headers);
        }
        assert_admission_parity(vec![
            header("link", &"a".repeat(parser::MAX_LINK_FIELD_BYTES + 1)),
            header("x", &"a".repeat(parser::MAX_HEADER_BYTES)),
        ]);
    }

    #[test]
    fn link_declarations_target_helper_retains_strict_origin_subset() {
        for value in [
            "https://example.com",
            "http://127.0.0.1:8080",
            "https://[2001:db8::1]",
        ] {
            assert!(is_canonical_target_origin(value), "{value}");
        }
        for value in [
            "https://example.com/",
            "https://EXAMPLE.com",
            "https://example.com:443",
            "https://a_b.example.com",
            "https://user@example.com",
            "https://@example.com",
            "http://127.1",
            "https://example.com?private=1",
            "https://example.com>; rel=preconnect, <https://example.com",
        ] {
            assert!(!is_canonical_target_origin(value), "{value}");
        }
    }
}
