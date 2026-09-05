//! v1.184 P2 Task 1 — generated agent-host session pair rules.

use nexus_contracts::generated::daemon_api::agent_host::{CreateSessionRequest, ExecuteOperationRequest, SessionResponse};

#[test]
fn legacy_create_session_json_roundtrip_omits_actor_fields() {
    let req: CreateSessionRequest = serde_json::from_str(
        r#"{"provider_id":"claude-native","cwd":"/tmp"}"#,
    )
    .expect("legacy request");
    assert!(req.actor_ref.is_none());
    assert!(req.viewpoint.is_none());
    assert_eq!(req.provider_id, "claude-native");
}

#[test]
fn both_present_is_actor_mode_payload() {
    let req: CreateSessionRequest = serde_json::from_value(serde_json::json!({
        "provider_id": "claude-native",
        "actor_ref": {"actor_kind":"creator","creator_id":"ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
        "viewpoint": {"world_id":"wld_worldA"}
    }))
    .expect("actor pair");
    assert!(req.actor_ref.is_some());
    assert!(req.viewpoint.is_some());
}

#[test]
fn session_response_omits_optional_pair_when_absent() {
    let resp = SessionResponse {
        active_op_id: None,
        actor_ref: None,
        model: None,
        provider_id: "p".into(),
        session_id: "s".into(),
        state: "Ready".into(),
        viewpoint: None,
    };
    assert_eq!(
        serde_json::to_value(resp).unwrap(),
        serde_json::json!({"session_id":"s","provider_id":"p","state":"Ready"})
    );
}

#[test]
fn prompt_operation_kind_is_snake_case() {
    let req = ExecuteOperationRequest::Prompt {
        content: "hello".into(),
    };
    assert_eq!(
        serde_json::to_value(req).unwrap(),
        serde_json::json!({"kind":"prompt","content":"hello"})
    );
}
