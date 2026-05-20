//! Integration tests: platform world fork / snapshot clients (wiremock).

use nexus_cloud_sync::sync_client::SyncClient;
use nexus_contracts::{WorldForkRequest, WorldSnapshotRequest};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn fork_world_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "fork_branch": {
            "schema_version": 1,
            "fork_branch_id": "fbk_child01",
            "world_id": "wld_child",
            "parent_world_id": "wld_parent",
            "parent_branch_id": "fbk_root01",
            "forked_from_event_id": "evt_fork01",
            "status": "active",
            "verification_status": "unverified",
            "created_by_creator_id": "ctr_test",
            "created_at": "2026-04-10T12:00:00Z"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/worlds/fork"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = WorldForkRequest {
        schema_version: 1,
        parent_world_id: Some("wld_parent".into()),
        child_world_id: Some("wld_child".into()),
        forked_from_event_id: Some("evt_fork01".into()),
        created_by_creator_id: Some("ctr_test".into()),
        fork_title: None,
    };

    let r = client.fork_world(&req).await.expect("fork");
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.fork_branch.fork_branch_id, "fbk_child01");
    assert_eq!(r.fork_branch.world_id, "wld_child");
}

#[tokio::test]
async fn fork_world_maps_400_to_platform_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/worlds/fork"))
        .respond_with(ResponseTemplate::new(400).set_body_string("invalid fork"))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = WorldForkRequest {
        schema_version: 1,
        parent_world_id: Some("wld_parent".into()),
        child_world_id: Some("wld_child".into()),
        forked_from_event_id: Some("evt_fork01".into()),
        created_by_creator_id: Some("ctr_test".into()),
        fork_title: None,
    };

    let err = client.fork_world(&req).await.expect_err("expect 400");
    match err {
        nexus_cloud_sync::SyncError::PlatformError { status, .. } => assert_eq!(status, 400),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn snapshot_world_parses_success() {
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "schema_version": 1,
        "world_id": "wld_test",
        "world_revision": 42,
        "at_event_id": "evt_head",
        "captured_at": "2026-04-10T12:00:00Z"
    });

    Mock::given(method("POST"))
        .and(path("/v1/worlds/snapshot"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = WorldSnapshotRequest {
        schema_version: 1,
        world_id: "wld_test".into(),
        at_event_id: None,
        branch_id: None,
        key_block_limit: None,
        timeline_event_limit: None,
    };

    let r = client.snapshot_world(&req).await.expect("snapshot");
    assert_eq!(r.world_revision, 42);
    assert_eq!(r.at_event_id.as_deref(), Some("evt_head"));
}
