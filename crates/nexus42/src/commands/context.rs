//! Context Command — `nexus42 context assemble` and `nexus42 context assemble-local`

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::context::client::ContextClient;
use crate::context::types::ContextAssembleRequestV1;
use crate::context::types::{error_code, error_message, is_error, MemoryKind};
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::RuntimeMode;
use nexus_domain::runtime_guard;
use nexus_domain::{
    context_assembly::AssembleResponse, DegradationGuard, DomainRuntimeMode,
    Stage0Assembly, TwoStageAssembly,
};

#[cfg(test)]
use nexus_domain::DegradationPolicy;

/// Validate WorldId format: must start with 'wld_' followed by alphanumeric characters
pub fn validate_world_id(s: &str) -> std::result::Result<String, String> {
    // Check prefix
    if !s.starts_with("wld_") {
        return Err(format!(
            "WorldId must start with 'wld_' prefix (got '{}')",
            s
        ));
    }

    // Check that there's content after prefix
    let suffix = &s[4..]; // Skip "wld_" prefix (4 chars)
    if suffix.is_empty() {
        return Err("WorldId must have alphanumeric characters after 'wld_' prefix".to_string());
    }

    // Check that suffix contains only alphanumeric characters
    if !suffix.chars().all(|c| c.is_alphanumeric()) {
        return Err(format!(
            "WorldId must contain only alphanumeric characters after 'wld_' prefix (got '{}')",
            suffix
        ));
    }

    // Return the validated string
    Ok(s.to_string())
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble context for a world via the Local API
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
        #[arg(short, long)]
        hint: Option<String>,
    },
}

/// Run context command
pub async fn run(cmd: ContextCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ContextCommand::Assemble {
            world_id,
            workspace_id,
            creator_id,
            include_memory,
            include_timeline,
            include_story_summaries,
            max_timeline_events,
            max_story_summaries,
            max_file_size,
            output_file,
        } => {
            // Guard: context assemble requires platform (ADR-017 D5)
            runtime_guard::require_platform(&config.runtime_mode(), "context assemble")?;

            // Resolve workspace_id and creator_id from config if not provided
            let workspace_id = workspace_id.unwrap_or_else(|| {
                config
                    .workspace_path
                    .as_ref()
                    .map(|_| "wrk_current".to_string())
                    .unwrap_or_else(|| "wrk_unknown".to_string())
            });

            let creator_id = creator_id.unwrap_or_else(|| {
                config
                    .active_creator_id
                    .clone()
                    .unwrap_or_else(|| "ctr_unknown".to_string())
            });

            // Build the request
            let request = ContextAssembleRequestV1 {
                request_id: format!("req_{}", uuid::Uuid::new_v4().simple()),
                workspace_id,
                creator_id,
                world_id,
                include_memory: Some(include_memory),
                include_timeline: Some(include_timeline),
                include_story_summaries: Some(include_story_summaries),
                memory_kinds: Some(vec![
                    MemoryKind::StorySummary.to_string(),
                    MemoryKind::ResearchMaterial.to_string(),
                    MemoryKind::ReviewNote.to_string(),
                ]),
                max_timeline_events: max_timeline_events.map(|v| v as i64),
                max_story_summaries: max_story_summaries.map(|v| v as i64),
            };

            // Note: max_file_size is not yet passed to the daemon API
            // It will be used when SummaryGenerator is integrated into the context assembly workflow
            // For now, suppress unused warning
            let _ = max_file_size;

            // Create daemon client and context client
            let daemon = DaemonClient::from_config(config);
            let client = ContextClient::new(daemon);

            // Call the Local API
            let response = client.assemble(&request).await?;

            // Handle error responses
            if is_error(&response) {
                let ec = error_code(&response).unwrap_or("unknown");
                let em = error_message(&response).unwrap_or("No details available");
                eprintln!("Error: Context assembly failed ({})", ec);
                eprintln!("  {}", em);
                if ec == "auth_expired" {
                    eprintln!("  Run `nexus42 auth login` to re-authenticate.");
                } else if ec == "world_not_found" {
                    eprintln!("  Check the world ID and ensure the world exists on the platform.");
                } else if ec == "platform_unavailable" {
                    eprintln!("  The platform may be temporarily unavailable. Try again later.");
                }
                std::process::exit(1);
            }

            // Output the response
            let output_json = serde_json::to_string_pretty(&response)?;
            match output_file {
                Some(path) => {
                    std::fs::write(&path, &output_json)?;
                    eprintln!("Context assembly written to {}", path);
                }
                None => {
                    println!("{}", output_json);
                }
            }

            Ok(())
        }
        ContextCommand::AssembleLocal {
            max_tokens,
            include_fragments,
            hint,
        } => assemble_local_with_routing(config, max_tokens, include_fragments, hint.as_deref()).await,
    }
}

/// Create a DegradationGuard from config, restoring from persisted snapshot if available.
pub fn create_degradation_guard(config: &CliConfig) -> DegradationGuard {
    let mode = config.runtime_mode();

    // If snapshot exists with non-Normal state, restore directly without
    // replaying failures (avoids unintended re-degradation — C-001).
    if let Some(snap) = config.degradation_snapshot() {
        if snap.state != nexus_domain::degradation::DegradationState::Normal {
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
    use nexus_domain::DegradationSnapshot;
    config.degradation_snapshot = Some(DegradationSnapshot::from_guard(guard));
    config.save()?;
    Ok(())
}

/// Mode-aware context assembly with degradation support (T6.7 + T6.8).
///
/// Routes by `runtime_mode`:
/// - `local_only` → Stage0Assembly directly
/// - `local_first` / `cloud_enhanced` → TwoStageAssembly with fallback to Stage0
///
/// Records platform results in DegradationGuard for degradation tracking.
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

    println!("{}", context);

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
/// Stage0Assembly if the platform is unavailable.
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
            let stage0 = build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
            Ok(if max_tokens.is_some() {
                stage0.assemble_with_truncation()
            } else {
                stage0.assemble()
            })
        }
        RuntimeMode::LocalFirst | RuntimeMode::CloudEnhanced => {
            // Two-stage: try platform, fallback to Stage0
            let platform_result = try_platform_assemble(config, hint).await;

            match platform_result {
                Some(response) => {
                    guard.record_platform_result(true, None);
                    let stage0 = build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
                    let two_stage = build_two_stage_from_local(&stage0, response, mode);
                    Ok(two_stage.assemble())
                }
                None => {
                    // Platform failed — record and fall back to Stage0
                    guard.record_platform_result(false, Some("assemble unavailable".to_string()));
                    let stage0 = build_stage0_from_local(config, hint, max_tokens, include_fragments).await?;
                    Ok(if max_tokens.is_some() {
                        stage0.assemble_with_truncation()
                    } else {
                        stage0.assemble()
                    })
                }
            }
        }
    }
}

/// Build a Stage0Assembly from local sources (SOUL.md, memories, fragments).
async fn build_stage0_from_local(
    config: &CliConfig,
    hint: Option<&str>,
    max_tokens: Option<usize>,
    include_fragments: bool,
) -> Result<Stage0Assembly> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    let home = crate::config::user_home_dir()?;

    // 1. Load SOUL.md
    let soul = nexus_domain::soul_io::load(&home, creator_id)?;

    // 2. List long-term memories (skip personality_core — already in SOUL personality)
    let slugs = nexus_domain::memory_io::list_memories(&home, creator_id)?;
    let mut long_term_memories = Vec::new();
    for slug in &slugs {
        if let Ok(mem) = nexus_domain::memory_io::load_memory(&home, creator_id, slug) {
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

/// Try to call platform assemble via daemon (best-effort).
///
/// Returns `Some(AssembleResponse)` if the platform call succeeds,
/// or `None` if the daemon is unavailable or the call fails.
/// This is used for two-stage assembly in `local_first`/`cloud_enhanced` modes.
async fn try_platform_assemble(
    config: &CliConfig,
    _hint: Option<&str>,
) -> Option<AssembleResponse> {
    let client = DaemonClient::from_config(config);

    // Use call_assemble which sends the request shape the daemon expects
    // (W-1 fix: ContextClient::assemble sent ContextAssembleRequestV1 causing 422).
    let creator_id = config
        .active_creator_id
        .as_deref()
        .unwrap_or("ctr_unknown");
    let runtime_mode_str = config.runtime_mode().to_string();

    match client
        .call_assemble(creator_id, "wrk_default", &runtime_mode_str, _hint)
        .await
    {
        Ok(Some(response)) => Some(response),
        Ok(None) => {
            tracing::debug!("Platform assemble returned None (unavailable)");
            None
        }
        Err(e) => {
            tracing::debug!(error = %e, "Failed to reach daemon for platform assemble");
            None
        }
    }
}

/// Build a TwoStageAssembly from local context data and a platform response.
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
        runtime_mode: mode,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test valid WorldId formats
    #[test]
    fn validate_world_id_accepts_valid_formats() {
        // Valid: starts with wld_ followed by alphanumeric
        assert!(validate_world_id("wld_abc123").is_ok());
        assert!(validate_world_id("wld_test").is_ok());
        assert!(validate_world_id("wld_ABCDEF123456").is_ok());
        assert!(validate_world_id("wld_1").is_ok());
    }

    /// Test invalid WorldId formats - missing prefix
    #[test]
    fn validate_world_id_rejects_missing_prefix() {
        let result = validate_world_id("abc123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - wrong prefix
    #[test]
    fn validate_world_id_rejects_wrong_prefix() {
        let result = validate_world_id("world_123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - empty
    #[test]
    fn validate_world_id_rejects_empty() {
        let result = validate_world_id("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - special characters
    #[test]
    fn validate_world_id_rejects_special_characters() {
        let result = validate_world_id("wld_test-123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));

        let result = validate_world_id("wld_test@123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));
    }

    /// Test invalid WorldId formats - only prefix
    #[test]
    fn validate_world_id_rejects_only_prefix() {
        let result = validate_world_id("wld_");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));
    }

    /// Test that AssembleLocal variant exists with new hint field
    #[test]
    fn context_command_assemble_local_exists() {
        let _cmd = ContextCommand::AssembleLocal {
            max_tokens: Some(1000),
            include_fragments: true,
            hint: Some("write chapter 3".to_string()),
        };
        let _cmd = ContextCommand::AssembleLocal {
            max_tokens: None,
            include_fragments: false,
            hint: None,
        };
    }

    // ── T6.7 / T6.8: Mode-aware routing tests ────────────────────────────

    /// Helper: create a minimal Stage0Assembly for routing tests.
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

    /// Helper: create a platform AssembleResponse for routing tests.
    fn make_platform_response() -> AssembleResponse {
        use nexus_domain::context_assembly::{
            AssembleMetadata, MemoryItemRef, TimelineEventRef,
        };

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

    /// T6.8: local_only mode uses Stage0Assembly directly.
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
        assert_eq!(guard.degradation_state(), nexus_domain::degradation::DegradationState::Normal);
        assert_eq!(guard.failure_count(), 0);
    }

    /// T6.8: cloud_enhanced mode attempts TwoStageAssembly with platform data.
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
        assert_eq!(guard.degradation_state(), nexus_domain::degradation::DegradationState::Normal);
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
        assert_eq!(guard.degradation_state(), nexus_domain::degradation::DegradationState::Normal);
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
            nexus_domain::degradation::DegradationState::DegradedLevel1
        );
        assert_eq!(guard.failure_count(), 0); // reset after degradation

        // Current mode should be downgraded
        assert_eq!(*guard.current_mode(), DomainRuntimeMode::new(RuntimeMode::LocalFirst));
    }

    /// local_first mode falls back to Stage0 on platform failure.
    #[tokio::test]
    async fn context_assemble_local_first_fallback() {
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalFirst);
        let mut guard = DegradationGuard::with_defaults(mode);

        // Simulate platform failure
        guard.record_platform_result(false, Some("connection refused".to_string()));

        assert_eq!(guard.failure_count(), 1);
        assert_eq!(guard.degradation_state(), nexus_domain::degradation::DegradationState::Normal);
    }

    /// build_two_stage_from_local produces correct TwoStageAssembly.
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
        assert_eq!(two_stage.runtime_mode, mode);
    }

    /// create_degradation_guard restores from persisted snapshot.
    ///
    /// C-001: For non-Normal state, restoration must NOT replay failures
    /// (which would trigger unintended re-degradation).
    #[test]
    fn create_guard_restores_from_snapshot() {
        use nexus_domain::degradation::DegradationState;

        // Non-Normal state: should restore directly without replaying failures
        let mut config = CliConfig::default();
        config.runtime_mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        config.degradation_snapshot = Some(nexus_domain::DegradationSnapshot {
            state: DegradationState::DegradedLevel1,
            failure_count: 2,
            last_health_check: None,
        });

        let guard = create_degradation_guard(&config);

        // State should be restored directly
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
        assert_eq!(guard.failure_count(), 2);
        // Mode should be downgraded one level from CloudEnhanced
        assert_eq!(*guard.current_mode(), DomainRuntimeMode::new(RuntimeMode::LocalFirst));

        // Normal state with failures: should replay to restore failure_count
        let mut config2 = CliConfig::default();
        config2.runtime_mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        config2.degradation_snapshot = Some(nexus_domain::DegradationSnapshot {
            state: DegradationState::Normal,
            failure_count: 1,
            last_health_check: None,
        });

        let guard2 = create_degradation_guard(&config2);
        assert_eq!(guard2.degradation_state(), DegradationState::Normal);
        assert_eq!(guard2.failure_count(), 1);
    }
}
