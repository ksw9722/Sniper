//! An OpenAPI 3.1 description of one origin's API, built from its retained
//! rows as Burp's API Exporter and mitmproxy2swagger build one from captured
//! traffic. It lists only what the rows showed: each method and path template,
//! the parameter names its requests carried with a type guessed from their
//! values, the statuses and media types it answered with, and the credentials
//! its rows showed. No captured value is copied into it; each operation names
//! its newest row (`x-sniper-newest-transaction`), Sniper's own ID for it.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{service_map_auth::AuthSignal, service_map_parameters::EndpointParameter};

/// Operations described per origin, the most recently used first.
pub const MAX_OPERATIONS: usize = 500;
/// Each operation's newest rows whose parameters are read.
pub const MAX_ROWS_PER_OPERATION: usize = 20;

/// The methods a path item holds. OPTIONS is left out: captured, it is nearly
/// always a browser's CORS preflight rather than part of the API.
pub const METHODS: [&str; 7] = ["get", "put", "post", "delete", "head", "patch", "trace"];

/// What one method and path template's rows showed.
#[derive(Default)]
pub struct Operation {
    pub requests: usize,
    /// Each response status, with the media types answered with it.
    pub responses: BTreeMap<u16, BTreeSet<String>>,
    /// Media types of the request bodies sent.
    pub request_types: BTreeSet<String>,
    pub auth: BTreeSet<AuthSignal>,
    pub parameters: Vec<EndpointParameter>,
    /// The newest row's transaction ID, to open or send on.
    pub newest: Option<Uuid>,
}

/// A path ending in one of these names a static file, whatever it was answered
/// with: a font sent as `application/octet-stream`, a 304 with no type.
const STATIC_EXTENSIONS: [&str; 22] = [
    "css", "js", "mjs", "map", "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "avif", "bmp",
    "woff", "woff2", "ttf", "otf", "eot", "mp4", "webm", "mp3", "wav",
];

/// The OpenAPI path for a captured path: its ID segments as templates
/// (`path_group`), and a brace it had escaped so it is not read as one.
pub fn operation_path(path: &str) -> String {
    let path = path.replace('{', "%7B").replace('}', "%7D");
    crate::service_map::path_group(&path).unwrap_or(path)
}

/// A content type's media type, without parameters and lowercased.
pub fn media_type(content_type: &str) -> Option<String> {
    let media = content_type.split(';').next()?.trim().to_ascii_lowercase();
    media.contains('/').then_some(media)
}

/// `operations` by path template and lowercase method. `omitted` counts the
/// operations the rows showed beyond `MAX_OPERATIONS`.
pub fn document(
    origin: &str,
    operations: &BTreeMap<(String, String), Operation>,
    omitted: usize,
) -> Value {
    let (mut paths, mut schemes) = (Map::new(), Map::new());
    let (mut requests, mut site_only) = (0, 0);
    for ((path, method), operation) in operations {
        requests += operation.requests;
        if !api(path, operation) {
            site_only += 1;
            continue;
        }
        let (template, path_parameters) = path_template(path);
        let item = paths.entry(template).or_insert_with(|| json!({}));
        if !path_parameters.is_empty() {
            item["parameters"] = path_parameters.into();
        }
        let mut value = json!({
            "responses": responses(operation),
            "x-sniper-requests": operation.requests,
        });
        if let Some(newest) = operation.newest {
            value["x-sniper-newest-transaction"] = newest.to_string().into();
        }
        let query: Vec<Value> = operation
            .parameters
            .iter()
            .filter(|parameter| parameter.location == "query")
            .map(|parameter| {
                json!({"name": parameter.name, "in": "query", "schema": schema(&parameter.examples, false)})
            })
            .collect();
        if !query.is_empty() {
            value["parameters"] = query.into();
        }
        if let Some(body) = request_body(operation) {
            value["requestBody"] = body;
        }
        // Each credential the rows showed is one way in; the rows cannot
        // tell which ones the server requires.
        let security: Vec<Value> = operation
            .auth
            .iter()
            .filter_map(|signal| scheme_key(signal, &mut schemes))
            .map(|key| json!({ key: [] }))
            .collect();
        if !security.is_empty() {
            value["security"] = security.into();
        }
        item[method.as_str()] = value;
    }
    let mut notes = vec![format!(
        "Observed by Sniper in {requests} captured requests to {origin}. It lists only what \
         they showed: methods, paths with ID segments as {{number}}, {{uuid}} and {{hex}}, \
         parameter names with types guessed from their values, response statuses and media \
         types, and the credentials the rows carried. No captured value is included."
    )];
    if site_only > 0 {
        notes.push(format!(
            "{site_only} operations that only answered with pages or static files are left out."
        ));
    }
    if omitted > 0 {
        notes.push(format!(
            "{omitted} less recently used operations are left out; at most {MAX_OPERATIONS} are listed."
        ));
    }
    let mut document = json!({
        "openapi": "3.1.0",
        "info": {"title": origin, "version": "observed", "description": notes.join(" ")},
        "servers": [{"url": origin}],
        "paths": paths,
    });
    if !schemes.is_empty() {
        document["components"] = json!({ "securitySchemes": schemes });
    }
    document
}

/// An operation that only ever answered with pages or static files (HTML,
/// styles, scripts, images, fonts, media, a web app manifest), or whose path
/// names a static file, belongs to the site rather than its API. One that
/// answered with anything else, or with no typed body, is kept.
fn api(path: &str, operation: &Operation) -> bool {
    let file = path.rsplit('/').next().unwrap_or("");
    if file.rsplit_once('.').is_some_and(|(_, extension)| {
        STATIC_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    }) {
        return false;
    }
    let mut types = operation.responses.values().flatten().peekable();
    types.peek().is_none()
        || types.any(|media| {
            ![
                "html",
                "css",
                "javascript",
                "ecmascript",
                "manifest",
                "font",
                "image/",
                "audio/",
                "video/",
            ]
            .iter()
            .any(|word| media.contains(word))
        })
}

/// A template's ID segments named for OpenAPI, a repeated one numbered
/// (`{number}`, `{number2}`), with a path parameter for each.
fn path_template(path: &str) -> (String, Vec<Value>) {
    let mut counts = BTreeMap::new();
    let mut parameters = Vec::new();
    let template = path
        .split('/')
        .map(|segment| {
            let schema = match segment {
                "{number}" => json!({"type": "integer"}),
                "{uuid}" => json!({"type": "string", "format": "uuid"}),
                "{hex}" => json!({"type": "string", "pattern": "^[0-9A-Fa-f]+$"}),
                _ => return segment.to_string(),
            };
            let kind = &segment[1..segment.len() - 1];
            let count = counts.entry(kind).and_modify(|n| *n += 1).or_insert(1);
            let name = match *count {
                1 => kind.to_string(),
                count => format!("{kind}{count}"),
            };
            parameters
                .push(json!({"name": name, "in": "path", "required": true, "schema": schema}));
            format!("{{{name}}}")
        })
        .collect::<Vec<_>>()
        .join("/");
    (template, parameters)
}

/// A type every value seen fits, or a string. A leading zero (`007`) marks a
/// code rather than a number.
fn schema(examples: &[String], json_body: bool) -> Value {
    let every = |test: &dyn Fn(&str) -> bool| {
        !examples.is_empty() && examples.iter().all(|value| test(value))
    };
    let numeric = |value: &str| {
        !(value.len() > 1 && value.starts_with('0') && value.as_bytes()[1].is_ascii_digit())
            && value.parse::<f64>().is_ok_and(f64::is_finite)
    };
    let kind = if every(&|value| numeric(value) && value.parse::<i64>().is_ok()) {
        "integer"
    } else if every(&numeric) {
        "number"
    } else if every(&|value| value == "true" || value == "false") {
        "boolean"
    } else if json_body && every(&|value| value.starts_with('[')) {
        "array"
    } else if json_body && every(&|value| value.starts_with('{')) {
        "object"
    } else {
        "string"
    };
    json!({ "type": kind })
}

/// Each request media type, with the form, JSON or multipart fields sent as
/// it, matched as `ParameterTally` read them.
fn request_body(operation: &Operation) -> Option<Value> {
    let content: Map<String, Value> = operation
        .request_types
        .iter()
        .map(|media| {
            let location = if media.contains("x-www-form-urlencoded") {
                "form"
            } else if media.contains("json") {
                "json"
            } else if media.contains("multipart/form-data") {
                "multipart"
            } else {
                ""
            };
            let properties: Map<String, Value> = operation
                .parameters
                .iter()
                .filter(|parameter| parameter.location == location)
                .map(|parameter| {
                    let schema = schema(&parameter.examples, location == "json");
                    (parameter.name.clone(), schema)
                })
                .collect();
            let value = if properties.is_empty() {
                json!({})
            } else {
                json!({"schema": {"type": "object", "properties": properties}})
            };
            (media.clone(), value)
        })
        .collect();
    (!content.is_empty()).then(|| json!({ "content": content }))
}

fn responses(operation: &Operation) -> Value {
    let mut responses: Map<String, Value> = operation
        .responses
        .iter()
        .map(|(status, types)| {
            let reason = http::StatusCode::from_u16(*status)
                .ok()
                .and_then(|status| status.canonical_reason())
                .unwrap_or("Observed");
            let mut response = json!({ "description": reason });
            if !types.is_empty() {
                let content: Map<String, Value> = types
                    .iter()
                    .map(|media| (media.clone(), json!({})))
                    .collect();
                response["content"] = content.into();
            }
            (status.to_string(), response)
        })
        .collect();
    if responses.is_empty() {
        responses.insert(
            "default".into(),
            json!({"description": "No response was captured"}),
        );
    }
    responses.into()
}

/// The key of the security scheme a signal stands for, added to `schemes` on
/// first use. OAuth, OpenID Connect and SAML are sign-in exchanges rather than
/// credentials a request carries, so they have none.
fn scheme_key(signal: &AuthSignal, schemes: &mut Map<String, Value>) -> Option<String> {
    let name = signal.name.as_deref();
    let value = match (signal.kind, name) {
        ("bearer_jwt", _) => json!({"type": "http", "scheme": "bearer", "bearerFormat": "JWT"}),
        ("bearer" | "basic" | "digest" | "negotiate" | "ntlm", _) => {
            json!({"type": "http", "scheme": signal.kind})
        }
        ("aws_sigv4", _) => json!({"type": "http", "scheme": "AWS4-HMAC-SHA256"}),
        ("scheme", Some(name)) => json!({"type": "http", "scheme": name}),
        ("scheme", None) => json!({"type": "apiKey", "in": "header", "name": "Authorization"}),
        ("api_key_header", Some(name)) => json!({"type": "apiKey", "in": "header", "name": name}),
        ("session_cookie" | "jwt_cookie", Some(name)) => {
            json!({"type": "apiKey", "in": "cookie", "name": name})
        }
        ("query_key", Some(name)) => json!({"type": "apiKey", "in": "query", "name": name}),
        _ => return None,
    };
    // A component key allows letters, digits, `.`, `-` and `_` only; two
    // names that read the same once cleaned get numbered keys.
    let base: String = name
        .map_or_else(
            || signal.kind.to_string(),
            |name| format!("{}_{name}", signal.kind),
        )
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    let mut key = base.clone();
    for n in 2.. {
        match schemes.get(&key) {
            Some(existing) if *existing == value => break,
            Some(_) => key = format!("{base}_{n}"),
            None => {
                schemes.insert(key.clone(), value);
                break;
            }
        }
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameter(location: &str, name: &str, examples: &[&str]) -> EndpointParameter {
        EndpointParameter {
            name: name.into(),
            location: location.into(),
            rows: 1,
            examples: examples.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn signal(kind: &'static str, name: Option<&str>) -> AuthSignal {
        AuthSignal {
            kind,
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn a_path_names_its_id_segments_and_braces_stay_literal() {
        assert_eq!(
            operation_path("/chats/12/posts/507f1f77bcf86cd799439011"),
            "/chats/{number}/posts/{hex}"
        );
        assert_eq!(operation_path("/raw/{x}"), "/raw/%7Bx%7D");
        let (template, parameters) = path_template("/a/{number}/b/{number}/c/{uuid}");
        assert_eq!(template, "/a/{number}/b/{number2}/c/{uuid}");
        assert_eq!(
            parameters,
            [
                json!({"name": "number", "in": "path", "required": true, "schema": {"type": "integer"}}),
                json!({"name": "number2", "in": "path", "required": true, "schema": {"type": "integer"}}),
                json!({"name": "uuid", "in": "path", "required": true, "schema": {"type": "string", "format": "uuid"}}),
            ]
        );
    }

    #[test]
    fn types_are_guessed_from_values_that_are_then_dropped() {
        let kind = |values: &[&str], json_body| {
            let values: Vec<String> = values.iter().map(|value| value.to_string()).collect();
            schema(&values, json_body)["type"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(kind(&["1", "-20"], false), "integer");
        assert_eq!(kind(&["1", "2.5"], false), "number");
        assert_eq!(kind(&["007"], false), "string");
        assert_eq!(kind(&["true", "false"], false), "boolean");
        assert_eq!(kind(&["[\"a\"]"], true), "array");
        assert_eq!(kind(&["[\"a\"]"], false), "string");
        assert_eq!(kind(&["inf"], false), "string");
        assert_eq!(kind(&[], false), "string");
    }

    #[test]
    fn an_origin_becomes_an_openapi_document_without_its_values() {
        let mut operations = BTreeMap::new();
        operations.insert(
            ("/api/users/{number}".to_string(), "get".to_string()),
            Operation {
                requests: 3,
                responses: BTreeMap::from([
                    (200, BTreeSet::from(["application/json".to_string()])),
                    (404, BTreeSet::new()),
                ]),
                request_types: BTreeSet::new(),
                auth: BTreeSet::from([
                    signal("bearer_jwt", None),
                    signal("session_cookie", Some("SID")),
                ]),
                parameters: vec![parameter("query", "page", &["2"])],
                newest: Some(Uuid::from_u128(7)),
            },
        );
        operations.insert(
            ("/api/users".to_string(), "post".to_string()),
            Operation {
                requests: 1,
                responses: BTreeMap::from([(
                    201,
                    BTreeSet::from(["application/json".to_string()]),
                )]),
                request_types: BTreeSet::from(["application/json".to_string()]),
                auth: BTreeSet::from([signal("oauth2", None)]),
                parameters: vec![parameter("json", "name", &["kim-secret-value"])],
                newest: None,
            },
        );
        operations.insert(
            ("/".to_string(), "get".to_string()),
            Operation {
                requests: 5,
                responses: BTreeMap::from([(200, BTreeSet::from(["text/html".to_string()]))]),
                ..Default::default()
            },
        );
        operations.insert(
            ("/beacon".to_string(), "post".to_string()),
            Operation {
                requests: 1,
                ..Default::default()
            },
        );
        operations.insert(
            ("/fonts/Brand.woff2".to_string(), "get".to_string()),
            Operation {
                requests: 1,
                responses: BTreeMap::from([(304, BTreeSet::new())]),
                ..Default::default()
            },
        );
        let document = document("https://api.example.com", &operations, 2);
        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(
            document["servers"],
            json!([{"url": "https://api.example.com"}])
        );
        let paths = document["paths"].as_object().unwrap();
        assert_eq!(
            paths.keys().collect::<Vec<_>>(),
            ["/api/users", "/api/users/{number}", "/beacon"]
        );
        let get = &paths["/api/users/{number}"]["get"];
        assert_eq!(
            paths["/api/users/{number}"]["parameters"][0]["name"],
            "number"
        );
        assert_eq!(
            get["parameters"],
            json!([{"name": "page", "in": "query", "schema": {"type": "integer"}}])
        );
        assert_eq!(get["responses"]["200"]["description"], "OK");
        assert_eq!(
            get["responses"]["200"]["content"],
            json!({"application/json": {}})
        );
        assert_eq!(get["responses"]["404"], json!({"description": "Not Found"}));
        assert_eq!(
            get["security"],
            json!([{"bearer_jwt": []}, {"session_cookie_SID": []}])
        );
        assert_eq!(get["x-sniper-requests"], 3);
        assert_eq!(
            get["x-sniper-newest-transaction"],
            "00000000-0000-0000-0000-000000000007"
        );
        let post = &paths["/api/users"]["post"];
        assert_eq!(
            post["requestBody"],
            json!({"content": {"application/json": {"schema": {"type": "object", "properties": {"name": {"type": "string"}}}}}})
        );
        assert!(post.get("security").is_none());
        assert_eq!(
            paths["/beacon"]["post"]["responses"],
            json!({"default": {"description": "No response was captured"}})
        );
        assert_eq!(
            document["components"]["securitySchemes"],
            json!({
                "bearer_jwt": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
                "session_cookie_SID": {"type": "apiKey", "in": "cookie", "name": "SID"},
            })
        );
        let description = document["info"]["description"].as_str().unwrap();
        assert!(description.contains("11 captured requests"));
        assert!(
            description.contains("2 operations that only answered with pages"),
            "a page, and a font by its name"
        );
        assert!(description.contains("2 less recently used operations"));
        assert!(!document.to_string().contains("kim-secret-value"));
    }

    #[test]
    fn names_that_read_the_same_once_cleaned_get_their_own_keys() {
        let mut schemes = Map::new();
        let first = scheme_key(&signal("api_key_header", Some("x-a+b")), &mut schemes);
        let second = scheme_key(&signal("api_key_header", Some("x-a$b")), &mut schemes);
        let again = scheme_key(&signal("api_key_header", Some("x-a+b")), &mut schemes);
        assert_eq!(first.as_deref(), Some("api_key_header_x-a_b"));
        assert_eq!(second.as_deref(), Some("api_key_header_x-a_b_2"));
        assert_eq!(again, first);
        assert_eq!(scheme_key(&signal("saml", None), &mut schemes), None);
    }
}
