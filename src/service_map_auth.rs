//! How each request authenticates, read from names and shapes only: the
//! Authorization scheme, an API key header, the session cookies a request
//! carried or a response set, the challenge a response made, and the OAuth,
//! OpenID Connect and SAML parameters in a URL. ZAP's authentication and
//! session-management passive rules were the checklist; the rules are
//! Sniper's own. A credential's value is never kept or reported: a JWT is told
//! from an opaque token by its shape, and only names from fixed lists or a
//! cookie's own name leave this module.

use crate::model::{HeaderRecord, TransactionRecord};

/// Every kind, in the order a card and the inspector present them: what a
/// request carries first, then the flows that hand credentials out.
pub const KINDS: [&str; 15] = [
    "bearer_jwt",
    "bearer",
    "basic",
    "digest",
    "negotiate",
    "ntlm",
    "aws_sigv4",
    "scheme",
    "api_key_header",
    "session_cookie",
    "jwt_cookie",
    "query_key",
    "oauth2",
    "oidc",
    "saml",
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthSignal {
    pub kind: &'static str,
    /// The header, cookie or parameter name, or the scheme, for the kinds
    /// that carry one.
    pub name: Option<String>,
}

// Headers that carry an API key or token by name.
const API_KEY_HEADERS: [&str; 11] = [
    "x-api-key",
    "api-key",
    "apikey",
    "x-apikey",
    "x-auth-token",
    "x-access-token",
    "x-api-token",
    "x-token",
    "x-goog-api-key",
    "ocp-apim-subscription-key",
    "x-functions-key",
];
// Query parameters that carry a credential in the URL itself.
const QUERY_KEYS: [&str; 5] = [
    "access_token",
    "id_token",
    "api_key",
    "apikey",
    "auth_token",
];
// Authorization schemes beyond the standard ones, by their usual spelling.
// Anything else is reported without a name: an unknown first word may be the
// credential itself.
const SCHEMES: [(&str, &str); 13] = [
    ("token", "Token"),
    ("jwt", "JWT"),
    ("apikey", "ApiKey"),
    ("api-key", "ApiKey"),
    ("hmac", "HMAC"),
    ("hawk", "Hawk"),
    ("oauth", "OAuth"),
    ("sharedkey", "SharedKey"),
    ("ssws", "SSWS"),
    ("bot", "Bot"),
    ("key", "Key"),
    ("signature", "Signature"),
    ("mac", "MAC"),
];
const MAX_HEADERS: usize = 64;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_PAIRS: usize = 64;

/// What this row's request and response say about how it authenticates.
pub fn detect(record: &TransactionRecord) -> Vec<AuthSignal> {
    let mut found = Vec::new();
    for header in bounded(&record.request.headers) {
        let name = header.name.to_ascii_lowercase();
        match name.as_str() {
            "authorization" => found.extend(authorization(&header.value)),
            "cookie" => {
                for pair in header.value.split(';').take(MAX_PAIRS) {
                    let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                    found.extend(cookie(name.trim(), value));
                }
            }
            _ if API_KEY_HEADERS.contains(&name.as_str()) => found.push(AuthSignal {
                kind: "api_key_header",
                name: Some(name),
            }),
            _ => {}
        }
    }
    if let Some(response) = &record.response {
        for header in bounded(&response.headers) {
            if header.name.eq_ignore_ascii_case("www-authenticate") {
                found.extend(challenge(&header.value));
            } else if header.name.eq_ignore_ascii_case("set-cookie") {
                let pair = header.value.split(';').next().unwrap_or("");
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                found.extend(cookie(name.trim(), value));
            }
        }
    }
    found.extend(url(&record.path));
    found.sort();
    found.dedup();
    found
}

fn bounded(headers: &[HeaderRecord]) -> impl Iterator<Item = &HeaderRecord> {
    headers
        .iter()
        .take(MAX_HEADERS)
        .filter(|header| header.value.len() <= MAX_VALUE_BYTES)
}

fn signal(kind: &'static str) -> AuthSignal {
    AuthSignal { kind, name: None }
}

fn authorization(value: &str) -> Option<AuthSignal> {
    let value = value.trim();
    let (scheme, credentials) = value
        .split_once(char::is_whitespace)
        .map_or((value, ""), |(scheme, rest)| (scheme, rest.trim()));
    let lower = scheme.to_ascii_lowercase();
    Some(match lower.as_str() {
        "basic" => signal("basic"),
        "bearer" if jwt(credentials) => signal("bearer_jwt"),
        "bearer" => signal("bearer"),
        "digest" => signal("digest"),
        "negotiate" => signal("negotiate"),
        "ntlm" => signal("ntlm"),
        "aws4-hmac-sha256" => signal("aws_sigv4"),
        // A bare token with no scheme in front of it.
        _ if credentials.is_empty() && jwt(scheme) => signal("bearer_jwt"),
        _ if scheme.is_empty() => return None,
        _ => AuthSignal {
            kind: "scheme",
            name: SCHEMES
                .iter()
                .find(|(spelling, _)| *spelling == lower)
                .map(|(_, name)| (*name).to_string()),
        },
    })
}

fn challenge(value: &str) -> Option<AuthSignal> {
    let scheme = value
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()?;
    Some(match scheme.to_ascii_lowercase().as_str() {
        "basic" => signal("basic"),
        "bearer" => signal("bearer"),
        "digest" => signal("digest"),
        "negotiate" => signal("negotiate"),
        "ntlm" => signal("ntlm"),
        _ => return None,
    })
}

/// A cookie that carries a JWT, or whose name says it holds a session.
fn cookie(name: &str, value: &str) -> Option<AuthSignal> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return None;
    }
    let kind = if jwt(value) {
        "jwt_cookie"
    } else if session_cookie(name) {
        "session_cookie"
    } else {
        return None;
    };
    Some(AuthSignal {
        kind,
        name: Some(name.to_string()),
    })
}

fn session_cookie(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.contains("csrf") || lower.contains("xsrf") {
        return false;
    }
    // Short words count only as a whole part, so `inside` is not a `sid`.
    lower.contains("sess")
        || lower.starts_with("wordpress_logged_in")
        || lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|part| matches!(part, "sid" | "sso" | "jwt" | "auth" | "token" | "login"))
}

/// A JSON Web Signature's shape: three base64url parts, the first two JSON.
fn jwt(value: &str) -> bool {
    let value = value.trim().trim_matches('"');
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts[1].starts_with("eyJ")
        && parts.iter().all(|part| {
            part.len() >= 10
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

fn url(target: &str) -> Vec<AuthSignal> {
    let mut found = Vec::new();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = path.split('#').next().unwrap_or("");
    if path.ends_with("/.well-known/openid-configuration") {
        found.push(signal("oidc"));
    }
    if path.ends_with("/.well-known/oauth-authorization-server") {
        found.push(signal("oauth2"));
    }
    let pairs: Vec<(String, &str)> = query
        .split('#')
        .next()
        .unwrap_or("")
        .split('&')
        .take(MAX_PAIRS)
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (name.to_ascii_lowercase(), value)
        })
        .collect();
    let has = |name: &str| pairs.iter().any(|(key, _)| key == name);
    // An authorization request names the client and what it wants back.
    if has("response_type") && has("client_id") {
        found.push(signal("oauth2"));
        if pairs.iter().any(|(key, value)| {
            key == "scope"
                && value
                    .split(['+', ' ', ','])
                    .chain(value.split("%20"))
                    .any(|scope| scope.eq_ignore_ascii_case("openid"))
        }) {
            found.push(signal("oidc"));
        }
    }
    if has("samlrequest") || has("samlresponse") {
        found.push(signal("saml"));
    }
    if has("x-amz-signature") || has("x-amz-credential") {
        found.push(signal("aws_sigv4"));
    }
    for key in QUERY_KEYS {
        if has(key) {
            found.push(AuthSignal {
                kind: "query_key",
                name: Some(key.to_string()),
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MessageRecord;

    // Split so secret scanning does not take a test sample for a leaked token.
    const TOKEN: &str = concat!(
        "eyJhbGciOiJIUzI1NiJ9.",
        "eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
    );

    fn record(
        path: &str,
        request: &[(&str, &str)],
        response: &[(&str, &str)],
    ) -> TransactionRecord {
        let headers = |pairs: &[(&str, &str)]| {
            let mut map = http::HeaderMap::new();
            for (name, value) in pairs {
                map.append(
                    http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    value.parse().unwrap(),
                );
            }
            MessageRecord::from_headers_and_body(&map, b"", 0)
        };
        TransactionRecord::http(
            chrono::Utc::now(),
            "GET".into(),
            "https".into(),
            "api.example.com".into(),
            path.into(),
            Some(200),
            1,
            headers(request),
            Some(headers(response)),
            vec![],
            None,
            None,
        )
    }

    fn kinds(found: Vec<AuthSignal>) -> Vec<(&'static str, Option<String>)> {
        found
            .into_iter()
            .map(|item| (item.kind, item.name))
            .collect()
    }

    #[test]
    fn requests_name_their_scheme_key_header_and_session_cookies() {
        let bearer = format!("Bearer {TOKEN}");
        let found = kinds(detect(&record(
            "/v1/items",
            &[
                ("Authorization", bearer.as_str()),
                ("X-API-Key", "k-123"),
                (
                    "Cookie",
                    "JSESSIONID=abc; _ga=GA1.2.3; XSRF-TOKEN=x; inside=1; jwt_id=eyJ.no; NID_AUT=z",
                ),
            ],
            &[],
        )));
        assert_eq!(
            found,
            [
                ("api_key_header", Some("x-api-key".into())),
                ("bearer_jwt", None),
                ("session_cookie", Some("JSESSIONID".into())),
                ("session_cookie", Some("jwt_id".into())),
            ]
        );
        for (value, expected) in [
            ("Basic dXNlcjpwYXNz", ("basic", None)),
            ("Bearer 3f2a9c8e1d", ("bearer", None)),
            ("Digest username=\"u\", realm=\"r\"", ("digest", None)),
            (
                "Negotiate YIIGhgYGKwYBBQUCoIIGejCCBnag",
                ("negotiate", None),
            ),
            ("NTLM TlRMTVNTUAABAAAAB4IIog==", ("ntlm", None)),
            (
                "AWS4-HMAC-SHA256 Credential=AKID/20261016/us-east-1/s3/aws4_request",
                ("aws_sigv4", None),
            ),
            (
                "Token 9944b09199c62bcf9418ad846dd0e4bbdfc6ee4b",
                ("scheme", Some("Token".to_string())),
            ),
            (TOKEN, ("bearer_jwt", None)),
        ] {
            assert_eq!(
                kinds(detect(&record("/", &[("Authorization", value)], &[]))),
                [expected.clone()],
                "{value}"
            );
        }
        // An unknown first word may be the credential itself: no name leaves.
        assert_eq!(
            kinds(detect(&record(
                "/",
                &[("Authorization", "9f86d081884c7d659a2feaa0c55ad015")],
                &[]
            ))),
            [("scheme", None)]
        );
    }

    #[test]
    fn responses_name_their_challenge_and_the_sessions_they_set() {
        let jwt_cookie = format!("access={TOKEN}; Path=/; HttpOnly");
        let found = kinds(detect(&record(
            "/login",
            &[],
            &[
                ("WWW-Authenticate", "Basic realm=\"admin\""),
                ("WWW-Authenticate", "Negotiate"),
                ("Set-Cookie", "PHPSESSID=abc; path=/"),
                ("Set-Cookie", jwt_cookie.as_str()),
                ("Set-Cookie", "theme=dark; path=/"),
            ],
        )));
        assert_eq!(
            found,
            [
                ("basic", None),
                ("jwt_cookie", Some("access".into())),
                ("negotiate", None),
                ("session_cookie", Some("PHPSESSID".into())),
            ]
        );
    }

    #[test]
    fn urls_name_oauth_oidc_saml_signed_urls_and_query_keys() {
        let detected = |path: &str| kinds(detect(&record(path, &[], &[])));
        assert_eq!(
            detected(
                "/oauth2/authorize?response_type=code&client_id=app&scope=openid%20profile&state=x"
            ),
            [("oauth2", None), ("oidc", None)]
        );
        assert_eq!(
            detected("/authorize?client_id=app&response_type=token&scope=read"),
            [("oauth2", None)]
        );
        assert_eq!(
            detected("/.well-known/openid-configuration"),
            [("oidc", None)]
        );
        assert_eq!(
            detected("/sso?SAMLRequest=fVJN&RelayState=x"),
            [("saml", None)]
        );
        assert_eq!(
            detected("/bucket/key?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Signature=abc"),
            [("aws_sigv4", None)]
        );
        assert_eq!(
            detected("/feed?access_token=abc&page=2"),
            [("query_key", Some("access_token".into()))]
        );
        assert!(
            detected("/search?q=client_id&response_type").is_empty(),
            "a value that names a parameter is not that parameter"
        );
        assert!(
            detected("/items?page=2&client_id=web").is_empty(),
            "a client id alone"
        );
    }

    #[test]
    fn cookie_names_are_reported_only_when_safe() {
        assert_eq!(cookie("sess id", "x"), None);
        assert_eq!(cookie(&"s".repeat(65), "x"), None);
        assert!(!session_cookie("csrf_session"));
        assert!(session_cookie("ASP.NET_SessionId"));
        assert!(session_cookie("connect.sid"));
        assert!(session_cookie("wordpress_logged_in_abc"));
        assert!(!session_cookie("consider"));
    }
}
