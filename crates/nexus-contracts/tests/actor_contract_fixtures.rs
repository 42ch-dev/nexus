//! Closed Actor/Character/ActorWorldBinding wire fixtures (v1.184 P0 Task 1).

use nexus_contracts::{
    ActorRef, ActorWorldBinding, ActorWorldBindingStatus, AddKnowledgeEntryRequest,
    Character, CharacterBindingDetail, CharacterDetail, CharacterLifecycleRequest,
    CharacterOperationResult, CharacterPendingReviewInfo, CharacterRunCaptureOutcome,
    CharacterStatus, CreateCharacterRequest, CreateCharacterResponse, DeleteKnowledgeEntryQuery,
    KnowledgeEntryDetail, KnowledgeViewItem, ListCharactersResponse,
    UpdateCharacterBindingRequest, UpdateCharacterRequest, UpdateKnowledgeEntryRequest,
};
use std::str::FromStr;

const HEX32: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn chr() -> String {
    format!("chr_{HEX32}")
}
fn ctr() -> String {
    format!("ctr_{HEX32}")
}

#[test]
fn actor_ref_accepts_closed_arms() {
    let creator: ActorRef =
        serde_json::from_value(serde_json::json!({"actor_kind":"creator","creator_id": ctr()}))
            .expect("creator actor");
    let _ = creator;
    let character: ActorRef = serde_json::from_value(serde_json::json!({
        "actor_kind":"character","character_id": chr()
    }))
    .expect("character actor");
    let _ = character;
}

#[test]
fn actor_ref_rejects_unknown_discriminant_and_dual_ids() {
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind":"npc","creator_id": ctr()
    }))
    .is_err());
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind":"creator","creator_id": ctr(),"character_id": chr()
    }))
    .is_err());
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind":"character","character_id": chr(),"extra": true
    }))
    .is_err());
}

#[test]
fn character_rejects_bounds_and_extra_properties() {
    let mut valid = serde_json::json!({
        "schema_version": 1,
        "character_id": chr(),
        "owner_creator_id": ctr(),
        "display_name": "Ada",
        "status": "active",
        "persona": {},
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z",
        "revision": 0
    });
    serde_json::from_value::<Character>(valid.clone()).expect("valid character");
    valid["display_name"] = serde_json::json!("");
    assert!(serde_json::from_value::<Character>(valid).is_err());
}

#[test]
fn create_request_rejects_ownership_leak() {
    assert!(
        serde_json::from_value::<CreateCharacterRequest>(serde_json::json!({
            "display_name":"Ada",
            "world_id": format!("wld_{HEX32}"),
            "owner_creator_id": ctr()
        }))
        .is_err()
    );
    serde_json::from_value::<CreateCharacterRequest>(serde_json::json!({
        "display_name":"Ada",
        "world_id": format!("wld_{HEX32}")
    }))
    .expect("valid create");
}

#[test]
fn binding_rejects_unknown_status() {
    let valid = serde_json::json!({
        "schema_version": 1,
        "binding_id": format!("awb_{HEX32}"),
        "character_id": chr(),
        "world_id": format!("wld_{HEX32}"),
        "status": "archived",
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    });
    assert!(serde_json::from_value::<ActorWorldBinding>(valid).is_err());
}

#[test]
fn root_status_populates_generated_records() {
    let character = serde_json::from_value::<Character>(serde_json::json!({
        "schema_version": 1,
        "character_id": chr(),
        "owner_creator_id": ctr(),
        "display_name": "Ada",
        "status": "active",
        "persona": {},
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z",
        "revision": 0
    }))
    .expect("character");
    assert_eq!(character.status, CharacterStatus::Active);
    assert_eq!(character.status.as_str(), "active");
    let constructed = Character {
        character_id: character.character_id.clone(),
        created_at: character.created_at,
        display_name: character.display_name.clone(),
        image_uri: None,
        owner_creator_id: character.owner_creator_id.clone(),
        persona: character.persona.clone(),
        schema_version: character.schema_version,
        status: CharacterStatus::Active,
        revision: character.revision,
        updated_at: character.updated_at,
    };
    assert_eq!(constructed.status, CharacterStatus::Active);

    let binding = serde_json::from_value::<ActorWorldBinding>(serde_json::json!({
        "schema_version": 1,
        "binding_id": format!("awb_{HEX32}"),
        "character_id": chr(),
        "world_id": format!("wld_{HEX32}"),
        "status": "active",
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    }))
    .expect("binding");
    assert_eq!(binding.status, ActorWorldBindingStatus::Active);
}

#[test]
fn character_display_name_unicode_scalar_and_trim_bounds() {
    use nexus_contracts::generated::domain::character::CharacterDisplayName;
    let ok_cjk: String = "你".repeat(120);
    CharacterDisplayName::from_str(&ok_cjk).expect("120 CJK scalars");
    assert!(CharacterDisplayName::from_str(&"你".repeat(121)).is_err());
    CharacterDisplayName::from_str(&"a".repeat(120)).expect("120 ascii");
    assert!(CharacterDisplayName::from_str("").is_err());
    assert!(CharacterDisplayName::from_str("   ").is_err());
    assert!(CharacterDisplayName::from_str(" Ada").is_err());
    assert!(CharacterDisplayName::from_str("Ada ").is_err());
}

#[test]
fn length_bounded_non_actor_string_keeps_whitespace() {
    use nexus_contracts::generated::daemon_api::worlds::create_fork_request::CreateForkRequestLabel;
    CreateForkRequestLabel::from_str("  fork label  ")
        .expect("length-only fork label keeps leading and trailing whitespace");
}

fn character_record(display_name: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "character_id": chr(),
        "owner_creator_id": ctr(),
        "display_name": display_name,
        "status": "active",
        "persona": {},
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    })
}

fn binding_record() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "binding_id": format!("awb_{HEX32}"),
        "character_id": chr(),
        "world_id": format!("wld_{HEX32}"),
        "status": "active",
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    })
}

#[test]
fn character_response_dtos_enforce_display_name_trim_and_unicode() {
    let ok = character_record("Ada");
    serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": &ok }))
        .expect("detail accepts trimmed name");
    serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
        "character": &ok,
        "binding": binding_record()
    }))
    .expect("create response accepts trimmed name");
    serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
        "items": [ok],
        "pagination": { "limit": 20, "has_more": false }
    }))
    .expect("list response accepts trimmed name");

    let cjk = character_record(&"你".repeat(120));
    serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": cjk }))
        .expect("detail accepts 120 CJK scalars");

    let leading = character_record(" Ada");
    assert!(serde_json::from_value::<CharacterDetail>(
        serde_json::json!({ "character": &leading })
    )
    .is_err());
    assert!(
        serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
            "character": &leading,
            "binding": binding_record()
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
            "items": [leading],
            "pagination": { "limit": 20, "has_more": false }
        }))
        .is_err()
    );

    let trailing = character_record("Ada ");
    assert!(serde_json::from_value::<CharacterDetail>(
        serde_json::json!({ "character": &trailing })
    )
    .is_err());
    assert!(
        serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
            "character": &trailing,
            "binding": binding_record()
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
            "items": [trailing],
            "pagination": { "limit": 20, "has_more": false }
        }))
        .is_err()
    );
}

#[test]
fn rust_fixtures_cover_malformed_ids() {
    let mut character = serde_json::json!({
        "schema_version": 1,
        "character_id": chr(),
        "owner_creator_id": ctr(),
        "display_name": "Ada",
        "status": "active",
        "persona": {},
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z",
        "revision": 0
    });
    character["character_id"] = serde_json::json!("chr_ABCDEF");
    assert!(serde_json::from_value::<Character>(character.clone()).is_err());
    character["character_id"] = serde_json::json!(format!("chr_{}", &HEX32[..31]));
    assert!(serde_json::from_value::<Character>(character).is_err());

    let mut binding = serde_json::json!({
        "schema_version": 1,
        "binding_id": "awb_short",
        "character_id": chr(),
        "world_id": format!("wld_{HEX32}"),
        "status": "active",
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z",
        "revision": 0
    });
    assert!(serde_json::from_value::<ActorWorldBinding>(binding.clone()).is_err());
    binding["binding_id"] = serde_json::json!(format!("awb_{HEX32}"));
    binding["character_id"] = serde_json::json!("chr_nothex");
    assert!(serde_json::from_value::<ActorWorldBinding>(binding).is_err());

    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind":"character","character_id":"chr_nothex"
    }))
    .is_err());
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind":"creator","creator_id": format!("CTR_{HEX32}")
    }))
    .is_err());
}

#[test]
fn character_revision_and_patch_boundary_fixtures() {
    let mut character = serde_json::json!({
        "schema_version": 1,
        "character_id": chr(),
        "owner_creator_id": ctr(),
        "display_name": "Ada",
        "status": "active",
        "persona": {},
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    });
    serde_json::from_value::<Character>(character.clone()).expect("revision 0");
    character["revision"] = serde_json::json!(9223372036854775806_i64);
    serde_json::from_value::<Character>(character.clone()).expect("max revision");
    character.as_object_mut().unwrap().remove("revision");
    assert!(serde_json::from_value::<Character>(character).is_err());

    let patch = serde_json::json!({"expected_revision": 0, "display_name": "Ada"});
    serde_json::from_value::<UpdateCharacterRequest>(patch).expect("patch");
    assert!(serde_json::from_value::<UpdateCharacterRequest>(serde_json::json!({
        "expected_revision": 0,
        "display_name": ""
    })).is_err());
    assert!(serde_json::from_value::<UpdateCharacterRequest>(serde_json::json!({
        "expected_revision": 0,
        "extra": true
    })).is_err());

    let life = serde_json::json!({"expected_revision": 1});
    serde_json::from_value::<CharacterLifecycleRequest>(life).expect("lifecycle");
}

#[test]
fn binding_revision_and_update_request_boundary_fixtures() {
    let mut binding = binding_record();
    serde_json::from_value::<ActorWorldBinding>(binding.clone()).expect("revision 0");
    binding["revision"] = serde_json::json!(9223372036854775806_i64);
    serde_json::from_value::<ActorWorldBinding>(binding.clone()).expect("max revision");
    binding.as_object_mut().unwrap().remove("revision");
    assert!(serde_json::from_value::<ActorWorldBinding>(binding).is_err());

    let patch = serde_json::json!({
        "expected_revision": 0,
        "world_sheet_entry_id": format!("kb_{HEX32}")
    });
    serde_json::from_value::<UpdateCharacterBindingRequest>(patch).expect("sheet patch");
    serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
        "expected_revision": 0,
        "world_sheet_entry_id": null
    }))
    .expect("null clears sheet");
    serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
        "expected_revision": 0
    }))
    .expect("omitted sheet member");
    assert!(serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
        "expected_revision": 0,
        "extra": true
    }))
    .is_err());
    assert!(serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
        "expected_revision": 0,
        "world_sheet_entry_id": "x".repeat(129)
    }))
    .is_err());

    let detail = serde_json::json!({ "binding": binding_record() });
    serde_json::from_value::<CharacterBindingDetail>(detail).expect("binding detail");
}

fn knowledge_view_item_json() -> serde_json::Value {
    serde_json::json!({
        "entry_id": format!("kb_{HEX32}"),
        "owner": { "kind": "character", "id": chr() },
        "creator_only": false,
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "status": "confirmed",
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z"
    })
}

#[test]
fn knowledge_view_item_revision_and_closed_shape_fixtures() {
    let mut item = knowledge_view_item_json();
    serde_json::from_value::<KnowledgeViewItem>(item.clone()).expect("revision 0");
    item["revision"] = serde_json::json!(9223372036854775806_i64);
    serde_json::from_value::<KnowledgeViewItem>(item.clone()).expect("max revision");
    item.as_object_mut().unwrap().remove("revision");
    assert!(serde_json::from_value::<KnowledgeViewItem>(item.clone()).is_err());
    item["owner"] = serde_json::json!({ "kind": "world", "id": format!("wld_{HEX32}") });
    item["revision"] = serde_json::json!(0);
    serde_json::from_value::<KnowledgeViewItem>(item.clone()).expect("world owner");
    item["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<KnowledgeViewItem>(item).is_err());
}

#[test]
fn knowledge_entry_detail_and_update_request_boundary_fixtures() {
    let item = knowledge_view_item_json();
    serde_json::from_value::<KnowledgeEntryDetail>(serde_json::json!({
        "item": item,
        "summary": "hello"
    }))
    .expect("detail with summary");
    serde_json::from_value::<KnowledgeEntryDetail>(serde_json::json!({
        "item": knowledge_view_item_json(),
        "summary": null
    }))
    .expect("detail with null summary");
    assert!(serde_json::from_value::<KnowledgeEntryDetail>(serde_json::json!({
        "item": knowledge_view_item_json(),
        "summary": "x".repeat(65537)
    }))
    .is_err());

    let patch = serde_json::json!({
        "expected_revision": 0,
        "canonical_name": "note-beta",
        "summary": ""
    });
    serde_json::from_value::<UpdateKnowledgeEntryRequest>(patch).expect("patch all members");
    serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "summary": null
    }))
    .expect("null clears summary");
    serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "canonical_name": "note-beta"
    }))
    .expect("omitted summary");
    assert!(serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "owner": { "kind": "character", "id": chr() }
    }))
    .is_err());
    assert!(serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "body": { "summary": "x" }
    }))
    .is_err());
    assert!(serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "modules": {}
    }))
    .is_err());
}

#[test]
fn add_knowledge_entry_request_summary_and_closed_shape_fixtures() {
    let base = serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha"
    });
    serde_json::from_value::<AddKnowledgeEntryRequest>(base).expect("without summary");
    serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "summary": ""
    }))
    .expect("empty summary is explicit value");
    assert!(serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "summary": "x".repeat(65537)
    }))
    .is_err());
    assert!(serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "owner": { "kind": "character", "id": chr() }
    }))
    .is_err());
    assert!(serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "body": { "summary": "x" }
    }))
    .is_err());
}

#[test]
fn delete_knowledge_entry_query_boundary_fixtures() {
    serde_json::from_value::<DeleteKnowledgeEntryQuery>(serde_json::json!({
        "expected_revision": 0
    }))
    .expect("delete query");
assert!(serde_json::from_value::<DeleteKnowledgeEntryQuery>(serde_json::json!({
        "expected_revision": 0,
        "extra": true
    }))
    .is_err());
}



#[test]
fn character_pending_review_info_requires_source_operation_id() {
    let manual = serde_json::json!({
        "pending_id": "pend_1",
        "session_id": "sess_1",
        "character_id": chr(),
        "task_kind": "unknown",
        "raw_digest": "digest",
        "created_at": "2026-09-05T10:00:00Z",
        "source_operation_id": null
    });
    let run = serde_json::json!({
        "pending_id": "run_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "session_id": "sess_1",
        "character_id": chr(),
        "task_kind": "unknown",
        "raw_digest": "digest",
        "created_at": "2026-09-05T10:00:00Z",
        "source_operation_id": "op_host_1"
    });
    let manual_parsed = serde_json::from_value::<CharacterPendingReviewInfo>(manual.clone())
        .expect("manual pending");
    assert!(manual_parsed.source_operation_id.is_none());
    let run_parsed = serde_json::from_value::<CharacterPendingReviewInfo>(run.clone())
        .expect("run pending");
    assert!(run_parsed.source_operation_id.is_some());
}

#[test]
fn character_operation_result_and_capture_outcome_roundtrip() {
    let capture = serde_json::json!({
        "status": "pending",
        "pending_id": null,
        "code": null
    });
    serde_json::from_value::<CharacterRunCaptureOutcome>(capture).expect("capture");
    let result = serde_json::json!({
        "operation_id": "op_1",
        "session_id": "sess_1",
        "run_status": "running",
        "finish_reason": null,
        "capture": {
            "status": "disabled",
            "pending_id": null,
            "code": null
        }
    });
    serde_json::from_value::<CharacterOperationResult>(result).expect("operation result");
}
