//! Retention metadata only: a synthetic loopback server, isolated empty paths,
//! and no runtime, capture, proxy, upstream, scanner, or user data access.
use serde_json::{json, Value};
use sniper::history_retention::{history_retention_config_token, HistoryRetentionPolicy};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const SESSION: &str = "d33ca670-10da-4ed0-a401-19c6071cfae0";
const OTHER: &str = "e77cc140-68fe-4bc3-910a-c0ce9cf55c86";
const PATH: &str = "/api/transactions/retention";
const PRIVATE: &str = "synthetic-private-diagnostic";

fn off() -> Value {
    json!({"mode":"off"})
}
fn keep(n: u32) -> Value {
    json!({"mode":"keep_newest","max_entries":n})
}
fn token(id: &str, policy: &Value, revision: u64) -> String {
    let typed: HistoryRetentionPolicy = serde_json::from_value(policy.clone()).unwrap();
    history_retention_config_token(Uuid::parse_str(id).unwrap(), &typed, revision)
}
fn snapshot(id: &str, policy: Value) -> Value {
    snapshot_at_revision(id, policy, 0)
}
fn snapshot_at_revision(id: &str, policy: Value, revision: u64) -> Value {
    json!({"session_id":id,"config_token":token(id, &policy, revision),"config_revision":revision.to_string(),"policy":policy,
        "status":{"total_count":12,"protected_count":4,"pending":false,"disk_cleanup_pending":false,"blocked_reason":null}})
}
fn summary(id: &str, active: bool) -> Value {
    json!({"id":id,"name":"Synthetic metadata","created_at":"2026-10-11T00:00:00Z",
        "updated_at":"2026-10-11T00:00:00Z","last_opened_at":"2026-10-11T00:00:00Z",
        "request_count":0,"websocket_count":0,"event_count":0,"fuzzer_count":0,
        "rule_count":0,"storage_path":"synthetic-unused-path","active":active})
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    body: Option<Value>,
}
#[derive(Clone)]
enum Reply {
    Http(u16, String),
    Incomplete(u16, String, bool),
    Redirect(u16),
    Disconnect,
}
impl Reply {
    fn json(value: Value) -> Self {
        Self::Http(200, value.to_string())
    }
}
#[derive(Default)]
struct Faults {
    get: Option<Reply>,
    post: Option<Reply>,
    sessions: Option<Value>,
    post_expected_revision: Option<u64>,
}
struct Fixture {
    api: String,
    root: PathBuf,
    requests: Arc<Mutex<Vec<Request>>>,
    faults: Arc<Mutex<Faults>>,
    release: Arc<tokio::sync::Notify>,
    server: tokio::task::JoinHandle<()>,
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Request {
    let mut bytes = Vec::new();
    let header_end = loop {
        assert!(bytes.len() < 16 * 1024);
        assert_ne!(stream.read_buf(&mut bytes).await.unwrap(), 0);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    let mut lines = headers.lines();
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let raw_path = first.next().unwrap().to_owned();
    let (path, query) = raw_path.split_once('?').unwrap_or((&raw_path, ""));
    let path = path.to_owned();
    let query = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(length < 4096);
    while bytes.len() < header_end + length {
        assert_ne!(stream.read_buf(&mut bytes).await.unwrap(), 0);
    }
    let body = (length > 0)
        .then(|| serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap());
    Request {
        method,
        path,
        query,
        body,
    }
}

impl Fixture {
    async fn new(initial: Value) -> Self {
        let root = std::env::temp_dir().join(format!("sniper-retention-cli-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let settings = json!({"runtime_instance_id":OTHER,"proxy_addr":"127.0.0.1:0","ui_addr":addr.to_string(),
            "data_dir":root.join("data").to_string_lossy(),"max_entries":100,"features":["http_capture","session_storage","replay"]});
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let faults = Arc::new(Mutex::new(Faults::default()));
        let server_faults = faults.clone();
        let release = Arc::new(tokio::sync::Notify::new());
        let server_release = release.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request =
                    tokio::time::timeout(Duration::from_secs(5), read_request(&mut stream))
                        .await
                        .unwrap();
                recorded.lock().unwrap().push(request.clone());
                let reply = {
                    let faults = server_faults.lock().unwrap();
                    match (request.method.as_str(), request.path.as_str()) {
                        ("GET", "/api/settings") => Reply::json(settings.clone()),
                        ("GET", "/api/sessions") => {
                            Reply::json(faults.sessions.clone().unwrap_or_else(|| {
                                json!([summary(SESSION, true), summary(OTHER, false)])
                            }))
                        }
                        ("GET", PATH) => faults.get.clone().unwrap_or_else(|| {
                            let id = request
                                .query
                                .get("session_id")
                                .expect("session must be pinned");
                            Reply::json(snapshot(id, initial.clone()))
                        }),
                        ("POST", PATH) => faults.post.clone().unwrap_or_else(|| {
                            let id = request
                                .query
                                .get("session_id")
                                .expect("session must be pinned");
                            let policy = &request.body.as_ref().unwrap()["policy"];
                            if let Some(revision) = faults.post_expected_revision {
                                let supplied =
                                    &request.body.as_ref().unwrap()["expected_config_token"];
                                if supplied != &json!(token(id, &initial, revision)) {
                                    return Reply::Http(409, PRIVATE.to_owned());
                                }
                            }
                            let mut value = snapshot_at_revision(
                                id,
                                policy.clone(),
                                u64::from(*policy != initial),
                            );
                            value["changed"] = json!(*policy != initial);
                            Reply::json(value)
                        }),
                        _ => Reply::Http(405, "outside synthetic metadata allowlist".into()),
                    }
                };
                let (status, body, location, incomplete, hold) = match reply {
                    Reply::Http(status, body) => (status, body, String::new(), false, false),
                    Reply::Incomplete(status, body, hold) => {
                        (status, body, String::new(), true, hold)
                    }
                    Reply::Redirect(status) => (
                        status,
                        "{}".into(),
                        "Location: /synthetic-redirect-target\r\n".into(),
                        false,
                        false,
                    ),
                    Reply::Disconnect => continue,
                };
                let response=format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n{body}",body.len()+usize::from(incomplete)*100);
                let _ = stream.write_all(response.as_bytes()).await;
                if hold {
                    server_release.notified().await;
                }
                let _ = stream.shutdown().await;
            }
        });
        Self {
            api: format!("http://{addr}"),
            root,
            requests,
            faults,
            release,
            server,
        }
    }

    async fn output(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        cmd.env_clear()
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("SNIPER_DATA_DIR", self.root.join("data"))
            .current_dir(&self.root)
            .args(["--output", "compact", "--api", &self.api])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            cmd.env("SystemRoot", root);
        }
        let output = tokio::time::timeout(
            Duration::from_secs(8),
            cmd.spawn().unwrap().wait_with_output(),
        )
        .await
        .expect("retention metadata command stalled")
        .unwrap();
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        assert_eq!(
            std::fs::read_dir(&self.root).unwrap().count(),
            0,
            "CLI wrote files"
        );
        output
    }
    async fn run(&self, args: &[&str]) -> (i32, Value) {
        let output = self.output(args).await;
        let value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{args:?}: {error}: {output:?}"));
        (output.status.code().unwrap(), value)
    }
    async fn call(&self, operation: &str, input: Value, flag: Option<&str>) -> (i32, Value) {
        let input = input.to_string();
        let mut args = vec!["call", operation, "--input", &input];
        if let Some(flag) = flag {
            args.push(flag);
        }
        self.run(&args).await
    }
    async fn set(
        &self,
        call: bool,
        explicit: Option<&str>,
        policy: Value,
        flag: &str,
    ) -> (i32, Value) {
        if call {
            let mut input = if policy == off() {
                json!({"off":true})
            } else {
                json!({"max_entries":policy["max_entries"]})
            };
            if let Some(id) = explicit {
                input["session_id"] = json!(id);
            }
            self.call("capture.http.retention.set", input, Some(flag))
                .await
        } else {
            let limit = policy["max_entries"].to_string();
            let mut args = vec!["capture", "http", "retention", "set", flag];
            if policy == off() {
                args.push("--off");
            } else {
                args.extend(["--max-entries", &limit]);
            }
            if let Some(id) = explicit {
                args.extend(["--session-id", id]);
            }
            self.run(&args).await
        }
    }
    fn take_requests(&self) -> Vec<Request> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }
    fn assert_requests(&self, id: &str, implicit: bool, write: Option<(&Value, &Value)>) {
        let mut expected = vec![Request {
            method: "GET".into(),
            path: "/api/settings".into(),
            query: BTreeMap::new(),
            body: None,
        }];
        if implicit {
            expected.push(Request {
                method: "GET".into(),
                path: "/api/sessions".into(),
                query: BTreeMap::new(),
                body: None,
            });
        }
        let mut query = BTreeMap::from([("session_id".into(), id.into())]);
        expected.push(Request {
            method: "GET".into(),
            path: PATH.into(),
            query: query.clone(),
            body: None,
        });
        if let Some((old, new)) = write {
            if implicit {
                query.insert("expected_active_session_id".into(), id.into());
            }
            expected.push(Request {
                method: "POST".into(),
                path: PATH.into(),
                query,
                body: Some(json!({"expected_config_token":token(id,old,0),"policy":new})),
            });
        }
        assert_eq!(
            self.take_requests(),
            expected,
            "only discovery and pinned retention metadata access are allowed"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn data(result: (i32, Value), call: bool) -> Value {
    assert_eq!(result.0, 0, "{result:?}");
    if call {
        assert_eq!(result.1["ok"], true);
        result.1["data"].clone()
    } else {
        result.1
    }
}
fn failure(result: &(i32, Value), operation: &str, outcome: &str) {
    assert_ne!(result.0, 0, "{result:?}");
    assert_eq!(result.1["ok"], false);
    assert_eq!(result.1["operation"], operation);
    assert_eq!(result.1["error"]["retryable"], false);
    assert_eq!(result.1["error"]["details"]["outcome"], outcome);
    assert!(result.1.get("data").is_none());
    assert!(!result.1.to_string().contains(PRIVATE));
}

#[tokio::test]
async fn direct_and_call_save_exact_policies_once_with_session_binding() {
    for call in [false, true] {
        for explicit in [None, Some(OTHER)] {
            for (old, new) in [
                (off(), keep(5)),
                (keep(5), off()),
                (keep(1), keep(u32::MAX)),
                (off(), off()),
            ] {
                let fixture = Fixture::new(old.clone()).await;
                let result = fixture.set(call, explicit, new.clone(), "--yes").await;
                if call {
                    assert_eq!(result.1["meta"]["session_id"], explicit.unwrap_or(SESSION));
                }
                let saved = data(result, call);
                assert_eq!(saved["policy"], new);
                assert_eq!(saved["changed"], old != new);
                assert_eq!(
                    saved["config_token"],
                    token(explicit.unwrap_or(SESSION), &new, u64::from(old != new))
                );
                fixture.assert_requests(
                    explicit.unwrap_or(SESSION),
                    explicit.is_none(),
                    Some((&old, &new)),
                );
            }
        }
    }
}

#[tokio::test]
async fn get_reads_metadata_only_and_reports_pending_cleanup_truthfully() {
    for reason in ["protected_records", "record_identity_conflict"] {
        for call in [false, true] {
            for explicit in [None, Some(OTHER)] {
                let fixture = Fixture::new(keep(2)).await;
                let id = explicit.unwrap_or(SESSION);
                let mut current = snapshot(id, keep(2));
                current["status"] = json!({"total_count":12,"protected_count":4,"pending":true,"disk_cleanup_pending":true,"blocked_reason":reason});
                fixture.faults.lock().unwrap().get = Some(Reply::json(current.clone()));
                let result = if call {
                    fixture
                        .call(
                            "capture.http.retention.get",
                            json!({"session_id":explicit}),
                            None,
                        )
                        .await
                } else {
                    let mut args = vec!["capture", "http", "retention", "get"];
                    if let Some(id) = explicit {
                        args.extend(["--session-id", id]);
                    }
                    fixture.run(&args).await
                };
                assert_eq!(data(result, call), current);
                fixture.assert_requests(id, explicit.is_none(), None);
            }
        }
    }
}

#[tokio::test]
async fn dry_run_is_offline_exact_and_does_not_invent_deletion_counts() {
    let fixture = Fixture::new(off()).await;
    for policy in [off(), keep(1), keep(u32::MAX)] {
        for explicit in [None, Some(OTHER)] {
            let direct = data(
                fixture
                    .set(false, explicit, policy.clone(), "--dry-run")
                    .await,
                false,
            );
            let called = data(
                fixture
                    .set(true, explicit, policy.clone(), "--dry-run")
                    .await,
                true,
            );
            assert_eq!(direct, called);
            assert_eq!(direct["api"]["write"]["body"]["policy"], policy);
            assert_eq!(direct["requires_confirmation"], true);
            assert_eq!(direct["api"]["write"]["automatic_retry"], false);
            assert!(direct.get("deleted_count").is_none());
            assert!(direct.get("status").is_none());
        }
    }
    let direct = data(
        fixture
            .run(&["capture", "http", "retention", "get", "--dry-run"])
            .await,
        false,
    );
    let called = data(
        fixture
            .call("capture.http.retention.get", json!({}), Some("--dry-run"))
            .await,
        true,
    );
    assert_eq!(direct, called);
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test]
async fn writes_require_confirmation_before_discovery() {
    let fixture = Fixture::new(off()).await;
    for result in [
        fixture
            .run(&["capture", "http", "retention", "set", "--off"])
            .await,
        fixture
            .call("capture.http.retention.set", json!({"max_entries":4}), None)
            .await,
    ] {
        assert_eq!(result.0, 2);
        assert_eq!(result.1["error"]["code"], "CONFIRMATION_REQUIRED");
    }
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test]
async fn invalid_call_inputs_are_rejected_offline() {
    let fixture = Fixture::new(off()).await;
    for input in [
        json!({}),
        json!({"off":false}),
        json!({"off":null}),
        json!({"off":"true"}),
        json!({"off":1}),
        json!({"max_entries":0}),
        json!({"max_entries":4294967296_u64}),
        json!({"max_entries":-1}),
        json!({"max_entries":1.5}),
        json!({"max_entries":1.0}),
        json!({"max_entries":"4"}),
        json!({"max_entries":true}),
        json!({"max_entries":null}),
        json!({"off":true,"max_entries":5}),
        json!({"off":false,"max_entries":null}),
        json!({"off":true,"session_id":"bad"}),
        json!({"off":true,"unknown":true}),
        json!({"policy":{"mode":"off"}}),
    ] {
        let result = fixture
            .call(
                "capture.http.retention.set",
                input.clone(),
                Some("--dry-run"),
            )
            .await;
        assert_eq!(result.0, 2, "{input}: {result:?}");
        assert_eq!(
            result.1["error"]["code"], "INVALID_INPUT",
            "{input}: {result:?}"
        );
    }
    for raw in ["{\"max_entries\":1e0}", "{\"max_entries\":-0}"] {
        let result = fixture
            .run(&[
                "call",
                "capture.http.retention.set",
                "--input",
                raw,
                "--dry-run",
            ])
            .await;
        assert_eq!(result.0, 2, "{raw}: {result:?}");
    }
    for input in [
        json!({"off":true}),
        json!({"max_entries":1}),
        json!({"session_id":false}),
    ] {
        assert_eq!(
            fixture
                .call("capture.http.retention.get", input, Some("--dry-run"))
                .await
                .0,
            2
        );
    }
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test]
async fn invalid_direct_limits_and_choice_conflicts_are_rejected_offline() {
    let fixture = Fixture::new(off()).await;
    for value in ["0", "4294967296", "-1", "1.5", "1.0", "1e0", "false", "NaN"] {
        let result = fixture
            .run(&[
                "capture",
                "http",
                "retention",
                "set",
                "--max-entries",
                value,
                "--dry-run",
            ])
            .await;
        assert_eq!(result.0, 2, "{value}: {result:?}");
        assert_eq!(result.1["operation"], "capture.http.retention.set");
    }
    for suffix in [
        vec!["--dry-run"],
        vec!["--off", "--max-entries", "4", "--dry-run"],
    ] {
        let mut args = vec!["capture", "http", "retention", "set"];
        args.extend(suffix);
        assert_eq!(fixture.run(&args).await.0, 2);
    }
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test]
async fn response_uuid_equivalence_and_additive_fields_are_safe() {
    for call in [false, true] {
        let fixture = Fixture::new(off()).await;
        let mut current = snapshot(SESSION, off());
        current["session_id"] = json!(SESSION.to_uppercase());
        current["future"] = json!(PRIVATE);
        current["policy"]["future"] = json!(PRIVATE);
        current["status"]["future"] = json!(PRIVATE);
        fixture.faults.lock().unwrap().get = Some(Reply::json(current));
        let mut ack = snapshot_at_revision(SESSION, keep(3), 1);
        ack["session_id"] = json!(Uuid::parse_str(SESSION).unwrap().simple().to_string());
        ack["changed"] = json!(true);
        ack["future"] = json!(PRIVATE);
        ack["policy"]["future"] = json!(PRIVATE);
        ack["status"]["future"] = json!(PRIVATE);
        fixture.faults.lock().unwrap().post = Some(Reply::json(ack));
        let saved = data(
            fixture
                .set(call, Some(&SESSION.to_uppercase()), keep(3), "--yes")
                .await,
            call,
        );
        assert_eq!(saved["session_id"], SESSION);
        assert!(!saved.to_string().contains(PRIVATE));
        fixture.assert_requests(SESSION, false, Some((&off(), &keep(3))));
    }
}

fn malformed_snapshots(base: &Value) -> Vec<Value> {
    let mut cases = vec![Value::Null, json!([]), json!({}), json!({"error":PRIVATE})];
    for field in [
        "session_id",
        "config_token",
        "config_revision",
        "policy",
        "status",
    ] {
        let mut value = base.clone();
        value.as_object_mut().unwrap().remove(field);
        cases.push(value);
        let mut value = base.clone();
        value[field] = Value::Null;
        cases.push(value);
    }
    for (field, replacement) in [
        ("session_id", json!(OTHER)),
        ("session_id", json!(PRIVATE)),
        ("config_token", json!("a".repeat(64))),
        ("policy", json!({"mode":"unknown"})),
        ("policy", json!({"mode":"off","max_entries":4})),
        ("policy", json!({"mode":"keep_newest"})),
        ("policy", json!({"mode":"keep_newest","max_entries":0})),
        (
            "policy",
            json!({"mode":"keep_newest","max_entries":4294967296_u64}),
        ),
        ("policy", json!({"mode":"keep_newest","max_entries":3.0})),
        ("status", json!([])),
    ] {
        let mut value = base.clone();
        value[field] = replacement;
        cases.push(value);
    }
    for field in [
        "total_count",
        "protected_count",
        "pending",
        "disk_cleanup_pending",
        "blocked_reason",
    ] {
        let mut value = base.clone();
        value["status"].as_object_mut().unwrap().remove(field);
        cases.push(value);
    }
    for (field, replacement) in [
        ("total_count", json!(-1)),
        ("total_count", json!(1.0)),
        ("total_count", json!("12")),
        ("protected_count", json!(13)),
        ("protected_count", json!(-1)),
        ("pending", json!(1)),
        ("pending", Value::Null),
        ("disk_cleanup_pending", json!("false")),
        ("disk_cleanup_pending", Value::Null),
        ("blocked_reason", json!(PRIVATE)),
        ("blocked_reason", json!(false)),
    ] {
        let mut value = base.clone();
        value["status"][field] = replacement;
        cases.push(value);
    }
    for revision in [
        json!(0),
        json!(""),
        json!("00"),
        json!("+1"),
        json!("-1"),
        json!("1.0"),
        json!("18446744073709551616"),
    ] {
        let mut value = base.clone();
        value["config_revision"] = revision;
        cases.push(value);
    }
    cases
}

#[tokio::test]
async fn invalid_read_metadata_prevents_every_write_and_sanitizes_diagnostics() {
    for call in [false, true] {
        for value in malformed_snapshots(&snapshot(SESSION, off())) {
            let fixture = Fixture::new(off()).await;
            fixture.faults.lock().unwrap().get = Some(Reply::json(value));
            let result = fixture.set(call, Some(SESSION), keep(3), "--yes").await;
            failure(&result, "capture.http.retention.set", "not_applied");
            assert_eq!(result.1["error"]["code"], "INVALID_RESPONSE");
            fixture.assert_requests(SESSION, false, None);
        }
    }
}

#[tokio::test]
async fn invalid_acknowledgements_are_unknown_without_retry_or_readback() {
    let mut base = snapshot_at_revision(SESSION, keep(3), 1);
    base["changed"] = json!(true);
    let mut cases = malformed_snapshots(&base);
    for replacement in [json!(false), json!(1), Value::Null] {
        let mut value = base.clone();
        value["changed"] = replacement;
        cases.push(value);
    }
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("changed");
    cases.push(missing);
    let mut wrong_policy = snapshot_at_revision(SESSION, keep(4), 1);
    wrong_policy["changed"] = json!(true);
    cases.push(wrong_policy);
    for revision in [0, 2] {
        let mut wrong_revision = snapshot_at_revision(SESSION, keep(3), revision);
        wrong_revision["changed"] = json!(true);
        cases.push(wrong_revision);
    }
    for call in [false, true] {
        for value in &cases {
            let fixture = Fixture::new(off()).await;
            fixture.faults.lock().unwrap().post = Some(Reply::json(value.clone()));
            let result = fixture.set(call, Some(SESSION), keep(3), "--yes").await;
            failure(&result, "capture.http.retention.set", "unknown");
            assert_eq!(result.1["error"]["code"], "INVALID_RESPONSE");
            assert!(result.1["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("same session"));
            fixture.assert_requests(SESSION, false, Some((&off(), &keep(3))));
        }
    }
}

#[tokio::test]
async fn errors_redirects_lost_and_incomplete_acknowledgements_never_retry() {
    let replies = vec![
        Reply::Http(400, PRIVATE.into()),
        Reply::Http(409, PRIVATE.into()),
        Reply::Http(428, PRIVATE.into()),
        Reply::Http(500, PRIVATE.into()),
        Reply::Http(
            503,
            format!("{{\"error\":\"{PRIVATE} requires --yes invalid input\"}}"),
        ),
        Reply::Redirect(301),
        Reply::Redirect(302),
        Reply::Redirect(303),
        Reply::Redirect(307),
        Reply::Redirect(308),
        Reply::Disconnect,
        Reply::Http(204, String::new()),
        Reply::Http(200, format!("{{\"private\":\"{PRIVATE}\"")),
        Reply::Http(200, "x".repeat(64 * 1024 + 1)),
        Reply::Incomplete(200, PRIVATE.into(), false),
    ];
    for call in [false, true] {
        for reply in &replies {
            let fixture = Fixture::new(off()).await;
            fixture.faults.lock().unwrap().post = Some(reply.clone());
            let result = fixture.set(call, None, keep(3), "--yes").await;
            failure(&result, "capture.http.retention.set", "unknown");
            fixture.assert_requests(SESSION, true, Some((&off(), &keep(3))));
        }
    }
}

#[tokio::test]
async fn http_error_headers_are_authoritative_without_waiting_for_diagnostics() {
    for write in [false, true] {
        for status in [400, 409, 500, 503] {
            let fixture = Fixture::new(off()).await;
            let reply = Reply::Incomplete(
                status,
                format!("{{\"error\":\"{PRIVATE} invalid requires --yes"),
                true,
            );
            if write {
                fixture.faults.lock().unwrap().post = Some(reply);
            } else {
                fixture.faults.lock().unwrap().get = Some(reply);
            }
            let result = fixture.set(true, Some(SESSION), keep(3), "--yes").await;
            fixture.release.notify_one();
            failure(
                &result,
                "capture.http.retention.set",
                if write { "unknown" } else { "not_applied" },
            );
            assert_eq!(result.1["error"]["code"], "HTTP_STATUS_ERROR");
            assert_eq!(result.1["error"]["details"]["status"], status);
            fixture.assert_requests(SESSION, false, write.then_some((&off(), &keep(3))));
        }
    }
}

#[tokio::test]
async fn read_redirects_and_invalid_active_session_never_write() {
    let fixture = Fixture::new(off()).await;
    fixture.faults.lock().unwrap().get = Some(Reply::Redirect(307));
    let result = fixture.set(true, Some(SESSION), keep(3), "--yes").await;
    failure(&result, "capture.http.retention.set", "not_applied");
    assert_eq!(result.1["error"]["code"], "REDIRECT_REFUSED");
    fixture.assert_requests(SESSION, false, None);
    for sessions in [
        json!([]),
        json!([summary(SESSION, false)]),
        json!([summary(SESSION, true), summary(OTHER, true)]),
        json!({"secret":PRIVATE}),
    ] {
        let fixture = Fixture::new(off()).await;
        fixture.faults.lock().unwrap().sessions = Some(sessions);
        let result = fixture.set(true, None, keep(3), "--yes").await;
        failure(&result, "capture.http.retention.set", "not_applied");
        let requests = fixture.take_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].path, "/api/sessions");
    }
}

#[tokio::test]
async fn discovery_help_schema_and_examples_are_offline_and_complete() {
    let fixture = Fixture::new(off()).await;
    let manifest = data(fixture.run(&["manifest"]).await, false);
    for operation in ["capture.http.retention.get", "capture.http.retention.set"] {
        let spec = manifest["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|spec| spec["operation"] == operation)
            .unwrap();
        assert_eq!(spec["requires_confirmation"], operation.ends_with(".set"));
        for kind in ["input", "output"] {
            let direct = data(fixture.run(&["schema", kind, operation]).await, false);
            let called = data(
                fixture
                    .call("schema", json!({"kind":kind,"operation":operation}), None)
                    .await,
                true,
            );
            assert_eq!(direct, called);
            assert_eq!(direct["schema"], spec[format!("{kind}_schema")]);
        }
        let examples = data(fixture.run(&["examples", operation]).await, false);
        assert_eq!(examples["examples"], spec["examples"]);
        for input in examples["examples"].as_array().unwrap() {
            assert_eq!(
                fixture
                    .call(operation, input.clone(), Some("--dry-run"))
                    .await
                    .0,
                0
            );
        }
    }
    for leaf in ["get", "set"] {
        let output = fixture
            .output(&["capture", "http", "retention", leaf, "--help"])
            .await;
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--session-id"));
        if leaf == "set" {
            assert!(help.contains("--off"));
            assert!(help.contains("--max-entries"));
            assert!(help.contains("--yes"));
        }
    }
    assert!(fixture.take_requests().is_empty());
}

// Deliberately narrow JSON Schema evaluator: unsupported assertions fail loudly
// rather than giving a passing parity test while silently ignoring a constraint.
fn assert_supported_schema(schema: &Value) {
    for key in schema.as_object().unwrap().keys() {
        assert!(
            matches!(
                key.as_str(),
                "type"
                    | "required"
                    | "properties"
                    | "additionalProperties"
                    | "oneOf"
                    | "not"
                    | "const"
                    | "enum"
                    | "minimum"
                    | "maximum"
                    | "pattern"
                    | "format"
                    | "x-sniper-json-number-representation"
                    | "description"
                    | "default"
            ),
            "unsupported schema keyword {key}"
        );
    }
    if let Some(properties) = schema.get("properties") {
        for property in properties.as_object().unwrap().values() {
            assert_supported_schema(property);
        }
    }
    if let Some(choices) = schema.get("oneOf") {
        for choice in choices.as_array().unwrap() {
            assert_supported_schema(choice);
        }
    }
    if let Some(forbidden) = schema.get("not") {
        assert_supported_schema(forbidden);
    }
}

fn schema_accepts(value: &Value, schema: &Value) -> bool {
    assert_supported_schema(schema);
    if let Some(types) = schema.get("type") {
        let matches_type = |kind: &Value| match kind.as_str().unwrap() {
            "object" => value.is_object(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "integer" => value.is_i64() || value.is_u64(),
            other => panic!("unsupported schema type {other}"),
        };
        if !types
            .as_array()
            .map(|types| types.iter().any(matches_type))
            .unwrap_or_else(|| matches_type(types))
        {
            return false;
        }
    }
    if schema
        .get("const")
        .is_some_and(|expected| expected != value)
    {
        return false;
    }
    if schema
        .get("enum")
        .is_some_and(|choices| !choices.as_array().unwrap().contains(value))
    {
        return false;
    }
    if schema
        .get("not")
        .is_some_and(|forbidden| schema_accepts(value, forbidden))
    {
        return false;
    }
    if schema.get("oneOf").is_some_and(|choices| {
        choices
            .as_array()
            .unwrap()
            .iter()
            .filter(|choice| schema_accepts(value, choice))
            .count()
            != 1
    }) {
        return false;
    }
    if let Some(object) = value.as_object() {
        if schema.get("required").is_some_and(|fields| {
            fields
                .as_array()
                .unwrap()
                .iter()
                .any(|field| !object.contains_key(field.as_str().unwrap()))
        }) {
            return false;
        }
        for (key, member) in object {
            if let Some(property) = schema
                .get("properties")
                .and_then(|properties| properties.get(key))
            {
                if !schema_accepts(member, property) {
                    return false;
                }
            } else if schema["additionalProperties"] == false {
                return false;
            }
        }
    }
    if let Some(text) = value.as_str() {
        if schema.get("pattern").is_some_and(|pattern| {
            !regex::Regex::new(pattern.as_str().unwrap())
                .unwrap()
                .is_match(text)
        }) {
            return false;
        }
        if let Some(format) = schema.get("format") {
            assert_eq!(format, "uuid");
            if Uuid::parse_str(text).is_err() {
                return false;
            }
        }
    }
    if value.is_number() {
        if schema.get("x-sniper-json-number-representation").is_some() {
            assert_eq!(
                schema["x-sniper-json-number-representation"],
                "unsigned-integer"
            );
            if !value.is_u64() {
                return false;
            }
        }
        if schema
            .get("minimum")
            .is_some_and(|min| value.as_f64().unwrap() < min.as_f64().unwrap())
        {
            return false;
        }
        if schema
            .get("maximum")
            .is_some_and(|max| value.as_f64().unwrap() > max.as_f64().unwrap())
        {
            return false;
        }
    }
    true
}

#[tokio::test]
async fn input_schema_acceptance_matches_call_parsing_and_uuid_spellings() {
    let fixture = Fixture::new(off()).await;
    let operation = "capture.http.retention.set";
    let schema = data(fixture.run(&["schema", "input", operation]).await, false)["schema"].clone();
    let mut inputs = vec![
        json!({"off":true}),
        json!({"off":false,"max_entries":1}),
        json!({"max_entries":u32::MAX,"session_id":null}),
        json!({}),
        json!({"off":false}),
        json!({"off":null}),
        json!({"off":true,"max_entries":2}),
        json!({"max_entries":0}),
        json!({"max_entries":4294967296_u64}),
        json!({"max_entries":-1}),
        json!({"max_entries":1.0}),
        json!({"max_entries":"2"}),
        json!({"max_entries":null}),
        json!({"off":true,"session_id":"bad"}),
        json!({"off":true,"session_id":false}),
        json!({"off":true,"unknown":2}),
    ];
    let uuid = Uuid::parse_str(SESSION).unwrap();
    for id in [
        SESSION.to_owned(),
        SESSION.to_uppercase(),
        uuid.simple().to_string(),
        uuid.braced().to_string(),
        uuid.urn().to_string(),
    ] {
        inputs.push(json!({"off":true,"session_id":id}));
    }
    for input in inputs {
        let expected = schema_accepts(&input, &schema);
        let result = fixture
            .call(operation, input.clone(), Some("--dry-run"))
            .await;
        assert_eq!(
            result.0 == 0,
            expected,
            "schema/call mismatch: {input}: {result:?}"
        );
    }
    let output_schema =
        data(fixture.run(&["schema", "output", operation]).await, false)["schema"].clone();
    let mut ack = snapshot(SESSION, keep(2));
    ack["changed"] = json!(true);
    assert!(schema_accepts(&ack, &output_schema));
    for field in [
        "session_id",
        "config_token",
        "config_revision",
        "policy",
        "status",
        "changed",
    ] {
        let mut missing = ack.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            !schema_accepts(&missing, &output_schema),
            "schema did not require {field}"
        );
    }
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test]
async fn intervening_off_on_off_policy_changes_reject_the_original_token() {
    assert_ne!(token(SESSION, &off(), 0), token(SESSION, &off(), 2));
    for call in [false, true] {
        let fixture = Fixture::new(off()).await;
        // The synthetic service has returned to Off at revision 2 since the GET.
        fixture.faults.lock().unwrap().post_expected_revision = Some(2);
        let result = fixture.set(call, None, keep(3), "--yes").await;
        failure(&result, "capture.http.retention.set", "unknown");
        assert_eq!(result.1["error"]["details"]["status"], 409);
        fixture.assert_requests(SESSION, true, Some((&off(), &keep(3))));
    }
}

#[tokio::test]
async fn revision_exhaustion_prevents_writes_but_allows_inspection() {
    let fixture = Fixture::new(off()).await;
    let current = snapshot_at_revision(SESSION, off(), u64::MAX);
    fixture.faults.lock().unwrap().get = Some(Reply::json(current.clone()));
    let schema = data(
        fixture
            .run(&["schema", "output", "capture.http.retention.get"])
            .await,
        false,
    )["schema"]
        .clone();
    assert!(schema_accepts(&current, &schema));
    for revision in [
        "18446744073709551616",
        "99999999999999999999",
        "00",
        "01",
        "-1",
        "+1",
    ] {
        let mut invalid = current.clone();
        invalid["config_revision"] = json!(revision);
        assert!(
            !schema_accepts(&invalid, &schema),
            "invalid revision passed schema: {revision}"
        );
    }
    let result = fixture
        .call(
            "capture.http.retention.get",
            json!({"session_id":SESSION}),
            None,
        )
        .await;
    assert_eq!(data(result, true), current);
    fixture.assert_requests(SESSION, false, None);
    let result = fixture.set(true, Some(SESSION), keep(3), "--yes").await;
    failure(&result, "capture.http.retention.set", "not_applied");
    assert_eq!(result.1["error"]["code"], "RETENTION_REVISION_EXHAUSTED");
    fixture.assert_requests(SESSION, false, None);
}
