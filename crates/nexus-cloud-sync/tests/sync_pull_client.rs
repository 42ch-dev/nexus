//! Integration tests: platform pull client (wiremock).

use nexus_contracts::generated::SyncPullRequest;
use nexus_cloud_sync::sync_client::SyncClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn pull_bundles_parses_empty_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "world_revision": 2,
        "confirmed_delta_sequence": 5,
        "bundles": [],
        "is_up_to_date": true
    });

    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_test".to_string(),
        after_confirmed_delta_sequence: None,
    };

    let r = client.pull_bundles(&req).await.expect("pull");
    assert_eq!(r.world_revision, 2);
    assert_eq!(r.confirmed_delta_sequence, 5);
    assert!(r.bundles.is_empty());
}

#[tokio::test]
async fn pull_bundles_maps_404_to_platform_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_missing".to_string(),
        after_confirmed_delta_sequence: None,
    };

    let err = client.pull_bundles(&req).await.expect_err("expect 404");
    match err {
        nexus_cloud_sync::SyncError::PlatformError { status, .. } => assert_eq!(status, 404),
        other => panic!("unexpected error: {other:?}"),
    }
}
