//! Real generated-DTO tri-state regression (R-V1190-FINDINGS-TRISTATE-DUP,
//! P1 nullable-binding residual): the `x-nexus-tri-state` fields on
//! `UpdateFindingRequest.rule_suggestion` and
//! `PatchWorkRequest.{world_id,story_ref}` must deserialize with presence
//! preserved — absent (keep) / `null` (clear) / value (set) — and serialize
//! back without losing the clear state.
//!
//! The same file pins the *other* half of the omission contract: a property
//! that the schema declares both `required` and nullable keeps its required
//! status (omission is rejected, `null` is a value), which the typify 0.8
//! cutover tightened rather than relaxed.

use nexus_contracts::{CoreServiceStopRequest, PatchWorkRequest, UpdateFindingRequest};

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
        serde_json::from_str(r#"{"rule_suggestion":"prefer scene breaks"}"#).expect("set parses");
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
    assert_eq!(
        set.world_id,
        Some(serde_json::Value::String("wld_owned".into()))
    );
    assert_eq!(
        set.story_ref,
        Some(serde_json::Value::String("ref-1".into()))
    );
}

/// A schema-`required` property that is also nullable stays required: the
/// generated carrier is `Option<T>` carrying typify 0.8's
/// `#[serde(deserialize_with = "Option::deserialize")]`, so an omitted key is
/// a deserialization error while an explicit `null` decodes to `None`.
/// `core-service-stop-request.schema.json` requires both `expected_instance_id`
/// and the nullable `expected_engine_epoch`.
#[test]
fn required_nullable_field_rejects_omission_and_accepts_null() {
    let omitted =
        serde_json::from_str::<CoreServiceStopRequest>(r#"{"expected_instance_id":"inst-1"}"#)
            .expect_err("omitting a required nullable property must not decode");
    assert!(
        omitted.to_string().contains("expected_engine_epoch"),
        "the omission error must name the field: {omitted}"
    );

    let null: CoreServiceStopRequest =
        serde_json::from_str(r#"{"expected_instance_id":"inst-1","expected_engine_epoch":null}"#)
            .expect("explicit null is the required value's null state");
    assert_eq!(null.expected_engine_epoch, None);

    let epoch: CoreServiceStopRequest =
        serde_json::from_str(r#"{"expected_instance_id":"inst-1","expected_engine_epoch":7}"#)
            .expect("a present integer is a valid required value");
    assert_eq!(epoch.expected_engine_epoch, Some(7));
}
