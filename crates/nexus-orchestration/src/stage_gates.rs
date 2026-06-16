//! Shared FL-E stage gate validation and schedule wiring (V1.34 creator-workflow §3–4).
//!
//! Provides:
//! - `check_stage_advance` — the single authoritative gate function
//!   used by both CLI `stage_advance` and daemon `PATCH /v1/local/works/{id}`.
//! - `preset_for_stage` — normative stage → preset mapping (spec §4).
//! - `build_stage_schedule_label` — schedule label for stage advance.
//! - `build_schedule_for_stage` — builds a correctly-shaped `AddScheduleRequest`
//!   for the FL-E stage schedule, ensuring contract alignment with the daemon.

use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};
use nexus_contracts::local::schedule::http::AddScheduleRequest;

/// Format a chapter number as a zero-padded label for path construction.
///
/// - `chapter_label(1) == "01"`
/// - `chapter_label(9) == "09"`
/// - `chapter_label(10) == "10"`
/// - `chapter_label(100) == "100"` (2-digit pad for 1-99, then grows naturally)
///
/// Spec §4.5.6 accepts 2-digit zero-pad for chapter 1-99; future iterations
/// may tighten to fixed-width if novels require it.
#[must_use]
pub fn chapter_label(chapter: i32) -> String {
    format!("{chapter:02}")
}

/// Normative stage → default preset mapping (spec §4).
///
/// Returns the canonical preset ID for a given FL-E stage.
/// Returns `None` for unknown stages.
///
/// This is the authoritative wiring function for the preset chain:
///
/// | Stage     | Preset                  |
/// |-----------|-------------------------|
/// | intake    | `creative-brief-intake` |
/// | research  | `research`              |
/// | produce   | `novel-writing`         |
/// | review    | `novel-chapter-review`  |
/// | persist   | `kb-extract`            |
#[must_use]
pub fn preset_for_stage(stage: &str) -> Option<&'static str> {
    crate::preset::validation::default_preset_for_stage(stage)
}

/// Build the schedule label for a stage advance (spec §4).
#[must_use]
pub fn build_stage_schedule_label(stage: &str, work_id: &str) -> String {
    format!("FL-E stage: {stage} (work: {work_id})")
}

/// Work fields available for preset input templates (T2, spec §4).
#[derive(Debug, Clone)]
pub struct WorkFields {
    /// Work entity ID (e.g. `wrk_abc123`).
    pub work_id: String,
    /// FL-E stage being advanced to.
    pub fl_e_stage: String,
    /// Creative brief JSON (may be empty string if intake not completed).
    pub creative_brief: String,
    /// Inspiration log JSON array (may be "[]" if empty).
    pub inspiration_log: String,
    /// Work reference slug (V1.38 P0: needed for novel-writing template vars).
    pub work_ref: Option<String>,
    /// Selected chapter number for novel-writing (V1.38 P0 §4.5.2).
    pub chapter: Option<i32>,
    /// Zero-padded chapter label (V1.38 P1): `"01"`, `"02"`, … `"10"`.
    pub chapter_label: Option<String>,
    /// Full path to chapter outline file (V1.38 P1).
    pub outline_path: Option<String>,
    /// Full path to chapter body file (V1.38 P1).
    pub body_path: Option<String>,
    /// Chapter slug from `work_chapters.slug` (V1.38 P1).
    pub slug: Option<String>,
    /// Fix W-2: path to research artifacts directory, populated when
    /// the produce stage follows a completed research stage in the auto-chain.
    pub research_artifacts_dir: Option<String>,
    /// V1.39 P3 (DF-65): workspace directory for reading rules files.
    /// When set, `build_preset_input` reads Layer 1 + Layer 2 rules.
    pub workspace_dir: Option<String>,
    /// V1.40 P2: pre-assembled World KB context block (YAML string).
    /// Populated by the caller (CLI/daemon) via `build_chapter_kb_block`.
    /// When `None`, the template guard `{{#if world_kb_block}}` correctly omits
    /// the block — legacy V1.39 worldless Works receive no World context.
    pub world_kb_block: Option<String>,
    /// V1.40 P3: `world_id` from Work row. Populated for World-bound Works.
    /// When `None`, the persist stage skips World KB promotion (legacy V1.39).
    pub world_id: Option<String>,
    /// V1.44 P2 (F-004): Volume number for multi-volume Works.
    /// When set, `build_preset_input` includes a `volume` template var so that
    /// the `novel-writing` preset preserves cross-volume context.
    pub volume: Option<i32>,
    /// V1.48 P1 (overlay §2 Consumer): pre-rendered open-findings block
    /// for chapter-scoped prompt injection. Populated by the caller (CLI
    /// `stage advance` or auto-chain enqueue) via
    /// [`crate::findings_block::build_open_findings_block`]. When `None`
    /// or empty, the template guard `{{#if open_findings_block}}` omits
    /// the section (AC2: no empty sentinel noise).
    pub open_findings_block: Option<String>,
}

/// Build the `presetInput` map for a stage schedule (T2, spec §4).
///
/// Returns a `serde_json::Value::Object` containing the Work fields that
/// stage presets consume via `{{preset.input.*}}` template variables.
///
/// All stages receive the same base set; individual presets select the
/// fields they need from the preset input namespace.
//
// Flat field-by-field projection: each block inserts one template var. The
// linear shape is the idiom of this function; splitting into sub-helpers
// would add indirection without reducing complexity.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn build_preset_input(fields: &WorkFields) -> serde_json::Value {
    let mut map = serde_json::json!({
        "work_id": fields.work_id,
        "fl_e_stage": fields.fl_e_stage,
        "creative_brief": fields.creative_brief,
        "inspiration_log": fields.inspiration_log,
    });

    // V1.38 P0 (T4): include work_ref and chapter when available.
    if let Some(ref wr) = fields.work_ref {
        map.as_object_mut().map(|o| {
            o.insert(
                "work_ref".to_string(),
                serde_json::Value::String(wr.clone()),
            )
        });
    }
    if let Some(ch) = fields.chapter {
        map.as_object_mut()
            .map(|o| o.insert("chapter".to_string(), serde_json::Value::Number(ch.into())));
    }

    // V1.38 P1: include chapter context fields when available.
    if let Some(ref cl) = fields.chapter_label {
        map.as_object_mut().map(|o| {
            o.insert(
                "chapter_label".to_string(),
                serde_json::Value::String(cl.clone()),
            )
        });
    }
    if let Some(ref op) = fields.outline_path {
        map.as_object_mut().map(|o| {
            o.insert(
                "outline_path".to_string(),
                serde_json::Value::String(op.clone()),
            )
        });
    }
    if let Some(ref bp) = fields.body_path {
        map.as_object_mut().map(|o| {
            o.insert(
                "body_path".to_string(),
                serde_json::Value::String(bp.clone()),
            )
        });
    }
    if let Some(ref sl) = fields.slug {
        map.as_object_mut()
            .map(|o| o.insert("slug".to_string(), serde_json::Value::String(sl.clone())));
    }

    // Fix W-2: research artifacts directory for produce stage (after research).
    if let Some(ref rad) = fields.research_artifacts_dir {
        map.as_object_mut().map(|o| {
            o.insert(
                "research_artifacts_dir".to_string(),
                serde_json::Value::String(rad.clone()),
            )
        });
    }

    // V1.39 P3 (DF-65): read rules layers when workspace_dir and work_ref
    // are both present. Layer 1 = embedded default; Layer 2 = per-work file.
    if let (Some(ref ws_dir), Some(ref wref)) = (&fields.workspace_dir, &fields.work_ref) {
        if let Some(rules) = read_rules_layers(ws_dir, wref) {
            map.as_object_mut().map(|o| {
                o.insert(
                    "rules_content".to_string(),
                    serde_json::Value::String(rules),
                )
            });
        }
    }

    // V1.49 P1 (narrative-indexes overlay §4): inject a compact foreshadowing
    // summary read from `Works/<work_ref>/Outlines/foreshadowing.md` when the
    // index is non-empty, so outline/draft prompts can surface active F###.
    // Mirrors the `rules_content` workspace read above. When the file is
    // missing/empty (or workspace_dir/work_ref absent, e.g. the auto-chain
    // enqueue path which sets workspace_dir=None), default to empty string so
    // strict-mode template rendering does not fail on
    // `{{preset.input.foreshadowing_summary}}`; the preset's
    // `{{#if foreshadowing_summary}}` guard then omits the section.
    let fsummary = match (&fields.workspace_dir, &fields.work_ref) {
        (Some(ws_dir), Some(wref)) => {
            let work_dir = std::path::Path::new(ws_dir).join("Works").join(wref);
            crate::narrative_index::read_foreshadowing_summary(&work_dir).unwrap_or_default()
        }
        _ => String::new(),
    };
    map.as_object_mut().map(|o| {
        o.insert(
            "foreshadowing_summary".to_string(),
            serde_json::Value::String(fsummary),
        )
    });

    // V1.40 P2 (QC3 C-1 fix): inject the pre-assembled World KB block.
    // The caller (CLI/daemon) is responsible for building the block via
    // `build_chapter_kb_block` and passing it here. When `None` (worldless
    // Works or legacy V1.39), the template guard `{{#if world_kb_block}}`
    // correctly omits the section.
    if let Some(ref wkb) = fields.world_kb_block {
        map.as_object_mut().map(|o| {
            o.insert(
                "world_kb_block".to_string(),
                serde_json::Value::String(wkb.clone()),
            )
        });
    } else {
        // Ensure the key is present as empty string so strict-mode template
        // rendering does not fail on `{{preset.input.world_kb_block}}`.
        map.as_object_mut().map(|o| {
            o.insert(
                "world_kb_block".to_string(),
                serde_json::Value::String(String::new()),
            )
        });
    }

    // V1.40 P3 (T5): inject world_id for World-bound Works.
    // Persist stage uses this to route extraction to the correct World KB.
    // When None (legacy V1.39 worldless Works), the persist stage skips
    // World KB promotion.
    if let Some(ref wid) = fields.world_id {
        map.as_object_mut().map(|o| {
            o.insert(
                "world_id".to_string(),
                serde_json::Value::String(wid.clone()),
            )
        });
    }

    // V1.44 P2 (F-004): inject volume for multi-volume Works.
    // The `novel-writing` preset can use `{{preset.input.volume}}` to preserve
    // cross-volume context (e.g. "Volume 2, Chapter 3").
    if let Some(vol) = fields.volume {
        map.as_object_mut()
            .map(|o| o.insert("volume".to_string(), serde_json::Value::Number(vol.into())));
    }

    // V1.48 P1 (overlay §2 Consumer): inject the pre-rendered open-findings
    // block. The caller (CLI/auto-chain) builds it via
    // `findings_block::build_open_findings_block` from the chapter-scoped DAO
    // query. When `None` (worldless Works, no open findings, or auto-chain
    // path without pool access yet), default to empty string so strict-mode
    // template rendering does not fail on `{{preset.input.open_findings_block}}`.
    // The preset's `{{#if open_findings_block}}` guard omits the section.
    let findings_block = fields.open_findings_block.clone().unwrap_or_default();
    map.as_object_mut().map(|o| {
        o.insert(
            "open_findings_block".to_string(),
            serde_json::Value::String(findings_block),
        )
    });

    map
}

/// Read rules content from Layer 1 (embedded default) and Layer 2 (per-work file).
///
/// Returns a combined Markdown string with both layers when at least one
/// layer is present. Returns `None` when no rules content is available
/// (neither Layer 1 default nor Layer 2 per-work file exists).
///
/// # Layer resolution (DF-65; V1.48 P2 — `AGENTS.md` migration)
///
/// - **Layer 1**: `crates/nexus-orchestration/embedded-rules/writing-craft.md`
///   (compiled into the binary via `include_str!`). User override at
///   `~/.nexus42/rules/writing-craft.md` takes precedence when it exists.
/// - **Layer 2 (preferred)**: `Works/<work_ref>/AGENTS.md` — V1.47 normative
///   per [novel-writing/workflow-profile.md §5.5.4]. New scaffolds write this path.
/// - **Layer 2 (legacy fallback, read-only)**: `Works/<work_ref>/Rules/novel-rules.md`
///   — used only when `AGENTS.md` is absent, for Works scaffolded before the
///   V1.48 migration. No bulk migration is performed (compass §0.1 #9).
#[must_use]
pub fn read_rules_layers(workspace_dir: &str, work_ref: &str) -> Option<String> {
    let mut parts = Vec::new();

    // Layer 1: embedded default (or user override).
    // Read from the dedicated embedded_rules module (not embedded-presets).
    // User override at ~/.nexus42/rules/writing-craft.md is a future addition
    // (requires home dir resolution at the call site).
    {
        let layer1 = crate::embedded_rules::WRITING_CRAFT;
        if !layer1.is_empty() {
            parts.push(format!(
                "## Layer 1 — Writing Craft Rules (shared)\n\n{layer1}"
            ));
        }
    }

    // Layer 2: per-work rules file.
    // V1.48 P2: prefer `Works/<work_ref>/AGENTS.md` (V1.47 normative); fall
    // back to legacy `Works/<work_ref>/Rules/novel-rules.md` read-only when
    // the AGENTS.md file is absent. The fallback preserves backward
    // compatibility for Works scaffolded before the migration; no bulk
    // migration is performed (compass §0.1 #9, overlay §3.3).
    let ws = std::path::Path::new(workspace_dir);
    let agents_md = nexus_home_layout::work_agents_md_path(ws, work_ref);
    let legacy_rules = nexus_home_layout::work_novel_rules_path(ws, work_ref);

    let layer2_content = std::fs::read_to_string(&agents_md)
        .or_else(|_| std::fs::read_to_string(&legacy_rules))
        .ok();
    if let Some(content) = layer2_content {
        if !content.trim().is_empty() {
            parts.push(format!("## Layer 2 — Novel Rules (per-work)\n\n{content}"));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n---\n\n"))
    }
}

/// Build a correctly-shaped `AddScheduleRequest` for an FL-E stage advance
/// (R-FL-E-P2-03: shared facade, no CLI-side bespoke DTO).
///
/// This is the single place that constructs the schedule creation request
/// for stage advances. The CLI and any future daemon-side auto-scheduling
/// should call this function instead of hand-building JSON.
///
/// # Arguments
/// * `stage` — Target FL-E stage (e.g. `"research"`)
/// * `creator_id` — Creator who owns the work/schedule
/// * `work_fields` — Work fields for preset input seeding
///
/// # Returns
/// `Some(AddScheduleRequest)` if the stage has a valid preset mapping,
/// `None` for unknown stages.
#[must_use]
pub fn build_schedule_for_stage(
    stage: &str,
    creator_id: &str,
    work_fields: &WorkFields,
) -> Option<AddScheduleRequest> {
    let pid = preset_for_stage(stage)?;

    let preset_input = build_preset_input(work_fields);

    Some(AddScheduleRequest {
        creator_id: creator_id.to_string(),
        preset_id: pid.to_string(),
        seed: Some(serde_json::to_string(&preset_input).unwrap_or_default()),
        label: Some(build_stage_schedule_label(stage, &work_fields.work_id)),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(preset_input),
        force_gates: false,
        reason: None,
    })
}

/// Error returned when a stage advance fails validation.
///
/// Carries a machine-readable `code` (stable error code for CLI automation)
/// and a human-readable `message`.
#[derive(Debug, Clone)]
pub struct StageGateError {
    /// Machine-readable error code (e.g. `UNKNOWN_STAGE`, `ACTIVE_SCHEDULE`).
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for StageGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for StageGateError {}

/// Input state for gate validation.
#[derive(Debug, Clone)]
pub struct WorkStageState {
    /// Current stage name (e.g. "intake").
    pub current_stage: String,
    /// Current `stage_status` (e.g. "complete").
    pub stage_status: String,
    /// V1.33 `intake_status` (e.g. "complete").
    pub intake_status: String,
}

/// Validate a stage advance request (spec §3.3 gates).
///
/// # Errors
///
/// Returns `StageGateError` with a descriptive message if any gate fails.
pub fn check_stage_advance(
    work: &WorkStageState,
    target_stage: &str,
    force: bool,
) -> Result<(), StageGateError> {
    // Gate 0: target must be a known stage
    let target_idx = stage_index(target_stage).ok_or_else(|| StageGateError {
        code: "FL_E_UNKNOWN_STAGE".to_string(),
        message: format!(
            "Unknown stage '{target_stage}'. Valid stages: {}",
            FL_E_STAGES.join(", ")
        ),
    })?;

    let current_idx = stage_index(&work.current_stage).unwrap_or(0);

    if !force {
        // Gate 1: cannot advance to same stage
        if target_stage == work.current_stage {
            return Err(StageGateError {
                code: "FL_E_SAME_STAGE".to_string(),
                message: format!(
                    "Work is already at stage '{}' ({}). Use a different target stage.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 2: cannot advance backwards
        if target_idx <= current_idx {
            return Err(StageGateError {
                code: "FL_E_BACKWARDS_ADVANCE".to_string(),
                message: format!(
                    "Cannot advance backwards from '{}' to '{}'. Stage order: {}",
                    work.current_stage,
                    target_stage,
                    FL_E_STAGES.join(" → ")
                ),
            });
        }

        // Gate 3: strict linear advance (+1 only)
        if target_idx != current_idx + 1 {
            let next_stage = FL_E_STAGES.get(current_idx + 1).unwrap_or(&"(unknown)");
            return Err(StageGateError {
                code: "FL_E_STAGE_SKIP".to_string(),
                message: format!(
                    "Cannot skip from '{}' to '{}'; expected next stage is '{}'. \
                     Use --force to skip stages.",
                    work.current_stage, target_stage, next_stage
                ),
            });
        }

        // Gate 4: at most one active FL-E stage schedule per Work (spec §2 #4)
        // Checked before completion gate so active-stage errors are more specific.
        if work.stage_status == "active" {
            return Err(StageGateError {
                code: "FL_E_ACTIVE_SCHEDULE".to_string(),
                message: format!(
                    "Work already has an active stage schedule ('{}' is '{}'). \
                     Wait for the current stage to complete or cancel before advancing.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 5: current stage must be complete (except intake, handled separately)
        if work.stage_status != "complete" && work.stage_status != "skipped" && current_idx > 0 {
            return Err(StageGateError {
                code: "FL_E_INCOMPLETE_STAGE".to_string(),
                message: format!(
                    "Current stage '{}' is '{}', not 'complete'. \
                     Complete the current stage first, or use --force to override.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 6: intake must be complete before advancing past it
        // (uses intake_status from V1.33, not stage_status)
        if work.current_stage == "intake" && work.intake_status != "complete" {
            return Err(StageGateError {
                code: "FL_E_INTAKE_INCOMPLETE".to_string(),
                message: format!(
                    "Cannot advance past intake: intake_status is '{}'. \
                     Complete intake first, or use --force to override.",
                    work.intake_status
                ),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_state(stage: &str, status: &str, intake: &str) -> WorkStageState {
        WorkStageState {
            current_stage: stage.to_string(),
            stage_status: status.to_string(),
            intake_status: intake.to_string(),
        }
    }

    #[test]
    fn valid_advance_intake_to_research() {
        let work = work_state("intake", "complete", "complete");
        assert!(check_stage_advance(&work, "research", false).is_ok());
    }

    #[test]
    fn reject_unknown_target() {
        let work = work_state("intake", "pending", "pending");
        let err = check_stage_advance(&work, "unknown", false).unwrap_err();
        assert_eq!(err.code, "FL_E_UNKNOWN_STAGE");
        assert!(err.message.contains("Unknown stage"));
    }

    #[test]
    fn reject_same_stage() {
        let work = work_state("research", "active", "complete");
        let err = check_stage_advance(&work, "research", false).unwrap_err();
        assert_eq!(err.code, "FL_E_SAME_STAGE");
        assert!(err.message.contains("already at stage"));
    }

    #[test]
    fn reject_backwards() {
        let work = work_state("research", "complete", "complete");
        let err = check_stage_advance(&work, "intake", false).unwrap_err();
        assert_eq!(err.code, "FL_E_BACKWARDS_ADVANCE");
        assert!(err.message.contains("backwards"));
    }

    #[test]
    fn reject_skip_without_force() {
        let work = work_state("intake", "complete", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert_eq!(err.code, "FL_E_STAGE_SKIP");
        assert!(err.message.contains("Cannot skip"));
    }

    #[test]
    fn allow_skip_with_force() {
        let work = work_state("intake", "pending", "pending");
        assert!(check_stage_advance(&work, "produce", true).is_ok());
    }

    #[test]
    fn reject_incomplete_current() {
        let work = work_state("research", "pending", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert_eq!(err.code, "FL_E_INCOMPLETE_STAGE");
        assert!(err.message.contains("not 'complete'"));
    }

    #[test]
    fn reject_intake_not_complete() {
        let work = work_state("intake", "complete", "pending");
        let err = check_stage_advance(&work, "research", false).unwrap_err();
        assert_eq!(err.code, "FL_E_INTAKE_INCOMPLETE");
        assert!(err.message.contains("intake_status"));
    }

    #[test]
    fn reject_active_schedule_exists() {
        let work = work_state("research", "active", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert_eq!(err.code, "FL_E_ACTIVE_SCHEDULE");
        assert!(err.message.contains("active stage schedule"));
    }

    #[test]
    fn allow_advance_after_complete() {
        let work = work_state("research", "complete", "complete");
        assert!(check_stage_advance(&work, "produce", false).is_ok());
    }

    // ── T1: preset_for_stage schedule wiring for all 4 post-intake stages ──────

    #[test]
    fn preset_for_stage_research() {
        assert_eq!(preset_for_stage("research"), Some("research"));
    }

    #[test]
    fn preset_for_stage_produce() {
        assert_eq!(preset_for_stage("produce"), Some("novel-writing"));
    }

    #[test]
    fn preset_for_stage_review() {
        assert_eq!(preset_for_stage("review"), Some("novel-chapter-review"));
    }

    #[test]
    fn preset_for_stage_persist() {
        assert_eq!(preset_for_stage("persist"), Some("kb-extract"));
    }

    #[test]
    fn preset_for_stage_intake() {
        assert_eq!(preset_for_stage("intake"), Some("creative-brief-intake"));
    }

    #[test]
    fn preset_for_stage_unknown() {
        assert_eq!(preset_for_stage("unknown"), None);
    }

    #[test]
    fn schedule_label_format() {
        let label = build_stage_schedule_label("research", "wrk_abc123");
        assert_eq!(label, "FL-E stage: research (work: wrk_abc123)");
    }

    /// End-to-end gate + schedule wiring for each stage transition.
    /// Validates that stage advance gates pass AND the correct preset is resolved.
    #[test]
    fn full_chain_gate_and_preset_resolution() {
        // intake → research
        let intake_done = work_state("intake", "complete", "complete");
        assert!(check_stage_advance(&intake_done, "research", false).is_ok());
        assert_eq!(preset_for_stage("research"), Some("research"));

        // research → produce
        let research_done = work_state("research", "complete", "complete");
        assert!(check_stage_advance(&research_done, "produce", false).is_ok());
        assert_eq!(preset_for_stage("produce"), Some("novel-writing"));

        // produce → review
        let produce_done = work_state("produce", "complete", "complete");
        assert!(check_stage_advance(&produce_done, "review", false).is_ok());
        assert_eq!(preset_for_stage("review"), Some("novel-chapter-review"));

        // review → persist
        let review_done = work_state("review", "complete", "complete");
        assert!(check_stage_advance(&review_done, "persist", false).is_ok());
        assert_eq!(preset_for_stage("persist"), Some("kb-extract"));
    }

    // ── T2: preset input templates consume Work fields ─────────────────────

    fn demo_work_fields(stage: &str) -> WorkFields {
        WorkFields {
            work_id: "wrk_demo123".to_string(),
            fl_e_stage: stage.to_string(),
            creative_brief: r#"{"genre":"sci-fi","tone":"literary"}"#.to_string(),
            inspiration_log: r#"[{"note":"first angle"}]"#.to_string(),
            work_ref: Some("my-novel".to_string()),
            chapter: Some(1),
            chapter_label: Some("01".to_string()),
            outline_path: Some("Works/my-novel/Outlines/chapters/ch01-outline.md".to_string()),
            body_path: Some("Works/my-novel/Stories/ch01-ch01.md".to_string()),
            slug: Some("ch01".to_string()),
            research_artifacts_dir: None,
            workspace_dir: None,
            world_kb_block: None,
            open_findings_block: None,
            world_id: None,
            volume: None,
        }
    }

    #[test]
    fn build_preset_input_contains_work_id() {
        let fields = demo_work_fields("research");
        let input = build_preset_input(&fields);
        assert_eq!(input["work_id"], "wrk_demo123");
    }

    #[test]
    fn build_preset_input_contains_fl_e_stage() {
        let fields = demo_work_fields("produce");
        let input = build_preset_input(&fields);
        assert_eq!(input["fl_e_stage"], "produce");
    }

    #[test]
    fn build_preset_input_contains_creative_brief() {
        let fields = demo_work_fields("produce");
        let input = build_preset_input(&fields);
        assert!(input["creative_brief"].is_string());
        assert!(!input["creative_brief"].as_str().unwrap().is_empty());
    }

    #[test]
    fn build_preset_input_contains_inspiration_log() {
        let fields = demo_work_fields("research");
        let input = build_preset_input(&fields);
        // inspiration_log should be a valid JSON array string
        let log = input["inspiration_log"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(log).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn preset_input_research_consumes_creative_brief_and_inspiration() {
        // T2 verification: research stage preset receives creative_brief + inspiration_log
        let fields = demo_work_fields("research");
        let input = build_preset_input(&fields);
        assert!(
            input.get("creative_brief").is_some(),
            "research preset input must include creative_brief"
        );
        assert!(
            input.get("inspiration_log").is_some(),
            "research preset input must include inspiration_log"
        );
        assert_eq!(preset_for_stage("research"), Some("research"));
    }

    #[test]
    fn preset_input_produce_consumes_creative_brief_and_inspiration() {
        // T2 verification: novel-writing (produce) receives creative_brief + inspiration_log
        let fields = demo_work_fields("produce");
        let input = build_preset_input(&fields);
        assert!(
            input.get("creative_brief").is_some(),
            "produce preset input must include creative_brief"
        );
        assert!(
            input.get("inspiration_log").is_some(),
            "produce preset input must include inspiration_log"
        );
        assert_eq!(preset_for_stage("produce"), Some("novel-writing"));
    }

    #[test]
    fn preset_input_review_receives_work_id() {
        // T2 verification: novel-chapter-review (review) receives work_id for context
        let fields = demo_work_fields("review");
        let input = build_preset_input(&fields);
        assert!(
            input.get("work_id").is_some(),
            "review preset input must include work_id"
        );
        assert_eq!(preset_for_stage("review"), Some("novel-chapter-review"));
    }

    #[test]
    fn preset_input_persist_receives_work_id() {
        // T2 verification: kb-extract (persist) receives work_id for KB extraction
        let fields = demo_work_fields("persist");
        let input = build_preset_input(&fields);
        assert!(
            input.get("work_id").is_some(),
            "persist preset input must include work_id"
        );
        assert_eq!(preset_for_stage("persist"), Some("kb-extract"));
    }

    // ── R-FL-E-P2-03: build_schedule_for_stage facade ──────────────────────

    #[test]
    fn build_schedule_for_stage_research() {
        let fields = demo_work_fields("research");
        let req = build_schedule_for_stage("research", "ctr_test", &fields)
            .expect("research should have a preset");
        assert_eq!(req.preset_id, "research");
        assert_eq!(req.creator_id, "ctr_test");
        assert!(req.seed.is_some());
        let seed_json: serde_json::Value = serde_json::from_str(&req.seed.unwrap()).unwrap();
        assert_eq!(seed_json["work_id"], "wrk_demo123");
        assert_eq!(seed_json["fl_e_stage"], "research");
        assert!(req.label.unwrap().contains("research"));
    }

    #[test]
    fn build_schedule_for_stage_produce() {
        let fields = demo_work_fields("produce");
        let req = build_schedule_for_stage("produce", "ctr_test", &fields)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");
    }

    #[test]
    fn build_schedule_for_stage_review() {
        let fields = demo_work_fields("review");
        let req = build_schedule_for_stage("review", "ctr_test", &fields)
            .expect("review should have a preset");
        assert_eq!(req.preset_id, "novel-chapter-review");
    }

    #[test]
    fn build_schedule_for_stage_persist() {
        let fields = demo_work_fields("persist");
        let req = build_schedule_for_stage("persist", "ctr_test", &fields)
            .expect("persist should have a preset");
        assert_eq!(req.preset_id, "kb-extract");
    }

    #[test]
    fn build_schedule_for_stage_unknown_returns_none() {
        let fields = demo_work_fields("unknown");
        assert!(
            build_schedule_for_stage("unknown", "ctr_test", &fields).is_none(),
            "unknown stage should return None"
        );
    }

    #[test]
    fn build_schedule_for_stage_seed_contains_all_work_fields() {
        let fields = demo_work_fields("research");
        let req = build_schedule_for_stage("research", "ctr_test", &fields).unwrap();
        let seed: serde_json::Value = serde_json::from_str(&req.seed.unwrap()).unwrap();
        assert_eq!(seed["work_id"], "wrk_demo123");
        assert_eq!(seed["fl_e_stage"], "research");
        assert!(seed["creative_brief"].is_string());
        assert!(seed["inspiration_log"].is_string());
    }

    // ── V1.38 P1: chapter context parameterization tests ─────────────────────

    /// Helper: produce `WorkFields` for a given chapter number.
    fn chapter_work_fields(chapter: i32, work_ref: &str) -> WorkFields {
        let ch_label = chapter_label(chapter);
        WorkFields {
            work_id: format!("wrk_{work_ref}"),
            fl_e_stage: "produce".to_string(),
            creative_brief: r#"{"genre":"sci-fi"}"#.to_string(),
            inspiration_log: "[]".to_string(),
            work_ref: Some(work_ref.to_string()),
            chapter: Some(chapter),
            chapter_label: Some(ch_label.clone()),
            outline_path: Some(format!(
                "Works/{work_ref}/Outlines/chapters/ch{ch_label}-outline.md"
            )),
            body_path: Some(format!(
                "Works/{work_ref}/Stories/ch{ch_label}-ch{ch_label}.md"
            )),
            slug: Some(format!("ch{ch_label}")),
            research_artifacts_dir: None,
            workspace_dir: None,
            world_kb_block: None,
            open_findings_block: None,
            world_id: None,
            volume: None,
        }
    }

    #[test]
    fn build_preset_input_chapter2_includes_all_context_fields() {
        let fields = chapter_work_fields(2, "my-novel");
        let input = build_preset_input(&fields);
        assert_eq!(input["chapter"], 2);
        assert_eq!(input["chapter_label"], "02");
        assert_eq!(
            input["outline_path"],
            "Works/my-novel/Outlines/chapters/ch02-outline.md"
        );
        assert_eq!(input["body_path"], "Works/my-novel/Stories/ch02-ch02.md");
        assert_eq!(input["slug"], "ch02");
    }

    #[test]
    fn build_preset_input_chapter10_label_is_not_triple_digit() {
        let fields = chapter_work_fields(10, "epic-saga");
        let input = build_preset_input(&fields);
        assert_eq!(input["chapter"], 10);
        assert_eq!(
            input["chapter_label"], "10",
            "chapter 10 label must be '10', not '010'"
        );
        assert_eq!(
            input["outline_path"],
            "Works/epic-saga/Outlines/chapters/ch10-outline.md"
        );
    }

    #[test]
    fn build_preset_input_chapter1_compat() {
        let fields = chapter_work_fields(1, "my-novel");
        let input = build_preset_input(&fields);
        assert_eq!(input["chapter"], 1);
        assert_eq!(input["chapter_label"], "01");
        assert!(input["outline_path"]
            .as_str()
            .unwrap()
            .ends_with("ch01-outline.md"));
        assert!(input["body_path"]
            .as_str()
            .unwrap()
            .ends_with("ch01-ch01.md"));
    }

    #[test]
    fn build_preset_input_omits_none_fields() {
        let fields = WorkFields {
            work_id: "wrk_minimal".to_string(),
            fl_e_stage: "produce".to_string(),
            creative_brief: "{}".to_string(),
            inspiration_log: "[]".to_string(),
            work_ref: None,
            chapter: None,
            chapter_label: None,
            outline_path: None,
            body_path: None,
            slug: None,
            research_artifacts_dir: None,
            workspace_dir: None,
            world_kb_block: None,
            open_findings_block: None,
            world_id: None,
            volume: None,
        };
        let input = build_preset_input(&fields);
        assert!(input.get("chapter").is_none());
        assert!(input.get("chapter_label").is_none());
        assert!(input.get("outline_path").is_none());
        assert!(input.get("body_path").is_none());
        assert!(input.get("slug").is_none());
        // Base fields still present
        assert!(input.get("work_id").is_some());
        assert!(input.get("fl_e_stage").is_some());
        // Fix W-2: research_artifacts_dir not present when None
        assert!(input.get("research_artifacts_dir").is_none());
        // V1.40 P2: world_kb_block defaults to empty string for worldless Works
        assert_eq!(input["world_kb_block"], "");
    }

    #[test]
    fn build_preset_input_includes_research_artifacts_dir_when_set() {
        let mut fields = demo_work_fields("produce");
        fields.research_artifacts_dir =
            Some(".nexus42/references/ACH20260609120000000/".to_string());
        let input = build_preset_input(&fields);
        assert_eq!(
            input["research_artifacts_dir"],
            ".nexus42/references/ACH20260609120000000/"
        );
    }

    #[test]
    fn schedule_for_produce_chapter2_includes_all_context() {
        let fields = chapter_work_fields(2, "cozy-mystery");
        let req = build_schedule_for_stage("produce", "ctr_test", &fields)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");

        let input = req.input.expect("input should be set");
        assert_eq!(input["chapter"], 2);
        assert_eq!(input["chapter_label"], "02");
        assert_eq!(
            input["outline_path"],
            "Works/cozy-mystery/Outlines/chapters/ch02-outline.md"
        );
        assert_eq!(
            input["body_path"],
            "Works/cozy-mystery/Stories/ch02-ch02.md"
        );
        assert_eq!(input["slug"], "ch02");

        // Seed should also contain the chapter context
        let seed: serde_json::Value = serde_json::from_str(&req.seed.unwrap()).unwrap();
        assert_eq!(seed["chapter"], 2);
        assert_eq!(seed["chapter_label"], "02");
    }

    #[test]
    fn chapter_label_formats_zero_padded_for_1_to_99() {
        assert_eq!(chapter_label(1), "01");
        assert_eq!(chapter_label(9), "09");
        assert_eq!(chapter_label(10), "10");
        assert_eq!(chapter_label(99), "99");
        assert_eq!(chapter_label(100), "100");
    }

    // ── V1.39 P3 → V1.40 P0.5: rules reader tests ──────────────────────
    // (V1.40 P0.5 migrated rules from embedded-presets/ to embedded_rules.rs)

    #[test]
    fn read_rules_layers_returns_layer1_from_embedded() {
        // Layer 1 is always available from embedded_rules module (compile-time include_str!)
        let result = read_rules_layers("/nonexistent/workspace", "my-novel");
        assert!(
            result.is_some(),
            "Layer 1 embedded content should always be present"
        );
        let content = result.expect("content");
        assert!(
            content.contains("Layer 1 — Writing Craft Rules"),
            "should contain Layer 1 header"
        );
        assert!(
            content.contains("Five-Question Gate"),
            "should contain embedded writing craft content"
        );
    }

    #[test]
    fn read_rules_layers_returns_both_layers_when_layer2_exists() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        // Create Layer 2 file
        let rules_dir = ws.join("Works").join("test-novel").join("Rules");
        std::fs::create_dir_all(&rules_dir).expect("mkdir");
        std::fs::write(
            rules_dir.join("novel-rules.md"),
            "# My Rules\n\n- POV: first person\n",
        )
        .expect("write");

        let result = read_rules_layers(&ws.to_string_lossy(), "test-novel");
        assert!(result.is_some());
        let content = result.expect("content");
        assert!(
            content.contains("Layer 1 — Writing Craft Rules"),
            "should contain Layer 1"
        );
        assert!(
            content.contains("Layer 2 — Novel Rules"),
            "should contain Layer 2"
        );
        assert!(
            content.contains("POV: first person"),
            "should contain Layer 2 content"
        );
    }

    #[test]
    fn read_rules_layers_skips_empty_layer2() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        // Create empty Layer 2 file
        let rules_dir = ws.join("Works").join("empty-novel").join("Rules");
        std::fs::create_dir_all(&rules_dir).expect("mkdir");
        std::fs::write(rules_dir.join("novel-rules.md"), "  \n").expect("write");

        let result = read_rules_layers(&ws.to_string_lossy(), "empty-novel");
        assert!(result.is_some());
        let content = result.expect("content");
        assert!(
            content.contains("Layer 1"),
            "Layer 1 should still be present"
        );
        assert!(
            !content.contains("Layer 2"),
            "empty Layer 2 should not appear"
        );
    }

    // ── V1.48 P2: AGENTS.md preference (overlay §3.1) ──────────────────

    #[test]
    fn read_rules_layers_prefers_agents_md_when_present() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();
        let work_root = ws.join("Works").join("neon-river");

        // Create BOTH the new AGENTS.md and the legacy Rules/novel-rules.md.
        // The AGENTS.md content must win.
        std::fs::create_dir_all(&work_root).expect("mkdir root");
        std::fs::write(
            work_root.join("AGENTS.md"),
            "# AGENTS.md — neon-river\n\n- POV: first (from AGENTS.md)\n",
        )
        .expect("write agents");
        std::fs::create_dir_all(work_root.join("Rules")).expect("mkdir rules");
        std::fs::write(
            work_root.join("Rules").join("novel-rules.md"),
            "- POV: third (from legacy)\n",
        )
        .expect("write legacy");

        let result = read_rules_layers(&ws.to_string_lossy(), "neon-river");
        let content = result.expect("content");
        assert!(
            content.contains("from AGENTS.md"),
            "AGENTS.md should win over legacy Rules/novel-rules.md"
        );
        assert!(
            !content.contains("from legacy"),
            "legacy Rules/novel-rules.md must NOT be read when AGENTS.md exists"
        );
    }

    #[test]
    fn read_rules_layers_falls_back_to_legacy_when_agents_md_absent() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        // Only legacy file exists (pre-V1.48 Work).
        let rules_dir = ws.join("Works").join("legacy-novel").join("Rules");
        std::fs::create_dir_all(&rules_dir).expect("mkdir");
        std::fs::write(
            rules_dir.join("novel-rules.md"),
            "- Tense: past (legacy fallback)\n",
        )
        .expect("write");

        let result = read_rules_layers(&ws.to_string_lossy(), "legacy-novel");
        let content = result.expect("content");
        assert!(
            content.contains("legacy fallback"),
            "legacy Rules/novel-rules.md should be read when AGENTS.md is absent"
        );
    }

    #[test]
    fn read_rules_layers_neither_agents_md_nor_legacy_returns_layer1_only() {
        // No Layer 2 files at all (fresh Work with neither file).
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();
        std::fs::create_dir_all(ws.join("Works").join("bare-novel")).expect("mkdir");

        let result = read_rules_layers(&ws.to_string_lossy(), "bare-novel");
        assert!(result.is_some(), "Layer 1 must still be present");
        let content = result.expect("content");
        assert!(content.contains("Layer 1"));
        assert!(!content.contains("Layer 2"));
    }

    #[test]
    fn build_preset_input_includes_rules_content_when_workspace_dir_set() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        // Create Layer 2 file
        let rules_dir = ws.join("Works").join("rules-test").join("Rules");
        std::fs::create_dir_all(&rules_dir).expect("mkdir");
        std::fs::write(rules_dir.join("novel-rules.md"), "- Tense: present\n").expect("write");

        let fields = WorkFields {
            work_id: "wrk_test".to_string(),
            fl_e_stage: "produce".to_string(),
            creative_brief: "{}".to_string(),
            inspiration_log: "[]".to_string(),
            work_ref: Some("rules-test".to_string()),
            chapter: Some(1),
            chapter_label: Some("01".to_string()),
            outline_path: None,
            body_path: None,
            slug: None,
            research_artifacts_dir: None,
            workspace_dir: Some(ws.to_string_lossy().to_string()),
            world_kb_block: None,
            open_findings_block: None,
            world_id: None,
            volume: None,
        };

        let input = build_preset_input(&fields);
        assert!(
            input.get("rules_content").is_some(),
            "rules_content should be present when workspace_dir is set"
        );
        let rules = input["rules_content"].as_str().expect("string");
        assert!(rules.contains("Tense: present"));
    }

    #[test]
    fn build_preset_input_omits_rules_content_when_no_workspace_dir() {
        let fields = WorkFields {
            work_id: "wrk_test".to_string(),
            fl_e_stage: "produce".to_string(),
            creative_brief: "{}".to_string(),
            inspiration_log: "[]".to_string(),
            work_ref: Some("my-novel".to_string()),
            chapter: Some(1),
            chapter_label: Some("01".to_string()),
            outline_path: None,
            body_path: None,
            slug: None,
            research_artifacts_dir: None,
            workspace_dir: None,
            world_kb_block: None,
            open_findings_block: None,
            world_id: None,
            volume: None,
        };

        let input = build_preset_input(&fields);
        assert!(
            input.get("rules_content").is_none(),
            "rules_content should be absent when workspace_dir is None"
        );
    }

    // ── V1.40 P2: world_kb_block preset input tests ─────────────────────────

    #[test]
    fn build_preset_input_includes_world_kb_block_when_set() {
        let mut fields = demo_work_fields("produce");
        fields.world_kb_block = Some("world_id: wld_1\nworld_name: Test\n".to_string());
        let input = build_preset_input(&fields);
        assert!(
            input.get("world_kb_block").is_some(),
            "world_kb_block should be present when set"
        );
        assert_eq!(
            input["world_kb_block"],
            "world_id: wld_1\nworld_name: Test\n"
        );
    }

    #[test]
    fn build_preset_input_world_kb_block_defaults_to_empty_for_worldless() {
        let fields = demo_work_fields("produce");
        let input = build_preset_input(&fields);
        // When world_kb_block is None (worldless), the key is still present
        // as empty string so strict-mode templates don't fail.
        assert_eq!(input["world_kb_block"], "");
    }

    // ── V1.49 P1: foreshadowing_summary preset input tests ──────────────────

    #[test]
    fn build_preset_input_includes_foreshadowing_summary_when_index_populated() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();
        let work = ws.join("Works").join("novel-fs");
        let outlines = work.join("Outlines");
        std::fs::create_dir_all(&outlines).expect("mkdir");
        // Populate the index with two rows.
        std::fs::write(
            outlines.join("foreshadowing.md"),
            crate::narrative_index::serialize_foreshadowing_index(&[
                crate::narrative_index::ForeshadowingRow {
                    id: "F001".to_string(),
                    description: "the locket".to_string(),
                    planted: "1".to_string(),
                    paid_off: String::new(),
                    status: "planned".to_string(),
                },
                crate::narrative_index::ForeshadowingRow {
                    id: "F002".to_string(),
                    description: "the prophecy".to_string(),
                    planted: "2".to_string(),
                    paid_off: String::new(),
                    status: "buried".to_string(),
                },
            ]),
        )
        .expect("write");

        let mut fields = demo_work_fields("produce");
        fields.work_ref = Some("novel-fs".to_string());
        fields.workspace_dir = Some(ws.to_string_lossy().to_string());

        let input = build_preset_input(&fields);
        let summary = input["foreshadowing_summary"]
            .as_str()
            .expect("foreshadowing_summary must be present");
        assert!(summary.contains("- F001 | the locket | planned"));
        assert!(summary.contains("- F002 | the prophecy | buried"));
    }

    #[test]
    fn build_preset_input_foreshadowing_summary_empty_when_index_absent() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();
        // No Works/ tree at all.
        let mut fields = demo_work_fields("produce");
        fields.work_ref = Some("no-such-novel".to_string());
        fields.workspace_dir = Some(ws.to_string_lossy().to_string());

        let input = build_preset_input(&fields);
        // Key present as empty string (strict-mode safe); the {{#if}} guard
        // in the prompt template omits the section.
        assert_eq!(input["foreshadowing_summary"], "");
    }

    #[test]
    fn build_preset_input_foreshadowing_summary_empty_when_no_workspace_dir() {
        // Auto-chain enqueue path sets workspace_dir=None → summary stays empty
        // (no workspace read attempted).
        let fields = demo_work_fields("produce");
        let input = build_preset_input(&fields);
        assert_eq!(input["foreshadowing_summary"], "");
    }
}
