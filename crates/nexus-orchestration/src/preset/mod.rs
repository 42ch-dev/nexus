//! Preset loader module.
//!
//! Loads preset bundles (YAML manifest + optional prompt templates) and
//! validates them per `orchestration-engine-v1.md` §7.6.
//!
//! Types: `nexus-contracts::local::orchestration::preset`.
//! Loader + validation: this module (`loader.rs`).
//! Embedded presets: `include_dir!` at compile time (§7.1 location #3).

use crate::capability::CapabilityRegistry;
use crate::system_preset_dir;
use crate::user_preset_dir;
use include_dir::include_dir;
use include_dir::Dir;
use std::path::Path;

pub mod loader;
pub mod manifest;

pub use loader::{
    load_preset, load_preset_from_str, LoadedPreset, PresetLoadError, ValidationProblem,
};

// ---------------------------------------------------------------------------
// Embedded presets
// ---------------------------------------------------------------------------

/// Embedded presets directory, compiled into the binary at build time.
///
/// Location: `crates/nexus-orchestration/embedded-presets/`
/// Structure per §7.1: `<preset-id>/preset.yaml` + `prompts/*.md`
static EMBEDDED_PRESETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/embedded-presets");

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

/// List all available embedded preset IDs.
///
/// Returns the names of all subdirectories under `embedded-presets/`
/// that contain a `preset.yaml` file.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityRegistry;

    #[test]
    fn embedded_novel_writing_parses() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        assert_eq!(loaded.id, "novel-writing");
        assert_eq!(loaded.version, 2); // WS-E T6: bumped for multi-agent roles

        // Must have both inner graphs.
        assert!(
            loaded.inner_graphs.contains_key("brainstorm_graph"),
            "expected brainstorm_graph inner graph"
        );
        assert!(
            loaded.inner_graphs.contains_key("drafting_graph"),
            "expected drafting_graph inner graph"
        );

        // Verify inner graph structure.
        let brainstorm = &loaded.inner_graphs["brainstorm_graph"];
        assert!(brainstorm.get_task("diverge").is_some());
        assert!(brainstorm.get_task("cluster").is_some());
        assert!(brainstorm.get_task("select").is_some());

        let drafting = &loaded.inner_graphs["drafting_graph"];
        assert!(drafting.get_task("draft_intro").is_some());
        assert!(drafting.get_task("draft_body").is_some());

        // Verify output bindings.
        assert_eq!(
            loaded.output_bindings.get("brainstorm_graph").unwrap(),
            "select.text"
        );
        assert_eq!(
            loaded.output_bindings.get("drafting_graph").unwrap(),
            "draft_body.text"
        );

        // Verify outer graph has 5 states.
        assert!(loaded.outer_graph.get_task("gathering").is_some());
        assert!(loaded.outer_graph.get_task("brainstorming").is_some());
        assert!(loaded.outer_graph.get_task("outlining").is_some());
        assert!(loaded.outer_graph.get_task("drafting").is_some());
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

        // Check manifest states.
        assert_eq!(loaded.manifest.states.len(), 5);

        // Verify state IDs.
        let state_ids: Vec<&str> = loaded
            .manifest
            .states
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert!(state_ids.contains(&"gathering"));
        assert!(state_ids.contains(&"brainstorming"));
        assert!(state_ids.contains(&"outlining"));
        assert!(state_ids.contains(&"drafting"));
        assert!(state_ids.contains(&"done"));

        // Verify gathering uses llm_judge exit.
        assert_eq!(loaded.manifest.states[0].id, "gathering");
        match &loaded.manifest.states[0].exit_when {
            Some(manifest::ExitWhen::LlmJudge {
                template_file,
                judge_capability,
                min_interval,
            }) => {
                assert_eq!(template_file.as_deref(), Some("prompts/gathering-exit.md"));
                assert_eq!(judge_capability.as_deref(), Some("judge.llm"));
                assert_eq!(min_interval.as_deref(), Some("PT6H"));
            }
            other => panic!("expected llm_judge exit_when, got: {other:?}"),
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
    fn novel_writing_has_nine_prompt_references() {
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
                    _ => None,
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

        assert_eq!(
            prompt_files.len(),
            8,
            "expected 8 prompt template references in enter + context_update + inner_graphs"
        );

        // Verify the embedded directory has all 11 prompt files (includes
        // gathering-exit.md which is referenced from exit_when, not enter,
        // outlining-ctx-update.md from the context_update hook, and
        // writer-system.md / reviewer-system.md from the roles section).
        let prompts_dir = EMBEDDED_PRESETS
            .get_dir("novel-writing/prompts")
            .expect("novel-writing/prompts dir should exist");
        assert_eq!(
            prompts_dir.files().count(),
            11,
            "expected 11 embedded prompt files"
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
        use std::fs;

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

        // Verify writer role has recommended_models
        let writer = loaded.roles.iter().find(|r| r.id == "writer").unwrap();
        assert_eq!(writer.recommended_models.len(), 2);
        assert!(writer.recommended_models[0].contains(':'));
        assert!(writer.recommended_models[1].contains(':'));
    }
}
