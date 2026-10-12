//! Offline discovery and synthetic loopback checks. No proxy, saved session,
//! captured traffic, or user data directory is opened.
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sniper::model::{TrafficKind, TransactionSummary};
use sniper::session::SessionSummary;
use std::process::Command;
use uuid::Uuid;

#[path = "support/schema_vocabulary.rs"]
mod schema_vocabulary;

fn cli(args: &[&str], expected_code: i32) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_sniper-cli"))
        .args(["--output", "compact", "--api", "http://127.0.0.1:1"])
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(expected_code), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn spec(operation: &str) -> Value {
    cli(&["manifest"], 0)["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|spec| spec["operation"] == operation)
        .unwrap()
        .clone()
}

fn output_schema(operation: &str) -> Value {
    let schema = cli(&["schema", "output", operation], 0)["schema"].clone();
    assert_eq!(schema, spec(operation)["output_schema"]);
    let input = json!({"kind":"output", "operation":operation}).to_string();
    let called = cli(&["call", "schema", "--input", &input], 0);
    assert_eq!(called["data"]["schema"], schema);
    schema
}

// Only the JSON Schema vocabulary used by these read-only output contracts.
fn matches_schema(value: &Value, schema: &Value) -> bool {
    schema_vocabulary::assert_supported(
        schema,
        &[
            "type",
            "enum",
            "required",
            "properties",
            "additionalProperties",
            "items",
            "minimum",
            "maximum",
            "minItems",
            "maxItems",
            "pattern",
            "format",
            "allOf",
            "anyOf",
            "oneOf",
            "x-sniper-json-number-representation",
        ],
        &["object", "array", "string", "integer", "boolean", "null"],
        &["uuid", "date-time", "ipv4", "ipv6"],
    );
    matches_schema_value(value, schema)
}

fn matches_schema_value(value: &Value, schema: &Value) -> bool {
    if schema["allOf"].as_array().is_some_and(|constraints| {
        !constraints
            .iter()
            .all(|constraint| matches_schema_value(value, constraint))
    }) {
        return false;
    }
    if let Some(ty) = schema.get("type") {
        let matches_type = |ty: &Value| match ty.as_str().unwrap() {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            other => panic!("unexpected schema type {other}"),
        };
        if let Some(types) = ty.as_array() {
            if !types.iter().any(matches_type) {
                return false;
            }
        } else if !matches_type(ty) {
            return false;
        }
    }
    if schema["enum"]
        .as_array()
        .is_some_and(|items| !items.contains(value))
    {
        return false;
    }
    if let Some(fields) = value.as_object() {
        if schema["required"].as_array().is_some_and(|required| {
            required
                .iter()
                .any(|name| !fields.contains_key(name.as_str().unwrap()))
        }) {
            return false;
        }
        for (key, item) in fields {
            if let Some(property) = schema["properties"].get(key) {
                if !matches_schema_value(item, property) {
                    return false;
                }
            } else if schema["additionalProperties"] == false {
                return false;
            }
        }
    }
    if let Some(items) = value.as_array() {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|min| items.len() < min as usize)
            || schema["maxItems"]
                .as_u64()
                .is_some_and(|max| items.len() > max as usize)
        {
            return false;
        }
        if let Some(item_schema) = schema.get("items") {
            if items
                .iter()
                .any(|item| !matches_schema_value(item, item_schema))
            {
                return false;
            }
        }
    }
    if value.is_number() {
        let integer = value
            .as_u64()
            .map(i128::from)
            .or_else(|| value.as_i64().map(i128::from));
        if schema["minimum"]
            .as_u64()
            .is_some_and(|min| integer.is_none_or(|n| n < i128::from(min)))
            || schema["maximum"]
                .as_u64()
                .is_some_and(|max| integer.is_none_or(|n| n > i128::from(max)))
        {
            return false;
        }
    }
    if let Some(text) = value.as_str() {
        if schema["pattern"]
            .as_str()
            .is_some_and(|pattern| !regex::Regex::new(pattern).unwrap().is_match(text))
        {
            return false;
        }
        match schema["format"].as_str() {
            Some("uuid") if Uuid::parse_str(text).is_err() => return false,
            Some("ipv4") if text.parse::<std::net::Ipv4Addr>().is_err() => return false,
            Some("ipv6") if text.parse::<std::net::Ipv6Addr>().is_err() => return false,
            Some("date-time") if DateTime::parse_from_rfc3339(text).is_err() => return false,
            _ => {}
        }
    }
    if schema["anyOf"].as_array().is_some_and(|choices| {
        !choices
            .iter()
            .any(|choice| matches_schema_value(value, choice))
    }) {
        return false;
    }
    if schema["oneOf"].as_array().is_some_and(|choices| {
        choices
            .iter()
            .filter(|choice| matches_schema_value(value, choice))
            .count()
            != 1
    }) {
        return false;
    }
    true
}

fn assert_schema(value: &Value, schema: &Value) {
    assert!(
        matches_schema(value, schema),
        "{value} does not match {schema}"
    );
}

#[test]
fn service_map_parse_errors_hide_rejected_credentials_and_queries() {
    for args in [
        vec![
            "scope",
            "service-map",
            "--origin",
            "https://user:SYNTHETIC_PRIVATE_VALUE@example.com",
        ],
        vec![
            "target",
            "service-map",
            "body-references",
            "--session-id",
            "00000000-0000-0000-0000-000000000000",
            "--evidence-id",
            "https://example.com?token=SYNTHETIC_PRIVATE_VALUE",
        ],
        vec![
            "scope",
            "service-map-evidence",
            "--session-id",
            "00000000-0000-0000-0000-000000000000",
            "--evidence-id",
            "https://example.com?token=SYNTHETIC_PRIVATE_VALUE",
        ],
    ] {
        let result = cli(&args, 2);
        assert_eq!(result["error"]["code"], "INVALID_INPUT");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
    }
}

#[test]
fn upstream_peer_settings_schemas_are_strict_and_discoverable_offline() {
    let mut value = json!({"session_id":Uuid::nil(),"enabled":false,"config_revision":"9007199254740993",
        "config_token":"a".repeat(64),"status":{"effective_enabled":false,"blocked_reason":null}});
    for operation in [
        "capture.http.upstream_peer.get",
        "capture.http.upstream_peer.set",
    ] {
        let operation_spec = spec(operation);
        let write = operation.ends_with(".set");
        assert_eq!(
            operation_spec["side_effect"],
            if write { "write" } else { "read" }
        );
        assert_eq!(operation_spec["requires_confirmation"], write);
        assert_eq!(
            operation_spec["input_schema"]["additionalProperties"],
            false
        );
        let schema = output_schema(operation);
        if write {
            value["changed"] = json!(false);
        }
        assert_schema(&value, &schema);
        for reason in [
            "persistence_uncertain",
            "invalid_configuration",
            "http_capture_paused",
        ] {
            value["status"]["blocked_reason"] = json!(reason);
            assert_schema(&value, &schema);
        }
        value["status"]["blocked_reason"] = Value::Null;
        let mut invalid = value.clone();
        invalid["status"]["blocked_reason"] = json!("anything_else");
        assert!(!matches_schema(&invalid, &schema));
        invalid = value.clone();
        invalid["private"] = json!("forbidden");
        assert!(!matches_schema(&invalid, &schema));
        invalid = value.clone();
        invalid["config_revision"] = json!(9007199254740993_u64);
        assert!(!matches_schema(&invalid, &schema));
        invalid = value.clone();
        invalid["config_revision"] = json!("18446744073709551616");
        assert!(!matches_schema(&invalid, &schema));
        for field in [
            "session_id",
            "enabled",
            "config_revision",
            "config_token",
            "status",
        ] {
            invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(!matches_schema(&invalid, &schema), "{operation}: {field}");
        }
        if write {
            invalid = value.clone();
            invalid.as_object_mut().unwrap().remove("changed");
            assert!(!matches_schema(&invalid, &schema));
        }
    }
}

#[test]
fn schema_matcher_guards_its_subset_and_checks_formats() {
    for schema in [
        json!({"properties":{"optional":{"const":"expected"}}}),
        json!({"properties":{"optional":{"type":"number"}}}),
        json!({"type":["object","number"]}),
        json!({"type":["object",false]}),
        json!({"anyOf":[{}, {"type":"number"}]}),
        json!({"properties":{"optional":{"minimum":-1}}}),
        json!({"anyOf":[{}, {"maximum":1.5}]}),
        json!({"properties":{"optional":{"pattern":false}}}),
        json!({"anyOf":[{}, {"pattern":"(?=a)"}]}),
    ] {
        assert!(
            std::panic::catch_unwind(|| matches_schema(&json!({}), &schema)).is_err(),
            "unsupported schema was accepted: {schema}"
        );
    }
    let integer = json!({"type":"integer","minimum":0,"maximum":u64::MAX});
    assert!(matches_schema(&json!(u64::MAX), &integer));
    assert!(!matches_schema(&json!(-1), &integer));
    for (format, valid) in [
        ("uuid", "00000000-0000-0000-0000-000000000001"),
        ("date-time", "2026-10-10T12:00:00Z"),
    ] {
        let schema = json!({"type":"string","format":format});
        assert!(matches_schema(&json!(valid), &schema));
        assert!(!matches_schema(&json!("invalid"), &schema));
    }
}

fn summary(annotated: bool) -> TransactionSummary {
    TransactionSummary {
        id: Uuid::from_u128(1),
        started_at: "2026-10-10T12:00:00Z".parse().unwrap(),
        kind: TrafficKind::Http,
        sequence: 42,
        method: "GET".into(),
        scheme: "https".into(),
        host: "example.com".into(),
        path: "/fixture".into(),
        status: annotated.then_some(200),
        duration_ms: 0,
        request_bytes: 0,
        response_bytes: 0,
        note_count: 0,
        has_response: annotated,
        content_type: annotated.then(|| "text/plain".into()),
        is_websocket: false,
        has_match_replace: false,
        color_tag: annotated.then(|| "blue".into()),
        has_user_note: annotated,
        note_preview: annotated.then(|| "Synthetic note".into()),
        annotation_revision: u64::from(annotated),
        header_search_text: if annotated {
            "Accept: text/plain".into()
        } else {
            String::new()
        },
    }
}

fn labeled_summary(annotated: bool) -> Value {
    let mut row = serde_json::to_value(summary(annotated)).unwrap();
    row["label"] = json!("#42 GET example.com/fixture");
    row
}

#[test]
fn examples_output_schema_describes_catalog_and_specific_data() {
    let schema = output_schema("examples");
    let catalog = cli(&["examples"], 0);
    assert_schema(&catalog, &schema);
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
    assert_schema(&json!([]), &schema);
    for input in ["{}", r#"{"operation":null}"#] {
        let envelope = cli(&["call", "examples", "--input", input], 0);
        assert_schema(&envelope["data"], &schema);
        assert!(!matches_schema(&envelope, &schema));
    }
    let specific = cli(&["examples", "session.list"], 0);
    assert_schema(&specific, &schema);
    let called = cli(
        &[
            "call",
            "examples",
            "--input",
            r#"{"operation":"session.list"}"#,
        ],
        0,
    );
    assert_schema(&called["data"], &schema);
    // Example values belong to each operation's input contract, not a shared
    // catalog contract that assumes every legacy input has identical fields.
    assert_schema(
        &json!({"operation":"example", "examples":[null, true, 1, "text", [], {}]}),
        &schema,
    );
    for malformed in [
        json!({}),
        json!([{"operation":"example", "examples":[]}]),
        json!({"operation":"example", "examples":{}}),
    ] {
        assert!(!matches_schema(&malformed, &schema));
    }
}

#[test]
fn history_output_schema_matches_labeled_summaries_and_nullable_page_metadata() {
    let schema = output_schema("capture.http.list");
    assert_schema(&json!([labeled_summary(false)]), &schema);
    let alternatives = schema["oneOf"].as_array().unwrap();
    assert_eq!(alternatives.len(), 2);
    let array = alternatives
        .iter()
        .find(|choice| choice["type"] == "array")
        .unwrap();
    let page = alternatives
        .iter()
        .find(|choice| choice["type"] == "object")
        .unwrap();
    let mut expected_summary = sniper::saved_contract::transaction_summary_schema();
    expected_summary["properties"]["label"] = json!({"type":"string"});
    expected_summary["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("label"));
    assert_eq!(array["items"], expected_summary);
    assert_eq!(page["properties"]["items"], *array);
    assert_schema(&json!([]), &schema);
    for annotated in [false, true] {
        let row = labeled_summary(annotated);
        assert_schema(&json!([row]), &schema);
        for metadata in [
            json!({"total":12, "filtered_total":8, "hidden_connect_total":1, "offset":5, "limit":1, "has_more":true}),
            json!({"total":null, "filtered_total":null, "hidden_connect_total":null, "offset":null, "limit":null, "has_more":null}),
        ] {
            let mut value = metadata;
            value["items"] = json!([row]);
            assert_schema(&value, &schema);
            value.as_object_mut().unwrap().remove("offset");
            assert!(!matches_schema(&value, &schema));
        }
        let mut missing_label = row.clone();
        missing_label.as_object_mut().unwrap().remove("label");
        assert!(!matches_schema(&json!([missing_label]), &schema));
        let mut nullable_optional = row;
        nullable_optional["note_preview"] = Value::Null;
        assert!(!matches_schema(&json!([nullable_optional]), &schema));
    }
    for malformed in [json!({}), json!([{}]), json!({"ok":true,"data":[]})] {
        assert!(!matches_schema(&malformed, &schema));
    }
}

#[test]
fn examples_discovery_matches_optional_nullable_operation() {
    let spec = spec("examples");
    let schema = cli(&["schema", "input", "examples"], 0)["schema"].clone();
    assert_eq!(schema, spec["input_schema"]);
    assert_eq!(spec["command"], "examples [operation]");
    assert_eq!(schema["required"], json!([]));
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["operation"]["type"],
        json!(["string", "null"])
    );
    assert!(spec["examples"].as_array().unwrap().contains(&json!({})));

    let catalog = cli(&["examples"], 0);
    assert!(catalog.as_array().is_some_and(|items| !items.is_empty()));
    for input in ["{}", r#"{"operation":null}"#] {
        let envelope = cli(&["call", "examples", "--input", input], 0);
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["operation"], "examples");
        assert_eq!(envelope["data"], catalog);
    }
    let specific = cli(&["examples", "session.list"], 0);
    let called = cli(
        &[
            "call",
            "examples",
            "--input",
            r#"{"operation":"session.list"}"#,
        ],
        0,
    );
    assert_eq!(called["data"], specific);
    assert_eq!(specific["operation"], "session.list");
    for input in [
        r#"{"operation":1}"#,
        r#"{"operation":true}"#,
        r#"{"operation":[]}"#,
        r#"{"operation":{}}"#,
        r#"{"unexpected":null}"#,
    ] {
        let error = cli(&["call", "examples", "--input", input], 2);
        assert_eq!(error["ok"], false);
        assert_eq!(error["error"]["code"], "INVALID_INPUT");
    }
}

#[test]
fn session_list_discovery_describes_serialized_summary_array() {
    let spec = spec("session.list");
    let schema = cli(&["schema", "output", "session.list"], 0)["schema"].clone();
    assert_eq!(schema, spec["output_schema"]);
    let called = cli(
        &[
            "call",
            "schema",
            "--input",
            r#"{"kind":"output","operation":"session.list"}"#,
        ],
        0,
    );
    assert_eq!(called["data"]["schema"], schema);
    assert_eq!(schema["type"], "array");
    // Adding the output schema must not advertise a new runtime input.
    assert_eq!(spec["input_schema"]["properties"], json!({}));
    assert_eq!(spec["input_schema"]["required"], json!([]));

    let timestamp = "2026-10-10T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
    let rows = serde_json::to_value(vec![SessionSummary {
        id: Uuid::nil(),
        name: "Synthetic offline fixture".into(),
        created_at: timestamp,
        updated_at: timestamp,
        last_opened_at: timestamp,
        request_count: 0,
        websocket_count: 1,
        event_count: 2,
        fuzzer_count: 0,
        rule_count: 0,
        storage_path: "synthetic-session".into(),
        active: false,
    }])
    .unwrap();
    let row = rows[0].as_object().unwrap();
    let item = &schema["items"];
    let properties = item["properties"].as_object().unwrap();
    assert_eq!(item["type"], "object");
    assert_eq!(*item, sniper::saved_contract::session_summary_schema());
    assert_eq!(properties.len(), row.len());
    assert_eq!(item["required"].as_array().unwrap().len(), row.len());
    for (name, value) in row {
        assert!(item["required"].as_array().unwrap().contains(&json!(name)));
        let field = &properties[name];
        match field["type"].as_str().unwrap() {
            "string" => assert!(value.is_string(), "{name}"),
            "integer" => {
                assert!(value.is_u64(), "{name}");
                assert_eq!(field["minimum"], 0, "{name}");
            }
            "boolean" => assert!(value.is_boolean(), "{name}"),
            other => panic!("unexpected field type: {name}: {other}"),
        }
    }
    assert_eq!(properties["id"]["format"], "uuid");
    for name in ["created_at", "updated_at", "last_opened_at"] {
        assert_eq!(properties[name]["format"], "date-time");
    }
    assert!(!properties.contains_key("intruder_count"));
}

mod history_fixture {
    use super::{assert_schema, labeled_summary, output_schema, summary};
    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{Request, StatusCode},
        response::{IntoResponse, Response},
        routing::any,
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use uuid::Uuid;

    #[derive(Debug)]
    struct RecordedRequest {
        method: String,
        path: String,
        query: BTreeMap<String, String>,
        body_len: usize,
        catalog_header: Option<String>,
    }

    struct MockState {
        settings: Value,
        settings_status: Mutex<u16>,
        settings_override: Mutex<Option<Value>>,
        payload: Mutex<Value>,
        read_status: Mutex<u16>,
        requests: Mutex<Vec<RecordedRequest>>,
    }

    struct MockApi {
        api: String,
        unused_dir: PathBuf,
        state: Arc<MockState>,
        server: tokio::task::JoinHandle<()>,
    }

    impl MockApi {
        async fn new() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let unused_dir =
                std::env::temp_dir().join(format!("sniper-history-schema-{}", Uuid::new_v4()));
            let state = Arc::new(MockState {
                settings: json!({
                    "runtime_instance_id":Uuid::new_v4(),
                    "proxy_addr":"127.0.0.1:0", "ui_addr":addr.to_string(),
                    "data_dir":unused_dir, "max_entries":2,
                    "features":["http_capture","session_storage","replay"]
                }),
                settings_status: Mutex::new(200),
                settings_override: Mutex::new(None),
                payload: Mutex::new(json!([])),
                read_status: Mutex::new(200),
                requests: Mutex::new(Vec::new()),
            });
            let app = Router::new()
                .fallback(any(mock_request))
                .with_state(state.clone());
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                api: format!("http://{addr}"),
                unused_dir,
                state,
                server,
            }
        }

        async fn command(&self, args: &[&str]) -> Value {
            self.command_with_code(args, 0).await
        }

        async fn command_with_code(&self, args: &[&str], expected_code: i32) -> Value {
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
            command
                .args(["--output", "compact", "--api", &self.api])
                .args(args)
                .env("SNIPER_DATA_DIR", &self.unused_dir)
                .env("HOME", self.unused_dir.join("home"))
                .env("USERPROFILE", self.unused_dir.join("home"))
                .env_remove("SNIPER_API_ADDR")
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(20), command.output())
                .await
                .expect("synthetic CLI fixture timed out")
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(expected_code),
                "{args:?}: {output:?}"
            );
            assert_service_map_stderr(&output, expected_code);
            assert!(
                !self.unused_dir.exists(),
                "read-only fixture created a data directory"
            );
            serde_json::from_slice(&output.stdout).unwrap()
        }

        fn assert_reads(&self, path: &str, expected_query: &[(&str, &str)]) {
            let catalog = matches!(
                path,
                "/api/target/service-map" | "/api/target/service-map/evidence"
            )
            .then_some("2");
            self.assert_reads_with_catalog(path, expected_query, catalog);
        }

        fn assert_reads_with_catalog(
            &self,
            path: &str,
            expected_query: &[(&str, &str)],
            catalog: Option<&str>,
        ) {
            let requests = std::mem::take(&mut *self.state.requests.lock().unwrap());
            assert_eq!(requests.len(), 2, "{requests:?}");
            assert_eq!(requests[0].path, "/api/settings", "{requests:?}");
            assert!(requests[0].query.is_empty(), "{requests:?}");
            assert_eq!(requests[0].catalog_header, None, "{requests:?}");
            assert_eq!(
                requests[1].catalog_header.as_deref(),
                catalog,
                "{requests:?}"
            );
            assert_eq!(requests[1].path, path, "{requests:?}");
            let mut expected =
                BTreeMap::from([("session_id".to_string(), Uuid::nil().to_string())]);
            expected.extend(
                expected_query
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string())),
            );
            assert_eq!(requests[1].query, expected, "{requests:?}");
            for request in requests {
                assert_eq!(request.method, "GET", "{request:?}");
                assert_eq!(request.body_len, 0, "{request:?}");
            }
        }
    }

    fn assert_service_map_stderr(output: &std::process::Output, expected_code: i32) {
        let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
        let data = payload.get("data").unwrap_or(&payload);
        let legacy = expected_code == 0
            && data.get("technology_catalog_version").is_none()
            && (data.get("snapshot").is_some() || data["source"] == "captured_http_metadata");
        let expected = if legacy {
            "Warning: Service Map API returned legacy technology catalog v1; technology coverage is limited to v1.\n"
        } else {
            ""
        };
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected,
            "{output:?}"
        );
    }

    impl Drop for MockApi {
        fn drop(&mut self) {
            self.server.abort();
        }
    }

    async fn mock_request(State(state): State<Arc<MockState>>, request: Request<Body>) -> Response {
        let (parts, body) = request.into_parts();
        let body = to_bytes(body, 1024).await.unwrap();
        state.requests.lock().unwrap().push(RecordedRequest {
            method: parts.method.to_string(),
            path: parts.uri.path().into(),
            query: url::form_urlencoded::parse(parts.uri.query().unwrap_or_default().as_bytes())
                .into_owned()
                .collect(),
            body_len: body.len(),
            catalog_header: parts
                .headers
                .get("x-sniper-technology-catalog-version")
                .map(|value| value.to_str().unwrap().to_owned()),
        });
        if parts.method != "GET" {
            return StatusCode::METHOD_NOT_ALLOWED.into_response();
        }
        match parts.uri.path() {
            "/api/settings" => {
                let status = StatusCode::from_u16(*state.settings_status.lock().unwrap()).unwrap();
                let payload = state.settings_override.lock().unwrap().clone().unwrap_or_else(||state.settings.clone());
                let mut response = (status,Json(payload)).into_response();
                if status.is_redirection() {
                    response.headers_mut().insert("location","/unexpected-discovery-redirect".parse().unwrap());
                }
                response
            },
            "/api/sessions" => Json(json!([{
                "id":Uuid::nil(),"name":"Synthetic session",
                "created_at":"2026-10-12T06:00:00Z","updated_at":"2026-10-12T06:00:00Z",
                "last_opened_at":"2026-10-12T06:00:00Z","request_count":2,"websocket_count":0,
                "event_count":0,"fuzzer_count":0,"rule_count":0,"storage_path":"synthetic-session","active":true
            }])).into_response(),
            "/api/transactions" | "/api/transactions-page" => {
                Json(state.payload.lock().unwrap().clone()).into_response()
            }
            "/api/target/service-map" | "/api/target/service-map/evidence" | "/api/target/service-map/body-references" | "/api/target/service-map/html-generators" => {
                let status = StatusCode::from_u16(*state.read_status.lock().unwrap()).unwrap();
                let mut response =
                    (status, Json(state.payload.lock().unwrap().clone())).into_response();
                if status.is_redirection() {
                    response
                        .headers_mut()
                        .insert("location", "/unexpected-redirect".parse().unwrap());
                }
                response
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        }
    }

    fn response_variants(rows: &Value) -> [(Value, Value); 3] {
        let unknown_metadata = json!({"total":null,"filtered_total":null,"hidden_connect_total":null,"offset":null,"limit":null,"has_more":null});
        let metadata = json!({"total":12,"filtered_total":null,"hidden_connect_total":1,"offset":5,"limit":2,"has_more":false});
        let mut complete_page = metadata.clone();
        complete_page["items"] = rows.clone();
        complete_page["ignored_upstream_field"] = json!(true);
        [
            (rows.clone(), unknown_metadata.clone()),
            (
                json!({"items":rows,"total":null,"ignored_upstream_field":true}),
                unknown_metadata,
            ),
            (complete_page, metadata),
        ]
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn history_read_modes_preserve_output_and_match_schema() {
        let schema = output_schema("capture.http.list");
        let mock = MockApi::new().await;
        let mut legacy_row = serde_json::to_value(summary(false)).unwrap();
        for field in ["sequence", "has_match_replace", "has_user_note"] {
            legacy_row.as_object_mut().unwrap().remove(field);
        }
        legacy_row["ignored_upstream_field"] = json!(true);
        legacy_row["label"] = json!("upstream label is not used");
        let mut legacy_output = labeled_summary(false);
        legacy_output["sequence"] = json!(0);
        legacy_output["label"] = json!("#0 GET example.com/fixture");
        let mut tunnel_row = serde_json::to_value(summary(true)).unwrap();
        tunnel_row["kind"] = json!("tunnel");
        tunnel_row["id"] = json!(Uuid::from_u128(2));
        let mut tunnel_output = tunnel_row.clone();
        tunnel_output["label"] = json!("#42 GET example.com/fixture");
        let rows = json!([legacy_row, tunnel_row]);
        let expected_rows = json!([legacy_output, tunnel_output]);
        let session_id = Uuid::nil().to_string();
        let cases = [
            (json!({}), vec![], "/api/transactions", vec![]),
            (json!({"page":false}), vec![], "/api/transactions", vec![]),
            (
                json!({"page":true,"offset":5,"limit":2}),
                vec!["--page", "--offset", "5", "--limit", "2"],
                "/api/transactions-page",
                vec![("offset", "5"), ("limit", "2")],
            ),
            (
                json!({"offset":5}),
                vec!["--offset", "5"],
                "/api/transactions-page",
                vec![("offset", "5")],
            ),
            (
                json!({"sort_key":"host","sort_direction":"asc"}),
                vec!["--sort-key", "host", "--sort-direction", "asc"],
                "/api/transactions-page",
                vec![("sort_key", "host"), ("sort_direction", "asc")],
            ),
            (
                json!({"sort_direction":"desc"}),
                vec!["--sort-direction", "desc"],
                "/api/transactions-page",
                vec![("sort_direction", "desc")],
            ),
            (
                json!({"before_sequence":42}),
                vec!["--before-sequence", "42"],
                "/api/transactions-page",
                vec![
                    ("before_sequence", "42"),
                    ("sort_key", "index"),
                    ("sort_direction", "desc"),
                ],
            ),
        ];
        for (payload, metadata) in response_variants(&rows) {
            *mock.state.payload.lock().unwrap() = payload;
            for (input, flags, path, query) in &cases {
                let expected = if input["page"] == true {
                    let mut expected = metadata.clone();
                    expected["items"] = expected_rows.clone();
                    expected
                } else {
                    expected_rows.clone()
                };
                let mut args = vec!["capture", "http", "list", "--session-id", &session_id];
                args.extend(flags.iter().copied());
                let direct = mock.command(&args).await;
                assert_eq!(direct, expected);
                assert_schema(&direct, &schema);
                mock.assert_reads(path, query);

                let mut input = input.clone();
                input["session_id"] = json!(session_id);
                let input = input.to_string();
                let called = mock
                    .command(&["call", "capture.http.list", "--input", &input])
                    .await;
                assert_eq!(called["ok"], true);
                assert_eq!(called["operation"], "capture.http.list");
                assert_eq!(called["data"], direct);
                assert_schema(&called["data"], &schema);
                assert!(!super::matches_schema(&called, &schema));
                mock.assert_reads(path, query);
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_history_read_variants_match_schema() {
        let schema = output_schema("capture.http.list");
        let mock = MockApi::new().await;
        for (payload, metadata) in response_variants(&json!([])) {
            *mock.state.payload.lock().unwrap() = payload;
            for page in [false, true] {
                let input = json!({"session_id":Uuid::nil(),"page":page}).to_string();
                let called = mock
                    .command(&["call", "capture.http.list", "--input", &input])
                    .await;
                let expected = if page {
                    let mut expected = metadata.clone();
                    expected["items"] = json!([]);
                    expected
                } else {
                    json!([])
                };
                assert_eq!(called["data"], expected);
                assert_schema(&called["data"], &schema);
                mock.assert_reads(
                    if page {
                        "/api/transactions-page"
                    } else {
                        "/api/transactions"
                    },
                    &[],
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_pins_the_active_session_once_without_switching() {
        let mock = MockApi::new().await;
        *mock.state.payload.lock().unwrap() = map_payload();
        let result = mock.command(&["call", "target.service_map.get"]).await;
        assert_eq!(result["meta"]["session_id"], Uuid::nil().to_string());
        let requests = std::mem::take(&mut *mock.state.requests.lock().unwrap());
        assert_eq!(requests.len(), 3, "{requests:?}");
        assert_eq!(requests[0].path, "/api/settings");
        assert_eq!(requests[1].path, "/api/sessions");
        assert_eq!(requests[2].path, "/api/target/service-map");
        assert_eq!(requests[0].catalog_header, None);
        assert_eq!(requests[1].catalog_header, None);
        assert_eq!(requests[2].catalog_header.as_deref(), Some("2"));
        assert_eq!(requests[2].query["session_id"], Uuid::nil().to_string());
        assert!(requests
            .iter()
            .all(|request| request.method == "GET" && request.body_len == 0));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_empty_and_truncated_results_keep_explicit_completeness() {
        let mock = MockApi::new().await;
        let schema = output_schema("target.service_map.get");
        let mut empty = map_payload();
        empty["origins"] = json!([]);
        for key in [
            "retained_rows",
            "considered_rows",
            "non_http_rows",
            "projected_rows",
            "included_rows",
            "excluded_rows",
            "omitted_rows",
            "origin_count",
            "endpoint_count",
        ] {
            empty["snapshot"][key] = json!(0);
        }
        for key in [
            "first_retained_started_at",
            "last_retained_started_at",
            "oldest_considered_at",
            "latest_considered_at",
        ] {
            empty["snapshot"][key] = Value::Null;
        }
        let mut truncated = map_payload();
        truncated["snapshot"]["complete"] = json!(false);
        truncated["snapshot"]["stopped_by"] = json!(["metadata_bytes_limit"]);
        truncated["snapshot"]["retained_rows"] = json!(3);
        for payload in [empty, truncated] {
            *mock.state.payload.lock().unwrap() = payload.clone();
            let session = Uuid::nil().to_string();
            let result = mock
                .command(&["scope", "service-map", "--session-id", &session])
                .await;
            assert_eq!(result, payload);
            assert_schema(&result, &schema);
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_discovery_never_follows_redirects_or_retries() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for (status, code, exit) in [(302, "REDIRECT_REFUSED", 5), (503, "HTTP_STATUS_ERROR", 6)] {
            *mock.state.settings_status.lock().unwrap() = status;
            *mock.state.settings_override.lock().unwrap() =
                Some(json!({"error":"SYNTHETIC_PRIVATE_VALUE"}));
            let result = mock
                .command_with_code(&["scope", "service-map", "--session-id", &session], exit)
                .await;
            assert_eq!(result["error"]["code"], code);
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            let requests = std::mem::take(&mut *mock.state.requests.lock().unwrap());
            assert_eq!(requests.len(), 1, "{requests:?}");
            assert_eq!(requests[0].path, "/api/settings");
            assert_eq!(requests[0].method, "GET");
            assert_eq!(requests[0].body_len, 0);
        }
        *mock.state.settings_status.lock().unwrap() = 200;
        let result = mock
            .command_with_code(&["scope", "service-map", "--session-id", &session], 5)
            .await;
        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        let requests = std::mem::take(&mut *mock.state.requests.lock().unwrap());
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert_eq!(requests[0].path, "/api/settings");
    }

    async fn service_map_discovery_command(
        root: &std::path::Path,
        args: &[&str],
        env_api: Option<&str>,
        expected_code: i32,
    ) -> Value {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        command
            .args(["--output", "compact"])
            .args(args)
            .env("SNIPER_DATA_DIR", root)
            .env("HOME", root.join("home"))
            .env("USERPROFILE", root.join("home"))
            .env_remove("SNIPER_API_ADDR")
            .kill_on_drop(true);
        if let Some(env_api) = env_api {
            command.env("SNIPER_API_ADDR", env_api);
        }
        let output = tokio::time::timeout(Duration::from_secs(20), command.output())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected_code), "{output:?}");
        assert_service_map_stderr(&output, expected_code);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_discovery_hides_invalid_api_values_without_fallback() {
        let mock = MockApi::new().await;
        let invalid = "http://[SYNTHETIC_PRIVATE_VALUE";
        let result = service_map_discovery_command(
            &mock.unused_dir,
            &["--api", invalid, "scope", "service-map"],
            None,
            2,
        )
        .await;
        assert_eq!(result["error"]["code"], "INVALID_INPUT");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        let result = service_map_discovery_command(
            &mock.unused_dir,
            &["scope", "service-map"],
            Some(invalid),
            2,
        )
        .await;
        assert_eq!(result["error"]["code"], "INVALID_INPUT");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        assert!(mock.state.requests.lock().unwrap().is_empty());
        assert!(!mock.unused_dir.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_discovery_preserves_runtime_files_and_checks_identity() {
        use sniper::runtime_state::{runtime_state_path, RuntimeStateSnapshot};
        let mock = MockApi::new().await;
        let root =
            std::env::temp_dir().join(format!("sniper-service-map-discovery-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = runtime_state_path(&root);
        let corrupt = b"{SYNTHETIC_PRIVATE_VALUE";
        std::fs::write(&path, corrupt).unwrap();
        let result = service_map_discovery_command(&root, &["scope", "service-map"], None, 6).await;
        assert_eq!(result["error"]["code"], "API_UNAVAILABLE");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        assert!(mock.state.requests.lock().unwrap().is_empty());

        let addr = mock.api.strip_prefix("http://").unwrap().parse().unwrap();
        let snapshot =
            RuntimeStateSnapshot::with_proxy_status("127.0.0.1:0".parse().unwrap(), addr, true);
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        for status in [302, 200] {
            *mock.state.settings_status.lock().unwrap() = status;
            let result =
                service_map_discovery_command(&root, &["scope", "service-map"], None, 5).await;
            assert_eq!(
                result["error"]["code"],
                if status == 302 {
                    "REDIRECT_REFUSED"
                } else {
                    "INVALID_RESPONSE"
                }
            );
            assert_eq!(result["error"]["retryable"], false);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
            let requests = std::mem::take(&mut *mock.state.requests.lock().unwrap());
            assert_eq!(requests.len(), 1, "{requests:?}");
            assert_eq!(requests[0].path, "/api/settings");
        }
        // The same identity checks must also permit the intended instance.
        let mut settings = mock.state.settings.clone();
        settings["runtime_instance_id"] = json!(snapshot.instance_id);
        settings["data_dir"] = json!(root);
        *mock.state.settings_override.lock().unwrap() = Some(settings);
        *mock.state.payload.lock().unwrap() = map_payload();
        let session = Uuid::nil().to_string();
        let result = service_map_discovery_command(
            &root,
            &["scope", "service-map", "--session-id", &session],
            None,
            0,
        )
        .await;
        assert_eq!(result, map_payload());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn map_payload() -> Value {
        let mut limits = serde_json::Map::new();
        for field in [
            "metadata_bytes",
            "origins",
            "endpoints",
            "endpoints_per_origin",
            "evidence_per_item",
            "redirects_per_origin",
            "header_links_per_origin",
            "technology_clues_per_row",
            "statuses_per_endpoint",
            "content_types_per_endpoint",
            "path_bytes",
            "host_bytes",
            "method_bytes",
            "headers_per_row",
            "header_bytes_per_row",
            "header_value_bytes",
        ] {
            limits.insert(field.into(), json!(100));
        }
        limits.insert("max_records".into(), json!(10000));
        let stamp = "2026-10-12T06:00:00Z";
        json!({
            "schema_version":1,"session_id":Uuid::nil(),"in_scope_only":false,"origin_filter":null,
            "snapshot":{
                "id":Uuid::nil(),"generation":Uuid::nil(),"captured_at":stamp,
                "first_retained_started_at":stamp,"last_retained_started_at":stamp,
                "window_order":"newest_capture_sequence",
                "oldest_considered_at":stamp,"latest_considered_at":stamp,
                "retained_rows":2,"considered_rows":2,"non_http_rows":0,"projected_rows":2,
                "included_rows":2,"excluded_rows":0,"omitted_rows":0,
                "origin_count":1,"endpoint_count":1,"complete":true,"stopped_by":[],"limits":limits
            },
            "origins":[{
                "origin":"https://example.com","host":"example.com","scheme":"https","port":443,
                "in_scope":true,"request_count":2,"first_seen":stamp,"last_seen":stamp,
                "endpoints":[{"method":"get","path":"/items/42","path_group":"/items/{number}",
                    "request_count":2,"statuses":[200,302],"content_types":["text/html"],"evidence":["v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000"]}],
                "technology_clues":[{"name":"nginx","header_name":"server","source":"response_header","confidence":"observed_header","evidence":["v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000"]}],
                "redirects":[{"target_origin":"https://login.example.com","target_path":"/sign-in","relationship":"redirect_instruction","source":"response_location_header","evidence":["v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000"]}],
                "header_links":[{"source_origin":"https://app.example.com","relationship":"referer_header","source":"request_header_declaration","evidence":["v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000"]}]
            }],
            "unavailable":["initiator","remote_ip","tls_certificate"],"deferred":["response_body_references"],"warnings":[]
        })
    }

    fn evidence_payload(available: bool) -> Value {
        let row = json!({
            "evidence_id":"v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000","record_id":Uuid::nil(),"sequence":"9007199254740993",
            "started_at":"2026-10-12T06:00:00Z","method":"get","origin":"https://example.com",
            "host":"example.com","scheme":"https","port":443,"path":"/items/42",
            "status":null,"content_type":null,"is_websocket":false,
            "technology_clues":[],"redirect":null,"header_links":[],"metadata_complete":true,"omitted":[]
        });
        json!({"schema_version":1,"session_id":Uuid::nil(),"evidence_id":"v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000",
            "available":available,"reason":if available {Value::Null} else {json!("stale_generation")},
            "source":"captured_http_metadata","row":if available {row} else {Value::Null}})
    }

    fn technology_observation_payload() -> Value {
        json!({
            "product_id":"nginx","name":"nginx","catalog_category":"web_server",
            "reported_version":"1.24.0","version_status":"reported",
            "source":"response_header","strength":"literal_declaration","header_name":"server",
            "header_index":0,"product_span":{"start":0,"end":5},
            "version_span":{"start":6,"end":12},
            "offset_basis":"retained_header_value_utf8","rule_id":"server.nginx.v1"
        })
    }

    fn technology_evidence_payload() -> Value {
        let mut payload = evidence_payload(true);
        payload["row"]["technology_observations"] = json!([technology_observation_payload()]);
        payload
    }

    fn technology_map_payload() -> Value {
        let mut payload = map_payload();
        payload["snapshot"]["generation"] = json!("22222222-2222-2222-2222-222222222222");
        for (name, cap) in [
            ("technology_observations_per_row", 8),
            ("technology_observations_per_origin", 32),
            ("technology_observations", 2000),
            ("technology_version_bytes", 32),
            ("technology_token_candidates_per_row", 128),
        ] {
            payload["snapshot"]["limits"][name] = json!(cap);
        }
        payload["origins"][0]["technology_observations"] = json!([{
            "product_id":"nginx","name":"nginx","catalog_category":"web_server",
            "reported_version":"1.24.0","version_status":"reported",
            "source":"response_header","strength":"literal_declaration","header_name":"server",
            "row_count":1,"first_seen":"2026-10-12T06:00:00Z","last_seen":"2026-10-12T06:00:00Z",
            "evidence":[BODY_REFERENCE_TOKEN]
        }]);
        payload
    }

    fn catalog_test_input(exact_row: bool, version: Option<u32>) -> Value {
        let mut input = json!({"session_id":Uuid::nil()});
        if exact_row {
            input["evidence_id"] = json!(BODY_REFERENCE_TOKEN);
        }
        if let Some(version) = version {
            input["technology_catalog_version"] = json!(version);
        }
        input
    }

    fn catalog_operation(exact_row: bool) -> &'static str {
        if exact_row {
            "target.service_map.evidence.get"
        } else {
            "target.service_map.get"
        }
    }

    fn assert_catalog_read(mock: &MockApi, exact_row: bool, version: &str) {
        let (path, query) = if exact_row {
            (
                "/api/target/service-map/evidence",
                vec![("evidence_id", BODY_REFERENCE_TOKEN)],
            )
        } else {
            (
                "/api/target/service-map",
                vec![("in_scope_only", "false"), ("max_records", "10000")],
            )
        };
        mock.assert_reads_with_catalog(path, &query, Some(version));
    }

    fn catalog_v2_payload(
        exact_row: bool,
        entry: &sniper::service_map_technology::CatalogEntry,
    ) -> Value {
        let mut payload = if exact_row {
            technology_evidence_payload()
        } else {
            technology_map_payload()
        };
        payload["technology_catalog_version"] = json!(2);
        let observation = if exact_row {
            &mut payload["row"]["technology_observations"][0]
        } else {
            &mut payload["origins"][0]["technology_observations"][0]
        };
        observation["product_id"] = json!(entry.product_id);
        observation["name"] = json!(entry.name);
        observation["catalog_category"] = json!(entry.catalog_category);
        observation["header_name"] = json!(entry.header_name);
        if exact_row {
            observation["rule_id"] = json!(entry.rule_id);
            observation["product_span"] = json!({"start":0,"end":entry.literal.len()});
            observation["version_span"] =
                json!({"start":entry.literal.len()+1,"end":entry.literal.len()+7});
        }
        payload
    }

    #[test]
    fn service_map_catalog_discovery_and_dry_run_are_header_only() {
        let session = Uuid::nil().to_string();
        for exact_row in [false, true] {
            let operation = catalog_operation(exact_row);
            let spec = super::spec(operation);
            let schema = super::cli(&["schema", "input", operation], 0)["schema"].clone();
            assert_eq!(spec["input_schema"], schema);
            assert_eq!(
                schema["properties"]["technology_catalog_version"]["type"],
                "integer"
            );
            assert_eq!(
                schema["properties"]["technology_catalog_version"]["enum"],
                json!([1, 2])
            );
            assert_eq!(
                schema["properties"]["technology_catalog_version"]["default"],
                2
            );
            assert!(spec["command"]
                .as_str()
                .unwrap()
                .contains("--technology-catalog-version"));
            for version in [None, Some(1), Some(2)] {
                let requested = version.unwrap_or(2).to_string();
                let input = catalog_test_input(exact_row, version).to_string();
                let called = super::cli(&["--dry-run", "call", operation, "--input", &input], 0);
                let mut args = vec![
                    "--dry-run",
                    "scope",
                    if exact_row {
                        "service-map-evidence"
                    } else {
                        "service-map"
                    },
                    "--session-id",
                    &session,
                ];
                if exact_row {
                    args.extend(["--evidence-id", BODY_REFERENCE_TOKEN]);
                }
                if version.is_some() {
                    args.extend(["--technology-catalog-version", &requested]);
                }
                let direct = super::cli(&args, 0);
                assert_eq!(called["data"], direct);
                assert_eq!(
                    direct["input"]["technology_catalog_version"],
                    version.unwrap_or(2)
                );
                assert_eq!(direct["api"]["method"], "GET");
                assert_eq!(
                    direct["api"]["headers"],
                    json!({"X-Sniper-Technology-Catalog-Version":requested})
                );
                assert_eq!(direct["api"]["body"], Value::Null);
                let path = direct["api"]["path"].as_str().unwrap();
                assert!(
                    !path.contains("technology_catalog") && !path.contains("technology-catalog")
                );
            }
            for input in spec["examples"].as_array().unwrap() {
                let example = super::cli(
                    &[
                        "--dry-run",
                        "call",
                        operation,
                        "--input",
                        &input.to_string(),
                    ],
                    0,
                );
                assert_eq!(
                    example["data"]["api"]["headers"]["X-Sniper-Technology-Catalog-Version"],
                    input
                        .get("technology_catalog_version")
                        .and_then(Value::as_u64)
                        .unwrap_or(2)
                        .to_string()
                );
            }
            for invalid in [
                Value::Null,
                json!(0),
                json!(3),
                json!(-1),
                json!(2.0),
                json!("2"),
                json!(true),
                json!({}),
            ] {
                let mut input = catalog_test_input(exact_row, None);
                input["technology_catalog_version"] = invalid;
                let error = super::cli(
                    &[
                        "--dry-run",
                        "call",
                        operation,
                        "--input",
                        &input.to_string(),
                    ],
                    2,
                );
                assert_eq!(error["error"]["code"], "INVALID_INPUT");
            }
        }
        let operation = "target.service_map.body_references.get";
        let spec = super::spec(operation);
        assert!(spec["input_schema"]["properties"]
            .get("technology_catalog_version")
            .is_none());
        assert!(spec["output_schema"]["properties"]
            .get("technology_catalog_version")
            .is_none());
        let input = json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN});
        let preview = super::cli(
            &[
                "--dry-run",
                "call",
                operation,
                "--input",
                &input.to_string(),
            ],
            0,
        );
        assert!(preview["data"]["api"].get("headers").is_none());
        assert!(preview["data"]["input"]
            .get("technology_catalog_version")
            .is_none());
        let mut rejected = input;
        rejected["technology_catalog_version"] = json!(2);
        let error = super::cli(
            &[
                "--dry-run",
                "call",
                operation,
                "--input",
                &rejected.to_string(),
            ],
            2,
        );
        assert_eq!(error["error"]["code"], "INVALID_INPUT");
        let error = super::cli(
            &[
                "--dry-run",
                "scope",
                "service-map",
                "--technology-catalog-version",
                "2",
                "body-references",
                "--session-id",
                &session,
                "--evidence-id",
                BODY_REFERENCE_TOKEN,
            ],
            2,
        );
        assert_eq!(error["error"]["code"], "INVALID_INPUT");
        let other = super::cli(&["--dry-run", "scope", "get-scope"], 0);
        assert!(other["api"].get("headers").is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_catalog_marker_negotiation_preserves_legacy_without_retry() {
        let mock = MockApi::new().await;
        for exact_row in [false, true] {
            let operation = catalog_operation(exact_row);
            let schema = output_schema(operation);
            for requested in [1, 2] {
                for marker in [
                    None,
                    Some(json!(1)),
                    Some(json!(2)),
                    Some(json!(0)),
                    Some(json!(3)),
                    Some(Value::Null),
                    Some(json!("SYNTHETIC_PRIVATE_VALUE")),
                    Some(json!(2.0)),
                    Some(json!(-1)),
                    Some(json!(true)),
                ] {
                    let mut payload = if exact_row {
                        technology_evidence_payload()
                    } else {
                        technology_map_payload()
                    };
                    if let Some(marker) = &marker {
                        payload["technology_catalog_version"] = marker.clone();
                    }
                    let valid = marker
                        .as_ref()
                        .is_none_or(|marker| marker.as_u64() == Some(requested as u64));
                    *mock.state.payload.lock().unwrap() = payload.clone();
                    let input = catalog_test_input(exact_row, Some(requested)).to_string();
                    let result = mock
                        .command_with_code(
                            &["call", operation, "--input", &input],
                            if valid { 0 } else { 5 },
                        )
                        .await;
                    if valid {
                        assert_eq!(result["data"], payload);
                        assert_schema(&result["data"], &schema);
                    } else {
                        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
                        assert_eq!(result["error"]["retryable"], false);
                        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
                    }
                    assert_catalog_read(&mock, exact_row, &requested.to_string());
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_catalog_v2_products_require_explicit_v2_response_marker() {
        use sniper::service_map_technology::CATALOG_V2_ADDITIONS;
        let mock = MockApi::new().await;
        assert_eq!(
            CATALOG_V2_ADDITIONS
                .iter()
                .map(|entry| entry.product_id)
                .collect::<Vec<_>>(),
            [
                "gunicorn",
                "uvicorn",
                "waitress",
                "openresty",
                "litespeed",
                "next_js"
            ]
        );
        for exact_row in [false, true] {
            let operation = catalog_operation(exact_row);
            let schema = output_schema(operation);
            for entry in CATALOG_V2_ADDITIONS {
                let payload = catalog_v2_payload(exact_row, entry);
                *mock.state.payload.lock().unwrap() = payload.clone();
                let input = catalog_test_input(exact_row, None).to_string();
                let result = mock.command(&["call", operation, "--input", &input]).await;
                assert_eq!(result["data"], payload);
                assert_schema(&payload, &schema);
                assert_catalog_read(&mock, exact_row, "2");
                for marker in [None, Some(1)] {
                    let mut invalid = payload.clone();
                    invalid
                        .as_object_mut()
                        .unwrap()
                        .remove("technology_catalog_version");
                    if let Some(marker) = marker {
                        invalid["technology_catalog_version"] = json!(marker);
                    }
                    assert!(!super::matches_schema(&invalid, &schema));
                    *mock.state.payload.lock().unwrap() = invalid;
                    let input = catalog_test_input(exact_row, marker).to_string();
                    let result = mock
                        .command_with_code(&["call", operation, "--input", &input], 5)
                        .await;
                    assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
                    assert_catalog_read(&mock, exact_row, if marker.is_some() { "1" } else { "2" });
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_catalog_v2_keeps_identity_bounds_and_error_guards() {
        use sniper::service_map_technology::CATALOG_V2_ADDITIONS;
        let mock = MockApi::new().await;
        for exact_row in [false, true] {
            let operation = catalog_operation(exact_row);
            let base = catalog_v2_payload(exact_row, &CATALOG_V2_ADDITIONS[0]);
            let input = catalog_test_input(exact_row, None).to_string();
            let mut invalids = Vec::new();
            for (field, value) in [
                ("name", json!("nginx")),
                ("catalog_category", json!("web_server")),
                ("header_name", json!("x-powered-by")),
                ("reported_version", json!("1.2beta")),
            ] {
                let mut invalid = base.clone();
                let observation = if exact_row {
                    &mut invalid["row"]["technology_observations"][0]
                } else {
                    &mut invalid["origins"][0]["technology_observations"][0]
                };
                observation[field] = value;
                invalids.push(invalid);
            }
            let mut over_cap = base.clone();
            if exact_row {
                over_cap["row"]["technology_observations"] =
                    json!(vec![base["row"]["technology_observations"][0].clone(); 9]);
                let mut wrong_rule = base.clone();
                wrong_rule["row"]["technology_observations"][0]["rule_id"] =
                    json!("server.gunicorn.v2");
                invalids.push(wrong_rule);
            } else {
                over_cap["origins"][0]["technology_observations"] = json!(vec![
                        base["origins"][0]["technology_observations"][0].clone();
                        33
                    ]);
                let mut wrong_identity = base.clone();
                wrong_identity["origins"][0]["technology_observations"][0]["evidence"] =
                    json!([BODY_REFERENCE_TOKEN.replace("22222222-2222", "11111111-2222")]);
                invalids.push(wrong_identity);
            }
            invalids.push(over_cap);
            for invalid in invalids {
                *mock.state.payload.lock().unwrap() = invalid;
                let result = mock
                    .command_with_code(&["call", operation, "--input", &input], 5)
                    .await;
                assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
                assert_eq!(result["error"]["retryable"], false);
                assert_catalog_read(&mock, exact_row, "2");
            }
            *mock.state.payload.lock().unwrap() = json!({"error":"SYNTHETIC_PRIVATE_VALUE"});
            for (status, exit) in [(400, 5), (503, 6)] {
                *mock.state.read_status.lock().unwrap() = status;
                let result = mock
                    .command_with_code(&["call", operation, "--input", &input], exit)
                    .await;
                assert_eq!(result["error"]["code"], "HTTP_STATUS_ERROR");
                assert_eq!(result["error"]["retryable"], false);
                assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
                assert_catalog_read(&mock, exact_row, "2");
            }
            *mock.state.read_status.lock().unwrap() = 200;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_extensions_preserve_old_servers_and_both_envelopes() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for exact_row in [false, true] {
            let operation = if exact_row {
                "target.service_map.evidence.get"
            } else {
                "target.service_map.get"
            };
            let path = if exact_row {
                "/api/target/service-map/evidence"
            } else {
                "/api/target/service-map"
            };
            let schema = output_schema(operation);
            let mut empty = if exact_row {
                evidence_payload(true)
            } else {
                map_payload()
            };
            let pointer = if exact_row {
                "/row/technology_observations"
            } else {
                "/origins/0/technology_observations"
            };
            if exact_row {
                empty["row"]["technology_observations"] = json!([]);
            } else {
                empty["origins"][0]["technology_observations"] = json!([]);
            }
            let payloads = if exact_row {
                vec![evidence_payload(true), empty, technology_evidence_payload()]
            } else {
                vec![map_payload(), empty, technology_map_payload()]
            };
            for payload in payloads {
                *mock.state.payload.lock().unwrap() = payload.clone();
                let args = if exact_row {
                    vec![
                        "scope",
                        "service-map-evidence",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        BODY_REFERENCE_TOKEN,
                    ]
                } else {
                    vec!["scope", "service-map", "--session-id", &session]
                };
                let direct = mock.command(&args).await;
                assert_eq!(direct, payload);
                assert_schema(&direct, &schema);
                assert_eq!(direct.pointer(pointer), payload.pointer(pointer));
                let query = if exact_row {
                    vec![("evidence_id", BODY_REFERENCE_TOKEN)]
                } else {
                    vec![("in_scope_only", "false"), ("max_records", "10000")]
                };
                mock.assert_reads(path, &query);
                let input = if exact_row {
                    json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN})
                } else {
                    json!({"session_id":session})
                }
                .to_string();
                let called = mock.command(&["call", operation, "--input", &input]).await;
                assert_eq!(called["data"], direct);
                assert_eq!(called["meta"]["session_id"], session);
                mock.assert_reads(path, &query);
            }
        }
    }

    #[test]
    fn service_map_technology_schemas_follow_the_fixed_catalog_and_nullable_contract() {
        let row_schema = output_schema("target.service_map.evidence.get");
        let schema = &row_schema["properties"]["row"]["anyOf"][1]["properties"]
            ["technology_observations"]["items"];
        for entry in sniper::service_map_technology::catalog_entries(
            sniper::service_map_technology::CatalogVersion::V2,
        ) {
            let mut observation = technology_observation_payload();
            observation["product_id"] = json!(entry.product_id);
            observation["name"] = json!(entry.name);
            observation["catalog_category"] = json!(entry.catalog_category);
            observation["header_name"] = json!(entry.header_name);
            observation["rule_id"] = json!(entry.rule_id);
            observation["product_span"]["end"] = json!(entry.literal.len());
            observation["version_span"] =
                json!({"start":entry.literal.len()+1,"end":entry.literal.len()+7});
            if entry.product_id == "apache_coyote" {
                observation["reported_version"] = Value::Null;
                observation["version_span"] = Value::Null;
                observation["version_status"] = json!("non_product_version");
            }
            assert_schema(&observation, schema);
            for status in ["not_reported", "unsupported_format"] {
                observation["reported_version"] = Value::Null;
                observation["version_span"] = Value::Null;
                observation["version_status"] = json!(status);
                assert_eq!(
                    super::matches_schema(&observation, schema),
                    entry.rule_id != "server.jetty_parenthesized.v1" || status != "not_reported"
                );
            }
        }
        for field in [
            "reported_version",
            "version_span",
            "rule_id",
            "header_index",
        ] {
            let mut missing = technology_observation_payload();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!super::matches_schema(&missing, schema), "{field}");
        }
        for version in [
            "",
            "1..2",
            "1.2.",
            "1.2beta",
            "１.２",
            "123456789012345678901234567890123",
        ] {
            let mut invalid = technology_observation_payload();
            invalid["reported_version"] = json!(version);
            assert!(!super::matches_schema(&invalid, schema), "{version}");
        }
        let mut invalid = technology_observation_payload();
        invalid["raw_header"] = json!("SYNTHETIC_PRIVATE_VALUE");
        assert!(!super::matches_schema(&invalid, schema));
        invalid = technology_observation_payload();
        invalid["version_span"] = Value::Null;
        assert!(!super::matches_schema(&invalid, schema));
        invalid = technology_observation_payload();
        invalid["product_id"] = json!("apache_http_server");
        assert!(!super::matches_schema(&invalid, schema));
        let map_schema = output_schema("target.service_map.get");
        for (name, _) in [
            ("technology_observations_per_row", 8),
            ("technology_observations_per_origin", 32),
            ("technology_observations", 2000),
            ("technology_version_bytes", 32),
            ("technology_token_candidates_per_row", 128),
        ] {
            let mut invalid = technology_map_payload();
            invalid["snapshot"]["limits"][name] = json!(999);
            assert!(!super::matches_schema(&invalid, &map_schema), "{name}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_rows_reject_malformed_or_private_extensions() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let base = technology_evidence_payload();
        let mut invalids = Vec::new();
        for (pointer, value) in [
            ("/product_id", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/name", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/catalog_category", json!("origin_server")),
            ("/catalog_category", json!("connector")),
            ("/source", json!("response_body")),
            ("/strength", json!("verified")),
            ("/header_name", json!("Server")),
            ("/header_name", json!("set-cookie")),
            ("/rule_id", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/rule_id", json!("server.php.v1")),
            ("/offset_basis", json!("unicode_characters")),
            ("/header_index", json!(128)),
            ("/header_index", json!(-1)),
            ("/product_span/start", json!(5)),
            ("/product_span/end", json!(4097)),
            ("/product_span/end", json!(4)),
            ("/version_span/start", json!(5)),
            ("/version_span/end", json!(4097)),
            ("/version_span/end", json!(11)),
            ("/version_span", Value::Null),
            ("/reported_version", Value::Null),
            ("/reported_version", json!("1.24.0-SYNTHETIC_PRIVATE_VALUE")),
            (
                "/reported_version",
                json!("123456789012345678901234567890123"),
            ),
            ("/version_status", json!("not_reported")),
            ("/version_status", json!("inferred")),
            ("/version_status", json!("non_product_version")),
        ] {
            let mut payload = base.clone();
            *payload["row"]["technology_observations"][0]
                .pointer_mut(pointer)
                .unwrap() = value;
            invalids.push(payload);
        }
        for field in ["reported_version", "version_span", "product_id", "rule_id"] {
            let mut payload = base.clone();
            payload["row"]["technology_observations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            invalids.push(payload);
        }
        let mut payload = base.clone();
        payload["row"]["technology_observations"][0]["raw_header"] =
            json!("SYNTHETIC_PRIVATE_VALUE");
        invalids.push(payload);
        let mut payload = base.clone();
        payload["row"]["technology_observations"][0]["product_span"]["raw"] =
            json!("SYNTHETIC_PRIVATE_VALUE");
        invalids.push(payload);
        for value in [
            Value::Null,
            json!({}),
            json!(vec![technology_observation_payload(); 9]),
        ] {
            let mut payload = base.clone();
            payload["row"]["technology_observations"] = value;
            invalids.push(payload);
        }
        let mut payload = base.clone();
        payload["row"]["technology_observations"] = json!([
            technology_observation_payload(),
            technology_observation_payload()
        ]);
        invalids.push(payload);
        let mut payload = base.clone();
        payload["row"]["metadata_complete"] = json!(false);
        payload["row"]["omitted"] = json!(["technology_observations_limit"]);
        invalids.push(payload);
        for (index, payload) in invalids.into_iter().enumerate() {
            *mock.state.payload.lock().unwrap() = payload;
            let input =
                json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN}).to_string();
            let args = if index % 2 == 0 {
                vec!["call", "target.service_map.evidence.get", "--input", &input]
            } else {
                vec![
                    "scope",
                    "service-map-evidence",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ]
            };
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "case {index}");
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_aggregates_reject_misbound_counts_limits_and_conflicts() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let base = technology_map_payload();
        let mut invalids = Vec::new();
        for (pointer, value) in [
            ("/product_id", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/name", json!("Apache")),
            ("/catalog_category", json!("connector")),
            ("/header_name", json!("x-generator")),
            ("/source", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/strength", json!("verified")),
            ("/reported_version", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/reported_version", Value::Null),
            ("/version_status", json!("unsupported_format")),
            ("/row_count", json!(0)),
            ("/row_count", json!(3)),
            ("/first_seen", json!("2026-10-12T05:59:59Z")),
            ("/last_seen", json!("2026-10-12T06:00:01Z")),
            ("/evidence", json!([])),
            (
                "/evidence",
                json!([BODY_REFERENCE_TOKEN, BODY_REFERENCE_TOKEN]),
            ),
            (
                "/evidence",
                json!([BODY_REFERENCE_TOKEN.replace("v1.00000000", "v1.11111111")]),
            ),
            (
                "/evidence",
                json!([BODY_REFERENCE_TOKEN.replace("22222222-2222", "11111111-2222")]),
            ),
        ] {
            let mut payload = base.clone();
            *payload["origins"][0]["technology_observations"][0]
                .pointer_mut(pointer)
                .unwrap() = value;
            invalids.push(payload);
        }
        for field in ["reported_version", "header_name", "row_count"] {
            let mut payload = base.clone();
            payload["origins"][0]["technology_observations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            invalids.push(payload);
        }
        for (field, value) in [
            ("rule_id", json!("server.nginx.v1")),
            ("product_span", json!({"start":0,"end":5})),
            ("raw_header", json!("SYNTHETIC_PRIVATE_VALUE")),
        ] {
            let mut payload = base.clone();
            payload["origins"][0]["technology_observations"][0][field] = value;
            invalids.push(payload);
        }
        for name in [
            "technology_observations_per_row",
            "technology_observations_per_origin",
            "technology_observations",
            "technology_version_bytes",
            "technology_token_candidates_per_row",
        ] {
            for value in [json!(999), Value::Null] {
                let mut payload = base.clone();
                payload["snapshot"]["limits"][name] = value;
                invalids.push(payload);
            }
        }
        for value in [
            Value::Null,
            json!({}),
            json!(vec![
                base["origins"][0]["technology_observations"][0].clone();
                33
            ]),
        ] {
            let mut payload = base.clone();
            payload["origins"][0]["technology_observations"] = value;
            invalids.push(payload);
        }
        let mut payload = base.clone();
        let duplicate = payload["origins"][0]["technology_observations"][0].clone();
        payload["origins"][0]["technology_observations"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        invalids.push(payload);
        for reason in [
            "technology_observations_per_origin_limit",
            "technology_observations_map_limit",
        ] {
            let mut payload = base.clone();
            payload["snapshot"]["complete"] = json!(false);
            payload["snapshot"]["stopped_by"] = json!([reason]);
            invalids.push(payload);
        }
        for (index, payload) in invalids.into_iter().enumerate() {
            *mock.state.payload.lock().unwrap() = payload;
            let input = json!({"session_id":session}).to_string();
            let args = if index % 2 == 0 {
                vec!["call", "target.service_map.get", "--input", &input]
            } else {
                vec!["scope", "service-map", "--session-id", &session]
            };
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "case {index}");
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_statuses_conflicting_versions_and_caps_remain_explicit() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.evidence.get");
        let mut cases = Vec::new();
        for status in ["not_reported", "unsupported_format"] {
            let mut payload = technology_evidence_payload();
            let observation = &mut payload["row"]["technology_observations"][0];
            observation["version_status"] = json!(status);
            observation["reported_version"] = Value::Null;
            observation["version_span"] = Value::Null;
            cases.push(payload);
        }
        let mut coyote = technology_evidence_payload();
        let observation = &mut coyote["row"]["technology_observations"][0];
        observation["product_id"] = json!("apache_coyote");
        observation["name"] = json!("Apache-Coyote");
        observation["catalog_category"] = json!("connector");
        observation["rule_id"] = json!("server.apache_coyote.v1");
        observation["product_span"]["end"] = json!(13);
        observation["version_status"] = json!("non_product_version");
        observation["reported_version"] = Value::Null;
        observation["version_span"] = Value::Null;
        cases.push(coyote);
        let mut long_version = technology_evidence_payload();
        long_version["row"]["technology_observations"][0]["reported_version"] =
            json!("12345678901234567890123456789012");
        long_version["row"]["technology_observations"][0]["version_span"]["end"] = json!(38);
        cases.push(long_version);
        let mut capped = technology_evidence_payload();
        capped["row"]["technology_observations"] = json!((0..8)
            .map(|index| {
                let mut observation = technology_observation_payload();
                observation["header_index"] = json!(index);
                observation
            })
            .collect::<Vec<_>>());
        capped["row"]["metadata_complete"] = json!(false);
        capped["row"]["omitted"] = json!(["technology_observations_limit"]);
        cases.push(capped);
        for payload in cases {
            *mock.state.payload.lock().unwrap() = payload.clone();
            let result = mock
                .command(&[
                    "scope",
                    "service-map-evidence",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ])
                .await;
            assert_eq!(result, payload);
            assert_schema(&result, &schema);
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        let mut mixed = technology_map_payload();
        let mut other = mixed["origins"][0]["technology_observations"][0].clone();
        other["reported_version"] = json!("1.25.0");
        other["evidence"] =
            json!([BODY_REFERENCE_TOKEN.replace("9007199254740993", "9007199254740994")]);
        mixed["origins"][0]["technology_observations"]
            .as_array_mut()
            .unwrap()
            .push(other);
        *mock.state.payload.lock().unwrap() = mixed.clone();
        let result = mock
            .command(&["scope", "service-map", "--session-id", &session])
            .await;
        assert_eq!(result, mixed);
        assert_schema(&result, &output_schema("target.service_map.get"));
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_shared_sequences_require_one_exact_row_token() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let mut base = technology_map_payload();
        let mut other = base["origins"][0]["technology_observations"][0].clone();
        other["reported_version"] = json!("1.25.0");
        base["origins"][0]["technology_observations"]
            .as_array_mut()
            .unwrap()
            .push(other);
        // One retained row can explicitly declare different product/version
        // tuples. Reusing its identical evidence token remains valid.
        *mock.state.payload.lock().unwrap() = base.clone();
        let result = mock
            .command(&["scope", "service-map", "--session-id", &session])
            .await;
        assert_eq!(result, base);
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
        for field in [4, 5] {
            let mut fields: Vec<_> = BODY_REFERENCE_TOKEN.split('.').collect();
            fields[field] = "44444444-4444-4444-4444-444444444444";
            let conflicting_token = fields.join(".");
            for pointer in [
                "/origins/0/technology_observations/1/evidence/0",
                "/origins/0/endpoints/0/evidence/0",
                "/origins/0/technology_clues/0/evidence/0",
                "/origins/0/redirects/0/evidence/0",
                "/origins/0/header_links/0/evidence/0",
            ] {
                let mut payload = base.clone();
                *payload.pointer_mut(pointer).unwrap() = json!(conflicting_token);
                *mock.state.payload.lock().unwrap() = payload;
                let input = json!({"session_id":session}).to_string();
                let result = mock
                    .command_with_code(&["call", "target.service_map.get", "--input", &input], 5)
                    .await;
                assert_eq!(
                    result["error"]["code"], "INVALID_RESPONSE",
                    "{pointer} field {field}"
                );
                assert_eq!(result["error"]["retryable"], false);
                assert!(!result.to_string().contains(&conflicting_token));
                mock.assert_reads(
                    "/api/target/service-map",
                    &[("in_scope_only", "false"), ("max_records", "10000")],
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_row_spans_are_disjoint_half_open_ranges() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for (product_start, version_start, header_index, valid) in [
            (1, 7, 0, false),  // Product overlaps the preceding product.
            (8, 14, 0, false), // Product overlaps the preceding version.
            (0, 6, 0, false),  // Duplicate source positions are overlapping too.
            (0, 6, 1, true),   // Distinct response headers have distinct byte bases.
            (12, 18, 0, true), // A half-open endpoint may touch the next start.
            (13, 19, 0, true), // Ordinary separated tokens remain valid.
        ] {
            let mut payload = technology_evidence_payload();
            let mut second = technology_observation_payload();
            second["product_span"] = json!({"start":product_start,"end":product_start+5});
            second["version_span"] = json!({"start":version_start,"end":version_start+6});
            second["header_index"] = json!(header_index);
            payload["row"]["technology_observations"]
                .as_array_mut()
                .unwrap()
                .push(second);
            *mock.state.payload.lock().unwrap() = payload.clone();
            let result = mock
                .command_with_code(
                    &[
                        "scope",
                        "service-map-evidence",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        BODY_REFERENCE_TOKEN,
                    ],
                    if valid { 0 } else { 5 },
                )
                .await;
            if valid {
                assert_eq!(result, payload);
            } else {
                assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
                assert_eq!(result["error"]["retryable"], false);
            }
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_token_budget_preserves_partial_results_without_claiming_products(
    ) {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for exact_row in [false, true] {
            let operation = if exact_row {
                "target.service_map.evidence.get"
            } else {
                "target.service_map.get"
            };
            let path = if exact_row {
                "/api/target/service-map/evidence"
            } else {
                "/api/target/service-map"
            };
            let schema = output_schema(operation);
            for count in [0, 1, 8] {
                let mut payload = if exact_row {
                    technology_evidence_payload()
                } else {
                    technology_map_payload()
                };
                if exact_row {
                    payload["row"]["technology_observations"] = json!((0..count)
                        .map(|index| {
                            let mut observation = technology_observation_payload();
                            observation["header_index"] = json!(index);
                            observation
                        })
                        .collect::<Vec<_>>());
                    payload["row"]["metadata_complete"] = json!(false);
                    payload["row"]["omitted"] = if count == 8 {
                        json!([
                            "technology_token_scan_limit",
                            "technology_observations_limit"
                        ])
                    } else {
                        json!(["technology_token_scan_limit"])
                    };
                } else {
                    let original = payload["origins"][0]["technology_observations"][0].clone();
                    payload["origins"][0]["technology_observations"] = json!((0..count)
                        .map(|index| {
                            let mut observation = original.clone();
                            observation["reported_version"] = json!(format!("{}.0", index + 1));
                            observation
                        })
                        .collect::<Vec<_>>());
                    payload["snapshot"]["complete"] = json!(false);
                    payload["snapshot"]["stopped_by"] = json!(["technology_token_scan_limit"]);
                }
                for call in [false, true] {
                    *mock.state.payload.lock().unwrap() = payload.clone();
                    let input = if exact_row {
                        json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN})
                    } else {
                        json!({"session_id":session})
                    }
                    .to_string();
                    let args = if call {
                        vec!["call", operation, "--input", &input]
                    } else if exact_row {
                        vec![
                            "scope",
                            "service-map-evidence",
                            "--session-id",
                            &session,
                            "--evidence-id",
                            BODY_REFERENCE_TOKEN,
                        ]
                    } else {
                        vec!["scope", "service-map", "--session-id", &session]
                    };
                    let result = mock.command(&args).await;
                    let data = if call { &result["data"] } else { &result };
                    assert_eq!(data, &payload);
                    assert_schema(data, &schema);
                    let query = if exact_row {
                        vec![("evidence_id", BODY_REFERENCE_TOKEN)]
                    } else {
                        vec![("in_scope_only", "false"), ("max_records", "10000")]
                    };
                    mock.assert_reads(path, &query);
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_token_budget_rejects_invalid_limits_and_false_completeness() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let mut invalid_maps = Vec::new();
        for value in [
            json!(0),
            json!(127),
            json!(129),
            Value::Null,
            json!("SYNTHETIC_PRIVATE_VALUE"),
        ] {
            let mut payload = technology_map_payload();
            payload["snapshot"]["limits"]["technology_token_candidates_per_row"] = value;
            invalid_maps.push(payload);
        }
        let mut payload = technology_map_payload();
        payload["origins"][0]["technology_observations"] = json!([]);
        payload["snapshot"]["stopped_by"] = json!(["technology_token_scan_limit"]);
        invalid_maps.push(payload);
        for payload in invalid_maps {
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock
                .command_with_code(&["scope", "service-map", "--session-id", &session], 5)
                .await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
        for absent in [false, true] {
            let mut payload = technology_evidence_payload();
            if absent {
                payload["row"]
                    .as_object_mut()
                    .unwrap()
                    .remove("technology_observations");
            } else {
                payload["row"]["technology_observations"] = json!([]);
            }
            payload["row"]["omitted"] = json!(["technology_token_scan_limit"]);
            *mock.state.payload.lock().unwrap() = payload;
            let input =
                json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN}).to_string();
            let result = mock
                .command_with_code(
                    &["call", "target.service_map.evidence.get", "--input", &input],
                    5,
                )
                .await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        // The parser-work budget is independent of the observation cap. It
        // cannot justify reporting that eight observations were retained.
        let mut payload = technology_evidence_payload();
        payload["row"]["technology_observations"] = json!([]);
        payload["row"]["metadata_complete"] = json!(false);
        payload["row"]["omitted"] = json!([
            "technology_token_scan_limit",
            "technology_observations_limit"
        ]);
        *mock.state.payload.lock().unwrap() = payload;
        let result = mock
            .command_with_code(
                &[
                    "scope",
                    "service-map-evidence",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ],
                5,
            )
            .await;
        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
        mock.assert_reads(
            "/api/target/service-map/evidence",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
        // Intermediate servers may omit the new cap while exposing observations.
        let mut payload = technology_map_payload();
        payload["snapshot"]["limits"]
            .as_object_mut()
            .unwrap()
            .remove("technology_token_candidates_per_row");
        *mock.state.payload.lock().unwrap() = payload.clone();
        let result = mock
            .command(&["scope", "service-map", "--session-id", &session])
            .await;
        assert_eq!(result, payload);
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
    }

    fn technology_map_at_cap(total: usize) -> Value {
        let mut payload = technology_map_payload();
        let original = payload["origins"][0].clone();
        let mut origins = Vec::new();
        let mut rows = 0usize;
        let mut remaining = total;
        while remaining > 0 {
            let count = remaining.min(32);
            let index = origins.len();
            let mut origin = original.clone();
            let host = format!("origin-{index}.example.com");
            origin["origin"] = json!(format!("https://{host}"));
            origin["host"] = json!(host);
            let row_count = count.div_ceil(8);
            rows += row_count;
            origin["request_count"] = json!(row_count);
            origin["endpoints"][0]["request_count"] = json!(row_count);
            let token =
                BODY_REFERENCE_TOKEN.replace("9007199254740993", &(index * 4 + 1).to_string());
            for field in ["endpoints", "technology_clues", "redirects", "header_links"] {
                origin[field][0]["evidence"] = json!([token]);
            }
            origin["technology_observations"] = json!((0..count)
                .map(|item| {
                    let mut observation = original["technology_observations"][0].clone();
                    observation["reported_version"] = json!(format!("{}.0", item + 1));
                    observation["evidence"] = json!([BODY_REFERENCE_TOKEN
                        .replace("9007199254740993", &(index * 4 + item / 8 + 1).to_string())]);
                    observation
                })
                .collect::<Vec<_>>());
            origins.push(origin);
            remaining -= count;
        }
        payload["snapshot"]["origin_count"] = json!(origins.len());
        payload["snapshot"]["endpoint_count"] = json!(origins.len());
        for field in [
            "retained_rows",
            "considered_rows",
            "projected_rows",
            "included_rows",
        ] {
            payload["snapshot"][field] = json!(rows);
        }
        payload["origins"] = json!(origins);
        payload
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_technology_global_budget_and_origin_binding_fail_closed() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for (total, valid) in [(32, true), (2000, true), (2001, false)] {
            let mut payload = technology_map_at_cap(total);
            if valid {
                payload["snapshot"]["complete"] = json!(false);
                payload["snapshot"]["stopped_by"] = json!([if total == 32 {
                    "technology_observations_per_origin_limit"
                } else {
                    "technology_observations_map_limit"
                }]);
            }
            *mock.state.payload.lock().unwrap() = payload.clone();
            let result = mock
                .command_with_code(
                    &["scope", "service-map", "--session-id", &session],
                    if valid { 0 } else { 5 },
                )
                .await;
            if valid {
                assert_eq!(result, payload);
            } else {
                assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            }
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
        let mut payload = technology_map_at_cap(40);
        let token = payload["origins"][0]["technology_observations"][0]["evidence"][0].clone();
        payload["origins"][1]["technology_observations"][0]["evidence"] = json!([token]);
        *mock.state.payload.lock().unwrap() = payload;
        let result = mock
            .command_with_code(&["scope", "service-map", "--session-id", &session], 5)
            .await;
        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
    }

    const BODY_REFERENCE_TOKEN: &str = "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000";

    fn peer_evidence_payload(status: &str, ip: &str, route: &str) -> Value {
        let mut payload = evidence_payload(true);
        payload["socket_peer"] = json!({
            "status":status,
            "observation":if status == "not_recorded" {Value::Null} else {json!({
                "source":"http_response_remote_addr","observed_at":"2026-10-12T06:00:01Z","route":route,
                "address":if status == "observed" {json!({"ip":ip,"port":443})} else {Value::Null},
                "reason":if status == "unavailable" {json!("remote_addr_unavailable")} else {Value::Null}
            })}
        });
        payload
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_peer_evidence_is_explicit_typed_and_matches_schema() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.evidence.get");
        assert!(!schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("socket_peer")));
        for status in ["observed", "not_recorded", "unavailable"] {
            for ip in ["192.0.2.7", "2001:db8::7"] {
                for route in ["direct", "http_proxy", "socks_proxy", "unknown"] {
                    let payload = peer_evidence_payload(status, ip, route);
                    *mock.state.payload.lock().unwrap() = payload.clone();
                    let direct = mock
                        .command(&[
                            "target",
                            "service-map-evidence",
                            "--session-id",
                            &session,
                            "--evidence-id",
                            BODY_REFERENCE_TOKEN,
                            "--include-socket-peer",
                        ])
                        .await;
                    assert_eq!(direct, payload);
                    assert_schema(&direct, &schema);
                    mock.assert_reads(
                        "/api/target/service-map/evidence",
                        &[
                            ("evidence_id", BODY_REFERENCE_TOKEN),
                            ("include_socket_peer", "true"),
                        ],
                    );
                    let input = json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN,"include_socket_peer":true}).to_string();
                    let called = mock
                        .command(&["call", "target.service_map.evidence.get", "--input", &input])
                        .await;
                    assert_eq!(called["data"], direct);
                    assert_eq!(called["meta"]["session_id"], session);
                    mock.assert_reads(
                        "/api/target/service-map/evidence",
                        &[
                            ("evidence_id", BODY_REFERENCE_TOKEN),
                            ("include_socket_peer", "true"),
                        ],
                    );
                }
            }
        }
        *mock.state.payload.lock().unwrap() = evidence_payload(false);
        let result = mock
            .command(&[
                "target",
                "service-map-evidence",
                "--session-id",
                &session,
                "--evidence-id",
                BODY_REFERENCE_TOKEN,
                "--include-socket-peer",
            ])
            .await;
        assert_eq!(result, evidence_payload(false));
        assert_schema(&result, &schema);
        mock.assert_reads(
            "/api/target/service-map/evidence",
            &[
                ("evidence_id", BODY_REFERENCE_TOKEN),
                ("include_socket_peer", "true"),
            ],
        );
        *mock.state.payload.lock().unwrap() = evidence_payload(true);
        let input = json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN,"include_socket_peer":false}).to_string();
        let result = mock
            .command(&["call", "target.service_map.evidence.get", "--input", &input])
            .await;
        assert_eq!(result["data"], evidence_payload(true));
        mock.assert_reads(
            "/api/target/service-map/evidence",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_peer_evidence_fails_closed_on_unrequested_or_malformed_extension() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map-evidence",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
            "--include-socket-peer",
        ];
        for (pointer, value) in [
            ("/socket_peer/status", json!("origin_verified")),
            ("/socket_peer/status", json!("unavailable")),
            ("/socket_peer/observation", Value::Null),
            ("/socket_peer/observation/source", json!("dns_lookup")),
            ("/socket_peer/observation/observed_at", json!("invalid")),
            (
                "/socket_peer/observation/route",
                json!("inferred_backend_chain"),
            ),
            (
                "/socket_peer/observation/address/ip",
                json!("SYNTHETIC_PRIVATE_VALUE.example.com"),
            ),
            ("/socket_peer/observation/address/port", json!(65536)),
            (
                "/socket_peer/observation/reason",
                json!("remote_addr_unavailable"),
            ),
            ("/row/evidence_id", json!("different-token")),
        ] {
            let mut payload = peer_evidence_payload("observed", "192.0.2.7", "direct");
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[
                    ("evidence_id", BODY_REFERENCE_TOKEN),
                    ("include_socket_peer", "true"),
                ],
            );
        }
        for mut payload in [
            evidence_payload(true),
            peer_evidence_payload("observed", "192.0.2.7", "direct"),
        ] {
            if payload.get("socket_peer").is_some() {
                payload["available"] = json!(false);
                payload["row"] = Value::Null;
                payload["reason"] = json!("stale");
            }
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[
                    ("evidence_id", BODY_REFERENCE_TOKEN),
                    ("include_socket_peer", "true"),
                ],
            );
        }
        *mock.state.payload.lock().unwrap() =
            peer_evidence_payload("observed", "192.0.2.7", "direct");
        let result = mock.command_with_code(&args[..args.len() - 1], 5).await;
        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
        assert!(!result.to_string().contains("192.0.2.7"));
        mock.assert_reads(
            "/api/target/service-map/evidence",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
        for value in [Value::Null, json!("true"), json!(1)] {
            let input = json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN,"include_socket_peer":value}).to_string();
            let result = mock
                .command_with_code(
                    &["call", "target.service_map.evidence.get", "--input", &input],
                    2,
                )
                .await;
            assert_eq!(result["error"]["code"], "INVALID_INPUT");
            assert!(mock.state.requests.lock().unwrap().is_empty());
        }
    }

    fn html_generators_payload(status: &str) -> Value {
        let available = matches!(status, "scanned" | "partial");
        json!({
            "schema_version":1,"generator_catalog_version":1,"session_id":Uuid::nil(),"evidence_id":BODY_REFERENCE_TOKEN,
            "available":available,"status":if available {"scanned"} else {status},"complete":status == "scanned",
            "reasons":match status {
                "scanned" => json!([]),"partial" => json!(["capture_preview_truncated"]),
                "unsupported" => json!(["unsupported_content_type"]),_ => json!(["inactive_session"])
            },
            "source_origin":if available {json!("https://example.com")} else {Value::Null},
            "source_sequence":if available {json!("9007199254740993")} else {Value::Null},
            "source_started_at":if available {json!("2026-10-12T00:00:00Z")} else {Value::Null},
            "retained_preview_bytes":if available {json!(88)} else {Value::Null},
            "scan_prefix_bytes":if available {88} else {0},
            "capture_preview_truncated":status == "partial","scan_prefix_truncated":false,
            "coverage":"literal_html_generator_explicit_head","head_closed":available,"parser_stop":null,
            "tag_candidates_inspected":if available {4} else {0},
            "additional_tag_candidate_encountered":false,"additional_observation_encountered":false,"unknown_product_declarations":0,
            "observations":if available {json!([{
                "product_id":"wordpress","name":"WordPress","catalog_category":"cms",
                "reported_version":"6.8.1","version_status":"reported","source":"response_html_meta_generator",
                "strength":"literal_declaration","rule_id":"html_meta_generator.wordpress.v1",
                "tag_span":{"start":12,"end":61},"content_span":{"start":44,"end":59},
                "product_span":{"start":44,"end":53},"version_span":{"start":54,"end":59},"offset_basis":"retained_utf8_preview"
            }])} else {json!([])},
            "limits":{"scan_bytes":262144,"serialized_row_bytes":2097152,"tag_candidates":4096,"tag_bytes":4096,"attributes":32,"observations":8,"version_bytes":32,"version_separator_bytes":32},
            "offset_basis":"retained_utf8_preview"
        })
    }

    #[test]
    fn service_map_html_generator_discovery_and_preview_are_explicit_and_offline() {
        let operation = "target.service_map.html_generators.get";
        let spec = super::spec(operation);
        let session = Uuid::nil().to_string();
        let input = json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN});
        assert_eq!(spec["side_effect"], "read");
        assert_eq!(spec["requires_confirmation"], false);
        assert_eq!(
            spec["input_schema"]["required"],
            json!(["session_id", "evidence_id"])
        );
        assert_eq!(spec["input_schema"]["additionalProperties"], false);
        assert_eq!(
            spec["input_schema"]["properties"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        let schema = super::cli(&["schema", "input", operation], 0);
        assert_eq!(schema["schema"], spec["input_schema"]);
        let called = super::cli(
            &[
                "--dry-run",
                "call",
                operation,
                "--input",
                &input.to_string(),
            ],
            0,
        );
        for noun in ["scope", "target"] {
            let direct = super::cli(
                &[
                    "--dry-run",
                    noun,
                    "service-map",
                    "html-generators",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ],
                0,
            );
            assert_eq!(direct, called["data"]);
            assert_eq!(direct["input"], input);
            assert_eq!(direct["api"]["method"], "GET");
            assert_eq!(direct["api"]["body"], Value::Null);
            assert!(direct["api"].get("headers").is_none());
            let path = direct["api"]["path"].as_str().unwrap();
            assert!(path.starts_with("/api/target/service-map/html-generators?"));
            let params: BTreeMap<_, _> =
                url::form_urlencoded::parse(path.split_once('?').unwrap().1.as_bytes())
                    .into_owned()
                    .collect();
            assert_eq!(
                params,
                BTreeMap::from([
                    ("session_id".into(), session.clone()),
                    ("evidence_id".into(), BODY_REFERENCE_TOKEN.into())
                ])
            );
            for forbidden in ["--technology-catalog-version", "--origin", "--max-records"] {
                let error = super::cli(
                    &[
                        "--dry-run",
                        noun,
                        "service-map",
                        "html-generators",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        BODY_REFERENCE_TOKEN,
                        forbidden,
                        "SYNTHETIC_PRIVATE_VALUE",
                    ],
                    2,
                );
                assert_eq!(error["error"]["code"], "INVALID_INPUT");
                assert!(!error.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            }
            for required in [
                vec!["--session-id", session.as_str()],
                vec!["--evidence-id", BODY_REFERENCE_TOKEN],
            ] {
                let mut args = vec!["--dry-run", noun, "service-map", "html-generators"];
                args.extend(required);
                assert_eq!(super::cli(&args, 2)["error"]["code"], "INVALID_INPUT");
            }
            for parent_flags in [
                vec!["--technology-catalog-version", "2"],
                vec!["--origin", "https://example.com"],
                vec!["--max-records", "10"],
                vec!["--session-id", session.as_str()],
                vec!["--in-scope-only"],
            ] {
                let mut args = vec!["--dry-run", noun, "service-map"];
                args.extend(parent_flags);
                args.extend([
                    "html-generators",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ]);
                assert_eq!(super::cli(&args, 2)["error"]["code"], "INVALID_INPUT");
            }
            let help = std::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"))
                .args([noun, "service-map", "html-generators", "--help"])
                .output()
                .unwrap();
            assert!(help.status.success());
            let text = String::from_utf8(help.stdout).unwrap();
            assert!(text.contains("Partial or unsupported"));
            assert!(text.contains("--session-id") && text.contains("--evidence-id"));
        }
        for field in ["session_id", "evidence_id"] {
            for value in [
                None,
                Some(Value::Null),
                Some(json!("SYNTHETIC_PRIVATE_VALUE")),
            ] {
                let mut invalid = input.clone();
                if let Some(value) = value {
                    invalid[field] = value;
                } else {
                    invalid.as_object_mut().unwrap().remove(field);
                }
                let error = super::cli(
                    &[
                        "--dry-run",
                        "call",
                        operation,
                        "--input",
                        &invalid.to_string(),
                    ],
                    2,
                );
                assert_eq!(error["error"]["code"], "INVALID_INPUT");
                assert!(!error.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            }
        }
        for field in [
            "technology_catalog_version",
            "generator_catalog_version",
            "origin",
            "in_scope_only",
            "max_records",
            "batch",
        ] {
            let mut invalid = input.clone();
            invalid[field] = json!(true);
            let error = super::cli(
                &[
                    "--dry-run",
                    "call",
                    operation,
                    "--input",
                    &invalid.to_string(),
                ],
                2,
            );
            assert_eq!(error["error"]["code"], "INVALID_INPUT");
        }
        for example in spec["examples"].as_array().unwrap() {
            let result = super::cli(
                &[
                    "--dry-run",
                    "call",
                    operation,
                    "--input",
                    &example.to_string(),
                ],
                0,
            );
            assert!(result["data"]["api"].get("headers").is_none());
        }
        assert_eq!(
            super::cli(&["examples", operation], 0)["examples"],
            spec["examples"]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_statuses_preserve_schema_and_exact_read_counts() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let operation = "target.service_map.html_generators.get";
        let schema = output_schema(operation);
        for status in ["scanned", "partial", "unsupported", "unavailable"] {
            let payload = html_generators_payload(status);
            *mock.state.payload.lock().unwrap() = payload.clone();
            for noun in ["scope", "target"] {
                let result = mock
                    .command(&[
                        noun,
                        "service-map",
                        "html-generators",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        BODY_REFERENCE_TOKEN,
                    ])
                    .await;
                assert_eq!(result, payload);
                assert_schema(&result, &schema);
                mock.assert_reads(
                    "/api/target/service-map/html-generators",
                    &[("evidence_id", BODY_REFERENCE_TOKEN)],
                );
            }
            let input =
                json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN}).to_string();
            let called = mock.command(&["call", operation, "--input", &input]).await;
            assert_eq!(called["data"], payload);
            assert_eq!(called["meta"]["session_id"], session);
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        // Reaching a supported head end does not make a truncated body complete.
        let mut payload = html_generators_payload("partial");
        payload["scan_prefix_truncated"] = json!(true);
        payload["scan_prefix_bytes"] = json!(262143);
        payload["retained_preview_bytes"] = json!(262145);
        payload["reasons"] = json!(["capture_preview_truncated", "scan_prefix_truncated"]);
        *mock.state.payload.lock().unwrap() = payload.clone();
        let result = mock
            .command(&[
                "target",
                "service-map",
                "html-generators",
                "--session-id",
                &session,
                "--evidence-id",
                BODY_REFERENCE_TOKEN,
            ])
            .await;
        assert_eq!(result, payload);
        assert_schema(&result, &schema);
        mock.assert_reads(
            "/api/target/service-map/html-generators",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_partial_stops_and_version_statuses_are_literal() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        let schema = output_schema("target.service_map.html_generators.get");
        for reason in sniper::service_map_html::PARSER_STOP_REASONS {
            let mut payload = html_generators_payload("scanned");
            payload["complete"] = json!(false);
            payload["head_closed"] = json!(false);
            payload["reasons"] = json!([reason]);
            payload["parser_stop"] = json!({"reason":reason,"byte_cursor":88});
            if *reason == "input_limit" {
                payload["scan_prefix_bytes"] = json!(262142);
                payload["retained_preview_bytes"] = json!(262145);
                payload["scan_prefix_truncated"] = json!(true);
                payload["parser_stop"]["byte_cursor"] = json!(262142);
                payload["reasons"] = json!(["input_limit", "scan_prefix_truncated"]);
            }
            if *reason == "tag_candidate_limit" {
                payload["tag_candidates_inspected"] = json!(4096);
                payload["scan_prefix_bytes"] = json!(20000);
                payload["retained_preview_bytes"] = json!(20000);
                payload["additional_tag_candidate_encountered"] = json!(true);
                payload["parser_stop"]["byte_cursor"] = json!(18000);
            }
            if *reason == "observation_limit" {
                let observation = payload["observations"][0].clone();
                payload["observations"] = json!((0..8)
                    .map(|index| {
                        let mut value = observation.clone();
                        for field in ["tag_span", "content_span", "product_span", "version_span"] {
                            for bound in ["start", "end"] {
                                value[field][bound] =
                                    json!(value[field][bound].as_u64().unwrap() + index * 100);
                            }
                        }
                        value
                    })
                    .collect::<Vec<_>>());
                payload["scan_prefix_bytes"] = json!(1000);
                payload["retained_preview_bytes"] = json!(1000);
                payload["parser_stop"]["byte_cursor"] = json!(800);
                payload["tag_candidates_inspected"] = json!(11);
                payload["additional_observation_encountered"] = json!(true);
            }
            *mock.state.payload.lock().unwrap() = payload.clone();
            let result = mock.command(&args).await;
            assert_eq!(result, payload);
            assert_schema(&result, &schema);
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        for product in ["wordpress", "drupal"] {
            for version_status in ["reported", "not_reported", "unsupported_format"] {
                let mut payload = html_generators_payload("scanned");
                let item = &mut payload["observations"][0];
                if product == "drupal" {
                    item["product_id"] = json!("drupal");
                    item["name"] = json!("Drupal");
                    item["rule_id"] = json!("html_meta_generator.drupal.v1");
                    item["product_span"]["end"] = json!(50);
                }
                item["version_status"] = json!(version_status);
                if version_status != "reported" {
                    item["reported_version"] = Value::Null;
                    item["version_span"] = Value::Null;
                }
                if version_status == "not_reported" {
                    item["content_span"]["end"] = item["product_span"]["end"].clone();
                }
                *mock.state.payload.lock().unwrap() = payload.clone();
                let result = mock.command(&args).await;
                assert_eq!(result, payload);
                assert_schema(&result, &schema);
                mock.assert_reads(
                    "/api/target/service-map/html-generators",
                    &[("evidence_id", BODY_REFERENCE_TOKEN)],
                );
            }
        }
        let mut empty = html_generators_payload("scanned");
        empty["observations"] = json!([]);
        empty["unknown_product_declarations"] = json!(1);
        *mock.state.payload.lock().unwrap() = empty.clone();
        assert_eq!(mock.command(&args).await, empty);
        mock.assert_reads(
            "/api/target/service-map/html-generators",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_rejects_misbound_private_or_unbounded_responses() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        for (pointer, value) in [
            ("/schema_version", json!(2)),
            ("/generator_catalog_version", json!(2)),
            ("/schema_version", json!(1.0)),
            ("/generator_catalog_version", json!("1")),
            ("/session_id", json!(Uuid::from_u128(1))),
            ("/evidence_id", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/source_sequence", json!("9007199254740992")),
            ("/source_sequence", json!("09007199254740993")),
            ("/source_sequence", json!("18446744073709551616")),
            ("/source_started_at", json!("SYNTHETIC_PRIVATE_VALUE")),
            (
                "/source_origin",
                json!("https://user:SYNTHETIC_PRIVATE_VALUE@example.com"),
            ),
            (
                "/source_origin",
                json!("https://example.com/path?SYNTHETIC_PRIVATE_VALUE"),
            ),
            ("/status", json!("partial")),
            ("/available", json!(false)),
            ("/complete", json!(false)),
            ("/reasons", json!(["SYNTHETIC_PRIVATE_VALUE"])),
            ("/reasons", json!(["inactive_session"])),
            ("/capture_preview_truncated", json!(true)),
            ("/scan_prefix_truncated", json!(true)),
            ("/head_closed", json!(false)),
            (
                "/parser_stop",
                json!({"reason":"end_of_preview","byte_cursor":88}),
            ),
            ("/scan_prefix_bytes", json!(262145)),
            ("/scan_prefix_bytes", json!(87)),
            ("/scan_prefix_bytes", json!(-1)),
            ("/retained_preview_bytes", json!(87)),
            ("/tag_candidates_inspected", json!(4097)),
            ("/tag_candidates_inspected", json!(0)),
            ("/additional_tag_candidate_encountered", json!(true)),
            ("/additional_observation_encountered", json!(true)),
            ("/unknown_product_declarations", json!(4097)),
            ("/unknown_product_declarations", json!(4)),
            ("/coverage", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/offset_basis", json!("wire_bytes")),
            ("/limits/scan_bytes", json!(262145)),
            ("/limits/version_bytes", json!(33)),
            (
                "/observations/0/product_id",
                json!("SYNTHETIC_PRIVATE_VALUE"),
            ),
            ("/observations/0/name", json!("Drupal")),
            ("/observations/0/catalog_category", json!("server")),
            (
                "/observations/0/rule_id",
                json!("html_meta_generator.drupal.v1"),
            ),
            ("/observations/0/source", json!("response_header")),
            ("/observations/0/strength", json!("proven")),
            ("/observations/0/offset_basis", json!("wire_bytes")),
            ("/observations/0/version_status", json!("not_reported")),
            (
                "/observations/0/version_status",
                json!("SYNTHETIC_PRIVATE_VALUE"),
            ),
            (
                "/observations/0/reported_version",
                json!("6.8-SYNTHETIC_PRIVATE_VALUE"),
            ),
            ("/observations/0/reported_version", json!("6..8")),
            ("/observations/0/reported_version", json!("6.8.1\n")),
            ("/observations/0/reported_version", json!("1".repeat(33))),
            ("/observations/0/tag_span/start", json!(61)),
            ("/observations/0/tag_span/end", json!(89)),
            ("/observations/0/content_span/start", json!(12)),
            ("/observations/0/content_span/end", json!(61)),
            ("/observations/0/product_span/start", json!(45)),
            ("/observations/0/product_span/end", json!(52)),
            ("/observations/0/version_span/start", json!(53)),
            ("/observations/0/version_span/end", json!(58)),
        ] {
            let mut payload = html_generators_payload("scanned");
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        for status in ["scanned", "unavailable"] {
            let payload = html_generators_payload(status);
            for field in payload.as_object().unwrap().keys() {
                let mut missing = payload.clone();
                missing.as_object_mut().unwrap().remove(field);
                *mock.state.payload.lock().unwrap() = missing;
                assert_eq!(
                    mock.command_with_code(&args, 5).await["error"]["code"],
                    "INVALID_RESPONSE",
                    "{status}: {field}"
                );
                mock.assert_reads(
                    "/api/target/service-map/html-generators",
                    &[("evidence_id", BODY_REFERENCE_TOKEN)],
                );
            }
        }
        for pointer in ["", "/observations/0", "/observations/0/tag_span", "/limits"] {
            let mut payload = html_generators_payload("scanned");
            payload
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("snippet".into(), json!("SYNTHETIC_PRIVATE_VALUE"));
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        let mut duplicated = html_generators_payload("scanned");
        duplicated["observations"] =
            json!([duplicated["observations"][0], duplicated["observations"][0]]);
        *mock.state.payload.lock().unwrap() = duplicated;
        assert_eq!(
            mock.command_with_code(&args, 5).await["error"]["code"],
            "INVALID_RESPONSE"
        );
        mock.assert_reads(
            "/api/target/service-map/html-generators",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
    }

    #[test]
    fn service_map_html_generator_schemas_reject_missing_nullable_and_free_text_fields() {
        let schema = output_schema("target.service_map.html_generators.get");
        for status in ["scanned", "partial", "unsupported", "unavailable"] {
            let payload = html_generators_payload(status);
            assert_schema(&payload, &schema);
            for field in payload.as_object().unwrap().keys() {
                let mut missing = payload.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(!super::matches_schema(&missing, &schema), "{field}");
            }
        }
        let payload = html_generators_payload("scanned");
        for field in payload["observations"][0].as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing["observations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(!super::matches_schema(&missing, &schema), "{field}");
        }
        for (pointer, value) in [
            ("/status", json!("partial")),
            ("/generator_catalog_version", json!(2)),
            ("/reasons", json!(["SYNTHETIC_PRIVATE_VALUE"])),
            ("/observations/0/name", json!("Drupal")),
            ("/observations/0/reported_version", json!("6..8")),
            ("/observations/0/reported_version", json!("1".repeat(33))),
            ("/observations/0/version_status", json!("not_reported")),
            (
                "/observations/0/product_id",
                json!("SYNTHETIC_PRIVATE_VALUE"),
            ),
            ("/scan_prefix_bytes", json!(262145)),
            ("/limits/observations", json!(9)),
        ] {
            let mut malformed = payload.clone();
            *malformed.pointer_mut(pointer).unwrap() = value;
            assert!(!super::matches_schema(&malformed, &schema), "{pointer}");
        }
        let mut nine = payload;
        nine["observations"] = json!(vec![nine["observations"][0].clone(); 9]);
        assert!(!super::matches_schema(&nine, &schema));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_http_failures_never_fallback_or_echo() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "target",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        for (status, code, exit) in [
            (404, "UNSUPPORTED_FEATURE", 5),
            (302, "REDIRECT_REFUSED", 5),
            (503, "HTTP_STATUS_ERROR", 6),
        ] {
            *mock.state.read_status.lock().unwrap() = status;
            *mock.state.payload.lock().unwrap() = json!({"error":"SYNTHETIC_PRIVATE_VALUE"});
            let result = mock.command_with_code(&args, exit).await;
            assert_eq!(result["error"]["code"], code);
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_rejects_inconsistent_partial_and_missing_states() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        let mut partial = html_generators_payload("scanned");
        partial["head_closed"] = json!(false);
        partial["complete"] = json!(false);
        partial["reasons"] = json!(["tag_byte_limit"]);
        // A raw cap cursor can split a Unicode character in the retained input.
        // The CLI has no body text and must not treat this as a slicing offset.
        partial["scan_prefix_bytes"] = json!(5000);
        partial["retained_preview_bytes"] = json!(5000);
        partial["parser_stop"] = json!({"reason":"tag_byte_limit","byte_cursor":4163});
        *mock.state.payload.lock().unwrap() = partial.clone();
        assert_eq!(mock.command(&args).await, partial);
        mock.assert_reads(
            "/api/target/service-map/html-generators",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
        for (pointer, value) in [
            ("/parser_stop/reason", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/parser_stop/byte_cursor", json!(5001)),
            ("/parser_stop/byte_cursor", json!(60)),
            ("/parser_stop/byte_cursor", json!(-1)),
            ("/parser_stop/byte_cursor", json!(4163.0)),
            ("/reasons", json!(["tag_byte_limit", "tag_byte_limit"])),
            ("/reasons", json!(["tag_byte_limit", "attribute_limit"])),
            ("/complete", json!(true)),
            ("/head_closed", json!(true)),
            ("/additional_tag_candidate_encountered", json!(true)),
            ("/additional_observation_encountered", json!(true)),
            ("/observations/0/tag_span/end", json!(4110)),
        ] {
            let mut payload = partial.clone();
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        for (pointer, value) in [
            ("/source_origin", json!("https://example.com")),
            ("/source_sequence", json!("9007199254740993")),
            ("/source_started_at", json!("2026-10-12T00:00:00Z")),
            ("/retained_preview_bytes", json!(0)),
            ("/scan_prefix_bytes", json!(1)),
            ("/capture_preview_truncated", json!(true)),
            ("/scan_prefix_truncated", json!(true)),
            ("/head_closed", json!(true)),
            ("/tag_candidates_inspected", json!(1)),
            ("/unknown_product_declarations", json!(1)),
            ("/reasons", json!(["end_of_preview"])),
            ("/reasons", json!(["inactive_session", "record_changed"])),
            (
                "/parser_stop",
                json!({"reason":"end_of_preview","byte_cursor":0}),
            ),
        ] {
            let mut payload = html_generators_payload("unavailable");
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            assert_eq!(
                mock.command_with_code(&args, 5).await["error"]["code"],
                "INVALID_RESPONSE",
                "{pointer}"
            );
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        for field in ["reported_version", "version_span"] {
            let mut payload = html_generators_payload("scanned");
            payload["observations"][0]["version_status"] = json!("not_reported");
            payload["observations"][0]["reported_version"] = Value::Null;
            payload["observations"][0]["version_span"] = Value::Null;
            payload["observations"][0]["content_span"]["end"] = json!(53);
            payload["observations"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            *mock.state.payload.lock().unwrap() = payload;
            assert_eq!(
                mock.command_with_code(&args, 5).await["error"]["code"],
                "INVALID_RESPONSE",
                "{field}"
            );
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_identity_failure_stops_before_inspection() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        *mock.state.settings_override.lock().unwrap() =
            Some(json!({"error":"SYNTHETIC_PRIVATE_VALUE"}));
        let result = mock.command_with_code(&args, 5).await;
        assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        let requests = mock.state.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].path, "/api/settings");
        assert_eq!(requests[0].catalog_header, None);
        assert!(requests[0].query.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_html_generator_candidate_counts_include_explicit_head_tags() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "html-generators",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        let mut closed_empty = html_generators_payload("scanned");
        closed_empty["observations"] = json!([]);
        let mut closed_unknown = closed_empty.clone();
        closed_unknown["unknown_product_declarations"] = json!(1);
        let mut partial_observed = html_generators_payload("scanned");
        partial_observed["head_closed"] = json!(false);
        partial_observed["complete"] = json!(false);
        partial_observed["reasons"] = json!(["end_of_preview"]);
        partial_observed["parser_stop"] = json!({"reason":"end_of_preview","byte_cursor":88});
        let mut partial_unknown = partial_observed.clone();
        partial_unknown["observations"] = json!([]);
        partial_unknown["unknown_product_declarations"] = json!(1);
        let mut partial_empty = partial_observed.clone();
        partial_empty["observations"] = json!([]);
        partial_empty["scan_prefix_bytes"] = json!(0);
        partial_empty["retained_preview_bytes"] = json!(0);
        partial_empty["parser_stop"]["byte_cursor"] = json!(0);
        for (payload, minimum) in [
            (html_generators_payload("scanned"), 4),
            (closed_empty, 3),
            (closed_unknown, 4),
            (partial_observed, 3),
            (partial_unknown, 3),
            (partial_empty, 0),
        ] {
            let mut boundary = payload.clone();
            boundary["tag_candidates_inspected"] = json!(minimum);
            *mock.state.payload.lock().unwrap() = boundary.clone();
            assert_eq!(mock.command(&args).await, boundary);
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
            if minimum > 0 {
                let mut invalid = payload;
                invalid["tag_candidates_inspected"] = json!(minimum - 1);
                *mock.state.payload.lock().unwrap() = invalid;
                assert_eq!(
                    mock.command_with_code(&args, 5).await["error"]["code"],
                    "INVALID_RESPONSE"
                );
                mock.assert_reads(
                    "/api/target/service-map/html-generators",
                    &[("evidence_id", BODY_REFERENCE_TOKEN)],
                );
            }
        }
        let mut ninth = html_generators_payload("scanned");
        let observation = ninth["observations"][0].clone();
        ninth["observations"] = json!((0..8)
            .map(|index| {
                let mut value = observation.clone();
                for field in ["tag_span", "content_span", "product_span", "version_span"] {
                    for bound in ["start", "end"] {
                        value[field][bound] =
                            json!(value[field][bound].as_u64().unwrap() + index * 100);
                    }
                }
                value
            })
            .collect::<Vec<_>>());
        ninth["head_closed"] = json!(false);
        ninth["complete"] = json!(false);
        ninth["reasons"] = json!(["observation_limit"]);
        ninth["parser_stop"] = json!({"reason":"observation_limit","byte_cursor":800});
        ninth["scan_prefix_bytes"] = json!(1000);
        ninth["retained_preview_bytes"] = json!(1000);
        ninth["additional_observation_encountered"] = json!(true);
        for candidates in [10, 11] {
            ninth["tag_candidates_inspected"] = json!(candidates);
            *mock.state.payload.lock().unwrap() = ninth.clone();
            let result = mock
                .command_with_code(&args, if candidates == 11 { 0 } else { 5 })
                .await;
            if candidates == 11 {
                assert_eq!(result, ninth);
            } else {
                assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            }
            mock.assert_reads(
                "/api/target/service-map/html-generators",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    fn body_references_payload(status: &str) -> Value {
        let available = matches!(status, "scanned" | "partial");
        let mut payload = json!({
            "schema_version":1,"session_id":Uuid::nil(),"evidence_id":BODY_REFERENCE_TOKEN,
            "available":available,"status":status,"complete":status == "scanned",
            "reasons":match status {
                "scanned" => json!([]),"partial" => json!(["capture_preview_truncated"]),
                "unsupported" => json!(["unsupported_content_type"]),_ => json!(["inactive_session"])
            },
            "scanned_bytes":if available {100} else {0},
            "capture_preview_truncated":status == "partial","scan_prefix_truncated":false,
            "candidates_scanned":if available {2} else {0},"omitted_reference_occurrences":0,
            "references":if available {json!([{"target_origin":"https://cdn.example.com","occurrences":2,"preview_byte_offsets":[10,60],"omitted_offsets":0}])} else {json!([])},
            "limits":{"scan_bytes":262144,"serialized_row_bytes":2097152,"origins":100,"candidates":4096,"candidate_bytes":4096,"offsets_per_origin":5},
            "coverage":"literal_absolute_http_urls","offset_basis":"retained_utf8_preview"
        });
        if available {
            payload["source_origin"] = json!("https://example.com");
            payload["source_sequence"] = json!("9007199254740993");
            payload["source_started_at"] = json!("2026-10-12T06:00:00Z");
            payload["retained_preview_bytes"] = json!(100);
        }
        payload
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_body_reference_statuses_match_schema_and_explicit_read_paths() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.body_references.get");
        let scanned = body_references_payload("scanned");
        for field in [
            "source_origin",
            "source_sequence",
            "source_started_at",
            "retained_preview_bytes",
        ] {
            let mut missing = scanned.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!super::matches_schema(&missing, &schema));
        }
        let mut too_many_offsets = scanned.clone();
        too_many_offsets["references"][0]["preview_byte_offsets"] = json!([1, 2, 3, 4, 5, 6]);
        assert!(!super::matches_schema(&too_many_offsets, &schema));
        let mut invalid_status = scanned.clone();
        invalid_status["status"] = json!("partial");
        assert!(!super::matches_schema(&invalid_status, &schema));
        let operation = super::spec("target.service_map.body_references.get");
        assert_eq!(operation["side_effect"], "read");
        assert_eq!(operation["requires_confirmation"], false);
        assert_eq!(
            operation["input_schema"]["required"],
            json!(["session_id", "evidence_id"])
        );
        for status in ["scanned", "partial", "unsupported", "unavailable"] {
            let payload = body_references_payload(status);
            *mock.state.payload.lock().unwrap() = payload.clone();
            let direct = mock
                .command(&[
                    "target",
                    "service-map",
                    "body-references",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    BODY_REFERENCE_TOKEN,
                ])
                .await;
            assert_eq!(direct, payload);
            assert_schema(&direct, &schema);
            mock.assert_reads(
                "/api/target/service-map/body-references",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
            let input =
                json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN}).to_string();
            let called = mock
                .command(&[
                    "call",
                    "target.service_map.body_references.get",
                    "--input",
                    &input,
                ])
                .await;
            assert_eq!(called["data"], direct);
            assert_eq!(called["meta"]["session_id"], session);
            mock.assert_reads(
                "/api/target/service-map/body-references",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        // Body-origin limits differ from the smaller metadata-origin limits.
        let mut long_origin = body_references_payload("scanned");
        long_origin["references"][0]["target_origin"] =
            json!(format!("https://{}.example.com", "a".repeat(600)));
        *mock.state.payload.lock().unwrap() = long_origin.clone();
        let result = mock
            .command(&[
                "scope",
                "service-map",
                "body-references",
                "--session-id",
                &session,
                "--evidence-id",
                BODY_REFERENCE_TOKEN,
            ])
            .await;
        assert_eq!(result, long_origin);
        assert_schema(&result, &schema);
        mock.assert_reads(
            "/api/target/service-map/body-references",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
        // Valid references omitted by the origin cap remain counted explicitly.
        let mut capped = body_references_payload("partial");
        capped["reasons"] = json!(["capture_preview_truncated", "origin_limit"]);
        capped["candidates_scanned"] = json!(104);
        capped["scanned_bytes"] = json!(20000);
        capped["retained_preview_bytes"] = json!(20000);
        capped["references"] = json!((0..100)
            .map(|index| json!({
                "target_origin":format!("https://ref{index}.example.com"),"occurrences":1,
                "preview_byte_offsets":[index * 100],"omitted_offsets":0
            }))
            .collect::<Vec<_>>());
        capped["omitted_reference_occurrences"] = json!(4);
        *mock.state.payload.lock().unwrap() = capped.clone();
        let result = mock
            .command(&[
                "scope",
                "service-map",
                "body-references",
                "--session-id",
                &session,
                "--evidence-id",
                BODY_REFERENCE_TOKEN,
            ])
            .await;
        assert_eq!(result, capped);
        mock.assert_reads(
            "/api/target/service-map/body-references",
            &[("evidence_id", BODY_REFERENCE_TOKEN)],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_body_references_reject_misbound_unsafe_or_unbounded_responses() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "scope",
            "service-map",
            "body-references",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        for (pointer, value) in [
            ("/schema_version", json!(2)),
            ("/session_id", json!(Uuid::from_u128(1))),
            ("/evidence_id", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/status", json!("SYNTHETIC_PRIVATE_VALUE")),
            ("/available", json!(false)),
            ("/complete", json!(false)),
            ("/reasons", json!(["SYNTHETIC_PRIVATE_VALUE"])),
            ("/source_sequence", json!("9007199254740992")),
            (
                "/source_origin",
                json!("https://user:SYNTHETIC_PRIVATE_VALUE@example.com"),
            ),
            (
                "/source_origin",
                json!("https://example.com/private?SYNTHETIC_PRIVATE_VALUE"),
            ),
            (
                "/references/0/target_origin",
                json!("https://example.com/private?SYNTHETIC_PRIVATE_VALUE"),
            ),
            (
                "/references/0/target_origin",
                json!("https://user:SYNTHETIC_PRIVATE_VALUE@example.com"),
            ),
            (
                "/references/0/target_origin",
                json!(format!("https://{}.example.com", "a".repeat(4096))),
            ),
            ("/references/0/occurrences", json!(0)),
            ("/references/0/omitted_offsets", json!(1)),
            ("/references/0/preview_byte_offsets", json!([60, 10])),
            ("/references/0/preview_byte_offsets", json!([10, 100])),
            ("/scanned_bytes", json!(262145)),
            ("/retained_preview_bytes", json!(99)),
            ("/candidates_scanned", json!(4097)),
            ("/candidates_scanned", json!(1)),
            ("/omitted_reference_occurrences", json!(15)),
            ("/capture_preview_truncated", json!(true)),
            ("/scan_prefix_truncated", json!(true)),
            ("/coverage", json!("all_runtime_calls")),
            ("/offset_basis", json!("wire_bytes")),
            ("/limits/scan_bytes", json!(262145)),
        ] {
            let mut payload = body_references_payload("scanned");
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/body-references",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
        for field in [
            "source_origin",
            "source_sequence",
            "source_started_at",
            "retained_preview_bytes",
        ] {
            let mut payload = body_references_payload("scanned");
            payload.as_object_mut().unwrap().remove(field);
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
            mock.assert_reads(
                "/api/target/service-map/body-references",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_body_reference_http_failures_do_not_retry_follow_or_echo() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = [
            "target",
            "service-map",
            "body-references",
            "--session-id",
            &session,
            "--evidence-id",
            BODY_REFERENCE_TOKEN,
        ];
        for (status, code, exit) in [(302, "REDIRECT_REFUSED", 5), (503, "HTTP_STATUS_ERROR", 6)] {
            *mock.state.read_status.lock().unwrap() = status;
            *mock.state.payload.lock().unwrap() = json!({"error":"SYNTHETIC_PRIVATE_VALUE"});
            let result = mock.command_with_code(&args, exit).await;
            assert_eq!(result["error"]["code"], code);
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/body-references",
                &[("evidence_id", BODY_REFERENCE_TOKEN)],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_reads_match_schema_and_share_direct_call_results() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.get");
        *mock.state.payload.lock().unwrap() = map_payload();
        let direct = mock
            .command(&["target", "service-map", "--session-id", &session])
            .await;
        assert_schema(&direct, &schema);
        assert_eq!(direct, map_payload());
        assert_eq!(direct["origins"][0]["endpoints"][0]["method"], "get");
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
        let input = json!({"session_id":session}).to_string();
        let called = mock
            .command(&["call", "target.service_map.get", "--input", &input])
            .await;
        assert_eq!(called["data"], direct);
        assert_eq!(called["meta"]["session_id"], session);
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
        let mut filtered = map_payload();
        filtered["in_scope_only"] = json!(true);
        filtered["origin_filter"] = json!("https://example.com");
        filtered["snapshot"]["limits"]["max_records"] = json!(25);
        *mock.state.payload.lock().unwrap() = filtered.clone();
        let filtered_result = mock
            .command(&[
                "scope",
                "service-map",
                "--session-id",
                &session,
                "--in-scope-only",
                "--max-records",
                "25",
                "--origin",
                "https://example.com",
            ])
            .await;
        assert_eq!(filtered_result, filtered);
        assert_schema(&filtered_result, &schema);
        mock.assert_reads(
            "/api/target/service-map",
            &[
                ("in_scope_only", "true"),
                ("max_records", "25"),
                ("origin", "https://example.com"),
            ],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_evidence_preserves_unavailable_and_exact_sequence() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.evidence.get");
        for available in [true, false] {
            let payload = evidence_payload(available);
            *mock.state.payload.lock().unwrap() = payload.clone();
            let result = mock
                .command(&[
                    "scope",
                    "service-map-evidence",
                    "--session-id",
                    &session,
                    "--evidence-id",
                    "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000",
                ])
                .await;
            assert_eq!(result, payload);
            assert_schema(&result, &schema);
            if available {
                assert_eq!(result["row"]["sequence"], "9007199254740993");
            }
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000")],
            );
            let input = json!({"session_id":session,"evidence_id":"v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000"}).to_string();
            let called = mock
                .command(&["call", "target.service_map.evidence.get", "--input", &input])
                .await;
            assert_eq!(called["data"], result);
            assert_eq!(called["meta"]["session_id"], session);
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000")],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_evidence_unavailable_reasons_are_known_codes_and_private() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let schema = output_schema("target.service_map.evidence.get");
        for (reason, valid) in [
            ("invalid_evidence_id", true),
            ("session_mismatch", true),
            ("stale_generation", true),
            ("record_missing", true),
            ("record_changed", true),
            ("metadata_unavailable", true),
            ("session_source_changed", true),
            ("", false),
            ("unknown_reason", false),
            ("store_generation_changed", false),
            ("SYNTHETIC_PRIVATE_VALUE diagnostic", false),
            (
                "https://user:SYNTHETIC_PRIVATE_VALUE@example.com/?token=SYNTHETIC_PRIVATE_VALUE",
                false,
            ),
        ] {
            let mut payload = evidence_payload(false);
            payload["reason"] = json!(reason);
            *mock.state.payload.lock().unwrap() = payload.clone();
            for call in [false, true] {
                let input =
                    json!({"session_id":session,"evidence_id":BODY_REFERENCE_TOKEN}).to_string();
                let args = if call {
                    vec!["call", "target.service_map.evidence.get", "--input", &input]
                } else {
                    vec![
                        "scope",
                        "service-map-evidence",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        BODY_REFERENCE_TOKEN,
                    ]
                };
                let result = mock
                    .command_with_code(&args, if valid { 0 } else { 5 })
                    .await;
                if valid {
                    let data = if call { &result["data"] } else { &result };
                    assert_eq!(data, &payload);
                    assert_schema(data, &schema);
                } else {
                    assert_eq!(result["error"]["code"], "INVALID_RESPONSE");
                    assert_eq!(result["error"]["retryable"], false);
                    assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
                }
                mock.assert_reads(
                    "/api/target/service-map/evidence",
                    &[("evidence_id", BODY_REFERENCE_TOKEN)],
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_rejects_misbound_or_unsafe_responses_without_echoing_data() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        let args = ["scope", "service-map", "--session-id", &session];
        for (pointer, value) in [
            ("/schema_version", json!(2)),
            ("/session_id", json!(Uuid::from_u128(1))),
            ("/in_scope_only", json!(true)),
            ("/origin_filter", json!("https://other.example.com")),
            ("/snapshot/limits/max_records", json!(20)),
            ("/snapshot/origin_count", json!(2)),
            ("/snapshot/complete", json!(false)),
            (
                "/origins/0/endpoints/0/path",
                json!("/items/42?token=SYNTHETIC_PRIVATE_VALUE"),
            ),
            (
                "/origins/0/redirects/0/target_origin",
                json!("https://user:SYNTHETIC_PRIVATE_VALUE@example.com"),
            ),
            (
                "/origins/0/technology_clues/0/confidence",
                json!("inferred_backend_chain"),
            ),
        ] {
            let mut payload = map_payload();
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock.command_with_code(&args, 5).await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
        let mut payload = map_payload();
        payload["unknown_sensitive_data"] = json!("SYNTHETIC_PRIVATE_VALUE");
        payload["origins"][0]["endpoints"][0]["raw_headers"] = json!("SYNTHETIC_PRIVATE_VALUE");
        *mock.state.payload.lock().unwrap() = payload;
        let result = mock.command(&args).await;
        assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
        assert_schema(&result, &output_schema("target.service_map.get"));
        mock.assert_reads(
            "/api/target/service-map",
            &[("in_scope_only", "false"), ("max_records", "10000")],
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_evidence_rejects_ambiguous_or_substituted_rows() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        for (pointer, value) in [
            ("/session_id", json!(Uuid::from_u128(1))),
            ("/evidence_id", json!("different-token")),
            ("/row/evidence_id", json!("different-row-token")),
            ("/row", Value::Null),
            ("/available", json!(false)),
            ("/reason", json!("stale")),
            ("/row/sequence", json!("09007199254740993")),
            ("/row/path", json!("/items?token=SYNTHETIC_PRIVATE_VALUE")),
        ] {
            let mut payload = evidence_payload(true);
            *payload.pointer_mut(pointer).unwrap() = value;
            *mock.state.payload.lock().unwrap() = payload;
            let result = mock
                .command_with_code(
                    &[
                        "scope",
                        "service-map-evidence",
                        "--session-id",
                        &session,
                        "--evidence-id",
                        "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000",
                    ],
                    5,
                )
                .await;
            assert_eq!(result["error"]["code"], "INVALID_RESPONSE", "{pointer}");
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map/evidence",
                &[("evidence_id", "v1.00000000-0000-0000-0000-000000000000.22222222-2222-2222-2222-222222222222.9007199254740993.33333333-3333-3333-3333-333333333333.00000000-0000-0000-0000-000000000000")],
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_map_refuses_redirects_and_does_not_echo_error_bodies() {
        let mock = MockApi::new().await;
        let session = Uuid::nil().to_string();
        *mock.state.payload.lock().unwrap() = json!({"error":"SYNTHETIC_PRIVATE_VALUE"});
        for (status, code, exit) in [
            (302, "REDIRECT_REFUSED", 5),
            (404, "HTTP_STATUS_ERROR", 5),
            (500, "HTTP_STATUS_ERROR", 6),
        ] {
            *mock.state.read_status.lock().unwrap() = status;
            let result = mock
                .command_with_code(&["scope", "service-map", "--session-id", &session], exit)
                .await;
            assert_eq!(result["error"]["code"], code);
            assert_eq!(result["error"]["details"]["status"], status);
            assert_eq!(result["error"]["retryable"], false);
            assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_VALUE"));
            mock.assert_reads(
                "/api/target/service-map",
                &[("in_scope_only", "false"), ("max_records", "10000")],
            );
        }
    }
}
