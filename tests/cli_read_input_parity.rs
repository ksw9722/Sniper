//! Exact read inputs must survive both CLI entry points, discovery and transport.
//! All responses are synthetic and all requests stay on an isolated loopback API.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

const SESSION: &str = "00000000-0000-4000-8000-000000000001";
const ORIGIN: &str = "https://example.com";
const SORT_KEYS: [&str; 11] = [
    "index",
    "host",
    "method",
    "path",
    "status",
    "length",
    "mime",
    "notes",
    "tls",
    "edited",
    "started_at",
];

struct Fixture {
    root: PathBuf,
    api: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("sniper-read-input-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let settings = json!({
            "runtime_instance_id": SESSION, "proxy_addr": "127.0.0.1:0",
            "ui_addr": addr.to_string(), "data_dir": root.to_string_lossy(),
            "max_entries": 100, "features": ["http_capture", "session_storage", "replay"]
        });
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let app = axum::Router::new().fallback(move |method: axum::http::Method, uri: axum::http::Uri| {
            let recorded = recorded.clone();
            let settings = settings.clone();
            async move {
                recorded.lock().unwrap().push((method.to_string(), uri.to_string()));
                assert_eq!(method, axum::http::Method::GET);
                let body = match uri.path() {
                    "/api/settings" => settings,
                    "/api/transactions-page" => json!({
                        "items": [], "total": 10, "filtered_total": 4,
                        "hidden_connect_total": 0, "offset": 0, "limit": 1, "has_more": true
                    }),
                    "/api/target/service-map/parameters" => {
                        let url = url::Url::parse(&format!("http://example.com{uri}")).unwrap();
                        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
                        json!({
                            "schema_version": 1, "session_id": query["session_id"],
                            "available": true, "origin": query["origin"],
                            "method": query["method"], "path": query["path"],
                            "rows": 1, "more_rows": false, "omitted_parameters": 0,
                            "parameters": [{"name": "q", "location": "query", "rows": 1, "examples": ["synthetic"]}]
                        })
                    }
                    _ => panic!("unexpected request: {uri}"),
                };
                axum::Json(body)
            }
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            root,
            api: format!("http://{addr}"),
            requests,
            server,
        }
    }

    async fn cli(&self, args: &[&str], exit: i32) -> Value {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"));
        command
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("CODEX_HOME", self.root.join("codex-home"))
            .env("SNIPER_DATA_DIR", self.root.join("data"))
            .current_dir(&self.root)
            .args(["--output", "compact", "--api", &self.api])
            .args(args)
            .kill_on_drop(true);
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        let output = tokio::time::timeout(Duration::from_secs(5), command.output())
            .await
            .expect("read-input CLI timed out")
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(exit),
            "args={args:?}, {output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn assert_requests(&self, expected: &[&str]) {
        let requests = std::mem::take(&mut *self.requests.lock().unwrap());
        assert_eq!(
            requests,
            expected
                .iter()
                .map(|path| ("GET".into(), path.to_string()))
                .collect::<Vec<_>>()
        );
        for name in ["home", "codex-home", "data"] {
            assert!(!self.root.join(name).exists(), "CLI created {name}");
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn history_sort_keys_agree_in_discovery_dry_runs_and_requests() {
    let f = Fixture::new().await;
    let schema = f.cli(&["schema", "input", "capture.http.list"], 0).await;
    for key in SORT_KEYS {
        let input = json!({"session_id":SESSION,"sort_key":key,"limit":1,"page":true}).to_string();
        let path = format!("/api/transactions-page?session_id={SESSION}&limit=1&sort_key={key}");
        for dry_run in [true, false] {
            let mut direct = vec![
                "capture",
                "http",
                "list",
                "--session-id",
                SESSION,
                "--sort-key",
                key,
                "--limit",
                "1",
                "--page",
            ];
            let mut call = vec!["call", "capture.http.list", "--input", &input];
            if dry_run {
                direct.push("--dry-run");
                call.push("--dry-run");
            }
            let typed = f.cli(&direct, 0).await;
            let canonical = f.cli(&call, 0).await;
            assert_eq!(canonical["ok"], true);
            assert_eq!(typed, canonical["data"]);
            if dry_run {
                assert_eq!(typed["input"]["sort_key"], key);
                assert_eq!(
                    typed["api"],
                    json!({"method":"GET","path":path,"body":null})
                );
                f.assert_requests(&[]);
            } else {
                assert_eq!(typed["has_more"], true);
                assert_eq!(typed["filtered_total"], 4);
                f.assert_requests(&["/api/settings", &path, "/api/settings", &path]);
            }
        }
    }
    let mut expected_keys: Vec<Value> = SORT_KEYS.into_iter().map(Value::from).collect();
    expected_keys.push(Value::Null);
    assert_eq!(
        schema["schema"]["properties"]["sort_key"]["enum"],
        json!(expected_keys)
    );
    let omitted = f.cli(&["capture", "http", "list", "--dry-run"], 0).await;
    let null = f
        .cli(
            &[
                "call",
                "capture.http.list",
                "--input",
                r#"{"sort_key":null}"#,
                "--dry-run",
            ],
            0,
        )
        .await;
    assert_eq!(omitted, null["data"]);
    for invalid in [
        json!("editted"),
        json!("Edited"),
        json!("edited "),
        json!(""),
        json!(1),
    ] {
        let input = json!({"sort_key":invalid}).to_string();
        let error = f
            .cli(
                &["call", "capture.http.list", "--input", &input, "--dry-run"],
                2,
            )
            .await;
        assert_eq!(error["error"]["code"], "INVALID_INPUT");
    }
    f.assert_requests(&[]);
}

#[tokio::test]
async fn service_map_method_tokens_preserve_exact_spelling_and_reject_invalid_inputs() {
    let f = Fixture::new().await;
    let long = "X".repeat(64);
    for method in [
        "GET",
        "M-SEARCH2!",
        "X.CUSTOM",
        "X+CUSTOM",
        "gEt-Custom",
        "!#$%&'*+-.^_`|~",
        &long,
    ] {
        let input =
            json!({"session_id":SESSION,"origin":ORIGIN,"method":method,"path":"/v1/items"})
                .to_string();
        let method_arg = format!("--method={method}");
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("origin", ORIGIN)
            .append_pair("method", method)
            .append_pair("path", "/v1/items")
            .append_pair("session_id", SESSION)
            .finish();
        let path = format!("/api/target/service-map/parameters?{query}");
        for dry_run in [true, false] {
            let mut direct = vec![
                "scope",
                "service-map",
                "parameters",
                "--session-id",
                SESSION,
                "--origin",
                ORIGIN,
                &method_arg,
                "--path",
                "/v1/items",
            ];
            let mut call = vec![
                "call",
                "target.service_map.parameters.get",
                "--input",
                &input,
            ];
            if dry_run {
                direct.push("--dry-run");
                call.push("--dry-run");
            }
            let typed = f.cli(&direct, 0).await;
            let canonical = f.cli(&call, 0).await;
            assert_eq!(canonical["ok"], true);
            assert_eq!(typed, canonical["data"]);
            if dry_run {
                assert_eq!(typed["input"]["method"], method);
                assert_eq!(
                    typed["api"],
                    json!({"method":"GET","path":path,"body":null})
                );
                f.assert_requests(&[]);
            } else {
                assert_eq!(typed["method"], method);
                assert_eq!(typed["rows"], 1);
                f.assert_requests(&["/api/settings", &path, "/api/settings", &path]);
            }
        }
    }
    let too_long = "X".repeat(65);
    for method in [
        "",
        " GET",
        "GET ",
        "GET POST",
        "GET\t",
        "GET\r",
        "GET\n",
        "G\u{7f}ET",
        "GÉT",
        "GET\u{a0}",
        "X/Y",
        "X:Y",
        "X(Y)",
        &too_long,
    ] {
        let input =
            json!({"session_id":SESSION,"origin":ORIGIN,"method":method,"path":"/"}).to_string();
        let method_arg = format!("--method={method}");
        for args in [
            vec![
                "scope",
                "service-map",
                "parameters",
                "--session-id",
                SESSION,
                "--origin",
                ORIGIN,
                &method_arg,
                "--path",
                "/",
                "--dry-run",
            ],
            vec![
                "call",
                "target.service_map.parameters.get",
                "--input",
                &input,
                "--dry-run",
            ],
        ] {
            let error = f.cli(&args, 2).await;
            assert_eq!(error["error"]["code"], "INVALID_INPUT");
            f.assert_requests(&[]);
        }
    }
}
