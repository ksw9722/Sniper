//! The parameters one endpoint was called with, read on request from its newest
//! retained rows: query string pairs, and form, JSON or multipart body fields.
//! Burp's Analyze target lists parameters the same way, by name with the values
//! it saw.
use std::collections::{BTreeMap, BTreeSet};

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{model::BodyEncoding, session::BodyLocator};

/// Newest matching rows read per request.
pub const MAX_ROWS: usize = 200;
pub const MAX_PARAMETERS: usize = 200;
pub const MAX_EXAMPLES: usize = 3;
pub const MAX_EXAMPLE_CHARS: usize = 80;
/// Body bytes parsed per row; a longer body is parsed from its start.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EndpointParameter {
    pub name: String,
    /// `query`, `form`, `json` or `multipart`.
    pub location: String,
    /// Rows that carried it, counted once per row.
    pub rows: usize,
    /// Distinct values as seen, each cut to `MAX_EXAMPLE_CHARS`.
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EndpointParameters {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    pub origin: String,
    pub method: String,
    pub path: String,
    /// Matching rows read, newest first.
    pub rows: usize,
    /// More rows matched than were read.
    pub more_rows: bool,
    pub parameters: Vec<EndpointParameter>,
    pub omitted_parameters: usize,
    /// Retained rows whose body was not read once `MAX_TOTAL_BODY_BYTES` were;
    /// their query strings still count.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unread_bodies: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl EndpointParameters {
    pub fn new(session_id: Uuid, origin: String, method: String, path: String) -> Self {
        Self {
            schema_version: 1,
            session_id,
            available: true,
            reasons: Vec::new(),
            origin,
            method,
            path,
            rows: 0,
            more_rows: false,
            parameters: Vec::new(),
            omitted_parameters: 0,
            unread_bodies: 0,
        }
    }

    pub fn unavailable(
        session_id: Uuid,
        origin: String,
        method: String,
        path: String,
        reason: &str,
    ) -> Self {
        Self {
            available: false,
            reasons: vec![reason.into()],
            ..Self::new(session_id, origin, method, path)
        }
    }
}

/// A serialized row larger than this is not read.
pub const MAX_ROW_BYTES: usize = 2 * 1024 * 1024;
/// Serialized retained rows read per request, so an upload endpoint's newest
/// 200 rows cannot mean hundreds of megabytes.
pub const MAX_TOTAL_BODY_BYTES: usize = 32 * 1024 * 1024;

// Only the request body is kept; serde skips every other field of the row.
#[derive(Deserialize)]
struct DiskRequestRow {
    id: Uuid,
    #[serde(default)]
    request: Option<DiskRequest>,
}

#[derive(Deserialize)]
struct DiskRequest {
    #[serde(default)]
    body_preview: String,
    #[serde(default)]
    body_encoding: BodyEncoding,
}

/// One retained row's request body, or None when it cannot be shown to be this
/// record's UTF-8 body.
pub(crate) fn read_request_body(
    file: &mut File,
    locator: BodyLocator,
    record_id: Uuid,
) -> Option<String> {
    let len = locator.len as usize;
    if len == 0 || len > MAX_ROW_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(locator.offset)).ok()?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).ok()?;
    let row: DiskRequestRow = serde_json::from_slice(&bytes).ok()?;
    let request = row.request?;
    (row.id == record_id && request.body_encoding == BodyEncoding::Utf8)
        .then_some(request.body_preview)
}

/// One matching row: its query, and its body or where to read it.
pub(crate) struct ParameterRow {
    pub key: crate::service_map::EvidenceKey,
    pub query: Option<String>,
    pub content_type: Option<String>,
    pub body: Option<String>,
    pub locator: Option<BodyLocator>,
}

#[derive(Default)]
pub(crate) struct ParameterTally {
    by_key: BTreeMap<(&'static str, String), (usize, Vec<String>)>,
    // Keep parsed JSON types separately from the bounded, unquoted UI examples.
    // A numeric-looking string and a number can have identical display text.
    json_types: BTreeMap<String, BTreeSet<&'static str>>,
}

impl ParameterTally {
    /// One row's parameters. A name counts once per row and location.
    pub(crate) fn add_row(&mut self, query: Option<&str>, content_type: Option<&str>, body: &str) {
        let mut pairs: Vec<(&'static str, String, Option<String>)> = Vec::new();
        if let Some(query) = query.filter(|query| !query.is_empty()) {
            for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
                pairs.push(("query", name.into_owned(), Some(value.into_owned())));
            }
        }
        let content_type = content_type.unwrap_or("");
        let location = body_parameter_location(content_type);
        let mut end = body.len().min(MAX_BODY_BYTES);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        let body = &body[..end];
        if !body.is_empty() {
            if location == Some("form") {
                for (name, value) in url::form_urlencoded::parse(body.as_bytes()) {
                    pairs.push(("form", name.into_owned(), Some(value.into_owned())));
                }
            } else if location == Some("json") {
                if let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(body) {
                    for (name, value) in fields {
                        let kind = match &value {
                            serde_json::Value::Null => "null",
                            serde_json::Value::Bool(_) => "boolean",
                            serde_json::Value::Number(number)
                                if number.is_i64() || number.is_u64() =>
                            {
                                "integer"
                            }
                            serde_json::Value::Number(_) => "number",
                            serde_json::Value::String(_) => "string",
                            serde_json::Value::Array(_) => "array",
                            serde_json::Value::Object(_) => "object",
                        };
                        self.json_types
                            .entry(name.clone())
                            .or_default()
                            .insert(kind);
                        let value = match value {
                            serde_json::Value::String(text) => text,
                            other => other.to_string(),
                        };
                        pairs.push(("json", name, Some(value)));
                    }
                }
            } else if location == Some("multipart") {
                for name in multipart_field_names(content_type, body) {
                    pairs.push(("multipart", name, None));
                }
            }
        }
        let mut seen = BTreeSet::new();
        for (location, name, value) in pairs {
            let entry = self.by_key.entry((location, name.clone())).or_default();
            if seen.insert((location, name)) {
                entry.0 += 1;
            }
            if let Some(value) = value {
                let value: String = value.chars().take(MAX_EXAMPLE_CHARS).collect();
                if entry.1.len() < MAX_EXAMPLES && !entry.1.contains(&value) {
                    entry.1.push(value);
                }
            }
        }
    }

    pub(crate) fn json_types(&self) -> BTreeMap<String, BTreeSet<&'static str>> {
        self.json_types.clone()
    }

    /// Query first, then bodies; the most frequent first within each.
    pub(crate) fn finish(self) -> (Vec<EndpointParameter>, usize) {
        let order = |location: &str| {
            ["query", "form", "json", "multipart"]
                .iter()
                .position(|item| *item == location)
        };
        let mut parameters: Vec<_> = self
            .by_key
            .into_iter()
            .map(|((location, name), (rows, examples))| EndpointParameter {
                name,
                location: location.into(),
                rows,
                examples,
            })
            .collect();
        parameters.sort_by(|a, b| {
            (order(&a.location), std::cmp::Reverse(a.rows), &a.name).cmp(&(
                order(&b.location),
                std::cmp::Reverse(b.rows),
                &b.name,
            ))
        });
        let omitted = parameters.len().saturating_sub(MAX_PARAMETERS);
        parameters.truncate(MAX_PARAMETERS);
        (parameters, omitted)
    }
}

fn is_header_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// One classifier for both extraction and the schemas that describe its fields.
/// Parameters such as a boundary or profile never select the body format.
pub(crate) fn body_parameter_location(content_type: &str) -> Option<&'static str> {
    let media = content_type.split(';').next()?.trim();
    if media.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        Some("form")
    } else if media.eq_ignore_ascii_case("multipart/form-data") {
        Some("multipart")
    } else if media.split_once('/').is_some_and(|(kind, subtype)| {
        is_header_token(kind)
            && is_header_token(subtype)
            && (subtype.eq_ignore_ascii_case("json")
                || (subtype.len() > 5 && subtype.to_ascii_lowercase().ends_with("+json")))
    }) {
        Some("json")
    } else {
        None
    }
}

// Split parameters at their syntax boundaries, not semicolons inside quoted
// filenames. Reject incomplete/ambiguous headers rather than invent a name.
fn header_parameter(value: &str, wanted: &str) -> Option<String> {
    let (_, mut rest) = value.split_once(';')?;
    let mut found = None;
    loop {
        let (key, tail) = rest.trim_start_matches([' ', '\t']).split_once('=')?;
        let key = key.trim_end_matches([' ', '\t']);
        if !is_header_token(key) {
            return None;
        }
        rest = tail.trim_start_matches([' ', '\t']);
        let parameter = if let Some(quoted) = rest.strip_prefix('"') {
            let mut text = String::new();
            let mut escaped = false;
            let mut closing = None;
            for (index, ch) in quoted.char_indices() {
                if ch.is_control() {
                    return None;
                }
                if escaped {
                    text.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    closing = Some(index);
                    break;
                } else {
                    text.push(ch);
                }
            }
            rest = &quoted[closing? + 1..];
            text
        } else {
            let end = rest.find(';').unwrap_or(rest.len());
            let text = rest[..end].trim_end_matches([' ', '\t']);
            if !is_header_token(text) {
                return None;
            }
            let text = text.to_string();
            rest = &rest[end..];
            text
        };
        if key.eq_ignore_ascii_case(wanted) {
            if found.is_some() {
                return None;
            }
            found = Some(parameter);
        }
        rest = rest.trim_start_matches([' ', '\t']);
        if rest.is_empty() {
            return found;
        }
        rest = rest.strip_prefix(';')?;
    }
}

fn multipart_field_names(content_type: &str, body: &str) -> Vec<String> {
    let Some(boundary) = header_parameter(content_type, "boundary") else {
        return Vec::new();
    };
    // MIME boundary grammar is bounded and case-sensitive, even when quoted.
    if boundary.is_empty()
        || boundary.len() > 70
        || boundary.ends_with(' ')
        || !boundary
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&byte))
    {
        return Vec::new();
    }
    let delimiter = format!("--{boundary}");
    let closing = format!("{delimiter}--");
    let mut names = Vec::new();
    let mut in_headers = false;
    let mut headers: Vec<String> = Vec::new();
    let mut valid_headers = true;
    for line in body.split_inclusive("\r\n") {
        let Some(line) = line.strip_suffix("\r\n") else {
            break;
        };
        let boundary_line = line.trim_end_matches([' ', '\t']);
        if boundary_line == closing {
            break;
        }
        if boundary_line == delimiter {
            in_headers = true;
            headers.clear();
            valid_headers = true;
        } else if in_headers {
            if line.is_empty() {
                if valid_headers {
                    names.extend(multipart_header_name(&headers));
                }
                headers.clear();
                in_headers = false;
            } else if line.starts_with([' ', '\t']) {
                // MIME headers may fold. Unfold only while in a real part's
                // header block, never header-looking text in its payload.
                if let Some(header) = headers.last_mut() {
                    header.push(' ');
                    header.push_str(line.trim_start_matches([' ', '\t']));
                } else {
                    valid_headers = false;
                }
            } else {
                headers.push(line.to_string());
            }
        }
    }
    names
}

fn multipart_header_name(headers: &[String]) -> Option<String> {
    let mut disposition = None;
    for line in headers {
        let (header, value) = line.split_once(':')?;
        if !is_header_token(header) {
            return None;
        }
        if header.eq_ignore_ascii_case("content-disposition") {
            if disposition.is_some() {
                return None;
            }
            disposition = Some(value);
        }
    }
    let value = disposition?;
    if !value
        .split(';')
        .next()?
        .trim()
        .eq_ignore_ascii_case("form-data")
    {
        return None;
    }
    header_parameter(value, "name").filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_add_up_by_name_and_location_with_a_few_values() {
        let mut tally = ParameterTally::default();
        tally.add_row(Some("q=shoes&page=1&page=2"), None, "");
        tally.add_row(
            Some("q=hats"),
            Some("application/json; charset=utf-8"),
            r#"{"id":7,"tags":["a"],"q":"in body"}"#,
        );
        tally.add_row(
            None,
            Some("application/x-www-form-urlencoded"),
            "user=kim&token=abc%20def",
        );
        tally.add_row(
            None,
            Some("multipart/form-data; boundary=x"),
            "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.png\"\r\n\r\n...",
        );
        let (parameters, omitted) = tally.finish();
        let view: Vec<_> = parameters
            .iter()
            .map(|item| {
                (
                    item.location.as_str(),
                    item.name.as_str(),
                    item.rows,
                    item.examples.join("|"),
                )
            })
            .collect();
        assert_eq!(
            view,
            [
                ("query", "q", 2, "shoes|hats".to_string()),
                ("query", "page", 1, "1|2".to_string()),
                ("form", "token", 1, "abc def".to_string()),
                ("form", "user", 1, "kim".to_string()),
                ("json", "id", 1, "7".to_string()),
                ("json", "q", 1, "in body".to_string()),
                ("json", "tags", 1, "[\"a\"]".to_string()),
                ("multipart", "file", 1, String::new()),
            ]
        );
        assert_eq!(omitted, 0);
    }

    #[test]
    fn a_long_value_is_cut_and_a_long_body_parsed_from_its_start() {
        let mut tally = ParameterTally::default();
        let long = "x".repeat(500);
        tally.add_row(Some(&format!("a={long}")), None, "");
        let body = format!("b=1&c={}", "y".repeat(MAX_BODY_BYTES));
        tally.add_row(None, Some("application/x-www-form-urlencoded"), &body);
        let (parameters, _) = tally.finish();
        assert_eq!(parameters[0].examples[0].chars().count(), MAX_EXAMPLE_CHARS);
        assert!(parameters.iter().any(|item| item.name == "b"));
    }

    #[test]
    fn native_json_types_survive_identical_and_capped_display_examples() {
        let mut tally = ParameterTally::default();
        for body in [
            r#"{"value":"42","other":"false"}"#,
            r#"{"value":42,"other":false}"#,
            r#"{"value":null}"#,
            r#"{"value":[],"other":"[draft"}"#,
            r#"{"value":{},"other":"{draft"}"#,
            r#"{"value":true}"#,
            r#"{"value":1.5}"#,
        ] {
            tally.add_row(None, Some("application/json"), body);
        }
        assert_eq!(
            tally.json_types()["value"],
            BTreeSet::from(["string", "integer", "null", "array", "object", "boolean", "number"])
        );
        assert_eq!(
            tally.json_types()["other"],
            BTreeSet::from(["string", "boolean"])
        );
        let (parameters, _) = tally.finish();
        let value = parameters
            .iter()
            .find(|parameter| parameter.name == "value")
            .unwrap();
        assert_eq!(value.examples, ["42", "null", "[]"]);
        assert_eq!(value.rows, 7);
        let serialized = serde_json::to_value(value).unwrap();
        assert_eq!(
            serialized.as_object().unwrap().len(),
            4,
            "parameter display wire shape stays unchanged"
        );
    }

    fn body_parameters(content_type: &str, body: &str) -> Vec<EndpointParameter> {
        let mut tally = ParameterTally::default();
        tally.add_row(None, Some(content_type), body);
        tally.finish().0
    }

    fn multipart(boundary: &str, disposition: &str, body: &str) -> String {
        format!("--{boundary}\r\nContent-Disposition: {disposition}\r\nContent-Type: text/plain\r\n\r\n{body}\r\n--{boundary}--\r\n")
    }

    #[test]
    fn multipart_names_come_only_from_part_dispositions() {
        let body = multipart(
            "control",
            r#"form-data; name="file"; filename="fixture; name=\"filename_fake\".txt""#,
            "literal ; name=\"invented\"\r\nContent-Disposition: form-data; name=\"header_fake\"\r\n\r\nSYNTHETIC_PRIVATE_UPLOAD",
        );
        let parameters = body_parameters("multipart/form-data; boundary=control", &body);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "file");
        assert_eq!(parameters[0].location, "multipart");
        assert!(parameters[0].examples.is_empty());
    }

    #[test]
    fn media_type_parameters_do_not_choose_the_body_parser() {
        let body = multipart(
            "json-boundary",
            r#"form-data; name="file""#,
            "plain content",
        );
        let parameters = body_parameters("multipart/form-data; boundary=json-boundary", &body);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "file");
        assert_eq!(parameters[0].location, "multipart");
        for mime in [
            "APPLICATION/JSON; charset=utf-8",
            "application/problem+json; profile=x-www-form-urlencoded",
            "application/vnd.example+json",
            "text/json",
        ] {
            let parameters = body_parameters(mime, r#"{"field":42}"#);
            assert_eq!(parameters.len(), 1, "{mime}");
            assert_eq!(parameters[0].location, "json", "{mime}");
        }
        for mime in [
            "text/plain; profile=application/json",
            "application/notjson",
            "application/json-seq",
            "application/x-www-form-urlencoded-fake",
            "text/plain; profile=multipart/form-data; boundary=x",
        ] {
            assert!(
                body_parameters(mime, r#"{"field":42}"#).is_empty(),
                "{mime}"
            );
        }
    }

    #[test]
    fn multipart_preserves_named_fields_and_case_sensitive_quoted_boundaries() {
        let body = concat!(
            "preamble ; name=\"preamble_fake\"\r\n",
            "--AbC\r\ncOnTeNt-DiSpOsItIoN: FoRm-DaTa; NAME=\"café items[]\"\r\n\r\nvalue\r\n",
            "--AbC\r\nContent-Disposition: form-data; name=token_name\r\n\r\nvalue\r\n",
            "--AbC\r\nContent-Disposition: form-data; name=\"café items[]\"\r\n\r\nvalue\r\n",
            "--AbC--\r\nContent-Disposition: form-data; name=\"epilogue_fake\"\r\n\r\n"
        );
        let mut tally = ParameterTally::default();
        for _ in 0..2 {
            tally.add_row(None, Some("Multipart/Form-Data; boundary=\"AbC\""), body);
        }
        let parameters = tally.finish().0;
        assert_eq!(
            parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["café items[]", "token_name"]
        );
        assert!(parameters
            .iter()
            .all(|p| p.rows == 2 && p.examples.is_empty()));
        assert!(body_parameters("multipart/form-data; boundary=abc", body).is_empty());
    }

    #[test]
    fn multipart_unfolds_disposition_and_other_mime_headers() {
        for headers in [
            "Content-Disposition: form-data; name=\"file\";\r\n filename=\"fixture.txt\"",
            "Content-Disposition: form-data;\r\n\tname=\"file\"",
            "Content-Disposition: form-data; name=\"file\"\r\nContent-Type: text/plain;\r\n charset=utf-8",
        ] {
            let body = format!("--x\r\n{headers}\r\n\r\nbody\r\n--x--\r\n");
            let parameters = body_parameters("multipart/form-data; boundary=x", &body);
            assert_eq!(parameters.len(), 1, "{headers:?}");
            assert_eq!(parameters[0].name, "file");
        }
    }

    #[test]
    fn malformed_or_unrelated_dispositions_do_not_invent_names() {
        for disposition in [
            r#"form-data; name="""#,
            r#"form-data; name="unterminated"#,
            "form-data; name=\"broken\r\nname\"",
            r#"form-data; filename="fixture; name=\"invented\".txt""#,
            r#"form-data; x-name="invented""#,
            r#"attachment; name="invented""#,
            r#"form-data; name="first"; name="second""#,
            r#"form-data; name="first"junk"#,
        ] {
            assert!(
                body_parameters(
                    "multipart/form-data; boundary=x",
                    &multipart("x", disposition, "")
                )
                .is_empty(),
                "{disposition:?}"
            );
        }
        let body = "--x\r\nX-Note: form-data; name=\"invented\"\r\n\r\nbody\r\n--x--\r\n";
        assert!(body_parameters("multipart/form-data; boundary=x", body).is_empty());
    }

    #[test]
    fn multipart_requires_complete_headers_within_the_body_budget() {
        for body in [
            "--x\r\nContent-Disposition: form-data; name=\"truncated",
            "--x\r\nContent-Disposition: form-data; name=\"truncated\"\r\n",
            "--x-extra\r\nContent-Disposition: form-data; name=\"fake\"\r\n\r\nbody",
        ] {
            assert!(body_parameters("multipart/form-data; boundary=x", body).is_empty());
        }
        let body = format!("--x\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\n{}\r\n--x\r\nContent-Disposition: form-data; name=\"past_budget\"\r\n\r\nvalue", "é".repeat(MAX_BODY_BYTES));
        let parameters = body_parameters("multipart/form-data; boundary=x", &body);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "file");
        assert!(parameters[0].examples.is_empty());
    }

    #[test]
    fn nested_multipart_and_empty_or_unsupported_bodies_stay_unexpanded() {
        let nested = multipart("inner", r#"form-data; name="nested""#, "value");
        let body = multipart("outer", r#"form-data; name="container""#, &nested);
        let parameters = body_parameters("multipart/form-data; boundary=outer", &body);
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "container");
        for mime in [
            "multipart/form-data",
            "multipart/form-data; boundary=",
            "multipart/mixed; boundary=outer",
            "text/plain",
        ] {
            assert!(body_parameters(mime, &body).is_empty(), "{mime}");
        }
        assert!(body_parameters("multipart/form-data; boundary=outer", "").is_empty());
        assert!(body_parameters("application/json", "[]").is_empty());
    }
}
