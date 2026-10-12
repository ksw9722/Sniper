//! Synthetic loopback settings only. No real sessions, proxy traffic or discovery probes.
use serde_json::{json, Value};
use sniper::upstream_peer_settings::config_token;
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
const PATH: &str = "/api/transactions/upstream-peer-settings";
const PRIVATE: &str = "synthetic-private-diagnostic";

fn snapshot(enabled: bool, revision: u64) -> Value {
    json!({"session_id":SESSION,"enabled":enabled,"config_revision":revision.to_string(),
        "config_token":config_token(Uuid::parse_str(SESSION).unwrap(),enabled,Some(revision)),
        "status":{"effective_enabled":enabled,"blocked_reason":null}})
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
    Json(Value),
    Http(u16, String),
    Redirect,
    Disconnect,
    Incomplete,
}
#[derive(Default)]
struct Faults {
    get: Option<Reply>,
    post: Option<Reply>,
}
struct Fixture {
    api: String,
    root: PathBuf,
    requests: Arc<Mutex<Vec<Request>>>,
    faults: Arc<Mutex<Faults>>,
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
    async fn new(enabled: bool, revision: u64) -> Self {
        let root = std::env::temp_dir().join(format!("sniper-peer-cli-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let settings = json!({"runtime_instance_id":OTHER,"proxy_addr":"127.0.0.1:0","ui_addr":addr.to_string(),
            "data_dir":root.join("data").to_string_lossy(),"max_entries":100,"features":["http_capture","session_storage","replay"]});
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let faults = Arc::new(Mutex::new(Faults::default()));
        let server_faults = faults.clone();
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
                        ("GET", "/api/settings") => Reply::Json(settings.clone()),
                        ("GET", PATH) => faults
                            .get
                            .clone()
                            .unwrap_or_else(|| Reply::Json(snapshot(enabled, revision))),
                        ("POST", PATH) => faults.post.clone().unwrap_or_else(|| {
                            let next = request.body.as_ref().unwrap()["enabled"].as_bool().unwrap();
                            let mut result = snapshot(next, revision + u64::from(enabled != next));
                            result["changed"] = json!(enabled != next);
                            Reply::Json(result)
                        }),
                        _ => Reply::Http(405, "outside synthetic settings allowlist".into()),
                    }
                };
                let (status, body, location, extra_len) = match reply {
                    Reply::Json(value) => (200, value.to_string(), "", 0),
                    Reply::Http(status, body) => (status, body, "", 0),
                    Reply::Redirect => (302, PRIVATE.into(), "Location: /must-not-follow\r\n", 0),
                    Reply::Incomplete => (200, "{}".into(), "", 100),
                    Reply::Disconnect => continue,
                };
                let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n{body}",body.len()+extra_len);
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        Self {
            api: format!("http://{addr}"),
            root,
            requests,
            faults,
            server,
        }
    }
    async fn run(&self, args: &[&str]) -> (i32, Value) {
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
        let output = tokio::time::timeout(Duration::from_secs(8), cmd.output())
            .await
            .expect("settings command stalled")
            .unwrap();
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        assert_eq!(
            std::fs::read_dir(&self.root).unwrap().count(),
            0,
            "CLI wrote files"
        );
        (
            output.status.code().unwrap(),
            serde_json::from_slice(&output.stdout).unwrap(),
        )
    }
    async fn invoke(&self, call: bool, enabled: Option<bool>, flag: Option<&str>) -> (i32, Value) {
        let mut args = if call {
            vec![
                "call",
                if enabled.is_some() {
                    "capture.http.upstream_peer.set"
                } else {
                    "capture.http.upstream_peer.get"
                },
                "--input",
            ]
        } else {
            vec![
                "capture",
                "http",
                "upstream-peer",
                if enabled.is_some() { "set" } else { "get" },
                "--session-id",
                SESSION,
            ]
        };
        let mut input = json!({"session_id":SESSION});
        if let Some(enabled) = enabled {
            input["enabled"] = json!(enabled);
        }
        let input = input.to_string();
        if call {
            args.push(&input);
        } else if let Some(enabled) = enabled {
            args.extend(["--enabled", if enabled { "true" } else { "false" }]);
        }
        if let Some(flag) = flag {
            args.push(flag);
        }
        self.run(&args).await
    }
    fn take_requests(&self) -> Vec<Request> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }
    fn assert_requests(&self, old: bool, revision: u64, next: Option<bool>) {
        let query = BTreeMap::from([
            ("session_id".into(), SESSION.into()),
            ("expected_active_session_id".into(), SESSION.into()),
        ]);
        let mut expected = vec![
            Request {
                method: "GET".into(),
                path: "/api/settings".into(),
                query: BTreeMap::new(),
                body: None,
            },
            Request {
                method: "GET".into(),
                path: PATH.into(),
                query: query.clone(),
                body: None,
            },
        ];
        if let Some(next) = next {
            expected.push(Request{method:"POST".into(),path:PATH.into(),query,body:Some(json!({"enabled":next,"expected_config_token":snapshot(old,revision)["config_token"]}))});
        }
        assert_eq!(
            self.take_requests(),
            expected,
            "Only settings discovery and one pinned GET/POST are allowed"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn success(result: (i32, Value), call: bool) -> Value {
    assert_eq!(result.0, 0, "{result:?}");
    if call {
        if result.1["data"].get("session_id").is_some() {
            assert_eq!(result.1["meta"]["session_id"], SESSION);
        }
        result.1["data"].clone()
    } else {
        result.1
    }
}
fn assert_failure(result: (i32, Value), code: &str, outcome: &str) {
    assert_ne!(result.0, 0, "{result:?}");
    assert_eq!(result.1["error"]["code"], code, "{result:?}");
    assert_eq!(result.1["error"]["retryable"], false);
    assert_eq!(result.1["error"]["details"]["outcome"], outcome);
    assert!(!result.1.to_string().contains(PRIVATE));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_direct_and_call_read_save_noop_and_disable_are_pinned_once() {
    for call in [false, true] {
        for old in [false, true] {
            let fixture = Fixture::new(old, 9007199254740993).await;
            let result = success(fixture.invoke(call, None, None).await, call);
            assert_eq!(result, snapshot(old, 9007199254740993));
            fixture.assert_requests(old, 9007199254740993, None);
            for next in [false, true] {
                let result = success(fixture.invoke(call, Some(next), Some("--yes")).await, call);
                let mut expected = snapshot(next, 9007199254740993 + u64::from(old != next));
                expected["changed"] = json!(old != next);
                assert_eq!(result, expected);
                fixture.assert_requests(old, 9007199254740993, Some(next));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_discovery_confirmation_and_dry_runs_are_offline_and_strict() {
    let fixture = Fixture::new(false, 0).await;
    for call in [false, true] {
        let result = fixture.invoke(call, Some(true), None).await;
        assert_eq!(result.1["error"]["code"], "CONFIRMATION_REQUIRED");
        let result = success(
            fixture.invoke(call, Some(true), Some("--dry-run")).await,
            call,
        );
        assert_eq!(result["api"]["write"]["body"]["enabled"], true);
        assert_eq!(result["api"]["write"]["automatic_retry"], false);
        assert!(result["api"]["write"]["path"]
            .as_str()
            .unwrap()
            .contains("expected_active_session_id="));
    }
    for operation in [
        "capture.http.upstream_peer.get",
        "capture.http.upstream_peer.set",
    ] {
        for args in [
            vec!["schema", "input", operation],
            vec!["schema", "output", operation],
            vec!["examples", operation],
        ] {
            assert_eq!(fixture.run(&args).await.0, 0);
        }
    }
    for input in [
        json!({}),
        json!({"session_id":null,"enabled":true}),
        json!({"session_id":SESSION}),
        json!({"session_id":SESSION,"enabled":null}),
        json!({"session_id":SESSION,"enabled":"true"}),
        json!({"session_id":SESSION,"enabled":true,"expected_config_token":"private"}),
        json!({"session_id":SESSION,"enabled":true,"backfill":true}),
    ] {
        let result = fixture
            .run(&[
                "call",
                "capture.http.upstream_peer.set",
                "--input",
                &input.to_string(),
                "--dry-run",
            ])
            .await;
        assert_eq!(result.0, 2, "{result:?}");
    }
    for args in [
        vec!["capture", "http", "upstream-peer", "get"],
        vec![
            "capture",
            "http",
            "upstream-peer",
            "set",
            "--session-id",
            SESSION,
        ],
        vec![
            "capture",
            "http",
            "upstream-peer",
            "set",
            "--session-id",
            SESSION,
            "--enabled",
            "yes",
        ],
    ] {
        assert_eq!(fixture.run(&args).await.0, 2);
    }
    assert!(fixture.take_requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_invalid_get_blocks_post_and_never_echoes_unknown_fields() {
    let fixture = Fixture::new(false, 0).await;
    for (pointer, value) in [
        ("/session_id", json!(OTHER)),
        ("/enabled", json!("false")),
        ("/config_revision", json!("01")),
        ("/config_revision", json!("18446744073709551616")),
        ("/config_token", json!(PRIVATE)),
        ("/status/effective_enabled", json!(true)),
        ("/status/blocked_reason", json!(PRIVATE)),
    ] {
        let mut payload = snapshot(false, 0);
        *payload.pointer_mut(pointer).unwrap() = value;
        fixture.faults.lock().unwrap().get = Some(Reply::Json(payload));
        assert_failure(
            fixture.invoke(false, Some(true), Some("--yes")).await,
            "INVALID_RESPONSE",
            "not_applied",
        );
        fixture.assert_requests(false, 0, None);
    }
    for field in [
        "session_id",
        "enabled",
        "config_revision",
        "config_token",
        "status",
    ] {
        let mut payload = snapshot(false, 0);
        payload.as_object_mut().unwrap().remove(field);
        fixture.faults.lock().unwrap().get = Some(Reply::Json(payload));
        assert_failure(
            fixture.invoke(true, Some(true), Some("--yes")).await,
            "INVALID_RESPONSE",
            "not_applied",
        );
        fixture.assert_requests(false, 0, None);
    }
    let mut payload = snapshot(false, 0);
    payload["private"] = json!(PRIVATE);
    payload["status"]["private"] = json!(PRIVATE);
    fixture.faults.lock().unwrap().get = Some(Reply::Json(payload));
    assert_eq!(
        success(fixture.invoke(false, None, None).await, false),
        snapshot(false, 0)
    );
    fixture.assert_requests(false, 0, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_get_failure_never_mutates_and_post_failure_never_retries_or_reads_back() {
    let fixture = Fixture::new(false, 0).await;
    for write in [false, true] {
        for (reply, code) in [
            (Reply::Redirect, "REDIRECT_REFUSED"),
            (Reply::Http(409, PRIVATE.into()), "HTTP_STATUS_ERROR"),
            (Reply::Http(503, PRIVATE.into()), "HTTP_STATUS_ERROR"),
            (Reply::Disconnect, "TRANSPORT_ERROR"),
            (Reply::Incomplete, "INVALID_RESPONSE"),
            (Reply::Http(200, PRIVATE.into()), "INVALID_RESPONSE"),
            (Reply::Http(200, "x".repeat(16385)), "INVALID_RESPONSE"),
        ] {
            {
                let mut faults = fixture.faults.lock().unwrap();
                if write {
                    faults.post = Some(reply);
                } else {
                    faults.get = Some(reply);
                }
            }
            assert_failure(
                fixture.invoke(true, Some(true), Some("--yes")).await,
                code,
                if write { "unknown" } else { "not_applied" },
            );
            fixture.assert_requests(false, 0, write.then_some(true));
        }
        *fixture.faults.lock().unwrap() = Faults::default();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_post_rejects_wrong_acknowledgement_and_preserves_decimal_max_noop() {
    let fixture = Fixture::new(false, 0).await;
    for (pointer, value) in [
        ("/session_id", json!(OTHER)),
        ("/enabled", json!(false)),
        ("/config_revision", json!("0")),
        ("/config_token", json!(PRIVATE)),
        ("/config_token", snapshot(false, 0)["config_token"].clone()),
        ("/changed", json!(false)),
        (
            "/status",
            json!({"effective_enabled":false,"blocked_reason":"persistence_uncertain"}),
        ),
        ("/status/effective_enabled", json!(false)),
    ] {
        let mut payload = snapshot(true, 1);
        payload["changed"] = json!(true);
        *payload.pointer_mut(pointer).unwrap() = value;
        fixture.faults.lock().unwrap().post = Some(Reply::Json(payload));
        assert_failure(
            fixture.invoke(false, Some(true), Some("--yes")).await,
            "INVALID_RESPONSE",
            "unknown",
        );
        fixture.assert_requests(false, 0, Some(true));
    }
    let fixture = Fixture::new(false, u64::MAX).await;
    let result = success(
        fixture.invoke(false, Some(false), Some("--yes")).await,
        false,
    );
    assert_eq!(result["config_revision"], u64::MAX.to_string());
    assert_eq!(result["changed"], false);
    fixture.assert_requests(false, u64::MAX, Some(false));
    assert_failure(
        fixture.invoke(false, Some(true), Some("--yes")).await,
        "UPSTREAM_PEER_REVISION_EXHAUSTED",
        "not_applied",
    );
    fixture.assert_requests(false, u64::MAX, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_status_blocks_capture_without_hiding_saved_switch() {
    let fixture = Fixture::new(true, 0).await;
    for reason in [
        "persistence_uncertain",
        "invalid_configuration",
        "http_capture_paused",
    ] {
        let mut payload = snapshot(true, 0);
        payload["status"] = json!({"effective_enabled":false,"blocked_reason":reason});
        fixture.faults.lock().unwrap().get = Some(Reply::Json(payload.clone()));
        assert_eq!(
            success(fixture.invoke(true, None, None).await, true),
            payload
        );
        fixture.assert_requests(true, 0, None);
    }
}
