//! Map domain errors to generated wire [`CoreError`] JSON for N-API consumers.
// note: an N-API consumer parses the rejection reason as JSON, so classification
// rides `code`/`details`, never `message`.

use napi::bindgen_prelude::Error;
use nexus_agent_host::core::readiness::safe_provider_message;
use nexus_agent_host::HostError;
use nexus_contracts::{CoreError, CoreErrorCode};
use nexus_core::{CoreError as DomainError, MemoryProviderRefusal};
use serde_json::Value;

/// note: serialize a wire [`CoreError`] as the N-API rejection reason.
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

/// note: map a native-core domain error to wire JSON for TS `JSON.parse(message)`.
pub fn napi_error_from_domain(err: DomainError) -> Error {
    napi_error_from_wire(wire_core_error_from_domain(err))
}

/// note: parse an `open_core` rejection reason into a typed N-API error.
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

/// note: serialize a wire [`CoreError`] for `open_core` rejection reasons.
pub fn open_reason_from_wire(err: CoreError) -> String {
    serde_json::to_string(&err).unwrap_or_else(|_| "open failed".to_string())
}

/// note: serialize a domain error for `open_core` rejection reasons.
pub fn open_reason_from_domain(err: DomainError) -> String {
    open_reason_from_wire(wire_core_error_from_domain(err))
}

/// note: serialize a host error for `open_core` rejection reasons.
pub fn open_reason_from_host(err: HostError) -> String {
    open_reason_from_wire(wire_core_error_from_host(err))
}

/// note: serialize an invalid-input wire error for host-query style rejections.
pub fn open_reason_invalid_input(message: impl Into<String>) -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::InvalidInput,
        message: message.into(),
        details: Default::default(),
        http_status: Some(400),
    })
}

/// note: serialize a not-found wire error for host-query style rejections.
pub fn open_reason_not_found(message: impl Into<String>) -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::NotFound,
        message: message.into(),
        details: Default::default(),
        http_status: Some(404),
    })
}

/// note: serialize a sanitized internal wire error for host-query style rejections.
pub fn open_reason_internal() -> String {
    open_reason_from_wire(CoreError {
        code: CoreErrorCode::Internal,
        message: "internal error".into(),
        details: Default::default(),
        http_status: Some(500),
    })
}

/// note: the typed `details.category` naming the absent embedded Host authority.
///
/// It is the ONLY stable discriminator for that state: the public code/status
/// stay `invalid_input`/`400` (an unattached authority is truthful degraded
/// readiness, not a client fault), so consumers must not read the human
/// `message` — a wording change would otherwise silently turn the designed
/// absence fallback into a hard failure.
pub const HOST_NOT_STARTED_CATEGORY: &str = "host_not_started";

/// note: the ONE wire envelope for the absent embedded Host authority.
///
/// The open/close sequence holds the attached authority pair (journal core +
/// Host authority) in one lifetime: both slots are published together at the
/// end of a successful open and released together by close, so either slot
/// missing means the scoped Host authority is not there.
pub fn host_not_started_wire() -> CoreError {
    CoreError {
        code: CoreErrorCode::InvalidInput,
        message: "host not started".into(),
        details: serde_json::Map::from_iter([(
            "category".into(),
            Value::String(HOST_NOT_STARTED_CATEGORY.into()),
        )]),
        http_status: Some(400),
    }
}

/// note: serialize the absent-authority envelope for the string-reason entry points.
pub fn open_reason_host_not_started() -> String {
    open_reason_from_wire(host_not_started_wire())
}

/// note: reject an absent authority with the typed envelope for N-API entry points.
///
/// The raw `Error::from_reason("<text>")` shape is deliberately not used here:
/// a bare reason cannot carry the typed category, and the TS boundary would
/// read it as an unmapped `internal` fault instead of the classified absence.
pub fn napi_error_host_not_started() -> Error {
    napi_error_from_wire(host_not_started_wire())
}

/// note: the typed `details.category` naming a principal handle that does not encode
/// this open's active principal at this generation (V1.206 P2 site 5).
///
/// Mirrors [`HOST_NOT_STARTED_CATEGORY`]: the public code/status stay
/// `invalid_input`/`400` (the retained external shape of the former
/// `Error::from_reason("invalid principal handle")` bare reason), so the human
/// `message` must not be read for classification — a wording change would
/// otherwise silently turn a stale/forged handle refusal into an unmapped
/// fault.
pub const INVALID_PRINCIPAL_HANDLE_CATEGORY: &str = "invalid_principal_handle";

/// note: the ONE wire envelope for a principal handle that fails the shared
/// verification in `verified_principal`.
pub fn invalid_principal_handle_wire() -> CoreError {
    CoreError {
        code: CoreErrorCode::InvalidInput,
        message: "invalid principal handle".into(),
        details: serde_json::Map::from_iter([(
            "category".into(),
            Value::String(INVALID_PRINCIPAL_HANDLE_CATEGORY.into()),
        )]),
        http_status: Some(400),
    }
}

/// note: reject an invalid principal handle with the typed envelope for N-API entry
/// points.
///
/// The raw `Error::from_reason("<text>")` shape is deliberately not used here:
/// a bare reason cannot carry the typed category, so the TS boundary would
/// classify the refusal by message text.
pub fn napi_error_invalid_principal_handle() -> Error {
    napi_error_from_wire(invalid_principal_handle_wire())
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

/// note: map a host error to the generated wire envelope without leaking raw diagnostics.
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

/// note: typed uninitialized denial shared by service-only entrypoints.
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
        // note: world-owner scoping denial renders as 403 `forbidden` with
        // resource `world {world_id}` and the reason split into `details`.
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
        // note: outline OCC conflicts keep their structured details on the wire.
        DomainError::OutlineConflict(details) => CoreError {
            code: CoreErrorCode::OutlineConflict,
            message: "outline conflict".into(),
            details: serde_json::Map::from_iter([
                (
                    "current_revision".into(),
                    serde_json::Number::from(details.current_revision).into(),
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
        // note: outline validation keeps its structured summary on the wire.
        DomainError::OutlineValidation(summary) => CoreError {
            code: CoreErrorCode::OutlineValidationFailed,
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
        // note: input validation keeps the core's structured per-entry object
        // (`{"invalid_entries": […]}`) verbatim under `details` on the 422
        // `invalid_input` envelope.
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
            code: coded_core_error_code(&code),
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "wire_code".into(),
                Value::String(code.clone()),
            )]),
            http_status: Some(coded_wire_status(&code)),
        },
        // note: a peer-side tool refusal — the spine's public code is always
        // `not_supported` (the sole raise site, `capabilities.rs`, sets it
        // literally), the peer's own lowercase code rides `details.wire_code`
        // verbatim (never re-parsed from the message), and the status is 400.
        // The public code is consumed as the enum constant rather than the
        // bound string because this surface types it as `CoreErrorCode`.
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
        // note: workspace-creation exclusivity refusal — the domain names the
        // exact collision, the stable 409 `workspace_conflict` code carries it.
        DomainError::WorkspaceConflict(message) => CoreError {
            code: CoreErrorCode::WorkspaceConflict,
            message: format!("workspace conflict: {message}"),
            details: Default::default(),
            http_status: Some(409),
        },
        // note: retained bearer-memory 403 shape — the core carries resource
        // and reason split; the message joins them.
        DomainError::ForbiddenReason { resource, reason } => CoreError {
            code: CoreErrorCode::Forbidden,
            message: format!("forbidden: {resource} — {reason}"),
            details: serde_json::Map::from_iter([
                ("resource".into(), Value::String(resource)),
                ("reason".into(), Value::String(reason)),
            ]),
            http_status: Some(403),
        },
        // note: truthful no-provider/capability 503 (e.g. SOUL synthesis
        // demanded with no capability registry).
        DomainError::ServiceUnavailable(message) => CoreError {
            code: CoreErrorCode::ServiceUnavailable,
            message,
            details: Default::default(),
            http_status: Some(503),
        },
        // note: narrative-quality rejection — `narrative_generation_failed`
        // rides the generated `invalid_input` vocabulary as the `carrier_code`
        // detail.
        DomainError::NarrativeRejected(message) => CoreError {
            code: CoreErrorCode::InvalidInput,
            message: message.clone(),
            details: serde_json::Map::from_iter([(
                "carrier_code".into(),
                Value::String("narrative_generation_failed".into()),
            )]),
            http_status: Some(400),
        },
        // why: the generated vocabulary has no generic `conflict` code, so
        // `owner_busy` carries the 409 envelope with the message verbatim.
        DomainError::Conflict(message) => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message,
            details: Default::default(),
            http_status: Some(409),
        },
        // note: stable actor-family conflicts (durable §11.1 codes) — the 409
        // rides `owner_busy`, the retained stable code travels as
        // `details.conflict_code`.
        DomainError::ActorConflict { code, message } => CoreError {
            code: CoreErrorCode::OwnerBusy,
            message,
            details: serde_json::Map::from_iter([("conflict_code".into(), Value::String(code))]),
            http_status: Some(409),
        },
        // why: a stale compose base is a compare-and-swap conflict — the
        // dedicated `strategy_candidate_conflict` code on an exact 409, with
        // the plan's full Q2 detail set (base/current revision, conflicting
        // path, recovery hint).
        DomainError::StrategyCandidateConflict(conflict) => CoreError {
            code: CoreErrorCode::StrategyCandidateConflict,
            message: format!(
                "Candidate changed before this edit. Reload revision {} and reapply the change.",
                conflict.current_revision
            ),
            details: serde_json::Map::from_iter([
                ("strategy_id".into(), Value::String(conflict.strategy_id)),
                (
                    "base_revision".into(),
                    serde_json::Number::from(conflict.base_revision).into(),
                ),
                (
                    "current_revision".into(),
                    serde_json::Number::from(conflict.current_revision).into(),
                ),
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
        // why: a repeat approve freezes nothing — the snapshot that already
        // froze this content is the answer, never a second row; the dedicated
        // 409 keeps it distinct from the stale-compose-base conflict above.
        DomainError::StrategyAlreadyApproved(conflict) => CoreError {
            code: CoreErrorCode::StrategyAlreadyApproved,
            message: format!(
                "This exact content is already approved as snapshot {}. Edit the candidate before approving again.",
                conflict.approval_id
            ),
            details: serde_json::Map::from_iter([
                ("strategy_id".into(), Value::String(conflict.strategy_id)),
                ("content_hash".into(), Value::String(conflict.content_hash)),
                ("approval_id".into(), Value::String(conflict.approval_id)),
                (
                    "candidate_revision".into(),
                    serde_json::Number::from(conflict.candidate_revision).into(),
                ),
            ]),
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
        // safety: a remote memory-provider refusal never forwards the peer's
        // message or extra details — only the whitelisted effective memory
        // code, an allowlisted peer wire code and the effective retry class
        // ride `details`, so a caller can still tell an uncertain outcome from
        // a clean failure (spec §3.1).
        DomainError::MemoryProviderRefused {
            code,
            wire_code,
            retry_class,
            family,
            conflict_code,
        } => {
            let mut details = serde_json::Map::new();
            details.insert("memory_code".into(), Value::String(code.clone()));
            if let Some(wire_code) = wire_code {
                details.insert("wire_code".into(), Value::String(wire_code));
            }
            details.insert("retry_class".into(), Value::String(retry_class));
            match conflict_code {
                // note: a proven definite memory conflict keeps the retained
                // consumer-visible `conflict_code`/409 envelope the local
                // `ActorConflict` renders, with static text.
                Some(conflict_code) => {
                    details.insert("conflict_code".into(), Value::String(conflict_code));
                    CoreError {
                        code: CoreErrorCode::OwnerBusy,
                        message: "memory revision mismatch".into(),
                        details,
                        http_status: Some(409),
                    }
                }
                None => {
                    let (wire_code_name, http_status) = match family {
                        MemoryProviderRefusal::NotFound => (CoreErrorCode::NotFound, 404),
                        MemoryProviderRefusal::Forbidden => (CoreErrorCode::Forbidden, 403),
                        MemoryProviderRefusal::InvalidInput => (CoreErrorCode::InvalidInput, 400),
                        MemoryProviderRefusal::Conflict => (CoreErrorCode::OwnerBusy, 409),
                        MemoryProviderRefusal::Unavailable => (CoreErrorCode::Busy, 503),
                    };
                    CoreError {
                        code: wire_code_name,
                        message: format!("memory provider refused: {code}"),
                        details,
                        http_status: Some(http_status),
                    }
                }
            }
        }
    }
}

/// note: map a preset/strategy authoring failure to wire JSON.
///
/// Each variant carries the `http_status` the HTTP transport publishes for the
/// same refusal, so the service transport (`apps/nexus-service/src/errors.ts`)
/// honours the envelope instead of re-deriving the status from the code.
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
            // note: `Rejected` is a `BadRequest` — `preset_rejected_status`
            // assigns 422 to the semantic-validation codes and 400 otherwise.
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
        // why: the inner `code` is deliberately not the public code — this
        // variant always reports `internal`; the real code stays in
        // `details.wire_code` for the operator.
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

fn coded_core_error_code(code: &str) -> CoreErrorCode {
    match code {
        "policy_blocked" => CoreErrorCode::PolicyBlocked,
        "service_unavailable" => CoreErrorCode::ServiceUnavailable,
        "compute_fuel_exhausted" => CoreErrorCode::ComputeFuelExhausted,
        "compute_wall_time_exceeded" => CoreErrorCode::ComputeWallTimeExceeded,
        "compute_memory_cap_exceeded" => CoreErrorCode::ComputeMemoryCapExceeded,
        "compute_module_trapped" => CoreErrorCode::ComputeModuleTrapped,
        "compute_module_error" => CoreErrorCode::ComputeModuleError,
        "outline_conflict" => CoreErrorCode::OutlineConflict,
        "outline_validation_failed" => CoreErrorCode::OutlineValidationFailed,
        _ => CoreErrorCode::InvalidInput,
    }
}

/// note: the status a coded refusal publishes, so this surface and the HTTP
/// transport render the same family for the same refusal.
///
/// `i64` rather than `u16` because the wire `CoreError.http_status` is
/// `Option<i64>` (generated from the schema); the numbers are identical, and
/// the helper exists only to feed that field.
fn coded_wire_status(code: &str) -> i64 {
    match code {
        "conflict" => 409,
        // Workflow control races (v1.195 S0-4/W6/W7): a lost/stale human-wait
        // token and a run that refuses the signal are both CONFLICTS — the
        // durable winner is safe and the losing call must be an exact 409 with
        // its coded detail, never a 400 "bad request" or a 500. The core
        // carries the code; this adapter decides the status
        // (`docs/specs/orchestration/orchestration-engine.md` §human wait).
        "workflow_wait_conflict" | "workflow_state_conflict" => 409,
        // why: the durable-operation refusals are conflicts —
        // `operation_id_conflict` is the first-writer-wins ownership refusal
        // (§B.1) and `uncertain` is the "no terminal receipt, never retry
        // blindly" answer (§B.3 item 4); each must be an exact 409 with its
        // coded detail, never the 400 fallback (spec §B.1/§B.3/§C, v1.207 P3).
        "operation_id_conflict" | "uncertain" | "operation_in_progress" => 409,
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
        | "compute_module_error"
        | "outline_validation_failed" => 422,
        "policy_blocked" => 403,
        "service_unavailable" => 503,
        "outline_conflict" => 409,
        _ => 400,
    }
}

/// note: the status table for a `PresetError::Rejected` code — 422 for the
/// semantic-validation codes, 400 otherwise (the `BadRequest` family).
///
/// `i64` for the same reason as [`coded_wire_status`]: the wire `http_status`
/// field is `Option<i64>`.
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
    fn policy_blocked_coded_refusal_uses_its_wire_code() {
        let wire = wire_core_error_from_domain(DomainError::Coded {
            code: "policy_blocked".into(),
            message: "capability denied by policy".into(),
        });

        assert_eq!(
            serde_json::to_value(&wire)
                .expect("serialize")
                .get("code")
                .and_then(Value::as_str),
            Some("policy_blocked")
        );
    }

    #[test]
    fn invalid_principal_handle_wire_preserves_the_external_shape() {
        let wire = invalid_principal_handle_wire();
        assert_eq!(wire.code, CoreErrorCode::InvalidInput);
        assert_eq!(wire.message, "invalid principal handle");
        assert_eq!(wire.http_status, Some(400));
        assert_eq!(
            wire.details.get("category").and_then(Value::as_str),
            Some(INVALID_PRINCIPAL_HANDLE_CATEGORY)
        );
    }

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
        // invariant: the public code is the spine's, never the peer's — the
        // peer's own code must not become the classification a client branches on.
        assert_eq!(wire.code, CoreErrorCode::NotSupported);
        assert_eq!(wire.message, "peer does not expose that tool");
        assert_eq!(wire.http_status, Some(400));
        // note: the peer's code survives verbatim in `details.wire_code`.
        assert_eq!(
            wire.details.get("wire_code").and_then(|v| v.as_str()),
            Some("capability_missing")
        );
    }

    /// note: the frozen durable-operation refusal codes render an exact 409
    /// with their code preserved in `details.wire_code`, never the 400
    /// `invalid_input` fallback (spec §B.1/§B.3/§C, v1.207 P3).
    #[test]
    fn durable_operation_refusals_are_conflicts() {
        for code in [
            "operation_id_conflict",
            "uncertain",
            "operation_in_progress",
        ] {
            let wire = wire_core_error_from_domain(DomainError::Coded {
                code: code.into(),
                message: format!("{code} from the receipt store"),
            });
            assert_eq!(
                wire.http_status,
                Some(409),
                "{code} must be a conflict, not a 400 fallback"
            );
            assert_eq!(
                wire.details.get("wire_code").and_then(Value::as_str),
                Some(code),
                "{code} travels verbatim in details.wire_code"
            );
        }
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
