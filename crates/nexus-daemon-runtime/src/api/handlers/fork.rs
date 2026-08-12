//! Fork handlers — daemon HTTP surface for the `nexus.fork.create`
//! capability (V1.162 P1 T2, plan
//! `2026-08-12-v1.162-p1-fork-backend-foundation`).
//!
//! One route:
//! - `POST /v1/daemon/worlds/:world_id/forks` — create a local timeline
//!   fork (a new `branch_id` within an owned world diverging from a
//!   fork-point event on the parent branch).
//!
//! # Delegation model (plan lock — thin delegate)
//!
//! The handler is a **thin delegate** to the `fork.create` capability path
//! (`nexus_orchestration::capability::builtins::ForkCreate`): the capability
//! owns the creator-ownership admission gate (`ensure_world_owned`), the
//! fork-point validation (event exists on the stated parent branch within
//! `world_id`), the `fbk_<uuid12>` branch allocation, and the canon
//! `fork_created` marker write carrying `fork_lineage` in
//! `extensions_nexus_json` (carrier B, T1). This handler does **not**
//! reimplement any of that logic — it parses the generated wire DTO,
//! resolves the active creator, enforces the daemon ownership guard *first*,
//! delegates, and maps capability errors onto the daemon error envelope.
//!
//! # Authz order (plan lock)
//!
//! 1. `require_world_owner` first → 403 for foreign worlds (before any
//!    fork-point read; no cross-world lineage leak).
//! 2. Fork-point validation (capability guard) → 422 on bad / non-existent
//!    fork-point.
//! 3. Success → 200 with `branch_id` (starts `fbk_`) + parent + fork-point +
//!    `created_at`.
//!
//! # Errors
//!
//! Returns 401 when no active creator can be read from config (tier2
//! middleware normally rejects earlier), 403 when the World is not owned by
//! the active creator, 404 when no `narrative_worlds` row exists for the
//! `world_id` (`require_world_owner`; pre-existing daemon convention — no
//! fork-point read on the 404 path), 422 when the fork-point event is not on
//! the stated parent branch / non-existent (capability `InputInvalid`), 503
//! when the capability worker is unavailable, and 500 for storage / internal
//! failures. Malformed JSON bodies are rejected by the axum `Json` extractor
//! (400 `JsonSyntaxError`); valid JSON that fails the generated DTO (missing
//! fields, `label` outside the schema's 1–200 char constraint) is rejected at
//! the same extractor boundary as 422 `JsonDataError` (axum 0.7 default).

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::api::handlers::world_kb_guards::require_world_owner;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::{CreateForkRequest, CreateForkResponse};
use nexus_orchestration::capability::Capability;
use serde_json::json;

// ── POST /v1/daemon/worlds/:world_id/forks ──────────────────────────────

/// Create a local timeline fork (V1.162 P1 T2).
///
/// Thin delegate to the `nexus.fork.create` capability: parse the generated
/// `CreateForkRequest`, resolve the active creator, run the daemon
/// `require_world_owner` guard FIRST (403 foreign world / 404 missing world,
/// before any fork-point read), then
/// call `ForkCreate::run` with the wire fields + creator context. The
/// capability re-checks ownership (its admission gate), validates the
/// fork-point (422), allocates the `fbk_` branch id, and appends the canon
/// `fork_created` marker with `fork_lineage` (T1 carrier B). The capability
/// output is round-tripped into the generated `CreateForkResponse`.
#[allow(clippy::missing_errors_doc)]
pub async fn create_fork(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<CreateForkRequest>,
) -> Result<Json<CreateForkResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    // Active creator from daemon config — parity with `create_world` in the
    // same `world_kb_routes()` mount (tier2 already gates active creator).
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Authz order (plan lock): ownership FIRST — 403 before any fork-point
    // read, so a foreign world cannot learn whether a fork-point exists.
    require_world_owner(pool, &world_id, &creator_id).await?;

    // Thin delegate: construct the capability over the daemon pool and call
    // its `run` with the wire DTO fields + resolved creator context. The
    // capability owns admission, fork-point validation, branch allocation,
    // and the canon marker write — single source of truth (plan §6 risk
    // mitigation: route delegates, never reimplements).
    let cap = nexus_orchestration::capability::builtins::ForkCreate::with_pool(pool.clone());
    let out = cap
        .run(json!({
            "world_id": world_id,
            "creator_id": creator_id,
            "parent_branch_id": req.parent_branch_id,
            "forked_from_event_id": req.forked_from_event_id,
            "label": req.label.map(String::from),
        }))
        .await
        .map_err(map_fork_capability_error)?;

    let resp: CreateForkResponse =
        serde_json::from_value(out).map_err(|e| NexusApiError::Internal {
            code: "FORK_CREATE_RESPONSE_DECODE".to_string(),
            message: format!("fork.create output did not match CreateForkResponse: {e}"),
        })?;

    tracing::info!(
        target: "worlds.fork",
        world_id = %world_id,
        branch_id = %resp.branch_id,
        parent_branch = %resp.parent_branch_id,
        "local timeline fork created"
    );

    Ok(Json(resp))
}

// ─── Error mapping ───────────────────────────────────────────────────────

/// Map a `fork.create` capability error onto the daemon error envelope.
///
/// | `CapabilityError` | `NexusApiError` |
/// |---|---|
/// | `Forbidden` (defensive — the daemon guard already ran) | 403 |
/// | `InputInvalid` (bad / non-existent fork-point) | 422 (`invalid_input`) |
/// | `WorkerUnavailable` | 503 |
/// | `Internal` / any other | 500 |
fn map_fork_capability_error(e: nexus_orchestration::capability::CapabilityError) -> NexusApiError {
    use nexus_orchestration::capability::CapabilityError;
    match e {
        CapabilityError::Forbidden(reason) => NexusApiError::Forbidden {
            resource: "world".to_string(),
            reason,
        },
        // Stable field-scoped message — never echo the capability's reason
        // verbatim (it includes caller-supplied fork-point/branch/world ids).
        CapabilityError::InputInvalid(_reason) => NexusApiError::InputValidationFailed {
            details: json!({ "fork_point": "fork point not found on parent branch" }),
        },
        CapabilityError::WorkerUnavailable => NexusApiError::ServiceUnavailable {
            message: "fork.create capability worker unavailable".to_string(),
        },
        other => NexusApiError::Internal {
            code: "FORK_CREATE_FAILED".to_string(),
            message: other.to_string(),
        },
    }
}
