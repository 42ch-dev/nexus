use nexus_contracts::{CoreTimelineEventsQuery, WorldSnapshotRequest};
use serde_json::json;

#[test]
fn accepts_generated_root_branch_id() {
    let branch_id = "fbk_root_0123abcd";
    let query: CoreTimelineEventsQuery = serde_json::from_value(json!({ "branch_id": branch_id }))
        .expect("a generated root branch is a valid timeline filter");
    assert_eq!(
        query.branch_id.as_deref().map(String::as_str),
        Some(branch_id)
    );
}

#[test]
fn snapshot_request_accepts_a_generated_root_branch_scope() {
    let root = "fbk_root_0123abcd";
    let query: WorldSnapshotRequest = serde_json::from_value(json!({
        "schema_version": 1,
        "world_id": "wld_worldA",
        "branch_id": root,
    }))
    .expect("a generated root branch is a valid snapshot scope");
    assert_eq!(query.branch_id.as_deref().map(String::as_str), Some(root));
}

#[test]
fn keeps_accepting_legacy_root_and_fork_ids() {
    for branch_id in ["fbk_root", "fbk_012345abcdef"] {
        assert!(serde_json::from_value::<CoreTimelineEventsQuery>(json!({
            "branch_id": branch_id,
        }))
        .is_ok());
    }
}

#[test]
fn rejects_malformed_branch_ids() {
    for branch_id in ["fbk_root_0123abc", "fbk_root_0123abcD", "fbk_fork_0123abcd"] {
        assert!(serde_json::from_value::<CoreTimelineEventsQuery>(json!({
            "branch_id": branch_id,
        }))
        .is_err());
    }
}
