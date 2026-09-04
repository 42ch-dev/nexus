//! Closed Actor/Character/ActorWorldBinding wire fixtures (v1.184 P0 Task 1).

use nexus_contracts::{
    ActorRef, ActorWorldBinding, ActorWorldBindingStatus, Character, CharacterDetail,
    CharacterStatus, CreateCharacterRequest, CreateCharacterResponse, ListCharactersResponse,
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
        "updated_at": "2026-09-05T00:00:00Z"
    });
    serde_json::from_value::<Character>(valid.clone()).expect("valid character");
    valid["display_name"] = serde_json::json!("");
    assert!(serde_json::from_value::<Character>(valid).is_err());
}

#[test]
fn create_request_rejects_ownership_leak() {
    assert!(serde_json::from_value::<CreateCharacterRequest>(serde_json::json!({
        "display_name":"Ada",
        "world_id": format!("wld_{HEX32}"),
        "owner_creator_id": ctr()
    }))
    .is_err());
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
        "updated_at": "2026-09-05T00:00:00Z"
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
        updated_at: character.updated_at,
    };
    assert_eq!(constructed.status, CharacterStatus::Active);

    let binding = serde_json::from_value::<ActorWorldBinding>(serde_json::json!({
        "schema_version": 1,
        "binding_id": format!("awb_{HEX32}"),
        "character_id": chr(),
        "world_id": format!("wld_{HEX32}"),
        "status": "active",
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
        "created_at": "2026-09-05T00:00:00Z",
        "updated_at": "2026-09-05T00:00:00Z"
    })
}

#[test]
fn character_response_dtos_enforce_display_name_trim_and_unicode() {
    let ok = character_record("Ada");
    serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": ok.clone() }))
        .expect("detail accepts trimmed name");
    serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
        "character": ok.clone(),
        "binding": binding_record()
    }))
    .expect("create response accepts trimmed name");
    serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
        "items": [ok.clone()],
        "pagination": { "limit": 20, "has_more": false }
    }))
    .expect("list response accepts trimmed name");

    let cjk = character_record(&"你".repeat(120));
    serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": cjk }))
        .expect("detail accepts 120 CJK scalars");

    let leading = character_record(" Ada");
    assert!(serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": leading.clone() })).is_err());
    assert!(serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
        "character": leading.clone(),
        "binding": binding_record()
    }))
    .is_err());
    assert!(serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
        "items": [leading],
        "pagination": { "limit": 20, "has_more": false }
    }))
    .is_err());

    let trailing = character_record("Ada ");
    assert!(serde_json::from_value::<CharacterDetail>(serde_json::json!({ "character": trailing.clone() })).is_err());
    assert!(serde_json::from_value::<CreateCharacterResponse>(serde_json::json!({
        "character": trailing.clone(),
        "binding": binding_record()
    }))
    .is_err());
    assert!(serde_json::from_value::<ListCharactersResponse>(serde_json::json!({
        "items": [trailing],
        "pagination": { "limit": 20, "has_more": false }
    }))
    .is_err());
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
        "updated_at": "2026-09-05T00:00:00Z"
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
        "updated_at": "2026-09-05T00:00:00Z"
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
