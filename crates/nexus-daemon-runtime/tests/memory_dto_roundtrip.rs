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
        has_more: Some(true),
        processed: Some(10),
    };
    let _: nexus_contracts::MemoryFragmentInfo = handler::MemoryFragmentInfo {
        fragment_id: "frag_1".into(),
        summary: "s".into(),
        keywords: Some(vec!["theme".into()]),
        created_at: Some("2026-07-01T00:00:00Z".into()),
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
/// promoted/fragmented/dropped counters). V1.80 REL-01 adds optional
/// `has_more` + `processed`; the round-trip covers both the populated additive
/// shape and the pre-V1.80 minimal JSON (no optional fields) still
/// deserializing.
#[test]
fn review_response_counts_are_integers() {
    let resp = handler::ReviewResponse {
        promoted: 3,
        fragmented: 5,
        dropped: 2,
        has_more: Some(true),
        processed: Some(10),
    };
    let _: ReviewResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["promoted"], json!(3));
    assert_eq!(v["fragmented"], json!(5));
    assert_eq!(v["dropped"], json!(2));
    // V1.80 additive fields serialize when populated.
    assert_eq!(v["has_more"], json!(true));
    assert_eq!(v["processed"], json!(10));
    let rt: ReviewResponse = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(rt, resp);

    // Absent optional fields are omitted from the wire (skip_serializing_if).
    let minimal = handler::ReviewResponse {
        promoted: 0,
        fragmented: 0,
        dropped: 0,
        has_more: None,
        processed: None,
    };
    let mv = serde_json::to_value(&minimal).unwrap();
    assert!(
        !mv.as_object().unwrap().contains_key("has_more"),
        "optional has_more must be omitted when None, got: {mv}"
    );
    assert!(
        !mv.as_object().unwrap().contains_key("processed"),
        "optional processed must be omitted when None, got: {mv}"
    );

    // Backward compatibility: pre-V1.80 minimal JSON (no optional fields)
    // still deserializes — older daemons/clients must not break.
    let old_json = json!({ "promoted": 1, "fragmented": 0, "dropped": 0 });
    let parsed: ReviewResponse = serde_json::from_value(old_json).unwrap();
    assert_eq!(parsed.promoted, 1);
    assert_eq!(parsed.has_more, None);
    assert_eq!(parsed.processed, None);
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

/// `ListMemoryFragmentsResponse` exposes `fragment_id` + `summary` plus the V1.79
/// additive `keywords` + `created_at` read-only fields, while internal fields
/// (`ttl`, `session_id`, `creator_id`) stay off this wire shape. The two additive
/// fields round-trip through serde; an explicit `None` is omitted on the wire.
#[test]
fn fragments_response_round_trips_keywords_and_created_at() {
    let resp = handler::ListMemoryFragmentsResponse {
        fragments: vec![handler::MemoryFragmentInfo {
            fragment_id: "frag_1".into(),
            summary: "a keyword fragment".into(),
            keywords: Some(vec!["historical fiction".into(), "moral ambiguity".into()]),
            created_at: Some("2026-07-01T00:00:00Z".into()),
        }],
    };
    let _: ListMemoryFragmentsResponse = resp;
    let v = serde_json::to_value(&resp).unwrap();
    let frag = &v["fragments"][0];
    assert_eq!(frag["fragment_id"], json!("frag_1"));
    assert_eq!(frag["summary"], json!("a keyword fragment"));
    // V1.79 additive fields serialize and carry the keyword array + timestamp.
    assert_eq!(
        frag["keywords"],
        json!(["historical fiction", "moral ambiguity"])
    );
    assert_eq!(frag["created_at"], json!("2026-07-01T00:00:00Z"));
    // Internal fragment fields are intentionally not on this wire shape.
    assert!(frag.get("ttl").is_none());
    assert!(frag.get("session_id").is_none());
    assert!(frag.get("creator_id").is_none());

    // Round-trip: serialize → deserialize → re-serialize is stable.
    let rt: ListMemoryFragmentsResponse = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&rt).unwrap(), v);

    // A fragment with no keyword labels still serializes keywords as an empty
    // array (the handler always emits Some when the DB column is populated;
    // malformed JSON degrades to an empty Vec, not an absent field).
    let empty_kw = handler::MemoryFragmentInfo {
        fragment_id: "frag_2".into(),
        summary: "no keywords".into(),
        keywords: Some(Vec::new()),
        created_at: Some("2026-07-01T00:00:00Z".into()),
    };
    let v_empty = serde_json::to_value(&empty_kw).unwrap();
    assert_eq!(v_empty["keywords"], json!([]));
    assert!(v_empty.get("ttl").is_none());
    assert!(v_empty.get("session_id").is_none());
}
