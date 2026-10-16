//! Library and framework versions retained responses declare about themselves.
//!
//! Response headers name few products and rarely a version: `X-Powered-By:
//! Next.js` carries none. Scripts and pages do, in the places each library
//! writes them: a license banner (`/*! jQuery v3.7.1`), a runtime literal
//! (`window.next={version:"14.2.3"}`), React DOM's renderer registration
//! (`version:"18.2.0",rendererPackageName:"react-dom"`), Angular's
//! `ng-version="17.1.0"`, or a page's `<meta name="generator">`. The map reads
//! JavaScript and HTML rows for these, once per row revision, as Retire.js
//! reads files for their banners. Only these anchors count: a bundle holds
//! other libraries' `version:"…"` literals too.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::LazyLock,
};

use regex::{Regex, RegexSet};
use serde::Deserialize;
use uuid::Uuid;

use crate::{model::BodyEncoding, service_map::ServiceMapRow, session::BodyLocator};

/// Retained rows read from disk per map; the rest are read on later maps.
pub const MAX_READS: usize = 256;
/// Bytes copied or read per map, resident and retained together.
pub const MAX_BYTES_PER_MAP: usize = 32 * 1024 * 1024;
/// A serialized row larger than this is not read at all.
pub const MAX_ROW_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub product_id: String,
    pub name: String,
    pub version: String,
}

/// What a row's body can declare: a script's libraries, or a page's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Script,
    Page,
}

pub(crate) fn kind(row: &ServiceMapRow) -> Option<Kind> {
    let mime = row.content_type.as_deref()?;
    if mime.contains("javascript") || mime.contains("ecmascript") {
        Some(Kind::Script)
    } else if mime.contains("html") {
        Some(Kind::Page)
    } else {
        None
    }
}

// `§` is the version: a digit, then letters, digits, `.`, `-` or `+`.
const SCRIPT_SIGNATURES: &[(&str, &str, &str)] = &[
    (
        "react",
        "React",
        r#"version:"§",rendererPackageName:"react-dom""#,
    ),
    ("react", "React", r"@license React v§"),
    (
        "vue",
        "Vue",
        r"@vue/(?:runtime-dom|runtime-core|shared|reactivity|compiler-core|compiler-dom) v§",
    ),
    ("vue", "Vue", r"Vue\.js v§"),
    ("nuxt", "Nuxt", r#"get nuxt\(\)\{return"§""#),
    ("angular", "Angular", r"@license Angular v§"),
    ("angularjs", "AngularJS", r"@license AngularJS v§"),
    (
        "angularjs",
        "AngularJS",
        r#"\{full:"§",major:[0-9]+,minor:[0-9]+,dot:[0-9]+"#,
    ),
    ("jquery", "jQuery", r"jQuery (?:JavaScript Library )?v§"),
    ("jquery_ui", "jQuery UI", r"jQuery UI - v§"),
    (
        "bootstrap",
        "Bootstrap",
        r"Bootstrap v§ \(https?://getbootstrap\.com",
    ),
    (
        "core_js",
        "core-js",
        r#"version:"§",mode:"(?:global|pure)""#,
    ),
    ("moment", "Moment.js", r"//! moment\.js\s*//! version : §"),
    ("underscore", "Underscore.js", r"Underscore\.js §"),
    ("dompurify", "DOMPurify", r"DOMPurify § \|"),
    ("axios", "Axios", r"Axios v§ Copyright"),
    ("socket_io", "Socket.IO", r"Socket\.IO v§"),
    ("chart_js", "Chart.js", r"Chart\.js v§"),
    ("swiper", "Swiper", r"\* Swiper §"),
    ("gsap", "GSAP", r"\* GSAP §"),
    ("video_js", "Video.js", r"Video\.js § <"),
    ("backbone", "Backbone.js", r"Backbone\.js §"),
    ("knockout", "Knockout", r"Knockout JavaScript library v§"),
    ("select2", "Select2", r"Select2 § \|"),
    (
        "sentry",
        "Sentry",
        r"@sentry/(?:browser|react|vue|angular|nextjs) §",
    ),
    ("pdf_js", "PDF.js", r#"pdfjsVersion\s*=\s*["']§"#),
];
const PAGE_SIGNATURES: &[(&str, &str, &str)] = &[("angular", "Angular", r#"\bng-version="§""#)];
const VERSION: &str = r"([0-9][0-9A-Za-z.+-]{0,39})";

struct Signatures {
    set: RegexSet,
    each: Vec<Regex>,
    products: &'static [(&'static str, &'static str, &'static str)],
}

fn compile(products: &'static [(&'static str, &'static str, &'static str)]) -> Signatures {
    let patterns: Vec<String> = products
        .iter()
        .map(|(_, _, pattern)| pattern.replace('§', VERSION))
        .collect();
    Signatures {
        set: RegexSet::new(&patterns).expect("valid library signatures"),
        each: patterns
            .iter()
            .map(|pattern| Regex::new(pattern).expect("valid library signature"))
            .collect(),
        products,
    }
}

static SCRIPTS: LazyLock<Signatures> = LazyLock::new(|| compile(SCRIPT_SIGNATURES));
static PAGES: LazyLock<Signatures> = LazyLock::new(|| compile(PAGE_SIGNATURES));
static GENERATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta\b[^>]*\bname\s*=\s*["']generator["'][^>]*>"#)
        .expect("valid generator pattern")
});
static CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bcontent\s*=\s*["']([^"']{1,120})["']"#).expect("valid content pattern")
});

/// Every library this body declares, one version per product: the first found.
pub(crate) fn detect(body: &str, kind: Kind) -> Vec<Detection> {
    let mut found: Vec<Detection> = Vec::new();
    let mut add = |detection: Detection| {
        if !found
            .iter()
            .any(|item| item.product_id == detection.product_id)
        {
            found.push(detection);
        }
    };
    if kind == Kind::Script {
        if let Some(version) = next_version(body) {
            add(Detection {
                product_id: "next_js".into(),
                name: "Next.js".into(),
                version,
            });
        }
    }
    let signatures = if kind == Kind::Script {
        &SCRIPTS
    } else {
        &PAGES
    };
    for index in signatures.set.matches(body).iter() {
        let (product_id, name, _) = signatures.products[index];
        if let Some(version) = signatures.each[index]
            .captures(body)
            .and_then(|captures| captures.get(1))
            .map(|version| version.as_str().to_string())
            .filter(|version| valid_version(version))
        {
            add(Detection {
                product_id: product_id.into(),
                name: name.into(),
                version,
            });
        }
    }
    if kind == Kind::Page {
        for tag in GENERATOR.find_iter(body).take(4) {
            if let Some(detection) = CONTENT
                .captures(tag.as_str())
                .and_then(|captures| generator(&captures[1]))
            {
                add(detection);
            }
        }
    }
    found
}

/// A generator's name and version: "WordPress 6.5.2", "Astro v4.0.3",
/// "Drupal 10 (https://www.drupal.org)". A name with no version is not kept.
fn generator(content: &str) -> Option<Detection> {
    let mut words = Vec::new();
    for word in content.split_whitespace().take(6) {
        let candidate = word
            .strip_prefix(['v', 'V'])
            .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(word);
        if candidate.starts_with(|c: char| c.is_ascii_digit()) {
            let version: String = candidate
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
                .collect();
            let name = words.join(" ");
            let product_id: String = name
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
                .trim_matches('_')
                .to_string();
            let name_ok = name.len() <= 40
                && name.starts_with(|c: char| c.is_ascii_alphabetic())
                && name.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '!' | '+' | '_' | '-')
                });
            return (name_ok && !product_id.is_empty() && valid_version(&version)).then(|| {
                Detection {
                    product_id,
                    name,
                    version,
                }
            });
        }
        words.push(word);
    }
    None
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 40
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

/// The version Next.js's client runtime names in this script, if it does.
fn next_version(script: &str) -> Option<String> {
    const ANCHOR: &str = "window.next={version:";
    for (at, _) in script.match_indices(ANCHOR) {
        let rest = &script[at + ANCHOR.len()..];
        // The App Router writes the literal. The Pages Router names a variable
        // assigned earlier in the bundle. ponytail: the nearest `name="…"`
        // before the anchor is taken as its value; a parser would be exact.
        if let Some(literal) = rest.strip_prefix('"') {
            if let Some(version) = literal.split('"').next().filter(|v| valid_version(v)) {
                return Some(version.to_string());
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
            let literal = &before[found + name.len() + 2..];
            if let Some(version) = literal.split('"').next().filter(|v| valid_version(v)) {
                return Some(version.to_string());
            }
        }
    }
    None
}

// Only the response body is kept; serde skips every other field of the row.
#[derive(Deserialize)]
struct DiskRow {
    id: Uuid,
    response: Option<DiskResponse>,
}

#[derive(Deserialize)]
struct DiskResponse {
    #[serde(default)]
    body_preview: String,
    #[serde(default)]
    body_encoding: BodyEncoding,
}

/// One retained row's response body, or None when it cannot be shown to be
/// this record's UTF-8 body.
pub(crate) fn read_body(file: &mut File, locator: BodyLocator, record_id: Uuid) -> Option<String> {
    let len = locator.len as usize;
    if len == 0 || len > MAX_ROW_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(locator.offset)).ok()?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).ok()?;
    let row: DiskRow = serde_json::from_slice(&bytes).ok()?;
    let response = row.response?;
    (row.id == record_id && response.body_encoding == BodyEncoding::Utf8)
        .then_some(response.body_preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(body: &str, kind: Kind) -> Vec<(String, String)> {
        detect(body, kind)
            .into_iter()
            .map(|item| (item.product_id, item.version))
            .collect()
    }

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
        assert_eq!(
            next_version(r#"window.next={version:"<script>",appDir:!0}"#),
            None
        );
        assert_eq!(next_version(r#"window.next={version:x,router:r}"#), None);
    }

    #[test]
    fn scripts_name_their_libraries_by_banner_or_runtime_anchor() {
        let bundle = concat!(
            r#"/*! jQuery v3.7.1 | (c) OpenJS Foundation and other contributors */"#,
            r#"{bundleType:0,version:"18.2.0",rendererPackageName:"react-dom"}"#,
            "/**\n* @vue/shared v3.4.21\n* (c) 2018-present Yuxi (Evan) You\n**/",
            r#"(r.versions||(r.versions=[])).push({version:"3.38.1",mode:"global",copyright:"© 2014-2024 Denis Pushkarev"})"#,
            r#"/*! @license DOMPurify 3.0.6 | (c) Cure53 and other contributors */"#,
            r#"/*! Axios v1.6.7 Copyright (c) 2024 Matt Zabriskie and contributors */"#,
            r#"window.next={version:"14.2.3",appDir:!0}"#,
            r#"other.version:"9.9.9""#,
        );
        assert_eq!(
            found(bundle, Kind::Script),
            [
                ("next_js".into(), "14.2.3".into()),
                ("react".into(), "18.2.0".into()),
                ("vue".into(), "3.4.21".into()),
                ("jquery".into(), "3.7.1".into()),
                ("core_js".into(), "3.38.1".into()),
                ("dompurify".into(), "3.0.6".into()),
                ("axios".into(), "1.6.7".into()),
            ]
        );
        assert!(found("console.log(1)", Kind::Script).is_empty());
        // Page markers are not read from scripts.
        assert!(found(r#"<app ng-version="17.1.0">"#, Kind::Script).is_empty());
    }

    #[test]
    fn pages_name_their_generator_and_angular_version() {
        let page = concat!(
            r#"<meta name="generator" content="WordPress 6.5.2">"#,
            r#"<meta content="Astro v4.0.3" name="generator">"#,
            r#"<meta name="generator" content="Joomla! - Open Source Content Management">"#,
            r#"<app-root ng-version="17.1.0"></app-root>"#,
        );
        assert_eq!(
            found(page, Kind::Page),
            [
                ("angular".into(), "17.1.0".into()),
                ("wordpress".into(), "6.5.2".into()),
                ("astro".into(), "4.0.3".into()),
            ]
        );
        assert_eq!(
            generator("Drupal 10 (https://www.drupal.org)").map(|item| (item.name, item.version)),
            Some(("Drupal".into(), "10".into()))
        );
    }
}
