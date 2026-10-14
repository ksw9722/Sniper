//! Contract for the small vendored reqwest extension used by proxy and Replay.
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::body::Body;
use http::{Request, Response, StatusCode, Uri, Version};
use hyper::{body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use tokio::net::TcpListener;

async fn origin() -> (
    SocketAddr,
    Arc<Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let targets = Arc::new(Mutex::new(Vec::new()));
    let observed = targets.clone();
    let task = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let observed = observed.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request: Request<Incoming>| {
                    let target = request.uri().path_and_query().unwrap().as_str().to_owned();
                    let mut observed = observed.lock().unwrap();
                    observed.push(target.clone());
                    let status = if target.contains("retry") && observed.len() == 1 {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else if target.contains("redirect") {
                        StatusCode::FOUND
                    } else {
                        StatusCode::OK
                    };
                    let response = Response::builder()
                        .status(status)
                        .header("Location", "/destination?own=query")
                        .body(Body::from(target))
                        .unwrap();
                    async { Ok::<_, Infallible>(response) }
                });
                let _ = Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(socket), service)
                    .await;
            });
        }
    });
    (address, targets, task)
}

fn client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
}

#[tokio::test]
async fn raw_target_preserves_http1_http2_and_request_clones() {
    let (address, _, task) = origin().await;
    let targets = [
        "/a/../b",
        "/a/%2e%2E/b?x=%2f&x=%2F+%20",
        "/./a/.%2e/b",
        "/a/../b?",
        "//a///b/%2f/%5C?empty=&bare&x=../",
        "/a\\b/../c?x=%26%3d",
        "/ordinary",
    ];
    for h2 in [false, true] {
        let builder = if h2 {
            client().http2_prior_knowledge()
        } else {
            client().http1_only()
        };
        let client = builder.build().unwrap();
        for target in targets {
            let uri: Uri = format!("http://{address}{target}").parse().unwrap();
            let request = client
                .get(uri.to_string())
                .raw_path_and_query(uri.path_and_query().unwrap().clone())
                .build()
                .unwrap();
            for request in [request.try_clone().unwrap(), request] {
                let response = client.execute(request).await.unwrap();
                assert_eq!(
                    response.version(),
                    if h2 {
                        Version::HTTP_2
                    } else {
                        Version::HTTP_11
                    }
                );
                assert_eq!(response.text().await.unwrap(), target);
            }
        }
    }
    task.abort();
}

#[tokio::test]
async fn redirect_gets_its_own_target_without_reapplying_override() {
    let (address, targets, task) = origin().await;
    let response = client()
        .build()
        .unwrap()
        .get(format!("http://{address}/redirect/../initial"))
        .raw_path_and_query("/redirect/../initial?raw=%2f".parse().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "/destination?own=query");
    assert_eq!(
        *targets.lock().unwrap(),
        ["/redirect/../initial?raw=%2f", "/destination?own=query"]
    );
    task.abort();
}

#[tokio::test]
async fn disabled_redirect_and_retry_keep_raw_target() {
    let (address, targets, task) = origin().await;
    let policy = reqwest::retry::for_host("127.0.0.1")
        .no_budget()
        .max_retries_per_request(1)
        .classify_fn(|result| {
            if result.status() == Some(StatusCode::SERVICE_UNAVAILABLE) {
                result.retryable()
            } else {
                result.success()
            }
        });
    let client = client()
        .retry(policy)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let target = "/retry/../redirect?raw=%2E";
    let response = client
        .get(format!("http://{address}{target}"))
        .raw_path_and_query(target.parse().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(response.text().await.unwrap(), target);
    assert_eq!(*targets.lock().unwrap(), [target, target]);
    task.abort();
}

#[tokio::test]
async fn raw_override_rejects_non_origin_forms_and_stale_url() {
    let client = client().build().unwrap();
    for target in ["*", "relative", "http://example.com/absolute"] {
        assert!(client
            .get("http://127.0.0.1:1/")
            .raw_path_and_query(target.parse().unwrap())
            .build()
            .is_err());
    }
    for target in ["/bad\r\nInjected: yes", "/bad\0target", "/bad path"] {
        assert!(target.parse::<http::uri::PathAndQuery>().is_err());
    }
    let mut request = client
        .get("http://127.0.0.1:1/a/../b")
        .raw_path_and_query("/a/../b".parse().unwrap())
        .build()
        .unwrap();
    request.url_mut().set_path("/changed");
    let error = client.execute(request).await.unwrap_err();
    assert!(error.is_builder());
    assert!(error.to_string().contains("builder error"));
    let error = client
        .get("http://127.0.0.1:1/a/../b")
        .raw_path_and_query("/a/../b".parse().unwrap())
        .query(&[("changed", "yes")])
        .send()
        .await
        .unwrap_err();
    assert!(error.is_builder());
}
