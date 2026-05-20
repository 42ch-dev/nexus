//! Integration tests: HTTP body size limits (chunked read path).

use nexus_cloud_sync::errors::SyncError;
use nexus_cloud_sync::sync_client::SyncClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn pull_sync_state_rejects_body_over_limit() {
    let mock_server = MockServer::start().await;
    let oversized = "x".repeat(500);

    Mock::given(method("GET"))
        .and(path("/v1/sync/state/wld_test"))
        .respond_with(ResponseTemplate::new(200).set_body_string(oversized))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::builder()
        .body_max_size(100)
        .build(base.trim_end_matches('/'), VALID_TOKEN)
        .expect("client");

    let err = client
        .pull_sync_state("wld_test")
        .await
        .expect_err("oversized body");
    assert!(matches!(err, SyncError::HttpBodySizeExceeded { .. }));
    if let SyncError::HttpBodySizeExceeded { actual, limit } = err {
        assert!(actual > limit);
        assert_eq!(limit, 100);
    }
}
