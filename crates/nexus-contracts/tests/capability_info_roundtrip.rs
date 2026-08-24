//! Round-trip serde tests for the generated `CapabilityInfo` wire type
//! (AR-40 additive `origin` provenance field).
//!
//! The schema declares `"origin"` with `"default": "builtin"` (additive;
//! `schema_version` stays 1; `additionalProperties` stays false). These tests
//! prove the generated Rust honors the default for back-compat payloads
//! (missing `origin` → `Builtin`) and round-trips both enum values.

use nexus_contracts::generated::daemon_api::orchestration::capabilities::capability_info::CapabilityInfoOrigin;
use nexus_contracts::CapabilityInfo;

#[test]
fn missing_origin_deserializes_with_default_builtin() {
    // Legacy payload without the new field must keep working (AR-40
    // back-compat: `origin` defaults to "builtin").
    let json = r#"{"name":"sync.pull","input_schema":"{}","output_schema":"{}"}"#;
    let info: CapabilityInfo = serde_json::from_str(json).expect("deserialize CapabilityInfo");
    assert_eq!(info.name, "sync.pull");
    assert_eq!(info.origin, CapabilityInfoOrigin::Builtin);
}

#[test]
fn origin_builtin_roundtrips() {
    let json =
        r#"{"name":"sync.pull","input_schema":"{}","output_schema":"{}","origin":"builtin"}"#;
    let info: CapabilityInfo = serde_json::from_str(json).expect("deserialize CapabilityInfo");
    assert_eq!(info.origin, CapabilityInfoOrigin::Builtin);
    let out = serde_json::to_value(&info).expect("serialize CapabilityInfo");
    assert_eq!(out["origin"], "builtin");
}

#[test]
fn origin_user_roundtrips() {
    let json = r#"{"name":"demo.pull","input_schema":"{}","output_schema":"{}","origin":"user"}"#;
    let info: CapabilityInfo = serde_json::from_str(json).expect("deserialize CapabilityInfo");
    assert_eq!(info.origin, CapabilityInfoOrigin::User);
    let out = serde_json::to_value(&info).expect("serialize CapabilityInfo");
    assert_eq!(out["origin"], "user");
}

#[test]
fn origin_peer_roundtrips() {
    // AR-68 #5: the wire origin enum gains "peer" (additive; the
    // orchestration CapabilityOrigin enum stays Builtin|User — the wire
    // string is produced by the handler).
    let json = r#"{"name":"tools.t3.echo","input_schema":"{}","output_schema":"{}","origin":"peer"}"#;
    let info: CapabilityInfo = serde_json::from_str(json).expect("deserialize CapabilityInfo");
    assert_eq!(info.origin, CapabilityInfoOrigin::Peer);
    let out = serde_json::to_value(&info).expect("serialize CapabilityInfo");
    assert_eq!(out["origin"], "peer");
}

#[test]
fn invalid_origin_value_is_rejected() {
    // The schema enum is closed: ["builtin","user"] — anything else must not
    // deserialize into the generated type.
    let json =
        r#"{"name":"demo.pull","input_schema":"{}","output_schema":"{}","origin":"marketplace"}"#;
    let err = serde_json::from_str::<CapabilityInfo>(json)
        .expect_err("closed enum rejects unknown origin");
    assert!(
        err.to_string().contains("unknown variant"),
        "error mentions the unknown variant: {err}"
    );
}
