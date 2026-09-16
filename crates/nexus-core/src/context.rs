//! Moment context, directive and inspection (v1.190 P2-T3).
//!
//! Moved from the daemon `api::handlers::{directive,inspector}` handlers and
//! the `directive_store` composition root: the Moment Directive set/show/
//! clear family, the enriched MCA inspector assembly, the shared
//! [`DirectiveStore`] adapters (lifecycle + read-only) and the serializable
//! admitted-Actor context. Storage is always `nexus-local-db` via the same
//! repositories; MCA is consumed on its default-features=false spoke edge.
//!
//! Observability contract (HARD, unchanged): `inspect_moment` **observes**
//! `assemble_moment` output only — no re-computation, no KB mutation, no
//! writes. The read-only directive store never runs the post-injection
//! lifecycle, so a poll never burns TTL or writes anchors; the Moment
//! Directive body is never on the wire (status/metadata only, AC-I3).

use nexus_contracts::generated::core::core_actor_context::{CoreActorContext, NexusActorRef};
use nexus_contracts::generated::daemon_api::inspector::moment_directive_request::{
    MomentDirectiveRequest, MomentDirectiveRequestAction, MomentDirectiveRequestScopeKind,
};
use nexus_contracts::generated::daemon_api::inspector::moment_directive_response::MomentDirectiveResponse;
use nexus_contracts::generated::daemon_api::inspector::{
    moment_inspect_request::MomentInspectRequest, moment_inspect_response::MomentInspectResponse,
};
use nexus_local_db::moment_directive::{
    clear, clear_on_scene_change, decrement_ttl_by, get_active_for_work, get_active_for_world,
    get_by_id, get_chapter_anchor, scope_kind, set_active, update_lifecycle_anchor,
    upsert_chapter_anchor, MomentDirectiveRow, NewMomentDirective,
};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::{
    get_work, is_novel_profile, narrative_write, LocalDbError, SqliteKnowledgeStore,
};
use nexus_moment_context_assembly::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, DirectiveTtlKind,
};
use nexus_moment_context_assembly::{
    assemble_moment_with_directive, build_inspector_packet, GenerationStage, MomentRequest,
    Stage0Assembly,
};
use nexus_spoke_adapter::adapter::NexusAdapter;
use nexus_spoke_adapter::SpokeBackedKbStore;
use sqlx::SqlitePool;

use crate::actors::ActorViewpoint;
use crate::error::{CoreError, CoreResult};
use crate::memory_pipeline::internal_err;
use crate::principal::Principal;
use crate::service::CoreService;

/// Unix epoch milliseconds (QC3-S-2 dedupe: the single directive lifecycle
/// clock, shared by both store adapters and the directive family).
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn wire_err(err: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("directive_wire_invalid: {err}"),
    }
}

fn invalid_input(field: &str, reason: &str) -> CoreError {
    CoreError::InvalidInput {
        field: field.to_string(),
        reason: reason.to_string(),
    }
}

fn database_error(e: &LocalDbError) -> CoreError {
    CoreError::Internal {
        category: format!("database_error: {e}"),
    }
}

/// Map a `narrative_write` domain error onto the matching core category: a
/// genuine storage fault stays the database class, an id-format rejection is
/// invalid input, a missing FK reference is not-found, and a sequence
/// conflict is the retained plain 409. (The world-ownership probes never
/// produce the write-specific arms in practice, but the mapping keeps the
/// classification honest instead of collapsing everything into 500.)
fn map_narrative_write_error(e: nexus_local_db::narrative_write::NarrativeWriteError) -> CoreError {
    match &e {
        nexus_local_db::narrative_write::NarrativeWriteError::Database(_) => {
            internal_err("database_error", e)
        }
        nexus_local_db::narrative_write::NarrativeWriteError::InvalidId {
            field,
            value,
            reason,
        } => invalid_input(field, &format!("invalid {field} '{value}': {reason}")),
        nexus_local_db::narrative_write::NarrativeWriteError::FkNotFound { table, id } => {
            CoreError::NotFound {
                resource: format!("referenced {table} '{id}' not found"),
            }
        }
        nexus_local_db::narrative_write::NarrativeWriteError::SequenceConflict {
            world_id,
            branch_id,
            sequence_no,
        } => CoreError::Conflict(format!(
            "sequence conflict: event already exists at ({world_id}, {branch_id}, {sequence_no})"
        )),
    }
}

// ── DirectiveStore adapters (composition root) ──────────────────────────

/// Composition-root [`DirectiveStore`] over `nexus-local-db`, consumed by
/// `assemble_moment_with_directive` at run/assemble wiring sites. Cannot live
/// in `nexus-local-db` (dependency cycle with MCA).
#[derive(Debug, Clone)]
pub struct LocalDirectiveStore {
    pool: SqlitePool,
}

impl LocalDirectiveStore {
    /// Create the adapter over a shared pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DirectiveStore for LocalDirectiveStore {
    async fn load_active(
        &self,
        creator_id: Option<&str>,
        work_id: Option<&str>,
        world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        let creator_id = creator_id?;
        let row = resolve_active_row(&self.pool, creator_id, work_id, world_id).await?;
        map_to_active_directive(row)
    }

    async fn after_injection(
        &self,
        directive_id: &str,
        event_id: Option<&str>,
        work_id: Option<&str>,
    ) {
        after_injection_lifecycle(&self.pool, directive_id, event_id, work_id).await;
    }
}

/// Read-only [`DirectiveStore`] for the inspector route (W-001).
///
/// Resolves + renders the active directive exactly like
/// [`LocalDirectiveStore`] but **never** runs the post-injection lifecycle —
/// [`DirectiveStore::after_injection`] is a no-op. The inspector is an
/// observation surface with a hard "no writes" contract: a poll must not burn
/// TTL, reset the scene anchor (`last_focused_event_id`), or write chapter
/// anchors. The `DirectiveStore` trait already separates `load_active` (read)
/// from `after_injection` (write), so this wrapper is the whole fix — the
/// packet shows the true remaining TTL as persisted at load and inspection
/// never mutates directive state.
#[derive(Debug, Clone)]
pub struct ReadOnlyDirectiveStore {
    pool: SqlitePool,
}

impl ReadOnlyDirectiveStore {
    /// Create the read-only adapter over a shared pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DirectiveStore for ReadOnlyDirectiveStore {
    async fn load_active(
        &self,
        creator_id: Option<&str>,
        work_id: Option<&str>,
        world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        let creator_id = creator_id?;
        let row = resolve_active_row(&self.pool, creator_id, work_id, world_id).await?;
        map_to_active_directive(row)
    }

    async fn after_injection(
        &self,
        _directive_id: &str,
        _event_id: Option<&str>,
        _work_id: Option<&str>,
    ) {
        // Read-only: the inspector never advances the directive lifecycle.
    }
}

/// Scope resolution (spec §3.2 — Work wins / World-override fallback).
///
/// 1. If the Work has a Work-scoped directive, use it.
/// 2. Else if the Work's `world_id` has a World-scoped override, use it.
/// 3. Else no directive.
///
/// A World override never leaks across unrelated Worlds or to worldless
/// Works: the Work→World binding is verified against the `works` table, and
/// an unknown Work (binding unverifiable) resolves to no directive.
/// A raw world assembly (no Work context) applies the World override
/// directly to the focused World.
///
/// **Error isolation (QC2-F2):** a **failed** read is "no directive", never a
/// fall-through. Only a *confirmed* result (`Ok(None)`) — no Work directive,
/// or a verified World-bound Work — may fall through to the World override.
/// All DB-error degradation paths warn (QC3-S001); failures degrade to
/// "no directive", never fail the assembly.
async fn resolve_active_row(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: Option<&str>,
    world_id: Option<&str>,
) -> Option<MomentDirectiveRow> {
    if let Some(work_id) = work_id {
        match get_active_for_work(pool, creator_id, work_id).await {
            // Work-wins.
            Ok(Some(row)) => return Some(row),
            // Unverifiable Work-directive state: do NOT fall through to the
            // World override (QC2-F2).
            Err(e) => {
                tracing::warn!(creator_id, work_id, error = %e,
                    "moment directive: work-scoped read failed; resolving to no directive");
                return None;
            }
            // Confirmed no Work directive — the binding check may fall
            // through to the World override.
            Ok(None) => {}
        }
        match get_work(pool, creator_id, work_id).await {
            // Binding verified: a World-bound Work inherits the override.
            Ok(Some(work)) => match work.world_id {
                Some(world_id) => match get_active_for_world(pool, creator_id, &world_id).await {
                    Ok(row) => row,
                    Err(e) => {
                        tracing::warn!(creator_id, work_id, error = %e,
                            "moment directive: world-override read failed; resolving to no directive");
                        None
                    }
                },
                // Confirmed worldless Work — no override can apply.
                None => None,
            },
            // Unknown Work (Ok(None)) or unreadable binding (Err): no override.
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(creator_id, work_id, error = %e,
                    "moment directive: work binding read failed; resolving to no directive");
                None
            }
        }
    } else {
        match get_active_for_world(pool, creator_id, world_id?).await {
            Ok(row) => row,
            Err(e) => {
                tracing::warn!(creator_id, error = %e,
                    "moment directive: world override read failed; resolving to no directive");
                None
            }
        }
    }
}

/// Map a stored row to the MCA payload. A corrupt row (empty body / unknown
/// depth / TTL kind strings) never injects — the adapter skips it with a
/// warning (failures degrade to "no directive", never fail the assembly).
fn map_to_active_directive(row: MomentDirectiveRow) -> Option<ActiveDirective> {
    if row.body.trim().is_empty() {
        // QC2-F5: a corrupt row with an empty body would render nothing yet
        // still count as an injection — skip it, and because `load_active`
        // returns `None` the post-injection TTL decrement never runs.
        tracing::warn!(directive_id = %row.directive_id,
            "moment directive row has an empty body; skipping injection");
        return None;
    }
    let Some(insert_depth) = DirectiveDepth::parse(&row.insert_depth) else {
        tracing::warn!(directive_id = %row.directive_id, depth = %row.insert_depth,
            "moment directive row has unknown insert_depth; skipping injection");
        return None;
    };
    let Some(ttl_kind) = DirectiveTtlKind::parse(&row.ttl_kind) else {
        tracing::warn!(directive_id = %row.directive_id, ttl_kind = %row.ttl_kind,
            "moment directive row has unknown ttl_kind; skipping injection");
        return None;
    };
    Some(ActiveDirective {
        directive_id: row.directive_id,
        body: row.body,
        insert_depth,
        ttl_kind,
        clear_on_scene_change: row.clear_on_scene_change,
        // V1.151 P0 (DF-76 spec §2 H6): carry the persisted metadata through
        // to MCA for the inspector packet — status/metadata only, never the
        // body (AC-I3). `ttl_remaining` is `i64` on the row; only non-
        // negative values surface (an active row's TTL never goes negative).
        // `u64` matches the wire input width (NonZeroU64) so counts above
        // u32::MAX render instead of silently nulling (QC3-S-1).
        ttl_remaining: u64::try_from(row.ttl_remaining).ok(),
        status: row.status,
        scope_kind: row.scope_kind,
        scope_id: row.scope_id,
    })
}

/// Post-injection lifecycle (spec §3.3) — run after a directive was actually
/// injected by `assemble_moment_with_directive`:
///
/// 1. **Scene clear**: when `clear_on_scene_change` is set and the focused
///    moment anchor (`MomentRequest.event_id`) changed between two injecting
///    assembles, soft-delete instead of decrementing. The first injection
///    (no previous anchor) never clears. Documented limitation (guide Q7):
///    no true scene concept exists; `event_id` is the V1.150 proxy.
/// 2. **TTL burn**: `generations` burns 1 on every injecting assemble.
///    `chapters` burns the **delta** of chapter advances since the last
///    injecting assemble **for the same work** — tracked per
///    (directive, work) in `moment_directive_chapter_anchors` so a
///    world-scoped directive burns independently per Work that uses it
///    (R-V1150P2-008); for essay/game-bible/script/worldless Works it
///    behaves identically to `generations` (documented fallback, spec §3.3).
/// 3. **Re-anchor**: store the observed `event_id` (directive row) and
///    chapter (per-work anchor table) so the next assemble can detect change.
///
/// Best-effort: failures are logged, never surfaced as assembly errors.
async fn after_injection_lifecycle(
    pool: &SqlitePool,
    directive_id: &str,
    event_id: Option<&str>,
    work_id: Option<&str>,
) {
    let Ok(Some(row)) = get_by_id(pool, directive_id).await else {
        return;
    };

    // Scene-change clear.
    if row.clear_on_scene_change {
        if let (Some(last), Some(current)) = (row.last_focused_event_id.as_deref(), event_id) {
            if last != current {
                if let Err(e) = clear_on_scene_change(pool, directive_id, now_ms()).await {
                    tracing::warn!(directive_id, error = %e, "moment directive scene-clear failed");
                }
                return;
            }
        }
    }

    // Chapter-advance TTL burn for novel Works with `chapters` TTL. The burn
    // is the chapter delta since this work's last injecting assemble — see
    // `decrement_ttl_by` in `nexus-local-db` for the write-failure threat
    // model (R-V1150P2-005 accepted: atomic via RETURNING; a write failure
    // on local-only SQLite indicates a broken DB, not a lost-update window).
    let (burn, chapter_anchor) = match (row.ttl_kind.as_str(), work_id) {
        ("chapters", Some(work_id)) => {
            match get_work(pool, &row.creator_id, work_id).await {
                Ok(Some(work)) if is_novel_profile(work.work_profile.as_deref()) => {
                    let chapter = i64::from(work.current_chapter);
                    let burn = match get_chapter_anchor(pool, directive_id, work_id).await {
                        // Delta since this work's last injecting assemble;
                        // never negative — a chapter rewind does not refund TTL.
                        Ok(Some(last)) => i64::max(0, chapter - last),
                        // First injecting assemble for this work: observe
                        // only, no burn (R-V1150P2-004).
                        Ok(None) => 0,
                        // Unreadable anchor: degrade like the non-novel
                        // fallback (burn 1, keep the directive moving toward
                        // expiry) — never fail the assembly (QC2-F2).
                        Err(e) => {
                            tracing::warn!(directive_id, work_id, error = %e,
                                "moment directive: chapter anchor read failed; burning 1");
                            1
                        }
                    };
                    (burn, Some((work_id, chapter)))
                }
                // Non-novel / unknown Work: `chapters` behaves like
                // `generations` (documented, not silent — spec §3.3).
                _ => (1, None),
            }
        }
        _ => (1, None),
    };

    // Anchor first, then burn: if the burn write fails, the next assemble
    // re-reads the row and re-attempts; if the directive expires at 0, the
    // anchors are already recorded for DF-76 inspection.
    if let Err(e) = update_lifecycle_anchor(pool, directive_id, event_id, now_ms()).await {
        tracing::warn!(directive_id, error = %e, "moment directive anchor update failed");
    }
    if let Some((work_id, chapter)) = chapter_anchor {
        if let Err(e) = upsert_chapter_anchor(pool, directive_id, work_id, chapter, now_ms()).await
        {
            tracing::warn!(directive_id, work_id, error = %e,
                "moment directive chapter anchor upsert failed");
        }
    }
    if burn > 0 {
        if let Err(e) = decrement_ttl_by(pool, directive_id, burn, now_ms()).await {
            tracing::warn!(directive_id, error = %e, "moment directive TTL decrement failed");
        }
    }
}

// ── Moment Directive family (set / show / clear) ────────────────────────

impl CoreService {
    /// Set / show / clear the active Moment Directive for a scope the
    /// principal owns (V1.151 P0, DF-76).
    ///
    /// Guard order (plan lock, preserved): principal verification → scope
    /// ownership (403 — a foreign scope never leaks directive state, not
    /// 404) → validation mirroring the CLI `handle_set` (400) → the
    /// `nexus_local_db::moment_directive` thin wrappers → directive row JSON
    /// (`show`/`set`) or `{}` (`clear`).
    ///
    /// - `set` requires a non-empty body, exactly one TTL kind with
    ///   `ttl_remaining >= 1`, a known `insert_depth`, and explicit
    ///   `replace` when a directive is already active in the scope (no
    ///   silent overwrite — the unique partial index
    ///   `moment_directives_one_active_per_scope` enforces it).
    /// - `show` resolves the **effective** directive (Work-wins / World
    ///   override inheritance) incl. body — the author surface, never the
    ///   inspector packet.
    /// - `clear` soft-deletes the active row (retained for DF-76
    ///   inspection).
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] when the principal fails verification,
    /// [`CoreError::ForbiddenReason`] for a foreign scope,
    /// [`CoreError::InvalidInput`] on validation failures,
    /// [`CoreError::Conflict`] for `set` without `replace` over an active
    /// directive, and the mapped storage error otherwise.
    pub async fn moment_directive(
        &self,
        principal: &Principal,
        req: MomentDirectiveRequest,
    ) -> CoreResult<MomentDirectiveResponse> {
        self.verify_principal(principal)?;
        let pool = &self.inner.pool;
        let creator_id = principal.creator_id();

        let scope_id = req.scope.id.trim();
        if scope_id.is_empty() {
            return Err(invalid_input("scope.id", "must be non-empty"));
        }

        // Ownership gate (inspector/check pattern): a foreign scope never
        // leaks directive state — 403, not 404.
        let owned = match req.scope.kind {
            MomentDirectiveRequestScopeKind::Work => {
                is_work_owned(pool, creator_id, scope_id).await?
            }
            MomentDirectiveRequestScopeKind::World => {
                narrative_write::is_world_owned(pool, creator_id, scope_id)
                    .await
                    .map_err(map_narrative_write_error)?
            }
        };
        if !owned {
            return Err(CoreError::ForbiddenReason {
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
            MomentDirectiveRequestAction::Set => set(pool, creator_id, &req, kind, scope_id).await,
            MomentDirectiveRequestAction::Show => show(pool, creator_id, kind, scope_id).await,
            MomentDirectiveRequestAction::Clear => {
                clear_action(pool, creator_id, kind, scope_id).await
            }
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
) -> CoreResult<MomentDirectiveResponse> {
    // Non-empty body after trim (CLI `handle_set`, spec §3.1).
    let Some(body) = req.body.as_deref() else {
        return Err(invalid_input("body", "is required for set"));
    };
    let body = body.trim();
    if body.is_empty() {
        return Err(invalid_input(
            "body",
            "must be non-empty (after trimming whitespace)",
        ));
    }
    // Exactly one TTL kind, count >= 1 (`NonZeroU64` makes ttl_remaining >= 1
    // by construction; the signed cast mirrors the CLI's `i64 --ttl-*` flags).
    // Wire name `ttl_remaining` per the spec §5 H5 lock (W-3 / QC1-F-001).
    let (Some(ttl_kind), Some(ttl_count)) = (req.ttl_kind, req.ttl_remaining) else {
        return Err(invalid_input(
            "ttl_kind",
            "exactly one TTL kind with a positive ttl_remaining is required for set",
        ));
    };
    let ttl_remaining = i64::try_from(ttl_count.get())
        .map_err(|_| invalid_input("ttl_remaining", "must fit in a signed 64-bit count"))?;
    // Known insert depth (closed wire enum; required for set).
    let Some(insert_depth) = req.insert_depth else {
        return Err(invalid_input("insert_depth", "is required for set"));
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
        nexus_local_db::moment_directive::replace_active(pool, &new)
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
                return Err(CoreError::Conflict(
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
/// Mirrors the CLI `resolve_effective_for_show`: for a Work scope the Work's
/// own directive wins; with none, the bound World's override is inherited
/// (the returned row's `scope_kind` / `scope_id` then name the inherited
/// source). A World scope returns the World override itself.
async fn show(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    kind: &str,
    scope_id: &str,
) -> CoreResult<MomentDirectiveResponse> {
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
) -> CoreResult<MomentDirectiveResponse> {
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
) -> CoreResult<bool> {
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
/// (500) if the row ever carries a value outside the schema vocabulary.
fn response_from_row(row: &MomentDirectiveRow) -> CoreResult<MomentDirectiveResponse> {
    let wire = serde_json::to_value(row).map_err(|e| CoreError::Internal {
        category: format!("directive_row_serialize: {e}"),
    })?;
    serde_json::from_value(wire).map_err(|e| CoreError::Internal {
        category: format!("directive_response_decode: {e}"),
    })
}

/// The `Empty` oneOf branch — serializes to `{}` (`show` with no effective
/// directive / `clear`).
fn empty_response() -> MomentDirectiveResponse {
    MomentDirectiveResponse::Empty(serde_json::Map::new())
}

/// Generate a stable directive id (`dir_<uuid v4>`).
fn generate_directive_id() -> String {
    format!("dir_{}", uuid::Uuid::new_v4())
}

// ── Inspector assembly ──────────────────────────────────────────────────

impl CoreService {
    /// Assemble one moment over an owned World and return the enriched
    /// inspector packet (V1.151 P0, DF-76).
    ///
    /// Guard order (plan lock, preserved): principal verification → world
    /// ownership (`is_world_owned`, 403) → work→world binding check (400
    /// when the Work is bound to a different World, QC2-S-001 — the
    /// rejection happens **before** any assembly) → `MomentRequest`
    /// construction mirroring the CLI `run_assemble_moment` wiring (confirmed
    /// relation edges preloaded for relation-hop expansion, degraded to
    /// activation-only when the read fails or the graph is empty) →
    /// `assemble_moment_with_directive` over the same persistent stores the
    /// CLI uses with a **read-only** directive store → enriched packet via
    /// the single relocated builder → JSON round-trip onto the generated
    /// wire DTO.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] when the principal fails verification,
    /// [`CoreError::ForbiddenReason`] for a foreign world,
    /// [`CoreError::InvalidInput`] for a cross-world Work binding, and the
    /// mapped internal errors otherwise.
    pub async fn inspect_moment(
        &self,
        principal: &Principal,
        req: MomentInspectRequest,
    ) -> CoreResult<MomentInspectResponse> {
        self.verify_principal(principal)?;
        let pool = &self.inner.pool;
        let creator_id = principal.creator_id();

        // Ownership gate (compute_runs / check pattern): a foreign World never
        // leaks assembly behavior — 403, not 404, so world existence stays
        // unobservable to other creators.
        let owned = narrative_write::is_world_owned(pool, creator_id, req.world_id.as_str())
            .await
            .map_err(map_narrative_write_error)?;
        if !owned {
            return Err(CoreError::ForbiddenReason {
                resource: format!("world {}", req.world_id.as_str()),
                reason: "you do not own this world".to_string(),
            });
        }

        // Work→world binding (QC2-S-001): when both ids are given, the Work's
        // own binding must agree with the request World. Otherwise a Work
        // bound to World B passed alongside owned World A would resolve B's
        // World override into A's assembly — the packet would show a
        // directive whose scope_id does not match the request world_id. A
        // worldless or unknown Work resolves no override and stays legal
        // (matches the CLI).
        if let Some(work_id) = req.work_id.as_deref() {
            let work = get_work(pool, creator_id, work_id)
                .await
                .map_err(|e| database_error(&e))?;
            if let Some(work) = work {
                if let Some(bound_world) = work.world_id.as_deref() {
                    if bound_world != req.world_id.as_str() {
                        return Err(invalid_input(
                            "work_id",
                            &format!(
                                "work {work_id} is bound to world {bound_world}, \
                                 not the requested world {}",
                                req.world_id.as_str()
                            ),
                        ));
                    }
                }
            }
        }

        // MomentRequest mirroring the CLI `run_assemble_moment` wiring minus
        // the CLI-only knobs: the active creator + world are always threaded;
        // work + generation stage only when the request carries them. Stage-0
        // stays the empty default — this is an observation surface and the
        // packet never renders stage-0 content (spec §2).
        let mut request = MomentRequest::new(Stage0Assembly::default())
            .with_world(req.world_id.as_str())
            .with_creator(creator_id)
            .with_user(creator_id);
        if let Some(work_id) = req.work_id.as_deref() {
            request = request.with_work(work_id);
        }
        if let Some(stage) = req.generation_stage.as_ref() {
            // The wire enum is schema-closed (8 variants mapping 1:1 onto
            // GenerationStage), so parse is total here; the `else` arm is
            // defensive against future schema drift — unknown degrades to
            // unspecified (all slots on), matching the CLI.
            if let Some(gs) = GenerationStage::parse(&stage.to_string()) {
                request = request.with_generation_stage(gs);
            } else {
                tracing::warn!(
                    stage = %stage,
                    "unknown generation stage for inspector moment; treating as unspecified (all slots on)"
                );
            }
        }

        // Greptile P1 (CLI parity): preload the World's confirmed relation
        // edges for relation-hop expansion — without them the inspector
        // packet would omit hopped entries that the CLI assembly includes.
        // Best-effort like the CLI: a storage-read failure degrades to
        // activation-only (no hop pass, no panic), and an empty confirmed
        // graph yields `None` (P0 activation-only behavior). The CLI only
        // caps the hop budget when the caller passes `--max-tokens`;
        // `MomentInspectRequest` carries no such knob, so the cap stays
        // unset — MCA's `hop_budget_tokens` then runs the hop pass
        // depth+cycle-only, identical to a default CLI invocation.
        let hop_edges = NexusAdapter::new(pool.clone())
            .list_hop_edges_for_world(req.world_id.as_str())
            .await
            .ok()
            .filter(|edges| !edges.is_empty());
        if let Some(edges) = hop_edges {
            request = request.with_hop_edges(edges);
        }

        // Four-domain assembly over the same persistent stores the CLI uses
        // + a **read-only** directive store (W-001 / QC2-W-001 + QC3-W-1):
        // the packet's `moment_directive` section reflects the active
        // directive (Work scope → World override fallback) without running
        // the post-injection lifecycle. None active ⇒ byte-equivalent to
        // plain `assemble_moment` (AC-I1b). Per-domain failures degrade to
        // omitted sections, they never reject.
        let narrative = SqliteNarrativeGateway::new(pool.clone());
        let kb = SpokeBackedKbStore::new(pool.clone());
        let knowledge = SqliteKnowledgeStore::new(pool.clone());
        let directives = ReadOnlyDirectiveStore::new(pool.clone());
        let ctx =
            assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives)
                .await;

        // Enriched packet → generated wire DTO via JSON round-trip at the
        // boundary (validates the builder output against the wire contract —
        // `moment_directive` carries status/metadata only, AC-I3).
        let packet = build_inspector_packet(&ctx);
        let resp: MomentInspectResponse =
            serde_json::from_value(packet).map_err(|e| CoreError::Internal {
                category: format!(
                    "inspector_packet_decode: build_inspector_packet output did not match \
                     the MomentInspectResponse wire shape: {e}"
                ),
            })?;
        Ok(resp)
    }
}

// ── Admitted Actor context ──────────────────────────────────────────────

impl CoreService {
    /// Admit a stored Actor against stored ownership and project the
    /// serializable [`CoreActorContext`] (v1.190 P2-T3 contract API).
    ///
    /// This is the single Actor-truth entry for the P3 `nexus.context.assemble`
    /// capability and the P4 Host prompt consumers: they call this service
    /// instead of re-deriving stored ownership, so Actor truth is never
    /// recomputed. The bounded SOUL/Memory/`ToM` mind content is a separate
    /// P2-T2 concern ([`crate::CoreCharacterMind`]) and is deliberately not
    /// folded into this schema-closed projection.
    ///
    /// # Errors
    /// As [`crate::CoreActorAdmission`] admission: [`CoreError::AuthRequired`]
    /// on principal failure, [`CoreError::NotFound`] for missing/foreign
    /// actor/world/binding rows, [`CoreError::ActorConflict`] for inactive
    /// rows, and [`CoreError::ActorInput`] for pair-shape violations.
    pub async fn actor_context(
        &self,
        principal: &Principal,
        actor: &crate::actors::AdmittedActor,
        viewpoint: &ActorViewpoint,
    ) -> CoreResult<CoreActorContext> {
        self.verify_principal(principal)?;
        // Full stored admission (owner, active World/Character/binding, the
        // bounded KnowledgeView) — the single Actor-truth path; the serializable
        // projection below carries no authority.
        let admitted = crate::actors::CoreActorAdmission::new(self.inner.pool.clone())
            .admit(principal.creator_id(), actor.clone(), viewpoint.clone())
            .await?;

        let actor_ref = actor_ref_wire(actor)?;
        let value = serde_json::json!({
            "actor_ref": serde_json::to_value(&actor_ref).map_err(wire_err)?,
            "owner_creator_id": admitted.owner_creator_id,
            "world_id": admitted.world_id,
            "binding_id": admitted.binding_id,
            "branch_id": admitted.branch_id,
            "event_id": admitted.event_id,
            "character_epoch": admitted.character_epoch,
        });
        serde_json::from_value(value).map_err(wire_err)
    }
}

type ContextActorRef = NexusActorRef;
type ContextConversionError = <ContextActorRef as std::str::FromStr>::Err;

/// Project the opaque admitted Actor onto the closed wire sum.
fn actor_ref_wire(actor: &crate::actors::AdmittedActor) -> CoreResult<ContextActorRef> {
    match actor {
        crate::actors::AdmittedActor::Creator { creator_id } => {
            Ok(ContextActorRef::CreatorActorRef {
                actor_kind: "creator".parse().map_err(wire_err)?,
                creator_id: creator_id.parse().map_err(wire_err)?,
            })
        }
        crate::actors::AdmittedActor::Character { character_id } => {
            Ok(ContextActorRef::CharacterActorRef {
                actor_kind: "character".parse().map_err(wire_err)?,
                character_id: character_id.parse().map_err(wire_err)?,
            })
        }
    }
}
