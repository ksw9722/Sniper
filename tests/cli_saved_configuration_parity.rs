//! Saved metadata only. The loopback mock has no proxy or upstream capability;
//! every request must be discovery, a selective rule PATCH, or a label update.
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Json, Router,
};
use serde_json::{json, Value};
use sniper::workspace::{ReplayTabState, WorkspaceStateSnapshot};
use uuid::Uuid;

const UPDATE: &str = "capture.intercept_rule.update";
const ENABLE: &str = "capture.intercept_rule.set_enabled";
const LABEL: &str = "replay.update";
const TAB: &str = "0e3dd0e5-4103-4dd3-9be1-b72ec18cc2a8";

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    body: Option<Value>,
}

#[derive(Default)]
struct Faults {
    no_active: bool,
    switch_after_sessions: bool,
    switch_after_workspace: bool,
    stale_revision: bool,
    get_body: Option<Value>,
    write_response: Option<(StatusCode, Value)>,
    redirect: Option<StatusCode>,
}

struct MockState {
    settings: Value,
    ids: [Uuid; 2],
    rule_id: Uuid,
    active: Mutex<Uuid>,
    rules: Mutex<BTreeMap<Uuid, Value>>,
    workspaces: Mutex<BTreeMap<Uuid, Value>>,
    requests: Mutex<Vec<RecordedRequest>>,
    faults: Mutex<Faults>,
}

struct MockApi {
    api: String,
    dir: PathBuf,
    state: Arc<MockState>,
    server: tokio::task::JoinHandle<()>,
}

fn saved_rule(id: Uuid) -> Value {
    json!({"id":id,"enabled":true,"scope":"both","host_pattern":"example.com",
        "path_pattern":"/synthetic","method_filter":["GET","POST"]})
}

fn workspace(id: Uuid) -> Value {
    let tab: ReplayTabState = serde_json::from_value(json!({
        "id":TAB,"type":"websocket","sequence":12,"custom_label":"Original label",
        "pinned":true,"notice":"synthetic saved notice","request_text":"unparsed saved draft",
        "source_transaction_id":Uuid::new_v4(),
        "target_scheme":"https","target_host":"example.com","target_port":"443",
        "target_manually_edited":true,
        "history_entries":[{"request_text":"unparsed saved history","notice":"history notice"}],
        "history_index":0,"ws_scheme":"wss","ws_host":"example.com","ws_port":443,
        "ws_path":"/saved-only","ws_headers":[{"name":"X-Example","value":"fixture"}],
        "ws_handshake_text":"unparsed saved handshake","ws_handshake_edited":true,
        "ws_editor_text":"saved editor","ws_message_type":"binary","ws_editor_body_encoded":true,
        "ws_setup_notice":"saved setup","ws_setup_queue":[{"synthetic":"saved queue item"}],
        "ws_frames":[{"index":7,"captured_at":"2026-10-10T00:00:00Z",
            "direction":"server_to_client","kind":"text","body":"saved frame",
            "body_encoding":"utf8","body_size":11,"preview_truncated":false}],
        "ws_frames_truncated":true,"ws_selected_frame_index":7,"ws_frame_window_start":7
    }))
    .unwrap();
    let mut snapshot = WorkspaceStateSnapshot {
        session_id: Some(id),
        revision: 17,
        client_id: Some("synthetic-desktop".into()),
        client_version: 9,
        ..Default::default()
    };
    snapshot.replay.active_tab_id = Some("other".into());
    snapshot.replay.tab_sequence = 20;
    snapshot.replay.tabs = vec![
        ReplayTabState {
            id: "other".into(),
            tab_type: "http".into(),
            sequence: 20,
            custom_label: "Unrelated label".into(),
            ..Default::default()
        },
        tab,
    ];
    snapshot.fuzzer.request_text = "unparsed unrelated saved draft".into();
    serde_json::to_value(snapshot).unwrap()
}

fn session_summary(id: Uuid, active: bool) -> Value {
    json!({"id":id,"name":"Synthetic session","created_at":"2026-10-10T00:00:00Z",
        "updated_at":"2026-10-10T00:00:00Z","last_opened_at":"2026-10-10T00:00:00Z",
        "request_count":0,"websocket_count":0,"event_count":0,"fuzzer_count":0,
        "rule_count":1,"storage_path":"synthetic-unused-path","active":active})
}

impl MockApi {
    async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("sniper-saved-config-cli-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ids = [Uuid::new_v4(), Uuid::new_v4()];
        let rule_id = Uuid::new_v4();
        let state = Arc::new(MockState {
            settings: json!({"runtime_instance_id":Uuid::new_v4(),"proxy_addr":"127.0.0.1:0",
                "ui_addr":addr.to_string(),"data_dir":dir.to_string_lossy(),"max_entries":100,
                "features":["http_capture","session_storage","replay"]}),
            ids,
            rule_id,
            active: Mutex::new(ids[0]),
            rules: Mutex::new(
                ids.into_iter()
                    .map(|id| (id, saved_rule(rule_id)))
                    .collect(),
            ),
            workspaces: Mutex::new(ids.into_iter().map(|id| (id, workspace(id))).collect()),
            requests: Mutex::new(Vec::new()),
            faults: Mutex::new(Faults::default()),
        });
        let app = Router::new()
            .fallback(any(mock_request))
            .with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            api: format!("http://{addr}"),
            dir,
            state,
            server,
        }
    }

    async fn command(&self, args: &[&str]) -> (i32, Value) {
        self.command_with_pending_stdin(args, false).await
    }

    async fn command_with_pending_stdin(&self, args: &[&str], pending: bool) -> (i32, Value) {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"))
            .env("SNIPER_DATA_DIR", &self.dir)
            .env("HOME", self.dir.join("unused-home"))
            .env("USERPROFILE", self.dir.join("unused-home"))
            .env_remove("SNIPER_API_ADDR")
            .args(["--output", "compact", "--api", &self.api])
            .args(args)
            .stdin(if pending {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        // Keeping this writer open proves WebSocket rejection does not read stdin.
        let held_stdin = child.stdin.take();
        let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
            .await
            .expect("metadata-only CLI waited for unavailable input")
            .unwrap();
        drop(held_stdin);
        let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "invalid JSON for {args:?}: {error}: {} / {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output.status.code().unwrap(), value)
    }

    async fn call(&self, operation: &str, input: Value, flag: Option<&str>) -> (i32, Value) {
        let encoded = input.to_string();
        let mut args = vec!["call", operation, "--input", &encoded];
        if let Some(flag) = flag {
            args.push(flag);
        }
        let result = self.command(&args).await;
        if result.0 == 0 && result.1["data"]["dry_run"] != true {
            if let Some(directory) = std::env::var_os("SNIPER_SCHEMA_CASES_DIR") {
                let directory = PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(
                    directory.join(format!("{}.json", Uuid::new_v4())),
                    serde_json::to_vec(
                        &json!({"operation":operation,"input":input,"data":result.1["data"]}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
        }
        result
    }

    fn take_requests(&self) -> Vec<RecordedRequest> {
        std::mem::take(&mut *self.state.requests.lock().unwrap())
    }

    fn assert_requests(&self, id: Uuid, implicit: bool, workspace: bool, write: Option<Value>) {
        let requests = self.take_requests();
        let patch_path = format!("/api/intercept-rules/{}", self.state.rule_id);
        let mut expected = vec![("GET", "/api/settings")];
        if implicit {
            expected.push(("GET", "/api/sessions"));
        }
        if workspace {
            expected.push(("GET", "/api/workspace-state"));
        }
        if write.is_some() {
            expected.push(if workspace {
                ("POST", "/api/replay/tabs/set-label")
            } else {
                ("PATCH", &patch_path)
            });
        }
        assert_eq!(
            requests
                .iter()
                .map(|r| (r.method.as_str(), r.path.as_str()))
                .collect::<Vec<_>>(),
            expected,
            "{requests:?}"
        );
        for request in requests {
            let mut query = BTreeMap::new();
            if request.path == "/api/workspace-state" || request.method == "PATCH" {
                query.insert("session_id".into(), id.to_string());
            }
            if request.method == "PATCH" && implicit {
                query.insert("expected_active_session_id".into(), id.to_string());
            }
            assert_eq!(request.query, query, "{request:?}");
            assert_eq!(
                request.body,
                if request.method == "GET" {
                    None
                } else {
                    write.clone()
                },
                "{request:?}"
            );
        }
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn mock_request(State(state): State<Arc<MockState>>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, 64 * 1024).await.unwrap();
    let body: Option<Value> = (!bytes.is_empty()).then(|| serde_json::from_slice(&bytes).unwrap());
    let query: BTreeMap<String, String> =
        url::form_urlencoded::parse(parts.uri.query().unwrap_or_default().as_bytes())
            .into_owned()
            .collect();
    state.requests.lock().unwrap().push(RecordedRequest {
        method: parts.method.to_string(),
        path: parts.uri.path().into(),
        query: query.clone(),
        body: body.clone(),
    });
    let faults = state.faults.lock().unwrap();
    match (parts.method.as_str(), parts.uri.path()) {
        ("GET", "/api/settings") => Json(state.settings.clone()).into_response(),
        ("GET", "/api/sessions") => {
            let active = *state.active.lock().unwrap();
            let rows: Vec<_> = state
                .ids
                .iter()
                .map(|id| session_summary(*id, !faults.no_active && *id == active))
                .collect();
            if faults.switch_after_sessions {
                *state.active.lock().unwrap() = state.ids[1];
            }
            Json(rows).into_response()
        }
        ("GET", "/api/workspace-state") => {
            let id = query
                .get("session_id")
                .and_then(|id| Uuid::parse_str(id).ok())
                .expect("workspace read must pin a session");
            let workspaces = state.workspaces.lock().unwrap();
            let Some(saved) = workspaces.get(&id) else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error":"session not found"})),
                )
                    .into_response();
            };
            if faults.switch_after_workspace {
                *state.active.lock().unwrap() = state.ids[1];
            }
            Json(faults.get_body.clone().unwrap_or_else(|| saved.clone())).into_response()
        }
        ("PATCH", path) if path.starts_with("/api/intercept-rules/") => {
            let id = query
                .get("session_id")
                .and_then(|id| Uuid::parse_str(id).ok())
                .expect("rule patch must pin a session");
            if query
                .get("expected_active_session_id")
                .is_some_and(|guard| *guard != state.active.lock().unwrap().to_string())
            {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"error":"active session changed"})),
                )
                    .into_response();
            }
            if let Some(status) = faults.redirect {
                return (
                    status,
                    [(header::LOCATION, "/forbidden-mutation-redirect")],
                    Json(json!({"error":"synthetic redirect"})),
                )
                    .into_response();
            }
            if let Some((status, response)) = &faults.write_response {
                return (*status, Json(response.clone())).into_response();
            }
            let mut rules = state.rules.lock().unwrap();
            let Some(rule) = rules
                .get_mut(&id)
                .filter(|_| path.ends_with(&state.rule_id.to_string()))
            else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error":"saved rule not found"})),
                )
                    .into_response();
            };
            for (key, value) in body.unwrap().as_object().unwrap() {
                assert!(
                    [
                        "scope",
                        "host_pattern",
                        "path_pattern",
                        "method_filter",
                        "enabled"
                    ]
                    .contains(&key.as_str()),
                    "unexpected rule field {key}"
                );
                rule[key] = value.clone();
            }
            Json(rule.clone()).into_response()
        }
        ("POST", "/api/replay/tabs/set-label") => {
            let body = body.unwrap();
            let id = serde_json::from_value::<Uuid>(body["session_id"].clone()).unwrap();
            if faults.stale_revision
                || body
                    .get("expected_active_session_id")
                    .is_some_and(|guard| *guard != json!(*state.active.lock().unwrap()))
            {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"error":"workspace or active session changed"})),
                )
                    .into_response();
            }
            if let Some(status) = faults.redirect {
                return (
                    status,
                    [(header::LOCATION, "/forbidden-mutation-redirect")],
                    Json(json!({"error":"synthetic redirect"})),
                )
                    .into_response();
            }
            if let Some((status, response)) = &faults.write_response {
                return (*status, Json(response.clone())).into_response();
            }
            let mut workspaces = state.workspaces.lock().unwrap();
            let saved = workspaces.get_mut(&id).unwrap();
            assert_eq!(body["expected_workspace_revision"], saved["revision"]);
            let tab = saved["replay"]["tabs"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|tab| tab["id"] == body["tab_id"])
                .unwrap();
            tab["custom_label"] = body["label"].clone();
            let tab = tab.clone();
            saved["revision"] = json!(saved["revision"].as_u64().unwrap() + 1);
            Json(json!({"session_id":id,"revision":saved["revision"],"tab":tab})).into_response()
        }
        _ => (
            StatusCode::METHOD_NOT_ALLOWED,
            "outside saved-metadata allowlist",
        )
            .into_response(),
    }
}

fn label_payload(id: Uuid, implicit: bool, revision: u64, label: &str) -> Value {
    let mut body =
        json!({"session_id":id,"tab_id":TAB,"expected_workspace_revision":revision,"label":label});
    if implicit {
        body["expected_active_session_id"] = json!(id);
    }
    body
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selective_rule_updates_have_exact_direct_and_call_parity() {
    for implicit in [false, true] {
        let api = MockApi::new().await;
        let id = api.state.ids[usize::from(!implicit)];
        let id_text = id.to_string();
        let rule_text = api.state.rule_id.to_string();
        let cases = [
            (json!({"enabled":false}), vec!["--enabled", "false"]),
            (
                json!({"scope":"response","host_pattern":"","path_pattern":"/changed","method_filter":["GET","PUT"]}),
                vec![
                    "--scope",
                    "response",
                    "--host-pattern",
                    "",
                    "--path-pattern",
                    "/changed",
                    "--method",
                    "GET",
                    "--method",
                    "PUT",
                ],
            ),
            (json!({"method_filter":[]}), vec!["--clear-methods"]),
            (json!({"path_pattern":""}), vec!["--path-pattern", ""]),
        ];
        for (patch, flags) in cases {
            let mut args = vec![
                "capture",
                "intercept-rule",
                "update",
                "--id",
                &rule_text,
                "--yes",
            ];
            args.extend(flags);
            let mut input = patch.clone();
            input["id"] = json!(api.state.rule_id);
            if !implicit {
                args.extend(["--session-id", &id_text]);
                input["session_id"] = json!(id);
            }
            let (code, direct) = api.command(&args).await;
            assert_eq!(code, 0, "{direct}");
            api.assert_requests(id, implicit, false, Some(patch.clone()));
            let (code, called) = api.call(UPDATE, input, Some("--yes")).await;
            assert_eq!(code, 0, "{called}");
            assert_eq!(called["data"], direct);
            assert_eq!(called["data"]["session_id"], json!(id));
            let mut expected = api.state.rules.lock().unwrap()[&id].clone();
            expected["session_id"] = json!(id);
            assert_eq!(direct, expected);
            api.assert_requests(id, implicit, false, Some(patch));
        }
        assert_eq!(
            api.state.rules.lock().unwrap()[&api.state.ids[usize::from(implicit)]],
            saved_rule(api.state.rule_id)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enabled_is_an_explicit_idempotent_setter_and_clear_false_preserves_methods() {
    for direct in [false, true] {
        for enabled in [false, true] {
            let api = MockApi::new().await;
            let id = api.state.ids[0];
            let rule_text = api.state.rule_id.to_string();
            for _ in 0..2 {
                let (code, result) = if direct {
                    api.command(&[
                        "capture",
                        "intercept-rule",
                        "set-enabled",
                        "--id",
                        &rule_text,
                        "--enabled",
                        if enabled { "true" } else { "false" },
                        "--yes",
                    ])
                    .await
                } else {
                    api.call(
                        ENABLE,
                        json!({"id":api.state.rule_id,"enabled":enabled}),
                        Some("--yes"),
                    )
                    .await
                };
                assert_eq!(code, 0, "{result}");
                let data = if direct { &result } else { &result["data"] };
                let mut expected = saved_rule(api.state.rule_id);
                expected["enabled"] = json!(enabled);
                expected["session_id"] = json!(id);
                assert_eq!(*data, expected);
                api.assert_requests(id, true, false, Some(json!({"enabled":enabled})));
            }
            let (code, result) = api
                .call(
                    UPDATE,
                    json!({"id":api.state.rule_id,"clear_methods":false,"enabled":false}),
                    Some("--yes"),
                )
                .await;
            assert_eq!(code, 0, "{result}");
            assert_eq!(result["data"]["method_filter"], json!(["GET", "POST"]));
            api.assert_requests(id, true, false, Some(json!({"enabled":false})));
            let (code, result) = api
                .call(
                    UPDATE,
                    json!({"id":api.state.rule_id,"clear_methods":true}),
                    Some("--yes"),
                )
                .await;
            assert_eq!(code, 0, "{result}");
            assert_eq!(result["data"]["method_filter"], json!([]));
            api.assert_requests(id, true, false, Some(json!({"method_filter":[]})));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rule_validation_confirmation_dry_run_and_discovery_stay_offline() {
    let api = MockApi::new().await;
    let id = api.state.rule_id;
    let id_text = id.to_string();
    for operation in [UPDATE, ENABLE] {
        let input = json!({"id":id,"enabled":false});
        let (code, result) = api.call(operation, input.clone(), None).await;
        assert_ne!(code, 0);
        assert_eq!(result["error"]["code"], "CONFIRMATION_REQUIRED");
        let (code, result) = api.call(operation, input.clone(), Some("--dry-run")).await;
        assert_eq!(code, 0, "{result}");
        assert_eq!(result["data"]["dry_run"], true);
        assert_eq!(result["data"]["input"], input);
        assert_eq!(result["data"]["api"]["method"], "PATCH");
        assert_eq!(
            result["data"]["api"]["path"],
            format!("/api/intercept-rules/{id}")
        );
        for kind in ["input", "output"] {
            let (code, schema) = api.command(&["schema", kind, operation]).await;
            assert_eq!(code, 0, "{schema}");
            assert_eq!(schema["schema"]["additionalProperties"], false);
            assert_eq!(schema["schema"]["properties"]["enabled"]["type"], "boolean");
            assert!(schema["schema"]["properties"]["id"].is_object());
            assert!(schema["schema"]["properties"]["session_id"].is_object());
        }
        let (code, examples) = api.command(&["examples", operation]).await;
        assert_eq!(code, 0, "{examples}");
        assert!(!examples["examples"].as_array().unwrap().is_empty());
        for field in ["id", "session_id", "enabled"] {
            let mut bad = input.clone();
            bad[field] = Value::Null;
            let (code, result) = api.call(operation, bad, Some("--yes")).await;
            assert_ne!(code, 0);
            assert_eq!(result["error"]["code"], "INVALID_INPUT", "{result}");
        }
        for bad in [
            json!({}),
            json!({"id":id}),
            json!({"id":"invalid","enabled":false}),
            json!({"id":id,"enabled":"false"}),
            json!({"id":id,"enabled":false,"unknown":1}),
        ] {
            let (code, result) = api.call(operation, bad, Some("--dry-run")).await;
            assert_ne!(code, 0);
            assert_eq!(result["error"]["code"], "INVALID_INPUT", "{result}");
        }
    }
    for bad in [
        json!({"id":id,"clear_methods":false}),
        json!({"id":id,"clear_methods":true,"method_filter":[]}),
        json!({"id":id,"scope":"invalid"}),
        json!({"id":id,"method_filter":"GET"}),
        json!({"id":id,"method_filter":[1]}),
        json!({"id":id,"method":["GET"]}),
        json!({"id":id,"all":true}),
        json!({"id":id,"clear_methods":"true"}),
    ]
    .into_iter()
    .chain(
        [
            "scope",
            "host_pattern",
            "path_pattern",
            "method_filter",
            "clear_methods",
        ]
        .map(|field| {
            let mut bad = json!({"id":id,"enabled":false});
            bad[field] = Value::Null;
            bad
        }),
    ) {
        for flag in ["--yes", "--dry-run"] {
            let (code, result) = api.call(UPDATE, bad.clone(), Some(flag)).await;
            assert_ne!(code, 0);
            assert_eq!(result["error"]["code"], "INVALID_INPUT", "{bad} {result}");
        }
    }
    for tail in [
        vec!["--yes"],
        vec!["--enabled", "False", "--yes"],
        vec!["--method", "GET", "--clear-methods", "--yes"],
        vec!["--scope", "invalid", "--yes"],
        vec!["--enabled", "false", "--unknown", "--yes"],
    ] {
        let mut args = vec!["capture", "intercept-rule", "update", "--id", &id_text];
        args.extend(tail);
        let (code, result) = api.command(&args).await;
        assert_ne!(code, 0);
        assert_eq!(result["error"]["code"], "INVALID_INPUT", "{result}");
    }
    for action in ["update", "set-enabled"] {
        let args = [
            "capture",
            "intercept-rule",
            action,
            "--id",
            &id_text,
            "--enabled",
            "false",
        ];
        let (code, result) = api.command(&args).await;
        assert_ne!(code, 0);
        assert_eq!(result["error"]["code"], "CONFIRMATION_REQUIRED");
        let mut dry = args.to_vec();
        dry.push("--dry-run");
        let (code, result) = api.command(&dry).await;
        assert_eq!(code, 0, "{result}");
        assert_eq!(result["api"]["method"], "PATCH");
    }
    let (code, manifest) = api.command(&["manifest"]).await;
    assert_eq!(code, 0);
    for operation in [UPDATE, ENABLE] {
        let row = manifest["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["operation"] == operation)
            .unwrap();
        assert_eq!(row["side_effect"], "write");
        assert_eq!(row["requires_confirmation"], true);
    }
    assert!(
        api.take_requests().is_empty(),
        "preflight and discovery must not probe settings"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_rules_write_errors_and_session_switches_never_retry_or_create() {
    for operation in [UPDATE, ENABLE] {
        for fault in [
            "missing",
            "conflict",
            "persistence",
            "active_switch",
            "bad_ack",
        ] {
            let api = MockApi::new().await;
            let id = api.state.ids[0];
            {
                let mut faults = api.state.faults.lock().unwrap();
                match fault {
                    "missing" => {
                        faults.write_response =
                            Some((StatusCode::NOT_FOUND, json!({"error":"rule not found"})))
                    }
                    "conflict" => {
                        faults.write_response =
                            Some((StatusCode::CONFLICT, json!({"error":"session changed"})))
                    }
                    "persistence" => {
                        faults.write_response = Some((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            json!({"error":"save failed"}),
                        ))
                    }
                    "active_switch" => faults.switch_after_sessions = true,
                    "bad_ack" => {
                        faults.write_response = Some((StatusCode::OK, saved_rule(Uuid::new_v4())))
                    }
                    _ => unreachable!(),
                }
            }
            let (code, result) = api
                .call(
                    operation,
                    json!({"id":api.state.rule_id,"enabled":false}),
                    Some("--yes"),
                )
                .await;
            assert_ne!(code, 0, "{result}");
            assert_eq!(result["ok"], false);
            assert_eq!(result["error"]["retryable"], false);
            api.assert_requests(id, true, false, Some(json!({"enabled":false})));
            assert_eq!(
                api.state.rules.lock().unwrap()[&id],
                saved_rule(api.state.rule_id)
            );
        }
        let api = MockApi::new().await;
        api.state.faults.lock().unwrap().no_active = true;
        let (code, result) = api
            .call(
                operation,
                json!({"id":api.state.rule_id,"enabled":false}),
                Some("--yes"),
            )
            .await;
        assert_ne!(code, 0, "{result}");
        api.assert_requests(api.state.ids[0], true, false, None);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn label_only_updates_preserve_every_saved_websocket_field() {
    for implicit in [false, true] {
        for tab_type in ["websocket", "http", ""] {
            let api = MockApi::new().await;
            let id = api.state.ids[usize::from(!implicit)];
            let id_text = id.to_string();
            api.state.workspaces.lock().unwrap().get_mut(&id).unwrap()["replay"]["tabs"][1]
                ["type"] = json!(tab_type);
            let before = api.state.workspaces.lock().unwrap()[&id].clone();
            let other_before =
                api.state.workspaces.lock().unwrap()[&api.state.ids[usize::from(implicit)]].clone();
            for (index, raw_label) in ["  New\n label \t", ""].into_iter().enumerate() {
                let normalized = if index == 0 { "New label" } else { "" };
                let revision = 17 + (index as u64 * 2);
                let mut args = vec![
                    "replay", "update", "--tab-id", TAB, "--label", raw_label, "--yes",
                ];
                let mut input = json!({"tab_id":TAB,"label":raw_label});
                if !implicit {
                    args.extend(["--session-id", &id_text]);
                    input["session_id"] = json!(id);
                }
                let (code, direct) = api.command(&args).await;
                assert_eq!(code, 0, "{direct}");
                api.assert_requests(
                    id,
                    implicit,
                    true,
                    Some(label_payload(id, implicit, revision, normalized)),
                );
                let (code, called) = api.call(LABEL, input, Some("--yes")).await;
                assert_eq!(code, 0, "{called}");
                assert_eq!(called["data"], direct);
                assert_eq!(called["data"]["session_id"], json!(id));
                let mut expected_tab = before["replay"]["tabs"][1].clone();
                expected_tab["custom_label"] = json!(normalized);
                expected_tab["session_id"] = json!(id);
                assert_eq!(direct, expected_tab);
                api.assert_requests(
                    id,
                    implicit,
                    true,
                    Some(label_payload(id, implicit, revision + 1, normalized)),
                );
                let mut expected_workspace = before.clone();
                expected_workspace["revision"] = json!(revision + 2);
                expected_workspace["replay"]["tabs"][1]["custom_label"] = json!(normalized);
                assert_eq!(
                    api.state.workspaces.lock().unwrap()[&id],
                    expected_workspace
                );
            }
            assert_eq!(
                api.state.workspaces.lock().unwrap()[&api.state.ids[usize::from(implicit)]],
                other_before
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn label_dry_runs_and_bad_preflight_never_touch_the_network() {
    let api = MockApi::new().await;
    let input = json!({"tab_id":TAB,"label":"New label"});
    let (code, result) = api.call(LABEL, input.clone(), None).await;
    assert_ne!(code, 0);
    assert_eq!(result["error"]["code"], "CONFIRMATION_REQUIRED");
    let (code, result) = api.call(LABEL, input, Some("--dry-run")).await;
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["api"]["path"], "/api/replay/tabs/set-label");
    let (code, direct) = api
        .command(&[
            "replay",
            "update",
            "--tab-id",
            TAB,
            "--label",
            "New label",
            "--dry-run",
        ])
        .await;
    assert_eq!(code, 0, "{direct}");
    assert_eq!(direct["api"]["path"], "/api/replay/tabs/set-label");
    let absent = api.dir.join("must-not-be-opened");
    let (code, mixed) = api
        .call(
            LABEL,
            json!({"tab_id":TAB,"label":"New label","request_file":absent}),
            Some("--dry-run"),
        )
        .await;
    assert_eq!(code, 0, "{mixed}");
    assert_eq!(mixed["data"]["api"]["path"], "/api/workspace-state");
    for bad in [
        json!({}),
        json!({"tab_id":TAB}),
        json!({"tab_id":TAB,"label":null}),
        json!({"tab_id":TAB,"label":false}),
        json!({"tab_id":TAB,"label":"X","unknown":true}),
    ] {
        let (code, result) = api.call(LABEL, bad, Some("--yes")).await;
        assert_ne!(code, 0);
        assert_eq!(result["error"]["code"], "INVALID_INPUT", "{result}");
    }
    assert!(api.take_requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixed_websocket_updates_reject_before_opening_request_file_or_reading_stdin() {
    for direct in [false, true] {
        for stdin in [false, true] {
            let api = MockApi::new().await;
            let id = api.state.ids[0];
            let before = api.state.workspaces.lock().unwrap()[&id].clone();
            let absent = api.dir.join("must-not-be-opened");
            let path = absent.to_str().unwrap();
            let encoded = if stdin {
                json!({"tab_id":TAB,"label":"New label","stdin":true})
            } else {
                json!({"tab_id":TAB,"label":"New label","request_file":path})
            }
            .to_string();
            let mut args = if direct {
                vec![
                    "replay",
                    "update",
                    "--tab-id",
                    TAB,
                    "--label",
                    "New label",
                    "--yes",
                ]
            } else {
                vec!["call", LABEL, "--input", &encoded, "--yes"]
            };
            if direct {
                if stdin {
                    args.push("--stdin");
                } else {
                    args.extend(["--request-file", path]);
                }
            }
            let (code, result) = api.command_with_pending_stdin(&args, stdin).await;
            assert_ne!(code, 0, "{result}");
            assert!(
                result["error"]["message"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("websocket"),
                "{result}"
            );
            api.assert_requests(id, true, true, None);
            assert_eq!(api.state.workspaces.lock().unwrap()[&id], before);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn label_conflicts_missing_tabs_and_invalid_acknowledgements_never_retry() {
    for fault in [
        "revision",
        "active_switch",
        "missing",
        "persistence",
        "bad_ack",
    ] {
        let api = MockApi::new().await;
        let id = api.state.ids[0];
        if fault == "missing" {
            api.state.workspaces.lock().unwrap().get_mut(&id).unwrap()["replay"]["tabs"] =
                json!([]);
        }
        let before = api.state.workspaces.lock().unwrap()[&id].clone();
        {
            let mut faults = api.state.faults.lock().unwrap();
            match fault {
                "revision" => faults.stale_revision = true,
                "active_switch" => faults.switch_after_workspace = true,
                "persistence" => {
                    faults.write_response = Some((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        json!({"error":"save failed"}),
                    ))
                }
                "bad_ack" => {
                    faults.write_response = Some((
                        StatusCode::OK,
                        json!({"session_id":id,"revision":18,"tab":{}}),
                    ))
                }
                "missing" => (),
                _ => unreachable!(),
            }
        }
        let (code, result) = api
            .call(
                LABEL,
                json!({"tab_id":TAB,"label":"New label"}),
                Some("--yes"),
            )
            .await;
        assert_ne!(code, 0, "{result}");
        assert_eq!(result["error"]["retryable"], false);
        api.assert_requests(
            id,
            true,
            true,
            (fault != "missing").then(|| label_payload(id, true, 17, "New label")),
        );
        assert_eq!(api.state.workspaces.lock().unwrap()[&id], before);
    }
    let api = MockApi::new().await;
    api.state.faults.lock().unwrap().get_body =
        Some(json!({"revision":"invalid","replay":{"tabs":[]}}));
    let (code, result) = api
        .call(
            LABEL,
            json!({"tab_id":TAB,"label":"New label"}),
            Some("--yes"),
        )
        .await;
    assert_ne!(code, 0, "{result}");
    api.assert_requests(api.state.ids[0], true, true, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn label_reads_cannot_retarget_the_selected_session() {
    for implicit in [false, true] {
        for returned_session in [Some(Uuid::new_v4()), None] {
            let api = MockApi::new().await;
            let id = api.state.ids[usize::from(!implicit)];
            let before = api.state.workspaces.lock().unwrap().clone();
            let mut wrong = before[&id].clone();
            if let Some(returned_session) = returned_session {
                wrong["session_id"] = json!(returned_session);
            } else {
                wrong.as_object_mut().unwrap().remove("session_id");
            }
            api.state.faults.lock().unwrap().get_body = Some(wrong);
            let mut input = json!({"tab_id":TAB,"label":"New label"});
            if !implicit {
                input["session_id"] = json!(id);
            }
            let (code, result) = api.call(LABEL, input, Some("--yes")).await;
            assert_ne!(code, 0, "{result}");
            api.assert_requests(id, implicit, true, None);
            assert_eq!(*api.state.workspaces.lock().unwrap(), before);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_rule_uuid_only_attempts_one_patch_and_never_creates() {
    let api = MockApi::new().await;
    let id = api.state.ids[1];
    let missing = Uuid::new_v4();
    let (code, result) = api
        .call(
            UPDATE,
            json!({"id":missing,"session_id":id,"enabled":false}),
            Some("--yes"),
        )
        .await;
    assert_ne!(code, 0, "{result}");
    assert_eq!(result["error"]["details"]["status"], 404);
    assert_eq!(result["error"]["retryable"], false);
    let requests = api.take_requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("GET", "/api/settings")
    );
    assert_eq!(requests[1].method, "PATCH");
    assert_eq!(requests[1].path, format!("/api/intercept-rules/{missing}"));
    assert_eq!(
        requests[1].query,
        BTreeMap::from([("session_id".into(), id.to_string())])
    );
    assert_eq!(requests[1].body, Some(json!({"enabled":false})));
    assert_eq!(
        api.state.rules.lock().unwrap()[&id],
        saved_rule(api.state.rule_id)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_writes_refuse_redirects_without_following_or_retrying() {
    for operation in [UPDATE, ENABLE, LABEL] {
        for status in [
            StatusCode::TEMPORARY_REDIRECT,
            StatusCode::PERMANENT_REDIRECT,
            StatusCode::SEE_OTHER,
        ] {
            let api = MockApi::new().await;
            let id = api.state.ids[0];
            let before_rules = api.state.rules.lock().unwrap().clone();
            let before_workspaces = api.state.workspaces.lock().unwrap().clone();
            api.state.faults.lock().unwrap().redirect = Some(status);
            let label = operation == LABEL;
            let input = if label {
                json!({"tab_id":TAB,"label":"New label"})
            } else {
                json!({"id":api.state.rule_id,"enabled":false})
            };
            let (code, result) = api.call(operation, input, Some("--yes")).await;
            assert_ne!(code, 0, "{operation} {status} {result}");
            assert_eq!(result["ok"], false);
            assert_eq!(result["error"]["retryable"], false);
            api.assert_requests(
                id,
                true,
                label,
                Some(if label {
                    label_payload(id, true, 17, "New label")
                } else {
                    json!({"enabled":false})
                }),
            );
            assert_eq!(*api.state.rules.lock().unwrap(), before_rules);
            assert_eq!(*api.state.workspaces.lock().unwrap(), before_workspaces);
        }
    }
}
