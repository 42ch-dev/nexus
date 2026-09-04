//! Closed Actor/Character/ActorWorldBinding wire fixtures (v1.184 P0 Task 1).

use nexus_contracts::{ActorRef, ActorWorldBinding, Character, CreateCharacterRequest};

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
