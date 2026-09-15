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
