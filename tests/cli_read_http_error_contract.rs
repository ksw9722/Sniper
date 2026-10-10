//! Read-only metadata fixtures: no runtime, captured traffic, or real user paths.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const SESSION_ID: &str = "00000000-0000-4000-8000-000000000001";
const PRIVATE_MARKER: &str = "synthetic-private-diagnostic";

#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
    incomplete: bool,
    hold_open: bool,
}

struct MockApi {
    api: String,
    root: PathBuf,
    reply: Arc<Mutex<Reply>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    body_release: Arc<tokio::sync::Notify>,
    server: tokio::task::JoinHandle<()>,
}

impl MockApi {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("sniper-read-http-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let settings = json!({
            "runtime_instance_id": Uuid::new_v4(),
            "proxy_addr": "127.0.0.1:0",
            "ui_addr": addr.to_string(),
            "data_dir": root.to_string_lossy(),
            "max_entries": 100,
            "features": ["http_capture", "session_storage", "replay"],
        })
        .to_string();
        let reply = Arc::new(Mutex::new(Reply {
            status: 503,
            body: String::new(),
            incomplete: false,
            hold_open: false,
        }));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let body_release = Arc::new(tokio::sync::Notify::new());
        let server_reply = reply.clone();
        let server_requests = requests.clone();
        let server_body_release = body_release.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let byte = tokio::time::timeout(Duration::from_secs(2), stream.read_u8())
                        .await
                        .unwrap()
                        .unwrap();
                    request.push(byte);
                    assert!(request.len() < 16 * 1024);
                }
                let request = String::from_utf8(request).unwrap();
                let mut line = request.lines().next().unwrap().split_whitespace();
                let method = line.next().unwrap();
                let path = line.next().unwrap();
                server_requests
                    .lock()
                    .unwrap()
                    .push((method.to_owned(), path.to_owned()));
                assert_eq!(method, "GET");
                let reply = if path == "/api/settings" {
                    Reply {
                        status: 200,
                        body: settings.clone(),
                        incomplete: false,
                        hold_open: false,
                    }
                } else {
                    assert!(matches!(
                        path.split('?').next().unwrap(),
                        "/api/sessions" | "/api/findings/count" | "/api/event-log"
                    ));
                    server_reply.lock().unwrap().clone()
                };
                let status = reqwest::StatusCode::from_u16(reply.status).unwrap();
                let len = reply.body.len() + usize::from(reply.incomplete) * 100;
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{}",
                    status, reply.body
                );
                // The CLI may close an HTTP-error response without consuming it.
                let _ = stream.write_all(response.as_bytes()).await;
                if reply.hold_open {
                    server_body_release.notified().await;
                }
                let _ = stream.shutdown().await;
            }
        });
        Self {
            api: format!("http://{addr}"),
            root,
            reply,
            requests,
            body_release,
            server,
        }
    }

    fn respond(&self, status: u16, body: String, incomplete: bool) {
        *self.reply.lock().unwrap() = Reply {
            status,
            body,
            incomplete,
            hold_open: false,
        };
    }

    async fn command(&self, operation: &str, call: bool) -> (i32, Value) {
        let (command, action) = operation.split_once('.').unwrap();
        let command = command.replace('_', "-");
        let input = if operation == "session.list" {
            json!({})
        } else {
            json!({"session_id":SESSION_ID})
        }
        .to_string();
        let args = if call {
            vec!["call", operation, "--input", &input]
        } else if operation == "session.list" {
            vec![command.as_str(), action]
        } else {
            vec![command.as_str(), action, "--session-id", SESSION_ID]
        };
        let mut cli = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        cli.env_clear()
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("CODEX_HOME", self.root.join("codex-home"))
            .env("SNIPER_DATA_DIR", self.root.join("data"))
            .current_dir(&self.root)
            .args(["--output", "compact", "--api", &self.api])
            .args(args)
            .kill_on_drop(true);
        // Windows socket-provider DLL paths can depend on SystemRoot. Preserve
        // that OS dependency without inheriting user, API, or proxy settings.
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            cli.env("SystemRoot", system_root);
        }
        let output = tokio::time::timeout(Duration::from_secs(5), cli.output())
            .await
            .expect("metadata CLI timed out")
            .unwrap();
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(
            output.stdout.iter().filter(|&&byte| byte == b'\n').count(),
            1
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        let exit = output.status.code().unwrap();
        let requests = std::mem::take(&mut *self.requests.lock().unwrap());
        assert_eq!(
            requests.len(),
            2,
            "no fallback reads or retries: operation={operation}, call={call}, exit={exit}, envelope={value}"
        );
        assert!(requests.iter().all(|(method, _)| method == "GET"));
        assert_eq!(requests[0].1, "/api/settings");
        let resource = match operation {
            "session.list" => "/api/sessions",
            "findings.count" => "/api/findings/count",
            "event_log.list" => "/api/event-log",
            _ => unreachable!(),
        };
        assert_eq!(requests[1].1.split('?').next().unwrap(), resource);
        for name in ["home", "codex-home", "data"] {
            assert!(!self.root.join(name).exists(), "CLI created {name}");
        }
        (exit, value)
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_http_error(status: u16, operation: &str, result: &(i32, Value)) {
    let (exit, value) = result;
    assert_eq!(*exit, if status >= 500 { 6 } else { 5 });
    assert_eq!(value["ok"], false);
    assert_eq!(value["operation"], operation);
    assert_eq!(value["schema_version"], "2026-06-22");
    assert_eq!(value["error"]["code"], "HTTP_STATUS_ERROR");
    assert_eq!(value["error"]["retryable"], status >= 500);
    assert_eq!(value["error"]["details"], json!({"status":status}));
    assert!(value["error"]["hint"].is_null());
    assert_eq!(
        value["error"]["message"],
        format!(
            "API read failed ({})",
            reqwest::StatusCode::from_u16(status).unwrap()
        )
    );
    assert!(!value.to_string().contains(PRIVATE_MARKER));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_read_status_wins_over_server_diagnostic_words() {
    let mock = MockApi::new().await;
    for status in [400, 409, 503] {
        for diagnostic in [
            "requires --dry-run or --yes",
            "unknown operation",
            "is not responding",
            "workspace state revision conflict",
            "failed (599 Synthetic)",
            "invalid input",
        ] {
            mock.respond(
                status,
                json!({"error":format!("{PRIVATE_MARKER}: {diagnostic}")}).to_string(),
                false,
            );
            for operation in ["session.list", "findings.count", "event_log.list"] {
                let direct = mock.command(operation, false).await;
                let called = mock.command(operation, true).await;
                assert_http_error(status, operation, &direct);
                assert_eq!(direct, called);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_body_shape_and_additive_fields_do_not_affect_status_or_leak() {
    let mock = MockApi::new().await;
    for status in [404, 503] {
        for body in [
            String::new(),
            format!("plain {PRIVATE_MARKER}"),
            format!("{{invalid JSON {PRIVATE_MARKER}"),
            json!({
                "error":PRIVATE_MARKER,
                "session_id":SESSION_ID,
                "owner_session_id":SESSION_ID,
                "future_field":{"diagnostic":PRIVATE_MARKER},
            })
            .to_string(),
        ] {
            mock.respond(status, body, false);
            let direct = mock.command("session.list", false).await;
            let called = mock.command("session.list", true).await;
            assert_http_error(status, "session.list", &direct);
            assert_eq!(direct, called);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incomplete_error_bodies_still_use_received_http_status() {
    let mock = MockApi::new().await;
    for status in [400, 503] {
        mock.respond(status, PRIVATE_MARKER.to_string(), true);
        let direct = mock.command("session.list", false).await;
        let called = mock.command("session.list", true).await;
        assert_http_error(status, "session.list", &direct);
        assert_eq!(direct, called);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_headers_finish_before_an_unreleased_body() {
    let mock = MockApi::new().await;
    for status in [400, 503] {
        mock.respond(status, String::new(), true);
        mock.reply.lock().unwrap().hold_open = true;
        for call in [false, true] {
            // The server cannot finish the advertised body until after this
            // bounded CLI call returns; no sleeps or production timeout changes.
            let result = mock.command("session.list", call).await;
            mock.body_release.notify_one();
            assert_http_error(status, "session.list", &result);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_status_with_invalid_or_incomplete_json_keeps_decode_contract() {
    let mock = MockApi::new().await;
    for (body, incomplete) in [
        (format!("{{invalid JSON {PRIVATE_MARKER}"), false),
        (format!("[{{\"name\":\"{PRIVATE_MARKER}\"}}]"), false),
        ("[]".to_owned(), true),
    ] {
        mock.respond(200, body, incomplete);
        let direct = mock.command("session.list", false).await;
        let called = mock.command("session.list", true).await;
        assert_eq!(direct, called);
        assert_eq!(direct.0, 1);
        assert_eq!(direct.1["error"]["code"], "CLI_ERROR");
        assert_eq!(direct.1["error"]["retryable"], false);
        assert_eq!(direct.1["error"]["details"], json!({}));
        assert_eq!(
            direct.1["error"]["message"],
            "failed to decode JSON response from /api/sessions"
        );
        assert!(!direct.1.to_string().contains(PRIVATE_MARKER));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_metadata_still_accepts_harmless_additive_fields() {
    let mock = MockApi::new().await;
    mock.respond(
        200,
        json!({"count":0,"future_field":{"fixture":true}}).to_string(),
        false,
    );
    let direct = mock.command("findings.count", false).await;
    let called = mock.command("findings.count", true).await;
    assert_eq!(direct.0, 0);
    assert_eq!(called.0, 0);
    assert_eq!(direct.1, json!({"count":0}));
    assert_eq!(called.1["data"], direct.1);
    assert_eq!(called.1["meta"]["session_id"], SESSION_ID);
}
