//! Library versions a retained script declares about itself.
//!
//! A response header can say a page is built with Next.js, but never which
//! version: `X-Powered-By: Next.js` carries none. The framework's client runtime
//! does, as `window.next={version:"…"}`. The map reads that from JavaScript rows
//! under `/_next/`, once per row revision, and attributes it to the page that
//! loaded the script.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use serde::Deserialize;
use uuid::Uuid;

use crate::{model::BodyEncoding, service_map::ServiceMapRow, session::BodyLocator};

/// Retained rows read per map; the rest are read on later maps.
pub const MAX_SCRIPT_READS: usize = 64;
/// A serialized row larger than this is not read at all.
pub const MAX_SCRIPT_ROW_BYTES: usize = 2 * 1024 * 1024;

/// A JavaScript response under Next.js's asset path, the only rows read.
pub(crate) fn is_candidate(row: &ServiceMapRow) -> bool {
    row.path.contains("/_next/")
        && row
            .content_type
            .as_deref()
            .is_some_and(|mime| mime.contains("javascript") || mime.contains("ecmascript"))
}

/// The version Next.js's client runtime names in this script, if it does.
pub(crate) fn next_version(script: &str) -> Option<String> {
    const ANCHOR: &str = "window.next={version:";
    for (at, _) in script.match_indices(ANCHOR) {
        let rest = &script[at + ANCHOR.len()..];
        // The App Router writes the literal. The Pages Router names a variable
        // assigned earlier in the bundle. ponytail: the nearest `name="…"`
        // before the anchor is taken as its value; a parser would be exact.
        if let Some(literal) = rest.strip_prefix('"') {
            if let Some(version) = version_literal(literal) {
                return Some(version);
            }
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$'))
            .collect();
        if name.is_empty() {
            continue;
        }
        let mut start = at.saturating_sub(4096);
        while !script.is_char_boundary(start) {
            start += 1;
        }
        let before = &script[start..at];
        if let Some(found) = before.rfind(&format!("{name}=\"")) {
            if let Some(version) = version_literal(&before[found + name.len() + 2..]) {
                return Some(version);
            }
        }
    }
    None
}

/// A quoted version: digits first, then letters, digits, `.`, `-` or `+`.
fn version_literal(text: &str) -> Option<String> {
    let version = &text[..text.find('"')?];
    (version.len() <= 40
        && version.contains('.')
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')))
    .then(|| version.to_string())
}

// Only the response body is kept; serde skips every other field of the row.
#[derive(Deserialize)]
struct DiskScriptRow {
    id: Uuid,
    response: Option<DiskScriptResponse>,
}

#[derive(Deserialize)]
struct DiskScriptResponse {
    #[serde(default)]
    body_preview: String,
    #[serde(default)]
    body_encoding: BodyEncoding,
}

/// One retained row's response body, or None when it cannot be shown to be
/// this record's UTF-8 script.
pub(crate) fn read_script(
    file: &mut File,
    locator: BodyLocator,
    record_id: Uuid,
) -> Option<String> {
    let len = locator.len as usize;
    if len == 0 || len > MAX_SCRIPT_ROW_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(locator.offset)).ok()?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).ok()?;
    let row: DiskScriptRow = serde_json::from_slice(&bytes).ok()?;
    let response = row.response?;
    (row.id == record_id && response.body_encoding == BodyEncoding::Utf8)
        .then_some(response.body_preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_version_reads_the_runtime_literal_or_its_variable() {
        // The App Router form, as nextjs.org ships it.
        assert_eq!(
            next_version(r#"n()}window.next={version:"16.5.0-canary.9",appDir:!0},("function""#),
            Some("16.5.0-canary.9".into())
        );
        // The Pages Router form: a variable assigned earlier in the bundle.
        assert_eq!(
            next_version(r#"let s="13.4.7",o=null;function f(){}window.next={version:s,router:r}"#),
            Some("13.4.7".into())
        );
        // Another library's version is not Next.js's.
        assert_eq!(
            next_version(r#"r.versions.push({version:"3.38.1",mode:"global"})"#),
            None
        );
        assert_eq!(
            next_version(r#"window.next={version:"<script>",appDir:!0}"#),
            None
        );
        assert_eq!(next_version(r#"window.next={version:x,router:r}"#), None);
    }
}
