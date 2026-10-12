//! End-to-end read contracts use inserted synthetic rows and a loopback API only.
//! No proxy listener, upstream service, browser, or captured user data is used.
use std::{path::Path, sync::Arc};

use http::HeaderMap;
use serde_json::{json, Value};
use sniper::{
    config::AppConfig,
    model::{MessageRecord, TransactionRecord},
    state::AppState,
};
use uuid::Uuid;

async fn read(dir: &Path, api: &str, operation: &str, input: Value) -> Value {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_sniper-cli"))
        .env("SNIPER_DATA_DIR", dir)
        .args([
            "--api", api, "--output", "compact", "call", operation, "--input",
        ])
        .arg(input.to_string())
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    assert!(!response.to_string().contains("SYNTHETIC_PRIVATE"));
    response["data"].clone()
}

fn row(path: &str) -> TransactionRecord {
    let mut request_headers = HeaderMap::new();
    request_headers.insert("origin", "https://app.example.com".parse().unwrap());
    request_headers.insert("authorization", "Bearer SYNTHETIC_PRIVATE".parse().unwrap());
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        "location",
        "https://auth.example.com/sign-in?key=SYNTHETIC_PRIVATE"
            .parse()
            .unwrap(),
    );
    response_headers.insert("server", "nginx/SYNTHETIC_PRIVATE".parse().unwrap());
    response_headers.insert("content-type", "text/html; charset=utf-8".parse().unwrap());
    let target = if path.starts_with("/users/1") {
        "older.example.com"
    } else {
        "newer.example.com"
    };
    let response_body = format!(
        "<a href=\"https://user:SYNTHETIC_PRIVATE@{target}/private?token=SYNTHETIC_PRIVATE#SYNTHETIC_PRIVATE\">fixture</a>"
    );
    TransactionRecord::http(
        chrono::Utc::now(),
        "GET".into(),
        "https".into(),
        "api.example.com".into(),
        path.into(),
        Some(302),
        1,
        MessageRecord::from_headers_and_body(&request_headers, b"SYNTHETIC_PRIVATE", 1024),
        Some(MessageRecord::from_headers_and_body(
            &response_headers,
            response_body.as_bytes(),
            1024,
        )),
        Vec::new(),
        None,
        None,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_map_real_read_routes_and_cli_keep_duplicate_rows_and_stale_evidence_exact() {
    let dir = std::env::temp_dir().join(format!("sniper-service-map-read-{}", Uuid::new_v4()));
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
    let session = state.session().await;
    let session_id = session.id();
    let first = row("/users/1?secret=SYNTHETIC_PRIVATE");
    let duplicate_id = first.id;
    let mut second = row("/users/2?secret=SYNTHETIC_PRIVATE");
    second.id = duplicate_id;
    session.store.insert(first).await;
    session.store.insert(second).await;
    // Exercise the real persistence path: the newest UUID occurrence becomes
    // lazy, while its older retained sibling must keep its own resident body.
    session.persist().await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = sniper::api::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (path, content_type, expected) in [
        (
            "/service-map.js",
            "application/javascript; charset=utf-8",
            include_str!("../web/service-map.js"),
        ),
        (
            "/app.js",
            "application/javascript; charset=utf-8",
            include_str!("../web/app.js"),
        ),
        (
            "/styles.css",
            "text/css; charset=utf-8",
            include_str!("../web/styles.css"),
        ),
        (
            "/",
            "text/html; charset=utf-8",
            include_str!("../web/index.html"),
        ),
    ] {
        let asset = client.get(format!("{api}{path}")).send().await.unwrap();
        assert_eq!(asset.status(), reqwest::StatusCode::OK, "{path}");
        assert_eq!(asset.headers()["content-type"], content_type, "{path}");
        assert_eq!(asset.text().await.unwrap(), expected, "{path}");
    }
    let map = read(
        &dir,
        &api,
        "target.service_map.get",
        json!({"session_id":session_id}),
    )
    .await;
    assert_eq!(map["snapshot"]["retained_rows"], 2);
    assert_eq!(map["snapshot"]["complete"], true);
    let origin = &map["origins"][0];
    assert_eq!(origin["request_count"], 2);
    assert_eq!(origin["endpoints"].as_array().unwrap().len(), 2);
    assert_eq!(
        origin["redirects"][0]["target_origin"],
        "https://auth.example.com"
    );
    assert_eq!(origin["redirects"][0]["target_path"], "/sign-in");
    assert_eq!(
        origin["header_links"][0]["source_origin"],
        "https://app.example.com"
    );
    assert_eq!(origin["technology_clues"][0]["name"], "nginx");
    let token = origin["endpoints"][0]["evidence"][0].as_str().unwrap();
    let input = json!({"session_id":session_id,"evidence_id":token});
    let evidence = read(&dir, &api, "target.service_map.evidence.get", input.clone()).await;
    assert_eq!(evidence["available"], true);
    assert_eq!(evidence["row"]["path"], "/users/1");
    assert_eq!(evidence["row"]["record_id"], duplicate_id.to_string());

    let body = read(
        &dir,
        &api,
        "target.service_map.body_references.get",
        input.clone(),
    )
    .await;
    assert_eq!(body["available"], true, "{body}");
    assert_eq!(body["status"], "scanned");
    assert_eq!(
        body["references"][0]["target_origin"],
        "https://older.example.com"
    );
    assert_eq!(body["offset_basis"], "retained_utf8_preview");
    assert_eq!(body["evidence_id"], token);
    assert_eq!(body["source_sequence"], evidence["row"]["sequence"]);
    let newest_token = origin["endpoints"][1]["evidence"][0].as_str().unwrap();
    let lazy_body = read(
        &dir,
        &api,
        "target.service_map.body_references.get",
        json!({"session_id": session_id, "evidence_id": newest_token}),
    )
    .await;
    assert_eq!(lazy_body["available"], true, "{lazy_body}");
    assert_eq!(
        lazy_body["references"][0]["target_origin"],
        "https://newer.example.com"
    );
    assert_ne!(body["source_sequence"], lazy_body["source_sequence"]);

    // Deletion reports distinct IDs; the map must still account for both rows.
    assert_eq!(session.store.delete_ids(&[duplicate_id]).await.unwrap(), 1);
    assert_eq!(session.store.len().await, 0);
    let missing = read(&dir, &api, "target.service_map.evidence.get", input.clone()).await;
    assert_eq!(missing["available"], false);
    assert!(missing["row"].is_null());
    let missing_body = read(&dir, &api, "target.service_map.body_references.get", input).await;
    assert_eq!(missing_body["available"], false);
    assert_eq!(missing_body["status"], "unavailable");
    assert!(missing_body["references"].as_array().unwrap().is_empty());
    let empty = read(
        &dir,
        &api,
        "target.service_map.get",
        json!({"session_id":session_id}),
    )
    .await;
    assert_eq!(empty["snapshot"]["retained_rows"], 0);
    assert!(empty["origins"].as_array().unwrap().is_empty());
    assert_eq!(state.session().await.id(), session_id);

    server.abort();
    let _ = server.await;
    drop(session);
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}
