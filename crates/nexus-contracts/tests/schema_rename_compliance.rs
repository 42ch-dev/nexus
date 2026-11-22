//! Schema rename compliance tests — V1.57 P2 (R-V156P1-M001).
//!
//! Verifies that the `agent_count` → `capability_count` rename in
//! `RegistryRefreshOutput` preserves backward compatibility via serde
//! field aliases.

use nexus_contracts::local::orchestration::RegistryRefreshOutput;
use serde_json::json;

#[test]
fn test_capability_count_rename_does_not_break_consumers_new_name() {
    let json = json!({
        "cacheAgeMs": 0,
        "capabilityCount": 42,
        "source": "synthetic",
        "snapshotVersion": "2026-06-22.v1",
        "generatedAt": "2026-06-22T00:00:00Z",
        "fetchTimeoutMs": 0,
        "maxRetries": 0,
        "retryCount": 0,
        "fallbackReason": ""
    });

    let output: RegistryRefreshOutput =
        serde_json::from_value(json).expect("deserialize with new field name capabilityCount");

    assert_eq!(output.capability_count, 42);
    assert_eq!(output.source, "synthetic");
}

#[test]
fn test_capability_count_rename_does_not_break_consumers_old_camelcase() {
    let json = json!({
        "cacheAgeMs": 0,
        "agentCount": 99,
        "source": "cdn",
        "snapshotVersion": "2026-06-22.v1",
        "generatedAt": "2026-06-22T00:00:00Z",
        "fetchTimeoutMs": 5000,
        "maxRetries": 3,
        "retryCount": 1,
        "fallbackReason": ""
    });

    let output: RegistryRefreshOutput =
        serde_json::from_value(json).expect("deserialize with old field name agentCount (alias)");

    assert_eq!(output.capability_count, 99);
    assert_eq!(output.source, "cdn");
}

#[test]
fn test_capability_count_rename_does_not_break_consumers_old_snakecase() {
    let json = json!({
        "cacheAgeMs": 0,
        "agent_count": 7,
        "source": "synthetic_fallback",
        "snapshotVersion": "2026-06-22.v1",
        "generatedAt": "2026-06-22T00:00:00Z",
        "fetchTimeoutMs": 5000,
        "maxRetries": 3,
        "retryCount": 3,
        "fallbackReason": "CDN unreachable"
    });

    let output: RegistryRefreshOutput =
        serde_json::from_value(json).expect("deserialize with old field name agent_count (alias)");

    assert_eq!(output.capability_count, 7);
    assert_eq!(output.source, "synthetic_fallback");
}

#[test]
fn test_capability_count_rename_serialization_uses_canonical_name() {
    let output = RegistryRefreshOutput {
        cache_age_ms: 0,
        capability_count: 35,
        source: "synthetic".to_string(),
        snapshot_version: "2026-06-22.v1".to_string(),
        generated_at: "2026-06-22T00:00:00Z".to_string(),
        fetch_timeout_ms: 0,
        max_retries: 0,
        retry_count: 0,
        fallback_reason: String::new(),
    };

    let json = serde_json::to_value(&output).expect("serialize RegistryRefreshOutput");

    assert!(
        !json.to_string().contains("agentCount"),
        "output must not contain old field name agentCount"
    );
    assert!(
        !json.to_string().contains("agent_count"),
        "output must not contain old field name agent_count"
    );

    assert_eq!(json["capabilityCount"], json!(35));
    assert_eq!(json["source"], json!("synthetic"));
}

#[test]
fn test_capability_count_rename_both_names_rejected_as_ambiguous() {
    let json = json!({
        "cacheAgeMs": 0,
        "agentCount": 10,
        "capabilityCount": 42,
        "source": "synthetic",
        "snapshotVersion": "2026-06-22.v1",
        "generatedAt": "2026-06-22T00:00:00Z",
        "fetchTimeoutMs": 0,
        "maxRetries": 0,
        "retryCount": 0,
        "fallbackReason": ""
    });

    let err = serde_json::from_value::<RegistryRefreshOutput>(json)
        .expect_err("both field names should be rejected as ambiguous");

    let err_msg = err.to_string();
    assert!(
        err_msg.contains("duplicate field"),
        "expected 'duplicate field' error, got: {err_msg}"
    );
}
