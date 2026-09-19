//! Closed Actor/Character/ActorWorldBinding wire fixtures (v1.184 P0 Task 1).

use nexus_contracts::{
    ActorRef, ActorWorldBinding, ActorWorldBindingStatus, AddKnowledgeEntryRequest,
    AddKnowledgeEntryRequestAudience, Character, CharacterBindingDetail, CharacterDetail,
    CharacterHolderEntryId, CharacterLifecycleRequest, CharacterOperationResult,
    CharacterPendingReviewInfo, CharacterRunCaptureOutcome, CharacterStatus,
    CreateCharacterRequest, CreateCharacterResponse, CreatorDetail, CreatorDetailHolderEntryId,
    DeleteKnowledgeEntryQuery, KnowledgeEntryDetail, KnowledgeViewItem,
    KnowledgeViewItemDisclosure, KnowledgeViewItemHolderEntryId, ListCharactersResponse,
    UpdateCharacterBindingRequest, UpdateCharacterRequest, UpdateKnowledgeEntryRequest,
    UpdateKnowledgeEntryRequestAudience, WorldKbEntityPatch, WorldKbEntityPatchAudience,
    WorldKbEntityProjection, WorldKbEntityProjectionDisclosure,
    WorldKbEntityProjectionHolderEntryId, WorldKbPatchEntityRequest,
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
        holder_entry_id: None,
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
    character["revision"] = serde_json::json!(9_223_372_036_854_775_806_i64);
    serde_json::from_value::<Character>(character.clone()).expect("max revision");
    character.as_object_mut().unwrap().remove("revision");
    assert!(serde_json::from_value::<Character>(character).is_err());

    let patch = serde_json::json!({"expected_revision": 0, "display_name": "Ada"});
    serde_json::from_value::<UpdateCharacterRequest>(patch).expect("patch");
    assert!(
        serde_json::from_value::<UpdateCharacterRequest>(serde_json::json!({
            "expected_revision": 0,
            "display_name": ""
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateCharacterRequest>(serde_json::json!({
            "expected_revision": 0,
            "extra": true
        }))
        .is_err()
    );

    let life = serde_json::json!({"expected_revision": 1});
    serde_json::from_value::<CharacterLifecycleRequest>(life).expect("lifecycle");
}

#[test]
fn binding_revision_and_update_request_boundary_fixtures() {
    let mut binding = binding_record();
    serde_json::from_value::<ActorWorldBinding>(binding.clone()).expect("revision 0");
    binding["revision"] = serde_json::json!(9_223_372_036_854_775_806_i64);
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
    assert!(
        serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
            "expected_revision": 0,
            "extra": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateCharacterBindingRequest>(serde_json::json!({
            "expected_revision": 0,
            "world_sheet_entry_id": "x".repeat(129)
        }))
        .is_err()
    );

    let detail = serde_json::json!({ "binding": binding_record() });
    serde_json::from_value::<CharacterBindingDetail>(detail).expect("binding detail");
}

fn knowledge_view_item_json() -> serde_json::Value {
    serde_json::json!({
        "entry_id": format!("kb_{HEX32}"),
        "owner": { "kind": "character", "id": chr() },
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "status": "confirmed",
        "revision": 0,
        "created_at": "2026-09-05T00:00:00Z"
    })
}

/// `knowledge_view_item_json()` with one member substituted (governance cases).
fn knowledge_view_item_json_with(key: &str, value: serde_json::Value) -> serde_json::Value {
    let mut item = knowledge_view_item_json();
    item[key] = value;
    item
}

/// Minimal `CreatorDetail` body — the inline creator family, no `$ref` leaf.
fn creator_record() -> serde_json::Value {
    serde_json::json!({
        "creator_id": ctr(),
        "has_api_key": false,
        "has_cached_token": false,
        "is_active": true
    })
}

#[test]
fn knowledge_view_item_revision_and_closed_shape_fixtures() {
    let mut item = knowledge_view_item_json();
    serde_json::from_value::<KnowledgeViewItem>(item.clone()).expect("revision 0");
    item["revision"] = serde_json::json!(9_223_372_036_854_775_806_i64);
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
    assert!(
        serde_json::from_value::<KnowledgeEntryDetail>(serde_json::json!({
            "item": knowledge_view_item_json(),
            "summary": "x".repeat(65537)
        }))
        .is_err()
    );

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
    assert!(
        serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
            "expected_revision": 0,
            "owner": { "kind": "character", "id": chr() }
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
            "expected_revision": 0,
            "body": { "summary": "x" }
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
            "expected_revision": 0,
            "modules": {}
        }))
        .is_err()
    );
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
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
            "owner_kind": "character",
            "character_id": chr(),
            "block_type": "info_point",
            "canonical_name": "note-alpha",
            "summary": "x".repeat(65537)
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
            "owner_kind": "character",
            "character_id": chr(),
            "block_type": "info_point",
            "canonical_name": "note-alpha",
            "owner": { "kind": "character", "id": chr() }
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(serde_json::json!({
            "owner_kind": "character",
            "character_id": chr(),
            "block_type": "info_point",
            "canonical_name": "note-alpha",
            "body": { "summary": "x" }
        }))
        .is_err()
    );
}

#[test]
fn delete_knowledge_entry_query_boundary_fixtures() {
    serde_json::from_value::<DeleteKnowledgeEntryQuery>(serde_json::json!({
        "expected_revision": 0
    }))
    .expect("delete query");
    assert!(
        serde_json::from_value::<DeleteKnowledgeEntryQuery>(serde_json::json!({
            "expected_revision": 0,
            "extra": true
        }))
        .is_err()
    );
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
    let manual_parsed =
        serde_json::from_value::<CharacterPendingReviewInfo>(manual).expect("manual pending");
    assert!(manual_parsed.source_operation_id.is_none());
    let run_parsed =
        serde_json::from_value::<CharacterPendingReviewInfo>(run).expect("run pending");
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

// ── v1.191 P1 T2 native holder governance (P0 custodian checkpoint) ────────
//
// The frozen T2 schema inputs retire the legacy `creator_only` boolean in
// favour of a native holder pair (`holder_entry_id` `^hld_[0-9a-f]{64}$` plus
// an `owner-private`-only `disclosure`) authored through a closed `audience`.
// These cases pin the *generated* DTOs against that frozen wire shape. They
// assert wire decoding only — no governance decision is taken here, and the
// service-resolved holder/verbatim-key gates live in `nexus-knowledge`
// (T2's own `v1191_governance` cases).

const HEX64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn hld() -> String {
    format!("hld_{HEX64}")
}

/// The create body with a substituted `audience` member.
fn add_request_with_audience(audience: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha",
        "audience": audience
    })
}

fn add_request_without_audience() -> serde_json::Value {
    serde_json::json!({
        "owner_kind": "character",
        "character_id": chr(),
        "block_type": "info_point",
        "canonical_name": "note-alpha"
    })
}

#[test]
fn add_knowledge_entry_audience_accepts_the_three_closed_kinds() {
    // Omission is the shared default, not a third state.
    let omitted =
        serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_without_audience())
            .expect("omitted audience");
    assert!(omitted.audience.is_none());

    let shared = serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
        &serde_json::json!({ "kind": "shared" }),
    ))
    .expect("shared audience");
    assert!(matches!(
        shared.audience,
        Some(AddKnowledgeEntryRequestAudience::Shared)
    ));
    assert_eq!(
        serde_json::to_value(&shared).expect("serializes")["audience"],
        serde_json::json!({ "kind": "shared" })
    );

    let author_only = serde_json::from_value::<AddKnowledgeEntryRequest>(
        add_request_with_audience(&serde_json::json!({ "kind": "author-only" })),
    )
    .expect("author-only audience");
    assert!(matches!(
        author_only.audience,
        Some(AddKnowledgeEntryRequestAudience::AuthorOnly)
    ));

    let private = serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
        &serde_json::json!({ "kind": "character-private", "character_id": chr() }),
    ))
    .expect("character-private audience");
    let wire = serde_json::to_value(&private).expect("serializes");
    assert_eq!(
        wire["audience"],
        serde_json::json!({ "kind": "character-private", "character_id": chr() })
    );
    let Some(AddKnowledgeEntryRequestAudience::CharacterPrivate(character_id)) = private.audience
    else {
        panic!("character-private must decode to its id-carrying arm");
    };
    assert_eq!(character_id.as_str(), chr());
}

#[test]
fn add_knowledge_entry_audience_rejects_every_non_native_shape() {
    // `character-private` is the only arm with a body: the `chr_` id is required
    // and pattern-checked, exactly as the frozen `oneOf` declares.
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
            &serde_json::json!({ "kind": "character-private" })
        ))
        .is_err()
    );
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
            &serde_json::json!({ "kind": "character-private", "character_id": "chr_nothex" })
        ))
        .is_err()
    );
    // Unknown kind, a non-object audience, and a wrong-cased kind.
    for audience in [
        serde_json::json!({ "kind": "viewer" }),
        serde_json::json!({ "kind": "AuthorOnly" }),
        serde_json::json!([]),
        serde_json::json!("shared"),
    ] {
        assert!(
            serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
                &audience.clone()
            ))
            .is_err(),
            "accepted non-native audience {audience}"
        );
    }
}

/// **Known generator gap — reported to the PM at this checkpoint.** Every
/// frozen audience `oneOf` arm declares `additionalProperties: false`, but the
/// generated adjacently-tagged DTO (`tag = "kind"`, `content =
/// "character_id"`) scans only for the tag/content keys and silently drops any
/// other member of the arm object. The authoring guards that carry governance
/// meaning are unaffected — root-level `additionalProperties: false` (no
/// `creator_only`, no client-authored `holder_entry_id`/`disclosure`) and the
/// strict `kind` tag are both still enforced, see
/// `knowledge_authoring_requests_reject_legacy_and_service_resolved_keys` and
/// `add_knowledge_entry_audience_rejects_every_non_native_shape`.
///
/// This case documents the tolerance so it cannot be mistaken for closure; it
/// fails loudly (making the finding actionable) if a schema or codegen change
/// makes the arms strict.
#[test]
fn audience_arm_extra_member_is_tolerated_by_the_generated_dto() {
    let tolerated = serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
        &serde_json::json!({ "kind": "shared", "extra": true }),
    ))
    .expect("the generated DTO drops an unknown member inside the arm");
    assert!(matches!(
        tolerated.audience,
        Some(AddKnowledgeEntryRequestAudience::Shared)
    ));

    // Only the tag is scanned strictly: an unknown `kind` is still refused,
    // with or without the extra member.
    assert!(
        serde_json::from_value::<AddKnowledgeEntryRequest>(add_request_with_audience(
            &serde_json::json!({ "kind": "viewer", "extra": true })
        ))
        .is_err()
    );
}

#[test]
fn knowledge_authoring_requests_reject_legacy_and_service_resolved_keys() {
    // Presence of the legacy `creator_only` key is rejected *including* `false`
    // (durable §3): the retired flag must never be read as "not restrictive".
    for value in [serde_json::json!(false), serde_json::json!(true)] {
        let mut create = add_request_with_audience(&serde_json::json!({ "kind": "shared" }));
        create["creator_only"] = value.clone();
        assert!(
            serde_json::from_value::<AddKnowledgeEntryRequest>(create).is_err(),
            "create accepted creator_only={value}"
        );

        let mut patch =
            serde_json::json!({ "expected_revision": 0, "canonical_name": "note-beta" });
        patch["creator_only"] = value.clone();
        assert!(
            serde_json::from_value::<UpdateKnowledgeEntryRequest>(patch).is_err(),
            "patch accepted creator_only={value}"
        );
    }

    // `holder_entry_id` / `disclosure` are service-resolved projections: no
    // authoring body may supply them.
    for (key, value) in [
        ("holder_entry_id", serde_json::json!(hld())),
        ("disclosure", serde_json::json!("owner-private")),
    ] {
        let mut create = add_request_with_audience(&serde_json::json!({ "kind": "author-only" }));
        create[key] = value.clone();
        assert!(
            serde_json::from_value::<AddKnowledgeEntryRequest>(create).is_err(),
            "create accepted client-authored {key}"
        );

        let mut patch = serde_json::json!({ "expected_revision": 0 });
        patch[key] = value;
        assert!(
            serde_json::from_value::<UpdateKnowledgeEntryRequest>(patch).is_err(),
            "patch accepted client-authored {key}"
        );
    }
}

#[test]
fn update_knowledge_entry_audience_is_cas_bound_and_omittable() {
    // Omission preserves stored governance; explicit `shared` is the authored
    // clear. Both ride the same `expected_revision` as content.
    let preserved = serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "canonical_name": "note-beta"
    }))
    .expect("omitted audience");
    assert!(preserved.audience.is_none());

    let cleared = serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "audience": { "kind": "shared" }
    }))
    .expect("explicit clear");
    assert!(matches!(
        cleared.audience,
        Some(UpdateKnowledgeEntryRequestAudience::Shared)
    ));

    let private = serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
        "expected_revision": 0,
        "audience": { "kind": "character-private", "character_id": chr() }
    }))
    .expect("private audience");
    assert!(matches!(
        private.audience,
        Some(UpdateKnowledgeEntryRequestAudience::CharacterPrivate(_))
    ));

    // A governance edit without the CAS revision is refused outright.
    assert!(
        serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
            "audience": { "kind": "shared" }
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<UpdateKnowledgeEntryRequest>(serde_json::json!({
            "expected_revision": 0,
            "audience": { "kind": "character-private" }
        }))
        .is_err()
    );
}

#[test]
fn knowledge_view_item_holder_governance_projection_fixtures() {
    // In-scope shared row: both governance members are absent — never the
    // string "shared", never a `false` flag.
    let shared = serde_json::from_value::<KnowledgeViewItem>(knowledge_view_item_json())
        .expect("shared projection");
    assert!(shared.holder_entry_id.is_none());
    assert!(shared.disclosure.is_none());
    let wire = serde_json::to_value(&shared).expect("serializes");
    assert!(wire.get("holder_entry_id").is_none());
    assert!(wire.get("disclosure").is_none());
    assert!(wire.get("creator_only").is_none());

    // Disclosure-restricted row: the resolved pair travels together.
    let mut private = knowledge_view_item_json();
    private["holder_entry_id"] = serde_json::json!(hld());
    private["disclosure"] = serde_json::json!("owner-private");
    let private = serde_json::from_value::<KnowledgeViewItem>(private).expect("private projection");
    assert_eq!(
        private.holder_entry_id.as_deref().map(String::as_str),
        Some(hld().as_str())
    );
    assert_eq!(
        private.disclosure,
        Some(KnowledgeViewItemDisclosure::OwnerPrivate)
    );
    let wire = serde_json::to_value(&private).expect("serializes");
    assert_eq!(wire["holder_entry_id"], serde_json::json!(hld()));
    assert_eq!(wire["disclosure"], serde_json::json!("owner-private"));

    // The pair is optional-but-typed: the raw wire rejects a bare string here,
    // the typed gate below owns the pattern.
    assert!(
        serde_json::from_value::<KnowledgeViewItem>(knowledge_view_item_json_with(
            "creator_only",
            serde_json::json!(false)
        ))
        .is_err()
    );
}

#[test]
fn holder_entry_id_projection_enforces_the_hld_pattern() {
    KnowledgeViewItemHolderEntryId::from_str(&hld()).expect("valid holder id");
    for malformed in [
        String::new(),
        "hld_".to_string(),
        format!("hld_{}", &HEX64[..63]),
        format!("hld_{HEX64}0"),
        format!("hld_{}", HEX64.to_uppercase()),
        format!("HLD_{HEX64}"),
        "hld_nothex".to_string(),
        format!("kb_{HEX64}"),
    ] {
        assert!(
            KnowledgeViewItemHolderEntryId::from_str(&malformed).is_err(),
            "accepted malformed holder id {malformed:?}"
        );
        let item = knowledge_view_item_json_with("holder_entry_id", serde_json::json!(malformed));
        assert!(
            serde_json::from_value::<KnowledgeViewItem>(item).is_err(),
            "wire accepted malformed holder id"
        );
    }
}

#[test]
fn disclosure_projection_is_the_owner_private_vocabulary_only() {
    KnowledgeViewItemDisclosure::from_str("owner-private").expect("the one known disclosure");
    for rejected in [
        "shared",
        "owner-public",
        "OWNER-PRIVATE",
        "owner_private",
        "",
        "character-private",
    ] {
        assert!(
            KnowledgeViewItemDisclosure::from_str(rejected).is_err(),
            "accepted unknown disclosure {rejected:?}"
        );
        // "shared" is the absence of disclosure, never a disclosure value.
        let item = knowledge_view_item_json_with("disclosure", serde_json::json!(rejected));
        assert!(
            serde_json::from_value::<KnowledgeViewItem>(item).is_err(),
            "wire accepted unknown disclosure {rejected:?}"
        );
    }
}

#[test]
fn identity_detail_holder_projection_is_read_only() {
    // Character: the projection is optional, pattern-checked, and never
    // accepted on a create/bind body.
    let bare = serde_json::from_value::<Character>(character_record("Ada")).expect("no holder yet");
    assert!(bare.holder_entry_id.is_none());
    CharacterHolderEntryId::from_str(&hld()).expect("valid holder id");
    assert!(CharacterHolderEntryId::from_str("hld_short").is_err());

    let mut covered = character_record("Ada");
    covered["holder_entry_id"] = serde_json::json!(hld());
    let covered = serde_json::from_value::<Character>(covered).expect("holder projection");
    assert_eq!(
        covered.holder_entry_id.as_deref().map(String::as_str),
        Some(hld().as_str())
    );

    let mut malformed = character_record("Ada");
    malformed["holder_entry_id"] = serde_json::json!("hld_short");
    assert!(serde_json::from_value::<Character>(malformed).is_err());

    let mut create = serde_json::json!({
        "display_name": "Ada",
        "world_id": format!("wld_{HEX32}")
    });
    create["holder_entry_id"] = serde_json::json!(hld());
    assert!(serde_json::from_value::<CreateCharacterRequest>(create).is_err());

    // CreatorDetail: the same read-only projection, inline in its own family.
    let bare = serde_json::from_value::<CreatorDetail>(creator_record()).expect("no holder yet");
    assert!(bare.holder_entry_id.is_none());

    let mut covered = creator_record();
    covered["holder_entry_id"] = serde_json::json!(hld());
    let covered = serde_json::from_value::<CreatorDetail>(covered).expect("holder projection");
    assert_eq!(
        covered.holder_entry_id.as_deref().map(String::as_str),
        Some(hld().as_str())
    );
    CreatorDetailHolderEntryId::from_str(&hld()).expect("valid holder id");
    assert!(CreatorDetailHolderEntryId::from_str("hld_short").is_err());

    let mut malformed = creator_record();
    malformed["holder_entry_id"] = serde_json::json!("hld_short");
    assert!(serde_json::from_value::<CreatorDetail>(malformed).is_err());
}

#[test]
fn world_kb_patch_and_projection_share_the_same_governance_contract() {
    // The third authoring surface declares the identical closed audience.
    let mut shared = serde_json::json!({});
    shared["audience"] = serde_json::json!({ "kind": "shared" });
    let parsed = serde_json::from_value::<WorldKbEntityPatch>(shared).expect("shared patch");
    assert!(matches!(
        parsed.audience,
        Some(WorldKbEntityPatchAudience::Shared)
    ));

    let private = serde_json::json!({
        "audience": { "kind": "character-private", "character_id": chr() }
    });
    let parsed = serde_json::from_value::<WorldKbEntityPatch>(private).expect("private patch");
    assert!(matches!(
        parsed.audience,
        Some(WorldKbEntityPatchAudience::CharacterPrivate(_))
    ));

    for rejected in [
        serde_json::json!({ "audience": { "kind": "character-private" } }),
        serde_json::json!({ "audience": { "kind": "viewer" } }),
        serde_json::json!({ "creator_only": false }),
        serde_json::json!({ "holder_entry_id": hld() }),
        serde_json::json!({ "disclosure": "owner-private" }),
    ] {
        assert!(
            serde_json::from_value::<WorldKbEntityPatch>(rejected.clone()).is_err(),
            "patch accepted {rejected}"
        );
    }

    // The CAS envelope carries the governance patch under `expected_version`.
    let envelope = serde_json::json!({
        "entity_id": format!("kb_{HEX32}"),
        "expected_version": 0,
        "patch": { "audience": { "kind": "author-only" } }
    });
    let parsed = serde_json::from_value::<WorldKbPatchEntityRequest>(envelope)
        .expect("governance patch under CAS");
    assert_eq!(parsed.expected_version, 0);
    assert!(
        serde_json::from_value::<WorldKbPatchEntityRequest>(serde_json::json!({
            "entity_id": format!("kb_{HEX32}"),
            "patch": { "audience": { "kind": "shared" } }
        }))
        .is_err()
    );

    // The canvas projection carries the same pair as the ActorView projection.
    WorldKbEntityProjectionHolderEntryId::from_str(&hld()).expect("valid holder id");
    assert!(WorldKbEntityProjectionHolderEntryId::from_str("hld_short").is_err());
    WorldKbEntityProjectionDisclosure::from_str("owner-private").expect("the one disclosure");
    assert!(WorldKbEntityProjectionDisclosure::from_str("shared").is_err());

    let projection = serde_json::json!({
        "key_block_id": format!("kb_{HEX32}"),
        "world_id": format!("wld_{HEX32}"),
        "block_type": "character",
        "canonical_name": "Entity",
        "status": "confirmed",
        "version": 0,
        "holder_entry_id": hld(),
        "disclosure": "owner-private"
    });
    let parsed =
        serde_json::from_value::<WorldKbEntityProjection>(projection).expect("restricted row");
    assert_eq!(
        parsed.holder_entry_id.as_deref().map(String::as_str),
        Some(hld().as_str())
    );
    assert_eq!(
        parsed.disclosure,
        Some(WorldKbEntityProjectionDisclosure::OwnerPrivate)
    );

    let mut malformed = serde_json::json!({
        "key_block_id": format!("kb_{HEX32}"),
        "world_id": format!("wld_{HEX32}"),
        "block_type": "character",
        "canonical_name": "Entity",
        "status": "confirmed",
        "version": 0
    });
    malformed["holder_entry_id"] = serde_json::json!("hld_short");
    assert!(serde_json::from_value::<WorldKbEntityProjection>(malformed).is_err());
}
