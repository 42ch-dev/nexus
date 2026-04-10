//! Integration tests: platform Publish story/history clients (wiremock).

use nexus_contracts::{PublishHistoryRequest, PublishStoryRequest};
use nexus_sync::sync_client::SyncClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn publish_story_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "outcome": "published",
        "message": "ok",
        "published_artifact_id": "art_1"
    });

    Mock::given(method("POST"))
        .and(path("/v1/publish/story"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = PublishStoryRequest {
        schema_version: 1,
        world_id: "wld_x".into(),
        manuscript_id: serde_json::Value::String("mss_y".into()),
        story_manifest_id: Some(serde_json::Value::String("stm_z".into())),
    };

    let r = client.publish_story(&req).await.expect("publish_story");
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.outcome.as_str(), "published");
    assert_eq!(r.published_artifact_id.as_deref(), Some("art_1"));
}

#[tokio::test]
async fn publish_story_maps_422_to_platform_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/publish/story"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
            "error": "invalid_state",
            "detail": "manuscript not staged"
        })))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = PublishStoryRequest {
        schema_version: 1,
        world_id: "wld_x".into(),
        manuscript_id: serde_json::Value::String("mss_y".into()),
        story_manifest_id: None,
    };

    let err = client.publish_story(&req).await.expect_err("expect 422");
    match err {
        nexus_sync::SyncError::PlatformError { status, .. } => assert_eq!(status, 422),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn publish_history_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "occurred_at": "2026-04-10T12:00:00Z",
            "outcome": "rejected",
            "message": "gate failed"
        }],
        "has_more": false
    });

    Mock::given(method("POST"))
        .and(path("/v1/publish/history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = PublishHistoryRequest {
        schema_version: 1,
        world_id: "wld_x".into(),
        manuscript_id: serde_json::Value::String("mss_y".into()),
        cursor: None,
        limit: Some(10),
    };

    let r = client.publish_history(&req).await.expect("history");
    assert_eq!(r.entries.len(), 1);
    assert!(!r.has_more);
    let e0 = r.entries[0].as_object().expect("history entry object");
    assert_eq!(e0.get("outcome").and_then(|v| v.as_str()), Some("rejected"));
}
