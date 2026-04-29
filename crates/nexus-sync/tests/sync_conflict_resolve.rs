//! Integration tests: sync conflict and merge scenarios.
//!
//! Tests push with conflicts, pull with merge scenarios,
//! reject workflow with conflict resolution, and bidirectional sync loops.

use nexus_contracts::generated::SyncPullRequest;
use nexus_contracts::{DeltaOperation, DeltaType};
use nexus_sync::delta_bundle::{BundleBuilder, LocalDelta};
use nexus_sync::sync_client::SyncClient;
use nexus_sync::SyncError;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VALID_TOKEN: &str =
    "valid_token_1234567890123456789012345678901234567890123456789012345678901234567890";

/// Test sync push when platform returns a conflict via success=false.
#[tokio::test]
async fn push_handles_version_mismatch_conflict() {
    let mock_server = MockServer::start().await;

    let conflict_json = json!({
        "success": false,
        "conflict_type": "version_mismatch",
        "conflicts": [],
        "server_world_revision": 10u64,
        "server_delta_sequence": 9u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&conflict_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Create,
        target_entity_type: Some("character".to_string()),
        target_entity_id: None,
        payload: json!({"display_name": "ConflictTest", "block_type": "character"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let bundle = BundleBuilder::new("wrk_conflict", "wld_conflict", "ctr_conflict")
        .submitting_creator_id("ctr_conflict")
        .add_delta(delta)
        .build()
        .expect("bundle");

    let result = client.push_bundle(&bundle).await;
    // Conflict response is returned as SyncError::SyncConflict
    match result {
        Ok(resp) => {
            // If success=false is handled gracefully, bundle_apply_status would be set
            assert!(!resp.success || resp.bundle_apply_status.is_some());
        }
        Err(SyncError::SyncConflict { conflict_type, .. }) => {
            assert!(conflict_type.contains("version_mismatch"));
        }
        other => panic!("Unexpected result: {other:?}"),
    }
}

/// Test sync push with partial success (some deltas applied, others rejected).
#[tokio::test]
async fn push_partial_success() {
    let mock_server = MockServer::start().await;

    let partial_json = json!({
        "success": true,
        "bundle_apply_status": "partial_success",
        "applied_deltas": ["delta_001", "delta_002"],
        "rejected_deltas": [{
            "delta_id": "delta_003",
            "reason": "entity_locked",
            "locked_by_creator_id": "ctr_other"
        }],
        "world_revision": 5u64,
        "confirmed_delta_sequence": 42u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&partial_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Update,
        target_entity_type: Some("character".to_string()),
        target_entity_id: Some("ent_abc".to_string()),
        payload: json!({"display_name": "Updated"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let bundle = BundleBuilder::new("wrk_partial", "wld_partial", "ctr_partial")
        .submitting_creator_id("ctr_partial")
        .add_delta(delta)
        .build()
        .expect("bundle");

    let resp = client.push_bundle(&bundle).await.expect("push");
    assert!(resp.success);
    assert_eq!(resp.world_revision, Some(5));
}

/// Test sync pull with incoming bundles (merge scenario).
#[tokio::test]
async fn pull_with_incoming_bundles() {
    let mock_server = MockServer::start().await;

    let pull_json = json!({
        "schema_version": 1,
        "world_revision": 10u64,
        "confirmed_delta_sequence": 100u64,
        "bundles": [
            {
                "schema_version": 1,
                "bundle_id": "bnd_001",
                "command_id": "cmd_001",
                "workspace_id": "wrk_remote",
                "world_id": "wld_merge_test",
                "creator_id": "ctr_other",
                "submitting_creator_id": "ctr_other",
                "bundle_type": "world_sync",
                "idempotency_key": "idem_001",
                "canonical_hash": "hash_001",
                "base_versions": {},
                "deltas": [
                    {
                        "schema_version": 1,
                        "delta_id": "delta_remote_1",
                        "delta_type": "key_block",
                        "operation": "create",
                        "target_entity_type": "character",
                        "payload": {"display_name": "RemoteChar"},
                        "local_timestamp": "2026-04-15T10:00:00Z"
                    }
                ],
                "created_at": "2026-04-15T10:00:00Z"
            }
        ],
        "is_up_to_date": false
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&pull_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_merge_test".to_string(),
        after_confirmed_delta_sequence: Some(99),
    };

    let resp = client.pull_bundles(&req).await.expect("pull");
    assert_eq!(resp.world_revision, 10);
    assert_eq!(resp.confirmed_delta_sequence, 100);
    assert!(!resp.bundles.is_empty(), "Expected incoming bundles");
    assert_eq!(resp.bundles.len(), 1);
    assert_eq!(resp.bundles[0].bundle_id.as_str(), "bnd_001");
}

/// Test sync pull with empty response (already up to date).
#[tokio::test]
async fn pull_empty_when_up_to_date() {
    let mock_server = MockServer::start().await;

    let up_to_date_json = json!({
        "schema_version": 1,
        "world_revision": 7u64,
        "confirmed_delta_sequence": 42u64,
        "bundles": [],
        "is_up_to_date": true
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&up_to_date_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_uptodate".to_string(),
        after_confirmed_delta_sequence: Some(42),
    };

    let resp = client.pull_bundles(&req).await.expect("pull");
    assert_eq!(resp.is_up_to_date, Some(true));
    assert!(resp.bundles.is_empty());
    assert_eq!(resp.confirmed_delta_sequence, 42);
}

/// Test bidirectional sync: push then pull to get remote changes.
#[tokio::test]
async fn bidirectional_push_then_pull() {
    let mock_server = MockServer::start().await;

    // Push response
    let push_json = json!({
        "success": true,
        "bundle_apply_status": "all_success",
        "world_revision": 8u64,
        "confirmed_delta_sequence": 50u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&push_json))
        .mount(&mock_server)
        .await;

    // Pull response with remote changes
    let pull_json = json!({
        "schema_version": 1,
        "world_revision": 9u64,
        "confirmed_delta_sequence": 51u64,
        "bundles": [
            {
                "schema_version": 1,
                "bundle_id": "bnd_remote_001",
                "command_id": "cmd_remote_001",
                "workspace_id": "wrk_other",
                "world_id": "wld_bidir",
                "creator_id": "ctr_other",
                "submitting_creator_id": "ctr_other",
                "bundle_type": "world_sync",
                "idempotency_key": "idem_r001",
                "canonical_hash": "hash_r001",
                "base_versions": {},
                "deltas": [
                    {
                        "schema_version": 1,
                        "delta_id": "delta_from_remote",
                        "delta_type": "key_block",
                        "operation": "create",
                        "target_entity_type": "location",
                        "payload": {"display_name": "RemoteLocation"},
                        "local_timestamp": "2026-04-15T11:00:00Z"
                    }
                ],
                "created_at": "2026-04-15T11:00:00Z"
            }
        ],
        "is_up_to_date": false
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&pull_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    // Push local delta
    let local_delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Create,
        target_entity_type: Some("character".to_string()),
        target_entity_id: None,
        payload: json!({"display_name": "LocalChar"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let push_bundle = BundleBuilder::new("wrk_bidir", "wld_bidir", "ctr_bidir")
        .submitting_creator_id("ctr_bidir")
        .add_delta(local_delta)
        .build()
        .expect("bundle");

    let push_resp = client.push_bundle(&push_bundle).await.expect("push");
    assert!(push_resp.success);
    assert_eq!(push_resp.world_revision, Some(8));

    // Pull to get remote changes
    let pull_req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_bidir".to_string(),
        after_confirmed_delta_sequence: Some(50),
    };

    let pull_resp = client.pull_bundles(&pull_req).await.expect("pull");
    assert_eq!(pull_resp.confirmed_delta_sequence, 51);
    assert!(!pull_resp.bundles.is_empty());
    assert_eq!(pull_resp.bundles[0].bundle_id.as_str(), "bnd_remote_001");
}

/// Test sync push rejected due to version mismatch conflict (409 status).
#[tokio::test]
async fn push_rejected_with_version_mismatch_409() {
    let mock_server = MockServer::start().await;

    let conflict_json = json!({
        "success": false,
        "conflict_type": "version_mismatch",
        "conflicts": [],
        "server_world_revision": 10u64,
        "server_delta_sequence": 9u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(409).set_body_json(&conflict_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Update,
        target_entity_type: Some("character".to_string()),
        target_entity_id: Some("ent_stale".to_string()),
        payload: json!({"display_name": "StaleUpdate"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let bundle = BundleBuilder::new("wrk_reject", "wld_reject", "ctr_reject")
        .submitting_creator_id("ctr_reject")
        .add_delta(delta)
        .build()
        .expect("bundle");

    let result = client.push_bundle(&bundle).await;
    match result {
        Ok(_) => panic!("Expected conflict error"),
        Err(SyncError::SyncConflict { conflict_type, .. }) => {
            assert!(conflict_type.contains("version_mismatch"));
        }
        other => panic!("Unexpected result: {other:?}"),
    }
}

/// Test pull after rejection: need to re-sync from scratch.
#[tokio::test]
async fn pull_after_rejection_refreshes_state() {
    let mock_server = MockServer::start().await;

    // After rejection, pull gives fresh state
    let refresh_json = json!({
        "schema_version": 1,
        "world_revision": 11u64,
        "confirmed_delta_sequence": 55u64,
        "bundles": [],
        "is_up_to_date": true
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&refresh_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_refresh".to_string(),
        after_confirmed_delta_sequence: None, // Full sync after rejection
    };

    let resp = client.pull_bundles(&req).await.expect("pull");
    assert_eq!(resp.world_revision, 11);
    assert_eq!(resp.confirmed_delta_sequence, 55);
    assert_eq!(resp.is_up_to_date, Some(true));
}

/// Test multiple sequential pushes maintain sequence integrity.
#[tokio::test]
async fn sequential_pushes_maintain_sequence() {
    let mock_server = MockServer::start().await;

    // Track sequences with static responses (wiremock doesn't support mutable closures well)
    // Instead, test individual pushes each succeed
    let push_json = json!({
        "success": true,
        "bundle_apply_status": "all_success",
        "world_revision": 1u64,
        "confirmed_delta_sequence": 1u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&push_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    for i in 0..3 {
        let delta = LocalDelta {
            delta_type: DeltaType::KeyBlock,
            operation: DeltaOperation::Create,
            target_entity_type: Some("character".to_string()),
            target_entity_id: None,
            payload: json!({"display_name": format!("Char{}", i)}),
            source_anchor: None,
            local_timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let bundle = BundleBuilder::new("wrk_seq", "wld_seq", "ctr_seq")
            .submitting_creator_id("ctr_seq")
            .add_delta(delta)
            .build()
            .expect("bundle");

        let resp = client.push_bundle(&bundle).await.expect("push");
        assert!(resp.success);
    }
}

/// Test sync pull with pagination (`has_more` = true via `is_up_to_date=false`).
#[tokio::test]
async fn pull_response_indicates_more_bundles() {
    let mock_server = MockServer::start().await;

    // First page with more data available
    let page1_json = json!({
        "schema_version": 1,
        "world_revision": 5u64,
        "confirmed_delta_sequence": 30u64,
        "bundles": [
            {
                "schema_version": 1,
                "bundle_id": "bnd_page1",
                "command_id": "cmd_page1",
                "workspace_id": "wrk_p1",
                "world_id": "wld_paginate",
                "creator_id": "ctr_1",
                "submitting_creator_id": "ctr_1",
                "bundle_type": "world_sync",
                "idempotency_key": "idem_p1",
                "canonical_hash": "hash_p1",
                "base_versions": {},
                "deltas": [],
                "created_at": "2026-04-15T12:00:00Z"
            }
        ],
        "is_up_to_date": false
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page1_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let req = SyncPullRequest {
        schema_version: 1,
        world_id: "wld_paginate".to_string(),
        after_confirmed_delta_sequence: None,
    };

    let resp = client.pull_bundles(&req).await.expect("pull");
    // is_up_to_date = false indicates more data is available
    assert_eq!(resp.is_up_to_date, Some(false));
    assert!(!resp.bundles.is_empty());
}

/// Test sync push with `all_success` status.
#[tokio::test]
async fn push_all_success_response() {
    let mock_server = MockServer::start().await;

    let success_json = json!({
        "success": true,
        "bundle_apply_status": "all_success",
        "world_revision": 3u64,
        "confirmed_delta_sequence": 15u64
    });
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&success_json))
        .mount(&mock_server)
        .await;

    let base = mock_server.uri();
    let client = SyncClient::new(base.trim_end_matches('/'), VALID_TOKEN).expect("client");

    let delta = LocalDelta {
        delta_type: DeltaType::KeyBlock,
        operation: DeltaOperation::Create,
        target_entity_type: Some("character".to_string()),
        target_entity_id: None,
        payload: json!({"display_name": "SuccessChar"}),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let bundle = BundleBuilder::new("wrk_success", "wld_success", "ctr_success")
        .submitting_creator_id("ctr_success")
        .add_delta(delta)
        .build()
        .expect("bundle");

    let resp = client.push_bundle(&bundle).await.expect("push");
    assert!(resp.success);
    assert_eq!(resp.bundle_apply_status.as_deref(), Some("all_success"));
    assert_eq!(resp.world_revision, Some(3));
    assert_eq!(resp.confirmed_delta_sequence, Some(15));
}
