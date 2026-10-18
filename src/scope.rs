//! Host scope matching.
//!
//! Scope decides what the proxy holds for intercept, what the history list shows
//! under "In scope", and which hosts the site map marks. That decision used to be
//! implemented three times — in `runtime`, `store` and `websocket` — with the
//! copies free to drift. It lives here once.
//!
//! A host is in scope when it matches an include pattern (or there are none, which
//! means everything) and matches no exclude pattern. Excludes therefore work on
//! their own: with no includes, everything except the excluded hosts is in scope.

/// Pattern forms, in the order they are tried:
///
/// - `*.example.com` — the domain and anything under it, so `example.com` and
///   `api.example.com` both match. This is the common case and predates the rest.
/// - `log-*.example.com` — `*` stands for any run of characters. Useful for
///   excluding a family of hosts that share a prefix.
/// - `api.example.com` — exactly that host.
///
/// Matching ignores case, a port, and a trailing DNS root dot.
pub fn host_matches_pattern(host: &str, pattern: &str) -> bool {
    PreparedPattern::new(pattern).matches(&normalize_host_for_matching(host))
}

#[derive(Clone, Copy)]
enum PatternKind {
    Exact,
    DomainSuffix,
    Glob,
}

struct PreparedPattern {
    normalized: String,
    kind: PatternKind,
}

impl PreparedPattern {
    fn new(pattern: &str) -> Self {
        let normalized = normalize_host_for_matching(pattern);
        let kind = if normalized
            .strip_prefix("*.")
            .is_some_and(|suffix| !suffix.contains('*'))
        {
            PatternKind::DomainSuffix
        } else if normalized.contains('*') {
            PatternKind::Glob
        } else {
            PatternKind::Exact
        };
        Self { normalized, kind }
    }

    fn matches(&self, hostname: &str) -> bool {
        match self.kind {
            PatternKind::Exact => hostname == self.normalized,
            PatternKind::DomainSuffix => {
                let suffix = &self.normalized[2..];
                hostname == suffix
                    || hostname
                        .strip_suffix(suffix)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
            PatternKind::Glob => glob_matches(hostname, &self.normalized),
        }
    }
}

/// A bounded caller-owned scope snapshot for repeated metadata comparisons.
/// Prepare before acquiring a store lock. The same matcher backs one-off calls;
/// this only reuses normalization and avoids per-pattern work allocations.
pub(crate) struct PreparedScope {
    include: PreparedPatterns,
    exclude: PreparedPatterns,
}

struct PreparedPatterns {
    exact: std::collections::BTreeSet<String>,
    patterned: Vec<PreparedPattern>,
}

impl PreparedPatterns {
    fn new(patterns: &[String]) -> Self {
        let mut prepared = Self {
            exact: Default::default(),
            patterned: Vec::new(),
        };
        for pattern in patterns {
            let pattern = PreparedPattern::new(pattern);
            if matches!(pattern.kind, PatternKind::Exact) {
                prepared.exact.insert(pattern.normalized);
            } else {
                prepared.patterned.push(pattern);
            }
        }
        prepared
    }
    fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.patterned.is_empty()
    }
    fn contains_bounded(
        &self,
        hostname: &str,
        remaining: &mut usize,
    ) -> Result<bool, ScopeWorkLimit> {
        if self.exact.contains(hostname) {
            return Ok(true);
        }
        for pattern in &self.patterned {
            // Charge before suffix/glob work. These byte-based logical units
            // bound repeated middle searches without a timing-dependent cutoff.
            let charge = hostname
                .len()
                .saturating_add(pattern.normalized.len())
                .saturating_add(16);
            *remaining = remaining.checked_sub(charge).ok_or(ScopeWorkLimit)?;
            if pattern.matches(hostname) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ScopeWorkLimit;

impl PreparedScope {
    pub(crate) fn within_limits(
        include: &[String],
        exclude: &[String],
        max_entries: usize,
        max_bytes: usize,
    ) -> bool {
        include.len() <= max_entries
            && exclude.len() <= max_entries
            && include
                .iter()
                .chain(exclude)
                .all(|pattern| pattern.len() <= max_bytes)
    }

    pub(crate) fn try_new(
        include: &[String],
        exclude: &[String],
        max_entries: usize,
        max_bytes: usize,
    ) -> Option<Self> {
        Self::within_limits(include, exclude, max_entries, max_bytes)
            .then(|| Self::new(include, exclude))
    }
    pub(crate) fn new(include: &[String], exclude: &[String]) -> Self {
        Self {
            include: PreparedPatterns::new(include),
            exclude: PreparedPatterns::new(exclude),
        }
    }

    pub(crate) fn contains_bounded(
        &self,
        host: &str,
        remaining: &mut usize,
    ) -> Result<bool, ScopeWorkLimit> {
        if self.include.is_empty() && self.exclude.is_empty() {
            return Ok(true);
        }
        let hostname = normalize_host_for_matching(host);
        let included =
            self.include.is_empty() || self.include.contains_bounded(&hostname, remaining)?;
        Ok(included && !self.exclude.contains_bounded(&hostname, remaining)?)
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, host: &str) -> bool {
        let mut remaining = usize::MAX;
        self.contains_bounded(host, &mut remaining)
            .expect("unbounded synthetic scope comparison")
    }
}

pub fn host_matches_any(host: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| host_matches_pattern(host, pattern))
}

/// Whether `host` is in scope given the include and exclude lists.
///
/// An empty include list means every host is included, so an exclude list alone
/// still narrows scope.
pub fn is_host_in_scope(host: &str, include: &[String], exclude: &[String]) -> bool {
    let included = include.is_empty() || host_matches_any(host, include);
    included && !host_matches_any(host, exclude)
}

/// `*` matches any run of characters, including dots. Anchored at both ends, so
/// `log-*.example.com` does not match `x-log-y.example.com`.
fn glob_matches(value: &str, pattern: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(last) = parts.next_back() else {
        return value == first;
    };
    if !value.starts_with(first) || !value.ends_with(last) {
        return false;
    }
    // The leading and trailing literals may overlap when the value is short, e.g.
    // "ab" against "a*b" — the remaining span must be non-negative.
    if first.len() + last.len() > value.len() {
        return false;
    }

    let mut rest = &value[first.len()..value.len() - last.len()];
    for middle in parts {
        if middle.is_empty() {
            continue;
        }
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

pub fn normalize_host_for_matching(host: &str) -> String {
    let mut value = host.trim().to_ascii_lowercase();
    if let Some((_, rest)) = value.split_once("://") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("//") {
        value = rest.to_string();
    }
    let host = value.split(['/', '?', '#']).next().unwrap_or("").trim();
    strip_dns_root_dot(host_without_port(host)).to_string()
}

fn strip_dns_root_dot(host: &str) -> &str {
    host.strip_suffix('.').unwrap_or(host)
}

pub fn host_without_port(host: &str) -> &str {
    let trimmed = host.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &rest[..end];
        }
    }
    if trimmed.matches(':').count() == 1 {
        return trimmed
            .split_once(':')
            .map(|(host, _)| host)
            .unwrap_or(trimmed);
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_matches(host: &str, pattern: &str) -> bool {
        let hostname = normalize_host_for_matching(host);
        let normalized = normalize_host_for_matching(pattern);
        if let Some(suffix) = normalized.strip_prefix("*.") {
            if !suffix.contains('*') {
                return hostname == suffix || hostname.ends_with(&format!(".{suffix}"));
            }
        }
        if normalized.contains('*') {
            let parts: Vec<&str> = normalized.split('*').collect();
            let first = parts[0];
            let last = parts[parts.len() - 1];
            if !hostname.starts_with(first)
                || !hostname.ends_with(last)
                || first.len() + last.len() > hostname.len()
            {
                return false;
            }
            let mut rest = &hostname[first.len()..hostname.len() - last.len()];
            for middle in &parts[1..parts.len() - 1] {
                if middle.is_empty() {
                    continue;
                }
                let Some(at) = rest.find(middle) else {
                    return false;
                };
                rest = &rest[at + middle.len()..];
            }
            return true;
        }
        hostname == normalized
    }

    #[test]
    fn prepared_scope_preserves_raw_host_and_pattern_normalization() {
        let values = [
            "",
            " ",
            "*",
            "*.",
            "**",
            "*.*",
            "a*b",
            "ab",
            "a..",
            "*.EXAMPLE.com.",
            "https://API.Example.com:443/path?secret#fragment",
            "//api.example.com",
            "API.Example.com.:8443",
            "[2001:db8::1]:443",
            "[::1]ignored",
            "a:invalid",
            "\u{2003}API.example.com\u{2003}",
            "2001:db8::1",
            "[::1",
            "[::1]",
            "127.0.0.1:443",
            "127.1",
            "Ä.Example.com",
            "ä.example.com",
            "example.com@other.example.com",
            "a*b*c",
            "a**b",
            "a*b*",
            "*a*b",
        ];
        for host in values {
            for pattern in values {
                let expected = original_matches(host, pattern);
                assert_eq!(
                    host_matches_pattern(host, pattern),
                    expected,
                    "host={host:?}, pattern={pattern:?}"
                );
                let prepared = PreparedScope::new(&[pattern.into()], &[]);
                assert_eq!(prepared.contains(host), expected);
                let mut remaining = usize::MAX;
                assert_eq!(
                    prepared.contains_bounded(host, &mut remaining),
                    Ok(expected)
                );
                assert_eq!(
                    PreparedScope::new(&[], &[pattern.into()]).contains(host),
                    !expected
                );
            }
        }
        assert!(PreparedScope::new(&[], &[]).contains(""));
    }

    #[test]
    fn prepared_scope_budget_is_exact_and_never_treats_exhaustion_as_out_of_scope() {
        let host = "api.example.com";
        let patterns = vec!["*.example.com".to_owned()];
        let scope = PreparedScope::try_new(&patterns, &[], 500, 512).unwrap();
        let charge = host.len() + patterns[0].len() + 16;
        let mut exact = charge;
        assert_eq!(scope.contains_bounded(host, &mut exact), Ok(true));
        assert_eq!(exact, 0);
        let mut short = charge - 1;
        assert_eq!(
            scope.contains_bounded(host, &mut short),
            Err(ScopeWorkLimit)
        );
        assert_eq!(short, charge - 1);
        let mut empty = 0;
        assert_eq!(
            scope.contains_bounded(host, &mut empty),
            Err(ScopeWorkLimit)
        );
        let exact_scope = PreparedScope::try_new(&[host.into()], &[], 500, 512).unwrap();
        assert_eq!(exact_scope.contains_bounded(host, &mut empty), Ok(true));
        assert!(PreparedScope::try_new(&vec!["*".into(); 501], &[], 500, 512).is_none());
        assert!(PreparedScope::try_new(&[], &["x".repeat(513)], 500, 512).is_none());
    }

    #[test]
    fn prepared_scope_generated_globs_match_original_engine() {
        fn strings(alphabet: &[char], max: usize) -> Vec<String> {
            let mut all = vec![String::new()];
            let mut previous = all.clone();
            for _ in 0..max {
                let next: Vec<_> = previous
                    .iter()
                    .flat_map(|prefix| alphabet.iter().map(move |ch| format!("{prefix}{ch}")))
                    .collect();
                all.extend(next.iter().cloned());
                previous = next;
            }
            all
        }
        let patterns = strings(&['a', 'b', '*', '.'], 4);
        let hosts = strings(&['a', 'b', '.'], 4);
        for pattern in &patterns {
            let matcher = PreparedScope::new(&[pattern.clone()], &[]);
            for host in &hosts {
                let expected = original_matches(host, pattern);
                assert_eq!(
                    matcher.contains(host),
                    expected,
                    "host={host:?}, pattern={pattern:?}"
                );
                assert_eq!(host_matches_pattern(host, pattern), expected);
            }
        }
        for index in 0..patterns.len() {
            let include = vec![patterns[index].clone(), "*.example.com".into()];
            let exclude = vec![patterns[(index * 17 + 3) % patterns.len()].clone()];
            let matcher = PreparedScope::new(&include, &exclude);
            for host in &hosts {
                let expected = include.iter().any(|p| original_matches(host, p))
                    && !exclude.iter().any(|p| original_matches(host, p));
                assert_eq!(matcher.contains(host), expected);
                assert_eq!(is_host_in_scope(host, &include, &exclude), expected);
            }
        }
    }

    fn list(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn wildcard_domain_covers_the_domain_and_everything_under_it() {
        assert!(host_matches_pattern("example.com", "*.example.com"));
        assert!(host_matches_pattern("api.example.com", "*.example.com"));
        assert!(host_matches_pattern("a.b.example.com", "*.example.com"));
        assert!(!host_matches_pattern("notexample.com", "*.example.com"));
        assert!(!host_matches_pattern(
            "example.com.evil.net",
            "*.example.com"
        ));
    }

    #[test]
    fn a_star_inside_a_label_matches_that_family_of_hosts() {
        // The case this was added for: exclude every log- host under a domain.
        assert!(host_matches_pattern(
            "log-ingest.example.com",
            "log-*.example.com"
        ));
        assert!(host_matches_pattern(
            "log-x.example.com",
            "log-*.example.com"
        ));
        assert!(!host_matches_pattern(
            "cdn.example.com",
            "log-*.example.com"
        ));
        // Anchored: a host that merely contains the prefix does not match.
        assert!(!host_matches_pattern(
            "x-log-y.example.com",
            "log-*.example.com"
        ));
    }

    #[test]
    fn matching_ignores_case_port_and_the_root_dot() {
        assert!(host_matches_pattern("API.Example.COM:443", "*.example.com"));
        assert!(host_matches_pattern("api.example.com.", "*.example.com"));
        assert!(host_matches_pattern(
            "Log-Ingest.Example.com:443",
            "log-*.example.com"
        ));
    }

    #[test]
    fn an_exclude_narrows_an_include() {
        let include = list(&["*.example.com"]);
        let exclude = list(&["log-*.example.com"]);
        assert!(is_host_in_scope("api.example.com", &include, &exclude));
        assert!(!is_host_in_scope(
            "log-ingest.example.com",
            &include,
            &exclude
        ));
        assert!(!is_host_in_scope("other.test", &include, &exclude));
    }

    #[test]
    fn an_exclude_works_with_no_includes_at_all() {
        // No includes means everything is in scope, so an exclude list on its own
        // has to still take hosts out of it.
        let exclude = list(&["log-ingest.example.com"]);
        assert!(is_host_in_scope("api.example.com", &[], &exclude));
        assert!(!is_host_in_scope("log-ingest.example.com", &[], &exclude));
    }

    #[test]
    fn everything_is_in_scope_when_both_lists_are_empty() {
        assert!(is_host_in_scope("anything.example", &[], &[]));
    }
}
