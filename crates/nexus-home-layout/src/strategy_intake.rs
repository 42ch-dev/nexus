//! Operator strategy-intake mode (composition ADR P6 condition 6).
//!
//! `[strategy_intake] mode` in `config.toml` selects which document a
//! **user-bundle** run executes: `ir-only` (the default, and the only arm)
//! runs the strategy's current apply and refuses a bundle with none.
//! Embedded and `_system.` presets never consult the key.
//!
//! The mode is read per new admission and frozen on the run descriptor, so
//! recovery follows the recorded mode, not the current config. The `yaml-only`
//! kill-switch that re-enabled YAML intake for one minor release after
//! default-off was removed at v1.244.0 (I-000509): a config that still names it
//! is refused with the replacement named, never mapped silently onto `ir-only`.
//! The import/export surface has no removal boundary.

use std::path::Path;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// Which document a user-bundle run executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyIntakeMode {
    /// IR-primary (the default and only arm): a current apply runs its
    /// document through the shared applied-document resolution; a user bundle
    /// with no current apply refuses before graph construction — never a YAML
    /// fallback.
    IrOnly,
}

/// Hand-written so a descriptor recorded while the `yaml-only` arm was live
/// refuses by name instead of reading as an unknown variant — or, worse, as
/// `ir-only`.
///
/// The enum has one arm, so a derived `Deserialize` would answer a recorded
/// `"yaml-only"` with a bare `unknown variant` message: recovery would fail as
/// a parse error with no cause named, for a run that is legitimately
/// non-replayable. Every other value outside `ir-only` refuses the same way,
/// so a hand-edited row cannot select an arm either.
impl<'de> Deserialize<'de> for StrategyIntakeMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "ir-only" => Ok(Self::IrOnly),
            "yaml-only" => Err(D::Error::custom(
                "intake_mode_retired: the run descriptor records [strategy_intake] mode \
                 'yaml-only', which was removed at v1.244.0; that run cannot be rebuilt — \
                 admit a new run under 'ir-only', which executes a user bundle's current apply \
                 and refuses a bundle with none",
            )),
            other => Err(D::Error::custom(format!(
                "intake_mode_invalid: the run descriptor records [strategy_intake] mode \
                 '{other}', expected 'ir-only'"
            ))),
        }
    }
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
/// outside `ir-only` is an error: a mistyped key must fail closed with the
/// operator-visible refusal, never silently select an arm. The removed
/// `yaml-only` arm is such a value — it refuses with `ir-only` named as the
/// replacement, because an operator still writing it is mid-incident and a
/// silent switch of the document their bundles run is the worst outcome.
///
/// The read takes no lock: `write_cli_config` publishes the whole document
/// atomically, so a lock-free reader never observes a truncated file
/// (same contract as the provider snapshot reader).
///
/// # Errors
/// The `intake_config_invalid`-prefixed reason when the document is
/// unreadable or unparseable, `[strategy_intake]` is not a table, `mode` is
/// not a string, or the value is outside `ir-only`.
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
        "yaml-only" => Err(
            "intake_config_invalid: [strategy_intake].mode is 'yaml-only', which was removed at \
             v1.244.0; set mode to 'ir-only' — a user bundle runs its current apply, and one with \
             no apply is refused"
                .to_string(),
        ),
        other => Err(format!(
            "intake_config_invalid: [strategy_intake].mode is '{other}', expected 'ir-only'"
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
    }

    /// The removed `yaml-only` arm is refused, and the refusal names the
    /// replacement: an operator still carrying the key is mid-incident, so a
    /// silent fall-through to `ir-only` would change which document their
    /// bundles run without saying so (I-000509).
    #[test]
    fn yaml_only_refuses_naming_the_replacement() {
        let home = home_with_config("[strategy_intake]\nmode = \"yaml-only\"\n");
        let error = strategy_intake_mode(home.path()).expect_err("the removed arm refuses");
        assert!(error.contains("intake_config_invalid"), "{error}");
        assert!(
            error.contains("'yaml-only'"),
            "the refusal names the key: {error}"
        );
        assert!(
            error.contains("'ir-only'"),
            "the refusal names the replacement: {error}"
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
