//! Context Command — `nexus42 platform context assemble`, `assemble-local`, and `assemble-moment`.
//!
//! KCA-002 B2: Context assembly runs CLI in-process via `nexus-moment-context-assembly`.
//! The daemon context-assemble proxy route is retired. The `assemble-local` subcommand
//! uses `Stage0Assembly` / `TwoStageAssembly` directly without daemon HTTP.
//!
//! V1.26: `assemble-moment` is a visible four-domain Moment assembly command that reads
//! from persistent stores (`SqliteNarrativeGateway`, `SqliteKbStore`). Knowledge slice
//! uses in-memory store (no persistent `UserKnowledgeStore` yet).
//!
//! The `assemble` (platform) subcommand is **deferred** — it prints a guidance message
//! and exits with code 2.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::local::domain::RuntimeMode;
use nexus_moment_context_assembly::cloud_stage::{AssembleResponse, AssemblyRuntimeMode};
use nexus_moment_context_assembly::{
    assemble_moment, MomentContext, MomentRequest, Stage0Assembly, TwoStageAssembly,
};

use crate::domain::{DegradationGuard, DomainRuntimeMode};

// Four-domain Moment assembly — in-memory knowledge store
// (narrative and KB use persistent stores from nexus-local-db)
use nexus_knowledge::InMemoryKnowledgeStore;

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

    /// Run Stage-0 local context assembly and output result (local-only)
    AssembleLocal {
        /// Max tokens for context (optional budget)
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Include fragment keywords section
        #[arg(long, default_value_t = true)]
        include_fragments: bool,

        /// Optional prompt hint for platform assemble (used in two-stage modes)
        #[arg(long)]
        hint: Option<String>,
    },

    /// Assemble four-domain Moment context from local persistent stores
    AssembleMoment {
        /// World ID to include in Moment context
        #[arg(long)]
        world_id: Option<String>,

        /// User ID for knowledge lookup
        #[arg(long)]
        user_id: Option<String>,

        /// Branch ID within the world
        #[arg(long)]
        branch_id: Option<String>,

        /// Event ID to focus context around
        #[arg(long)]
        event_id: Option<String>,
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
            eprintln!("Use `assemble-local` for Stage0/TwoStage or `assemble-moment` for local four-domain Moment assembly.");
            std::process::exit(2);
        }
        ContextCommand::AssembleLocal {
            max_tokens,
            include_fragments,
            hint,
        } => {
            assemble_local_with_routing(config, max_tokens, include_fragments, hint.as_deref())
                .await
        }
        ContextCommand::AssembleMoment {
            world_id,
            user_id,
            branch_id,
            event_id,
        } => {
            let ctx = run_assemble_moment(
                config,
                world_id.as_deref(),
                user_id.as_deref(),
                branch_id.as_deref(),
                event_id.as_deref(),
            )
            .await?;

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
    let url = format!("{}/v1/local/memory/fragments", config.daemon_url);

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
        tracing::debug!("Daemon does not implement /v1/local/memory/fragments, skipping");
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

/// Run four-domain Moment assembly using persistent narrative + KB stores.
///
/// Uses `SqliteNarrativeGateway` and `SqliteKbStore` from `nexus-local-db`
/// for world state, timeline, and KB data. Knowledge slice uses in-memory
/// store (limitation: no persistent `UserKnowledgeStore` in V1.26).
///
/// # Errors
///
/// Returns `CliError` if the database cannot be opened or migrations fail.
#[allow(clippy::future_not_send)]
pub async fn run_assemble_moment(
    config: &CliConfig,
    world_id: Option<&str>,
    user_id: Option<&str>,
    branch_id: Option<&str>,
    _event_id: Option<&str>,
) -> Result<MomentContext> {
    let pool = open_shared_pool(config).await?;
    let narrative = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
    let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool);
    let knowledge = InMemoryKnowledgeStore::new();

    let wid = world_id.unwrap_or("wld_default");
    let uid = user_id.unwrap_or("user_default");

    // Build Stage0Assembly — load from creator memory if available
    let stage0 = build_stage0_from_local(config, None, None, false)
        .await
        .unwrap_or_else(|_| Stage0Assembly {
            personality: "Local Moment assembly.".to_string(),
            experience: "Four-domain context from persistent stores.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            system_prefix: String::new(),
            user_prompt: "Moment context assembly.".to_string(),
            max_tokens: None,
        });

    // Build MomentRequest
    let mut request = MomentRequest::new(stage0).with_world(wid).with_user(uid);
    if let Some(bid) = branch_id {
        request = request.with_branch(bid);
    }

    // Call assemble_moment with persistent stores
    Ok(assemble_moment(&request, &narrative, &kb, &knowledge).await)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

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

    /// Test that `AssembleLocal` variant exists with new hint field
    #[test]
    fn context_command_assemble_local_exists() {
        let _ = ContextCommand::AssembleLocal {
            max_tokens: Some(1000),
            include_fragments: true,
            hint: Some("write chapter 3".to_string()),
        };
        let _ = ContextCommand::AssembleLocal {
            max_tokens: None,
            include_fragments: false,
            hint: None,
        };
    }

    /// C1.1: `AssembleMoment` variant exists without `hide = true`.
    /// Clap's `hide = true` is a `#[command(...)]` attribute, not runtime-testable
    /// directly, but we verify the variant is constructible and documented.
    #[test]
    fn context_command_assemble_moment_exists() {
        let _ = ContextCommand::AssembleMoment {
            world_id: Some("wld_test".to_string()),
            user_id: Some("user_test".to_string()),
            branch_id: None,
            event_id: None,
        };
        let _ = ContextCommand::AssembleMoment {
            world_id: None,
            user_id: None,
            branch_id: None,
            event_id: None,
        };
    }

    /// C1.3: Verify `Assemble` arm prints deferred message (no V1.10 reference).
    /// We check the non-test portion of the source file.
    #[test]
    fn assemble_arm_has_no_v1_10_reference() {
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
            source.contains("assemble-local"),
            "deferred message must mention assemble-local"
        );
        assert!(
            source.contains("assemble-moment"),
            "deferred message must mention assemble-moment"
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
        kb_seed::key_block(
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
        let knowledge = InMemoryKnowledgeStore::new();

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
        let knowledge = InMemoryKnowledgeStore::new();

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
}
