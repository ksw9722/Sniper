use super::*;
use crate::{
    model::{HeaderRecord, MessageRecord},
    store::TransactionStore,
};
use std::time::Instant;

fn fixture_record(host: &str, sequence: u64) -> TransactionRecord {
    let message = MessageRecord::from_headers_and_body(&http::HeaderMap::new(), b"", 0);
    let mut record = TransactionRecord::http(
        DateTime::<Utc>::from_timestamp(1_760_000_000, 0).unwrap(),
        "GET".into(),
        "https".into(),
        host.into(),
        "/items/1?synthetic=1".into(),
        Some(200),
        1,
        message.clone(),
        Some(message),
        vec![],
        None,
        None,
    );
    record.id = Uuid::from_u128(sequence as u128 + 1);
    record.sequence = sequence;
    record.response.as_mut().unwrap().headers = vec![HeaderRecord {
        name: "Server".into(),
        value: "nginx/1.28.0 Caddy/2.9.1".into(),
    }];
    record
}

fn same_output(mut legacy: ServiceMap, mut bounded: ServiceMap) {
    legacy.snapshot.id = Uuid::nil();
    bounded.snapshot.id = Uuid::nil();
    assert_eq!(
        serde_json::to_vec(&legacy).unwrap(),
        serde_json::to_vec(&bounded).unwrap()
    );
}

fn expensive_scope() -> (Vec<String>, Vec<String>) {
    let include = vec!["*.example.com".into()];
    let exclude = (0..500)
        .map(|i| format!("{}z{i:03}*", "*a".repeat(230)))
        .collect();
    (include, exclude)
}

fn long_host(index: usize) -> String {
    format!(
        "{index:03}.{}.{}.{}.{}.example.com",
        "a".repeat(63),
        "a".repeat(63),
        "a".repeat(63),
        "a".repeat(43)
    )
}

#[test]
fn bounded_scope_raw_semantics_include_ipv6_malformed_and_empty() {
    let values = [
        "",
        " ",
        "*",
        "*.",
        "**",
        "*.*",
        "a*b",
        "ab",
        "a..",
        "*.EXAMPLE.com.",
        "https://API.Example.com:443/path?synthetic#fragment",
        "//api.example.com",
        "API.Example.com.:8443",
        "[2001:db8::1]:443",
        "[::1]ignored",
        "a:invalid",
        "\u{2003}API.example.com\u{2003}",
        "2001:db8::1",
        "[::1",
        "[::1]",
        "127.0.0.1:443",
        "127.1",
        "Ä.Example.com",
        "ä.example.com",
        "example.com@other.example.com",
        "a*b*c",
        "a**b",
        "a*b*",
        "*a*b",
    ];
    for host in values {
        for pattern in values {
            for (include, exclude) in [
                (vec![pattern.to_owned()], vec![]),
                (vec![], vec![pattern.to_owned()]),
            ] {
                let expected = scope::is_host_in_scope(host, &include, &exclude);
                let mut bounded = BoundedScope::new(&include, &exclude).unwrap();
                assert_eq!(
                    bounded.contains(host),
                    Ok(expected),
                    "{host:?}, {pattern:?}"
                );
                let remaining = bounded.remaining;
                assert_eq!(bounded.contains(host), Ok(expected));
                assert_eq!(bounded.remaining, remaining);
            }
        }
    }
    let mut empty = BoundedScope::new(&[], &[]).unwrap();
    empty.remaining = 0;
    for host in values {
        assert_eq!(empty.contains(host), Ok(true));
    }
    assert!(empty.cache.is_empty());
    assert_eq!(empty.cache_key_bytes, 0);
}

#[test]
fn bounded_scope_budget_boundaries_and_completed_true_false_cache() {
    let include = vec!["*.example.com".into()];
    let exclude = vec!["*.blocked.example.com".into()];
    let mut bounded = BoundedScope::new(&include, &exclude).unwrap();
    for (host, expected) in [("api.example.com", true), ("a.blocked.example.com", false)] {
        assert_eq!(bounded.contains(host), Ok(expected));
        let remaining = bounded.remaining;
        assert_eq!(bounded.contains(host), Ok(expected));
        assert_eq!(bounded.remaining, remaining);
        assert_eq!(bounded.cache.get(host), Some(&expected));
    }
    // Canonical aliases must not accidentally reuse the raw-host cache key.
    assert_eq!(bounded.contains("API.Example.com.:443"), Ok(true));
    assert_eq!(bounded.cache.len(), 3);
    let host = "api.example.com";
    let mut measured = BoundedScope::new(&include, &exclude).unwrap();
    measured.contains(host).unwrap();
    let cost = MAX_SCOPE_WORK - measured.remaining;
    assert!(cost > 0);
    let mut exact = BoundedScope::new(&include, &exclude).unwrap();
    exact.remaining = cost;
    assert_eq!(exact.contains(host), Ok(true));
    assert_eq!(exact.remaining, 0);
    assert_eq!(exact.contains(host), Ok(true));
    let mut short = BoundedScope::new(&include, &exclude).unwrap();
    short.remaining = cost - 1;
    assert_eq!(short.contains(host), Err(scope::ScopeWorkLimit));
    assert!(short.cache.is_empty());
    assert_eq!(short.cache_key_bytes, 0);
}

#[test]
fn bounded_scope_cache_entry_and_byte_limits_keep_charging_uncached_hosts() {
    let exclude = vec!["*.blocked.example.com".into()];
    for fill_bytes in [false, true] {
        let mut bounded = BoundedScope::new(&[], &exclude).unwrap();
        let count = if fill_bytes {
            MAX_SCOPE_CACHE_KEY_BYTES / 512
        } else {
            MAX_SCOPE_CACHE_ENTRIES
        };
        for index in 0..count {
            let prefix = format!("{index:03}.");
            let host = if fill_bytes {
                format!("{prefix}{}", "a".repeat(512 - prefix.len()))
            } else {
                format!("{prefix}example.com")
            };
            assert_eq!(bounded.contains(&host), Ok(true));
        }
        assert_eq!(bounded.cache.len(), count);
        if fill_bytes {
            assert_eq!(bounded.cache_key_bytes, MAX_SCOPE_CACHE_KEY_BYTES);
        }
        let bytes = bounded.cache_key_bytes;
        let host = "uncached.example.com";
        let cost = host.len() + exclude[0].len() + 16;
        bounded.remaining = cost * 2;
        assert_eq!(bounded.contains(host), Ok(true));
        assert_eq!(bounded.remaining, cost);
        assert_eq!(bounded.contains(host), Ok(true));
        assert_eq!(bounded.remaining, 0);
        assert_eq!(bounded.contains(host), Err(scope::ScopeWorkLimit));
        assert_eq!(bounded.cache.len(), count);
        assert_eq!(bounded.cache_key_bytes, bytes);
        assert!(!bounded.cache.contains_key(host));
    }
}

#[tokio::test]
async fn bounded_scope_preparation_rejects_instead_of_truncating() {
    let session = Uuid::from_u128(90);
    let store = TransactionStore::from_records(vec![fixture_record("api.example.com", 1)]);
    let snapshot = store.service_map_snapshot(session, 10).await;
    for (include, exclude) in [
        (vec!["example.com".into(); 501], vec![]),
        (vec![], vec!["example.com".into(); 501]),
        (vec!["x".repeat(513)], vec![]),
        (vec![], vec!["x".repeat(513)]),
    ] {
        assert!(matches!(
            build_bounded(
                session,
                snapshot.clone(),
                10,
                false,
                None,
                &include,
                &exclude
            ),
            Err(scope::ScopeWorkLimit)
        ));
    }
    let boundary = vec!["x".repeat(512); 500];
    assert!(build_bounded(session, snapshot, 10, false, None, &boundary, &boundary).is_ok());
}

#[tokio::test]
async fn bounded_scope_complete_v1_v2_outputs_equal_legacy_bytes() {
    let session = Uuid::from_u128(92);
    let hosts = [
        "API.Example.com.:443",
        "api.example.com:8443",
        "outside.example.com",
        "excluded.example.com",
        "[2001:db8::1]:443",
        "[2001:0db8:0:0:0:0:0:1]:443",
        "127.0.0.1:443",
        "Ä.example.com",
        "xn--4ca.example.com",
        "nested.api.example.com",
        "example.com/path",
        "[::1",
        "",
        "user:synthetic@example.com",
    ];
    let store = TransactionStore::from_records(
        hosts
            .iter()
            .enumerate()
            .map(|(index, host)| fixture_record(host, index as u64 + 1))
            .collect(),
    );
    let scopes = [
        (vec![], vec![]),
        (
            vec!["*.EXAMPLE.COM.:443".to_owned()],
            vec!["excluded.example.com".to_owned()],
        ),
        (vec![], vec!["*.example.com".to_owned()]),
        (
            vec!["*a*i*.example.com".to_owned()],
            vec!["nested.*".to_owned()],
        ),
        (vec!["[2001:0db8:0:0:0:0:0:1]:443".to_owned()], vec![]),
        (
            vec!["Ä.example.com".to_owned(), "api.example.com".to_owned()],
            vec!["ä.example.com".to_owned()],
        ),
    ];
    for catalog in [
        technology::CatalogVersion::V1,
        technology::CatalogVersion::V2,
    ] {
        let snapshot = store
            .service_map_snapshot_for_catalog(session, DEFAULT_MAX_RECORDS, catalog)
            .await;
        assert!(snapshot.omitted_rows > 0);
        for (include, exclude) in &scopes {
            for in_scope_only in [false, true] {
                for origin in [
                    None,
                    Some("https://api.example.com".into()),
                    Some("https://absent.example.com".into()),
                ] {
                    let legacy = build(
                        session,
                        snapshot.clone(),
                        DEFAULT_MAX_RECORDS,
                        in_scope_only,
                        origin.clone(),
                        include,
                        exclude,
                    );
                    let bounded = build_bounded(
                        session,
                        snapshot.clone(),
                        DEFAULT_MAX_RECORDS,
                        in_scope_only,
                        origin,
                        include,
                        exclude,
                    )
                    .unwrap();
                    same_output(legacy, bounded);
                }
            }
        }
    }
}

#[tokio::test]
async fn bounded_scope_distinct_host_exhaustion_returns_no_partial_map() {
    let session = Uuid::from_u128(93);
    let (include, exclude) = expensive_scope();
    let store = TransactionStore::from_records(
        (1..=40)
            .map(|i| fixture_record(&long_host(i), i as u64))
            .collect(),
    );
    let snapshot = store
        .service_map_snapshot(session, DEFAULT_MAX_RECORDS)
        .await;
    assert_eq!(snapshot.rows.len(), 40);
    assert!(matches!(
        build_bounded(
            session,
            snapshot,
            DEFAULT_MAX_RECORDS,
            false,
            None,
            &include,
            &exclude
        ),
        Err(scope::ScopeWorkLimit)
    ));
    let store = TransactionStore::from_records(
        (1..=1000)
            .map(|i| fixture_record(&long_host(1), i))
            .collect(),
    );
    let snapshot = store
        .service_map_snapshot(session, DEFAULT_MAX_RECORDS)
        .await;
    let expected = snapshot.rows.len();
    let bounded = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &include,
        &exclude,
    )
    .unwrap();
    assert_eq!(bounded.snapshot.included_rows, expected);
    assert_eq!(bounded.snapshot.excluded_rows, 0);
    assert_eq!(bounded.origins[0].request_count, expected);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "local synthetic bounded-path measurement; no timing thresholds"]
async fn bounded_scope_cost_measurement() {
    let session = Uuid::from_u128(94);
    for max_records in [DEFAULT_MAX_RECORDS, MAX_RECORDS] {
        for scenario in [
            "empty",
            "common",
            "dense_exact",
            "long_glob_repeated",
            "long_glob_unique",
        ] {
            let (include, exclude) = match scenario {
                "empty" => (vec![], vec![]),
                "common" => (
                    vec!["*.example.com".into()],
                    vec!["*.blocked.example.com".into()],
                ),
                "dense_exact" => {
                    let mut include: Vec<_> = (0..499)
                        .map(|i| format!("i-{i:03}.{}.example.com", "a".repeat(478)))
                        .collect();
                    include.push("api.example.com".into());
                    let exclude = (0..500)
                        .map(|i| format!("e-{i:03}.{}.example.com", "a".repeat(478)))
                        .collect();
                    (include, exclude)
                }
                _ => {
                    let mut include = (0..499)
                        .map(|i| format!("missing-{i:03}.example.com"))
                        .collect::<Vec<_>>();
                    include.push("*.example.com".into());
                    (include, expensive_scope().1)
                }
            };
            let baseline_long_host = format!(
                "{}.{}.{}.{}.example.com",
                "a".repeat(63),
                "a".repeat(63),
                "a".repeat(63),
                "a".repeat(47),
            );
            let store = std::sync::Arc::new(TransactionStore::from_records(
                (1..=MAX_RECORDS)
                    .rev()
                    .map(|i| {
                        let host = match scenario {
                            "long_glob_repeated" => baseline_long_host.clone(),
                            "long_glob_unique" => format!("{i:05}{}", &baseline_long_host[5..]),
                            _ => "api.example.com".into(),
                        };
                        let mut record = fixture_record(&host, i as u64);
                        // Match the earlier scope-cost audit's header-free rows.
                        record.response.as_mut().unwrap().headers.clear();
                        record
                    })
                    .collect(),
            ));
            for repetition in 1..=2 {
                let worker_store = std::sync::Arc::clone(&store);
                let include = include.clone();
                let exclude = exclude.clone();
                let (measurement, epoch) = tokio::task::spawn_blocking(move || {
                    // The uncontended wrapper timing includes read-lock acquisition,
                    // the coherent projection, and guard drop, but no queue delay.
                    let started = Instant::now();
                    let (snapshot, epoch) = worker_store.service_map_snapshot_blocking_for_catalog(session, max_records, technology::CatalogVersion::V1, &crate::store::ListFilters::default());
                    let projection = started.elapsed();
                    let projected_rows = snapshot.rows.len();
                    let legacy_snapshot = matches!(scenario, "empty" | "common").then(|| snapshot.clone());
                    let started = Instant::now();
                    let bounded = build_bounded(session, snapshot, max_records, false, None, &include, &exclude);
                    let bounded_elapsed = started.elapsed();
                    let result = if bounded.is_ok() { "ok" } else { "scope_work_limit" };
                    let serialization = bounded.as_ref().ok().map(|map| {
                        let started = Instant::now();
                        let bytes = serde_json::to_vec(map).unwrap().len();
                        (started.elapsed().as_secs_f64(), bytes)
                    });
                    let legacy_elapsed = legacy_snapshot.map(|snapshot| {
                        let started = Instant::now();
                        let legacy = build(session, snapshot, max_records, false, None, &include, &exclude);
                        let elapsed = started.elapsed();
                        same_output(legacy, bounded.unwrap());
                        elapsed.as_secs_f64()
                    });
                    assert_eq!(result, if scenario == "long_glob_unique" { "scope_work_limit" } else { "ok" });
                    (serde_json::json!({
                        "scenario": scenario, "repetition": repetition, "requested_rows": max_records,
                        "projected_rows": projected_rows, "projection_worker_seconds": projection.as_secs_f64(),
                        "bounded_build_seconds": bounded_elapsed.as_secs_f64(), "scope_preparation_included": true,
                        "legacy_build_seconds": legacy_elapsed, "serialization_seconds_and_bytes": serialization,
                        "result": result, "timing_claim": "local synthetic observation only; no native latency or deadline guarantee"
                    }), epoch)
                }).await.unwrap();
                assert!(store.service_map_metadata_epoch_matches(epoch).await);
                eprintln!("BOUNDED_SCOPE_MEASUREMENT {measurement}");
            }
        }
    }
}

#[tokio::test]
async fn bounded_scope_full_caches_never_omit_rows() {
    let session = Uuid::from_u128(95);
    for byte_limit in [false, true] {
        let base = if byte_limit {
            // Synthetic userinfo remains internal, while all rows have the same
            // canonical origin. This exercises 512-byte raw keys without the
            // map's unrelated origin or endpoint caps affecting the assertion.
            format!("{}@example.com", "a".repeat(500))
        } else {
            "api.example.com".into()
        };
        let count = if byte_limit { 300 } else { 600 };
        let records = (0..count)
            .map(|index| {
                let mut bit = 0;
                let host: String = base
                    .chars()
                    .map(|character| {
                        if character.is_ascii_alphabetic() && bit < 10 {
                            let upper = index & (1 << bit) != 0;
                            bit += 1;
                            if upper {
                                character.to_ascii_uppercase()
                            } else {
                                character
                            }
                        } else {
                            character
                        }
                    })
                    .collect();
                fixture_record(&host, index as u64 + 1)
            })
            .collect();
        let store = TransactionStore::from_records(records);
        let snapshot = store
            .service_map_snapshot(session, DEFAULT_MAX_RECORDS)
            .await;
        assert_eq!(snapshot.rows.len(), count);
        let include = vec!["*example.com".into()];
        let legacy = build(
            session,
            snapshot.clone(),
            DEFAULT_MAX_RECORDS,
            false,
            None,
            &include,
            &[],
        );
        let bounded = build_bounded(
            session,
            snapshot,
            DEFAULT_MAX_RECORDS,
            false,
            None,
            &include,
            &[],
        )
        .unwrap();
        assert_eq!(bounded.snapshot.included_rows, count);
        assert_eq!(bounded.snapshot.omitted_rows, 0);
        assert_eq!(bounded.snapshot.excluded_rows, 0);
        assert_eq!(bounded.origins.len(), 1);
        assert_eq!(bounded.origins[0].request_count, count);
        same_output(legacy, bounded);
    }
}

fn filter_fixture(
    host: &str,
    path: &str,
    status: u16,
    content_type: &str,
    referer: Option<&str>,
    sequence: u64,
) -> TransactionRecord {
    // MessageRecord derives its content type when built, so build it from headers.
    let mut response = http::HeaderMap::new();
    response.insert(http::header::CONTENT_TYPE, content_type.parse().unwrap());
    let mut request = http::HeaderMap::new();
    if let Some(value) = referer {
        request.insert(http::header::REFERER, value.parse().unwrap());
    }
    let mut record = fixture_record(host, sequence);
    record.path = path.into();
    record.status = Some(status);
    record.request = MessageRecord::from_headers_and_body(&request, b"", 0);
    record.response = Some(MessageRecord::from_headers_and_body(&response, b"", 0));
    record
}

// The Service Map display filter reuses HTTP history's row matcher, so a page's
// image CDN disappears with its rows and its Referer arrow, while the newest-row
// window that bounds the work stays the same size.
#[test]
fn display_filter_drops_rows_as_history_does_and_counts_them() {
    use crate::store::ListFilters;
    let session = Uuid::from_u128(97);
    let page = "https://www.example.com/";
    let store = TransactionStore::from_records(vec![
        filter_fixture("www.example.com", "/", 200, "text/html", None, 1),
        filter_fixture(
            "cdn.example.com",
            "/logo.png",
            200,
            "image/png",
            Some(page),
            2,
        ),
        filter_fixture(
            "api.example.com",
            "/v1/items",
            200,
            "application/json",
            Some(page),
            3,
        ),
        filter_fixture(
            "api.example.com",
            "/v1/missing",
            404,
            "application/json",
            Some(page),
            4,
        ),
    ]);
    let read = |filters: ListFilters| {
        let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
            session,
            DEFAULT_MAX_RECORDS,
            technology::CatalogVersion::V1,
            &filters,
        );
        build_bounded(
            session,
            snapshot,
            DEFAULT_MAX_RECORDS,
            false,
            None,
            &[],
            &[],
        )
        .unwrap()
    };
    let origins = |map: &ServiceMap| -> Vec<String> {
        let mut ids: Vec<String> = map.origins.iter().map(|o| o.origin.clone()).collect();
        ids.sort();
        ids
    };
    let cdn = "https://cdn.example.com".to_string();

    let all = read(ListFilters::default());
    assert!(origins(&all).contains(&cdn));
    assert_eq!(all.snapshot.filtered_rows, 0);

    let static_hidden = read(ListFilters {
        hidden_extensions: vec!["png".into()],
        ..ListFilters::default()
    });
    assert!(!origins(&static_hidden).contains(&cdn));
    assert_eq!(static_hidden.snapshot.filtered_rows, 1);
    assert_eq!(static_hidden.snapshot.considered_rows, 4);
    assert_eq!(static_hidden.snapshot.included_rows, 3);

    let images_off = read(ListFilters {
        mime_types: Some(vec!["html".into(), "json".into()]),
        ..ListFilters::default()
    });
    assert_eq!(origins(&images_off), origins(&static_hidden));

    let success_only = read(ListFilters {
        status_classes: Some(vec!["success".into()]),
        ..ListFilters::default()
    });
    let api = success_only
        .origins
        .iter()
        .find(|origin| origin.origin == "https://api.example.com")
        .unwrap();
    assert!(api
        .endpoints
        .iter()
        .all(|endpoint| endpoint.path != "/v1/missing"));
    assert_eq!(success_only.snapshot.filtered_rows, 1);
}

fn with_request_headers(
    mut record: TransactionRecord,
    headers: &[(&str, &str)],
) -> TransactionRecord {
    for (name, value) in headers {
        record.request.headers.push(HeaderRecord {
            name: (*name).into(),
            value: (*value).into(),
        });
    }
    record
}

#[test]
fn request_kind_reads_sec_fetch_dest_then_the_response_type() {
    let kind = |dest: Option<&str>, content_type: &str, method: &str, origin: bool| {
        let mut headers = vec![];
        if let Some(dest) = dest {
            headers.push(("Sec-Fetch-Dest", dest));
        }
        if origin {
            headers.push(("Origin", "https://b.example.com"));
        }
        let mut record = with_request_headers(
            filter_fixture("a.example.com", "/", 200, content_type, None, 1),
            &headers,
        );
        record.method = method.into();
        request_kind(&record)
    };
    assert_eq!(
        kind(Some("empty"), "text/html", "GET", false),
        "api",
        "the browser's own declaration wins"
    );
    assert_eq!(
        kind(Some("document"), "application/json", "GET", false),
        "navigation"
    );
    assert_eq!(kind(Some("iframe"), "text/html", "GET", false), "frame");
    assert_eq!(
        kind(Some("serviceworker"), "text/plain", "GET", false),
        "script"
    );
    assert_eq!(kind(Some("Font"), "text/plain", "GET", false), "static");
    assert_eq!(kind(Some("report"), "text/plain", "POST", false), "other");
    assert_eq!(kind(None, "application/problem+json", "GET", false), "api");
    assert_eq!(kind(None, "text/html", "GET", false), "navigation");
    assert_eq!(kind(None, "image/webp", "GET", false), "static");
    assert_eq!(
        kind(None, "image/svg+xml", "GET", false),
        "static",
        "an SVG image is not XML data"
    );
    assert_eq!(kind(None, "text/javascript", "GET", false), "script");
    assert_eq!(
        kind(None, "application/octet-stream", "GET", true),
        "api",
        "an Origin header marks a cross-origin call"
    );
    assert_eq!(kind(None, "application/octet-stream", "POST", false), "api");
    assert_eq!(
        kind(None, "application/octet-stream", "GET", false),
        "other"
    );
}

// A page loading an image and a page calling an API used to be the same arrow.
// Each link now counts its rows and what kind of request each one was.
#[test]
fn links_count_their_rows_by_request_kind() {
    let session = Uuid::from_u128(98);
    let page = "https://www.example.com/";
    let dest = |record, value| with_request_headers(record, &[("Sec-Fetch-Dest", value)]);
    let store = TransactionStore::from_records(vec![
        dest(
            filter_fixture(
                "cdn.example.com",
                "/logo.png",
                200,
                "image/png",
                Some(page),
                1,
            ),
            "image",
        ),
        dest(
            filter_fixture(
                "cdn.example.com",
                "/app.js",
                200,
                "application/javascript",
                Some(page),
                2,
            ),
            "script",
        ),
        dest(
            filter_fixture(
                "api.example.com",
                "/v1/items",
                200,
                "application/json",
                Some(page),
                3,
            ),
            "empty",
        ),
        dest(
            filter_fixture(
                "api.example.com",
                "/v1/items",
                200,
                "application/json",
                Some(page),
                4,
            ),
            "empty",
        ),
        // An app sends no Sec-Fetch-Dest: the JSON response marks its API call.
        filter_fixture(
            "api.example.com",
            "/v1/app",
            200,
            "application/json",
            Some(page),
            5,
        ),
        dest(
            filter_fixture("login.example.com", "/", 200, "text/html", Some(page), 6),
            "document",
        ),
    ]);
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V1,
        &crate::store::ListFilters::default(),
    );
    let map = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let link = |host: &str| {
        map.origins
            .iter()
            .find(|origin| origin.origin == format!("https://{host}"))
            .unwrap()
            .header_links[0]
            .clone()
    };
    let kinds = |pairs: &[(&str, usize)]| -> BTreeMap<String, usize> {
        pairs
            .iter()
            .map(|(kind, count)| ((*kind).into(), *count))
            .collect()
    };
    let cdn = link("cdn.example.com");
    assert_eq!(
        (cdn.request_count, cdn.request_kinds),
        (2, kinds(&[("script", 1), ("static", 1)]))
    );
    let api = link("api.example.com");
    assert_eq!(
        (api.request_count, api.request_kinds),
        (3, kinds(&[("api", 3)]))
    );
    assert_eq!(
        link("login.example.com").request_kinds,
        kinds(&[("navigation", 1)])
    );
}

// A link also counts the rows whose response let its source origin read them,
// judged as a browser judges: one Access-Control-Allow-Origin, compared as
// written, and credentials only with a named origin.
#[test]
fn links_count_rows_whose_response_allowed_the_source_origin() {
    let session = Uuid::from_u128(99);
    let page = "https://www.example.com";
    let call = |sequence, response_headers: &[(&str, &str)]| {
        let mut record = with_request_headers(
            filter_fixture(
                "api.example.com",
                "/v1/items",
                200,
                "application/json",
                Some("https://www.example.com/"),
                sequence,
            ),
            &[("Origin", page), ("Sec-Fetch-Dest", "empty")],
        );
        for (name, value) in response_headers {
            record
                .response
                .as_mut()
                .unwrap()
                .headers
                .push(HeaderRecord {
                    name: (*name).into(),
                    value: (*value).into(),
                });
        }
        record
    };
    let store = TransactionStore::from_records(vec![
        call(
            1,
            &[
                ("Access-Control-Allow-Origin", page),
                ("Access-Control-Allow-Credentials", "true"),
            ],
        ),
        call(
            2,
            &[
                ("Access-Control-Allow-Origin", "*"),
                ("Access-Control-Allow-Credentials", "true"),
            ],
        ),
        call(
            3,
            &[("Access-Control-Allow-Origin", "https://other.example.com")],
        ),
        call(
            4,
            &[
                ("Access-Control-Allow-Origin", page),
                ("Access-Control-Allow-Origin", "*"),
            ],
        ),
        call(
            5,
            &[("Access-Control-Allow-Origin", "https://WWW.example.com:443")],
        ),
        call(6, &[]),
    ]);
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V1,
        &crate::store::ListFilters::default(),
    );
    let map = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let api = map
        .origins
        .iter()
        .find(|origin| origin.origin == "https://api.example.com")
        .unwrap();
    assert_eq!(api.header_links.len(), 2, "Origin and Referer");
    for link in &api.header_links {
        assert_eq!(
            (
                link.request_count,
                link.cors_allowed_count,
                link.cors_credentials_count
            ),
            (6, 2, 1),
            "{}: the named origin and * allow, two values or another spelling do not",
            link.relationship
        );
    }
}

// Bundles, pages and asset URLs name front-end technologies; the map gives an
// asset's to the page that loaded it, to the asset's own origin when no page did,
// and keeps a page's with the page.
#[test]
fn front_technologies_go_to_the_page_that_loaded_the_asset() {
    let session = Uuid::from_u128(100);
    let asset =
        |host: &str, path: &str, kind: &str, referer: Option<&str>, body: &str, sequence| {
            let mut record = filter_fixture(host, path, 200, kind, referer, sequence);
            record.response.as_mut().unwrap().body_preview = body.into();
            record
        };
    let next = "/_next/static/chunks/main-app.js";
    let js = "application/javascript";
    let store = TransactionStore::from_records(vec![
        filter_fixture("www.example.com", "/", 200, "text/html", None, 1),
        asset(
            "cdn.example.com",
            next,
            js,
            Some("https://www.example.com/"),
            r#"n()}window.next={version:"14.2.3",appDir:!0}"#,
            2,
        ),
        asset(
            "cdn.example.com",
            next,
            js,
            Some("https://www.example.com/shop"),
            r#"window.next={version:"14.2.3",appDir:!0}"#,
            3,
        ),
        asset(
            "api.example.com",
            next,
            js,
            None,
            r#"let s="13.4.7";window.next={version:s,router:r}"#,
            4,
        ),
        // Its URL alone says Next.js built it; its body names no version.
        asset("img.example.com", next, js, None, "console.log(1)", 5),
        // A stylesheet is read by its URL only.
        asset(
            "cdn.example.com",
            "/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css",
            "text/css",
            Some("https://www.example.com/"),
            "",
            6,
        ),
        // A page's markup and generator stay with the page, whatever its Referer.
        asset(
            "blog.example.com",
            "/",
            "text/html",
            Some("https://www.example.com/"),
            r#"<meta name="generator" content="WordPress 6.5.2"><div data-v-1a2b3c4d>"#,
            7,
        ),
    ]);
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V1,
        &crate::store::ListFilters::default(),
    );
    let map = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let found = |host: &str| {
        map.origins
            .iter()
            .find(|origin| origin.origin == format!("https://{host}"))
            .unwrap()
            .front_technologies
            .iter()
            .map(|item| {
                (
                    item.product_id.clone(),
                    item.version.clone(),
                    item.source_origin
                        .trim_start_matches("https://")
                        .to_string(),
                    item.row_count,
                )
            })
            .collect::<Vec<_>>()
    };
    let one = |id: &str, version: Option<&str>, source: &str, rows| {
        (
            id.to_string(),
            version.map(str::to_string),
            source.to_string(),
            rows,
        )
    };
    assert_eq!(
        found("www.example.com"),
        [
            one("next_js", Some("14.2.3"), "cdn.example.com", 2),
            one("bootstrap", Some("5.3.2"), "cdn.example.com", 1),
        ],
        "each bundle counts once though its URL and body both name Next.js"
    );
    assert!(
        found("cdn.example.com").is_empty(),
        "the CDN only served them"
    );
    assert_eq!(
        found("api.example.com"),
        [one("next_js", Some("13.4.7"), "api.example.com", 1)]
    );
    assert_eq!(
        found("img.example.com"),
        [one("next_js", None, "img.example.com", 1)]
    );
    assert_eq!(
        found("blog.example.com"),
        [
            one("wordpress", Some("6.5.2"), "blog.example.com", 1),
            one("vue", None, "blog.example.com", 1),
        ],
        "a CMS leads a framework"
    );
}

// The UI's whole-history map folds every retained row, newest first, keeping
// none: where the windowed map holds every row it is the same map, and past
// the window's row and byte caps every row still counts. The window held only
// 6,202 of these rows before its 8 MiB projection cap stopped it.
#[test]
fn whole_history_map_matches_the_window_and_reads_past_it() {
    let session = Uuid::from_u128(101);
    let store = |rows: u64| {
        TransactionStore::from_records(
            (1..=rows)
                .rev()
                .map(|i| fixture_record(&format!("h{}.example.com", i % 7), i))
                .collect(),
        )
    };
    let filters = crate::store::ListFilters::default();
    let all_rows = |store: &TransactionStore| {
        store
            .service_map_all_rows_blocking(
                session,
                technology::CatalogVersion::V2,
                &filters,
                false,
                None,
                &[],
                &[],
            )
            .unwrap()
    };
    let small = store(3_000);
    let (snapshot, _) = small.service_map_snapshot_blocking_for_catalog(
        session,
        3_000,
        technology::CatalogVersion::V2,
        &filters,
    );
    assert!(snapshot.stopped_by.is_empty());
    let windowed = build_bounded(session, snapshot, 3_000, false, None, &[], &[]).unwrap();
    let (mut all, complete) = all_rows(&small);
    assert!(complete, "no script or page body was left to read");
    all.snapshot.captured_at = windowed.snapshot.captured_at;
    same_output(windowed, all);

    let large = store(12_000);
    let (snapshot, _) = large.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V2,
        &filters,
    );
    assert!(snapshot.stopped_by.contains("metadata_bytes"));
    let (all, _) = all_rows(&large);
    assert_eq!(
        (all.snapshot.considered_rows, all.snapshot.included_rows),
        (12_000, 12_000)
    );
    assert!(all.snapshot.complete, "{:?}", all.snapshot.stopped_by);
}

// The whole-history cache serves an unchanged history, or a recent map unless
// a fresh one is asked for, whatever changed since.
#[test]
fn whole_history_cache_serves_an_unchanged_or_recent_map() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let store = TransactionStore::from_records(vec![
        fixture_record("a.example.com", 2),
        fixture_record("b.example.com", 1),
    ]);
    let cache = |cost: u64, complete: bool| {
        store.store_service_map(
            "key".into(),
            store.service_map_history_token(),
            std::time::Duration::from_secs(cost),
            complete,
            bytes::Bytes::from_static(b"{}"),
        )
    };
    cache(0, true);
    let unchanged = store.service_map_history_token();
    assert!(store.cached_service_map("key", true).is_some(), "unchanged");
    assert!(store.cached_service_map("other", false).is_none());
    cache(0, false);
    assert!(
        store.cached_service_map("key", true).is_none(),
        "bodies left to read"
    );

    cache(60, true);
    runtime.block_on(store.insert(fixture_record("c.example.com", 3)));
    let appended = store.service_map_history_token();
    assert_ne!(appended, unchanged, "an append changes the history");
    runtime.block_on(store.update_record(Uuid::from_u128(2), |record| record.status = Some(204)));
    assert_ne!(
        store.service_map_history_token(),
        appended,
        "so does an update"
    );
    assert!(
        store.cached_service_map("key", false).is_some(),
        "a recent map serves a poll"
    );
    assert!(
        store.cached_service_map("key", true).is_none(),
        "a fresh read rebuilds"
    );
}

// Each row's User-Agent names its client; the map counts its rows per origin,
// and apart those it sent itself, with no Origin or Referer.
#[test]
fn clients_count_their_rows_and_their_own_visits() {
    let session = Uuid::from_u128(103);
    let chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
    let row =
        |host: &str, kind: &str, user_agent: Option<&str>, referer: Option<&str>, sequence| {
            let mut record = filter_fixture(host, "/", 200, kind, None, sequence);
            let mut request = http::HeaderMap::new();
            if let Some(value) = user_agent {
                request.insert(http::header::USER_AGENT, value.parse().unwrap());
            }
            if let Some(value) = referer {
                request.insert(http::header::REFERER, value.parse().unwrap());
            }
            record.request = MessageRecord::from_headers_and_body(&request, b"", 0);
            record
        };
    let store = TransactionStore::from_records(vec![
        row("www.example.com", "text/html", Some(chrome), None, 1),
        row(
            "api.example.com",
            "application/json",
            Some(chrome),
            Some("https://www.example.com/"),
            2,
        ),
        row(
            "api.example.com",
            "application/json",
            Some(chrome),
            Some("https://www.example.com/"),
            3,
        ),
        row(
            "api.example.com",
            "application/json",
            Some("okhttp/4.12.0"),
            None,
            4,
        ),
        row("api.example.com", "application/json", None, None, 5),
        // The same browser a version older is the same client.
        row(
            "www.example.com",
            "text/html",
            Some(&chrome.replace("Chrome/141", "Chrome/140")),
            None,
            6,
        ),
    ]);
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V2,
        &crate::store::ListFilters::default(),
    );
    let map = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let clients: Vec<_> = map
        .clients
        .iter()
        .map(|client| {
            (
                client.name.as_str(),
                client.versions.clone(),
                client.os.as_deref(),
                client.request_count,
                client
                    .origins
                    .iter()
                    .map(|origin| {
                        (
                            origin.origin.trim_start_matches("https://"),
                            origin.request_count,
                            origin.entry_kinds.clone().into_iter().collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let kinds = |kind: &str, rows| vec![(kind.to_string(), rows)];
    assert_eq!(
        clients,
        [
            (
                "Chrome",
                vec![141, 140],
                Some("macOS"),
                4,
                vec![
                    ("api.example.com", 2, vec![]),
                    ("www.example.com", 2, kinds("navigation", 2)),
                ]
            ),
            (
                "OkHttp",
                vec![4],
                None,
                1,
                vec![("api.example.com", 1, kinds("api", 1))]
            ),
            (
                "No User-Agent",
                vec![],
                None,
                1,
                vec![("api.example.com", 1, kinds("api", 1))]
            ),
        ],
        "a page's calls are not the browser's own visits; an app's are"
    );
    assert!(map.snapshot.complete);
}

// What a response's own headers, cookies and error body say about its servers
// stays with the origin that responded, even when a page's Referer loaded it.
#[test]
fn server_technologies_stay_with_the_origin_that_responded() {
    let session = Uuid::from_u128(104);
    let response =
        |host: &str, status, kind: &str, headers: &[(&str, &str)], body: &str, sequence| {
            let mut record = filter_fixture(
                host,
                "/v1/items",
                status,
                kind,
                Some("https://www.example.com/"),
                sequence,
            );
            let mut map = http::HeaderMap::new();
            map.insert(http::header::CONTENT_TYPE, kind.parse().unwrap());
            for (name, value) in headers {
                map.append(
                    http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    value.parse().unwrap(),
                );
            }
            record.response = Some(MessageRecord::from_headers_and_body(
                &map,
                body.as_bytes(),
                1024,
            ));
            record
        };
    let store = TransactionStore::from_records(vec![
        filter_fixture("www.example.com", "/", 200, "text/html", None, 1),
        response(
            "api.example.com",
            404,
            "application/json",
            &[("CF-RAY", "8d2c1e9a7b3f1a2b-ICN"), ("Set-Cookie", "JSESSIONID=abc; Path=/"), ("Set-Cookie", "BIGipServerpool=1677787402.20480.0000; path=/")],
            r#"{"timestamp":"2026-10-16T09:00:00.000+00:00","status":404,"error":"Not Found","path":"/v1/items"}"#,
            2,
        ),
        response(
            "api.example.com",
            500,
            "text/html",
            &[],
            "<pre>\tat org.springframework.web.servlet.DispatcherServlet.doDispatch(DispatcherServlet.java:1089)</pre>",
            3,
        ),
    ]);
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V2,
        &crate::store::ListFilters::default(),
    );
    let map = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let origin = |host: &str| {
        map.origins
            .iter()
            .find(|origin| origin.origin == format!("https://{host}"))
            .unwrap()
    };
    let servers: Vec<_> = origin("api.example.com")
        .server_technologies
        .iter()
        .map(|item| {
            (
                item.product_id.as_str(),
                item.category.as_str(),
                item.signals.join("+"),
                item.row_count,
            )
        })
        .collect();
    assert_eq!(
        servers,
        [
            ("backend_address", "exposure", "cookie".to_string(), 1),
            ("java_stack_trace", "exposure", "body".to_string(), 1),
            ("cloudflare", "edge", "header".to_string(), 1),
            ("f5_big_ip", "gateway", "cookie".to_string(), 1),
            ("java_servlet", "app_server", "cookie".to_string(), 1),
            ("spring_boot", "backend", "body".to_string(), 1),
            ("java", "language", "body".to_string(), 1),
        ],
        "exposures first, then outermost layer to innermost"
    );
    let www = origin("www.example.com");
    assert!(
        www.server_technologies.is_empty() && www.front_technologies.is_empty(),
        "the page only loaded them"
    );
}

// An origin keeps its newest icon even when the display filter hides `ico` and
// `png` rows, as it does by default. Only a GET answered 200 with a raster
// image at an icon path counts, and only bytes that are an image are served.
#[test]
fn origins_keep_their_newest_icon_whatever_the_display_filter_hides() {
    use crate::store::ListFilters;
    let session = Uuid::from_u128(105);
    let png: &[u8] = b"\x89PNG\r\n\x1a\nIHDR";
    let icon = |host: &str, path: &str, status, kind: &str, body: &[u8], sequence| {
        let mut record = filter_fixture(host, path, status, kind, None, sequence);
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::CONTENT_TYPE, kind.parse().unwrap());
        record.response = Some(MessageRecord::from_headers_and_body(&headers, body, 1024));
        record
    };
    // Newest first, as `from_records` takes them.
    let store = TransactionStore::from_records(vec![
        filter_fixture("www.example.com", "/", 200, "text/html", None, 1),
        icon(
            "www.example.com",
            "/static/Favicon-32x32.png",
            200,
            "image/png",
            png,
            2,
        ),
        icon(
            "www.example.com",
            "/favicon.ico",
            200,
            "image/x-icon",
            png,
            3,
        ),
        filter_fixture(
            "api.example.com",
            "/v1/items",
            200,
            "application/json",
            None,
            4,
        ),
        icon(
            "api.example.com",
            "/favicon.ico",
            404,
            "image/x-icon",
            png,
            5,
        ),
        icon(
            "api.example.com",
            "/favicon.svg",
            200,
            "image/svg+xml",
            b"<svg/>",
            6,
        ),
        icon("api.example.com", "/logo.png", 200, "image/png", png, 7),
        icon(
            "api.example.com",
            "/favicon.ico",
            200,
            "text/html",
            b"<html>",
            8,
        ),
        filter_fixture(
            "cdn.example.com",
            "/app.js",
            200,
            "text/javascript",
            None,
            9,
        ),
        icon(
            "cdn.example.com",
            "/apple-touch-icon.png",
            200,
            "image/png",
            b"<html>no</html>",
            10,
        ),
    ]);
    let hidden = ListFilters {
        hidden_extensions: vec!["ico".into(), "png".into()],
        ..ListFilters::default()
    };
    let (snapshot, _) = store.service_map_snapshot_blocking_for_catalog(
        session,
        DEFAULT_MAX_RECORDS,
        technology::CatalogVersion::V2,
        &hidden,
    );
    let windowed = build_bounded(
        session,
        snapshot,
        DEFAULT_MAX_RECORDS,
        false,
        None,
        &[],
        &[],
    )
    .unwrap();
    let (all, _) = store
        .service_map_all_rows_blocking(
            session,
            technology::CatalogVersion::V2,
            &hidden,
            false,
            None,
            &[],
            &[],
        )
        .unwrap();
    for map in [&windowed, &all] {
        let favicon = |host: &str| {
            map.origins
                .iter()
                .find(|origin| origin.origin == format!("https://{host}"))
                .unwrap()
                .favicon
                .clone()
        };
        // The evidence ends with the row's record ID: the fixture's sequence + 1.
        let row = |token: Option<String>| {
            token.map(|token| token.rsplit('.').next().unwrap().parse::<Uuid>().unwrap())
        };
        assert_eq!(
            row(favicon("www.example.com")),
            Some(Uuid::from_u128(3)),
            "the newest icon"
        );
        assert_eq!(
            favicon("api.example.com"),
            None,
            "a failure, an SVG, a logo and a page"
        );
        assert_eq!(row(favicon("cdn.example.com")), Some(Uuid::from_u128(11)));
        assert!(
            map.origins.iter().all(|origin| origin
                .endpoints
                .iter()
                .all(
                    |endpoint| !endpoint.path.ends_with(".ico") && !endpoint.path.ends_with(".png")
                )),
            "the icon rows themselves stay filtered out"
        );
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let origin = |host: &str| {
        all.origins
            .iter()
            .find(|origin| origin.origin == format!("https://{host}"))
            .unwrap()
    };
    let token = origin("www.example.com").favicon.clone().unwrap();
    assert_eq!(
        runtime.block_on(store.service_map_favicon(session, &token)),
        Some(("image/png", png.to_vec()))
    );
    assert_eq!(
        runtime.block_on(store.service_map_favicon(Uuid::from_u128(1), &token)),
        None,
        "another session"
    );
    let cdn = origin("cdn.example.com").favicon.clone().unwrap();
    assert_eq!(
        runtime.block_on(store.service_map_favicon(session, &cdn)),
        None,
        "bytes that are not an image are not served, whatever the header says"
    );
    let page = origin("www.example.com").endpoints[0].evidence[0].clone();
    assert_eq!(
        runtime.block_on(store.service_map_favicon(session, &page)),
        None,
        "a page row"
    );
    assert_eq!(
        favicon_type(b"\x00\x00\x01\x00\x01\x00"),
        Some("image/x-icon")
    );
    assert_eq!(
        favicon_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
        Some("image/webp")
    );
    assert_eq!(
        favicon_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        None
    );
    assert_eq!(favicon_type(&vec![0x89; MAX_FAVICON_BYTES + 1]), None);
}
