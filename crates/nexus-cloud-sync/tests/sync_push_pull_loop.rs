//! Golden-style integration: mock platform accepts push then returns an empty pull window.

use nexus_cloud_sync::delta_bundle::{BundleBuilder, LocalDelta};
use nexus_cloud_sync::sync_client::SyncClient;
use nexus_contracts::generated::SyncPullRequest;
use nexus_contracts::{DeltaOperation, DeltaType};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

#[tokio::test]
async fn mock_platform_push_then_pull_empty_loop() {
    let mock_server = MockServer::start().await;

    let push_json = json!({
        "success": true,
        "bundle_apply_status": "all_success",
        "world_revision": 7u64,
        "confirmed_delta_sequence": 42u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&push_json))
        .mount(&mock_server)
        .await;

    let pull_json = json!({
        "schema_version": 1,
        "world_revision": 7u64,
        "confirmed_delta_sequence": 42u64,
        "bundles": [],
        "is_up_to_date": true
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&pull_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Create,
        target_entity_type: Some("character".to_string()),
        target_entity_id: None,
        payload: json!({"display_name": "Loop", "block_type": "character"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let bundle = BundleBuilder::new("wrk_loop", "wld_loop", "ctr_loop")
        .submitting_creator_id("ctr_loop")
        .add_delta(delta)
        .build()
        .expect("bundle");

    let push_resp = client.push_bundle(&bundle).await.expect("push");
    assert!(push_resp.success);

    let pull_req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_loop".to_string(),
        after_confirmed_delta_sequence: None,
    };
    let pull_resp = client.pull_bundles(&pull_req).await.expect("pull");
    assert_eq!(pull_resp.confirmed_delta_sequence, 42);
    assert!(pull_resp.bundles.is_empty());
}
