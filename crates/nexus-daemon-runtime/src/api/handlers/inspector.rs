//! Inspector handlers — Daemon HTTP surface for the enriched MCA assembly
//! inspector packet (V1.151 P0, DF-76).
//!
//! One route:
//! - `POST /v1/daemon/inspector/moment` — assemble one moment over an owned
//!   World and return the enriched inspector packet (`modules.placement` +
//!   `modules.activation_trace` + `slot_map` + `budget` +
//!   `moment_directive` status/metadata).
//!
//! ## Observability contract (HARD)
//!
//! This route **observes** `assemble_moment` output only — no re-computation,
//! no KB mutation, no writes. The packet is built by the single relocated
//! builder `nexus_moment_context_assembly::build_inspector_packet` (AC-I2
//! reuse; the activation engine is never re-implemented here). The Moment
//! Directive body is never on the wire — the builder reads only
//! `ctx.moment_directive_meta` (AC-I3, body exclusion by construction).
//!
//! ## Directive-store wiring
//!
//! The handler wires the relocated composition root
//! (`nexus_daemon_runtime::directive_store::LocalDirectiveStore`, shared
//! with the CLI and the `POST /v1/daemon/moment-directive` route — T3, DF-76)
//! into `assemble_moment_with_directive`, so the packet's `moment_directive`
//! section reflects an active directive for the scoped World/Work. When no
//! directive is active that path is **byte-equivalent** to the plain
//! `assemble_moment` (AC-I1b) — the section renders `"none"` + nulls.
//!
//! ## Response mapping decision
//!
//! Mirrors the V1.148 `check` route (architect lock): the enriched packet is
//! round-tripped through JSON into the generated `MomentInspectResponse` DTO
//! at the boundary (validates the builder output against the wire contract);
//! any failure surfaces as a 500 `NexusApiError` so the central
//! `IntoResponse` emits the canonical `{ success: false, error: {...} }`
//! envelope. `assemble_moment` degrades per-domain-section (individual
//! domain failures are logged and omitted — it never returns a spoke
//! reject), so there is no 4xx spoke-reject mapping on this path.
//!
//! # Errors
//!
//! Returns 401 when no active creator can be read from config (tier2
//! middleware normally rejects earlier with 409), 403 when the World is not
//! owned by the active creator, 500 when the pool is uninitialized, the
//! ownership check fails at the database, or the builder output does not map
//! onto the `MomentInspectResponse` wire shape.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::directive_store::LocalDirectiveStore;
use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::generated::daemon_api::inspector::{
    moment_inspect_request::MomentInspectRequest,
    moment_inspect_response::MomentInspectResponse,
};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::narrative_write;
use nexus_local_db::SqliteKnowledgeStore;
use nexus_moment_context_assembly::{
    assemble_moment_with_directive, build_inspector_packet, GenerationStage, MomentRequest,
    Stage0Assembly,
};
use nexus_spoke_adapter::SpokeBackedKbStore;

// ── POST /v1/daemon/inspector/moment ─────────────────────────────────────

/// Assemble one moment over an owned World and return the enriched
/// inspector packet (V1.151 P0, DF-76).
///
/// Guard order (plan lock): tier2 middleware (API key + active creator) →
/// world ownership (`is_world_owned`, 403) → `MomentRequest` construction
/// (mirrors the CLI `run_assemble_moment` wiring — creator + world always,
/// work + generation stage when present) → plain `assemble_moment` over the
/// same persistent stores the CLI uses (`SqliteNarrativeGateway` /
/// `SpokeBackedKbStore` / `SqliteKnowledgeStore`) → enriched packet via the
/// single relocated builder → JSON round-trip onto the generated
/// `MomentInspectResponse` wire DTO.
#[allow(clippy::missing_errors_doc)]
pub async fn inspect_moment(
    State(state): State<WorkspaceState>,
    Json(req): Json<MomentInspectRequest>,
) -> Result<Json<MomentInspectResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Ownership gate (compute_runs / check pattern): a foreign World never
    // leaks assembly behavior — 403, not 404, so world existence stays
    // unobservable to other creators.
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

    // MomentRequest mirroring the CLI `run_assemble_moment` wiring
    // (apps/nexus42/.../context.rs:685-767) minus the CLI-only knobs: the
    // active creator + world are always threaded; work + generation stage
    // only when the request carries them. Stage-0 stays the empty default —
    // this is an observation surface and the packet never renders stage-0
    // content (spec §2: `modules` / `slot_map` / `budget` /
    // `moment_directive`).
    let mut request = MomentRequest::new(Stage0Assembly::default())
        .with_world(req.world_id.as_str())
        .with_creator(&creator_id)
        .with_user(&creator_id);
    if let Some(work_id) = req.work_id.as_deref() {
        request = request.with_work(work_id);
    }
    if let Some(stage) = req.generation_stage.as_ref() {
        // The wire enum is schema-closed (8 variants mapping 1:1 onto
        // GenerationStage), so parse is total here; the `else` arm is
        // defensive against future schema drift — unknown degrades to
        // unspecified (all slots on), matching the CLI (context.rs:736-745).
        if let Some(gs) = GenerationStage::parse(&stage.to_string()) {
            request = request.with_generation_stage(gs);
        } else {
            tracing::warn!(
                stage = %stage,
                "unknown generation stage for inspector moment; treating as unspecified (all slots on)"
            );
        }
    }

    // Four-domain assembly over the same persistent stores the CLI uses
    // (context.rs:643-670) + the relocated directive store (T3, DF-76 — see
    // module docs): the packet's `moment_directive` section reflects an
    // active directive; none active ⇒ byte-equivalent to plain
    // `assemble_moment` (AC-I1b). Per-domain failures degrade to omitted
    // sections, they never reject.
    let narrative = SqliteNarrativeGateway::new(pool.clone());
    let kb = SpokeBackedKbStore::new(pool.clone());
    let knowledge = SqliteKnowledgeStore::new(pool.clone());
    let directives = LocalDirectiveStore::new(pool.clone());
    let ctx =
        assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives).await;

    // Enriched packet → generated wire DTO via JSON round-trip at the
    // boundary (mirrors run_check's response mapping; validates the builder
    // output against the wire contract — `moment_directive` carries
    // status/metadata only, AC-I3).
    let packet = build_inspector_packet(&ctx);
    let resp: MomentInspectResponse =
        serde_json::from_value(packet).map_err(|e| NexusApiError::Internal {
            code: "INSPECTOR_PACKET_DECODE".to_string(),
            message: format!(
                "build_inspector_packet output did not match the MomentInspectResponse \
                 wire shape: {e}"
            ),
        })?;
    Ok(Json(resp))
}
