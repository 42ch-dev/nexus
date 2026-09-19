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

use nexus_contracts::{
    CoreServiceStopRequest, KnowledgeViewItem, PatchWorkRequest, UpdateFindingRequest,
    UpdateKnowledgeEntryRequest, UpdateKnowledgeEntryRequestAudience,
};

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

/// The serialization half of the same contract: a required nullable property
/// must stay *present* on the wire while it is null. The generated field
/// carries no `skip_serializing_if`, so the null state emits the key rather
/// than dropping it — a `skip_serializing_if` regression here would silently
/// make the field disappear for consumers that treat absence as "not
/// supplied".
#[test]
fn required_nullable_field_serializes_its_null_state() {
    let request: CoreServiceStopRequest = CoreServiceStopRequest::builder()
        .expected_instance_id("inst-1")
        .expected_engine_epoch(None)
        .try_into()
        .expect("the builder accepts an absent engine epoch");
    assert_eq!(request.expected_engine_epoch, None);

    let wire = serde_json::to_string(&request).expect("serializes");
    assert!(
        wire.contains(r#""expected_engine_epoch":null"#),
        "a required nullable property must keep its key: {wire}"
    );
}

/// The v1.191 native holder-governance fields carry **two** states, not three.
///
/// The frozen `audience` member is absent (preserve the stored pair) or an
/// authored value; its only clear is the explicit `{"kind":"shared"}` arm, and
/// the schemas declare no `null` arm. The read-only `holder_entry_id` /
/// `disclosure` projection pair is service-resolved and likewise has no null
/// state — "shared" is the *absence* of both, never a value.
///
/// This pins the safety-relevant half: a `null` on either family must never
/// read as a third intent (a clear, or a resolved holder). Governance can only
/// be cleared by naming the `shared` arm.
#[test]
fn governance_fields_have_no_null_state() {
    const HEX32: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let view_item = |holder: &str| {
        format!(
            r#"{{"entry_id":"kb_1","owner":{{"kind":"world","id":"wld_{HEX32}"}},
               "holder_entry_id":{holder},"block_type":"info_point",
               "canonical_name":"note-alpha","status":"confirmed","revision":0,
               "created_at":"2026-09-05T00:00:00Z"}}"#
        )
    };
    let hld = format!("hld_{}", "0123456789abcdef".repeat(4));

    // Absent and `null` both leave `audience` unset — neither is a clear.
    let absent: UpdateKnowledgeEntryRequest =
        serde_json::from_str(r#"{"expected_revision":0}"#).expect("absent audience");
    assert!(absent.audience.is_none());
    let null: UpdateKnowledgeEntryRequest =
        serde_json::from_str(r#"{"expected_revision":0,"audience":null}"#).expect("null audience");
    assert!(null.audience.is_none());

    // The explicit shared arm is the authored clear, and it round-trips.
    let cleared: UpdateKnowledgeEntryRequest =
        serde_json::from_str(r#"{"expected_revision":0,"audience":{"kind":"shared"}}"#)
            .expect("explicit clear");
    assert!(matches!(
        cleared.audience,
        Some(UpdateKnowledgeEntryRequestAudience::Shared)
    ));
    let round_trip: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&cleared).expect("serializes"))
            .expect("round-trips");
    assert_eq!(
        round_trip,
        serde_json::json!({ "expected_revision": 0, "audience": { "kind": "shared" } })
    );

    // The projection pair is absence-bearing: a shared row emits neither key.
    let shared: KnowledgeViewItem = serde_json::from_str(
        r#"{"entry_id":"kb_1","owner":{"kind":"world","id":"wld_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "block_type":"info_point","canonical_name":"note-alpha","status":"confirmed",
            "revision":0,"created_at":"2026-09-05T00:00:00Z"}"#,
    )
    .expect("shared projection");
    assert!(shared.holder_entry_id.is_none());
    assert!(shared.disclosure.is_none());
    let wire = serde_json::to_string(&shared).expect("serializes");
    assert!(
        !wire.contains("holder_entry_id") && !wire.contains("disclosure"),
        "a shared projection must omit both governance members: {wire}"
    );

    // A null holder is not a resolved holder; it stays the shared absence.
    let nulled: KnowledgeViewItem = serde_json::from_str(&view_item("null")).expect("null holder");
    assert!(nulled.holder_entry_id.is_none());

    // A resolved pair round-trips as authored.
    let restricted: KnowledgeViewItem =
        serde_json::from_str(&view_item(&format!(r#""{hld}""#))).expect("resolved holder");
    let wire = serde_json::to_string(&restricted).expect("serializes");
    assert!(
        wire.contains(&format!(r#""holder_entry_id":"{hld}""#)),
        "{wire}"
    );
}
