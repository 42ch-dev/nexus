//! Integration tests: platform Explore browse/search clients (wiremock).

use nexus_cloud_sync::sync_client::SyncClient;
use nexus_contracts::{ExploreBrowseRequest, ExploreSearchRequest};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn explore_browse_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "hit_type": "world",
            "entity_id": "wld_x",
            "title": "Test world"
        }],
        "next_cursor": "cur_next",
        "has_more": true
    });

    Mock::given(method("POST"))
        .and(path("/v1/explore/browse"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = ExploreBrowseRequest {
        schema_version: 1,
        cursor: None,
        limit: Some(10),
        scope: Some("worlds".into()),
    };

    let r = client.explore_browse(&req).await.expect("browse");
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.entries.len(), 1);
    assert!(r.has_more);
    assert_eq!(r.next_cursor.as_deref(), Some("cur_next"));
}

#[tokio::test]
async fn explore_search_maps_401_to_platform_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/explore/search"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = ExploreSearchRequest {
        schema_version: 1,
        query: "hello".into(),
        cursor: None,
        limit: None,
    };

    let err = client.explore_search(&req).await.expect_err("expect 401");
    match err {
        nexus_cloud_sync::SyncError::PlatformError { status, .. } => assert_eq!(status, 401),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn explore_search_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "entries": [],
        "has_more": false
    });

    Mock::given(method("POST"))
        .and(path("/v1/explore/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = ExploreSearchRequest {
        schema_version: 1,
        query: "q".into(),
        cursor: None,
        limit: None,
    };

    let r = client.explore_search(&req).await.expect("search");
    assert_eq!(r.entries.len(), 0);
    assert!(!r.has_more);
}
