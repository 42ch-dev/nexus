//! Moment Directive route — `POST /v1/daemon/moment-directive` (V1.151 P0,
//! DF-76).
//!
//! One route: set / show / clear the active Moment Directive for a scope the
//! active creator owns. Thin HTTP wrapper over the existing
//! `nexus_local_db::moment_directive` functions (`set_active` /
//! `replace_active` / `get_active_for_work` / `get_active_for_world` /
//! `clear`) — directive persistence is never re-implemented here.
//!
//! ## Surface (spec §5, architect lock H5)
//!
//! - `set` validates exactly like the CLI `handle_set`
//!   (`apps/nexus42/.../moment_directive.rs`): non-empty body after trim,
//!   exactly one TTL kind with `ttl_remaining >= 1` (wire name per the spec
//!   §5 H5 lock, W-3 / QC1-F-001), known `insert_depth`, explicit
//!   `replace` when a directive is already active in the scope (no silent
//!   overwrite — the unique partial index
//!   `moment_directives_one_active_per_scope` enforces it). Returns the
//!   inserted row.
//! - `show` resolves the **effective** directive for the scope (spec §3.2,
//!   Work-wins / World-override — mirrors the CLI
//!   `resolve_effective_for_show`, W-2 / QC3): a Work scope returns the
//!   Work's own directive, else the bound World's override; a World scope
//!   returns the World override itself. Returns the row **incl. body** —
//!   this is the author surface (DF-76), not the inspector packet (AC-I3:
//!   the packet's `moment_directive` section stays status-only).
//!   No effective directive → `{}` (mirrors the CLI's "No active Moment
//!   Directive for this scope." success message).
//! - `clear` soft-deletes the active row (row retained for DF-76
//!   inspection). Returns `{}`.
//!
//! ## Ownership (HARD)
//!
//! Mounted in tier2 (`require_api_key` + `require_active_creator`). The
//! handler additionally verifies the scope `id` belongs to the active
//! creator: Work scope via `works::get_work` (the query is creator-scoped —
//! `Ok(None)` ⇒ not owned), World scope via `narrative_write::is_world_owned`.
//! A foreign/nonexistent scope is 403, not 404, so scope existence stays
//! unobservable to other creators (mirrors `inspect_moment` / `run_check`).
//!
//! # Errors
//!
//! Returns 401 when no active creator can be read from config (tier2
//! middleware normally rejects earlier with 409), 403 when the scope is not
//! owned by the active creator, 400 on validation failures (empty body,
//! missing TTL kind/depth, empty scope id), 409 when `set` without `replace`
//! hits an already-active directive, and 500 on storage / internal failures.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::directive_store::now_ms;
use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::generated::daemon_api::inspector::moment_directive_request::{
    MomentDirectiveRequest, MomentDirectiveRequestAction, MomentDirectiveRequestScopeKind,
};
use nexus_contracts::generated::daemon_api::inspector::moment_directive_response::MomentDirectiveResponse;
use nexus_local_db::moment_directive::{
    clear, get_active_for_work, get_active_for_world, replace_active, scope_kind, set_active,
    MomentDirectiveRow, NewMomentDirective,
};
use nexus_local_db::{get_work, narrative_write, LocalDbError};

// ── POST /v1/daemon/moment-directive ────────────────────────────────────

/// Set / show / clear the active Moment Directive for an owned scope
/// (V1.151 P0, DF-76).
///
/// Guard order (plan lock): tier2 middleware (API key + active creator) →
/// scope ownership (403) → validation mirroring CLI `handle_set` (400) →
/// `nexus_local_db::moment_directive` thin wrapper → directive row JSON
/// (`show`/`set`) or `{}` (`clear`).
#[allow(clippy::missing_errors_doc)]
pub async fn moment_directive(
    State(state): State<WorkspaceState>,
    Json(req): Json<MomentDirectiveRequest>,
) -> Result<Json<MomentDirectiveResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let scope_id = req.scope.id.trim();
    if scope_id.is_empty() {
        return Err(validation("scope.id", "must be non-empty"));
    }

    // Ownership gate (inspector/check pattern): a foreign scope never leaks
    // directive state — 403, not 404.
    let owned = match req.scope.kind {
        MomentDirectiveRequestScopeKind::Work => is_work_owned(pool, &creator_id, scope_id).await?,
        MomentDirectiveRequestScopeKind::World => {
            narrative_write::is_world_owned(pool, &creator_id, scope_id)
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "DATABASE_ERROR".to_string(),
                    message: e.to_string(),
                })?
        }
    };
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("{} {}", req.scope.kind, scope_id),
            reason: "you do not own this scope".to_string(),
        });
    }

    // The DB scope_kind string (`scope_kind::WORK` | `scope_kind::WORLD`).
    let kind = match req.scope.kind {
        MomentDirectiveRequestScopeKind::Work => scope_kind::WORK,
        MomentDirectiveRequestScopeKind::World => scope_kind::WORLD,
    };

    match req.action {
        MomentDirectiveRequestAction::Set => {
            Ok(Json(set(pool, &creator_id, &req, kind, scope_id).await?))
        }
        MomentDirectiveRequestAction::Show => {
            Ok(Json(show(pool, &creator_id, kind, scope_id).await?))
        }
        MomentDirectiveRequestAction::Clear => {
            Ok(Json(clear_action(pool, &creator_id, kind, scope_id).await?))
        }
    }
}

/// `set` — validation mirrors CLI `handle_set`, then
/// `set_active` / `replace_active` (thin wrapper).
async fn set(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    req: &MomentDirectiveRequest,
    kind: &str,
    scope_id: &str,
) -> Result<MomentDirectiveResponse, NexusApiError> {
    // Non-empty body after trim (CLI `handle_set`, spec §3.1).
    let Some(body) = req.body.as_deref() else {
        return Err(validation("body", "is required for set"));
    };
    let body = body.trim();
    if body.is_empty() {
        return Err(validation(
            "body",
            "must be non-empty (after trimming whitespace)",
        ));
    }
    // Exactly one TTL kind, count >= 1 (`NonZeroU64` makes ttl_remaining >= 1
    // by construction; the signed cast mirrors the CLI's `i64 --ttl-*` flags).
    // Wire name `ttl_remaining` per the spec §5 H5 lock (W-3 / QC1-F-001).
    let (Some(ttl_kind), Some(ttl_count)) = (req.ttl_kind, req.ttl_remaining) else {
        return Err(validation(
            "ttl_kind",
            "exactly one TTL kind with a positive ttl_remaining is required for set",
        ));
    };
    let ttl_remaining = i64::try_from(ttl_count.get())
        .map_err(|_| validation("ttl_remaining", "must fit in a signed 64-bit count"))?;
    // Known insert depth (closed wire enum; required for set).
    let Some(insert_depth) = req.insert_depth else {
        return Err(validation("insert_depth", "is required for set"));
    };

    let new = NewMomentDirective {
        directive_id: &generate_directive_id(),
        creator_id,
        scope_kind: kind,
        scope_id,
        body,
        insert_depth: &insert_depth.to_string(),
        ttl_kind: &ttl_kind.to_string(),
        ttl_remaining,
        clear_on_scene_change: req.clear_on_scene_change.unwrap_or(false),
        now: now_ms(),
    };

    let row = if req.replace.unwrap_or(false) {
        replace_active(pool, &new)
            .await
            .map_err(|e| database_error(&e))?
    } else {
        match set_active(pool, &new).await {
            Ok(row) => row,
            // The unique partial index `moment_directives_one_active_per_scope`
            // rejects a second active row — surface as 409, mirroring the
            // CLI's "--replace required" message (no silent overwrite).
            Err(LocalDbError::Sqlx(sqlx::Error::Database(db_err)))
                if db_err.is_unique_violation() =>
            {
                return Err(NexusApiError::Conflict(
                    "A Moment Directive is already active for this scope. \
                     Pass \"replace\": true to supersede it (the old directive is retained \
                     with `replaced_by` set to the new id)."
                        .to_string(),
                ));
            }
            Err(e) => return Err(database_error(&e)),
        }
    };

    response_from_row(&row)
}

/// `show` — resolve the **effective** directive for the scope (incl. body,
/// the author surface); `{}` when nothing is effective.
///
/// Mirrors the CLI `resolve_effective_for_show`
/// (`apps/nexus42/.../moment_directive.rs:298-324`, spec §3.2 — W-2 / QC3):
/// for a Work scope the Work's own directive wins; with none, the bound
/// World's override is inherited (the returned row's `scope_kind` /
/// `scope_id` then name the inherited source). A World scope returns the
/// World override itself.
async fn show(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    kind: &str,
    scope_id: &str,
) -> Result<MomentDirectiveResponse, NexusApiError> {
    let row = if kind == scope_kind::WORK {
        match get_active_for_work(pool, creator_id, scope_id).await {
            // Work-wins.
            Ok(Some(row)) => Some(row),
            // Confirmed no Work directive — inherit the bound World's
            // override (the ownership gate already verified the Work, so
            // `get_work` resolves; a worldless Work has no override).
            Ok(None) => {
                let world_id = get_work(pool, creator_id, scope_id)
                    .await
                    .map_err(|e| database_error(&e))?
                    .and_then(|w| w.world_id);
                match world_id {
                    Some(world_id) => get_active_for_world(pool, creator_id, &world_id)
                        .await
                        .map_err(|e| database_error(&e))?,
                    None => None,
                }
            }
            Err(e) => return Err(database_error(&e)),
        }
    } else {
        get_active_for_world(pool, creator_id, scope_id)
            .await
            .map_err(|e| database_error(&e))?
    };

    row.map_or_else(|| Ok(empty_response()), |row| response_from_row(&row))
}

/// `clear` — soft-delete the active row (retained for DF-76 inspection);
/// always responds `{}`.
async fn clear_action(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    kind: &str,
    scope_id: &str,
) -> Result<MomentDirectiveResponse, NexusApiError> {
    clear(pool, creator_id, kind, scope_id, now_ms())
        .await
        .map_err(|e| database_error(&e))?;
    Ok(empty_response())
}

/// Work ownership: `works::get_work` is creator-scoped in the query itself —
/// `Ok(Some(_))` means the Work belongs to the active creator.
async fn is_work_owned(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<bool, NexusApiError> {
    get_work(pool, creator_id, work_id)
        .await
        .map(|row| row.is_some())
        .map_err(|e| database_error(&e))
}

/// Map a directive row onto the typed response (the `Directive` oneOf branch).
///
/// JSON round-trip bridge (check.rs precedent): the row's `serde::Serialize`
/// output IS the wire shape, so `from_value` converts it into the generated
/// enum — including the String → closed-enum conversions — and fails loudly
/// (500) if the row ever carries a value outside the schema vocabulary,
/// instead of silently widening the wire contract. The generated enum keeps
/// the nullable fields present-as-`null` (no `skip_serializing_if`), so the
/// serialized response is byte-identical to the pre-schema `Json<Value>`.
fn response_from_row(row: &MomentDirectiveRow) -> Result<MomentDirectiveResponse, NexusApiError> {
    let wire = serde_json::to_value(row).map_err(|e| NexusApiError::Internal {
        code: "DIRECTIVE_ROW_SERIALIZE".to_string(),
        message: e.to_string(),
    })?;
    serde_json::from_value(wire).map_err(|e| NexusApiError::Internal {
        code: "DIRECTIVE_RESPONSE_DECODE".to_string(),
        message: e.to_string(),
    })
}

/// The `Empty` oneOf branch — serializes to `{}` (`show` with no effective
/// directive / `clear`; mirrors the pre-schema wire behavior).
fn empty_response() -> MomentDirectiveResponse {
    MomentDirectiveResponse::Empty(serde_json::Map::new())
}

/// Invalid-input (400) helper.
fn validation(field: &str, reason: &str) -> NexusApiError {
    NexusApiError::InvalidInput {
        field: field.to_string(),
        reason: reason.to_string(),
    }
}

/// Map a local-db error onto the canonical internal (500) envelope.
fn database_error(e: &LocalDbError) -> NexusApiError {
    NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    }
}

/// Generate a stable directive id (`dir_<uuid v4>`).
fn generate_directive_id() -> String {
    format!("dir_{}", uuid::Uuid::new_v4())
}

// `now_ms` — shared crate-level helper (`crate::directive_store::now_ms`,
// QC3-S-2 dedupe).
