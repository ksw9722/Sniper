//! Legacy session metadata only. This synthetic loopback fixture has no runtime,
//! proxy, upstream, credentials, or access to real session data.
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const SESSION: &str = "d33ca670-10da-4ed0-a401-19c6071cfae0";
const OTHER: &str = "e77cc140-68fe-4bc3-910a-c0ce9cf55c86";
const NAME: &str = "Updated session";
const REDIRECT_TARGET: &str = "/synthetic-redirect-target";
const PRIVATE_MARKER: &str = "synthetic-private-diagnostic";
const OPERATIONS: [&str; 4] = ["create", "rename", "switch", "delete"];

#[derive(Clone, Debug, PartialEq)]
struct RecordedRequest {
    method: String,
    path: String,
    body: Option<Value>,
}

#[derive(Clone)]
enum Reply {
    Http {
        status: u16,
        body: String,
    },
    IncompleteHttp {
        status: u16,
        body: String,
        hold_open: bool,
    },
    Redirect(u16),
    Disconnect,
}

struct Fixture {
    api: String,
    root: PathBuf,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    body_release: Arc<tokio::sync::Notify>,
    server: tokio::task::JoinHandle<()>,
}

fn summary(id: &str) -> Value {
    json!({
        "id": id, "name": NAME, "created_at": "2026-10-11T00:00:00Z",
        "updated_at": "2026-10-11T00:00:00Z", "last_opened_at": "2026-10-11T00:00:00Z",
        "request_count": 0, "websocket_count": 0, "event_count": 0,
        "fuzzer_count": 0, "rule_count": 0, "storage_path": "synthetic-unused-path",
        "active": true
    })
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> RecordedRequest {
    let mut bytes = Vec::new();
    let header_end = loop {
        assert!(
            bytes.len() < 16 * 1024,
            "oversized synthetic request headers"
        );
        assert_ne!(stream.read_buf(&mut bytes).await.unwrap(), 0);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    let mut lines = headers.lines();
    let mut request_line = lines.next().unwrap().split_whitespace();
    let method = request_line.next().unwrap().to_owned();
    let path = request_line.next().unwrap().to_owned();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, length)| length.trim().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(content_length < 4096, "oversized synthetic request body");
    while bytes.len() < header_end + content_length {
        assert_ne!(stream.read_buf(&mut bytes).await.unwrap(), 0);
    }
    let body = (content_length > 0)
        .then(|| serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap());
    RecordedRequest { method, path, body }
}

impl Fixture {
    async fn new(reply: Reply) -> Self {
        let root = std::env::temp_dir().join(format!("sniper-session-metadata-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let settings = json!({
            "runtime_instance_id": OTHER, "proxy_addr": "127.0.0.1:0",
            "ui_addr": addr.to_string(), "data_dir": root.join("data").to_string_lossy(),
            "max_entries": 100, "features": ["http_capture", "session_storage", "replay"]
        });
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let body_release = Arc::new(tokio::sync::Notify::new());
        let server_body_release = body_release.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request =
                    tokio::time::timeout(Duration::from_secs(5), read_request(&mut stream))
                        .await
                        .expect("CLI left an incomplete synthetic request");
                recorded.lock().unwrap().push(request.clone());
                let response = if request.method == "GET" && request.path == "/api/settings" {
                    Reply::Http {
                        status: 200,
                        body: settings.to_string(),
                    }
                } else if request.path == REDIRECT_TARGET {
                    // A followed redirect would return success, exposing either a
                    // repeated mutation or a misleading POST-to-GET conversion.
                    Reply::Http {
                        status: 200,
                        body: summary(SESSION).to_string(),
                    }
                } else {
                    reply.clone()
                };
                let (status, body, location, incomplete, hold_open) = match response {
                    Reply::Http { status, body } => (status, body, String::new(), false, false),
                    Reply::IncompleteHttp {
                        status,
                        body,
                        hold_open,
                    } => (status, body, String::new(), true, hold_open),
                    Reply::Redirect(status) => (
                        status,
                        "{}".to_owned(),
                        format!("Location: {REDIRECT_TARGET}\r\n"),
                        false,
                        false,
                    ),
                    // The mutation was recorded, but no acknowledgement arrived.
                    Reply::Disconnect => continue,
                };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n{body}",
                    body.len() + usize::from(incomplete) * 100
                );
                // Response loss is allowed to close the client socket early.
                let _ = stream.write_all(response.as_bytes()).await;
                if hold_open {
                    server_body_release.notified().await;
                }
                let _ = stream.shutdown().await;
            }
        });
        Self {
            api: format!("http://{addr}"),
            root,
            requests,
            body_release,
            server,
        }
    }

    async fn run(&self, operation: &str, id: &str, call: bool) -> (i32, Value) {
        self.run_with_confirmation(operation, id, call, true).await
    }

    async fn run_with_confirmation(
        &self,
        operation: &str,
        id: &str,
        call: bool,
        yes: bool,
    ) -> (i32, Value) {
        let mut input = if matches!(operation, "create" | "list") {
            json!({})
        } else {
            json!({"id": id})
        };
        if matches!(operation, "create" | "rename") {
            input["name"] = json!(NAME);
        }
        let args = if call {
            vec![
                "call".to_owned(),
                format!("session.{operation}"),
                "--input".to_owned(),
                input.to_string(),
            ]
        } else {
            let mut args = vec!["session".to_owned(), operation.to_owned()];
            for (key, value) in input.as_object().unwrap() {
                args.extend([format!("--{key}"), value.as_str().unwrap().to_owned()]);
            }
            args
        };
        let mut cli = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        cli.env_clear()
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("CODEX_HOME", self.root.join("codex-home"))
            .env("SNIPER_DATA_DIR", self.root.join("data"))
            .current_dir(&self.root)
            .args(["--output", "compact", "--api", &self.api])
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if yes {
            cli.arg("--yes");
        }
        // Windows socket-provider DLL paths can depend on SystemRoot. Preserve
        // that OS dependency without inheriting user, API, or proxy settings.
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            cli.env("SystemRoot", system_root);
        }
        let child = cli.spawn().unwrap();
        let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
            .await
            .expect("metadata CLI did not finish")
            .unwrap();
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        assert_eq!(
            output.stdout.iter().filter(|byte| **byte == b'\n').count(),
            1
        );
        let value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("invalid JSON for {args:?}: {error}: {output:?}"));
        assert_eq!(
            std::fs::read_dir(&self.root).unwrap().count(),
            0,
            "CLI wrote files"
        );
        (output.status.code().unwrap(), value)
    }

    fn assert_single_write(&self, operation: &str) {
        let (method, path, body) = match operation {
            "create" => (
                "POST",
                "/api/sessions".to_owned(),
                Some(json!({"name": NAME})),
            ),
            "rename" => (
                "PATCH",
                format!("/api/sessions/{SESSION}"),
                Some(json!({"name": NAME})),
            ),
            "switch" => (
                "POST",
                format!("/api/sessions/{SESSION}/activate"),
                Some(json!({})),
            ),
            "delete" => ("DELETE", format!("/api/sessions/{SESSION}"), None),
            _ => unreachable!(),
        };
        assert_eq!(
            *self.requests.lock().unwrap(),
            vec![
                RecordedRequest {
                    method: "GET".to_owned(),
                    path: "/api/settings".to_owned(),
                    body: None
                },
                RecordedRequest {
                    method: method.to_owned(),
                    path,
                    body
                },
            ],
            "only discovery and the intended mutation are allowed"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_failure(result: &(i32, Value), operation: &str) {
    assert_ne!(result.0, 0, "{result:?}");
    assert_eq!(result.1["ok"], false);
    assert_eq!(result.1["operation"], format!("session.{operation}"));
    assert_eq!(result.1["error"]["retryable"], false);
    assert!(result.1.get("data").is_none());
}

fn assert_http_error(status: u16, operation: &str, result: &(i32, Value)) {
    assert_failure(result, operation);
    assert_eq!(result.0, if status < 500 { 5 } else { 6 });
    assert_eq!(result.1["error"]["code"], "HTTP_STATUS_ERROR");
    assert_eq!(
        result.1["error"]["details"],
        json!({
            "status": status, "idempotent": false, "outcome": "unknown"
        })
    );
    assert_eq!(
        result.1["error"]["message"],
        format!(
            "Session metadata write failed ({}); mutation outcome is unknown",
            reqwest::StatusCode::from_u16(status).unwrap()
        )
    );
    assert_eq!(result.1["error"]["hint"],
        "Inspect session list and current state before deliberately issuing another write; never automatically retry.");
    assert!(!result.1.to_string().contains(PRIVATE_MARKER));
    assert!(!result.1.to_string().contains("saved.v1"));
}

fn success_data(result: (i32, Value), operation: &str, call: bool) -> Value {
    assert_eq!(result.0, 0, "{result:?}");
    if call {
        assert_eq!(result.1["ok"], true);
        assert_eq!(result.1["operation"], format!("session.{operation}"));
        result.1["data"].clone()
    } else {
        result.1
    }
}

#[tokio::test]
async fn redirects_never_follow_or_repeat_session_mutations() {
    for call in [false, true] {
        for operation in OPERATIONS {
            for status in [303, 307, 308] {
                let fixture = Fixture::new(Reply::Redirect(status)).await;
                let result = fixture.run(operation, SESSION, call).await;
                assert_failure(&result, operation);
                assert_http_error(status, operation, &result);
                fixture.assert_single_write(operation);
            }
        }
    }
}

#[tokio::test]
async fn rename_and_switch_reject_other_session_acknowledgements() {
    for call in [false, true] {
        for operation in ["rename", "switch"] {
            let fixture = Fixture::new(Reply::Http {
                status: 200,
                body: summary(OTHER).to_string(),
            })
            .await;
            let result = fixture.run(operation, SESSION, call).await;
            assert_failure(&result, operation);
            assert_eq!(result.0, 1);
            assert_eq!(result.1["error"]["code"], "CLI_ERROR");
            let message = result.1["error"]["message"].as_str().unwrap();
            assert!(message.contains("mutation outcome is unknown"));
            assert!(message.contains("inspect current state before retrying"));
            assert!(!message.contains(OTHER));
            assert!(!message.contains("saved.v1"));
            fixture.assert_single_write(operation);
        }
    }
}

#[tokio::test]
async fn equivalent_uuid_spellings_match_by_value() {
    let id = Uuid::parse_str(SESSION).unwrap();
    let spellings = [
        SESSION.to_uppercase(),
        id.simple().to_string(),
        id.urn().to_string(),
        id.braced().to_string(),
    ];
    for call in [false, true] {
        for operation in ["rename", "switch"] {
            for (index, requested) in spellings.iter().enumerate() {
                let returned = &spellings[(index + 1) % spellings.len()];
                let fixture = Fixture::new(Reply::Http {
                    status: 200,
                    body: summary(returned).to_string(),
                })
                .await;
                let data = success_data(
                    fixture.run(operation, requested, call).await,
                    operation,
                    call,
                );
                assert_eq!(data["id"], SESSION);
                fixture.assert_single_write(operation);
            }
        }
    }
}

#[tokio::test]
async fn successful_summaries_keep_existing_fields_and_accept_additive_fields() {
    for call in [false, true] {
        for operation in ["create", "rename", "switch"] {
            // Creation returns a server-selected UUID. Existing summary fields
            // remain server-authoritative; only rename/switch identity is bound.
            let mut expected = summary(if operation == "create" {
                OTHER
            } else {
                SESSION
            });
            expected["active"] = json!(false);
            expected["name"] = json!("Server-normalized session");
            let mut response = expected.clone();
            response["future_metadata"] = json!({"harmless": true});
            let fixture = Fixture::new(Reply::Http {
                status: 200,
                body: response.to_string(),
            })
            .await;
            let data = success_data(fixture.run(operation, SESSION, call).await, operation, call);
            assert_eq!(data, expected);
            fixture.assert_single_write(operation);
        }
    }
}

#[tokio::test]
async fn delete_preserves_status_only_success_contract() {
    for call in [false, true] {
        for (status, body) in [
            (200, "not JSON"),
            (200, "{\"ok\":false}"),
            (202, "{}"),
            (204, ""),
        ] {
            let fixture = Fixture::new(Reply::Http {
                status,
                body: body.to_owned(),
            })
            .await;
            let data = success_data(fixture.run("delete", SESSION, call).await, "delete", call);
            assert_eq!(data, json!({"ok": true, "id": SESSION}));
            fixture.assert_single_write("delete");
        }
    }
}

#[tokio::test]
async fn lost_or_failed_acknowledgements_are_never_retried() {
    for call in [false, true] {
        for operation in OPERATIONS {
            let mut replies = vec![
                Reply::Disconnect,
                Reply::Http {
                    status: 503,
                    body: "temporary synthetic failure".to_owned(),
                },
            ];
            if operation != "delete" {
                replies.push(Reply::Http {
                    status: 200,
                    body: "not JSON".to_owned(),
                });
                replies.push(Reply::Http {
                    status: 200,
                    body: "{}".to_owned(),
                });
            }
            for reply in replies {
                let has_error_status = matches!(&reply, Reply::Http { status: 503, .. });
                let fixture = Fixture::new(reply).await;
                let result = fixture.run(operation, SESSION, call).await;
                assert_failure(&result, operation);
                if has_error_status {
                    assert_http_error(503, operation, &result);
                } else {
                    assert_eq!(result.0, 1);
                    assert_eq!(result.1["error"]["code"], "CLI_ERROR");
                }
                fixture.assert_single_write(operation);
            }
        }
    }
}

#[tokio::test]
async fn actual_write_status_wins_over_server_diagnostic_words() {
    for status in [303, 400, 409, 503] {
        for diagnostic in [
            "requires --dry-run or --yes",
            "unknown operation",
            "is not responding",
            "workspace state revision conflict",
            "failed (599 Synthetic)",
            "invalid input",
        ] {
            for operation in OPERATIONS {
                let mut results = Vec::new();
                for call in [false, true] {
                    let fixture = Fixture::new(Reply::Http {
                        status,
                        body: format!("{PRIVATE_MARKER}: {diagnostic}"),
                    })
                    .await;
                    let result = fixture.run(operation, SESSION, call).await;
                    assert_http_error(status, operation, &result);
                    fixture.assert_single_write(operation);
                    results.push(result);
                }
                assert_eq!(results[0], results[1]);
            }
        }
    }
}

#[tokio::test]
async fn error_body_shapes_do_not_affect_status_or_leak() {
    for status in [404, 503] {
        for body in [
            String::new(),
            format!("plain {PRIVATE_MARKER}"),
            format!("{{invalid JSON {PRIVATE_MARKER}"),
            json!({"error":format!("{PRIVATE_MARKER}: requires --dry-run or --yes"),
                "session_id":SESSION, "owner_session_id":OTHER,
                "future_field":{"diagnostic":PRIVATE_MARKER}})
            .to_string(),
        ] {
            for operation in OPERATIONS {
                for call in [false, true] {
                    let fixture = Fixture::new(Reply::Http {
                        status,
                        body: body.clone(),
                    })
                    .await;
                    let result = fixture.run(operation, SESSION, call).await;
                    assert_http_error(status, operation, &result);
                    fixture.assert_single_write(operation);
                }
            }
        }
    }
}

#[tokio::test]
async fn error_headers_finish_without_a_complete_or_released_body() {
    for status in [400, 503] {
        for hold_open in [false, true] {
            for operation in OPERATIONS {
                for call in [false, true] {
                    let fixture = Fixture::new(Reply::IncompleteHttp {
                        status,
                        body: if hold_open {
                            String::new()
                        } else {
                            PRIVATE_MARKER.to_owned()
                        },
                        hold_open,
                    })
                    .await;
                    // When held, the advertised body cannot finish until after
                    // this bounded CLI call returns. No sleeps or timeout changes.
                    let result = fixture.run(operation, SESSION, call).await;
                    fixture.body_release.notify_one();
                    assert_http_error(status, operation, &result);
                    fixture.assert_single_write(operation);
                }
            }
        }
    }
}

#[tokio::test]
async fn missing_confirmation_is_still_local_and_sends_nothing() {
    for operation in OPERATIONS {
        for call in [false, true] {
            let fixture = Fixture::new(Reply::Disconnect).await;
            let result = fixture
                .run_with_confirmation(operation, SESSION, call, false)
                .await;
            assert_failure(&result, operation);
            assert_eq!(result.0, 2);
            assert_eq!(result.1["error"]["code"], "CONFIRMATION_REQUIRED");
            assert_eq!(
                result.1["error"]["details"],
                json!({"operation":format!("session.{operation}")})
            );
            assert!(result.1["error"]["message"]
                .as_str()
                .unwrap()
                .contains("requires --dry-run or --yes"));
            assert!(fixture.requests.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn unrelated_session_list_and_reveal_keep_existing_http_error_contracts() {
    for call in [false, true] {
        for operation in ["list", "reveal"] {
            let fixture = Fixture::new(Reply::Http {
                status: 503,
                body: "synthetic unrelated failure".to_owned(),
            })
            .await;
            let result = fixture.run(operation, SESSION, call).await;
            assert_eq!(result.0, 6);
            assert_eq!(result.1["error"]["code"], "HTTP_STATUS_ERROR");
            let (method, path, body) = if operation == "list" {
                assert_eq!(
                    result.1["error"]["message"],
                    "API read failed (503 Service Unavailable)"
                );
                assert_eq!(result.1["error"]["retryable"], true);
                assert_eq!(result.1["error"]["details"], json!({"status":503}));
                assert!(result.1["error"]["hint"].is_null());
                ("GET", "/api/sessions".to_owned(), None)
            } else {
                assert_eq!(result.1["error"]["message"], format!(
                    "request to /api/sessions/{SESSION}/reveal failed (503 Service Unavailable): synthetic unrelated failure"
                ));
                assert_eq!(result.1["error"]["retryable"], false);
                assert_eq!(
                    result.1["error"]["details"],
                    json!({"status":503,"idempotent":false})
                );
                assert_eq!(result.1["error"]["hint"],
                    "This operation may have partially applied; inspect current state before retrying.");
                (
                    "POST",
                    format!("/api/sessions/{SESSION}/reveal"),
                    Some(json!({})),
                )
            };
            assert_eq!(
                *fixture.requests.lock().unwrap(),
                vec![
                    RecordedRequest {
                        method: "GET".to_owned(),
                        path: "/api/settings".to_owned(),
                        body: None
                    },
                    RecordedRequest {
                        method: method.to_owned(),
                        path,
                        body
                    },
                ]
            );
        }
    }
}
