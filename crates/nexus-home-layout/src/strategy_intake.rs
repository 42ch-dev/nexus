//! Operator strategy-intake mode (composition ADR P6 condition 6).
//!
//! `[strategy_intake] mode` in `config.toml` selects which document a
//! **user-bundle** run executes: `ir-only` (the default) runs the strategy's
//! current apply and refuses a bundle with none, `yaml-only` is the emergency
//! compatibility arm that runs the YAML manifest without reading the apply.
//! Embedded and `_system.` presets never consult the key.
//!
//! The mode is read per new admission and frozen on the run descriptor, so
//! recovery follows the recorded mode, not the current config. `yaml-only`
//! is supported for the v1.243 minor line and removed at v1.244.0; the
//! import/export surface has no removal boundary.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which document a user-bundle run executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyIntakeMode {
    /// IR-primary (the default): a current apply runs its document through
    /// the shared applied-document resolution; a user bundle with no current
    /// apply refuses before graph construction — never a YAML fallback.
    IrOnly,
    /// Emergency compatibility arm: the YAML manifest and graph run without
    /// reading the current apply. Removed at v1.244.0.
    YamlOnly,
}

/// Whether `preset_id` resolved to a user bundle — the only source intake
/// consults an apply (or the intake key) for.
///
/// This is the ONE classification both sides of the intake gate use:
/// [`crate::apply::is_user_preset`] in `nexus-core` delegates here, and the
/// engine's preset trait entries call this same function, so the two cannot
/// disagree about which presets the intake key governs. A `_system.` id is
/// not a user preset even when its bundle is a directory on disk, and an
/// embedded id is not one even when a user bundle shadows it (the loader
/// recorded the source it actually resolved).
#[must_use]
pub fn is_user_strategy_bundle(preset_id: &str, source_is_directory: bool) -> bool {
    !preset_id.starts_with("_system.") && source_is_directory
}

/// Read `[strategy_intake] mode` from `<nexus_home>/config.toml` for one new
/// admission.
///
/// A missing file, a missing `[strategy_intake]` table, or a missing `mode`
/// key all default to [`StrategyIntakeMode::IrOnly`]. An unreadable or
/// unparseable document, a non-table `[strategy_intake]`, or a `mode` value
/// outside `ir-only`/`yaml-only` is an error: a mistyped key must fail
/// closed with the operator-visible refusal, never silently select an arm.
///
/// The read takes no lock: `write_cli_config` publishes the whole document
/// atomically, so a lock-free reader never observes a truncated file
/// (same contract as the provider snapshot reader).
///
/// # Errors
/// The `intake_config_invalid`-prefixed reason when the document is
/// unreadable or unparseable, `[strategy_intake]` is not a table, `mode` is
/// not a string, or the value is outside `ir-only`/`yaml-only`.
pub fn strategy_intake_mode(nexus_home: &Path) -> Result<StrategyIntakeMode, String> {
    let config_path = nexus_home.join("config.toml");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StrategyIntakeMode::IrOnly);
        }
        Err(error) => {
            return Err(format!(
                "intake_config_invalid: cannot read {}: {error}",
                config_path.display()
            ));
        }
    };
    if content.trim().is_empty() {
        return Ok(StrategyIntakeMode::IrOnly);
    }
    let config: toml::Value = toml::from_str(&content).map_err(|error| {
        format!(
            "intake_config_invalid: cannot parse {}: {error}",
            config_path.display()
        )
    })?;
    let Some(section) = config.get("strategy_intake") else {
        return Ok(StrategyIntakeMode::IrOnly);
    };
    let table = section
        .as_table()
        .ok_or_else(|| "intake_config_invalid: [strategy_intake] is not a table".to_string())?;
    let Some(mode) = table.get("mode") else {
        return Ok(StrategyIntakeMode::IrOnly);
    };
    let value = mode.as_str().ok_or_else(|| {
        "intake_config_invalid: [strategy_intake].mode is not a string".to_string()
    })?;
    match value {
        "ir-only" => Ok(StrategyIntakeMode::IrOnly),
        "yaml-only" => Ok(StrategyIntakeMode::YamlOnly),
        other => Err(format!(
            "intake_config_invalid: [strategy_intake].mode is '{other}', expected 'ir-only' or \
             'yaml-only'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with_config(body: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().expect("temp home");
        if !body.is_empty() {
            std::fs::write(home.path().join("config.toml"), body).expect("write config.toml");
        }
        home
    }

    #[test]
    fn missing_file_defaults_to_ir_only() {
        let home = home_with_config("");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::IrOnly)
        );
    }

    #[test]
    fn empty_file_defaults_to_ir_only() {
        let home = home_with_config("   \n");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::IrOnly)
        );
    }

    #[test]
    fn missing_table_and_missing_key_default_to_ir_only() {
        let home = home_with_config("active_creator_id = \"ctr_x\"\n");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::IrOnly)
        );
        let home = home_with_config("[strategy_intake]\nother = 1\n");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::IrOnly)
        );
    }

    #[test]
    fn explicit_modes_parse() {
        let home = home_with_config("[strategy_intake]\nmode = \"ir-only\"\n");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::IrOnly)
        );
        let home = home_with_config("[strategy_intake]\nmode = \"yaml-only\"\n");
        assert_eq!(
            strategy_intake_mode(home.path()),
            Ok(StrategyIntakeMode::YamlOnly)
        );
    }

    #[test]
    fn invalid_config_refuses() {
        let home = home_with_config("this is not toml");
        let error = strategy_intake_mode(home.path()).expect_err("unparseable toml refuses");
        assert!(
            error.contains("intake_config_invalid"),
            "the refusal names the stable text: {error}"
        );

        let home = home_with_config("[strategy_intake]\nmode = \"sometimes\"\n");
        let error = strategy_intake_mode(home.path()).expect_err("unknown mode refuses");
        assert!(error.contains("intake_config_invalid"), "{error}");

        let home = home_with_config("[strategy_intake]\nmode = 3\n");
        let error = strategy_intake_mode(home.path()).expect_err("non-string mode refuses");
        assert!(error.contains("intake_config_invalid"), "{error}");

        let home = home_with_config("strategy_intake = \"yaml-only\"\n");
        let error = strategy_intake_mode(home.path()).expect_err("non-table section refuses");
        assert!(error.contains("intake_config_invalid"), "{error}");
    }

    #[test]
    fn classification_names_one_rule() {
        assert!(is_user_strategy_bundle("my-strategy", true));
        assert!(!is_user_strategy_bundle("my-strategy", false));
        assert!(!is_user_strategy_bundle("_system.research", true));
    }
}
