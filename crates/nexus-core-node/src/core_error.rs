//! Map domain errors to generated wire [`CoreError`] JSON for N-API consumers.

use napi::bindgen_prelude::Error;
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
            details: serde_json::Map::from_iter([
                ("resource".into(), Value::String(resource)),
            ]),
            http_status: Some(403),
        },
        DomainError::NotFound { resource } => CoreError {
            code: CoreErrorCode::NotFound,
            message: format!("not found: {resource}"),
            details: serde_json::Map::from_iter([
                ("resource".into(), Value::String(resource)),
            ]),
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
        DomainError::Internal { category } => CoreError {
            code: CoreErrorCode::Internal,
            message: if category.starts_with("config_load:")
                || category.starts_with("database_error:")
            {
                "internal: configuration or database error".into()
            } else {
                format!("internal: {category}")
            },
            details: Default::default(),
            http_status: Some(500),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::WorldKbConflictError;

    #[test]
    fn world_kb_conflict_serializes_structured_details() {
        let wire = wire_core_error_from_domain(DomainError::WorldKbConflict(
            WorldKbConflictError {
                current_version: 2,
                entity_id: "kb_cas".into(),
                conflicting_path: "revision".into(),
                recovery_hint: "refetch graph".into(),
            },
        ));
        assert_eq!(wire.code, CoreErrorCode::WorldKbConflict);
        assert_eq!(wire.details.get("current_version").and_then(|v| v.as_u64()), Some(2));
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
