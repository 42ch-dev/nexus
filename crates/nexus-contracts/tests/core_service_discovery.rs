//! P5-T0 bounded integration target: the frozen launch/discovery contract.
//!
//! Covers `CoreServiceDiscovery` / `CoreServiceStopRequest` generated from
//! `schemas/core/core-service-discovery.schema.json` and
//! `schemas/core/core-service-stop-request.schema.json`:
//!
//! - exact raw-home / selected-workspace identity round-trips (the DTO layer
//!   never rewrites the raw user home),
//! - generated deserialization is closed: unknown fields and ambiguous
//!   endpoint / instance / epoch shapes are rejected.

use nexus_contracts::generated::core::core_service_discovery::CoreServiceDiscovery;
use nexus_contracts::generated::core::core_service_stop_request::CoreServiceStopRequest;
use serde_json::{json, Value};

/// A valid `ready` HTTP discovery record with a raw (un-suffixed) user home
/// and selected creator/workspace identity.
fn ready_http_discovery() -> Value {
    json!({
        "schema_version": 1,
        "instance_id": "inst-7f3c9a1d2b",
        "pid": 4128,
        "user_home": "/Users/ava/raw-home",
        "creator_id": "ctr_localabcdef123456",
        "workspace_slug": "novel-draft",
        "engine_epoch": 3,
        "endpoint": { "transport": "http", "url": "https://127.0.0.1:8123" },
        "tls_fingerprint": null,
        "readiness": "ready",
        "protocol_version": 1
    })
}

#[test]
fn discovery_and_stop_reject_ambiguous_identity() {
    // ── Exact identity round-trip (raw home + selected workspace) ─────────
    let record = ready_http_discovery();
    let dto: CoreServiceDiscovery =
        serde_json::from_value(record.clone()).expect("ready HTTP record deserializes");
    let back: Value = serde_json::to_value(&dto).expect("ready HTTP record re-serializes");
    assert_eq!(back, record, "round-trip must preserve the record exactly");
    // Raw home semantics: the DTO carries the raw user home verbatim
    // (home-layout appends `.nexus42` exactly once, never the wire type).
    assert_eq!(dto.user_home.as_str(), "/Users/ava/raw-home");
    assert_eq!(dto.creator_id.as_deref(), Some("ctr_localabcdef123456"));
    assert_eq!(dto.workspace_slug.as_deref(), Some("novel-draft"));
    assert_eq!(dto.engine_epoch, Some(3));

    // Uninitialized shell: null identity/epoch is representable and stable.
    let shell = json!({
        "schema_version": 1,
        "instance_id": "inst-0f0f0f0f0f",
        "pid": 0,
        "user_home": "/Users/ava/raw-home",
        "creator_id": null,
        "workspace_slug": null,
        "engine_epoch": null,
        "endpoint": { "transport": "unix", "path": "/Users/ava/raw-home/.nexus42/run/service.sock" },
        "tls_fingerprint": null,
        "readiness": "uninitialized",
        "protocol_version": 1
    });
    let shell_dto: CoreServiceDiscovery =
        serde_json::from_value(shell.clone()).expect("uninitialized shell deserializes");
    assert_eq!(
        serde_json::to_value(&shell_dto).expect("shell re-serializes"),
        shell
    );
    assert_eq!(shell_dto.creator_id, None);
    assert_eq!(shell_dto.engine_epoch, None);

    // ── Closed object: unknown fields are rejected ─────────────────────────
    let mut leaky = ready_http_discovery();
    leaky["bearer_secret"] = json!("must-not-carry-secrets");
    assert!(serde_json::from_value::<CoreServiceDiscovery>(leaky).is_err());

    let mut stray = ready_http_discovery();
    stray["extra"] = json!(1);
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(stray).is_err(),
        "unknown fields must be rejected on the closed discovery object"
    );

    let mut mixed = ready_http_discovery();
    mixed["endpoint"] = json!({ "transport": "http", "url": "https://127.0.0.1:8123", "path": "/tmp/service.sock" });
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(mixed).is_err(),
        "an endpoint carrying both URL and socket path is ambiguous"
    );

    let mut socket_as_http = ready_http_discovery();
    socket_as_http["endpoint"] = json!({ "transport": "http", "path": "/tmp/service.sock" });
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(socket_as_http).is_err(),
        "http arm requires a URL, not a socket path"
    );

    let mut url_as_unix = shell.clone();
    url_as_unix["endpoint"] = json!({ "transport": "unix", "url": "https://127.0.0.1:8123" });
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(url_as_unix).is_err(),
        "unix arm requires a socket path, not a URL"
    );

    let mut unknown_transport = ready_http_discovery();
    unknown_transport["endpoint"] = json!({ "transport": "grpc", "url": "https://127.0.0.1:8123" });
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(unknown_transport).is_err(),
        "unknown transports match neither union arm"
    );

    let mut socket_url = ready_http_discovery();
    socket_url["endpoint"] = json!({ "transport": "http", "url": "/tmp/service.sock" });
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(socket_url).is_err(),
        "the http arm rejects a socket path where a URL is required"
    );

    // ── Ambiguous instance identity is rejected ────────────────────────────
    let mut no_instance = ready_http_discovery();
    no_instance.as_object_mut().unwrap().remove("instance_id");
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(no_instance).is_err(),
        "a record without an instance id cannot be attached or stopped"
    );

    let mut empty_instance = ready_http_discovery();
    empty_instance["instance_id"] = json!("");
    assert!(
        serde_json::from_value::<CoreServiceDiscovery>(empty_instance).is_err(),
        "an empty instance id is not a usable identity"
    );

    // ── Stop request: exact identity + epoch, closed and total ─────────────
    let stop = json!({
        "expected_instance_id": "inst-7f3c9a1d2b",
        "expected_engine_epoch": 3
    });
    let stop_dto: CoreServiceStopRequest =
        serde_json::from_value(stop.clone()).expect("stop request deserializes");
    assert_eq!(
        serde_json::to_value(&stop_dto).expect("stop request re-serializes"),
        stop
    );
    assert_eq!(stop_dto.expected_instance_id.as_str(), "inst-7f3c9a1d2b");
    assert_eq!(stop_dto.expected_engine_epoch, Some(3));

    // Null epoch targets an uninitialized service and round-trips.
    let stop_shell: CoreServiceStopRequest = serde_json::from_value(json!({
        "expected_instance_id": "inst-0f0f0f0f0f",
        "expected_engine_epoch": null
    }))
    .expect("null-epoch stop request deserializes");
    assert_eq!(stop_shell.expected_engine_epoch, None);

    // Ambiguous epoch shapes: mistyped and unknown fields.
    assert!(serde_json::from_value::<CoreServiceStopRequest>(json!({
        "expected_instance_id": "inst-7f3c9a1d2b",
        "expected_engine_epoch": "3"
    }))
    .is_err());
    assert!(serde_json::from_value::<CoreServiceStopRequest>(json!({
        "expected_instance_id": "inst-7f3c9a1d2b",
        "expected_engine_epoch": 3,
        "pid": 4128
    }))
    .is_err());

    // Identity must be present on stop as well.
    assert!(serde_json::from_value::<CoreServiceStopRequest>(json!({
        "expected_engine_epoch": 3
    }))
    .is_err());
}
