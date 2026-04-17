//! Preset loader module.
//!
//! Loads preset bundles (YAML manifest + optional prompt templates) and
//! validates them per `orchestration-engine-v1.md` §7.6.
//!
//! Types: `nexus-contracts::local::orchestration::preset`.
//! Loader + validation: this module (`loader.rs`).
//! Embedded presets: `include_dir!` at compile time (§7.1 location #3).

use crate::capability::CapabilityRegistry;
use include_dir::include_dir;
use include_dir::Dir;

pub mod loader;
pub mod manifest;

pub use loader::{load_preset, load_preset_from_str, LoadedPreset, PresetLoadError, ValidationProblem};

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
        .ok_or_else(|| PresetLoadError::Validation {
            len: 1,
            problems: vec![loader::ValidationProblem {
                path: String::new(),
                error: format!("embedded preset '{id}' not found or missing preset.yaml"),
            }],
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
        assert_eq!(loaded.version, 1);

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
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("not found")),
            "expected 'not found' error: {problems:?}"
        );
    }

    #[test]
    fn novel_writing_manifest_has_correct_states() {
        let caps = CapabilityRegistry::with_builtins();
        let loaded = load_embedded_preset("novel-writing", &caps).unwrap();

        // Check manifest states.
        assert_eq!(loaded.manifest.states.len(), 5);

        // Verify state IDs.
        let state_ids: Vec<&str> =
            loaded.manifest.states.iter().map(|s| s.id.as_str()).collect();
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
        let done = loaded.manifest.states.iter().find(|s| s.id == "done").unwrap();
        assert!(done.terminal, "done state should be terminal");
    }

    #[test]
    fn novel_writing_has_eight_prompt_references() {
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

        assert_eq!(prompt_files.len(), 7, "expected 7 prompt template references in enter + inner_graphs");

        // Verify the embedded directory has all 8 prompt files (includes
        // gathering-exit.md which is referenced from exit_when, not enter).
        let prompts_dir = EMBEDDED_PRESETS
            .get_dir("novel-writing/prompts")
            .expect("novel-writing/prompts dir should exist");
        assert_eq!(prompts_dir.files().count(), 8, "expected 8 embedded prompt files");
    }
}


