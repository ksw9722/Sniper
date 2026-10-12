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
            "pattern",
            "format",
            "allOf",
            "anyOf",
            "oneOf",
            "x-sniper-json-number-representation",
        ],
        &["object", "array", "string", "integer", "boolean", "null"],
        &["uuid", "date-time"],
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
            assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
            assert!(
                !self.unused_dir.exists(),
                "read-only fixture created a data directory"
            );
            serde_json::from_slice(&output.stdout).unwrap()
        }

        fn assert_reads(&self, path: &str, expected_query: &[(&str, &str)]) {
            let requests = std::mem::take(&mut *self.state.requests.lock().unwrap());
            assert_eq!(requests.len(), 2, "{requests:?}");
            assert_eq!(requests[0].path, "/api/settings", "{requests:?}");
            assert!(requests[0].query.is_empty(), "{requests:?}");
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
            "/api/target/service-map" | "/api/target/service-map/evidence" => {
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
        assert!(output.stderr.is_empty(), "{output:?}");
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
            "available":available,"reason":if available {Value::Null} else {json!("store_generation_changed")},
            "source":"captured_http_metadata","row":if available {row} else {Value::Null}})
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
