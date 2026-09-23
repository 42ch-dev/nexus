//! Map domain errors to generated wire [`CoreError`] JSON for N-API consumers.

use napi::bindgen_prelude::Error;
use nexus_agent_host::core::readiness::safe_provider_message;
use nexus_agent_host::HostError;
use nexus_contracts::{CoreError, CoreErrorCode};
use nexus_core::CoreError as DomainError;
use serde_json::Value;

/// Serialize a wire [`CoreError`] as the N-API rejection reason.
pub fn napi_error_from_wire(err: CoreError) -> Error {
    let payload = serde_json::to_string(&err).unwrap_or_else(|_| {
        serde_json::to_string(&CoreError {
            code: CoreErrorCode::Internal,
            message: "internal: core error serialization failed".into(),
            details: Default::default(),
            http_status: Some(500),
        })
        .expect("fallback core error serializes")
    });
    Error::from_reason(payload)
}

/// Map a native-core domain error to wire JSON for TS `JSON.parse(message)`.
pub fn napi_error_from_domain(err: DomainError) -> Error {
    napi_error_from_wire(wire_core_error_from_domain(err))
}

/// Parse an `open_core` rejection reason into a typed N-API error.
pub fn napi_error_from_open_reason(reason: String) -> Error {
    if let Ok(wire) = serde_json::from_str::<CoreError>(&reason) {
        return napi_error_from_wire(wire);
    }
    napi_error_from_wire(CoreError {
        code: CoreErrorCode::Internal,
        message: "open failed".into(),
        details: serde_json::Map::from_iter([(
            "category".into(),
            Value::String("open_failed".into()),
        )]),
        http_status: Some(500),
    })
}

/// Serialize a wire [`CoreError`] for `open_core` rejection reasons.
pub fn open_reason_from_wire(err: CoreError) -> String {
    serde_json::to_string(&err).unwrap_or_else(|_| "open failed".to_string())
}

/// Serialize a domain error for `open_core` rejection reasons.
pub fn open_reason_from_domain(err: DomainError) -> String {
    open_reason_from_wire(wire_core_error_from_domain(err))
}

/// Serialize a host error for `open_core` rejection reasons.
pub fn open_reason_from_host(err: HostError) -> String {
    open_reason_from_wire(wire_core_error_from_host(err))
}

/// Serialize an invalid-input wire error for host-query style rejections.
pub fn open_reason_invalid_input(message: impl Into<String>) -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::InvalidInput,
        message: message.into(),
        details: Default::default(),
        http_status: Some(400),
    })
}

/// Serialize a not-found wire error for host-query style rejections.
pub fn open_reason_not_found(message: impl Into<String>) -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::NotFound,
        message: message.into(),
        details: Default::default(),
        http_status: Some(404),
    })
}

/// Serialize a sanitized internal wire error for host-query style rejections.
pub fn open_reason_internal() -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::Internal,
        message: "internal error".into(),
        details: Default::default(),
        http_status: Some(500),
    })
}

fn internal_error_bucket(category: &str) -> &'static str {
    if category.starts_with("config_load:") || category.starts_with("database_error:") {
        "configuration_or_database"
    } else if category.starts_with("spoke") || category.contains("kb_store") {
        "storage"
    } else {
        "internal"
    }
}

/// Map a host error to the generated wire envelope without leaking raw diagnostics.
pub fn wire_core_error_from_host(err: HostError) -> CoreError {
    let category = err.category();
    let message = safe_provider_message(&err);
    let code = match category {
        "policy_denied" | "owner_workspace_mismatch" => CoreErrorCode::Forbidden,
        "operation_cancelled" => CoreErrorCode::Interrupted,
        "operation_timeout" | "cleanup_unconfirmed" | "provider_unavailable" => CoreErrorCode::Busy,
        "launch_failed" | "capability_unsupported" | "provider_protocol_error" => {
            CoreErrorCode::InvalidInput
        }
        _ => CoreErrorCode::Internal,
    };
    let http_status = match code {
        CoreErrorCode::Forbidden => 403,
        CoreErrorCode::Interrupted | CoreErrorCode::Busy => 503,
        CoreErrorCode::InvalidInput => 400,
        _ => 500,
    };
    CoreError {
        code,
        message,
        details: serde_json::Map::from_iter([("category".into(), Value::String(category.into()))]),
        http_status: Some(http_status),
    }
}

/// Typed uninitialized denial shared by service-only entrypoints.
pub fn service_only_uninitialized_wire() -> CoreError {
    CoreError {
        code: CoreErrorCode::Uninitialized,
        message: "workspace not initialized".into(),
        details: Default::default(),
        http_status: Some(409),
    }
}

pub fn wire_core_error_from_domain(err: DomainError) -> CoreError {
    match err {
        DomainError::Uninitialized => CoreError {
            code: CoreErrorCode::Uninitialized,
            message: "workspace not initialized".into(),
            details: Default::default(),
            http_status: Some(409),
        },
        DomainError::AuthRequired => CoreError {
            code: CoreErrorCode::AuthRequired,
            message: "authentication required".into(),
            details: Default::default(),
            http_status: Some(401),
        },
        DomainError::Forbidden { resource } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: format!("forbidden: {resource}"),
            details: serde_json::Map::from_iter([("resource".into(), Value::String(resource))]),
            http_status: Some(403),
        },
        // World-owner scoping denial: mirrors the daemon adapter's
        // `Forbidden { resource: "world {world_id}", reason }` (403).
        DomainError::WorldOwnerDenied { world_id, reason } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: format!("forbidden: world {world_id} — {reason}"),
            details: serde_json::Map::from_iter([
                (
                    "resource".into(),
                    Value::String(format!("world {world_id}")),
                ),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(403),
        },
        DomainError::NotFound { resource } => CoreError {
            code: CoreErrorCode::NotFound,
            message: format!("not found: {resource}"),
            details: serde_json::Map::from_iter([("resource".into(), Value::String(resource))]),
            http_status: Some(404),
        },
        DomainError::InvalidInput { field, reason } => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: format!("invalid input: {field} — {reason}"),
            details: serde_json::Map::from_iter([
                ("field".into(), Value::String(field)),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(400),
        },
        // Outline-canvas OCC conflict: the daemon adapter forwards
        // current_revision/node_id/conflicting_path/recovery_hint as the 409
        // payload; the structs carry no Serialize, so the fields are mapped
        // one by one (same as the daemon's field-wise mapping). The generated
        // code vocabulary has no outline_conflict arm yet, so the generic
        // client-fault code carries the envelope.
        DomainError::OutlineConflict(details) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: "outline conflict".into(),
            details: serde_json::Map::from_iter([
                (
                    "current_revision".into(),
                    Value::Number(serde_json::Number::from(details.current_revision)),
                ),
                ("node_id".into(), Value::String(details.node_id)),
                (
                    "conflicting_path".into(),
                    Value::String(details.conflicting_path),
                ),
                ("recovery_hint".into(), Value::String(details.recovery_hint)),
            ]),
            http_status: Some(409),
        },
        // Outline-canvas validation failure: the summary lands under
        // `validation_summary` with the daemon's 422 (same vocabulary gap;
        // OutlineValidationError also maps field-wise).
        DomainError::OutlineValidation(summary) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: "outline validation failed".into(),
            details: serde_json::Map::from_iter([(
                "validation_summary".into(),
                serde_json::json!({
                    "errors": summary.errors,
                    "warnings": summary.warnings,
                }),
            )]),
            http_status: Some(422),
        },
        DomainError::WorldKbConflict(details) => {
            let detail_value = serde_json::to_value(&details).unwrap_or(Value::Null);
            let details_map = match detail_value {
                Value::Object(map) => map,
                _ => Default::default(),
            };
            CoreError {
                code: CoreErrorCode::WorldKbConflict,
                message: "world kb conflict".into(),
                details: details_map,
                http_status: Some(409),
            }
        }
        DomainError::WorldKbValidation(details) => {
            let detail_value = serde_json::to_value(&details).unwrap_or(Value::Null);
            let details_map = match detail_value {
                Value::Object(map) => map,
                _ => Default::default(),
            };
            CoreError {
                code: CoreErrorCode::WorldKbValidation,
                message: "world kb validation failed".into(),
                details: details_map,
                http_status: Some(422),
            }
        }
        // Compute input validation (V1.147 P3 F2): the core carries the
        // structured per-entry object (`{"invalid_entries": […]}`) and the
        // daemon renders it as the 422 `invalid_input` envelope with that
        // object verbatim under `details`; mirror both here.
        DomainError::InputValidation { details } => {
            let detail_value = serde_json::to_value(&details).unwrap_or(Value::Null);
            let details_map = match detail_value {
                Value::Object(map) => map,
                _ => Default::default(),
            };
            CoreError {
                code: CoreErrorCode::InvalidInput,
                message: "input validation failed".into(),
                details: details_map,
                http_status: Some(422),
            }
        }
        DomainError::OwnerBusy => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message: "writer owner busy".into(),
            details: Default::default(),
            http_status: Some(409),
        },
        DomainError::WriterFenced => CoreError {
            code: CoreErrorCode::WriterFenced,
            message: "writer fenced".into(),
            details: Default::default(),
            http_status: Some(409),
        },
        DomainError::SchemaMismatch => CoreError {
            code: CoreErrorCode::SchemaMismatch,
            message: "schema mismatch".into(),
            details: Default::default(),
            http_status: Some(409),
        },
        DomainError::Preset(error) => wire_core_error_from_preset(error),
        DomainError::Coded { code, message } => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "wire_code".into(),
                Value::String(code.clone()),
            )]),
            http_status: Some(coded_wire_status(&code)),
        },
        // A peer-side tool refusal. The spine's public code is always
        // `not_supported` (the sole raise site, `capabilities.rs`, sets it
        // literally); the peer's own lowercase code rides in
        // `details.wire_code`, verbatim — never re-parsed from the message.
        // Mirrors `From<CoreError> for NexusApiError`, where
        // `PeerDenied { code, wire_code, message }` becomes `PeerToolDenied`:
        // `error_code()` returns the public `code`, `details` carry
        // `wire_code`, and the status is `BAD_REQUEST`. The public code is
        // consumed as the enum constant rather than the bound string because
        // this surface types it as `CoreErrorCode`.
        DomainError::PeerDenied {
            wire_code, message, ..
        } => CoreError {
            code: CoreErrorCode::NotSupported,
            message,
            details: serde_json::Map::from_iter([("wire_code".into(), Value::String(wire_code))]),
            http_status: Some(400),
        },
        DomainError::Busy => CoreError {
            code: CoreErrorCode::Busy,
            message: "busy".into(),
            details: Default::default(),
            http_status: Some(503),
        },
        DomainError::Closing => CoreError {
            code: CoreErrorCode::Closing,
            message: "closing".into(),
            details: Default::default(),
            http_status: Some(503),
        },
        DomainError::Interrupted => CoreError {
            code: CoreErrorCode::Interrupted,
            message: "interrupted".into(),
            details: Default::default(),
            http_status: Some(503),
        },
        DomainError::ActorInput(message) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message,
            details: Default::default(),
            http_status: Some(400),
        },
        // Retained bearer-memory 403 wire shape: the core carries the split
        // resource + reason; render the daemon's combined forbidden message.
        DomainError::ForbiddenReason { resource, reason } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: format!("forbidden: {resource} — {reason}"),
            details: serde_json::Map::from_iter([
                ("resource".into(), Value::String(resource)),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(403),
        },
        // Truthful no-provider/capability 503 (e.g. SOUL synthesis demanded
        // with no capability registry).
        DomainError::ServiceUnavailable(message) => CoreError {
            code: CoreErrorCode::Busy,
            message,
            details: Default::default(),
            http_status: Some(503),
        },
        // Narrative-quality rejection: the daemon's BadRequest
        // `narrative_generation_failed` rides the generated invalid_input
        // vocabulary with the carrier code in details.
        DomainError::NarrativeRejected(message) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "carrier_code".into(),
                Value::String("narrative_generation_failed".into()),
            )]),
            http_status: Some(400),
        },
        // Generic domain conflict (409); the generated vocabulary has no
        // generic `conflict` code yet, so `owner_busy` carries the 409
        // envelope with the message verbatim.
        DomainError::Conflict(message) => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message,
            details: Default::default(),
            http_status: Some(409),
        },
        // Stable actor-family wire conflicts (durable §11.1 codes): the 409
        // rides `owner_busy` with the retained stable code in details.
        DomainError::ActorConflict { code, message } => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message,
            details: serde_json::Map::from_iter([("conflict_code".into(), Value::String(code))]),
            http_status: Some(409),
        },
        DomainError::Internal { category } => CoreError {
            code: CoreErrorCode::Internal,
            message: "internal error".into(),
            details: serde_json::Map::from_iter([(
                "category".into(),
                Value::String(internal_error_bucket(&category).into()),
            )]),
            http_status: Some(500),
        },
    }
}

/// Map a preset/strategy authoring failure to wire JSON.
///
/// Mirrors `From<PresetError> for NexusApiError` in the daemon adapter
/// (`crates/nexus-daemon-runtime/src/api/errors.rs`) variant for variant, so a
/// preset error reads identically whether it reached the caller over HTTP or
/// through this native surface. The mapped `http_status` is the daemon's own
/// status for the same variant — the wire envelope carries it so the TS layer
/// does not re-derive it.
fn wire_core_error_from_preset(error: nexus_core::PresetError) -> CoreError {
    use nexus_core::PresetError;
    match error {
        PresetError::Rejected { code, message } => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "wire_code".into(),
                Value::String(code.clone()),
            )]),
            // `Rejected` is the daemon's `BadRequest`, whose status comes from
            // its own code table: the semantic-validation codes are 422 and
            // everything else 400.
            http_status: Some(preset_rejected_status(&code)),
        },
        PresetError::InvalidInput { field, reason } => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: format!("{reason} (field: {field})"),
            details: serde_json::Map::from_iter([
                ("field".into(), Value::String(field)),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(422),
        },
        PresetError::NotFound(resource) => CoreError {
            code: CoreErrorCode::NotFound,
            message: resource.clone(),
            details: serde_json::Map::from_iter([("resource".into(), Value::String(resource))]),
            http_status: Some(404),
        },
        PresetError::Conflict(message) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "wire_code".into(),
                Value::String("conflict".into()),
            )]),
            http_status: Some(409),
        },
        PresetError::Forbidden { resource, reason } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: reason.clone(),
            details: serde_json::Map::from_iter([
                ("resource".into(), Value::String(resource)),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(403),
        },
        // The inner `code` is deliberately NOT leaked as the public code: the
        // daemon always reports `internal` for this variant.
        PresetError::Internal { code, message } => CoreError {
            code: CoreErrorCode::Internal,
            message: "internal error".into(),
            details: serde_json::Map::from_iter([
                (
                    "bucket".into(),
                    Value::String(internal_error_bucket(&message).into()),
                ),
                ("wire_code".into(), Value::String(code)),
            ]),
            http_status: Some(500),
        },
        PresetError::StrategyConflict(conflict) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: format!("strategy conflict: {}", conflict.conflicting_path),
            details: serde_json::Map::from_iter([
                (
                    "current_revision".into(),
                    Value::from(conflict.current_revision),
                ),
                ("node_id".into(), Value::String(conflict.node_id)),
                (
                    "conflicting_path".into(),
                    Value::String(conflict.conflicting_path),
                ),
                (
                    "recovery_hint".into(),
                    Value::String(conflict.recovery_hint),
                ),
            ]),
            http_status: Some(409),
        },
        PresetError::StrategyValidation(summary) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: "strategy validation failed".into(),
            details: serde_json::Map::from_iter([(
                "validation".into(),
                serde_json::to_value(&summary).unwrap_or_default(),
            )]),
            http_status: Some(422),
        },
    }
}

/// The status the daemon adapter assigns to a coded refusal.
///
/// Mirrors the daemon's own tables so this native surface and the HTTP
/// transport render the same family for the same refusal.
///
/// Returns `i64` rather than the daemon's `u16` because the wire
/// `CoreError.http_status` is `Option<i64>` (generated from the schema); the
/// numbers are identical either way, and the helper exists only to feed that
/// field.
fn coded_wire_status(code: &str) -> i64 {
    match code {
        "conflict" => 409,
        // Workflow control races (v1.195 S0-4/W6/W7): a lost/stale human-wait
        // token and a run that refuses the signal are both CONFLICTS — the
        // durable winner is safe and the losing call must be an exact 409 with
        // its coded detail, never a 400 "bad request" or a 500. The core
        // carries the code; this adapter decides the status
        // (`.mstar/specs/orchestration-engine.md` §human wait).
        "workflow_wait_conflict" | "workflow_state_conflict" => 409,
        "invalid_state"
        | "invalid_transition"
        | "invalid_input"
        | "world_id_required"
        | "invalid_world_id"
        | "world_clear_forbidden"
        | "too_many_findings"
        | "strategy_self_loop"
        | "strategy_transition_duplicate"
        | "compute_fuel_exhausted"
        | "compute_wall_time_exceeded"
        | "compute_memory_cap_exceeded"
        | "compute_module_trapped"
        | "compute_module_error" => 422,
        "policy_blocked" => 403,
        _ => 400,
    }
}

/// The daemon's status table for a `PresetError::Rejected` code.
///
/// `Rejected` renders as the daemon's `BadRequest`, whose public status is 422
/// for the semantic-validation codes and 400 otherwise.
///
/// Returns `i64` for the same reason as [`coded_wire_status`]: the wire
/// `http_status` field is `Option<i64>`.
fn preset_rejected_status(code: &str) -> i64 {
    match code {
        "world_id_required"
        | "invalid_world_id"
        | "world_clear_forbidden"
        | "invalid_transition"
        | "invalid_input"
        | "invalid_state"
        | "too_many_findings"
        | "strategy_self_loop"
        | "strategy_transition_duplicate" => 422,
        _ => 400,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::WorldKbConflictError;

    #[test]
    fn internal_error_is_sanitized() {
        let wire = wire_core_error_from_domain(DomainError::Internal {
            category: "spoke decode: secret".into(),
        });
        assert_eq!(wire.code, CoreErrorCode::Internal);
        assert_eq!(wire.message, "internal error");
        assert_eq!(
            wire.details.get("category").and_then(|v| v.as_str()),
            Some("storage")
        );
    }

    #[test]
    fn peer_denied_keeps_the_public_code_and_the_peer_wire_code() {
        let wire = wire_core_error_from_domain(DomainError::PeerDenied {
            code: "not_supported".into(),
            wire_code: "capability_missing".into(),
            message: "peer does not expose that tool".into(),
        });
        // The public code is the spine's, never the peer's — the peer's own
        // code must not become the classification a client branches on.
        assert_eq!(wire.code, CoreErrorCode::NotSupported);
        assert_eq!(wire.message, "peer does not expose that tool");
        assert_eq!(wire.http_status, Some(400));
        // The peer's code survives verbatim in details, where the daemon's
        // `PeerToolDenied` also carries it.
        assert_eq!(
            wire.details.get("wire_code").and_then(|v| v.as_str()),
            Some("capability_missing")
        );
    }

    #[test]
    fn world_kb_conflict_serializes_structured_details() {
        let wire =
            wire_core_error_from_domain(DomainError::WorldKbConflict(WorldKbConflictError {
                current_version: 2,
                entity_id: "kb_cas".into(),
                conflicting_path: "revision".into(),
                recovery_hint: "refetch graph".into(),
            }));
        assert_eq!(wire.code, CoreErrorCode::WorldKbConflict);
        assert_eq!(
            wire.details.get("current_version").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            wire.details.get("entity_id").and_then(|v| v.as_str()),
            Some("kb_cas")
        );
        let json = serde_json::to_string(&wire).expect("serialize");
        let parsed: CoreError = serde_json::from_str(&json).expect("round trip");
        assert_eq!(parsed.code, CoreErrorCode::WorldKbConflict);
        assert_eq!(
            parsed.details.get("recovery_hint").and_then(|v| v.as_str()),
            Some("refetch graph")
        );
    }
}
