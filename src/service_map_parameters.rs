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
        let mime = content_type.unwrap_or("").to_ascii_lowercase();
        let mut end = body.len().min(MAX_BODY_BYTES);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        let body = &body[..end];
        if !body.is_empty() {
            if mime.contains("x-www-form-urlencoded") {
                for (name, value) in url::form_urlencoded::parse(body.as_bytes()) {
                    pairs.push(("form", name.into_owned(), Some(value.into_owned())));
                }
            } else if mime.contains("json") {
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
            } else if mime.contains("multipart/form-data") {
                // Field names only: a part can be a file. `; name="` never
                // matches inside `; filename="`.
                for part in body.split("; name=\"").skip(1) {
                    if let Some(name) = part.split('"').next().filter(|name| !name.is_empty()) {
                        pairs.push(("multipart", name.to_string(), None));
                    }
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
}
