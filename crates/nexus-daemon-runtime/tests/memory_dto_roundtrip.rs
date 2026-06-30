//! V1.78 P0 (Batch 1) — memory DTO round-trip regression.
//!
//! Guards the A2 normalization: the `handlers::memory` request/response/query/
//! item types MUST be the generated `nexus_contracts` contract types (no
//! hand-written DTO drift). The tests prove (a) the handler module serves the
//! exact contract types, and (b) a value built the way the handler builds it
//! serializes to the documented wire shape and round-trips through serde.
//!
//! If someone re-introduces a hand-written struct with a divergent shape, or
//! the codegen drifts from the served type, these tests fail to compile or
//! assert.

#![allow(clippy::unwrap_used)]

use nexus_contracts::{
    CountPendingReviewsResponse, CreatePendingReviewResponse, DeletePendingReviewResponse,
    ListMemoryFragmentsResponse, ListPendingReviewsResponse, PaginationInfo, PendingReviewInfo,
    ReviewResponse,
};
use nexus_daemon_runtime::api::handlers::memory as handler;
use serde_json::{json, Value};

/// Build a `PendingReviewInfo` exactly as the handler's SQL projection would,
/// then vary the nullable `world_id`.
fn sample_pending(world_id: Option<&str>) -> handler::PendingReviewInfo {
    handler::PendingReviewInfo {
        pending_id: "pending_1".into(),
        session_id: "sess_1".into(),
        creator_id: "ctr_author".into(),
        world_id: world_id.map(str::to_string),
        task_kind: "brainstorm".into(),
        raw_digest: "Discussed narrative structure and character arcs.".into(),
        created_at: "2026-07-01T00:00:00Z".into(),
    }
}

/// Compile-time proof that the handler module serves the exact contract types:
/// each assignment type-checks only if `handler::T` and `nexus_contracts::T`
/// are the SAME type (the `pub use` re-export). A re-introduced hand-written
/// DTO would break this.
#[test]
fn handler_serves_exact_contract_types() {
    let _: nexus_contracts::PendingReviewInfo = sample_pending(Some("w1"));
    let _: nexus_contracts::CreatePendingReviewResponse = handler::CreatePendingReviewResponse {
        success: true,
        pending_id: "p1".into(),
    };
    let _: nexus_contracts::DeletePendingReviewResponse = handler::DeletePendingReviewResponse {
        success: true,
        pending_id: "p1".into(),
    };
    let _: nexus_contracts::CountPendingReviewsResponse =
        handler::CountPendingReviewsResponse { count: 7 };
    let _: nexus_contracts::ReviewResponse = handler::ReviewResponse {
        promoted: 3,
        fragmented: 5,
        dropped: 2,
    };
    let _: nexus_contracts::MemoryFragmentInfo = handler::MemoryFragmentInfo {
        fragment_id: "frag_1".into(),
        summary: "s".into(),
    };
}

/// `PendingReviewInfo` round-trips and the nullable `world_id` follows the
/// optional contract (present → string; absent → omitted by `skip_serializing_if`).
#[test]
fn pending_review_info_round_trips_and_omits_null_world_id() {
    let with_world = sample_pending(Some("world_1"));
    let without_world = sample_pending(None);

    // Present world_id serializes as a string; all required fields present.
    let v = serde_json::to_value(&with_world).unwrap();
    assert_eq!(v["pending_id"], json!("pending_1"));
    assert_eq!(v["session_id"], json!("sess_1"));
    assert_eq!(v["creator_id"], json!("ctr_author"));
    assert_eq!(v["world_id"], json!("world_1"));
    assert_eq!(v["task_kind"], json!("brainstorm"));
    assert_eq!(
        v["raw_digest"],
        json!("Discussed narrative structure and character arcs.")
    );
    assert_eq!(v["created_at"], json!("2026-07-01T00:00:00Z"));

    // Absent world_id is omitted from the wire (optional contract field).
    let v_none = serde_json::to_value(&without_world).unwrap();
    assert!(
        !v_none.as_object().unwrap().contains_key("world_id"),
        "optional world_id must be omitted when None, got: {v_none}"
    );

    // Round-trip: serialize → deserialize → re-serialize is stable.
    let rt: PendingReviewInfo = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&rt).unwrap(), v);
    // Null world_id also deserializes to None (open item #6: absent/null accepted).
    let with_null_world: Value = serde_json::to_value(&with_world).unwrap();
    let mut with_null = with_null_world.clone();
    with_null["world_id"] = Value::Null;
    let parsed: PendingReviewInfo = serde_json::from_value(with_null).unwrap();
    assert!(parsed.world_id.is_none());
}

/// `ListPendingReviewsResponse` carries `items` + the shared `PaginationInfo`
/// envelope, mirroring the findings list response convention.
#[test]
fn list_pending_reviews_response_shape() {
    let resp = handler::ListPendingReviewsResponse {
        items: vec![sample_pending(None), sample_pending(Some("w1"))],
        pagination: PaginationInfo {
            limit: 50,
            next_cursor: Some("pending_1".into()),
            has_more: true,
        },
    };
    let _: ListPendingReviewsResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["pagination"]["limit"], json!(50));
    assert_eq!(v["pagination"]["next_cursor"], json!("pending_1"));
    assert_eq!(v["pagination"]["has_more"], json!(true));

    // Round-trip stability.
    let rt: ListPendingReviewsResponse = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&rt).unwrap(), v);
}

/// `ReviewResponse` counts serialize as integers (the handler reports
/// promoted/fragmented/dropped counters).
#[test]
fn review_response_counts_are_integers() {
    let resp = handler::ReviewResponse {
        promoted: 3,
        fragmented: 5,
        dropped: 2,
    };
    let _: ReviewResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["promoted"], json!(3));
    assert_eq!(v["fragmented"], json!(5));
    assert_eq!(v["dropped"], json!(2));
    let rt: ReviewResponse = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(rt, resp);
}

/// `CountPendingReviewsResponse.count` is a JSON integer.
#[test]
fn count_response_is_integer() {
    let resp = handler::CountPendingReviewsResponse { count: 42 };
    let _: CountPendingReviewsResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["count"], json!(42));
    assert!(v["count"].is_i64());
}

/// Create/delete success responses echo `pending_id`.
#[test]
fn create_and_delete_responses_echo_pending_id() {
    let create = handler::CreatePendingReviewResponse {
        success: true,
        pending_id: "p1".into(),
    };
    let delete = handler::DeletePendingReviewResponse {
        success: true,
        pending_id: "p1".into(),
    };
    let _: CreatePendingReviewResponse = create;
    let _: DeletePendingReviewResponse = delete;
    let vc = serde_json::to_value(&create).unwrap();
    let vd = serde_json::to_value(&delete).unwrap();
    assert_eq!(vc, json!({ "success": true, "pending_id": "p1" }));
    assert_eq!(vd, json!({ "success": true, "pending_id": "p1" }));
}

/// `ListMemoryFragmentsResponse` exposes only `fragment_id` + `summary`.
#[test]
fn fragments_response_exposes_only_id_and_summary() {
    let resp = handler::ListMemoryFragmentsResponse {
        fragments: vec![handler::MemoryFragmentInfo {
            fragment_id: "frag_1".into(),
            summary: "a keyword fragment".into(),
        }],
    };
    let _: ListMemoryFragmentsResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    let frag = &v["fragments"][0];
    assert_eq!(frag["fragment_id"], json!("frag_1"));
    assert_eq!(frag["summary"], json!("a keyword fragment"));
    // Internal fragment fields are intentionally not on this wire shape.
    assert!(frag.get("keywords").is_none());
    assert!(frag.get("ttl").is_none());
    assert!(frag.get("session_id").is_none());
}
