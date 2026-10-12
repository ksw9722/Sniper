//! Literal declarations from bounded, already-retained response headers only.
//! Catalog categories describe products, not observed infrastructure roles.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::service_map::{MAX_EVIDENCE, MAX_HEADERS, MAX_HEADER_VALUE_BYTES, MAX_RECORDS};

pub const MAX_OBSERVATIONS_PER_ROW: usize = 8;
pub const MAX_OBSERVATIONS_PER_ORIGIN: usize = 32;
pub const MAX_OBSERVATIONS: usize = 2_000;
pub const MAX_VERSION_BYTES: usize = 32;
pub const MAX_TOKEN_CANDIDATES_PER_ROW: usize = 128;

/// Catalog revisions are independent of observation rule revisions. The default
/// stays frozen so new products cannot displace legacy observations at any cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogVersion {
    V1,
    V2,
}

impl CatalogVersion {
    pub const fn number(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }

    pub const fn from_number(number: u32) -> Option<Self> {
        match number {
            1 => Some(Self::V1),
            2 => Some(Self::V2),
            _ => None,
        }
    }
}

/// Shared across eligible response headers so short unknown tokens cannot
/// amplify projection work while the retained store's read lock is held.
#[derive(Default)]
pub(crate) struct TokenScanBudget {
    candidates: usize,
    pub(crate) omitted: bool,
}

impl TokenScanBudget {
    fn inspect_next(&mut self) -> bool {
        if self.candidates == MAX_TOKEN_CANDIDATES_PER_ROW {
            self.omitted = true;
            false
        } else {
            self.candidates += 1;
            true
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCategory {
    WebServer,
    ApplicationServer,
    ApplicationFramework,
    LanguageRuntime,
    Cms,
    EdgeService,
    Connector,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    Reported,
    NotReported,
    UnsupportedFormat,
    NonProductVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    fn is_valid(self) -> bool {
        self.start < self.end && self.end <= MAX_HEADER_VALUE_BYTES
    }
}

/// A missing nullable field is not equivalent to an explicit null in an
/// observation. This prevents clients accepting incomplete provenance objects.
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnologyObservation {
    pub product_id: String,
    pub name: String,
    pub catalog_category: CatalogCategory,
    #[serde(deserialize_with = "required_nullable")]
    pub reported_version: Option<String>,
    pub version_status: VersionStatus,
    pub source: String,
    pub strength: String,
    pub header_name: String,
    /// Zero-based index in the retained response's complete header array.
    pub header_index: usize,
    pub product_span: ByteSpan,
    #[serde(deserialize_with = "required_nullable")]
    pub version_span: Option<ByteSpan>,
    pub offset_basis: String,
    pub rule_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnologyAggregate {
    pub product_id: String,
    pub name: String,
    pub catalog_category: CatalogCategory,
    #[serde(deserialize_with = "required_nullable")]
    pub reported_version: Option<String>,
    pub version_status: VersionStatus,
    pub source: String,
    pub strength: String,
    pub header_name: String,
    /// Distinct retained rows, not occurrences or distinct transaction UUIDs.
    pub row_count: usize,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CatalogEntry {
    pub product_id: &'static str,
    pub name: &'static str,
    pub catalog_category: CatalogCategory,
    pub header_name: &'static str,
    pub literal: &'static str,
    pub rule_id: &'static str,
}

macro_rules! entry {
    ($id:literal, $name:literal, $category:ident, $header:literal, $literal:literal, $rule:literal) => {
        CatalogEntry {
            product_id: $id,
            name: $name,
            catalog_category: CatalogCategory::$category,
            header_name: $header,
            literal: $literal,
            rule_id: $rule,
        }
    };
}

/// Frozen revision-1 allowlist, shared with clients' validation and schema generation.
pub const CATALOG: &[CatalogEntry] = &[
    // The longer literal must precede Apache so it cannot become an HTTPD claim.
    entry!(
        "apache_tomcat",
        "Apache Tomcat",
        ApplicationServer,
        "server",
        "Apache Tomcat",
        "server.apache_tomcat.v1"
    ),
    entry!(
        "nginx",
        "nginx",
        WebServer,
        "server",
        "nginx",
        "server.nginx.v1"
    ),
    entry!(
        "apache_http_server",
        "Apache",
        WebServer,
        "server",
        "Apache",
        "server.apache_http_server.v1"
    ),
    entry!(
        "caddy",
        "Caddy",
        WebServer,
        "server",
        "Caddy",
        "server.caddy.v1"
    ),
    entry!(
        "cloudflare",
        "Cloudflare",
        EdgeService,
        "server",
        "Cloudflare",
        "server.cloudflare.v1"
    ),
    entry!(
        "microsoft_iis",
        "Microsoft IIS",
        WebServer,
        "server",
        "Microsoft-IIS",
        "server.microsoft_iis.v1"
    ),
    entry!(
        "envoy",
        "Envoy",
        EdgeService,
        "server",
        "Envoy",
        "server.envoy.v1"
    ),
    entry!(
        "php",
        "PHP",
        LanguageRuntime,
        "server",
        "PHP",
        "server.php.v1"
    ),
    entry!(
        "jetty",
        "Jetty",
        ApplicationServer,
        "server",
        "Jetty",
        "server.jetty.v1"
    ),
    entry!(
        "jetty",
        "Jetty",
        ApplicationServer,
        "server",
        "Jetty",
        "server.jetty_parenthesized.v1"
    ),
    entry!(
        "apache_coyote",
        "Apache-Coyote",
        Connector,
        "server",
        "Apache-Coyote",
        "server.apache_coyote.v1"
    ),
    entry!(
        "php",
        "PHP",
        LanguageRuntime,
        "x-powered-by",
        "PHP",
        "x_powered_by.php.v1"
    ),
    entry!(
        "express",
        "Express",
        ApplicationFramework,
        "x-powered-by",
        "Express",
        "x_powered_by.express.v1"
    ),
    entry!(
        "asp_net",
        "ASP.NET",
        ApplicationFramework,
        "x-powered-by",
        "ASP.NET",
        "x_powered_by.asp_net.v1"
    ),
    entry!(
        "wordpress",
        "WordPress",
        Cms,
        "x-generator",
        "WordPress",
        "x_generator.wordpress.v1"
    ),
    entry!(
        "drupal",
        "Drupal",
        Cms,
        "x-generator",
        "Drupal",
        "x_generator.drupal.v1"
    ),
];

/// Revision 2 adds only these independently declared product literals.
/// Categories describe products, never inferred deployed infrastructure roles.
pub const CATALOG_V2_ADDITIONS: &[CatalogEntry] = &[
    entry!(
        "gunicorn",
        "Gunicorn",
        ApplicationServer,
        "server",
        "gunicorn",
        "server.gunicorn.v1"
    ),
    entry!(
        "uvicorn",
        "Uvicorn",
        ApplicationServer,
        "server",
        "uvicorn",
        "server.uvicorn.v1"
    ),
    entry!(
        "waitress",
        "Waitress",
        ApplicationServer,
        "server",
        "waitress",
        "server.waitress.v1"
    ),
    entry!(
        "openresty",
        "OpenResty",
        WebServer,
        "server",
        "openresty",
        "server.openresty.v1"
    ),
    entry!(
        "litespeed",
        "LiteSpeed",
        WebServer,
        "server",
        "LiteSpeed",
        "server.litespeed.v1"
    ),
    entry!(
        "next_js",
        "Next.js",
        ApplicationFramework,
        "x-powered-by",
        "Next.js",
        "x_powered_by.next_js.v1"
    ),
];

pub fn catalog_entries(version: CatalogVersion) -> impl Iterator<Item = &'static CatalogEntry> {
    CATALOG.iter().chain(match version {
        CatalogVersion::V1 => [].iter(),
        CatalogVersion::V2 => CATALOG_V2_ADDITIONS.iter(),
    })
}

fn numeric_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VERSION_BYTES
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_version(product_id: &str, version: Option<&str>, status: VersionStatus) -> bool {
    match (status, version) {
        (VersionStatus::Reported, Some(value)) => {
            product_id != "apache_coyote" && numeric_version(value)
        }
        (VersionStatus::NotReported | VersionStatus::UnsupportedFormat, None) => true,
        (VersionStatus::NonProductVersion, None) => product_id == "apache_coyote",
        _ => false,
    }
}

impl TechnologyObservation {
    /// Validates the safe contract without needing (or exposing) raw headers.
    /// Only the original projection can establish the literal substring match.
    pub fn is_valid(&self) -> bool {
        self.is_valid_for_catalog(CatalogVersion::V1)
    }

    pub fn is_valid_for_catalog(&self, catalog: CatalogVersion) -> bool {
        let Some(entry) = catalog_entries(catalog).find(|entry| {
            entry.rule_id == self.rule_id
                && entry.product_id == self.product_id
                && entry.name == self.name
                && entry.catalog_category == self.catalog_category
                && entry.header_name == self.header_name
        }) else {
            return false;
        };
        self.source == "response_header"
            && self.strength == "literal_declaration"
            && self.offset_basis == "retained_header_value_utf8"
            && self.header_index < MAX_HEADERS
            && self.product_span.is_valid()
            && self.product_span.end - self.product_span.start == entry.literal.len()
            && valid_version(
                &self.product_id,
                self.reported_version.as_deref(),
                self.version_status,
            )
            && match (&self.reported_version, self.version_span) {
                (Some(version), Some(span)) => {
                    span.is_valid()
                        && span.start > self.product_span.end
                        && (self.header_name == "x-generator"
                            || span.start == self.product_span.end + 1)
                        && span.end - span.start == version.len()
                }
                (None, None) => true,
                _ => false,
            }
            && (self.rule_id != "server.jetty_parenthesized.v1"
                || matches!(
                    self.version_status,
                    VersionStatus::Reported | VersionStatus::UnsupportedFormat
                ))
    }
}

impl TechnologyAggregate {
    pub fn is_valid(&self) -> bool {
        self.is_valid_for_catalog(CatalogVersion::V1)
    }

    pub fn is_valid_for_catalog(&self, catalog: CatalogVersion) -> bool {
        catalog_entries(catalog).any(|entry| {
            entry.product_id == self.product_id
                && entry.name == self.name
                && entry.catalog_category == self.catalog_category
                && entry.header_name == self.header_name
        }) && self.source == "response_header"
            && self.strength == "literal_declaration"
            && valid_version(
                &self.product_id,
                self.reported_version.as_deref(),
                self.version_status,
            )
            && (1..=MAX_RECORDS).contains(&self.row_count)
            && self.first_seen <= self.last_seen
            && !self.evidence.is_empty()
            && self.evidence.len() <= MAX_EVIDENCE.min(self.row_count)
            && self
                .evidence
                .iter()
                .all(|id| crate::service_map::EvidenceKey::parse(id).is_some())
            && self
                .evidence
                .iter()
                .enumerate()
                .all(|(index, id)| !self.evidence[..index].contains(id))
    }
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// Returns the end of a nested/escaped HTTP comment. An unmatched comment
/// consumes the remainder rather than promoting its text into product tokens.
fn comment_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = index.saturating_add(1),
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    bytes.len()
}

fn token_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && !whitespace(bytes[index]) && bytes[index] != b'(' {
        index += 1;
    }
    index
}

/// Adds at most the row cap. Returns true if a literal declaration was omitted.
/// The caller applies the existing total-header/value/metadata budgets first.
#[cfg(test)]
pub(crate) fn observe_header(
    header_name: &str,
    value: &str,
    header_index: usize,
    observations: &mut Vec<TechnologyObservation>,
    budget: &mut TokenScanBudget,
) -> bool {
    observe_header_for_catalog(
        header_name,
        value,
        header_index,
        observations,
        budget,
        CatalogVersion::V1,
    )
}

pub(crate) fn observe_header_for_catalog(
    header_name: &str,
    value: &str,
    header_index: usize,
    observations: &mut Vec<TechnologyObservation>,
    budget: &mut TokenScanBudget,
    catalog: CatalogVersion,
) -> bool {
    if !matches!(header_name, "server" | "x-powered-by" | "x-generator")
        || value.len() > MAX_HEADER_VALUE_BYTES
        || header_index >= MAX_HEADERS
    {
        return false;
    }
    let bytes = value.as_bytes();
    let mut index = 0usize;
    let mut omitted = false;
    let mut malformed_suffix = false;
    while index < bytes.len() {
        if whitespace(bytes[index]) {
            index += 1;
            malformed_suffix = false;
            continue;
        }
        // Only an actual next candidate establishes omitted inspection work;
        // reaching exactly the budget at the end remains complete.
        if !budget.inspect_next() {
            break;
        }
        if malformed_suffix {
            index = token_end(bytes, index).max(index + 1);
            malformed_suffix = false;
            continue;
        }
        if bytes[index] == b'(' {
            if header_name != "server" {
                break;
            }
            index = comment_end(bytes, index);
            if bytes
                .get(index)
                .is_some_and(|byte| !whitespace(*byte) && *byte != b'(')
            {
                malformed_suffix = true;
            }
            continue;
        }
        if bytes[index] == b')' {
            // Malformed separators cannot introduce a fresh literal token.
            index = token_end(bytes, index + 1).max(index + 1);
            continue;
        }
        let entry = catalog_entries(catalog).find(|entry| {
            entry.header_name == header_name
                && entry.rule_id != "server.jetty_parenthesized.v1"
                && value
                    .get(index..index + entry.literal.len())
                    .is_some_and(|literal| literal.eq_ignore_ascii_case(entry.literal))
                && bytes
                    .get(index + entry.literal.len())
                    .is_none_or(|byte| whitespace(*byte) || matches!(byte, b'/' | b'('))
        });
        let Some(mut entry) = entry else {
            if header_name != "server" {
                break;
            }
            index = token_end(bytes, index).max(index + 1);
            continue;
        };
        let product_span = ByteSpan {
            start: index,
            end: index + entry.literal.len(),
        };
        let mut after = product_span.end;
        let mut candidate = None;
        let mut malformed_parentheses = false;
        if bytes.get(after) == Some(&b'/') {
            let start = after + 1;
            after = token_end(bytes, start);
            candidate = Some(ByteSpan { start, end: after });
        } else if entry.product_id == "jetty" && bytes.get(after) == Some(&b'(') {
            entry = catalog_entries(catalog)
                .find(|entry| entry.rule_id == "server.jetty_parenthesized.v1")
                .unwrap();
            let start = after + 1;
            after = comment_end(bytes, after);
            let closed = after > start && bytes.get(after - 1) == Some(&b')');
            malformed_parentheses = !closed
                || bytes
                    .get(after)
                    .is_some_and(|byte| !whitespace(*byte) && *byte != b'(');
            candidate = Some(ByteSpan {
                start,
                end: if closed { after - 1 } else { after },
            });
        } else if header_name == "x-generator" {
            while bytes.get(after).is_some_and(|byte| whitespace(*byte)) {
                after += 1;
            }
            if after < bytes.len() && bytes[after] != b'(' {
                let start = after;
                after = token_end(bytes, after);
                candidate = Some(ByteSpan { start, end: after });
            }
        }
        let (reported_version, version_span, version_status) = match candidate {
            None => (None, None, VersionStatus::NotReported),
            Some(span)
                if !malformed_parentheses && numeric_version(&value[span.start..span.end]) =>
            {
                if entry.product_id == "apache_coyote" {
                    (None, None, VersionStatus::NonProductVersion)
                } else {
                    (
                        Some(value[span.start..span.end].to_string()),
                        Some(span),
                        VersionStatus::Reported,
                    )
                }
            }
            Some(_) => (None, None, VersionStatus::UnsupportedFormat),
        };
        if observations.len() < MAX_OBSERVATIONS_PER_ROW {
            observations.push(TechnologyObservation {
                product_id: entry.product_id.into(),
                name: entry.name.into(),
                catalog_category: entry.catalog_category,
                reported_version,
                version_status,
                source: "response_header".into(),
                strength: "literal_declaration".into(),
                header_name: header_name.into(),
                header_index,
                product_span,
                version_span,
                offset_basis: "retained_header_value_utf8".into(),
                rule_id: entry.rule_id.into(),
            });
        } else {
            omitted = true;
        }
        if header_name != "server" {
            break;
        }
        index = after.max(product_span.end);
        if entry.rule_id == "server.jetty_parenthesized.v1"
            && bytes
                .get(index)
                .is_some_and(|byte| !whitespace(*byte) && *byte != b'(')
        {
            malformed_suffix = true;
        }
    }
    omitted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(header: &str, value: &str) -> Vec<TechnologyObservation> {
        let mut observations = vec![];
        let mut budget = TokenScanBudget::default();
        assert!(!observe_header(
            header,
            value,
            0,
            &mut observations,
            &mut budget
        ));
        assert!(!budget.omitted);
        assert!(observations.iter().all(TechnologyObservation::is_valid));
        observations
    }

    fn parse_catalog(
        header: &str,
        value: &str,
        catalog: CatalogVersion,
    ) -> Vec<TechnologyObservation> {
        let mut observations = vec![];
        let mut budget = TokenScanBudget::default();
        assert!(!observe_header_for_catalog(
            header,
            value,
            7,
            &mut observations,
            &mut budget,
            catalog
        ));
        assert!(!budget.omitted);
        assert!(observations
            .iter()
            .all(|observation| observation.is_valid_for_catalog(catalog)));
        observations
    }

    #[test]
    fn header_technology_catalog_v2_literals_are_opt_in_and_preserve_exact_provenance() {
        assert_eq!(CATALOG_V2_ADDITIONS.len(), 6);
        assert_eq!(catalog_entries(CatalogVersion::V1).count(), CATALOG.len());
        assert_eq!(
            catalog_entries(CatalogVersion::V2).count(),
            CATALOG.len() + 6
        );
        for entry in CATALOG_V2_ADDITIONS {
            for (suffix, expected_status) in [
                ("", VersionStatus::NotReported),
                ("/12.3.4", VersionStatus::Reported),
            ] {
                let prefix = if entry.header_name == "server" {
                    " \t(한글 😀) "
                } else {
                    " \t"
                };
                let literal = entry.literal.to_ascii_uppercase();
                let value = format!("{prefix}{literal}{suffix}");
                assert!(parse_catalog(entry.header_name, &value, CatalogVersion::V1).is_empty());
                let observations = parse_catalog(entry.header_name, &value, CatalogVersion::V2);
                assert_eq!(observations.len(), 1, "{}", entry.rule_id);
                let observation = &observations[0];
                assert_eq!(observation.product_id, entry.product_id);
                assert_eq!(observation.name, entry.name);
                assert_eq!(observation.catalog_category, entry.catalog_category);
                assert_eq!(observation.rule_id, entry.rule_id);
                assert_eq!(observation.header_index, 7);
                assert_eq!(
                    observation.product_span,
                    ByteSpan {
                        start: prefix.len(),
                        end: prefix.len() + entry.literal.len()
                    }
                );
                assert_eq!(observation.version_status, expected_status);
                assert!(!observation.is_valid());
                match observation.version_span {
                    Some(span) => assert_eq!(&value[span.start..span.end], "12.3.4"),
                    None => assert!(suffix.is_empty()),
                }
            }
            for suffix in [
                "/",
                "/v1.2",
                "/1.2-SENTINEL",
                "/1..2",
                "/1.2;SENTINEL",
                "/123456789012345678901234567890123",
            ] {
                let observations = parse_catalog(
                    entry.header_name,
                    &format!("{}{suffix}", entry.literal),
                    CatalogVersion::V2,
                );
                assert_eq!(
                    observations[0].version_status,
                    VersionStatus::UnsupportedFormat
                );
                assert!(observations[0].reported_version.is_none());
                assert!(observations[0].version_span.is_none());
                assert!(!serde_json::to_string(&observations)
                    .unwrap()
                    .contains("SENTINEL"));
            }
            for value in [
                format!("not{}/1", entry.literal),
                format!("{}extra/1", entry.literal),
                format!("({}/1)", entry.literal),
                format!("(ignored){}/1", entry.literal),
            ] {
                assert!(
                    parse_catalog(entry.header_name, &value, CatalogVersion::V2).is_empty(),
                    "{value}"
                );
            }
            for header in [
                "authorization",
                "set-cookie",
                "server-timing",
                "x-generator",
            ] {
                assert!(parse_catalog(header, entry.literal, CatalogVersion::V2).is_empty());
            }
        }
        assert!(parse_catalog("server", "Next.js/1", CatalogVersion::V2).is_empty());
        assert!(parse_catalog("x-powered-by", "unknown Next.js/1", CatalogVersion::V2).is_empty());
        assert!(parse_catalog("x-powered-by", "gunicorn/1", CatalogVersion::V2).is_empty());
        let all = parse_catalog(
            "server",
            "gunicorn uvicorn waitress openresty LiteSpeed",
            CatalogVersion::V2,
        );
        assert_eq!(
            all.iter()
                .map(|o| o.product_id.as_str())
                .collect::<Vec<_>>(),
            vec!["gunicorn", "uvicorn", "waitress", "openresty", "litespeed"]
        );
        assert_eq!(all[3].catalog_category, CatalogCategory::WebServer);
        assert_eq!(
            parse_catalog(
                "server",
                "openresty/12345678901234567890123456789012",
                CatalogVersion::V2
            )[0]
            .version_status,
            VersionStatus::Reported
        );
    }

    #[test]
    fn header_technology_catalog_v2_uses_the_same_shared_token_and_header_bounds() {
        for catalog in [CatalogVersion::V1, CatalogVersion::V2] {
            for count in [
                MAX_TOKEN_CANDIDATES_PER_ROW - 1,
                MAX_TOKEN_CANDIDATES_PER_ROW,
            ] {
                let mut observations = vec![];
                let mut budget = TokenScanBudget::default();
                observe_header_for_catalog(
                    "server",
                    &"x ".repeat(count),
                    0,
                    &mut observations,
                    &mut budget,
                    catalog,
                );
                observe_header_for_catalog(
                    "x-powered-by",
                    "Next.js/1.2",
                    1,
                    &mut observations,
                    &mut budget,
                    catalog,
                );
                assert_eq!(budget.omitted, count == MAX_TOKEN_CANDIDATES_PER_ROW);
                assert_eq!(
                    observations.len(),
                    usize::from(
                        catalog == CatalogVersion::V2 && count < MAX_TOKEN_CANDIDATES_PER_ROW
                    )
                );
            }
            let mut observations = vec![];
            let mut budget = TokenScanBudget::default();
            observe_header_for_catalog(
                "server",
                "gunicorn/1",
                MAX_HEADERS,
                &mut observations,
                &mut budget,
                catalog,
            );
            observe_header_for_catalog(
                "server",
                &format!("gunicorn/{}", "1".repeat(MAX_HEADER_VALUE_BYTES)),
                0,
                &mut observations,
                &mut budget,
                catalog,
            );
            assert!(observations.is_empty());
            assert_eq!(budget.candidates, 0);
        }
    }

    #[test]
    fn header_technology_preserves_catalog_products_and_explicit_versions() {
        for entry in CATALOG {
            if entry.rule_id == "server.jetty_parenthesized.v1" {
                continue;
            }
            let value = format!("{}/12.3.4", entry.literal);
            let observations = parse(entry.header_name, &value);
            assert_eq!(observations.len(), 1, "{}", entry.rule_id);
            let observation = &observations[0];
            assert_eq!(observation.product_id, entry.product_id);
            assert_eq!(observation.rule_id, entry.rule_id);
            assert_eq!(observation.catalog_category, entry.catalog_category);
            assert_eq!(
                observation.product_span,
                ByteSpan {
                    start: 0,
                    end: entry.literal.len()
                }
            );
            if entry.product_id == "apache_coyote" {
                assert_eq!(observation.version_status, VersionStatus::NonProductVersion);
                assert!(observation.reported_version.is_none());
                assert!(observation.version_span.is_none());
            } else {
                assert_eq!(observation.reported_version.as_deref(), Some("12.3.4"));
            }
        }
        let wordpress = parse("x-generator", "WordPress 6.7.1 (https://example.com)");
        assert_eq!(wordpress[0].reported_version.as_deref(), Some("6.7.1"));
        let drupal = parse("x-generator", "Drupal\t 10");
        assert_eq!(drupal[0].reported_version.as_deref(), Some("10"));
    }

    #[test]
    fn header_technology_jetty_and_coyote_are_distinct_literal_declarations() {
        let observations = parse(
            "server",
            "Jetty(12.0.1) Apache-Coyote/1.1 Apache Tomcat/10.1.30",
        );
        assert_eq!(observations.len(), 3);
        assert_eq!(observations[0].rule_id, "server.jetty_parenthesized.v1");
        assert_eq!(observations[0].reported_version.as_deref(), Some("12.0.1"));
        assert_eq!(observations[1].product_id, "apache_coyote");
        assert_eq!(observations[1].catalog_category, CatalogCategory::Connector);
        assert_eq!(
            observations[1].version_status,
            VersionStatus::NonProductVersion
        );
        assert_eq!(observations[2].product_id, "apache_tomcat");
        assert_eq!(observations[2].reported_version.as_deref(), Some("10.1.30"));
        let bare = parse("server", "Apache-Coyote");
        assert_eq!(bare[0].version_status, VersionStatus::NotReported);
        assert!(parse("server", "Tomcat/10.1.30").is_empty());
    }

    #[test]
    fn header_technology_complete_numeric_versions_never_expose_suffixes() {
        for suffix in [
            "SECRET_VERSION_SENTINEL",
            "1.2-SENTINEL",
            "1.2.v20240101",
            "v1.2",
            "1..2",
            ".1",
            "1.",
            "",
            "1.2/SENTINEL",
            "1.2)SENTINEL",
            "1.2;SENTINEL",
            "1.2,SECRET_SENTINEL",
            "1.2é",
            "123456789012345678901234567890123",
        ] {
            let observations = parse("server", &format!("nginx/{suffix}"));
            assert_eq!(
                observations[0].version_status,
                VersionStatus::UnsupportedFormat,
                "{suffix}"
            );
            assert!(observations[0].reported_version.is_none());
            assert!(observations[0].version_span.is_none());
            assert!(!serde_json::to_string(&observations)
                .unwrap()
                .contains("SENTINEL"));
        }
        for value in [
            "Jetty(9.4.51.v20230217)",
            "Jetty(12.0.1",
            "Jetty(12.0.1)SENTINEL",
            "Jetty()",
            "Jetty((12.0.1))",
        ] {
            let observations = parse("server", value);
            assert_eq!(
                observations[0].version_status,
                VersionStatus::UnsupportedFormat,
                "{value}"
            );
            assert!(observations[0].version_span.is_none());
        }
        assert_eq!(
            parse("server", "nginx/12345678901234567890123456789012")[0].version_status,
            VersionStatus::Reported
        );
    }

    #[test]
    fn header_technology_comments_multi_products_and_boundaries_are_conservative() {
        let observations = parse(
            "server",
            r"Apache/2.4.62 (Unix (nginx/9) Caddy/8 \) Envoy/7) PHP/8.3.1",
        );
        assert_eq!(
            observations
                .iter()
                .map(|o| o.product_id.as_str())
                .collect::<Vec<_>>(),
            vec!["apache_http_server", "php"]
        );
        assert_eq!(observations[1].reported_version.as_deref(), Some("8.3.1"));
        for value in [
            "notnginx/1.2",
            "nginxplus/1.2",
            "Apache-Coyotex/1.1",
            "Microsoft-IISx/10",
            "(nginx/1.2)",
            "(unfinished nginx/1.2",
            "(ignored)PHP/8.3",
            "example.com/nginx/1.2",
            "?nginx/1.2",
            "nginx=1.2",
            "PHPx/8",
            "éPHP/8",
        ] {
            assert!(parse("server", value).is_empty(), "{value}");
        }
        for header in [
            "authorization",
            "set-cookie",
            "x-server",
            "server-timing",
            "x-aspnet-version",
        ] {
            assert!(parse(header, "nginx/1.2 PHP/8").is_empty());
        }
        assert!(parse("x-powered-by", "unknown PHP/8").is_empty());
        assert!(parse("x-powered-by", "(comment) PHP/8").is_empty());
        assert!(parse("x-generator", "PHP/8").is_empty());
        let concatenated = parse("server", "Jetty(12.0.1)nginx/1.2");
        assert_eq!(concatenated.len(), 1);
        assert_eq!(concatenated[0].product_id, "jetty");
        assert_eq!(
            concatenated[0].version_status,
            VersionStatus::UnsupportedFormat
        );
        let commented = parse("server", "nginx/1.2 (ignored)PHP/8.3 Caddy/2");
        assert_eq!(
            commented
                .iter()
                .map(|o| o.product_id.as_str())
                .collect::<Vec<_>>(),
            vec!["nginx", "caddy"]
        );
    }

    #[test]
    fn header_technology_byte_spans_reference_original_unicode_and_whitespace() {
        let value = " \t(한글 😀 nginx/99) \tNginx/1.24.0 (日本語) nginx/1.24.0";
        let mut observations = vec![];
        assert!(!observe_header(
            "server",
            value,
            7,
            &mut observations,
            &mut TokenScanBudget::default()
        ));
        assert_eq!(observations.len(), 2);
        for observation in &observations {
            assert!(observation.is_valid());
            assert_eq!(observation.header_index, 7);
            let product = &value[observation.product_span.start..observation.product_span.end];
            assert!(product.eq_ignore_ascii_case("nginx"));
            let span = observation.version_span.unwrap();
            assert_eq!(&value[span.start..span.end], "1.24.0");
        }
        assert_eq!(
            observations[0].product_span.start,
            value.find("Nginx").unwrap()
        );
        assert_eq!(
            observations[1].product_span.start,
            value.rfind("nginx").unwrap()
        );
    }

    #[test]
    fn header_technology_row_cap_and_header_bounds_are_explicit() {
        let mut observations = vec![];
        let value = "nginx/1 ".repeat(MAX_OBSERVATIONS_PER_ROW + 1);
        assert!(observe_header(
            "server",
            &value,
            0,
            &mut observations,
            &mut TokenScanBudget::default()
        ));
        assert_eq!(observations.len(), MAX_OBSERVATIONS_PER_ROW);
        assert!(observe_header(
            "server",
            "PHP/8",
            1,
            &mut observations,
            &mut TokenScanBudget::default()
        ));
        let mut none = vec![];
        assert!(!observe_header(
            "server",
            "PHP/8",
            MAX_HEADERS,
            &mut none,
            &mut TokenScanBudget::default()
        ));
        assert!(!observe_header(
            "server",
            &"n".repeat(MAX_HEADER_VALUE_BYTES + 1),
            0,
            &mut none,
            &mut TokenScanBudget::default()
        ));
        assert!(none.is_empty());
    }

    #[test]
    fn header_technology_validation_rejects_unsafe_contract_shapes() {
        let observation = parse("server", "nginx/1.2").remove(0);
        let original = serde_json::to_value(&observation).unwrap();
        for field in ["reported_version", "version_span"] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<TechnologyObservation>(value).is_err());
        }
        let mut value = original.clone();
        value["raw_header"] = "SECRET_SENTINEL".into();
        assert!(serde_json::from_value::<TechnologyObservation>(value).is_err());
        for (field, replacement) in [
            ("reported_version", serde_json::json!("1.2-SENTINEL")),
            ("reported_version", serde_json::Value::Null),
            ("name", serde_json::json!("nginx SENTINEL")),
            ("catalog_category", serde_json::json!("unknown")),
            ("header_index", serde_json::json!(128)),
            ("source", serde_json::json!("response_body")),
            ("rule_id", serde_json::json!("arbitrary")),
            ("version_status", serde_json::json!("non_product_version")),
            ("product_span", serde_json::json!({"start":0,"end":6})),
            ("version_span", serde_json::json!({"start":7,"end":10})),
            ("version_span", serde_json::json!({"start":4094,"end":4097})),
        ] {
            let mut value = original.clone();
            value[field] = replacement;
            let decoded: TechnologyObservation = serde_json::from_value(value).unwrap();
            assert!(!decoded.is_valid(), "{field}");
        }
    }

    #[test]
    fn header_technology_candidate_boundaries_only_omit_actual_uninspected_work() {
        for count in [127, 128, 129] {
            let mut budget = TokenScanBudget::default();
            let mut observations = vec![];
            assert!(!observe_header(
                "server",
                &"x ".repeat(count),
                0,
                &mut observations,
                &mut budget
            ));
            assert!(observations.is_empty());
            assert_eq!(budget.candidates, count.min(MAX_TOKEN_CANDIDATES_PER_ROW));
            assert_eq!(budget.omitted, count > MAX_TOKEN_CANDIDATES_PER_ROW);
        }
        for (prefix_count, expected_count, expected_partial) in [(127, 1, false), (128, 0, true)] {
            let mut budget = TokenScanBudget::default();
            let mut observations = vec![];
            let value = format!("{}nginx/1.2-SENTINEL", "x ".repeat(prefix_count));
            assert!(!observe_header(
                "server",
                &value,
                0,
                &mut observations,
                &mut budget
            ));
            assert_eq!(observations.len(), expected_count);
            assert_eq!(budget.omitted, expected_partial);
            assert!(observations
                .iter()
                .all(
                    |observation| observation.version_status == VersionStatus::UnsupportedFormat
                        && observation.reported_version.is_none()
                        && observation.version_span.is_none()
                ));
            assert!(!serde_json::to_string(&observations)
                .unwrap()
                .contains("SENTINEL"));
        }
    }

    #[test]
    fn header_technology_candidate_budget_is_shared_across_headers() {
        let mut budget = TokenScanBudget::default();
        let mut observations = vec![];
        observe_header(
            "server",
            &"x ".repeat(127),
            0,
            &mut observations,
            &mut budget,
        );
        observe_header(
            "set-cookie",
            "PRIVATE_SENTINEL",
            1,
            &mut observations,
            &mut budget,
        );
        assert_eq!(budget.candidates, 127);
        observe_header(
            "x-powered-by",
            "PHP/8.3.1",
            2,
            &mut observations,
            &mut budget,
        );
        assert_eq!(budget.candidates, 128);
        assert!(!budget.omitted);
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].header_index, 2);
        assert_eq!(observations[0].reported_version.as_deref(), Some("8.3.1"));
        observe_header("server", " \t ", 3, &mut observations, &mut budget);
        assert!(!budget.omitted);
        observe_header(
            "x-generator",
            "WordPress 6.7",
            4,
            &mut observations,
            &mut budget,
        );
        assert!(budget.omitted);
        assert_eq!(budget.candidates, 128);
        assert_eq!(observations.len(), 1);
    }

    #[test]
    fn header_technology_candidate_budget_counts_comments_and_malformed_tokens() {
        for prefix in ["(x (nested)) ", ")malformed "] {
            for (count, expected_count, expected_partial) in [(127, 1, false), (128, 0, true)] {
                let mut budget = TokenScanBudget::default();
                let mut observations = vec![];
                let value = format!("{}Jetty(12.0.1)", prefix.repeat(count));
                observe_header("server", &value, 0, &mut observations, &mut budget);
                assert_eq!(observations.len(), expected_count);
                assert_eq!(budget.omitted, expected_partial);
                assert!(observations
                    .iter()
                    .all(|observation| observation.reported_version.as_deref() == Some("12.0.1")));
            }
        }
        for (suffix, expected_count, expected_status) in [
            ("(x)PHP/8", 0, None),
            (
                "Jetty(12.0.1)nginx/1.2",
                1,
                Some(VersionStatus::UnsupportedFormat),
            ),
        ] {
            let mut budget = TokenScanBudget::default();
            let mut observations = vec![];
            let value = format!("{}{suffix}", "x ".repeat(127));
            observe_header("server", &value, 0, &mut observations, &mut budget);
            assert!(budget.omitted);
            assert_eq!(budget.candidates, 128);
            assert_eq!(observations.len(), expected_count);
            assert_eq!(
                observations
                    .first()
                    .map(|observation| observation.version_status),
                expected_status
            );
            assert!(observations
                .iter()
                .all(|observation| observation.reported_version.is_none()));
        }
        let mut budget = TokenScanBudget::default();
        let mut observations = vec![];
        let value = format!("{}(x)PHP/8", "x ".repeat(126));
        observe_header("server", &value, 0, &mut observations, &mut budget);
        assert_eq!(budget.candidates, 128);
        assert!(!budget.omitted);
        assert!(observations.is_empty());
    }
}
