//! `nexus42 preset init|list` — manage user-installed presets.
//!
//! Init: scaffolds a new preset directory under `~/.nexus42/presets/<name>/`.
//! List: enumerates all available presets grouped by source.

#![allow(clippy::print_literal)]

use crate::config::{nexus_home, CliConfig};
use crate::errors::{CliError, Result};
use clap::{Parser, Subcommand};
use nexus_home_layout::user_preset_bundle_dir;

/// The template YAML for a new user preset.
const PRESET_INIT_TEMPLATE: &str = r#"preset:
  id: {{name}}
  version: 1
  kind: creator
  description: "Custom orchestration strategy"
  requires_capabilities: []
  initial: start
  terminal: done
states:
  - id: start
    description: "Begin the workflow"
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

/// Template for the default prompt file scaffolding.
const PROMPT_INIT_CONTENT: &str = r"# Start Prompt

{{input}}
";

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// Create a new user preset bundle
    Init {
        /// Preset name (used as directory name and preset ID)
        name: String,
    },
    /// List all available presets grouped by source
    List,
}

#[derive(Debug, Parser)]
#[command(subcommand_required = true, name = "preset")]
struct PresetCli {
    #[command(subcommand)]
    command: PresetCommand,
}

/// Run the preset command.
pub async fn run(cmd: PresetCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        PresetCommand::Init { name } => {
            let home = nexus_home()?;
            init_preset_at(&home, &name)
        }
        PresetCommand::List => list_presets(config).await,
    }
}

/// Internal: scaffold a preset at a specific nexus home path.
fn init_preset_at(home: &std::path::Path, name: &str) -> Result<()> {
    // Validate name is a single path segment.
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.starts_with('_')
    {
        return Err(CliError::Other(format!(
            "Invalid preset name {name:?}. Must be a non-empty path segment without path separators, \
             not '.', '..', and must not start with '_'."
        )));
    }

    let bundle_dir = user_preset_bundle_dir(home, name);

    if bundle_dir.exists() {
        return Err(CliError::Other(format!(
            "Preset '{name}' already exists at {}",
            bundle_dir.display()
        )));
    }

    // Create directory structure.
    let prompts_dir = bundle_dir.join("prompts");
    std::fs::create_dir_all(&prompts_dir)?;

    // Write preset.yaml with the name substituted in.
    let preset_yaml = PRESET_INIT_TEMPLATE.replace("{{name}}", name);
    std::fs::write(bundle_dir.join("preset.yaml"), preset_yaml)?;

    // Write a default start prompt.
    std::fs::write(prompts_dir.join("start.md"), PROMPT_INIT_CONTENT)?;

    println!("✓ Created preset '{}' at {}", name, bundle_dir.display());
    println!("  preset.yaml  — manifest (edit to define your strategy)");
    println!("  prompts/     — prompt templates");
    println!();
    println!(
        "Next: edit {} to customize your strategy.",
        bundle_dir.join("preset.yaml").display()
    );

    Ok(())
}

/// List all available presets grouped by source.
///
/// Groups presets into `[embedded]`, `[system]`, and `[user]` sections.
/// If a user preset has the same ID as an embedded preset, the user version
/// takes precedence and is marked with `(overrides embedded)`.
async fn list_presets(config: &CliConfig) -> Result<()> {
    // ── Embedded presets ────────────────────────────────────────────
    // Get from the daemon API (which has access to the orchestration crate).
    let client = crate::api::DaemonClient::from_config(config);
    let base = "/v1/local/orchestration";

    let resp: nexus_contracts::local::orchestration::http::ListPresetsResponse =
        client.get(&format!("{base}/presets")).await?;

    // Classify presets by source.
    let embedded: Vec<&String> = resp
        .presets
        .iter()
        .filter(|p| !p.starts_with("_system."))
        .collect();
    let system: Vec<&String> = resp
        .presets
        .iter()
        .filter(|p| p.starts_with("_system."))
        .collect();

    // ── User presets ────────────────────────────────────────────────
    // Scan user preset IDs directly from the filesystem.
    let home = crate::config::user_home_dir()?;
    let user_ids = nexus_home_layout::list_user_preset_ids(&home);

    println!("Available presets:\n");

    // Embedded section.
    println!("[embedded]");
    if embedded.is_empty() {
        println!("  (none)");
    } else {
        for id in &embedded {
            let user_override = user_ids.iter().any(|e| e == id.as_str());
            if user_override {
                println!("  {id}  (overridden by user preset)");
            } else {
                println!("  {id}");
            }
        }
    }

    // System section.
    println!("\n[system]");
    if system.is_empty() {
        println!("  (none)");
    } else {
        for id in &system {
            println!("  {id}");
        }
    }

    // User section.
    println!("\n[user]");
    if user_ids.is_empty() {
        println!("  (none)");
    } else {
        for id in &user_ids {
            let embedded_override = embedded.iter().any(|e| e.as_str() == id);
            if embedded_override {
                println!("  {id}  (overrides embedded)");
            } else {
                println!("  {id}");
            }
        }
    }

    println!(
        "\n{} preset(s) total ({} embedded, {} system, {} user)",
        embedded.len() + system.len() + user_ids.len(),
        embedded.len(),
        system.len(),
        user_ids.len()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_init_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "init", "my-strategy"]).expect("parse");
        match cmd.command {
            PresetCommand::Init { name } => assert_eq!(name, "my-strategy"),
            PresetCommand::List => panic!("expected Init variant"),
        }
    }

    #[test]
    fn preset_list_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "list"]).expect("parse");
        match cmd.command {
            PresetCommand::List => {} // expected
            PresetCommand::Init { .. } => panic!("expected List variant"),
        }
    }

    #[test]
    fn preset_subcommand_required() {
        let result = PresetCli::try_parse_from(["preset"]);
        assert!(result.is_err());
    }

    #[test]
    fn init_preset_creates_directory_structure() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nexus_home = tmp.path();

        let result = init_preset_at(nexus_home, "test-strat");
        assert!(result.is_ok(), "init should succeed: {result:?}");

        let bundle_dir = user_preset_bundle_dir(nexus_home, "test-strat");
        assert!(bundle_dir.join("preset.yaml").exists());
        assert!(bundle_dir.join("prompts").exists());
        assert!(bundle_dir.join("prompts/start.md").exists());

        // Verify preset.yaml content includes the name.
        let content = std::fs::read_to_string(bundle_dir.join("preset.yaml")).expect("read preset.yaml");
        assert!(content.contains("id: test-strat"));
    }

    #[test]
    fn init_preset_errors_on_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nexus_home = tmp.path();

        // First init should succeed.
        init_preset_at(nexus_home, "existing-strat").expect("first init");

        // Second init should fail.
        let err = init_preset_at(nexus_home, "existing-strat").expect_err("second init should fail");
        let display = format!("{err}");
        assert!(
            display.contains("already exists"),
            "expected 'already exists' error: {display}"
        );
    }

    #[test]
    fn init_preset_rejects_invalid_names() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nexus_home = tmp.path();

        let empty = init_preset_at(nexus_home, "");
        assert!(empty.is_err());

        let slash = init_preset_at(nexus_home, "foo/bar");
        assert!(slash.is_err());

        let dot = init_preset_at(nexus_home, ".");
        assert!(dot.is_err());

        let dotdot = init_preset_at(nexus_home, "..");
        assert!(dotdot.is_err());

        let system_prefix = init_preset_at(nexus_home, "_system");
        assert!(system_prefix.is_err());
    }

    #[test]
    fn list_user_preset_ids_finds_dirs_with_preset_yaml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();
        let base = nexus_home_layout::user_preset_base_dir(home);
        std::fs::create_dir_all(&base).expect("create base dir");

        // Create a valid preset dir.
        std::fs::create_dir_all(base.join("valid-strat")).expect("create valid-strat");
        std::fs::write(base.join("valid-strat/preset.yaml"), "dummy").expect("write preset.yaml");

        // Create a dir without preset.yaml (should be skipped).
        std::fs::create_dir_all(base.join("empty-dir")).expect("create empty-dir");

        let result = nexus_home_layout::list_user_preset_ids(home);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "valid-strat");
    }
}
