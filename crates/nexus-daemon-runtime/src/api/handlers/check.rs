//! Check handlers — Daemon HTTP surface for spoke `orchestrate_check`
//! (V1.148 P2; closes V1.146 Non-Goal 5a — the check op is lib-import-only
//! until this route).
//!
//! One route:
//! - `POST /v1/daemon/check` — run spoke `orchestrate_check` over an owned
//!   World; returns the persisted Finding(s) (mental-layer checker pair
//!   since V1.164 P2 T3). Since V1.166 (AR-1) the handler crosses the
//!   world-scoped seam [`orchestrate_check_world_scoped`]: empty
//!   `rule_refs` auto-include the world's `status=active` rules and
//!   foreign-world / embedded rules reject with 400 before any evaluation.
//!
//! ## Response mapping decision (V1.148 P2 architect lock)
//!
//! The architect lock offers two mappings for spoke-level failures: a
//! `200 + CheckResponse body` carrying the spoke `{ "error": ErrorEnvelope }`
//! branch, or the nearest compute/kb pattern (reject codes → 4xx/5xx daemon
//! error-envelope responses via `NexusApiError`).
//!
//! **Chosen: reject codes → 4xx/5xx `NexusApiError`.** Rationale:
//!
//! 1. There is no existing "200 + structured error body" spoke-shaped route
//!    in this crate — `compute_runs` and `world_kb` both surface spoke/orch
//!    failures as `NexusApiError` (4xx/5xx), so the "when present" preference
//!    for a 200-error precedent does not apply.
//! 2. The daemon API error-envelope single-source rule
//!    (`errors.rs` module docs, R-V167P0-QC1-S-AGENTS) says handlers must
//!    return `NexusApiError` so the central `IntoResponse` emits the canonical
//!    `{ success: false, error: {...} }` shape — a hand-built 200 error body
//!    would bypass it.
//! 3. `orchestrate_check` only ever produces the `findings` success branch
//!    (`success_response(json!({"findings": ...}))`) or a `SpokeResult::Reject`
//!    — the `{ "error": ... }` wire branch is unreachable today. The
//!    `check-response.schema.json` still mirrors the spoke `oneOf` for wire
//!    parity (and the handler round-trips any future `error`-branch value into
//!    the generated DTO defensively).
//!
//! Mapping table (mirrors `world_kb::map_upsert_reject` for the reachable set):
//!
//! | `SpokeRejectCode` | `NexusApiError` |
//! |-------------------|-----------------|
//! | `InvalidInput` (spoke scope/finding wire problems; AR-1 wrapper rejects — embedded rules, foreign-world `rule_refs`) | `InvalidInput` (400) |
//! | `InternalError` (`RuleQueryPort` / `ScopeQueryPort` / `FindingPort` storage failures; AR-1 storage errors) | `Internal` (500) |
//! | other / defensive | `Internal` (500, `SPOKE_ORCHESTRATOR_REJECT`) |
//!
//! ## Auto-apply constraint
//!
//! This route does **not** mutate `KnowledgeEntry` / `Relation` / manuscript.
//! Spoke `orchestrate_check` calls `FindingPort::put_findings` — persisting
//! findings is the check outcome, not "apply fix". No promote/upsert from
//! this route.
//!
//! ## Checker callback
//!
//! V1.148 P2 shipped `run_checker` as a baseline no-op evaluator
//! (`Ok(Vec::new())` → zero findings). **V1.164 P2 T3 replaces it** with
//! the mental-layer checker pair (`check::mental::run_check`): the callback
//! reads the scoped entry `modules.belief` rows (Task 1 dialect) and the
//! scoped `TimelineEvent.modules.observation` metadata (P1 passthrough)
//! from its `CheckRunInput` and emits `stale_belief_drift` (warning) /
//! `dramatic_irony_asymmetry` (info) findings. The callback input already
//! carries the scoped events, so the checker never touches the store.
//!
//! V1.166 AR-1 (world scoping): rules are **world-scoped before
//! orchestration** by [`orchestrate_check_world_scoped`] — empty
//! `rule_refs` expand to the check world's `status=active` rule ids at the
//! adapter boundary, foreign-world refs reject the whole check (PD-3, fail
//! closed), and embedded `rules` are refused (not an authoring path).
//! Spoke `orchestrate_check` still resolves the (now world-owned) refs via
//! `RuleQueryPort`.
//!
//! # Errors
//!
//! Returns 401 when no active creator can be read from config (tier2
//! middleware normally rejects earlier with 409), 403 when the World is not
//! owned by the active creator, 400 when `scope.scope_id` does not match
//! `world_id` or the request body does not map onto the spoke `CheckRequest`
//! wire shape, 400 for spoke `InvalidInput` rejects, and 500 for storage /
//! internal failures.
//!
//! # Panics
//!
//! None: `NexusAdapter` port methods are natively `async fn` (spoke-operations
//! 0.9.1 surface, V1.153 P0 T2) and run on the handler's runtime.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::generated::daemon_api::check::{
    check_request::CheckRequest as NexusCheckRequest,
    check_response::CheckResponse as NexusCheckResponse,
};
use nexus_local_db::narrative_write;
use nexus_spoke_adapter::{
    orchestrate_check_world_scoped, CheckRequest, NexusAdapter, SpokeReject, SpokeRejectCode,
    SpokeResult,
};
use serde_json::json;

// ── POST /v1/daemon/check ────────────────────────────────────────────────

/// Run spoke `orchestrate_check` over an owned World (V1.148 P2).
///
/// Guard order (plan lock): tier2 middleware (API key + active creator) →
/// world ownership (`is_world_owned`, 403) → scope consistency
/// (`scope.scope_id == world_id`, 400) → spoke `CheckRequest` wire mapping
/// (400) → `orchestrate_check_world_scoped` (V1.166 AR-1 seam: embedded-rules
/// reject / auto-include / foreign-id reject, then spoke orchestration with
/// the mental-layer checker — V1.164 P2 T3) → response mapping (see module
/// docs).
#[allow(clippy::missing_errors_doc)]
pub async fn run_check(
    State(state): State<WorkspaceState>,
    Json(req): Json<NexusCheckRequest>,
) -> Result<Json<NexusCheckResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Ownership gate (compute_runs pattern): a foreign World never leaks
    // check behavior — 403, not 404, so world existence stays unobservable
    // to other creators.
    let owned = narrative_write::is_world_owned(pool, &creator_id, req.world_id.as_str())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {}", req.world_id.as_str()),
            reason: "you do not own this world".to_string(),
        });
    }

    // Scope consistency gate (plan lock): the spoke scope selector must be
    // anchored to the owned World. 400 per the architect lock (not 422).
    if req.scope.scope_id != req.world_id.as_str() {
        return Err(NexusApiError::InvalidInput {
            field: "scope.scope_id".to_string(),
            reason: format!(
                "scope.scope_id '{}' does not match world_id '{}'",
                req.scope.scope_id,
                req.world_id.as_str()
            ),
        });
    }

    // Map the daemon DTO onto the spoke `CheckRequest` wire shape via JSON
    // round-trip (mirrors `world_kb::build_spoke_upsert_request`; the adapter
    // is the only import boundary — no direct spoke-operations /
    // spoke-schemas dependency in this crate). `world_id` is intentionally not
    // carried: spoke's scope.scope_id is the world selector (already validated
    // above). Any DTO value that does not fit the spoke wire (e.g. a rule
    // object missing required fields, or an extensions key outside
    // ^[a-z][a-z0-9_-]*$) rejects here as 400 — honest boundary validation.
    let wire = json!({
        "scope": req.scope,
        "rule_refs": req.rule_refs,
        "rules": req.rules,
        "checker_kinds": req.checker_kinds,
        "extensions": req.extensions,
    });
    let check_req: CheckRequest =
        serde_json::from_value(wire).map_err(|e| NexusApiError::InvalidInput {
            field: "check request".to_string(),
            reason: format!("request does not map onto the spoke CheckRequest wire shape: {e}"),
        })?;

    // Mental-layer checker (V1.164 P2 T3): the callback input carries the
    // scoped entries + events (`CheckRunInput`), so the checker classifies
    // purely from the input (no store reads) — it emits the
    // `stale_belief_drift` / `dramatic_irony_asymmetry` pair and stamps
    // `extensions.nexus.world_id` (the AR-2 routing key) + `creator_id`
    // (provenance). FindingPort routes world-scoped findings onto
    // `world_findings` (DR-68, AR-2).
    let adapter = NexusAdapter::new(pool.clone());
    // V1.166 AR-1 — world-scoped seam: `orchestrate_check_world_scoped`
    // pre-expands empty `rule_refs` to this world's `status=active` rules
    // and fail-closes on embedded rules / foreign-world refs BEFORE spoke
    // orchestration (a reject persists nothing, evaluates nothing). The
    // spoke `orchestrate_check` itself stays world-agnostic.
    let result =
        orchestrate_check_world_scoped(&adapter, req.world_id.as_str(), check_req, |input| {
            crate::check::mental::run_check(&input, &creator_id)
        })
        .await;

    match result {
        // Success branch: findings (possibly empty). Round-trip through JSON
        // into the generated daemon DTO (mirrors map_upsert_response's wire
        // round-trip; validates the spoke payload against the wire contract
        // at the boundary).
        SpokeResult::Ok(spoke_resp) => {
            let wire = serde_json::to_value(&spoke_resp).map_err(|e| NexusApiError::Internal {
                code: "SPOKE_RESPONSE_DECODE".to_string(),
                message: format!("orchestrate_check returned a non-serializable response: {e}"),
            })?;
            let resp: NexusCheckResponse =
                serde_json::from_value(wire).map_err(|e| NexusApiError::Internal {
                    code: "SPOKE_RESPONSE_DECODE".to_string(),
                    message: format!(
                        "orchestrate_check response did not match the daemon CheckResponse \
                         wire shape: {e}"
                    ),
                })?;
            Ok(Json(resp))
        }
        SpokeResult::Reject(reject) => Err(map_check_reject(reject)),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Map a spoke `SpokeReject` to the daemon error envelope (module docs table).
fn map_check_reject(reject: SpokeReject) -> NexusApiError {
    match reject.code {
        // Client-input problems surfaced by the spoke layer (scope wire
        // conversion, finding shape validation) → 400 (S-001 explicit-400
        // contract, mirrors world_kb map_upsert_reject).
        SpokeRejectCode::InvalidInput => NexusApiError::InvalidInput {
            field: "check".to_string(),
            reason: reject.message,
        },
        // Port / storage failures (RuleQueryPort, ScopeQueryPort,
        // FindingPort) → 500.
        SpokeRejectCode::InternalError => NexusApiError::Internal {
            code: "INTERNAL_ERROR".to_string(),
            message: format!("orchestrate_check internal error: {}", reject.message),
        },
        // Defensive: no other code is reachable on this path today
        // (orchestrate_check's reject producers are the ports above and
        // wire_convert / success_response, both InvalidInput).
        _ => NexusApiError::Internal {
            code: "SPOKE_ORCHESTRATOR_REJECT".to_string(),
            message: format!(
                "orchestrate_check rejected: {}: {}",
                reject.code, reject.message
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_spoke_adapter::{SpokeReject, SpokeRejectCode};
    use serde_json::{json, Map};

    fn reject(code: SpokeRejectCode, message: &str) -> SpokeReject {
        SpokeReject {
            code,
            message: message.to_string(),
            details: Some(Map::from_iter([("key".to_string(), json!("value"))])),
        }
    }

    /// The mapping table (module docs) is the observable HTTP contract of the
    /// reject path — pin it so a future remap is a conscious decision.
    #[test]
    fn reject_mapping_table() {
        let invalid_input = map_check_reject(reject(SpokeRejectCode::InvalidInput, "bad scope"));
        assert_eq!(
            invalid_input.status_code(),
            400,
            "InvalidInput must stay 4xx"
        );

        let internal = map_check_reject(reject(SpokeRejectCode::InternalError, "db down"));
        assert_eq!(internal.status_code(), 500, "InternalError must stay 500");
        assert_eq!(internal.error_code(), "internal");

        // Defensive fallback: codes unreachable today still fail closed.
        let stale = map_check_reject(reject(SpokeRejectCode::RevisionConflict, "n/a"));
        assert_eq!(
            stale.status_code(),
            500,
            "unreachable reject codes must fail closed"
        );
    }
}
