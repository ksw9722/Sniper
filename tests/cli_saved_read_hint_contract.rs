//! Recovery guidance uses disposable metadata-only mocks, never a Sniper runtime.
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Method, StatusCode, Uri},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const SESSION_ID: &str = "00000000-0000-4000-8000-000000000001";
const OPERATION_ID: &str = "22222222-2222-4222-8222-222222222222";
const RECEIPT_HINT: &str = "Use saved.v1.operation.get with the original operation_id to inspect a receipt; never automatically retry a mutation.";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("sniper-saved-read-hints-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    async fn call(&self, api: &str, operation: &str, input: Value, exit: i32) -> Value {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        command
            .env_clear()
            .env("HOME", self.0.join("home"))
            .env("USERPROFILE", self.0.join("home"))
            .env("CODEX_HOME", self.0.join("codex-home"))
            .env("SNIPER_DATA_DIR", self.0.join("data"))
            .current_dir(&self.0)
            .args([
                "--output",
                "compact",
                "--api",
                api,
                "call",
                operation,
                "--input",
                &input.to_string(),
            ])
            .kill_on_drop(true);
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let output = tokio::time::timeout(Duration::from_secs(5), command.output())
            .await
            .expect("metadata CLI timed out")
            .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert!(!self.0.join("data").exists());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["operation"], operation);
        assert_eq!(result["error"]["retryable"], false);
        result
    }

    async fn served_error(&self, operation: &str, input: Value, wire: Value) -> Value {
        self.served_response(operation, input, StatusCode::CONFLICT, wire.to_string())
            .await
    }

    async fn served_response(
        &self,
        operation: &str,
        input: Value,
        status: StatusCode,
        body: String,
    ) -> Value {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_calls = calls.clone();
        let expected_operation = operation.to_owned();
        let expected_input = input.clone();
        let settings = json!({
            "runtime_instance_id":Uuid::new_v4(), "proxy_addr":"127.0.0.1:0",
            "ui_addr":addr.to_string(), "data_dir":self.0.to_string_lossy(),
            "max_entries":100, "features":["http_capture","session_storage","replay"]
        });
        let app = Router::new()
            .route("/api/settings", get(move || async move { Json(settings) }))
            .route(
                "/api/saved/v1/call",
                post(move |Json(request): Json<Value>| async move {
                    assert_eq!(
                        request,
                        json!({"operation":expected_operation,"input":expected_input})
                    );
                    observed_calls.fetch_add(1, Ordering::SeqCst);
                    axum::http::Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .header("location", "/unexpected")
                        .body(Body::from(body))
                        .unwrap()
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let result = self
            .call(&format!("http://{addr}"), operation, input, 5)
            .await;
        server.abort();
        let _ = server.await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "metadata reads are not retried"
        );
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wire_error(code: &str, outcome: &str, operation_id: Value, session_id: Value) -> Value {
    json!({"ok":false,"contract_version":"saved.v1","error":{
        "code":code,"message":"Synthetic metadata error","outcome":outcome,
        "operation_id":operation_id,"session_id":session_id,"retryable":false
    }})
}

#[tokio::test]
async fn saved_read_failures_offer_actionable_read_recovery() {
    let f = Fixture::new();
    let input = json!({"continuation":{
        "session_id":SESSION_ID,"store_generation":Uuid::new_v4(),
        "before_sequence":2,"limit":1
    }});
    for (code, expected) in [
        (
            "STALE_CONTINUATION",
            "fresh listing with the same explicit session_id",
        ),
        ("SESSION_NOT_FOUND", "saved.v1.session.list"),
        ("SESSION_UNAVAILABLE", "deliberately rerunning this read"),
    ] {
        let result = f
            .served_error(
                "saved.v1.http.list",
                input.clone(),
                wire_error(code, "not_applied", Value::Null, json!(SESSION_ID)),
            )
            .await;
        assert_eq!(result["error"]["code"], code);
        assert_eq!(result["error"]["details"]["session_id"], SESSION_ID);
        assert_eq!(result["error"]["details"]["operation_id"], Value::Null);
        assert_eq!(result["error"]["details"]["outcome"], "not_applied");
        assert_eq!(result["error"]["message"], "Synthetic metadata error");
        let hint = result["error"]["hint"].as_str().unwrap();
        assert!(hint.contains(expected), "{result}");
        assert!(!hint.contains("original operation_id"), "{result}");
    }
    let invalid = f
        .served_error("saved.v1.http.list", input, json!({"unexpected":true}))
        .await;
    assert_eq!(invalid["error"]["code"], "INVALID_RESPONSE");
    assert!(!invalid["error"]["message"]
        .as_str()
        .unwrap()
        .contains("receipt"));
    assert!(invalid["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("deliberately rerunning this read"));
    let sessions = f
        .served_error(
            "saved.v1.session.list",
            json!({"after_id":SESSION_ID,"limit":1}),
            wire_error(
                "STALE_CONTINUATION",
                "not_applied",
                Value::Null,
                Value::Null,
            ),
        )
        .await;
    assert_eq!(sessions["error"]["code"], "STALE_CONTINUATION");
    let hint = sessions["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("fresh session listing"), "{sessions}");
    assert!(hint.contains("same selected API"), "{sessions}");
    assert!(hint.contains("same page limit"), "{sessions}");
    assert!(!hint.contains("session_id"), "{sessions}");
}

#[tokio::test]
async fn saved_read_redirect_and_malformed_messages_match_read_hints() {
    let f = Fixture::new();
    for (status, body, code) in [
        (StatusCode::TEMPORARY_REDIRECT, "{}", "REDIRECT_REFUSED"),
        (StatusCode::OK, "Synthetic invalid JSON", "INVALID_RESPONSE"),
        (
            StatusCode::OK,
            r#"{"ok":true,"contract_version":"saved.v1","data":{}}"#,
            "INVALID_RESPONSE",
        ),
        (
            StatusCode::CONFLICT,
            r#"{"ok":false,"contract_version":"saved.v1"}"#,
            "INVALID_RESPONSE",
        ),
    ] {
        let result = f
            .served_response(
                "saved.v1.session.list",
                json!({"limit":1}),
                status,
                body.to_owned(),
            )
            .await;
        assert_eq!(result["error"]["code"], code);
        assert_eq!(result["error"]["details"]["outcome"], "not_applied");
        assert_eq!(result["error"]["details"]["operation_id"], Value::Null);
        assert!(
            !result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("receipt"),
            "{result}"
        );
        assert!(
            result["error"]["hint"]
                .as_str()
                .unwrap()
                .contains("deliberately rerunning this read"),
            "{result}"
        );
    }
    let lookup = f
        .served_response(
            "saved.v1.operation.get",
            json!({"operation_id":OPERATION_ID}),
            StatusCode::OK,
            "Synthetic invalid JSON".to_owned(),
        )
        .await;
    assert_eq!(
        lookup["error"]["message"],
        "Could not decode the saved-data response; inspect the operation receipt"
    );
    assert_eq!(lookup["error"]["hint"], RECEIPT_HINT);
}

#[tokio::test]
async fn saved_input_and_unavailable_read_hints_are_local_and_actionable() {
    let f = Fixture::new();
    // A listening socket catches accidental API contact without serving a runtime.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let invalid = f
        .call(&api, "saved.v1.http.list", json!({"limit":0}), 2)
        .await;
    assert_eq!(invalid["error"]["code"], "INVALID_INPUT");
    assert!(invalid["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("schema input"));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    // Refused-connect timing varies by OS and can outlast the harness deadline
    // across discovery probes. Reject a complete, promptly served settings
    // response instead, retaining the bound port and the same discovery path.
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed_requests = requests.clone();
    let app = Router::new().fallback(move |method: Method, uri: Uri| async move {
        observed_requests
            .lock()
            .unwrap()
            .push((method, uri.to_string()));
        Json(json!({"fixture":"not a Sniper settings response"}))
    });
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let unavailable = f
        .call(&api, "saved.v1.session.list", json!({"limit":1}), 5)
        .await;
    server.abort();
    let _ = server.await;
    let requests = requests.lock().unwrap();
    assert!(
        !requests.is_empty(),
        "the fixture must exercise API discovery"
    );
    assert!(
        requests
            .iter()
            .all(|(method, path)| *method == Method::GET && path == "/api/settings"),
        "rejected discovery must not send a saved-data call: {requests:?}"
    );
    assert_eq!(unavailable["error"]["code"], "API_UNAVAILABLE");
    assert!(unavailable["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("selected API endpoint"));
    assert!(!unavailable["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("original operation_id"));
}

#[tokio::test]
async fn saved_read_transport_errors_offer_read_context_without_retry() {
    let f = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let input = json!({"session_id":SESSION_ID,"limit":1});
    let expected_input = input.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = calls.clone();
    let settings = json!({
        "runtime_instance_id":Uuid::new_v4(), "proxy_addr":"127.0.0.1:0",
        "ui_addr":addr.to_string(), "data_dir":f.0.to_string_lossy(),
        "max_entries":100, "features":["http_capture","session_storage","replay"]
    })
    .to_string();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(stream.read_u8().await.unwrap());
                assert!(header.len() < 16 * 1024);
            }
            let header = String::from_utf8(header).unwrap();
            if header.starts_with("GET /api/settings ") {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{settings}",
                    settings.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else {
                assert!(header.starts_with("POST /api/saved/v1/call "));
                let len: usize = header
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut body = vec![0; len];
                stream.read_exact(&mut body).await.unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&body).unwrap(),
                    json!({"operation":"saved.v1.http.list","input":expected_input})
                );
                observed_calls.fetch_add(1, Ordering::SeqCst);
                // Deliberately close a read response without invoking any runtime.
            }
            stream.shutdown().await.unwrap();
        }
    });
    let result = f
        .call(&format!("http://{addr}"), "saved.v1.http.list", input, 5)
        .await;
    server.abort();
    let _ = server.await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(result["error"]["code"], "TRANSPORT_ERROR");
    assert_eq!(
        result["error"]["message"],
        "Saved-data read response unavailable"
    );
    assert_eq!(result["error"]["details"]["operation_id"], Value::Null);
    assert_eq!(result["error"]["details"]["outcome"], "not_applied");
    let hint = result["error"]["hint"].as_str().unwrap();
    assert!(
        hint.contains("deliberately rerunning this read"),
        "{result}"
    );
    assert!(!hint.contains("original operation_id"), "{result}");
}

#[tokio::test]
async fn receipt_lookup_mutation_unknown_operation_and_unknown_outcome_keep_receipt_guidance() {
    let f = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    for (operation, input, code) in [
        (
            "saved.v1.session.rename",
            json!({"operation_id":OPERATION_ID,"session_id":SESSION_ID,"name":"Synthetic rename"}),
            "CONFIRMATION_REQUIRED",
        ),
        ("saved.v1.unknown", json!({}), "UNKNOWN_OPERATION"),
    ] {
        let result = f.call(&api, operation, input, 2).await;
        assert_eq!(result["error"]["code"], code);
        assert_eq!(result["error"]["hint"], RECEIPT_HINT);
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let lookup = f
        .served_error(
            "saved.v1.operation.get",
            json!({"operation_id":OPERATION_ID}),
            wire_error(
                "SESSION_UNAVAILABLE",
                "not_applied",
                json!(OPERATION_ID),
                Value::Null,
            ),
        )
        .await;
    assert_eq!(lookup["error"]["code"], "SESSION_UNAVAILABLE");
    assert_eq!(lookup["error"]["details"]["operation_id"], OPERATION_ID);
    assert_eq!(lookup["error"]["hint"], RECEIPT_HINT);
    let unknown = f
        .served_error(
            "saved.v1.http.list",
            json!({"session_id":SESSION_ID}),
            wire_error(
                "SESSION_UNAVAILABLE",
                "unknown",
                Value::Null,
                json!(SESSION_ID),
            ),
        )
        .await;
    assert_eq!(unknown["error"]["details"]["outcome"], "unknown");
    assert_eq!(unknown["error"]["hint"], RECEIPT_HINT);
}
