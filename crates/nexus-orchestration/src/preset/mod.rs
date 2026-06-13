//! Preset loader module.
//!
//! Loads preset bundles (YAML manifest + optional prompt templates) and
//! validates them per `orchestration-engine.md` §7.6.
//!
//! Types: `nexus-contracts::local::orchestration::preset`.
//! Loader + validation: this module (`loader.rs`).
//! Semantic validation facade: `validation.rs` (V1.32 P1).
//! Embedded presets: `include_dir!` at compile time (§7.1 location #3).
//!
//! ## V1.32 Validator boundary notes (A7)
//!
//! The `validation.rs` module provides `validate_preset_semantic()` and
//! `validate_assets_in_bundle()` as the **single shared validation surface**
//! for both the runtime loader path and the daemon API endpoint. The existing
//! `loader::validate_manifest()` continues to run during `load_preset_from_str()`
//! for backward compatibility. The new semantic checks are additive — they
//! produce `ValidationDiagnostic` values (not `ValidationProblem`) with richer
//! metadata (severity, category).
//!
//! The daemon `POST /v1/local/presets:validate` handler is being updated to
//! route through the shared validation facade. P4 will own the broader spec
//! update in `orchestration-engine.md` §7.6/§8 to document the unified
//! validation contract. Key design decisions:
//!
//! - **Orphan inner graphs = WARNING** (not error): allows presets to define
//!   utility graphs for future use without breaking validation.
//! - **Capability arg drift** checks are best-effort: the registry exposes
//!   `input_schema()` as a JSON string; we parse it for top-level `properties`
//!   and `required` but do not perform full JSON Schema validation.
//! - **A6 decision: NO new CLI wrapper** — the daemon endpoint is the only
//!   user-facing validation surface. Adding a CLI subcommand would require
//!   updating `cli-spec.md` and command-surface contract tests, which is
//!   deferred to a future plan.

use crate::capability::CapabilityRegistry;
use crate::system_preset_dir;
use crate::user_preset_dir;
use include_dir::include_dir;
use include_dir::Dir;
use std::path::Path;

pub mod loader;
pub mod manifest;
pub mod validation;

pub use loader::{
    load_preset, load_preset_from_str, load_preset_from_str_with_limits,
    loader_validate_manifest_compat, yaml_value_depth, LoadedPreset, PresetLoadError,
    ValidationProblem, DEFAULT_MAX_YAML_DEPTH, DEFAULT_MAX_YAML_SIZE,
};
pub use validation::{
    validate_assets_in_bundle, validate_path_safety, validate_preset_semantic, DiagnosticCategory,
    DiagnosticSeverity, ValidationDiagnostic, ValidationResult,
};

// ---------------------------------------------------------------------------
// Embedded presets
// ---------------------------------------------------------------------------

/// Embedded presets directory, compiled into the binary at build time.
///
/// Location: `crates/nexus-orchestration/embedded-presets/`
/// Structure per §7.1: `<preset-id>/preset.yaml` + `prompts/*.md`
/// QC1 W-2: pub(crate) so the `preset_version_for_id` sync test in `auto_chain`
/// can read preset.yaml contents.
pub(crate) static EMBEDDED_PRESETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/embedded-presets");

/// Load an embedded preset by ID.
///
/// Searches the compiled-in `embedded-presets/` directory for a subdirectory
/// matching `id`, reads `preset.yaml`, and delegates to
/// [`load_preset_from_str`] for parsing + validation.
///
/// # Errors
///
/// Returns [`PresetLoadError`] if:
/// - No embedded preset with the given `id` exists
/// - `preset.yaml` is missing or fails to parse
/// - Validation fails per §7.6
///
/// # Example
///
/// ```ignore
/// let caps = CapabilityRegistry::with_builtins();
/// let loaded = load_embedded_preset("novel-writing", &caps)?;
/// assert_eq!(loaded.id, "novel-writing");
/// ```
pub fn load_embedded_preset(
    id: &str,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    // Read preset.yaml from the embedded tree.
    let preset_file = EMBEDDED_PRESETS
        .get_file(format!("{id}/preset.yaml"))
        .ok_or_else(|| PresetLoadError::NotFound {
            preset_id: id.to_string(),
        })?;

    let yaml = preset_file
        .contents_utf8()
        .ok_or_else(|| PresetLoadError::Validation {
            len: 1,
            problems: vec![loader::ValidationProblem {
                path: format!("{id}/preset.yaml"),
                error: "preset.yaml contains invalid UTF-8".into(),
            }],
        })?;

    load_preset_from_str(yaml, caps)
}

// ---------------------------------------------------------------------------
// Composable search order: user → system → embedded
// ---------------------------------------------------------------------------

/// Resolve a preset by ID using the composable search order.
///
/// Search order (highest priority first):
/// 1. **User presets** — `~/.nexus42/presets/<id>/` — overrides embedded presets with same ID
/// 2. **System presets** — `~/.nexus42/presets/_system/<id>/` — qualified as `_system.<id>`
/// 3. **Embedded presets** — compiled into the binary at build time
///
/// If a user preset has the same `id` as an embedded preset, the user version
/// wins (first found = returned). System presets use `_system.<name>` qualified
/// IDs, so they don't directly collide with user/embedded IDs unless queried
/// with the qualified form.
///
/// # Arguments
///
/// * `id` — The preset ID to resolve (e.g., `"novel-writing"` or `"_system.maintenance"`)
/// * `nexus_home` — Path to `~/.nexus42/` for user/system preset scanning
/// * `caps` — Capability registry for validation
///
/// # Errors
///
/// Returns [`PresetLoadError`] if the preset is not found in any source, or
/// if loading/validation fails.
pub fn resolve_preset(
    id: &str,
    nexus_home: &Path,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    // 1. Try user presets (unless the ID starts with `_system.`).
    if !id.starts_with("_system.") {
        let user_result = user_preset_dir::scan_user_presets(nexus_home, caps);
        if let Some(entry) = user_preset_dir::find_user_preset(&user_result, id) {
            tracing::debug!(preset_id = %id, source = "user", "resolved preset from user directory");
            return Ok(entry.loaded.clone());
        }
    }

    // 2. Try system presets (qualified IDs like `_system.maintenance`).
    let system_result = system_preset_dir::scan_system_presets(nexus_home, caps);
    if let Some(entry) = system_preset_dir::find_system_preset(&system_result, id) {
        tracing::debug!(preset_id = %id, source = "system", "resolved preset from system directory");
        return Ok(entry.loaded.clone());
    }

    // 3. Fall back to embedded presets.
    match load_embedded_preset(id, caps) {
        Ok(loaded) => {
            tracing::debug!(preset_id = %id, source = "embedded", "resolved preset from embedded");
            Ok(loaded)
        }
        Err(PresetLoadError::NotFound { .. }) => {
            // Embedded preset not found — return a comprehensive error.
            Err(PresetLoadError::NotFound {
                preset_id: id.to_string(),
            })
        }
        Err(e) => Err(e),
    }
}

/// Direct O(1) preset lookup by ID (QC3 W-1 optimization).
///
/// Tries common-case direct bundle paths before falling back to the full
/// [`resolve_preset`] scan. This avoids scanning the entire user/system
/// preset directory tree for a single known preset ID.
///
/// Search order:
/// 1. **User preset** — `<nexus_home>/presets/<id>/preset.yaml`
/// 2. **Embedded preset** — compiled-in `embedded-presets/<id>/preset.yaml`
/// 3. Returns `None` — caller should fall back to [`resolve_preset`] (which
///    also handles system presets with `_system.` qualified IDs).
///
/// System presets (`_system.<name>`) are intentionally **not** handled here
/// because their directory layout differs (`_system/<name>/`). The full scan
/// in [`resolve_preset`] covers that path.
///
/// # Arguments
///
/// * `id` — The preset ID (must NOT start with `_system.`)
/// * `nexus_home` — Path to `~/.nexus42/`
/// * `caps` — Capability registry for validation
pub fn lookup_preset_by_id(
    id: &str,
    nexus_home: &Path,
    caps: &CapabilityRegistry,
) -> Option<LoadedPreset> {
    // System-qualified IDs require the full scan — bail early.
    if id.starts_with("_system.") {
        return None;
    }

    // 1. Direct user preset path.
    let user_yaml = nexus_home.join("presets").join(id).join("preset.yaml");
    if user_yaml.is_file() {
        match std::fs::read_to_string(&user_yaml) {
            Ok(yaml) => match load_preset_from_str(&yaml, caps) {
                Ok(loaded) => {
                    tracing::debug!(preset_id = %id, source = "user-direct", "resolved preset via direct path");
                    return Some(loaded);
                }
                Err(e) => {
                    tracing::warn!(
                        preset_id = %id,
                        ?user_yaml,
                        error = %e,
                        "direct user preset path exists but failed to load; falling back to full scan"
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    preset_id = %id,
                    ?user_yaml,
                    error = %e,
                    "failed to read user preset file; falling back to full scan"
                );
            }
        }
    }

    // 2. Embedded preset.
    match load_embedded_preset(id, caps) {
        Ok(loaded) => {
            tracing::debug!(preset_id = %id, source = "embedded-direct", "resolved preset via direct embedded lookup");
            Some(loaded)
        }
        Err(PresetLoadError::NotFound { .. }) => None,
        Err(e) => {
            tracing::warn!(
                preset_id = %id,
                error = %e,
                "embedded preset load error; falling back to full scan"
            );
            None
        }
    }
}

/// List all available embedded preset IDs.
///
/// Returns the names of all subdirectories under `embedded-presets/`
/// that contain a `preset.yaml` file.
#[must_use]
pub fn list_embedded_presets() -> Vec<String> {
    EMBEDDED_PRESETS
        .dirs()
        .filter_map(|dir| {
            let name = dir.path().file_name()?.to_str()?.to_string();
            if EMBEDDED_PRESETS
                .get_file(format!("{name}/preset.yaml"))
                .is_some()
            {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

/// Read a template file from an embedded preset bundle.
///
/// Given a preset ID and a relative path (e.g. `prompts/gathering-exit.md`),
/// attempts to read the file content from the embedded presets directory.
///
/// # Errors
///
/// Returns `None` if the file doesn't exist in the embedded bundle.
/// This is intentional — callers should fall back to using the raw path
/// string (for backward compat with tests that pass inline templates).
#[must_use]
pub fn read_embedded_template(preset_id: &str, template_path: &str) -> Option<String> {
    // SAFETY: path traversal is validated at load time by assert_template_file_safe.
    // The path is always relative and within the preset bundle root.
    let full_path = format!("{preset_id}/{template_path}");
    EMBEDDED_PRESETS
        .get_file(&full_path)
        .and_then(|f| f.contents_utf8().map(std::string::ToString::to_string))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityRegistry;
    use crate::preset::manifest;

    #[test]
    fn embedded_novel_writing_parses() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        assert_eq!(loaded.id, "novel-writing");
        assert_eq!(loaded.version, 7); // V1.40 P2: world_kb_block template var for World context prompt injection

        // V1.36 P3: inner graphs removed; chapter-scoped states instead.
        assert!(
            loaded.inner_graphs.is_empty(),
            "P3 novel-writing should not have inner graphs"
        );

        // Verify outer graph has 5 states (outline_chapter, draft_chapter, finalize, finalize_commit, done).
        assert!(loaded.outer_graph.get_task("outline_chapter").is_some());
        assert!(loaded.outer_graph.get_task("draft_chapter").is_some());
        assert!(loaded.outer_graph.get_task("finalize").is_some());
        assert!(loaded.outer_graph.get_task("finalize_commit").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // Verify source hash is non-trivial.
        assert!(!loaded.source_hash.is_empty());
        assert_ne!(loaded.source_hash, [0u8; 32]);
    }

    #[test]
    fn list_embedded_presets_includes_novel_writing() {
        let presets = list_embedded_presets();
        assert!(
            presets.iter().any(|p| p == "novel-writing"),
            "expected 'novel-writing' in embedded presets: {presets:?}"
        );
    }

    #[test]
    fn embedded_novel_writing_excludes_deprecated_prompt_archive() {
        assert!(
            EMBEDDED_PRESETS
                .get_dir("novel-writing/prompts/_deprecated")
                .is_none(),
            "deprecated prompt archive must not be compiled into embedded presets"
        );
        assert!(
            read_embedded_template("novel-writing", "prompts/_deprecated/draft-intro.md").is_none(),
            "deprecated draft-intro.md should not be readable from embedded presets"
        );
        assert!(
            read_embedded_template("novel-writing", "prompts/_deprecated/draft-body.md").is_none(),
            "deprecated draft-body.md should not be readable from embedded presets"
        );
        assert!(
            read_embedded_template("novel-writing", "prompts/draft-chapter.md").is_some(),
            "current novel-writing prompt templates should remain embedded"
        );
    }

    #[test]
    fn embedded_preset_unknown_id_fails() {
        let caps = CapabilityRegistry::with_builtins();
        let err = load_embedded_preset("nonexistent-preset", &caps).unwrap_err();
        assert!(
            matches!(&err, PresetLoadError::NotFound { .. }),
            "expected NotFound error: {err:?}"
        );
    }

    #[test]
    fn novel_writing_manifest_has_correct_states() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        // P4 fix wave: 5 states (outline_chapter, draft_chapter, finalize, finalize_commit, done).
        assert_eq!(loaded.manifest.states.len(), 5);

        // Verify state IDs.
        let state_ids: Vec<&str> = loaded
            .manifest
            .states
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert!(state_ids.contains(&"outline_chapter"));
        assert!(state_ids.contains(&"draft_chapter"));
        assert!(state_ids.contains(&"finalize"));
        assert!(state_ids.contains(&"finalize_commit"));
        assert!(state_ids.contains(&"done"));

        // Verify finalize uses llm_judge exit (P3 T7: 五问 quality gate).
        let finalize = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "finalize")
            .expect("finalize state should exist");
        // P4 fix wave: finalize has NO enter actions.
        assert!(
            finalize.enter.is_empty(),
            "finalize should have no enter actions (deferred to finalize_commit)"
        );
        match &finalize.exit_when {
            Some(manifest::ExitWhen::LlmJudge {
                template_file,
                judge_capability,
                min_interval,
            }) => {
                assert_eq!(template_file.as_deref(), Some("prompts/finalize-exit.md"));
                assert_eq!(judge_capability.as_deref(), Some("judge.llm"));
                assert_eq!(min_interval.as_deref(), Some("PT6H"));
            }
            other => panic!("expected llm_judge exit_when for finalize, got: {other:?}"),
        }
        // P4 fix wave: finalize transitions to finalize_commit, not done.
        match &finalize.next {
            Some(manifest::NextTarget::Linear(target)) => {
                assert_eq!(target, "finalize_commit");
            }
            other => panic!("finalize next should be Linear(finalize_commit), got: {other:?}"),
        }

        // Verify done is terminal.
        let done = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "done")
            .unwrap();
        assert!(done.terminal, "done state should be terminal");
    }

    #[test]
    fn novel_writing_has_correct_prompt_references() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        // Collect all prompt_file references from manifest states.
        let mut prompt_files: Vec<&str> = loaded
            .manifest
            .states
            .iter()
            .flat_map(|s| {
                s.enter.iter().filter_map(|a| match a {
                    manifest::EnterAction::Capability { args, .. } => {
                        args.as_ref()?.get("prompt_file")?.as_str()
                    }
                    manifest::EnterAction::InnerGraph { .. } => None,
                    manifest::EnterAction::HostTool { .. } => None,
                })
            })
            .collect();

        // Collect template_file references from context_update hooks.
        for s in &loaded.manifest.states {
            if let Some(ref hook) = s.context_update {
                prompt_files.push(&hook.template_file);
            }
        }

        // Collect template_file references from inner graph nodes.
        if let Some(ref igs) = loaded.manifest.inner_graphs {
            for ig in igs.values() {
                for node in &ig.nodes {
                    if let Some(ref tf) = node.template_file {
                        prompt_files.push(tf.as_str());
                    }
                }
            }
        }

        // P3: 2 prompt files from enter actions (outline-chapter.md, draft-chapter.md).
        // No context_update hooks or inner graphs in P3 preset.
        assert_eq!(
            prompt_files.len(),
            2,
            "expected 2 prompt template references in enter actions: {prompt_files:?}"
        );
        assert!(
            prompt_files.iter().any(|f| f.contains("outline-chapter")),
            "expected outline-chapter.md reference"
        );
        assert!(
            prompt_files.iter().any(|f| f.contains("draft-chapter")),
            "expected draft-chapter.md reference"
        );

        // Verify the embedded directory has only current prompt files.
        let prompts_dir = EMBEDDED_PRESETS
            .get_dir("novel-writing/prompts")
            .expect("novel-writing/prompts dir should exist");
        let prompt_count = prompts_dir.files().count();
        assert!(
            prompt_count >= 12,
            "expected at least 12 current embedded prompt files, got {prompt_count}"
        );
    }

    // ── WS-A T4: Composable search order ─────────────────────────────────

    #[test]
    fn resolve_preset_finds_embedded_preset_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();
        let caps = CapabilityRegistry::with_builtins();

        let loaded = resolve_preset("novel-writing", nexus_home, &caps).unwrap();
        assert_eq!(loaded.id, "novel-writing");
    }

    #[test]
    fn resolve_preset_user_overrides_embedded() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();

        // Create a user preset with the same ID as an embedded preset.
        let bundle_dir = nexus_home.join("presets").join("novel-writing");
        fs::create_dir_all(&bundle_dir).unwrap();
        let override_yaml = r#"
preset:
  id: novel-writing
  version: 99
  kind: creator
  description: "user override of novel-writing"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
        fs::write(bundle_dir.join("preset.yaml"), override_yaml).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let loaded = resolve_preset("novel-writing", nexus_home, &caps).unwrap();
        assert_eq!(
            loaded.version, 99,
            "user preset should override embedded preset"
        );
        assert_eq!(
            loaded.manifest.states.len(),
            2,
            "user preset has 2 states, not 5"
        );
    }

    #[test]
    fn resolve_preset_unknown_id_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();
        let caps = CapabilityRegistry::with_builtins();

        let err = resolve_preset("nonexistent-preset", nexus_home, &caps).unwrap_err();
        assert!(
            matches!(&err, PresetLoadError::NotFound { .. }),
            "expected NotFound error: {err:?}"
        );
    }

    #[test]
    fn resolve_preset_finds_system_preset() {
        use crate::system_preset_dir::ensure_maintenance_preset;

        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();

        // Create the _system/maintenance/ preset
        ensure_maintenance_preset(nexus_home).unwrap();

        let caps = CapabilityRegistry::with_builtins();
        let loaded = resolve_preset("_system.maintenance", nexus_home, &caps).unwrap();
        assert_eq!(loaded.id, "maintenance");
    }

    // ── WS-A T7: Integration — user preset loads end-to-end ──────────

    #[test]
    fn user_preset_loads_end_to_end() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();
        let nexus_home = tmp.path();

        // Create a full user preset at ~/.nexus42/presets/test-strategy/
        let bundle_dir = nexus_home.join("presets").join("test-strategy");
        fs::create_dir_all(&bundle_dir).unwrap();

        let valid_yaml = r#"
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "End-to-end test preset"
  requires_capabilities:
    - workspace.open
  initial: start
  terminal: done
states:
  - id: start
    description: "Begin"
    enter:
      - kind: capability
        name: workspace.open
        args:
          prompt_file: prompts/start.md
          vars:
            input: "{{preset.input}}"
    exit_when:
      kind: manual
    next: done
  - id: done
    terminal: true
"#;
        fs::write(bundle_dir.join("preset.yaml"), valid_yaml).unwrap();
        fs::create_dir_all(bundle_dir.join("prompts")).unwrap();
        fs::write(
            bundle_dir.join("prompts/start.md"),
            "# Start Prompt\n\n{{input}}",
        )
        .unwrap();

        // Resolve via the composable search order.
        let caps = CapabilityRegistry::with_builtins();
        let loaded = resolve_preset("test-strategy", nexus_home, &caps).unwrap();

        // Verify it loaded from user source (not embedded).
        assert_eq!(loaded.id, "test-strategy");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.manifest.states.len(), 2);
        assert_eq!(loaded.manifest.states[0].id, "start");
        assert!(loaded.manifest.states[1].terminal);

        // Verify it has an outer graph.
        assert!(loaded.outer_graph.get_task("start").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // Verify source hash is valid.
        assert!(!loaded.source_hash.is_empty());
        assert_ne!(loaded.source_hash, [0u8; 32]);
    }

    #[test]
    fn novel_writing_has_multi_agent_roles() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        // WS-E T6: Verify roles section
        assert_eq!(loaded.roles.len(), 2);
        assert!(loaded.roles.iter().any(|r| r.id == "writer"));
        assert!(loaded.roles.iter().any(|r| r.id == "reviewer"));

        // Verify writer role has recommended_skills
        let writer = loaded.roles.iter().find(|r| r.id == "writer").unwrap();
        assert_eq!(writer.recommended_skills.len(), 1);
        assert_eq!(writer.recommended_skills[0], "novel-writing-assistant");
    }

    // ── P3: Agentic Pattern Preset tests ────────────────────────────────

    #[test]
    fn list_embedded_presets_includes_reflection_loop() {
        let presets = list_embedded_presets();
        assert!(
            presets.iter().any(|p| p == "reflection-loop"),
            "expected 'reflection-loop' in embedded presets: {presets:?}"
        );
    }

    #[test]
    fn list_embedded_presets_includes_memory_augmented() {
        let presets = list_embedded_presets();
        assert!(
            presets.iter().any(|p| p == "memory-augmented"),
            "expected 'memory-augmented' in embedded presets: {presets:?}"
        );
    }

    #[test]
    fn embedded_reflection_loop_loads_and_validates() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("reflection-loop", &caps).unwrap();

        assert_eq!(loaded.id, "reflection-loop");
        assert_eq!(loaded.version, 1);

        // Linear state machine: draft → revise → summarize → done
        assert!(loaded.outer_graph.get_task("draft").is_some());
        assert!(loaded.outer_graph.get_task("revise").is_some());
        assert!(loaded.outer_graph.get_task("summarize").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // Two inner graphs: draft_graph, revise_graph
        assert!(
            loaded.inner_graphs.contains_key("draft_graph"),
            "expected draft_graph inner graph"
        );
        assert!(
            loaded.inner_graphs.contains_key("revise_graph"),
            "expected revise_graph inner graph"
        );

        // Verify inner graph structure
        let draft_graph = &loaded.inner_graphs["draft_graph"];
        assert!(draft_graph.get_task("generate").is_some());

        let revise_graph = &loaded.inner_graphs["revise_graph"];
        assert!(revise_graph.get_task("apply_critique").is_some());

        // Output bindings
        assert_eq!(
            loaded.output_bindings.get("draft_graph").unwrap(),
            "generate.text"
        );
        assert_eq!(
            loaded.output_bindings.get("revise_graph").unwrap(),
            "apply_critique.text"
        );

        // Source hash is non-trivial
        assert!(!loaded.source_hash.is_empty());
        assert_ne!(loaded.source_hash, [0u8; 32]);

        // Single-agent preset — no roles
        assert!(
            loaded.roles.is_empty(),
            "reflection-loop should not have roles"
        );
    }

    #[test]
    fn reflection_loop_has_llm_judge_exit_conditions() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("reflection-loop", &caps).unwrap();

        // draft state uses llm_judge
        let draft = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "draft")
            .unwrap();
        match &draft.exit_when {
            Some(manifest::ExitWhen::LlmJudge {
                template_file,
                judge_capability,
                ..
            }) => {
                assert_eq!(
                    template_file.as_deref(),
                    Some("prompts/draft-quality-check.md")
                );
                assert_eq!(judge_capability.as_deref(), Some("judge.llm"));
            }
            other => panic!("expected llm_judge exit_when for draft, got: {other:?}"),
        }

        // revise state also uses llm_judge
        let revise = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "revise")
            .unwrap();
        match &revise.exit_when {
            Some(manifest::ExitWhen::LlmJudge {
                template_file,
                judge_capability,
                ..
            }) => {
                assert_eq!(
                    template_file.as_deref(),
                    Some("prompts/revise-quality-check.md")
                );
                assert_eq!(judge_capability.as_deref(), Some("judge.llm"));
            }
            other => panic!("expected llm_judge exit_when for revise, got: {other:?}"),
        }

        // summarize state uses manual exit
        let summarize = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "summarize")
            .unwrap();
        assert!(
            matches!(summarize.exit_when, Some(manifest::ExitWhen::Manual)),
            "expected manual exit_when for summarize"
        );

        // done is terminal
        let done = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "done")
            .unwrap();
        assert!(done.terminal, "done state should be terminal");
    }

    #[test]
    fn reflection_loop_has_correct_prompt_files() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("reflection-loop", &caps).unwrap();

        // Verify prompt files referenced in inner_graph nodes
        let _draft_graph = &loaded.inner_graphs["draft_graph"];
        let _revise_graph = &loaded.inner_graphs["revise_graph"];

        // draft_graph.generate should reference prompts/generate-draft.md
        // (verified via inner graph node template_file)
        if let Some(ref igs) = loaded.manifest.inner_graphs {
            let draft_nodes = &igs["draft_graph"].nodes;
            assert_eq!(draft_nodes[0].id, "generate");
            assert_eq!(
                draft_nodes[0].template_file.as_deref(),
                Some("prompts/generate-draft.md")
            );

            let revise_nodes = &igs["revise_graph"].nodes;
            assert_eq!(revise_nodes[0].id, "apply_critique");
            assert_eq!(
                revise_nodes[0].template_file.as_deref(),
                Some("prompts/apply-critique.md")
            );
        }

        // Verify all 5 prompt files exist in the embedded directory
        let prompts_dir = EMBEDDED_PRESETS
            .get_dir("reflection-loop/prompts")
            .expect("reflection-loop/prompts dir should exist");
        assert_eq!(
            prompts_dir.files().count(),
            5,
            "expected 5 embedded prompt files for reflection-loop"
        );
    }

    #[test]
    fn embedded_memory_augmented_loads_and_validates() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("memory-augmented", &caps).unwrap();

        assert_eq!(loaded.id, "memory-augmented");
        assert_eq!(loaded.version, 1);

        // Linear state machine: recall → generate → persist → done
        assert!(loaded.outer_graph.get_task("recall").is_some());
        assert!(loaded.outer_graph.get_task("generate").is_some());
        assert!(loaded.outer_graph.get_task("persist").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // One inner graph: generate_graph
        assert!(
            loaded.inner_graphs.contains_key("generate_graph"),
            "expected generate_graph inner graph"
        );

        // Verify inner graph structure
        let generate_graph = &loaded.inner_graphs["generate_graph"];
        assert!(generate_graph.get_task("generate_with_context").is_some());

        // Output binding
        assert_eq!(
            loaded.output_bindings.get("generate_graph").unwrap(),
            "generate_with_context.text"
        );

        // Source hash is non-trivial
        assert!(!loaded.source_hash.is_empty());
        assert_ne!(loaded.source_hash, [0u8; 32]);

        // Single-agent preset — no roles
        assert!(
            loaded.roles.is_empty(),
            "memory-augmented should not have roles"
        );
    }

    #[test]
    fn memory_augmented_uses_creator_capabilities() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("memory-augmented", &caps).unwrap();

        // recall state uses creator.read_memory
        let recall = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "recall")
            .unwrap();
        assert_eq!(recall.enter.len(), 1);
        match &recall.enter[0] {
            manifest::EnterAction::Capability { name, args } => {
                assert_eq!(name, "creator.read_memory");
                // Verify args contain keyword reference
                assert!(args.is_some());
                let args = args.as_ref().unwrap();
                assert!(args.get("keyword").is_some());
            }
            other => panic!("expected capability enter for recall, got: {other:?}"),
        }

        // recall exit_when is rule
        assert!(
            matches!(recall.exit_when, Some(manifest::ExitWhen::Rule)),
            "expected rule exit_when for recall"
        );

        // generate state uses inner_graph
        let generate = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "generate")
            .unwrap();
        assert_eq!(generate.enter.len(), 1);
        match &generate.enter[0] {
            manifest::EnterAction::InnerGraph { name } => {
                assert_eq!(name, "generate_graph");
            }
            other => panic!("expected inner_graph enter for generate, got: {other:?}"),
        }

        // persist state uses creator.write_memory
        let persist = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "persist")
            .unwrap();
        assert_eq!(persist.enter.len(), 1);
        match &persist.enter[0] {
            manifest::EnterAction::Capability { name, args } => {
                assert_eq!(name, "creator.write_memory");
                assert!(args.is_some());
                let args = args.as_ref().unwrap();
                assert!(args.get("content").is_some());
                assert!(args.get("keywords").is_some());
            }
            other => panic!("expected capability enter for persist, got: {other:?}"),
        }

        // done is terminal
        let done = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "done")
            .unwrap();
        assert!(done.terminal, "done state should be terminal");
    }

    #[test]
    fn memory_augmented_has_correct_prompt_files() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("memory-augmented", &caps).unwrap();

        // Verify prompt file in generate_graph
        if let Some(ref igs) = loaded.manifest.inner_graphs {
            let gen_nodes = &igs["generate_graph"].nodes;
            assert_eq!(gen_nodes[0].id, "generate_with_context");
            assert_eq!(
                gen_nodes[0].template_file.as_deref(),
                Some("prompts/generate-with-memory.md")
            );
        }

        // Verify all 3 prompt files exist in the embedded directory
        let prompts_dir = EMBEDDED_PRESETS
            .get_dir("memory-augmented/prompts")
            .expect("memory-augmented/prompts dir should exist");
        assert_eq!(
            prompts_dir.files().count(),
            3,
            "expected 3 embedded prompt files for memory-augmented"
        );
    }

    #[test]
    fn two_new_presets_in_registry_iteration() {
        let presets = list_embedded_presets();

        // Must contain both new presets
        assert!(
            presets.iter().any(|p| p == "reflection-loop"),
            "reflection-loop must be in embedded presets"
        );
        assert!(
            presets.iter().any(|p| p == "memory-augmented"),
            "memory-augmented must be in embedded presets"
        );

        // Existing presets still present
        assert!(
            presets.iter().any(|p| p == "novel-writing"),
            "novel-writing must still be present"
        );
        assert!(
            presets.iter().any(|p| p == "kb-extract"),
            "kb-extract must still be present"
        );

        // Total count: at least novel-writing + kb-extract + research +
        // soul-experience-refresh + reflection-loop + memory-augmented = 6
        assert!(
            presets.len() >= 6,
            "expected at least 6 embedded presets, got {}: {presets:?}",
            presets.len()
        );
    }

    // ── P2 B1/B2: Embedded preset smoke discovery + P1 gate ─────────────

    /// Collect all asset file references from a manifest and check they exist
    /// in the embedded presets directory.
    fn check_embedded_asset_existence(
        preset_id: &str,
        manifest: &manifest::PresetManifest,
        errors: &mut Vec<String>,
    ) {
        // Collect template_file / prompt_file / system_prompt_file references.
        let asset_refs = validation::collect_asset_file_references(manifest);

        for (dot_path, rel_path) in &asset_refs {
            let full_path = format!("{preset_id}/{rel_path}");
            if EMBEDDED_PRESETS.get_file(&full_path).is_none() {
                errors.push(format!(
                    "preset '{preset_id}': asset '{rel_path}' referenced at {dot_path} \
                     does not exist in embedded directory"
                ));
            }
        }
    }

    #[test]
    fn all_embedded_presets_pass_strict_validation_gate() {
        let caps = CapabilityRegistry::with_builtins();
        let preset_ids = list_embedded_presets();

        assert!(
            !preset_ids.is_empty(),
            "expected at least one embedded preset"
        );

        let mut all_errors: Vec<String> = Vec::new();
        let mut all_warnings: Vec<String> = Vec::new();

        for preset_id in &preset_ids {
            // Step 1: Load the preset (runs structural validation in the loader).
            let loaded = match load_embedded_preset(preset_id, &caps) {
                Ok(l) => l,
                Err(e) => {
                    all_errors.push(format!("preset '{preset_id}' failed to load: {e}"));
                    continue;
                }
            };

            // Step 2: Run P1 semantic validation (A2: reachability, terminal
            // consistency, id match, inner graph refs; A4: capability compat).
            let semantic_result = validation::validate_preset_semantic(&loaded.manifest, &caps);
            for d in &semantic_result.diagnostics {
                match d.severity {
                    validation::DiagnosticSeverity::Error => {
                        // Known false positive: orchestration-layer args
                        // (prompt_file, vars) that don't appear in
                        // creator.inject_prompt's input_schema because the
                        // engine resolves them before calling the capability.
                        // Only downgrade creator.inject_prompt drift; all
                        // other CapabilityArgDrift errors are real.
                        if d.category == validation::DiagnosticCategory::CapabilityArgDrift
                            && d.message.contains("capability 'creator.inject_prompt'")
                        {
                            all_warnings.push(format!(
                                "preset '{preset_id}' capability arg drift at {}: {}",
                                d.path, d.message
                            ));
                        } else {
                            all_errors.push(format!(
                                "preset '{preset_id}' semantic error at {}: {}",
                                d.path, d.message
                            ));
                        }
                    }
                    validation::DiagnosticSeverity::Warning => {
                        all_warnings.push(format!(
                            "preset '{preset_id}' warning at {}: {}",
                            d.path, d.message
                        ));
                    }
                }
            }

            // Step 3: Run path safety checks (A3 structural).
            let path_result = validation::validate_path_safety(&loaded.manifest);
            for d in &path_result.diagnostics {
                if d.severity == validation::DiagnosticSeverity::Error {
                    all_errors.push(format!(
                        "preset '{preset_id}' path-safety error at {}: {}",
                        d.path, d.message
                    ));
                }
            }

            // Step 4: Check asset file existence in the embedded dir.
            check_embedded_asset_existence(preset_id, &loaded.manifest, &mut all_errors);
        }

        // Report warnings but only fail on errors.
        if !all_warnings.is_empty() {
            eprintln!(
                "embedded preset validation warnings (non-blocking):\n{}",
                all_warnings.join("\n")
            );
        }

        assert!(
            all_errors.is_empty(),
            "embedded preset validation failures:\n{}",
            all_errors.join("\n")
        );
    }

    #[test]
    fn kb_extract_inner_graph_is_wired() {
        // B3: Verify kb-extract's extraction_graph is referenced by an enter action.
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("kb-extract", &caps).unwrap();

        // The extracting state must have an inner_graph enter action.
        let extracting = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "extracting")
            .expect("kb-extract must have an 'extracting' state");

        let has_inner_graph_enter = extracting.enter.iter().any(|a| {
            matches!(a, manifest::EnterAction::InnerGraph { name } if name == "extraction_graph")
        });
        assert!(
            has_inner_graph_enter,
            "kb-extract 'extracting' state must reference extraction_graph via inner_graph enter action"
        );

        // Also verify the P1 validator produces no orphan warnings for extraction_graph.
        let result = validation::validate_preset_semantic(&loaded.manifest, &caps);
        let orphan_warnings: Vec<_> = result
            .warnings()
            .filter(|d| {
                d.category == validation::DiagnosticCategory::OrphanInnerGraph
                    && d.message.contains("extraction_graph")
            })
            .collect();
        assert!(
            orphan_warnings.is_empty(),
            "extraction_graph should NOT be orphan: {:?}",
            orphan_warnings
        );
    }

    #[test]
    fn memory_augmented_rule_exit_is_explicit_always_true() {
        // B5 (TD-V131-08): Verify that the recall state's `exit_when: kind: rule`
        // is the explicit always-true form. ExitWhen::Rule is a unit variant
        // whose contract is "transition as soon as enter action completes".
        // This test locks that contract.
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("memory-augmented", &caps).unwrap();

        let recall = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "recall")
            .expect("memory-augmented must have a 'recall' state");

        // Must use ExitWhen::Rule (always-true / immediate transition).
        assert!(
            matches!(recall.exit_when, Some(manifest::ExitWhen::Rule)),
            "recall state must use exit_when: kind: rule (explicit always-true)"
        );
    }

    #[test]
    fn embedded_preset_validation_catches_structurally_invalid_preset() {
        // B2: Verify the smoke test machinery would catch a bad preset.
        // We construct a manifest with a known structural issue and verify
        // the validator detects it.
        let yaml = r"
preset:
  id: broken-test
  version: 1
  kind: creator
  description: intentionally broken
  requires_capabilities:
    - totally.nonexistent.capability
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    enter: []
    exit_when: { kind: manual }
  - id: c
    terminal: true
";
        let manifest: manifest::PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validation::validate_preset_semantic(&manifest, &caps);

        assert!(
            result.has_errors(),
            "expected errors for structurally invalid manifest, got: {:?}",
            result.diagnostics
        );

        // Must have at least a MissingCapability error.
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| { d.category == validation::DiagnosticCategory::MissingCapability }),
            "expected MissingCapability error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn unknown_capability_arg_drift_is_not_downgraded() {
        // W-002: Prove the smoke test's CapabilityArgDrift downgrade is
        // scoped to creator.inject_prompt only. A synthetic preset that omits
        // a required arg for a different capability must still surface as an
        // error (not silently downgraded to a warning).
        let yaml = r#"
preset:
  id: drift-probe
  version: 1
  kind: creator
  description: synthetic preset to verify drift narrowing
  requires_capabilities:
    - kb.extract_work
  initial: start
  terminal: done
states:
  - id: start
    enter:
      - kind: capability
        name: kb.extract_work
        args:
          bogus_extra: "not_in_schema"
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: manifest::PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validation::validate_preset_semantic(&manifest, &caps);

        // kb.extract_work requires creator_id (omitted) → Error-severity
        // CapabilityArgDrift. Must NOT be downgraded (only creator.inject_prompt
        // gets the downgrade).
        let drift_errors: Vec<_> = result
            .errors()
            .filter(|d| {
                d.category == validation::DiagnosticCategory::CapabilityArgDrift
                    && d.message.contains("capability 'kb.extract_work'")
            })
            .collect();

        assert!(
            !drift_errors.is_empty(),
            "kb.extract_work CapabilityArgDrift should NOT be downgraded; expected at least one error, got: {:?}",
            result.diagnostics
        );
    }

    // ── V1.39 P2: novel-brainstorm + novel-review-master ──────────────

    #[test]
    fn list_embedded_presets_includes_novel_brainstorm() {
        let presets = list_embedded_presets();
        assert!(
            presets.iter().any(|p| p == "novel-brainstorm"),
            "expected 'novel-brainstorm' in embedded presets: {presets:?}"
        );
    }

    #[test]
    fn list_embedded_presets_includes_novel_review_master() {
        let presets = list_embedded_presets();
        assert!(
            presets.iter().any(|p| p == "novel-review-master"),
            "expected 'novel-review-master' in embedded presets: {presets:?}"
        );
    }

    #[test]
    fn embedded_novel_brainstorm_loads_and_validates() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-brainstorm", &caps).unwrap();

        assert_eq!(loaded.id, "novel-brainstorm");
        assert_eq!(loaded.version, 1);

        // State machine: gather → synthesize → done
        assert_eq!(loaded.manifest.preset.initial, "gather");
        assert!(loaded.outer_graph.get_task("gather").is_some());
        assert!(loaded.outer_graph.get_task("synthesize").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // Verify linear transitions
        assert!(loaded.manifest.states.iter().any(|s| {
            s.id == "gather" && s.next == Some(manifest::NextTarget::Linear("synthesize".into()))
        }));
        assert!(loaded.manifest.states.iter().any(|s| {
            s.id == "synthesize" && s.next == Some(manifest::NextTarget::Linear("done".into()))
        }));

        // Auto-chain compatible: exit_when uses llm_judge (not manual)
        let gather = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "gather")
            .expect("gather state must exist");
        assert!(
            matches!(gather.exit_when, Some(manifest::ExitWhen::LlmJudge { .. })),
            "gather must use llm_judge exit for auto-chain compatibility"
        );

        // Gates present under preset section
        assert!(
            !loaded.manifest.preset.gates.is_empty(),
            "novel-brainstorm must have preset gates"
        );

        // Prompt templates exist
        assert!(
            read_embedded_template("novel-brainstorm", "prompts/gather-findings.md").is_some(),
            "gather-findings.md prompt must exist"
        );
        assert!(
            read_embedded_template("novel-brainstorm", "prompts/synthesize-ideas.md").is_some(),
            "synthesize-ideas.md prompt must exist"
        );
    }

    #[test]
    fn embedded_novel_review_master_loads_and_validates() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-review-master", &caps).unwrap();

        assert_eq!(loaded.id, "novel-review-master");
        assert_eq!(loaded.version, 2);

        // State machine: present → await_decision → sync_world_kb → done
        assert_eq!(loaded.manifest.preset.initial, "present");
        assert!(loaded.outer_graph.get_task("present").is_some());
        assert!(loaded.outer_graph.get_task("await_decision").is_some());
        assert!(loaded.outer_graph.get_task("sync_world_kb").is_some());
        assert!(loaded.outer_graph.get_task("done").is_some());

        // Verify linear transitions
        assert!(loaded.manifest.states.iter().any(|s| {
            s.id == "present"
                && s.next == Some(manifest::NextTarget::Linear("await_decision".into()))
        }));
        assert!(loaded.manifest.states.iter().any(|s| {
            s.id == "await_decision"
                && s.next == Some(manifest::NextTarget::Linear("sync_world_kb".into()))
        }));
        assert!(loaded.manifest.states.iter().any(|s| {
            s.id == "sync_world_kb" && s.next == Some(manifest::NextTarget::Linear("done".into()))
        }));

        // Human-in-loop: exit_when uses manual (not auto-chain)
        let present = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == "present")
            .expect("present state must exist");
        assert!(
            matches!(present.exit_when, Some(manifest::ExitWhen::Manual)),
            "present must use manual exit for human-in-loop"
        );

        // Gates present under preset section
        assert!(
            !loaded.manifest.preset.gates.is_empty(),
            "novel-review-master must have preset gates"
        );

        // Prompt templates exist
        assert!(
            read_embedded_template("novel-review-master", "prompts/present-findings.md").is_some(),
            "present-findings.md prompt must exist"
        );
        assert!(
            read_embedded_template("novel-review-master", "prompts/await-decision.md").is_some(),
            "await-decision.md prompt must exist"
        );
    }
}
