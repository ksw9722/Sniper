//! A raw request handed to `sniper-cli replay` names no scheme of its own, so the
//! scheme given on the command line has to become the request's scheme. Left at the
//! parser's default (or at the scheme of the placeholder `replay open` starts from),
//! `--scheme http` reads as a target override, which the server refuses outright
//! when the request's host is an IP address: plain-HTTP replay to `127.0.0.1` or any
//! other address could not be set up from the CLI at all.
use std::{path::Path, sync::Arc};

use serde_json::Value;
use sniper::{config::AppConfig, state::AppState};
use uuid::Uuid;

async fn command(dir: &Path, api: &str, args: &[&str]) -> (bool, Value) {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"))
        .env("SNIPER_DATA_DIR", dir)
        .args(["--output", "compact", "--api", api])
        .args(args)
        .output()
        .await
        .unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "invalid CLI JSON: {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value)
}

fn tab_id(value: &Value) -> String {
    let tab = value.get("data").unwrap_or(value);
    tab["id"].as_str().expect("tab id").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_raw_request_follows_the_cli_scheme_on_an_ip_address_host() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_port = upstream.local_addr().unwrap().port();
    let upstream_app = axum::Router::new().fallback(|| async { "plain-http-upstream" });
    let upstream_server = tokio::spawn(async move {
        axum::serve(upstream, upstream_app).await.unwrap();
    });

    let dir = std::env::temp_dir().join(format!("sniper-replay-scheme-{}", Uuid::new_v4()));
    let state = Arc::new(
        AppState::new(AppConfig {
            data_dir: dir.clone(),
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            ui_addr: "127.0.0.1:0".parse().unwrap(),
            max_entries: 100,
            max_transaction_entries: 100,
            body_preview_bytes: 1024,
        })
        .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = sniper::api::router(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let request_file = dir.join("raw-request.txt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        &request_file,
        format!("GET /raw HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\n\r\n"),
    )
    .unwrap();
    let request_file = request_file.to_str().unwrap();
    let port = upstream_port.to_string();
    let target = [
        "--request-file",
        request_file,
        "--scheme",
        "http",
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--yes",
    ];

    // Opened straight from the file.
    let mut open = vec!["replay", "open"];
    open.extend(target);
    let (ok, opened) = command(&dir, &api, &open).await;
    assert!(ok, "{opened}");
    let opened_tab = tab_id(&opened);

    // Opened as a placeholder first, then filled in, the way an agent drives it.
    let (ok, placeholder) = command(&dir, &api, &["replay", "open", "--yes"]).await;
    assert!(ok, "{placeholder}");
    let filled_tab = tab_id(&placeholder);
    let mut update = vec!["replay", "update", "--tab-id", &filled_tab];
    update.extend(target);
    let (ok, filled) = command(&dir, &api, &update).await;
    assert!(ok, "{filled}");

    for tab in [&opened_tab, &filled_tab] {
        let (ok, sent) = command(&dir, &api, &["replay", "send", "--tab-id", tab, "--yes"]).await;
        assert!(ok, "{sent}");
        assert!(
            sent.to_string().contains("plain-http-upstream"),
            "the replay reached the plain-HTTP upstream: {sent}"
        );
    }

    server.abort();
    upstream_server.abort();
    let _ = std::fs::remove_dir_all(&dir);
}
