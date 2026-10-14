//! Synthetic loopback queue only: no proxy, upstream traffic, or user sessions.
use axum::{body::Body, extract::State, http::Request, routing::any, Json, Router};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

struct Fixture {
    api: String,
    dir: std::path::PathBuf,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(queued: bool) -> Self {
        Self::with_delay(queued, false).await
    }
    async fn with_delay(queued: bool, delayed: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("sniper-intercept-parity-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().fallback(any(handler)).with_state((
            requests.clone(),
            queued,
            delayed,
            addr.to_string(),
        ));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            api: format!("http://{addr}"),
            dir,
            requests,
            task,
        }
    }
    async fn cli(&self, args: &[&str], expected: i32) -> Value {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        command
            .env_clear()
            .env("SNIPER_DATA_DIR", &self.dir)
            .env("HOME", &self.dir)
            .args(["--api", &self.api, "--output", "compact"])
            .args(args)
            .kill_on_drop(true);
        // Winsock needs SystemRoot to locate socket-provider DLLs even when
        // all traffic stays on loopback. Keep user/API settings isolated.
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let output = tokio::time::timeout(Duration::from_secs(5), command.output())
            .await
            .expect("CLI exceeded bounded wait")
            .unwrap();
        assert_eq!(output.status.code(), Some(expected), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
const ID: &str = "11111111-1111-1111-1111-111111111111";
const SESSION: &str = "22222222-2222-2222-2222-222222222222";
async fn handler(
    State((requests, queued, delayed, addr)): State<(Arc<Mutex<Vec<String>>>, bool, bool, String)>,
    request: Request<Body>,
) -> Json<Value> {
    assert_eq!(request.method(), "GET");
    let path = request.uri().path();
    requests.lock().unwrap().push(request.uri().to_string());
    if path == "/api/settings" {
        return Json(
            json!({"runtime_instance_id":ID,"proxy_addr":"127.0.0.1:0", "ui_addr":addr, "max_entries":100,"data_dir":"synthetic-unused-path",
            "features":["http_capture","session_storage","replay"]}),
        );
    }
    assert_eq!(
        request.uri().query(),
        Some("session_id=22222222-2222-2222-2222-222222222222")
    );
    let response = path.starts_with("/api/response-intercepts");
    let mut record = if response {
        json!({"id":ID,"started_at":"2026-10-14T00:00:00Z","scheme":"http","host":"example.com","method":"GET","path":"/fixture","status":200,
            "response":{"status":200,"headers":[],"body":"fixture","body_encoding":"utf8"}})
    } else {
        json!({"id":ID,"started_at":"2026-10-14T00:00:00Z","peer_addr":"127.0.0.1:0","is_websocket":false,
            "request":{"method":"GET","scheme":"http","host":"example.com","port":80,"path":"/fixture","headers":[],"body":"","body_encoding":"utf8"}})
    };
    if path.ends_with(ID) {
        return Json(record);
    }
    assert!(path == "/api/intercepts" || path == "/api/response-intercepts");
    if delayed {
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
    if !queued {
        return Json(json!([]));
    }
    if !response {
        record["scheme"] = json!("http");
        record["host"] = json!("example.com");
        record["method"] = json!("GET");
        record["path"] = json!("/fixture");
    }
    Json(json!([record]))
}

#[tokio::test]
async fn intercept_reads_are_discoverable_and_validate_offline() {
    let f = Fixture::new(false).await;
    let manifest = f.cli(&["manifest"], 0).await;
    for operation in [
        "capture.intercept.get",
        "capture.intercept.wait",
        "capture.response_intercept.wait",
    ] {
        let spec = manifest["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|spec| spec["operation"] == operation)
            .unwrap();
        assert_eq!(spec["side_effect"], "read");
        assert_eq!(spec["requires_confirmation"], false);
        assert_eq!(
            f.cli(&["schema", "input", operation], 0).await["schema"],
            spec["input_schema"]
        );
        f.cli(&["examples", operation], 0).await;
        let input = if operation.ends_with(".get") {
            json!({"id":ID,"session_id":SESSION})
        } else {
            json!({"session_id":SESSION,"timeout":3,"poll_interval":50})
        };
        let preview = f
            .cli(
                &[
                    "call",
                    operation,
                    "--input",
                    &input.to_string(),
                    "--dry-run",
                ],
                0,
            )
            .await;
        assert_eq!(preview["operation"], operation);
        if operation.ends_with(".wait") {
            assert_eq!(preview["data"]["input"]["poll_interval"], 50);
            let defaults = f.cli(&["call", operation, "--dry-run"], 0).await;
            assert_eq!(defaults["data"]["input"]["timeout"], 30);
            assert_eq!(defaults["data"]["input"]["poll_interval"], 200);
            let nulls = f
                .cli(
                    &[
                        "call",
                        operation,
                        "--input",
                        r#"{"session_id":null,"timeout":null,"poll_interval":null}"#,
                        "--dry-run",
                    ],
                    0,
                )
                .await;
            assert_eq!(nulls["data"]["input"], defaults["data"]["input"]);
            for bad in [
                json!({"timeout":-1}),
                json!({"poll_interval":"50"}),
                json!({"timeout":u64::MAX}),
                json!({"poll_intervl":50}),
            ] {
                assert_eq!(
                    f.cli(
                        &["call", operation, "--input", &bad.to_string(), "--dry-run"],
                        2
                    )
                    .await["error"]["code"],
                    "INVALID_INPUT"
                );
            }
        }
    }
    f.cli(
        &[
            "call",
            "capture.intercept.get",
            "--input",
            r#"{"id":"bad-id"}"#,
            "--dry-run",
        ],
        2,
    )
    .await;
    for args in [
        vec!["capture", "intercept", "wait"],
        vec!["intercept", "wait"],
        vec!["capture", "response-intercept", "wait"],
    ] {
        let mut args = args;
        args.extend(["--timeout", "18446744073709551615", "--dry-run"]);
        f.cli(&args, 2).await;
    }
    assert!(f.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn canonical_get_and_wait_return_pinned_queue_details_without_writes() {
    let f = Fixture::new(true).await;
    for operation in [
        "capture.intercept.get",
        "capture.intercept.wait",
        "capture.response_intercept.wait",
    ] {
        let input = if operation.ends_with(".get") {
            json!({"id":ID,"session_id":SESSION})
        } else {
            json!({"session_id":SESSION,"timeout":0})
        };
        let result = f
            .cli(&["call", operation, "--input", &input.to_string()], 0)
            .await;
        assert_eq!(result["ok"], true);
        assert_eq!(result["data"]["id"], ID);
        assert!(result["data"]
            .get(if operation.contains("response") {
                "response"
            } else {
                "request"
            })
            .is_some());
    }
    let requests = f.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|path| path.starts_with("/api/intercepts/"))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|path| path.starts_with("/api/response-intercepts/"))
            .count(),
        1
    );
}

#[tokio::test]
async fn waits_do_not_oversleep_or_poll_again_after_deadline() {
    let f = Fixture::new(false).await;
    for operation in ["capture.intercept.wait", "capture.response_intercept.wait"] {
        f.requests.lock().unwrap().clear();
        let result = f
            .cli(
                &[
                    "call",
                    operation,
                    "--input",
                    &json!({"session_id":SESSION,"timeout":1,"poll_interval":u64::MAX}).to_string(),
                ],
                1,
            )
            .await;
        assert_eq!(result["ok"], false);
        let requests = f.requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|path| path.contains("intercepts"))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn positive_wait_timeout_bounds_stalled_queue_reads() {
    let f = Fixture::with_delay(false, true).await;
    let started = std::time::Instant::now();
    let result = f
        .cli(
            &[
                "call",
                "capture.intercept.wait",
                "--input",
                &json!({"session_id":SESSION,"timeout":1}).to_string(),
            ],
            1,
        )
        .await;
    assert_eq!(result["ok"], false);
    assert!(started.elapsed() < Duration::from_secs(3));
}
