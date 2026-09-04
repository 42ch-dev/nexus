//! Context Command — `nexus42 platform context assemble` and `assemble-moment`.
//!
//! V1.28: `assemble-moment` is the single local assembly SSOT.
//! Stage0 / degradation / optional two-stage behavior are flags on that command.
//!
//! The `assemble` (platform) subcommand is **deferred** — it prints a guidance message
//! and exits with code 2.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::local::domain::RuntimeMode;
use nexus_moment_context_assembly::cloud_stage::{AssembleResponse, AssemblyRuntimeMode};
use nexus_moment_context_assembly::{
    assemble_moment_with_directive, build_inspector_packet, GenerationStage, MomentContext,
    MomentRequest, Stage0Assembly, TwoStageAssembly,
};

use crate::domain::{DegradationGuard, DomainRuntimeMode};

// Four-domain Moment assembly — persistent knowledge store
// (narrative, KB, and knowledge all use persistent stores from nexus-local-db)
use nexus_local_db::SqliteKnowledgeStore;

#[cfg(test)]
use crate::domain::DegradationPolicy;
#[cfg(test)]
use nexus_moment_context_assembly::cloud_stage::{
    AssembleMetadata, MemoryItemRef, TimelineEventRef,
};

/// Validate `WorldId` format: must start with 'wld_' followed by alphanumeric characters.
///
/// # Errors
///
/// Returns an error string if:
/// - The input does not start with 'wld_' prefix
/// - The suffix after 'wld_' is empty
/// - The suffix contains non-alphanumeric characters
pub fn validate_world_id(s: &str) -> std::result::Result<String, String> {
    // Check prefix
    if !s.starts_with("wld_") {
        return Err(format!("WorldId must start with 'wld_' prefix (got '{s}')"));
    }

    // Check that there's content after prefix
    let suffix = &s[4..]; // Skip "wld_" prefix (4 chars)
    if suffix.is_empty() {
        return Err("WorldId must have alphanumeric characters after 'wld_' prefix".to_string());
    }

    // Check that suffix contains only alphanumeric characters
    if !suffix.chars().all(char::is_alphanumeric) {
        return Err(format!(
            "WorldId must contain only alphanumeric characters after 'wld_' prefix (got '{suffix}')"
        ));
    }

    // Return the validated string
    Ok(s.to_string())
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble context for a world via future direct platform context API
    Assemble {
        /// World ID (required for context assembly, format: wld_[a-zA-Z0-9]+)
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,

        /// Workspace ID (defaults to current workspace)
        #[arg(long)]
        workspace_id: Option<String>,

        /// Creator ID (defaults to active creator)
        #[arg(long)]
        creator_id: Option<String>,

        /// Include memory items in assembled context
        #[arg(long, default_value_t = true)]
        include_memory: bool,

        /// Include timeline events in assembled context
        #[arg(long, default_value_t = true)]
        include_timeline: bool,

        /// Include story summaries in assembled context
        #[arg(long, default_value_t = true)]
        include_story_summaries: bool,

        /// Maximum number of recent timeline events (null = platform default)
        #[arg(long)]
        max_timeline_events: Option<u64>,

        /// Maximum number of story summaries (null = platform default)
        #[arg(long)]
        max_story_summaries: Option<u64>,

        /// Maximum file size in bytes for summary generation (null = no limit)
        #[arg(long)]
        max_file_size: Option<u64>,

        /// Output file path (default: stdout as JSON)
        #[arg(long)]
        output_file: Option<String>,
    },

    /// Assemble four-domain Moment context from local persistent stores (SSOT)
    AssembleMoment {
        /// World ID to include in Moment context
        #[arg(long)]
        world_id: Option<String>,

        /// Work ID for the work-bound moment (V1.150 P1 — Moment Directive
        /// scope resolution + chapter-advance TTL)
        #[arg(long)]
        work_id: Option<String>,

        /// User ID for knowledge lookup
        #[arg(long)]
        user_id: Option<String>,

        /// Branch ID within the world
        #[arg(long)]
        branch_id: Option<String>,

        /// Event ID to focus context around
        #[arg(long)]
        event_id: Option<String>,

        /// Cross-domain token budget (approximate chars/4 heuristic)
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Exclude fragment keywords section (default: include fragments)
        #[arg(long)]
        no_fragments: bool,

        /// Optional prompt hint for Stage0 / two-stage assembly
        #[arg(long)]
        hint: Option<String>,

        /// Maximum number of KB knowledge entries to return
        #[arg(long)]
        kb_limit: Option<usize>,

        /// Text search filter for KB query (case-insensitive substring)
        #[arg(long)]
        kb_search: Option<String>,

        /// Filter KB by block type (character, ability, scene, etc.)
        #[arg(long)]
        kb_type: Option<String>,

        /// Maximum number of user knowledge entries to return (default: 20)
        #[arg(long, default_value_t = 20)]
        knowledge_limit: usize,

        /// Emit diagnostic inspector packet JSON (lore activation is on by default; `NEXUS_MCA_LORE_ACTIVATION=off` disables)
        #[arg(long)]
        emit_packet: bool,

        /// Render a human-readable inspector view of the enriched packet
        /// (activation trace / slot map / budget / moment directive sections).
        /// Observation only — never changes assembled output (AC-I6).
        /// Mutually exclusive with --emit-packet.
        #[arg(long, conflicts_with = "emit_packet")]
        inspect: bool,

        /// Write inspector packet JSON to file instead of stdout
        #[arg(long)]
        packet_out: Option<String>,

        /// Generation stage for spec §4 slot gating (V1.150 P2):
        /// `intake|research|produce|review|persist|work_maintenance|system_maintenance|unspecified`.
        /// Default (absent) = unspecified = all slots on (current behavior,
        /// inspector path). The preset runner / schedule threads the
        /// executing stage through this flag (see
        /// `.mstar/iterations/v1.150/guides/generation-trigger-wiring.md`).
        #[arg(long)]
        stage: Option<String>,
    },
}

/// Run context command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Context assembly fails (platform API errors, file I/O errors)
/// - Degradation guard checks fail
/// - Configuration cannot be loaded
#[allow(clippy::too_many_lines)] // CLI dispatch arm — param plumbing (V1.150 P1 directive summary)
pub async fn run(cmd: ContextCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ContextCommand::Assemble {
            world_id: _,
            workspace_id: _,
            creator_id: _,
            include_memory: _,
            include_timeline: _,
            include_story_summaries: _,
            max_timeline_events: _,
            max_story_summaries: _,
            max_file_size: _,
            output_file: _,
        } => {
            eprintln!("Platform cloud context assembly is not yet available.");
            eprintln!("Use `assemble-moment` for local four-domain Moment assembly.");
            std::process::exit(2);
        }
        ContextCommand::AssembleMoment {
            world_id,
            work_id,
            user_id,
            branch_id,
            event_id,
            max_tokens,
            no_fragments,
            hint,
            kb_limit,
            kb_search,
            kb_type,
            knowledge_limit,
            emit_packet,
            inspect,
            packet_out,
            stage,
        } => {
            let maybe_ctx = run_assemble_moment(
                config,
                world_id.as_deref(),
                work_id.as_deref(),
                user_id.as_deref(),
                branch_id.as_deref(),
                event_id.as_deref(),
                max_tokens,
                !no_fragments,
                hint.as_deref(),
                kb_limit,
                kb_search.as_deref(),
                kb_type.as_deref(),
                Some(knowledge_limit),
                emit_packet,
                inspect,
                packet_out.as_deref(),
                stage.as_deref(),
            )
            .await?;

            // None means --emit-packet already wrote JSON to stdout;
            // skip normal context output.
            let Some(ctx) = maybe_ctx else {
                return Ok(());
            };

            // Print full context to stdout
            println!("{}", ctx.to_full_context());

            // Print summary to stderr
            eprintln!("\n--- Moment Assembly (local persistent) ---");
            eprintln!("Stage-0: present");
            eprintln!(
                "World state: {}",
                if ctx.world_state.is_some() {
                    "present"
                } else {
                    "absent"
                }
            );
            eprintln!(
                "Timeline: {}",
                if ctx.timeline.is_some() {
                    "present"
                } else {
                    "absent"
                }
            );
            eprintln!(
                "World KB: {}",
                if ctx.world_kb.is_some() {
                    "present"
                } else {
                    "absent"
                }
            );
            eprintln!(
                "Moment Directive: {}",
                if ctx.moment_directive.is_some() {
                    "present"
                } else {
                    "absent"
                }
            );
            eprintln!(
                "User knowledge: {}",
                if ctx.user_knowledge.is_some() {
                    "present"
                } else {
                    "absent"
                }
            );

            Ok(())
        }
    }
}

/// Create a `DegradationGuard` from config, restoring from persisted snapshot if available.
#[must_use]
pub fn create_degradation_guard(config: &CliConfig) -> DegradationGuard {
    let mode = config.runtime_mode();

    // If snapshot exists with non-Normal state, restore directly without
    // replaying failures (avoids unintended re-degradation — C-001).
    if let Some(snap) = config.degradation_snapshot() {
        if snap.state != crate::domain::degradation::DegradationState::Normal {
            return DegradationGuard::restore_from_snapshot(snap, mode);
        }
        // Normal state with failures: replay to restore failure_count only
        let mut guard = DegradationGuard::with_defaults(mode);
        for _ in 0..snap.failure_count {
            guard.record_platform_result(false, None);
        }
        return guard;
    }

    DegradationGuard::with_defaults(mode)
}

/// Persist degradation guard state to config.
fn save_degradation_guard(config: &mut CliConfig, guard: &DegradationGuard) -> Result<()> {
    use crate::domain::DegradationSnapshot;
    config.degradation_snapshot = Some(DegradationSnapshot::from_guard(guard));
    config.save()?;
    Ok(())
}

/// Mode-aware context assembly with degradation support (T6.7 + T6.8).
///
/// Routes by `runtime_mode`:
/// - `local_only` → `Stage0Assembly` directly
/// - `local_first` / `cloud_enhanced` → `TwoStageAssembly` with fallback to Stage0
///
/// Records platform results in `DegradationGuard` for degradation tracking.
///
/// This function is the internal routing core, shared between `assemble-moment`
/// and future platform assembly paths. It is not wired directly to any single
/// CLI subcommand — callers build the request and invoke it as needed.
#[allow(dead_code)]
async fn assemble_local_with_routing(
    config: &CliConfig,
    max_tokens: Option<usize>,
    include_fragments: bool,
    hint: Option<&str>,
) -> Result<()> {
    let mut guard = create_degradation_guard(config);
    let context = assemble_context(config, &mut guard, hint, max_tokens, include_fragments).await?;

    // Display degradation state if degraded
    if guard.degradation_state().is_degraded() {
        eprintln!(
            "⚠ Degradation: {} (failures: {})",
            guard.degradation_state().display_label(),
            guard.failure_count()
        );
    }

    println!("{context}");

    // Persist degradation state
    let mut cfg = config.clone();
    save_degradation_guard(&mut cfg, &guard)?;

    Ok(())
}

/// Core routing function for context assembly.
///
/// Dispatches to the appropriate assembly strategy based on the current
/// runtime mode (which may have been downgraded by the degradation guard).
/// In non-local-only modes, attempts a platform call and falls back to
/// `Stage0Assembly` if the platform is unavailable.
///
/// # Errors
///
/// Returns an error if:
/// - Local file scanning or reading fails during Stage-0 assembly
/// - Configuration cannot be accessed
/// - Degradation guard state cannot be persisted
pub async fn assemble_context(
    config: &CliConfig,
    guard: &mut DegradationGuard,
    hint: Option<&str>,
    max_tokens: Option<usize>,
    include_fragments: bool,
) -> Result<String> {
    let mode = *guard.current_mode();

    match mode.inner() {
        RuntimeMode::LocalOnly => {
            // Stage-0 only — no platform dependency
            let stage0 =
                build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
            Ok(if max_tokens.is_some() {
                stage0.assemble_with_truncation()
            } else {
                stage0.assemble()
            })
        }
        RuntimeMode::LocalFirst | RuntimeMode::CloudEnhanced => {
            // Two-stage: try platform, fallback to Stage0
            let platform_result = try_platform_assemble(config, hint);

            if let Some(response) = platform_result {
                guard.record_platform_result(true, None);
                let stage0 =
                    build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
                let two_stage = build_two_stage_from_local(&stage0, response, mode);
                Ok(two_stage.assemble())
            } else {
                // Platform failed — record and fall back to Stage0
                guard.record_platform_result(false, Some("assemble unavailable".to_string()));
                let stage0 =
                    build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
                Ok(if max_tokens.is_some() {
                    stage0.assemble_with_truncation()
                } else {
                    stage0.assemble()
                })
            }
        }
    }
}

/// Build a `Stage0Assembly` from local sources (SOUL.md, memories, fragments).
async fn build_stage0_from_local(
    config: &CliConfig,
    hint: Option<&str>,
    max_tokens: Option<usize>,
    include_fragments: bool,
) -> Result<Stage0Assembly> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 system identity use <id>` first.".to_string(),
        )
    })?;

    let home = crate::config::user_home_dir()?;

    // 1. Load SOUL.md
    let soul = nexus_creator_memory::soul_io::load(&home, creator_id)?;

    // 2. List long-term memories (skip personality_core — already in SOUL personality)
    let slugs = nexus_creator_memory::memory_io::list_memories(&home, creator_id)?;
    let mut long_term_memories = Vec::new();
    for slug in &slugs {
        if let Ok(mem) = nexus_creator_memory::memory_io::load_memory(&home, creator_id, slug) {
            if mem.frontmatter.memory_kind == "personality_core" {
                continue;
            }
            long_term_memories.push(mem);
        }
    }

    // 3. Build fragment keywords (best-effort from daemon, optional)
    let fragment_keywords = if include_fragments {
        collect_fragment_keywords(config).await
    } else {
        Vec::new()
    };

    // 4. Build Stage0Assembly
    Ok(Stage0Assembly {
        personality: soul.personality.clone().unwrap_or_default(),
        experience: soul.experience.clone().unwrap_or_default(),
        long_term_memories,
        fragment_keywords,
        system_prefix: String::new(),
        user_prompt: hint.unwrap_or("").to_string(),
        max_tokens,
    })
}

/// Try to obtain a platform assemble response (best-effort).
///
/// KCA-002 B2: The daemon context-assemble proxy route is retired.
/// This function is a placeholder for future direct platform API integration.
/// Currently always returns `None`, causing the caller to fall back to
/// in-process `Stage0Assembly`.
///
/// When the direct platform context API becomes available, this function
/// should be updated to call it directly (not via daemon proxy).
fn try_platform_assemble(_config: &CliConfig, _hint: Option<&str>) -> Option<AssembleResponse> {
    // KCA-002 B2: Daemon proxy route retired.
    // Future: call platform context API directly when available.
    tracing::debug!(
        "Platform context API not yet wired for direct call; using in-process Stage0 assembly"
    );
    None
}

/// Build a `TwoStageAssembly` from local context data and a platform response.
fn build_two_stage_from_local(
    local: &Stage0Assembly,
    platform_response: AssembleResponse,
    mode: DomainRuntimeMode,
) -> TwoStageAssembly {
    TwoStageAssembly {
        stage1_response: Some(platform_response),
        personality: local.personality.clone(),
        experience: local.experience.clone(),
        long_term_memories: local.long_term_memories.clone(),
        fragment_keywords: local.fragment_keywords.clone(),
        user_prompt: local.user_prompt.clone(),
        system_prefix: local.system_prefix.clone(),
        max_tokens: local.max_tokens,
        runtime_mode: AssemblyRuntimeMode::new(*mode.inner()),
    }
}

/// Best-effort collection of fragment keywords from the daemon.
/// Returns empty vec if daemon is unavailable or endpoint doesn't exist (404).
async fn collect_fragment_keywords(config: &CliConfig) -> Vec<String> {
    let url = format!("{}/v1/daemon/memory/fragments", config.daemon_url);

    let response = match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::debug!(error = %e, "Failed to reach daemon for fragment keywords");
            return Vec::new();
        }
    };

    // Gracefully handle 404 (endpoint may not be implemented yet)
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::debug!("Daemon does not implement /v1/daemon/memory/fragments, skipping");
        return Vec::new();
    }

    if !response.status().is_success() {
        tracing::debug!(
            status = %response.status(),
            "Unexpected status from fragment keywords endpoint"
        );
        return Vec::new();
    }

    let data: serde_json::Value = match response.json().await {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = %e, "Failed to parse fragment keywords response");
            return Vec::new();
        }
    };

    // Extract keywords from fragment records
    let mut keywords = Vec::new();
    if let Some(fragments) = data.get("fragments").and_then(|v| v.as_array()) {
        for fragment in fragments {
            if let Some(kws) = fragment.get("keywords").and_then(|v| v.as_array()) {
                for kw in kws {
                    if let Some(s) = kw.as_str() {
                        keywords.push(s.to_string());
                    }
                }
            }
        }
    }

    keywords
}

// ── V1.26: Four-domain Moment assembly (persistent stores) ──────────

/// Open a shared `SqlitePool` for persistent stores.
async fn open_shared_pool(config: &CliConfig) -> Result<sqlx::SqlitePool> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(pool)
}

/// V1.149 P0 T2: lore activation is DEFAULT-ON. Reads the env off-switch
/// `NEXUS_MCA_LORE_ACTIVATION` (`off|0|false`, case-insensitive, trimmed)
/// that restores V1.146 flag-off semantics; any other value — including
/// unset or empty — keeps activation on (spec §6). Extracted for unit
/// testing (P0 fix wave, QC F-002).
fn lore_activation_env_is_off() -> bool {
    std::env::var("NEXUS_MCA_LORE_ACTIVATION").is_ok_and(|v| lore_activation_value_is_off(&v))
}

/// Parse a single `NEXUS_MCA_LORE_ACTIVATION` value: `off` / `0` / `false`
/// (case-insensitive, trimmed) → off; anything else → on.
fn lore_activation_value_is_off(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "off" | "0" | "false"
    )
}

/// Run four-domain Moment assembly using persistent narrative + KB + knowledge stores.
///
/// Uses `SqliteNarrativeGateway`, `SqliteKbStore`, and `SqliteKnowledgeStore`
/// from `nexus-local-db` for all four domain slices.
///
/// Returns `None` when `--emit-packet` writes diagnostic JSON to stdout or
/// `--inspect` renders the readable inspector view (normal context output
/// should be suppressed). Returns `Some(ctx)` when normal context output
/// should proceed.
///
/// # Errors
///
/// Returns `CliError` if the database cannot be opened or migrations fail.
#[allow(clippy::future_not_send)]
#[allow(clippy::fn_params_excessive_bools)]
#[allow(clippy::too_many_arguments)] // CLI param plumbing — acceptable until refactored into builder
#[allow(clippy::too_many_lines)] // CLI param plumbing + flag tail — same builder-refactor path
pub async fn run_assemble_moment(
    config: &CliConfig,
    world_id: Option<&str>,
    work_id: Option<&str>,
    user_id: Option<&str>,
    branch_id: Option<&str>,
    event_id: Option<&str>,
    max_tokens: Option<usize>,
    include_fragments: bool,
    hint: Option<&str>,
    kb_limit: Option<usize>,
    kb_search: Option<&str>,
    kb_type: Option<&str>,
    knowledge_limit: Option<usize>,
    emit_packet: bool,
    inspect: bool,
    packet_out: Option<&str>,
    stage: Option<&str>,
) -> Result<Option<MomentContext>> {
    // V1.149 P0 T2: activation is DEFAULT-ON. The env off-switch
    // (NEXUS_MCA_LORE_ACTIVATION=off|0|false, case-insensitive) restores
    // V1.146 flag-off semantics; any other value (incl. unset, empty, =1)
    // keeps activation on (spec §6).
    let activation_off = lore_activation_env_is_off();
    if emit_packet && activation_off {
        return Err(crate::errors::CliError::Other(
            "--emit-packet requires lore activation. Activation is on by default; \
             unset the NEXUS_MCA_LORE_ACTIVATION off-switch (off/0/false) to keep it enabled."
                .to_string(),
        ));
    }

    let pool = open_shared_pool(config).await?;
    let narrative = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
    // V1.145 P2 — the MCA WorldKB read now crosses the spoke-adapter boundary:
    // `SpokeBackedKbStore` routes `query` through `NexusAdapter`'s
    // scoped read (storage → spoke `KnowledgeEntry` → `KnowledgeEntryRecord` via the
    // `spoke_to_knowledge_record` conversion seam), matching `SqliteKbStore::query`
    // behavior exactly (silent 500-row window; no reject-on-overflow).
    let kb = nexus_spoke_adapter::SpokeBackedKbStore::new(pool.clone());
    // V1.149 P1: preload the world's confirmed relation edges for relation-hop
    // expansion when activation is on (off-switch ⇒ no hop load; spec §6).
    // The edge source is the inherent `NexusAdapter::list_hop_edges_for_world`
    // — spoke `RelationPort` is get/put only, so the storage list primitive
    // (`list_confirmed_relationships_paginated`, confirmed graph) backs the
    // loader. A storage-read failure degrades to activation-only (no hop
    // pass), consistent with `assemble_moment`'s per-section degradation.
    // The default call returns the first page of up to
    // `HOP_EDGE_LIST_LIMIT` (10_000) newest edges; a caller that needs the
    // full graph walks it with `list_hop_edges_for_world_paginated` (V1.158
    // P2 T3, R-V1149P1-001).
    let wid = world_id.unwrap_or("wld_default");
    let hop_edges = if activation_off {
        None
    } else {
        nexus_spoke_adapter::adapter::NexusAdapter::new(pool.clone())
            .list_hop_edges_for_world(wid)
            .await
            .ok()
            .filter(|edges| !edges.is_empty())
    };
    let uid = user_id.unwrap_or("user_default");
    let knowledge = SqliteKnowledgeStore::new(pool.clone());

    // Build Stage0Assembly — load from creator memory if available
    let stage0 = build_stage0_from_local(config, hint, max_tokens, include_fragments)
        .await
        .unwrap_or_else(|_| Stage0Assembly {
            personality: "Local Moment assembly.".to_string(),
            experience: "Four-domain context from persistent stores.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            system_prefix: String::new(),
            user_prompt: "Moment context assembly.".to_string(),
            max_tokens,
        });

    // Build MomentRequest with KB query + budget fields
    let mut request = MomentRequest::new(stage0).with_world(wid).with_user(uid);

    // V1.150 P1 (DF-75): Moment Directive scope resolution is keyed on the
    // creator + work of the moment (spec §3.2). The active creator is always
    // threaded through; the work is optional — a raw world assembly applies
    // the World override directly.
    if let Some(cid) = config.active_creator_id.as_deref() {
        request = request.with_creator(cid);
    }
    if let Some(wid) = work_id {
        request = request.with_work(wid);
    }
    if let Some(eid) = event_id {
        request = request.with_event(eid);
    }
    if let Some(bid) = branch_id {
        request = request.with_branch(bid);
    }
    if let Some(mt) = max_tokens {
        request = request.with_max_tokens(mt);
    }
    if let Some(limit) = kb_limit {
        request = request.with_kb_limit(limit);
    }
    if let Some(search) = kb_search {
        request = request.with_kb_text_search(search);
    }
    if let Some(bt_str) = kb_type {
        if let Ok(bt) = serde_json::from_value::<nexus_contracts::BlockType>(
            serde_json::Value::String(bt_str.to_string()),
        ) {
            request = request.with_kb_block_type(bt);
        }
    }
    if let Some(limit) = knowledge_limit {
        request = request.with_knowledge_limit(limit);
    }

    // V1.149 P0 T2: `MomentRequest` defaults to activation ON; only the
    // off-switch needs an explicit call here.
    if activation_off {
        request = request.with_activation_enabled(false);
    }

    // V1.150 P2 (DF-75, spec §4 / Q4 lock): generation-stage wire. The
    // preset runner / schedule path drives assembly through this CLI entry
    // and threads the stage it is executing via `--stage` (see
    // `guides/generation-trigger-wiring.md`). Unknown values degrade to
    // unspecified (all slots on) with a warning — an unknown stage must
    // never fail or panic the inspector path (T3 safe default).
    if let Some(stage_str) = stage {
        if let Some(stage) = GenerationStage::parse(stage_str) {
            request = request.with_generation_stage(stage);
        } else {
            tracing::warn!(
                stage = %stage_str,
                "unknown generation stage for assemble-moment; treating as unspecified (all slots on)"
            );
        }
    }

    // V1.149 P1: relation-hop expand — pass the preloaded edges and the
    // caller hop cap. `hop_max_tokens` is set to the full cross-domain
    // budget; MCA refines it to the spec Q1 remainder (personality never
    // truncated + world_state + timeline + primary-KB reservations) at the
    // call site (see `hop_budget_tokens` in nexus-moment-context-assembly).
    if let Some(edges) = hop_edges {
        request = request.with_hop_edges(edges);
        if let Some(mt) = max_tokens {
            request = request.with_hop_max_tokens(mt);
        }
    }

    // Call assemble_moment with persistent stores. V1.150 P1: the Moment
    // Directive store (composition-root adapter over the same pool) is wired
    // here — an active directive renders into the reserved `moment.directive`
    // slot and its TTL / scene-change lifecycle runs. When no directive is
    // active, `assemble_moment_with_directive` is byte-equivalent to the
    // plain `assemble_moment` (AC-I1b).
    let directives = nexus_daemon_runtime::directive_store::LocalDirectiveStore::new(pool);
    let ctx =
        assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives).await;

    // V1.146 P4 T3: emit diagnostic inspector packet when --emit-packet is set.
    if emit_packet {
        emit_inspector_packet(&ctx, packet_out)?;
        if packet_out.is_some() {
            // JSON written to file; normal context output should follow.
            Ok(Some(ctx))
        } else {
            // JSON written to stdout; suppress normal context output.
            Ok(None)
        }
    } else if inspect {
        // V1.151 P0 T4: --inspect renders the same enriched packet (T1's
        // relocated builder) as a human-readable view. Observation only —
        // the assembled output is untouched (AC-I6). Mirrors the
        // --emit-packet stdout suppression: the view replaces the context.
        let packet = build_inspector_packet(&ctx);
        println!("{}", render_inspector_readable(&packet));
        Ok(None)
    } else {
        Ok(Some(ctx))
    }
}

/// Build and emit the inspector packet diagnostic JSON (V1.151 P0: the
/// enriched packet — `modules` unchanged + `slot_map` / `budget` /
/// `moment_directive` product-local sections). The builder relocated to MCA
/// (`nexus_moment_context_assembly::build_inspector_packet`) so the daemon
/// route and CLI share one implementation.
fn emit_inspector_packet(ctx: &MomentContext, packet_out: Option<&str>) -> Result<()> {
    let packet = build_inspector_packet(ctx);

    let json_str = serde_json::to_string_pretty(&packet).map_err(|e| {
        crate::errors::CliError::Other(format!("Failed to serialize inspector packet: {e}"))
    })?;

    if let Some(path) = packet_out {
        std::fs::write(path, format!("{json_str}\n")).map_err(|e| {
            crate::errors::CliError::Other(format!("Failed to write packet to {path}: {e}"))
        })?;
        eprintln!("Inspector packet written to {path}");
    } else {
        println!("{json_str}");
    }

    Ok(())
}

/// Render the enriched inspector packet (V1.151 P0 T4) as a human-readable
/// view for `assemble-moment --inspect`.
///
/// Reads the same `build_inspector_packet` JSON that `--emit-packet` writes
/// and formats four sections — activation trace (✓ fired / ✗ missed with the
/// human-readable reason; fired entries additionally show their slot from
/// `slot_map`, plus hop depth/origin when the packet carries them), slot map,
/// budget (chars/4 estimates), and the Moment Directive (status/metadata
/// only — the packet never carries the body, AC-I3). Observation only: the
/// assembled output is never modified (AC-I6).
#[allow(clippy::too_many_lines)] // one block per readable section — keeps sections scannable
fn render_inspector_readable(packet: &serde_json::Value) -> String {
    use std::collections::HashMap;
    use std::fmt::Write as _;

    let mut out = String::new();

    // ── Activation trace ──────────────────────────────────────────────
    out.push_str("## Activation trace\n");
    let trace = packet["modules"]["activation_trace"].as_array();
    let slot_by_entry: HashMap<&str, &str> = packet["slot_map"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| Some((e.get("entry_id")?.as_str()?, e.get("slot")?.as_str()?)))
        .collect();
    match trace {
        Some(entries) if !entries.is_empty() => {
            for entry in entries {
                let name = entry["canonical_name"].as_str().unwrap_or("?");
                let id = entry["entry_id"].as_str().unwrap_or("?");
                let reason = entry["reason"].as_str().unwrap_or("");
                if entry["accepted"].as_bool().unwrap_or(false) {
                    writeln!(out, "  ✓ {name}  ({id})").ok();
                    if let Some(slot) = slot_by_entry.get(id) {
                        writeln!(out, "      slot: {slot}").ok();
                    }
                    // Hop depth / origin only when the packet carries them
                    // (not emitted today; forward-compatible, spec §2 H4).
                    if let Some(depth) = entry.get("hop_depth").and_then(serde_json::Value::as_u64)
                    {
                        writeln!(out, "      hop depth: {depth}").ok();
                    }
                    if let Some(origin) = entry
                        .get("hop_origin_entry_id")
                        .and_then(serde_json::Value::as_str)
                    {
                        writeln!(out, "      hop origin: {origin}").ok();
                    }
                } else {
                    writeln!(out, "  ✗ {name}  ({id})").ok();
                }
                writeln!(out, "      reason: {reason}").ok();
            }
        }
        _ => out.push_str("  (no activation trace entries)\n"),
    }

    // ── Slot map ──────────────────────────────────────────────────────
    out.push_str("\n## Slot map\n");
    match packet["slot_map"].as_array() {
        Some(entries) if !entries.is_empty() => {
            for entry in entries {
                let id = entry["entry_id"].as_str().unwrap_or("?");
                let slot = entry["slot"].as_str().unwrap_or("?");
                writeln!(out, "  {id} → {slot}").ok();
            }
        }
        _ => out.push_str("  (empty)\n"),
    }

    // ── Budget ────────────────────────────────────────────────────────
    out.push_str("\n## Budget\n");
    let budget = &packet["budget"];
    let est = |key: &str| budget[key].as_u64().unwrap_or(0);
    let cap = budget["cap"]
        .as_u64()
        .map_or_else(|| "none".to_string(), |v| v.to_string());
    let remaining = budget["remaining"]
        .as_u64()
        .map_or_else(|| "none".to_string(), |v| v.to_string());
    writeln!(
        out,
        "  primary tokens est: {} (chars/4)",
        est("primary_tokens_est")
    )
    .ok();
    writeln!(out, "  hop tokens est: {} (chars/4)", est("hop_tokens_est")).ok();
    writeln!(out, "  cap: {cap}").ok();
    writeln!(out, "  remaining: {remaining}").ok();

    // ── Moment Directive ──────────────────────────────────────────────
    out.push_str("\n## Moment Directive\n");
    let directive = &packet["moment_directive"];
    let status = directive["status"].as_str().unwrap_or("none");
    if status == "none" {
        out.push_str("  status: none (no active directive)\n");
    } else {
        let scope = directive["scope"].as_str().unwrap_or("?");
        let scope_id = directive["scope_id"].as_str().unwrap_or("?");
        writeln!(out, "  scope: {scope} ({scope_id})").ok();
        writeln!(
            out,
            "  insert depth: {}",
            directive["insert_depth"].as_str().unwrap_or("?")
        )
        .ok();
        let ttl_kind = directive["ttl_kind"].as_str().unwrap_or("?");
        match directive["ttl_remaining"].as_u64() {
            Some(remaining_ttl) => {
                writeln!(out, "  ttl: {ttl_kind} ({remaining_ttl} remaining)").ok()
            }
            None => writeln!(out, "  ttl: {ttl_kind}").ok(),
        };
        writeln!(
            out,
            "  clear on scene change: {}",
            directive["clear_on_scene_change"]
                .as_bool()
                .unwrap_or(false)
        )
        .ok();
        writeln!(out, "  status: {status}").ok();
    }

    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    // Import activation types for inspector packet tests.
    use nexus_moment_context_assembly::assemble_moment;
    use nexus_spoke_adapter::adapter::activation::ActivationTraceEntry;

    /// Test valid `WorldId` formats
    #[test]
    fn validate_world_id_accepts_valid_formats() {
        // Valid: starts with wld_ followed by alphanumeric
        assert!(validate_world_id("wld_abc123").is_ok());
        assert!(validate_world_id("wld_test").is_ok());
        assert!(validate_world_id("wld_ABCDEF123456").is_ok());
        assert!(validate_world_id("wld_1").is_ok());
    }

    /// Test invalid `WorldId` formats - missing prefix
    #[test]
    fn validate_world_id_rejects_missing_prefix() {
        let result = validate_world_id("abc123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid `WorldId` formats - wrong prefix
    #[test]
    fn validate_world_id_rejects_wrong_prefix() {
        let result = validate_world_id("world_123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid `WorldId` formats - empty
    #[test]
    fn validate_world_id_rejects_empty() {
        let result = validate_world_id("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid `WorldId` formats - special characters
    #[test]
    fn validate_world_id_rejects_special_characters() {
        let result = validate_world_id("wld_test-123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));

        let result = validate_world_id("wld_test@123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));
    }

    /// Test invalid `WorldId` formats - only prefix
    #[test]
    fn validate_world_id_rejects_only_prefix() {
        let result = validate_world_id("wld_");
        assert!(result.is_err());
        assert!(result
            .expect_err("validation should fail")
            .contains("alphanumeric characters"));
    }

    /// L3.1: `AssembleLocal` variant no longer exists in `ContextCommand`.
    #[test]
    fn context_command_no_assemble_local() {
        let source = include_str!("context.rs");
        // Strip test module to avoid false positives
        let non_test = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            !non_test.contains("AssembleLocal"),
            "context.rs non-test code must not contain AssembleLocal"
        );
        assert!(
            !non_test.contains("assemble-local"),
            "context.rs non-test code must not reference assemble-local subcommand"
        );
    }

    /// C1.1: `AssembleMoment` variant exists with all V1.28 flags.
    #[test]
    fn context_command_assemble_moment_exists_with_flags() {
        let _ = ContextCommand::AssembleMoment {
            world_id: Some("wld_test".to_string()),
            work_id: Some("wrk_test".to_string()),
            user_id: Some("user_test".to_string()),
            branch_id: None,
            event_id: None,
            max_tokens: Some(1000),
            no_fragments: true,
            hint: Some("write chapter 3".to_string()),
            kb_limit: Some(5),
            kb_search: Some("hero".to_string()),
            kb_type: Some("character".to_string()),
            knowledge_limit: 10,
            emit_packet: true,
            inspect: false,
            packet_out: Some("packet.json".to_string()),
            stage: Some("produce".to_string()),
        };
        let _ = ContextCommand::AssembleMoment {
            world_id: None,
            work_id: None,
            user_id: None,
            branch_id: None,
            event_id: None,
            max_tokens: None,
            no_fragments: false,
            hint: None,
            kb_limit: None,
            kb_search: None,
            kb_type: None,
            knowledge_limit: 20,
            emit_packet: false,
            inspect: true,
            packet_out: None,
            stage: None,
        };
    }

    /// C1.3: Verify `Assemble` arm prints deferred message mentioning `assemble-moment` only.
    #[test]
    fn assemble_arm_deferred_message() {
        let source = include_str!("context.rs");
        // Strip test module to avoid false positives from test assertions
        let non_test = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            !non_test.contains("V1.10"),
            "context.rs non-test code must not reference V1.10"
        );
        assert!(
            source.contains("Platform cloud context assembly is not yet available"),
            "context.rs must contain deferred platform message"
        );
        assert!(
            source.contains("assemble-moment"),
            "deferred message must mention assemble-moment"
        );
        // After L3.1, deferred message must NOT mention assemble-local
        assert!(
            !non_test.contains("assemble-local"),
            "deferred message must not mention assemble-local"
        );
    }

    /// C3.1: Test `run_assemble_moment` with persistent seed data.
    /// Seeds a world and KB block into a fresh `SQLite` DB, then verifies
    /// that `assemble_moment` returns world state and KB sections.
    #[tokio::test]
    async fn assemble_moment_with_persistent_seed() {
        use nexus_local_db::kb_store::seed as kb_seed;
        use nexus_local_db::narrative_gateway::seed as narrative_seed;

        // Create fresh SQLite DB
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        nexus_local_db::seed_versions(&pool).await.unwrap();

        // Seed world + event (narrative_seed::world also seeds the creator for FK)
        narrative_seed::world(
            &pool,
            "wld_test",
            "ctr_test",
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await;
        narrative_seed::event(
            &pool,
            "evt_test_1",
            "wld_test",
            "fbk_root",
            "story_advance",
            1,
        )
        .await;

        // Seed KB block (no need to call kb_seed::world — world already seeded above)
        kb_seed::knowledge_entry(
            &pool,
            "kb_hero",
            "wld_test",
            "Character",
            "Hero",
            "confirmed",
        )
        .await;

        // Build persistent stores from the pool
        let narrative =
            nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
        let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let knowledge = SqliteKnowledgeStore::new(pool.clone());

        // Build request
        let stage0 = Stage0Assembly {
            personality: "Test personality.".to_string(),
            experience: "Test experience.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            system_prefix: String::new(),
            user_prompt: "Test prompt.".to_string(),
            max_tokens: None,
        };
        let request = MomentRequest::new(stage0)
            .with_world("wld_test")
            .with_user("user_test");

        // Run assembly
        let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

        // Verify world state from persistent store
        assert!(
            ctx.world_state.is_some(),
            "world_state should be present from persistent store"
        );
        assert!(
            ctx.world_state.as_ref().unwrap().contains("Test World"),
            "world state should contain seeded title"
        );

        // Verify timeline from persistent store
        assert!(
            ctx.timeline.is_some(),
            "timeline should be present from persistent store"
        );

        // Verify KB from persistent store
        assert!(
            ctx.world_kb.is_some(),
            "world_kb should be present from persistent store"
        );
        assert!(
            ctx.world_kb.as_ref().unwrap().contains("Hero"),
            "KB should contain seeded key block name"
        );

        // Verify full context assembles correctly
        let full = ctx.to_full_context();
        assert!(full.contains("## World State"));
        assert!(full.contains("## Timeline"));
        assert!(full.contains("## World Knowledge Base"));
    }

    /// C3.1: Test `assemble_moment` returns absent sections when no data seeded.
    #[tokio::test]
    async fn assemble_moment_empty_db_returns_absent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        nexus_local_db::seed_versions(&pool).await.unwrap();

        let narrative =
            nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
        let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let knowledge = SqliteKnowledgeStore::new(pool.clone());

        let stage0 = Stage0Assembly {
            personality: "Test.".to_string(),
            experience: "Test.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            system_prefix: String::new(),
            user_prompt: "Test.".to_string(),
            max_tokens: None,
        };
        let request = MomentRequest::new(stage0).with_world("wld_ghost");

        let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

        assert!(
            ctx.world_state.is_none(),
            "world_state should be absent for unknown world"
        );
        assert!(
            ctx.timeline.is_none(),
            "timeline should be absent for unknown world"
        );
        assert!(
            ctx.world_kb.is_none(),
            "world_kb should be absent for unknown world"
        );
    }

    /// C3.1: Persistent E2E — demo seed + assemble-moment, all four domains non-empty.
    ///
    /// Seeds a world, event, KB block, and knowledge entry into a fresh DB,
    /// then runs `assemble_moment` and verifies all four domain sections
    /// are present (world state, timeline, world KB, user knowledge).
    #[tokio::test]
    async fn assemble_moment_persistent_four_domains() {
        use nexus_knowledge::{KnowledgeStore, KnowledgeTag, UserKnowledgeEntry};

        // Create fresh SQLite DB
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        nexus_local_db::seed_versions(&pool).await.unwrap();

        // Seed demo data: world + event + KB block (same as demo seed)
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_demo",
            "ctr_test",
            "Demo World",
            "demo-world",
            "private",
            "manual",
        )
        .await;
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            "evt_demo_1",
            "wld_demo",
            "fbk_root",
            "story_advance",
            0,
        )
        .await;
        nexus_local_db::kb_store::seed::knowledge_entry(
            &pool,
            "kb_demo_hero",
            "wld_demo",
            "Character",
            "Hero",
            "confirmed",
        )
        .await;

        // Seed knowledge entry (persistent)
        let knowledge_store = nexus_local_db::SqliteKnowledgeStore::new(pool.clone());
        let entry = UserKnowledgeEntry::new(
            "user_default",
            vec![
                KnowledgeTag::new("demo"),
                KnowledgeTag::new("worldbuilding"),
            ],
            "Demo knowledge entry for testing Moment context assembly.",
        );
        knowledge_store.store(entry).await.unwrap();

        // Build persistent stores from the pool
        let narrative =
            nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
        let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let knowledge = nexus_local_db::SqliteKnowledgeStore::new(pool);

        // Build request
        let stage0 = Stage0Assembly {
            personality: "Test personality.".to_string(),
            experience: "Test experience.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            system_prefix: String::new(),
            user_prompt: "Test prompt.".to_string(),
            max_tokens: None,
        };
        let request = MomentRequest::new(stage0)
            .with_world("wld_demo")
            .with_user("user_default");

        // Run assembly
        let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

        // Verify ALL four domains are present
        assert!(
            ctx.world_state.is_some(),
            "world_state should be present from persistent store"
        );
        assert!(
            ctx.world_state.as_ref().unwrap().contains("Demo World"),
            "world state should contain demo title"
        );

        assert!(
            ctx.timeline.is_some(),
            "timeline should be present from persistent store"
        );

        assert!(
            ctx.world_kb.is_some(),
            "world_kb should be present from persistent store"
        );
        assert!(
            ctx.world_kb.as_ref().unwrap().contains("Hero"),
            "KB should contain demo key block name"
        );

        assert!(
            ctx.user_knowledge.is_some(),
            "user_knowledge should be present from persistent store"
        );
        assert!(
            ctx.user_knowledge
                .as_ref()
                .unwrap()
                .contains("Demo knowledge"),
            "user knowledge should contain demo entry"
        );

        // Verify full context
        let full = ctx.to_full_context();
        assert!(full.contains("## World State"));
        assert!(full.contains("## Timeline"));
        assert!(full.contains("## World Knowledge Base"));
        assert!(full.contains("## User Knowledge"));
    }

    /// C3.2: Restart test — second invocation sees same knowledge.
    ///
    /// Creates two separate store instances from the same DB file,
    /// simulating a process restart. Verifies knowledge persists.
    #[tokio::test]
    async fn assemble_moment_restart_sees_same_knowledge() {
        use nexus_knowledge::{KnowledgeStore, KnowledgeTag, UserKnowledgeEntry};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        nexus_local_db::seed_versions(&pool).await.unwrap();

        // Process 1: seed knowledge
        let store1 = nexus_local_db::SqliteKnowledgeStore::new(pool);
        let entry = UserKnowledgeEntry::new(
            "user_default",
            vec![KnowledgeTag::new("restart-test")],
            "Knowledge that survives restart.",
        );
        let id = entry.id.clone();
        store1.store(entry).await.unwrap();
        drop(store1);

        // Process 2: read knowledge (simulating restart)
        let pool2 = nexus_local_db::open_pool(&db_path).await.unwrap();
        let store2 = nexus_local_db::SqliteKnowledgeStore::new(pool2);
        let retrieved = store2.get("user_default", &id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().content,
            "Knowledge that survives restart."
        );
    }

    // ── T6.7 / T6.8: Mode-aware routing tests ────────────────────────────

    /// Helper: create a minimal `Stage0Assembly` for routing tests.
    fn make_test_stage0() -> Stage0Assembly {
        Stage0Assembly {
            personality: "Creative and bold.".to_string(),
            experience: "10 years of writing.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: vec!["plot".to_string()],
            system_prefix: String::new(),
            user_prompt: "Write chapter 3.".to_string(),
            max_tokens: None,
        }
    }

    /// Helper: create a platform `AssembleResponse` for routing tests.
    fn make_platform_response() -> AssembleResponse {
        AssembleResponse {
            memory_items: vec![MemoryItemRef {
                memory_id: "mem_platform_1".to_string(),
                content_summary: "Platform memory summary".to_string(),
                relevance_score: Some(0.95),
            }],
            kb: Vec::new(),
            timeline: vec![TimelineEventRef {
                event_id: "evt_1".to_string(),
                event_type: "session_created".to_string(),
                timestamp: "2026-04-14T10:00:00Z".to_string(),
            }],
            metadata: AssembleMetadata {
                assembled_at: "2026-04-14T12:00:00Z".to_string(),
                token_count_estimate: Some(500),
            },
        }
    }

    /// T6.8: `local_only` mode uses `Stage0Assembly` directly.
    #[tokio::test]
    async fn context_assemble_local_only_uses_stage0() {
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        let guard = DegradationGuard::with_defaults(mode);
        let stage0 = make_test_stage0();

        // Build the output using Stage0 directly (simulating what assemble_context does)
        let output = stage0.assemble();

        // Verify Stage0 ordering (no platform sections)
        assert!(output.contains("## Personality"));
        assert!(output.contains("Creative and bold."));
        assert!(output.contains("## Experience"));
        assert!(output.contains("10 years of writing."));
        assert!(output.contains("Write chapter 3."));

        // No platform-specific sections should appear
        assert!(
            !output.contains("### Platform Memory Items"),
            "Stage0 output should not contain platform memory section"
        );
        assert!(
            !output.contains("### Timeline Events"),
            "Stage0 output should not contain timeline section"
        );

        // Guard should remain in Normal state (no platform call attempted)
        assert_eq!(
            guard.degradation_state(),
            crate::domain::degradation::DegradationState::Normal
        );
        assert_eq!(guard.failure_count(), 0);
    }

    /// T6.8: `cloud_enhanced` mode attempts `TwoStageAssembly` with platform data.
    #[tokio::test]
    async fn context_assemble_cloud_enhanced_attempts_two_stage() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);
        let stage0 = make_test_stage0();
        let platform_response = make_platform_response();

        // Simulate successful platform call → TwoStageAssembly
        guard.record_platform_result(true, None);
        let two_stage = build_two_stage_from_local(&stage0, platform_response, mode);
        let output = two_stage.assemble();

        // Verify TwoStage output includes both local and platform data
        assert!(output.contains("## Personality"));
        assert!(output.contains("Creative and bold."));
        assert!(output.contains("### Fragment keywords"));
        assert!(output.contains("plot"));

        // Platform-specific sections should appear
        assert!(
            output.contains("### Platform Memory Items"),
            "TwoStage output should contain platform memory section"
        );
        assert!(
            output.contains("Platform memory summary"),
            "TwoStage output should contain platform memory content"
        );
        assert!(
            output.contains("### Timeline Events"),
            "TwoStage output should contain timeline section"
        );

        // Guard should be in Normal state (platform succeeded)
        assert_eq!(
            guard.degradation_state(),
            crate::domain::degradation::DegradationState::Normal
        );
        assert_eq!(guard.failure_count(), 0);
    }

    /// T6.7: Fallback to Stage0 when platform call fails.
    #[tokio::test]
    async fn context_assemble_fallback_on_platform_failure() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);
        let stage0 = make_test_stage0();

        // Simulate platform failure → record and fall back to Stage0
        guard.record_platform_result(false, Some("assemble unavailable".to_string()));
        let output = stage0.assemble();

        // Verify Stage0 output (fallback)
        assert!(output.contains("## Personality"));
        assert!(output.contains("Creative and bold."));
        assert!(output.contains("## Experience"));

        // No platform-specific sections in fallback
        assert!(
            !output.contains("### Platform Memory Items"),
            "Fallback output should not contain platform memory section"
        );

        // Guard should record the failure
        assert_eq!(guard.failure_count(), 1);
        // Not yet at threshold (default is 3), so still Normal
        assert_eq!(
            guard.degradation_state(),
            crate::domain::degradation::DegradationState::Normal
        );
    }

    /// Degradation triggers after threshold failures.
    #[tokio::test]
    async fn context_assemble_degradation_triggers_after_threshold() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        // Record failures up to threshold
        for i in 0..3 {
            guard.record_platform_result(false, Some(format!("failure {i}")));
        }

        // Should have degraded to level 1
        assert_eq!(
            guard.degradation_state(),
            crate::domain::degradation::DegradationState::DegradedLevel1
        );
        assert_eq!(guard.failure_count(), 0); // reset after degradation

        // Current mode should be downgraded
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalFirst)
        );
    }

    /// `local_first` mode falls back to Stage0 on platform failure.
    #[tokio::test]
    async fn context_assemble_local_first_fallback() {
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalFirst);
        let mut guard = DegradationGuard::with_defaults(mode);

        // Simulate platform failure
        guard.record_platform_result(false, Some("connection refused".to_string()));

        assert_eq!(guard.failure_count(), 1);
        assert_eq!(
            guard.degradation_state(),
            crate::domain::degradation::DegradationState::Normal
        );
    }

    /// `build_two_stage_from_local` produces correct `TwoStageAssembly`.
    #[test]
    fn build_two_stage_preserves_local_data() {
        let stage0 = make_test_stage0();
        let platform = make_platform_response();
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalFirst);

        let two_stage = build_two_stage_from_local(&stage0, platform, mode);

        assert!(two_stage.stage1_response.is_some());
        assert_eq!(two_stage.personality, "Creative and bold.");
        assert_eq!(two_stage.experience, "10 years of writing.");
        assert_eq!(two_stage.fragment_keywords, vec!["plot".to_string()]);
        assert_eq!(two_stage.user_prompt, "Write chapter 3.");
        assert_eq!(
            two_stage.runtime_mode,
            AssemblyRuntimeMode::new(RuntimeMode::LocalFirst)
        );
    }

    /// `create_degradation_guard` restores from persisted snapshot.
    ///
    /// C-001: For non-Normal state, restoration must NOT replay failures
    /// (which would trigger unintended re-degradation).
    #[test]
    fn create_guard_restores_from_snapshot() {
        use crate::domain::degradation::DegradationState;

        // Non-Normal state: should restore directly without replaying failures
        let mut config = CliConfig::default();
        config.runtime_mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        config.degradation_snapshot = Some(crate::domain::DegradationSnapshot {
            state: DegradationState::DegradedLevel1,
            failure_count: 2,
            last_health_check: None,
            last_upgrade_attempt: None,
        });

        let guard = create_degradation_guard(&config);

        // State should be restored directly
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
        assert_eq!(guard.failure_count(), 2);
        // Mode should be downgraded one level from CloudEnhanced
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalFirst)
        );

        // Normal state with failures: should replay to restore failure_count
        let mut config2 = CliConfig::default();
        config2.runtime_mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        config2.degradation_snapshot = Some(crate::domain::DegradationSnapshot {
            state: DegradationState::Normal,
            failure_count: 1,
            last_health_check: None,
            last_upgrade_attempt: None,
        });

        let guard2 = create_degradation_guard(&config2);
        assert_eq!(guard2.degradation_state(), DegradationState::Normal);
        assert_eq!(guard2.failure_count(), 1);
    }

    // ── R2: Workspace slug resolution in try_platform_assemble ─────────

    #[test]
    fn workspace_slug_for_creator_returns_configured_slug() {
        use crate::config::DEFAULT_WORKSPACE_SLUG;
        use std::collections::HashMap;

        let mut config = CliConfig::default();
        config.active_creator_id = Some("ctr_alice".to_string());
        config.active_workspace_slug_by_creator =
            HashMap::from([("ctr_alice".to_string(), "wrk_novel".to_string())]);

        let slug = config.workspace_slug_for_creator("ctr_alice");
        assert_eq!(slug, "wrk_novel");

        // Unknown creator falls back to default
        let slug = config.workspace_slug_for_creator("ctr_unknown");
        assert_eq!(slug, DEFAULT_WORKSPACE_SLUG);
    }

    #[test]
    fn workspace_slug_for_creator_defaults_when_empty() {
        use crate::config::DEFAULT_WORKSPACE_SLUG;

        let config = CliConfig::default();
        let slug = config.workspace_slug_for_creator("ctr_anyone");
        assert_eq!(slug, DEFAULT_WORKSPACE_SLUG);
    }

    // ── V1.150 P1: inspector packet never carries the Moment Directive (AC-I3)

    /// AC-I3 (V1.150 P1, extended V1.151 P0): the inspector packet
    /// (`modules.placement` + `modules.activation_trace` + the enriched
    /// product-local `slot_map` / `budget` / `moment_directive` sections) is
    /// derived only from the activation trace + additive metadata. The
    /// Moment Directive is product-local — its **body** must never appear
    /// anywhere in the packet JSON, including the new `moment_directive`
    /// section (metadata-only by construction, spec §2 H6).
    #[test]
    fn inspector_packet_never_carries_moment_directive() {
        use nexus_moment_context_assembly::directive::{
            DirectiveDepth, DirectiveTtlKind, MomentDirectiveScope,
        };
        use nexus_moment_context_assembly::MomentDirectiveStatus;
        use nexus_spoke_adapter::adapter::activation::ActivationBudget;

        let trace = vec![
            ActivationTraceEntry {
                entry_id: "kb_hero".to_string(),
                canonical_name: "Hero".to_string(),
                reason: "keyword match".to_string(),
                accepted: true,
                hop_origin_entry_id: None,
                hop_depth: None,
                source_relation_type: None,
                source_relation_id: None,
            },
            ActivationTraceEntry {
                entry_id: "kb_castle".to_string(),
                canonical_name: "Castle".to_string(),
                reason: "no keyword match".to_string(),
                accepted: false,
                hop_origin_entry_id: None,
                hop_depth: None,
                source_relation_type: None,
                source_relation_id: None,
            },
        ];
        let ctx = MomentContext {
            stage0_context: "stage0".to_string(),
            moment_directive: Some("DIRECTIVE_SECRET_MARKER keep the prose terse".to_string()),
            moment_directive_depth: DirectiveDepth::Head,
            activation_trace: Some(trace),
            // V1.151 P0: populate the enriched surface — the directive meta
            // carries status/metadata only (never the body).
            slot_map: Some(vec![nexus_moment_context_assembly::slots::SlotMapEntry {
                entry_id: "kb_hero".to_string(),
                slot: "world.before".to_string(),
            }]),
            activation_budget: Some(ActivationBudget {
                primary_tokens_est: 4,
                hop_tokens_est: 0,
                cap: None,
                remaining: None,
            }),
            moment_directive_meta: Some(MomentDirectiveStatus {
                scope: MomentDirectiveScope {
                    kind: "work".to_string(),
                    id: "wrk_1".to_string(),
                },
                insert_depth: DirectiveDepth::Head,
                ttl_kind: DirectiveTtlKind::Generations,
                ttl_remaining: Some(2),
                clear_on_scene_change: false,
                status: "active".to_string(),
            }),
            ..MomentContext::default()
        };

        let packet = build_inspector_packet(&ctx);
        let json = serde_json::to_string(&packet).expect("packet serializes");
        assert!(
            !json.contains("DIRECTIVE_SECRET_MARKER"),
            "AC-I3: inspector packet must never carry the Moment Directive body"
        );
        assert!(json.contains("kb_hero"), "trace entries still present");
        assert!(json.contains("modules"), "packet shape preserved");
        // Enriched product-local sections present (V1.151 P0, spec §2).
        assert!(
            json.contains("slot_map"),
            "enriched slot_map section present"
        );
        assert!(json.contains("budget"), "enriched budget section present");
        assert!(
            json.contains("moment_directive"),
            "enriched moment_directive section present"
        );
        // The moment_directive section is metadata-only: scope metadata
        // present, no `body` key anywhere in the packet.
        assert!(
            json.contains("\"scope\":\"work\""),
            "directive scope metadata present"
        );
        assert!(
            !json.contains("\"body\""),
            "AC-I3: no body key anywhere in the enriched packet"
        );
    }

    // ── V1.145 P2: assemble_moment behavior equivalence (T4) ────────────
    //
    // Proves the MCA wiring switch from `SqliteKbStore` to
    // `SpokeBackedKbStore` produces byte-identical `assemble_moment` output:
    // the `world_kb` section (the only KB-dependent slice) is rendered from
    // `canonical_name` / `block_type` / `body.summary`, all of which round-trip
    // losslessly through the spoke conversion seam.

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_mca_world(pool: &sqlx::SqlitePool) {
        // SAFETY: test-only static INSERTs with bind params.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_t4', 'wrk_test', 'ctr_test', 'T4 World', 't4-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// T4: `assemble_moment` renders an identical `world_kb` section whether
    /// the KB store is `SqliteKbStore` (pre-P2) or `SpokeBackedKbStore` (P2).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn assemble_moment_world_kb_identical_across_kb_stores() {
        use nexus_contracts::BlockType;
        use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
        use nexus_knowledge::world_kb::KbStore;
        use nexus_moment_context_assembly::MomentContext;

        // `KbStore` has async methods (not dyn-compatible), so use a generic
        // helper that runs `assemble_moment` against a concrete store type.
        #[allow(clippy::future_not_send)]
        async fn run<K: KbStore>(pool: &sqlx::SqlitePool, kb_store: &K) -> MomentContext {
            let narrative =
                nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
            let knowledge = SqliteKnowledgeStore::new(pool.clone());
            let stage0 = Stage0Assembly {
                personality: "P.".to_string(),
                experience: "E.".to_string(),
                user_prompt: "P.".to_string(),
                ..Stage0Assembly::default()
            };
            let request = MomentRequest::new(stage0).with_world("wld_t4");
            assemble_moment(&request, &narrative, kb_store, &knowledge).await
        }

        let (pool, _dir) = fresh_pool().await;
        seed_mca_world(&pool).await;

        // Seed entries whose bodies exercise summary + a block_type each.
        let seeder = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        for (idx, (bt, name)) in [
            (BlockType::Character, "Alice"),
            (BlockType::Item, "Atlantis"),
            (BlockType::Organization, "Anvil"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut entry = KnowledgeEntryRecord::new("wld_t4", bt, name);
            entry.entry_id = format!("kb_t4_{idx}");
            entry.body = Some(KnowledgeEntryBody {
                summary: Some(format!("{name} summary")),
                // An integer attribute that the spoke typed body alone would
                // round-trip as a float — the lossless carrier must recover it.
                attributes: Some(serde_json::json!({"order": idx})),
                ..Default::default()
            });
            seeder.insert_knowledge_entry(entry).await.unwrap();
        }

        let _stage0 = Stage0Assembly {
            personality: "P.".to_string(),
            experience: "E.".to_string(),
            user_prompt: "P.".to_string(),
            ..Stage0Assembly::default()
        };

        let sqlite_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let ctx_sqlite = run(&pool, &sqlite_store).await;
        let spoke_store = nexus_spoke_adapter::SpokeBackedKbStore::new(pool.clone());
        let ctx_spoke = run(&pool, &spoke_store).await;

        // The KB-dependent slice must be byte-identical.
        assert_eq!(
            ctx_sqlite.world_kb, ctx_spoke.world_kb,
            "assemble_moment world_kb must be identical across SqliteKbStore and SpokeBackedKbStore"
        );
        // Sanity: the section is non-empty and contains the seeded names.
        let kb_text = ctx_spoke.world_kb.as_deref().unwrap_or("");
        assert!(kb_text.contains("Alice"));
        assert!(kb_text.contains("Atlantis"));
        assert!(kb_text.contains("Anvil"));
    }

    // ── V1.146 P4 T3: inspector packet emission ──────────────────────

    /// Build a `MomentContext` with a mock activation trace.
    fn mock_ctx_with_trace(trace: Vec<ActivationTraceEntry>) -> MomentContext {
        MomentContext {
            stage0_context: "Test".to_string(),
            activation_trace: Some(trace),
            ..MomentContext::default()
        }
    }

    /// Helper: build a trace entry (primary-only row — no hop fields).
    fn trace_entry(
        entry_id: &str,
        canonical_name: &str,
        reason: &str,
        accepted: bool,
    ) -> ActivationTraceEntry {
        ActivationTraceEntry {
            entry_id: entry_id.to_string(),
            canonical_name: canonical_name.to_string(),
            reason: reason.to_string(),
            accepted,
            hop_origin_entry_id: None,
            hop_depth: None,
            source_relation_type: None,
            source_relation_id: None,
        }
    }

    /// P4 T3: inspector packet JSON contains `modules.placement` and
    /// `modules.activation_trace` keys with correct structure.
    #[test]
    fn inspector_packet_contains_placement_and_trace_keys() {
        let trace = vec![
            trace_entry("kb_hero", "Hero", "and_any: matched keys [king]", true),
            trace_entry(
                "kb_castle",
                "Castle",
                "and_any: no key matched (1 keys scanned)",
                false,
            ),
            trace_entry("kb_forest", "Forest", "no activation module", true),
        ];
        let ctx = mock_ctx_with_trace(trace);

        // Write packet to temp file and inspect JSON.
        let dir = tempfile::tempdir().unwrap();
        let packet_path = dir.path().join("packet.json");
        let path_str = packet_path.to_str().unwrap();

        emit_inspector_packet(&ctx, Some(path_str)).unwrap();

        let raw = std::fs::read_to_string(&packet_path).unwrap();
        let packet: serde_json::Value = serde_json::from_str(&raw).unwrap();

        // Top-level key
        let modules = &packet["modules"];
        assert!(modules.is_object(), "packet.modules must be an object");

        // placement
        let placement = &modules["placement"];
        assert!(placement.is_array(), "placement must be an array");
        assert_eq!(placement.as_array().unwrap().len(), 2, "2 accepted entries");

        // First placed: Hero (accepted)
        assert_eq!(placement[0]["entry_id"], "kb_hero");
        assert_eq!(placement[0]["canonical_name"], "Hero");
        assert!(placement[0]["reason"]
            .as_str()
            .unwrap()
            .contains("matched keys"));

        // Second placed: Forest (neutral, accepted)
        assert_eq!(placement[1]["entry_id"], "kb_forest");
        assert_eq!(placement[1]["canonical_name"], "Forest");

        // activation_trace
        let activation_trace = &modules["activation_trace"];
        assert!(
            activation_trace.is_array(),
            "activation_trace must be an array"
        );
        assert_eq!(
            activation_trace.as_array().unwrap().len(),
            3,
            "all 3 entries traced"
        );

        // Verify each trace entry has required fields
        for (i, entry) in activation_trace.as_array().unwrap().iter().enumerate() {
            assert!(entry["entry_id"].is_string(), "trace[{i}] missing entry_id");
            assert!(
                entry["canonical_name"].is_string(),
                "trace[{i}] missing canonical_name"
            );
            assert!(entry["reason"].is_string(), "trace[{i}] missing reason");
            assert!(
                entry["accepted"].is_boolean(),
                "trace[{i}] missing accepted"
            );
        }

        // Unmatched entry (Castle) appears in trace with accepted=false
        // but does NOT appear in placement.
        assert_eq!(activation_trace[1]["entry_id"], "kb_castle");
        assert_eq!(activation_trace[1]["accepted"], false);
        assert!(
            !placement
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["entry_id"].as_str() == Some("kb_castle")),
            "Castle must not appear in placement (unmatched)"
        );
    }

    /// P4 T3: empty trace produces valid but minimal packet.
    #[test]
    fn inspector_packet_empty_trace() {
        let ctx = mock_ctx_with_trace(vec![]);

        let dir = tempfile::tempdir().unwrap();
        let packet_path = dir.path().join("packet.json");
        let path_str = packet_path.to_str().unwrap();

        emit_inspector_packet(&ctx, Some(path_str)).unwrap();

        let raw = std::fs::read_to_string(&packet_path).unwrap();
        let packet: serde_json::Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(packet["modules"]["placement"].as_array().unwrap().len(), 0);
        assert_eq!(
            packet["modules"]["activation_trace"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    /// P4 T3: `activation_trace` is None (no activation enabled) → empty arrays.
    #[test]
    fn inspector_packet_no_trace_produces_empty_arrays() {
        let ctx = MomentContext {
            stage0_context: "Test".to_string(),
            activation_trace: None,
            ..MomentContext::default()
        };

        let dir = tempfile::tempdir().unwrap();
        let packet_path = dir.path().join("packet.json");
        let path_str = packet_path.to_str().unwrap();

        emit_inspector_packet(&ctx, Some(path_str)).unwrap();

        let raw = std::fs::read_to_string(&packet_path).unwrap();
        let packet: serde_json::Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(packet["modules"]["placement"].as_array().unwrap().len(), 0);
        assert_eq!(
            packet["modules"]["activation_trace"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    // ── NEXUS_MCA_LORE_ACTIVATION off-switch (P0 fix wave, QC F-002) ──

    #[test]
    fn lore_activation_off_switch_off_values() {
        for value in ["off", "0", "false", " OFF ", "FALSE", "Off"] {
            assert!(
                lore_activation_value_is_off(value),
                "{value:?} must disable activation"
            );
        }
    }

    #[test]
    fn lore_activation_off_switch_on_values() {
        for value in ["", "1", "on", "true", "garbage", "disabled", "yes"] {
            assert!(
                !lore_activation_value_is_off(value),
                "{value:?} must keep activation on"
            );
        }
    }

    #[test]
    fn lore_activation_off_switch_env_wrapper() {
        // Unset → on (spec §6 default); `off` → off; `1` → on. Kept in one
        // test so the env writes stay sequential — no other test in this
        // binary touches `NEXUS_MCA_LORE_ACTIVATION`.
        std::env::remove_var("NEXUS_MCA_LORE_ACTIVATION");
        assert!(
            !lore_activation_env_is_off(),
            "unset must keep activation on"
        );
        std::env::set_var("NEXUS_MCA_LORE_ACTIVATION", "off");
        assert!(lore_activation_env_is_off(), "off must disable activation");
        std::env::set_var("NEXUS_MCA_LORE_ACTIVATION", "1");
        assert!(!lore_activation_env_is_off(), "=1 must keep activation on");
        std::env::remove_var("NEXUS_MCA_LORE_ACTIVATION");
    }

    // ── V1.151 P0 T4: assemble-moment --inspect + enriched --emit-packet ──

    /// Build a `MomentContext` carrying the full enriched surface (activation
    /// trace + `slot_map` + budget + directive meta) for the renderer /
    /// emission tests — the same shape `build_inspector_packet` consumes.
    fn enriched_mock_ctx() -> MomentContext {
        use nexus_moment_context_assembly::directive::{
            DirectiveDepth, DirectiveTtlKind, MomentDirectiveScope,
        };
        use nexus_moment_context_assembly::MomentDirectiveStatus;
        use nexus_spoke_adapter::adapter::activation::ActivationBudget;

        let trace = vec![
            trace_entry(
                "kb_hero",
                "Hero",
                "primary-any (literal): matched key [king]",
                true,
            ),
            trace_entry(
                "kb_castle",
                "Castle",
                "primary-any (literal): no key matched",
                false,
            ),
        ];
        MomentContext {
            stage0_context: "Test".to_string(),
            activation_trace: Some(trace),
            slot_map: Some(vec![nexus_moment_context_assembly::slots::SlotMapEntry {
                entry_id: "kb_hero".to_string(),
                slot: "world.before".to_string(),
            }]),
            activation_budget: Some(ActivationBudget {
                primary_tokens_est: 4,
                hop_tokens_est: 2,
                cap: Some(100),
                remaining: Some(94),
            }),
            moment_directive_meta: Some(MomentDirectiveStatus {
                scope: MomentDirectiveScope {
                    kind: "work".to_string(),
                    id: "wrk_1".to_string(),
                },
                insert_depth: DirectiveDepth::Head,
                ttl_kind: DirectiveTtlKind::Generations,
                ttl_remaining: Some(2),
                clear_on_scene_change: false,
                status: "active".to_string(),
            }),
            ..MomentContext::default()
        }
    }

    /// T4: `--inspect` renders the four readable sections with the fired
    /// entry's canonical name + its slot id from `slot_map`.
    #[test]
    fn render_inspector_readable_shows_four_sections() {
        let packet = build_inspector_packet(&enriched_mock_ctx());
        let view = render_inspector_readable(&packet);

        // Four section headers.
        assert!(view.contains("## Activation trace"));
        assert!(view.contains("## Slot map"));
        assert!(view.contains("## Budget"));
        assert!(view.contains("## Moment Directive"));
        // Fired entry: canonical name + slot id; missed entry + reason.
        assert!(view.contains("Hero"), "fired canonical_name rendered");
        assert!(
            view.contains("world.before"),
            "fired entry's slot id rendered from slot_map"
        );
        assert!(view.contains("Castle"), "missed entry rendered");
        assert!(
            view.contains("matched key [king]"),
            "human-readable reason rendered"
        );
        // Directive status/metadata only — never the body (AC-I3).
        assert!(view.contains("status: active"));
        assert!(view.contains("wrk_1"));
        assert!(
            !view.contains("DIRECTIVE_SECRET_MARKER"),
            "AC-I3: directive body never rendered"
        );
        // Budget figures.
        assert!(view.contains("primary tokens est: 4"));
        assert!(view.contains("remaining: 94"));
    }

    /// T4: an empty packet still renders all four sections with empty markers.
    #[test]
    fn render_inspector_readable_empty_packet() {
        let packet = build_inspector_packet(&MomentContext::default());
        let view = render_inspector_readable(&packet);

        assert!(view.contains("## Activation trace"));
        assert!(view.contains("(no activation trace entries)"));
        assert!(view.contains("## Slot map"));
        assert!(view.contains("(empty)"));
        assert!(view.contains("## Budget"));
        assert!(view.contains("## Moment Directive"));
        assert!(view.contains("status: none"));
    }

    /// T4: the `--emit-packet` emission path (enriched builder) writes the
    /// three additive product-local sections as top-level keys.
    #[test]
    fn emit_inspector_packet_emits_enriched_json() {
        let ctx = enriched_mock_ctx();
        let dir = tempfile::tempdir().unwrap();
        let packet_path = dir.path().join("packet.json");
        let path_str = packet_path.to_str().unwrap();

        emit_inspector_packet(&ctx, Some(path_str)).unwrap();

        let raw = std::fs::read_to_string(&packet_path).unwrap();
        let packet: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(packet["modules"].is_object(), "modules section present");
        assert!(
            packet["slot_map"].is_array(),
            "enriched slot_map top-level key present"
        );
        assert!(
            packet["budget"].is_object(),
            "enriched budget top-level key present"
        );
        assert!(
            packet["moment_directive"].is_object(),
            "enriched moment_directive top-level key present"
        );
        // Enriched values round-trip through the emission path.
        assert_eq!(packet["slot_map"][0]["slot"], "world.before");
        assert_eq!(packet["budget"]["primary_tokens_est"], 4);
        assert_eq!(packet["moment_directive"]["status"], "active");
    }

    /// T4: `--inspect` and `--emit-packet` are mutually exclusive at the clap
    /// level (`conflicts_with`); `--inspect` alone parses and sets the flag.
    #[test]
    fn inspect_and_emit_packet_conflict_at_clap() {
        use crate::cli::{Cli, Commands};
        use crate::commands::platform::PlatformCommand;
        use clap::Parser;

        let inspect_only = Cli::try_parse_from([
            "nexus42",
            "platform",
            "context",
            "assemble-moment",
            "--inspect",
        ])
        .expect("--inspect alone must parse");
        match inspect_only.into_command() {
            Some(Commands::Platform { command }) => match command {
                PlatformCommand::Context { command } => match *command {
                    ContextCommand::AssembleMoment { inspect, .. } => {
                        assert!(inspect, "--inspect must set inspect = true");
                    }
                    ContextCommand::Assemble { .. } => panic!("unexpected context subcommand"),
                },
                _ => panic!("unexpected platform subcommand"),
            },
            _ => panic!("unexpected top-level command"),
        }

        let conflict = Cli::try_parse_from([
            "nexus42",
            "platform",
            "context",
            "assemble-moment",
            "--inspect",
            "--emit-packet",
        ]);
        assert!(
            conflict.is_err(),
            "--inspect + --emit-packet must be rejected by clap"
        );
    }
}
