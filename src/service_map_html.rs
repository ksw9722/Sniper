//! Explicit, passive declarations in one guarded retained HTML response preview.
//! Products and versions are reported literals, never verified installations.
use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    service_map_body::{BodySource, RetainedPreview, MAX_SERIALIZED_ROW_BYTES},
    service_map_html_parser as parser,
};

pub const MAX_SCAN_BYTES: usize = parser::MAX_INPUT_BYTES;
pub const MAX_TAG_CANDIDATES: usize = parser::MAX_TAG_CANDIDATES;
pub const MAX_TAG_BYTES: usize = parser::MAX_TAG_BYTES;
pub const MAX_ATTRIBUTES: usize = parser::MAX_ATTRIBUTES;
pub const MAX_OBSERVATIONS: usize = parser::MAX_OBSERVATIONS;
pub const MAX_VERSION_BYTES: usize = parser::MAX_VERSION_BYTES;
pub const MAX_VERSION_SEPARATOR_BYTES: usize = parser::MAX_VERSION_SEPARATOR_BYTES;

pub const ACQUISITION_REASONS: &[&str] = &[
    "invalid_evidence_id",
    "session_mismatch",
    "stale_generation",
    "record_missing",
    "record_changed",
    "body_identity_unverifiable",
    "response_unavailable",
    "metadata_limit",
    "unsupported_content_type",
    "unsupported_body_encoding",
    "unsupported_content_encoding",
    "unsupported_metadata",
    "serialized_row_limit",
    "body_storage_unavailable",
    "body_read_failed",
    "malformed_retained_row",
    "retained_preview_unavailable",
    "body_scan_failed",
    "inactive_session",
    "session_source_changed",
    "metadata_unavailable",
];
pub const TRUNCATION_REASONS: &[&str] = &["capture_preview_truncated", "scan_prefix_truncated"];
pub const PARSER_STOP_REASONS: &[&str] = &[
    "end_of_preview",
    "input_limit",
    "tag_candidate_limit",
    "tag_byte_limit",
    "attribute_limit",
    "observation_limit",
    "unsupported_document_context",
    "unsupported_head_context",
    "unsupported_doctype",
    "unsupported_comment",
    "unsupported_script_escape",
    "unsupported_entity",
    "unsupported_self_closing",
    "malformed_tag",
    "duplicate_attribute",
    "null_byte",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapHtmlLimits {
    pub scan_bytes: usize,
    pub serialized_row_bytes: usize,
    pub tag_candidates: usize,
    pub tag_bytes: usize,
    pub attributes: usize,
    pub observations: usize,
    pub version_bytes: usize,
    pub version_separator_bytes: usize,
}

impl Default for ServiceMapHtmlLimits {
    fn default() -> Self {
        Self {
            scan_bytes: parser::MAX_INPUT_BYTES,
            serialized_row_bytes: MAX_SERIALIZED_ROW_BYTES,
            tag_candidates: parser::MAX_TAG_CANDIDATES,
            tag_bytes: parser::MAX_TAG_BYTES,
            attributes: parser::MAX_ATTRIBUTES,
            observations: parser::MAX_OBSERVATIONS,
            version_bytes: parser::MAX_VERSION_BYTES,
            version_separator_bytes: parser::MAX_VERSION_SEPARATOR_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl From<parser::ByteSpan> for ByteSpan {
    fn from(span: parser::ByteSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HtmlGeneratorObservation {
    pub product_id: String,
    pub name: String,
    pub catalog_category: String,
    pub reported_version: Option<String>,
    pub version_status: String,
    pub source: String,
    pub strength: String,
    pub rule_id: String,
    pub tag_span: ByteSpan,
    pub content_span: ByteSpan,
    pub product_span: ByteSpan,
    pub version_span: Option<ByteSpan>,
    pub offset_basis: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HtmlParserStop {
    pub reason: String,
    /// Raw byte cursor, not a UTF-8 string-slicing boundary or accepted span.
    pub byte_cursor: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceMapHtmlGenerators {
    pub schema_version: u32,
    pub generator_catalog_version: u32,
    pub session_id: Uuid,
    pub evidence_id: String,
    pub available: bool,
    pub status: String,
    /// True only for a supported closed head with no capture, input, or output cap.
    pub complete: bool,
    pub reasons: Vec<String>,
    pub source_origin: Option<String>,
    pub source_sequence: Option<String>,
    pub source_started_at: Option<DateTime<Utc>>,
    pub retained_preview_bytes: Option<usize>,
    /// Bounded prefix offered to the parser, not bytes fully parsed.
    pub scan_prefix_bytes: usize,
    pub capture_preview_truncated: bool,
    pub scan_prefix_truncated: bool,
    pub coverage: String,
    pub head_closed: bool,
    pub parser_stop: Option<HtmlParserStop>,
    pub tag_candidates_inspected: usize,
    pub additional_tag_candidate_encountered: bool,
    pub additional_observation_encountered: bool,
    pub unknown_product_declarations: usize,
    pub observations: Vec<HtmlGeneratorObservation>,
    pub limits: ServiceMapHtmlLimits,
    pub offset_basis: String,
}

impl ServiceMapHtmlGenerators {
    pub fn missing(session_id: Uuid, evidence_id: String, reason: &str) -> Self {
        // Only fixed reader reasons may enter this envelope. Never include an
        // underlying I/O error or an arbitrary value from captured content.
        let reason = if ACQUISITION_REASONS.contains(&reason) {
            reason
        } else {
            "body_scan_failed"
        };
        Self {
            schema_version: 1,
            generator_catalog_version: 1,
            session_id,
            evidence_id,
            available: false,
            status: "unavailable".into(),
            complete: false,
            reasons: vec![reason.into()],
            source_origin: None,
            source_sequence: None,
            source_started_at: None,
            retained_preview_bytes: None,
            scan_prefix_bytes: 0,
            capture_preview_truncated: false,
            scan_prefix_truncated: false,
            coverage: "literal_html_generator_explicit_head".into(),
            head_closed: false,
            parser_stop: None,
            tag_candidates_inspected: 0,
            additional_tag_candidate_encountered: false,
            additional_observation_encountered: false,
            unknown_product_declarations: 0,
            observations: Vec::new(),
            limits: ServiceMapHtmlLimits::default(),
            offset_basis: "retained_utf8_preview".into(),
        }
    }

    pub(crate) fn unsupported(session_id: Uuid, evidence_id: String, reason: &str) -> Self {
        let mut result = Self::missing(session_id, evidence_id, reason);
        result.status = "unsupported".into();
        result
    }
}

fn stop_reason(reason: parser::StopReason) -> &'static str {
    use parser::StopReason::*;
    match reason {
        EndOfPreview => "end_of_preview",
        InputLimit => "input_limit",
        TagCandidateLimit => "tag_candidate_limit",
        TagByteLimit => "tag_byte_limit",
        AttributeLimit => "attribute_limit",
        ObservationLimit => "observation_limit",
        UnsupportedDocumentContext => "unsupported_document_context",
        UnsupportedHeadContext => "unsupported_head_context",
        UnsupportedDoctype => "unsupported_doctype",
        UnsupportedComment => "unsupported_comment",
        UnsupportedScriptEscape => "unsupported_script_escape",
        UnsupportedEntity => "unsupported_entity",
        UnsupportedSelfClosing => "unsupported_self_closing",
        MalformedTag => "malformed_tag",
        DuplicateAttribute => "duplicate_attribute",
        NullByte => "null_byte",
    }
}

pub(crate) fn scan_preview(
    session_id: Uuid,
    evidence_id: String,
    source: BodySource,
    preview: RetainedPreview,
) -> ServiceMapHtmlGenerators {
    if preview.total_bytes() == 0 && !source.proves_empty() {
        return ServiceMapHtmlGenerators::missing(
            session_id,
            evidence_id,
            "retained_preview_unavailable",
        );
    }
    let mut result = ServiceMapHtmlGenerators::missing(session_id, evidence_id, "body_scan_failed");
    result.available = true;
    result.status = "scanned".into();
    result.capture_preview_truncated = source.capture_preview_truncated()
        || source
            .expected_preview_bytes()
            .is_some_and(|expected| preview.total_bytes() < expected);
    result.source_origin = Some(source.origin);
    result.source_sequence = Some(source.sequence.to_string());
    result.source_started_at = Some(source.started_at);
    result.retained_preview_bytes = Some(preview.total_bytes());
    let inspection = parser::inspect_html_generator(preview.prefix());
    debug_assert_eq!(inspection.input_bytes_available, preview.prefix().len());
    debug_assert!(!inspection.input_limit_exceeded);
    result.scan_prefix_bytes = inspection.input_bytes_considered;
    result.scan_prefix_truncated = result.scan_prefix_bytes < preview.total_bytes();
    let mut reasons = BTreeSet::new();
    if result.capture_preview_truncated {
        reasons.insert("capture_preview_truncated");
    }
    if result.scan_prefix_truncated {
        reasons.insert("scan_prefix_truncated");
    }
    match inspection.coverage {
        parser::Coverage::ExplicitHeadClosed => result.head_closed = true,
        parser::Coverage::Partial { reason, at_byte } => {
            // The shared reader already bounded the string. Preserve the pure
            // parser while restoring the known reason for this shortened input.
            let reason =
                if reason == parser::StopReason::EndOfPreview && result.scan_prefix_truncated {
                    "input_limit"
                } else {
                    stop_reason(reason)
                };
            reasons.insert(reason);
            result.parser_stop = Some(HtmlParserStop {
                reason: reason.into(),
                byte_cursor: at_byte,
            });
        }
    }
    result.tag_candidates_inspected = inspection.tag_candidates_inspected;
    result.additional_tag_candidate_encountered = inspection.additional_tag_candidate_encountered;
    result.additional_observation_encountered = inspection.additional_observation_encountered;
    result.unknown_product_declarations = inspection.unknown_product_declarations;
    result.observations = inspection
        .observations
        .into_iter()
        .map(|observation| {
            debug_assert_eq!(
                observation.offset_basis,
                parser::OffsetBasis::RetainedUtf8PreviewBytes
            );
            let (product_id, name, rule_id) = match observation.product {
                parser::Product::WordPress => {
                    ("wordpress", "WordPress", "html_meta_generator.wordpress.v1")
                }
                parser::Product::Drupal => ("drupal", "Drupal", "html_meta_generator.drupal.v1"),
            };
            HtmlGeneratorObservation {
                product_id: product_id.into(),
                name: name.into(),
                catalog_category: "cms".into(),
                reported_version: observation.reported_version,
                version_status: match observation.version_status {
                    parser::VersionStatus::Reported => "reported",
                    parser::VersionStatus::NotReported => "not_reported",
                    parser::VersionStatus::UnsupportedFormat => "unsupported_format",
                }
                .into(),
                source: "response_html_meta_generator".into(),
                strength: "literal_declaration".into(),
                rule_id: rule_id.into(),
                tag_span: observation.tag_span.into(),
                content_span: observation.content_span.into(),
                product_span: observation.product_span.into(),
                version_span: observation.version_span.map(Into::into),
                offset_basis: "retained_utf8_preview".into(),
            }
        })
        .collect();
    result.complete = result.head_closed && reasons.is_empty();
    result.reasons = reasons.into_iter().map(str::to_owned).collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BodyEncoding, HeaderRecord, MessageRecord, TransactionRecord};

    fn record(body: &str) -> TransactionRecord {
        let response = MessageRecord {
            headers: Vec::new(),
            body_preview: body.into(),
            body_encoding: BodyEncoding::Utf8,
            body_size: body.len(),
            decoded_body_size: None,
            preview_truncated: false,
            content_type: Some("text/html; charset=utf-8".into()),
            content_decoded: false,
        };
        let mut row = TransactionRecord::http(
            Utc::now(),
            "GET".into(),
            "https".into(),
            "example.com".into(),
            "/private?credential=DO_NOT_RETURN".into(),
            Some(200),
            1,
            response.clone(),
            Some(response),
            Vec::new(),
            None,
            None,
        );
        row.sequence = u64::MAX;
        row
    }

    fn scan(row: &TransactionRecord) -> ServiceMapHtmlGenerators {
        scan_preview(
            Uuid::nil(),
            "synthetic-evidence".into(),
            BodySource::from_html_record(row).unwrap(),
            RetainedPreview::from_str(&row.response.as_ref().unwrap().body_preview),
        )
    }

    #[test]
    fn html_generators_return_only_allowlisted_literals_and_exact_byte_spans() {
        let body = "<html><head><title>é &amp; safe</title><meta name='generator' content='WordPress 6.8.1'><meta name='generator' content='Drupal 11.0-private-suffix'><meta name='generator' content='UNKNOWN_PRIVATE_PRODUCT'></head><body>DO_NOT_RETURN</body>";
        let result = scan(&record(body));
        assert!(result.available && result.complete && result.head_closed);
        assert_eq!(result.status, "scanned");
        assert!(result.reasons.is_empty());
        assert!(result.parser_stop.is_none());
        assert_eq!(
            result.source_sequence.as_deref(),
            Some("18446744073709551615")
        );
        assert_eq!(result.unknown_product_declarations, 1);
        assert_eq!(result.observations.len(), 2);
        let observation = &result.observations[0];
        assert_eq!(observation.product_id, "wordpress");
        assert_eq!(observation.reported_version.as_deref(), Some("6.8.1"));
        assert_eq!(
            &body[observation.product_span.start..observation.product_span.end],
            "WordPress"
        );
        let span = observation.version_span.unwrap();
        assert_eq!(&body[span.start..span.end], "6.8.1");
        assert_eq!(result.observations[1].version_status, "unsupported_format");
        assert!(result.observations[1].reported_version.is_none());
        assert!(result.observations[1].version_span.is_none());
        let json = serde_json::to_string(&result).unwrap();
        for secret in [
            "UNKNOWN_PRIVATE_PRODUCT",
            "private-suffix",
            "DO_NOT_RETURN",
            "<meta",
            "<body",
            "credential",
            "&amp;",
        ] {
            assert!(!json.contains(secret), "leaked {secret}");
        }
        assert_eq!(result.scan_prefix_bytes, body.len());
        assert_eq!(result.retained_preview_bytes, Some(body.len()));
    }

    #[test]
    fn html_generators_distinguish_head_closure_from_capture_and_prefix_limits() {
        let head = "<html><head><meta name='generator' content='Drupal'></head>";
        for capture in [false, true] {
            for prefix in [false, true] {
                let body = if prefix {
                    head.to_owned() + &"x".repeat(MAX_SCAN_BYTES)
                } else {
                    head.into()
                };
                let mut row = record(&body);
                row.response.as_mut().unwrap().preview_truncated = capture;
                let result = scan(&row);
                assert!(result.head_closed);
                assert!(result.parser_stop.is_none());
                assert_eq!(result.complete, !capture && !prefix);
                assert_eq!(result.capture_preview_truncated, capture);
                assert_eq!(result.scan_prefix_truncated, prefix);
                assert_eq!(result.status, "scanned");
                assert_eq!(
                    result.reasons.contains(&"capture_preview_truncated".into()),
                    capture
                );
                assert_eq!(
                    result.reasons.contains(&"scan_prefix_truncated".into()),
                    prefix
                );
            }
        }
    }

    #[test]
    fn html_generators_restore_known_input_limit_without_claiming_consumed_prefix() {
        let prefix = "<html><head><style>";
        let body = prefix.to_owned() + &"x".repeat(MAX_SCAN_BYTES - prefix.len() - 1) + "€tail";
        let result = scan(&record(&body));
        assert_eq!(result.scan_prefix_bytes, MAX_SCAN_BYTES - 1);
        assert_eq!(result.retained_preview_bytes, Some(body.len()));
        assert!(result.scan_prefix_truncated);
        assert!(!result.head_closed && !result.complete);
        let stop = result.parser_stop.unwrap();
        assert_eq!(stop.reason, "input_limit");
        assert_eq!(stop.byte_cursor, MAX_SCAN_BYTES - 1);
        assert_eq!(result.reasons, ["input_limit", "scan_prefix_truncated"]);

        let body = "<html><head><template>".to_owned() + &"x".repeat(1000);
        let result = scan(&record(&body));
        assert_eq!(result.scan_prefix_bytes, body.len());
        let stop = result.parser_stop.unwrap();
        assert_eq!(stop.reason, "unsupported_head_context");
        assert!(stop.byte_cursor < result.scan_prefix_bytes);
    }

    #[test]
    fn html_generators_capture_only_stop_stays_end_of_preview() {
        let mut row = record("<html><head>");
        row.response.as_mut().unwrap().preview_truncated = true;
        let result = scan(&row);
        assert_eq!(result.parser_stop.unwrap().reason, "end_of_preview");
        assert_eq!(
            result.reasons,
            ["capture_preview_truncated", "end_of_preview"]
        );
        let empty = scan(&record(""));
        assert!(empty.available);
        assert!(!empty.complete);
        assert_eq!(empty.parser_stop.unwrap().byte_cursor, 0);
        assert_eq!(empty.scan_prefix_bytes, 0);
    }

    #[test]
    fn html_generators_count_only_encountered_output_and_candidate_limits() {
        let one = "<meta name='generator' content='Drupal 11'>";
        let body = format!("<html><head>{}</head>", one.repeat(MAX_OBSERVATIONS + 1));
        let result = scan(&record(&body));
        assert_eq!(result.observations.len(), MAX_OBSERVATIONS);
        assert!(result.additional_observation_encountered);
        assert_eq!(result.reasons, ["observation_limit"]);
        assert!(!result.complete);
        let body = format!("<html><head>{}</head>", "<meta>".repeat(MAX_TAG_CANDIDATES));
        let result = scan(&record(&body));
        assert_eq!(result.tag_candidates_inspected, MAX_TAG_CANDIDATES);
        assert!(result.additional_tag_candidate_encountered);
        assert_eq!(result.reasons, ["tag_candidate_limit"]);
    }

    #[test]
    fn html_generators_missing_and_unsupported_have_explicit_nulls_and_no_source() {
        for result in [
            ServiceMapHtmlGenerators::missing(Uuid::nil(), "token".into(), "record_changed"),
            ServiceMapHtmlGenerators::unsupported(
                Uuid::nil(),
                "token".into(),
                "unsupported_content_type",
            ),
        ] {
            assert!(!result.available && !result.complete && !result.head_closed);
            assert!(result.observations.is_empty());
            let json = serde_json::to_value(result).unwrap();
            for field in [
                "source_origin",
                "source_sequence",
                "source_started_at",
                "retained_preview_bytes",
                "parser_stop",
            ] {
                assert_eq!(json.get(field), Some(&serde_json::Value::Null));
            }
            assert_eq!(json["scan_prefix_bytes"], 0);
            assert_eq!(json["tag_candidates_inspected"], 0);
        }
        let result =
            ServiceMapHtmlGenerators::missing(Uuid::nil(), "token".into(), "PRIVATE_RAW_ERROR");
        assert_eq!(result.reasons, ["body_scan_failed"]);
    }

    #[test]
    fn html_generators_eligibility_is_stricter_than_unchanged_url_references() {
        let mut row = record("<html><head></head>");
        for mime in [
            "application/xhtml+xml",
            "text/javascript",
            "application/javascript",
            "application/ecmascript",
            "text/ecmascript",
            "application/x-javascript",
        ] {
            row.response.as_mut().unwrap().content_type = Some(mime.into());
            assert!(BodySource::from_record(&row).is_ok(), "{mime}");
            assert_eq!(
                BodySource::from_html_record(&row).unwrap_err(),
                "unsupported_content_type"
            );
        }
        row.response.as_mut().unwrap().content_type = Some("TEXT/HTML; charset=UTF-8".into());
        assert!(BodySource::from_html_record(&row).is_ok());
        row.response.as_mut().unwrap().body_encoding = BodyEncoding::Base64;
        assert_eq!(
            BodySource::from_html_record(&row).unwrap_err(),
            "unsupported_body_encoding"
        );
        let response = row.response.as_mut().unwrap();
        response.body_encoding = BodyEncoding::Utf8;
        response.headers.push(HeaderRecord {
            name: "content-encoding".into(),
            value: "gzip".into(),
        });
        assert_eq!(
            BodySource::from_html_record(&row).unwrap_err(),
            "unsupported_content_encoding"
        );
        row.response.as_mut().unwrap().content_decoded = true;
        assert!(BodySource::from_html_record(&row).is_ok());
        // A policy longer than one value's cap still leaves the page readable;
        // only the headers' total stops the read.
        row.response.as_mut().unwrap().headers = vec![HeaderRecord {
            name: "Content-Security-Policy".into(),
            value: "x".repeat(crate::service_map::MAX_HEADER_VALUE_BYTES * 2),
        }];
        assert!(BodySource::from_html_record(&row).is_ok());
        row.response.as_mut().unwrap().headers = vec![HeaderRecord {
            name: "X-Test".into(),
            value: "PRIVATE".repeat(crate::service_map::MAX_HEADER_BYTES / 7 + 1),
        }];
        assert_eq!(
            BodySource::from_html_record(&row).unwrap_err(),
            "metadata_limit"
        );
        row.response = None;
        assert_eq!(
            BodySource::from_html_record(&row).unwrap_err(),
            "response_unavailable"
        );
    }

    #[test]
    fn html_generators_infer_lost_suffix_and_refuse_unproven_empty_preview() {
        let mut row = record("<html><head></head>");
        row.response.as_mut().unwrap().body_size += 100;
        assert!(scan(&row).capture_preview_truncated);
        let response = row.response.as_mut().unwrap();
        response.content_decoded = true;
        response.decoded_body_size = Some(response.body_preview.len());
        assert!(scan(&row).complete);
        row.response.as_mut().unwrap().decoded_body_size = Some(1000);
        assert!(scan(&row).capture_preview_truncated);
        row.response.as_mut().unwrap().body_preview.clear();
        let result = scan(&row);
        assert!(!result.available);
        assert_eq!(result.reasons, ["retained_preview_unavailable"]);
    }
}
