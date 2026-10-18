//! Cache regressions use isolated synthetic history and a loopback API.
use serde_json::Value;
use sniper::{
    config::AppConfig,
    model::{MessageRecord, TransactionRecord},
    state::AppState,
};
use std::sync::Arc;

async fn setup() -> (
    Arc<AppState>,
    String,
    tokio::task::JoinHandle<()>,
    std::path::PathBuf,
) {
    let dir = std::env::temp_dir().join(format!("sniper-map-cache-api-{}", uuid::Uuid::new_v4()));
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
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (state, api, server, dir)
}

async fn map(client: &reqwest::Client, url: &str) -> Value {
    let response = client.get(url).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.unwrap()
}

fn row() -> TransactionRecord {
    let message = MessageRecord::from_headers_and_body(&Default::default(), b"", 1024);
    TransactionRecord::http(
        chrono::Utc::now(),
        "GET".into(),
        "https".into(),
        "www.example.com".into(),
        "/".into(),
        Some(200),
        0,
        message.clone(),
        Some(message),
        vec![],
        None,
        None,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_map_cache_annotation_refresh() {
    for (filter, field, value) in [
        ("only_notes=true", "user_note", "synthetic note"),
        ("color_tags=red", "color_tag", "red"),
    ] {
        let (state, api, server, dir) = setup().await;
        let session = state.session().await;
        let record = row();
        let id = record.id;
        session.store.insert(record).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("{api}/api/target/service-map?all_rows=true&{filter}&fresh=true");
        assert_eq!(map(&client, &url).await["snapshot"]["included_rows"], 0);
        let annotation_url = format!("{api}/api/transactions/{id}/annotations");
        for (annotation, included) in [(serde_json::json!(value), 1), (Value::Null, 0)] {
            let payload = serde_json::json!({field: annotation});
            let response = client
                .patch(&annotation_url)
                .json(&payload)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            assert_eq!(
                map(&client, &url).await["snapshot"]["included_rows"],
                included,
                "{filter}: {payload}"
            );
        }
        server.abort();
        let _ = server.await;
        drop(session);
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_map_cache_annotation_rollback_refresh() {
    for (filter, color, note) in [
        ("only_notes=true", None, Some("synthetic note".to_string())),
        ("color_tags=red", Some("red".to_string()), None),
    ] {
        let (state, api, server, dir) = setup().await;
        let session = state.session().await;
        let mut record = row();
        let id = record.id;
        record.color_tag = color.clone();
        record.user_note = note.clone();
        session.store.insert(record).await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("{api}/api/target/service-map?all_rows=true&{filter}&fresh=true");
        let before = map(&client, &url).await;
        assert_eq!(before["snapshot"]["included_rows"], 1);
        let evidence = before["origins"][0]["endpoints"][0]["evidence"][0]
            .as_str()
            .unwrap();
        assert!(session
            .store
            .restore_annotations_if_current(id, color, note, None, None)
            .await
            .unwrap());
        assert_eq!(
            map(&client, &url).await["snapshot"]["included_rows"],
            0,
            "{filter}"
        );
        let source = map(
            &client,
            &format!(
                "{api}/api/target/service-map/evidence?session_id={}&evidence_id={evidence}",
                session.id()
            ),
        )
        .await;
        assert_eq!(
            source["available"], true,
            "annotations preserve evidence identity"
        );
        server.abort();
        let _ = server.await;
        drop(session);
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_map_cache_catalog_negotiation() {
    let (state, api, server, dir) = setup().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("{api}/api/target/service-map?all_rows=true");
    for selector in [None, Some("1"), None, Some("2"), Some("1"), None] {
        let mut request = client.get(&url);
        if let Some(selector) = selector {
            request = request.header("x-sniper-technology-catalog-version", selector);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let response: Value = response.json().await.unwrap();
        assert_eq!(
            response.get("technology_catalog_version"),
            selector
                .map(|v| serde_json::json!(v.parse::<u8>().unwrap()))
                .as_ref(),
            "selector {selector:?}"
        );
    }
    server.abort();
    let _ = server.await;
    drop(state);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_map_cache_body_versions_keep_exact_api_evidence() {
    let (state, api, server, dir) = setup().await;
    let session = state.session().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("{api}/api/target/service-map?all_rows=true&fresh=true");
    let body = |version: &str| {
        let mut headers = http::HeaderMap::new();
        headers.insert("content-type", "application/javascript".parse().unwrap());
        MessageRecord::from_headers_and_body(
            &headers,
            format!("/*! jQuery v{version} | (c) OpenJS Foundation */").as_bytes(),
            1024,
        )
    };
    let mut old = row();
    old.path = "/vendor.js".into();
    old.response = Some(body("3.6.0"));
    session.store.insert(old).await;
    let first = map(&client, &url).await;
    assert_eq!(
        first["origins"][0]["front_technologies"][0]["version"],
        "3.6.0"
    );
    let mut new = row();
    new.path = "/vendor.js".into();
    new.response = Some(body("3.7.1"));
    let id = new.id;
    session.store.insert(new).await;
    for version in ["3.7.1", "3.5.0"] {
        if version == "3.5.0" {
            session
                .store
                .update_record(id, |row| row.response = Some(body(version)))
                .await
                .unwrap();
        }
        let next = map(&client, &url).await;
        let technologies = next["origins"][0]["front_technologies"].as_array().unwrap();
        assert_eq!(technologies.len(), 2);
        let matching = technologies
            .iter()
            .find(|item| {
                item["evidence"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|token| token.as_str().unwrap().ends_with(&id.to_string()))
            })
            .unwrap();
        assert_eq!(matching["version"], version);
        assert_eq!(matching["row_count"], 1);
    }
    server.abort();
    let _ = server.await;
    drop(session);
    drop(state);
    let _ = std::fs::remove_dir_all(dir);
}
