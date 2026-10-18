//! Scoped body-budget regressions use synthetic history and a loopback API.
use serde_json::Value;
use sniper::{
    config::AppConfig,
    model::{MessageRecord, TransactionRecord},
    runtime::RuntimeSettingsUpdate,
    state::AppState,
};
use std::sync::Arc;

fn row(host: &str, mime: &str, body: &[u8], referer: Option<&str>) -> TransactionRecord {
    let mut request_headers = http::HeaderMap::new();
    if let Some(referer) = referer {
        request_headers.insert("referer", referer.parse().unwrap());
    }
    let mut response_headers = http::HeaderMap::new();
    response_headers.insert("content-type", mime.parse().unwrap());
    TransactionRecord::http(
        chrono::Utc::now(),
        "GET".into(),
        "https".into(),
        host.into(),
        if mime == "text/html" {
            "/"
        } else {
            "/vendor.js"
        }
        .into(),
        Some(200),
        0,
        MessageRecord::from_headers_and_body(&request_headers, b"", 1024),
        Some(MessageRecord::from_headers_and_body(
            &response_headers,
            body,
            1024,
        )),
        vec![],
        None,
        None,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scoped_service_map_finds_older_page_and_cdn_bodies_on_first_read() {
    let dir = std::env::temp_dir().join(format!("sniper-map-scoped-api-{}", uuid::Uuid::new_v4()));
    let state = Arc::new(
        AppState::new(AppConfig {
            data_dir: dir.clone(),
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            ui_addr: "127.0.0.1:0".parse().unwrap(),
            max_entries: 1000,
            max_transaction_entries: 1000,
            body_preview_bytes: 1024,
        })
        .unwrap(),
    );
    let session = state.session().await;
    session
        .runtime
        .update(RuntimeSettingsUpdate {
            scope_patterns: Some(vec!["www.example.com".into()]),
            ..Default::default()
        })
        .await
        .unwrap();
    let page = row(
        "www.example.com",
        "text/html",
        br#"<meta name="generator" content="WordPress 6.5.2">"#,
        None,
    );
    let page_id = page.id.to_string();
    session.store.insert(page).await;
    let script = row(
        "cdn.example.com",
        "application/javascript",
        b"/*! jQuery v3.7.1 | (c) OpenJS Foundation */",
        Some("https://www.example.com/"),
    );
    let script_id = script.id.to_string();
    session.store.insert(script).await;
    for i in 0..300 {
        let host = format!("p{i}.example.com");
        session
            .store
            .insert(row(
                &host,
                "text/html",
                br#"<meta name="generator" content="Drupal 10.0.0">"#,
                None,
            ))
            .await;
        session
            .store
            .insert(row(
                "cdn.example.com",
                "application/javascript",
                b"/*! jQuery v3.6.0 | (c) OpenJS Foundation */",
                Some(&format!("https://{host}/")),
            ))
            .await;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = sniper::api::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("{api}/api/target/service-map?all_rows=true&in_scope_only=true&fresh=true");
    let mut snapshot = None;
    for _ in 0..3 {
        let response = client
            .get(&url)
            .header("x-sniper-technology-catalog-version", "2")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let map: Value = response.json().await.unwrap();
        assert_eq!(map["snapshot"]["retained_rows"], 602);
        assert_eq!(map["snapshot"]["included_rows"], 1);
        assert_eq!(map["snapshot"]["excluded_rows"], 601);
        assert_eq!(map["snapshot"]["complete"], true);
        assert_eq!(map["origins"].as_array().unwrap().len(), 1);
        let technologies = map["origins"][0]["front_technologies"].as_array().unwrap();
        assert_eq!(
            technologies.len(),
            2,
            "older included bodies must be read immediately"
        );
        for (product, version, source, id) in [
            ("wordpress", "6.5.2", "https://www.example.com", &page_id),
            ("jquery", "3.7.1", "https://cdn.example.com", &script_id),
        ] {
            let found = technologies
                .iter()
                .find(|item| item["product_id"] == product)
                .unwrap();
            assert_eq!(found["version"], version);
            assert_eq!(found["source_origin"], source);
            assert_eq!(found["row_count"], 1);
            let evidence = found["evidence"][0].as_str().unwrap();
            assert!(evidence.ends_with(id));
            let response = client
                .get(format!("{api}/api/target/service-map/evidence"))
                .query(&[
                    ("session_id", session.id().to_string()),
                    ("evidence_id", evidence.to_string()),
                ])
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            assert_eq!(response.json::<Value>().await.unwrap()["available"], true);
        }
        if let Some(snapshot) = &snapshot {
            assert_eq!(
                &map["snapshot"]["id"], snapshot,
                "a fully read unchanged map is reusable"
            );
        }
        snapshot = Some(map["snapshot"]["id"].clone());
    }
    server.abort();
    let _ = server.await;
    drop(session);
    drop(state);
    let _ = std::fs::remove_dir_all(dir);
}
