//! Real generated-DTO tri-state regression (R-V1190-FINDINGS-TRISTATE-DUP,
//! P1 nullable-binding residual): the `x-nexus-tri-state` fields on
//! `UpdateFindingRequest.rule_suggestion` and
//! `PatchWorkRequest.{world_id,story_ref}` must deserialize with presence
//! preserved — absent (keep) / `null` (clear) / value (set) — and serialize
//! back without losing the clear state.

use nexus_contracts::{PatchWorkRequest, UpdateFindingRequest};

#[test]
fn update_finding_rule_suggestion_keeps_all_three_states() {
    // Absent → None (do not touch the column).
    let keep: UpdateFindingRequest =
        serde_json::from_str(r#"{"status":"triaged"}"#).expect("keep parses");
    assert_eq!(keep.rule_suggestion, None);
    assert_eq!(keep.status.as_deref(), Some("triaged"));

    // Explicit null → Some(Value::Null) (clear to SQL NULL); this is the
    // state a plain Option<T> carrier erases.
    let clear: UpdateFindingRequest =
        serde_json::from_str(r#"{"rule_suggestion":null}"#).expect("clear parses");
    assert_eq!(clear.rule_suggestion, Some(serde_json::Value::Null));

    // Value → Some(Value::String) (set).
    let set: UpdateFindingRequest =
        serde_json::from_str(r#"{"rule_suggestion":"prefer scene breaks"}"#)
            .expect("set parses");
    assert_eq!(
        set.rule_suggestion,
        Some(serde_json::Value::String("prefer scene breaks".into()))
    );
}

#[test]
fn update_finding_round_trips_the_clear_state() {
    let clear: UpdateFindingRequest =
        serde_json::from_str(r#"{"rule_suggestion":null}"#).expect("parses");
    let wire = serde_json::to_string(&clear).expect("serializes");
    assert_eq!(wire, r#"{"rule_suggestion":null}"#);

    let keep: UpdateFindingRequest = serde_json::from_str("{}").expect("parses");
    assert_eq!(serde_json::to_string(&keep).expect("serializes"), "{}");
}

#[test]
fn update_finding_rejects_unknown_fields() {
    let error = serde_json::from_str::<UpdateFindingRequest>(r#"{"nope":1}"#)
        .expect_err("unknown field must be rejected");
    assert!(error.to_string().contains("unknown field"), "{error}");
}

#[test]
fn patch_work_binding_fields_keep_all_three_states() {
    let keep: PatchWorkRequest = serde_json::from_str(r#"{"title":"New"}"#).expect("parses");
    assert_eq!(keep.world_id, None);
    assert_eq!(keep.story_ref, None);
    assert_eq!(keep.title.as_deref(), Some("New"));

    let clear: PatchWorkRequest =
        serde_json::from_str(r#"{"world_id":null,"story_ref":null}"#).expect("parses");
    assert_eq!(clear.world_id, Some(serde_json::Value::Null));
    assert_eq!(clear.story_ref, Some(serde_json::Value::Null));

    let set: PatchWorkRequest =
        serde_json::from_str(r#"{"world_id":"wld_owned","story_ref":"ref-1"}"#).expect("parses");
    assert_eq!(set.world_id, Some(serde_json::Value::String("wld_owned".into())));
    assert_eq!(set.story_ref, Some(serde_json::Value::String("ref-1".into())));
}
