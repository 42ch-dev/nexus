//! Runtime mode management command.

use crate::config::CliConfig;
use crate::domain::runtime_mode::DomainRuntimeMode;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum RuntimeModeCommand {
    /// Show current runtime mode
    Show,
    /// Set runtime mode (`local_only`, `local_first`, `cloud_enhanced`)
    Set {
        /// Target runtime mode
        mode: String,
    },
}

/// Run `runtime_mode` command.
///
/// # Errors
///
/// Returns an error if:
/// - Invalid runtime mode value
/// - CLI configuration cannot be saved
pub fn run(command: RuntimeModeCommand, config: &CliConfig) -> Result<()> {
    match command {
        RuntimeModeCommand::Show => {
            show(config);
            Ok(())
        }
        RuntimeModeCommand::Set { mode } => set(config, &mode),
    }
}

fn show(config: &CliConfig) {
    let mode = config.runtime_mode();
    println!("Runtime mode: {mode}");
    println!();
    println!(
        "Platform dependency: {}",
        if mode.allows_platform() {
            "allowed"
        } else {
            "prohibited"
        }
    );
    println!(
        "Platform LLM: {}",
        if mode.allows_platform_llm() {
            "allowed"
        } else {
            "prohibited"
        }
    );

    // Display degradation policy info (if available)
    if let Some(snapshot) = config.degradation_snapshot() {
        println!();
        println!("--- Degradation Status ---");
        println!("State: {}", snapshot.state.display_label());
        println!("Failure count: {}", snapshot.failure_count);
        if let Some(hc) = &snapshot.last_health_check {
            // Parse ISO 8601 back to a displayable time; fall back to raw string on parse error.
            let time_str = chrono::DateTime::parse_from_rfc3339(&hc.checked_at).map_or_else(
                |_| hc.checked_at.clone(),
                |dt| dt.format("%H:%M:%S").to_string(),
            );
            println!(
                "Last health check: {} ({})",
                time_str,
                if hc.is_healthy {
                    "healthy"
                } else {
                    "unhealthy"
                }
            );
        }
    }

    println!();
    if mode.is_local_only() {
        println!(
            "Blocked operations: sync, publish, auth login/register, platform context assemble, explore"
        );
    }
}

fn set(config: &CliConfig, mode_str: &str) -> Result<()> {
    // NOTE (S-007): Not warning about the daemon when setting runtime mode is an
    // intentional UX choice. Users who run `nexus42 runtime-mode set` are expected
    // to manage the daemon lifecycle themselves — they either restart the daemon
    // to pick up the new mode, or they understand that the daemon uses the mode
    // from its own startup snapshot. A daemon notification mechanism is deferred.
    // V1.2 residual S-007 (program-review, nit): Runtime mode set no daemon warning
    // UX choice: users who set runtime mode manage daemon themselves
    let new_mode = DomainRuntimeMode::parse(mode_str)
        .map_err(|e| crate::errors::CliError::Config(e.to_string()))?;

    let old_mode = config.runtime_mode();
    if old_mode == new_mode {
        println!("Runtime mode is already '{new_mode}'.");
        return Ok(());
    }

    println!("Switching runtime mode: {old_mode} → {new_mode}");

    // Warn about platform requirements
    if new_mode.allows_platform() {
        println!("Note: {new_mode} mode requires platform connectivity for some operations.");
    }

    // Persist
    let mut updated = config.clone();
    updated.runtime_mode = new_mode;
    updated.save()?;

    println!("Runtime mode set to '{new_mode}'.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::degradation::{DegradationSnapshot, DegradationState, HealthCheckSnapshot};

    /// Helper: build a `CliConfig` with a degradation snapshot.
    fn config_with_degradation(
        state: DegradationState,
        failure_count: u32,
        last_healthy: Option<bool>,
    ) -> CliConfig {
        CliConfig {
            degradation_snapshot: Some(DegradationSnapshot {
                state,
                failure_count,
                last_health_check: last_healthy.map(|is_healthy| HealthCheckSnapshot {
                    is_healthy,
                    checked_at: "2026-04-15T10:30:00Z".to_string(),
                }),
                last_upgrade_attempt: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn show_without_degradation_outputs_basic_info() {
        let config = CliConfig::default();
        // Capture stdout
        let mut buf = Vec::new();
        show_to(&config, &mut buf).expect("show_to");
        let output = String::from_utf8(buf).expect("utf8 output");
        assert!(output.contains("Runtime mode: local_only"));
        assert!(output.contains("Platform dependency: prohibited"));
        assert!(output.contains("Blocked operations:"));
        assert!(!output.contains("Degradation Status"));
    }

    #[test]
    fn show_with_normal_degradation_outputs_section() {
        let config = config_with_degradation(DegradationState::Normal, 0, Some(true));
        let mut buf = Vec::new();
        show_to(&config, &mut buf).expect("show_to");
        let output = String::from_utf8(buf).expect("utf8 output");
        assert!(output.contains("--- Degradation Status ---"));
        assert!(output.contains("State: Normal"));
        assert!(output.contains("Failure count: 0"));
        assert!(output.contains("Last health check:"));
        assert!(output.contains("healthy"));
    }

    #[test]
    fn show_with_degraded_state_outputs_labels() {
        let config = config_with_degradation(DegradationState::DegradedLevel1, 3, Some(false));
        let mut buf = Vec::new();
        show_to(&config, &mut buf).expect("show_to");
        let output = String::from_utf8(buf).expect("utf8 output");
        assert!(output.contains("State: Degraded (Level 1)"));
        assert!(output.contains("Failure count: 3"));
        assert!(output.contains("unhealthy"));
    }

    #[test]
    fn show_forced_local_only_outputs_label() {
        let config = config_with_degradation(DegradationState::ForcedLocalOnly, 0, None);
        let mut buf = Vec::new();
        show_to(&config, &mut buf).expect("show_to");
        let output = String::from_utf8(buf).expect("utf8 output");
        assert!(output.contains("State: Forced local_only"));
        // No last health check line when None
        assert!(!output.contains("Last health check:"));
    }

    /// Write `show()` output to a `dyn Write` instead of stdout (for testing).
    fn show_to(config: &CliConfig, w: &mut dyn std::io::Write) -> Result<()> {
        let mode = config.runtime_mode();
        writeln!(w, "Runtime mode: {mode}")?;
        writeln!(w)?;
        writeln!(
            w,
            "Platform dependency: {}",
            if mode.allows_platform() {
                "allowed"
            } else {
                "prohibited"
            }
        )?;
        writeln!(
            w,
            "Platform LLM: {}",
            if mode.allows_platform_llm() {
                "allowed"
            } else {
                "prohibited"
            }
        )?;

        if let Some(snapshot) = config.degradation_snapshot() {
            writeln!(w)?;
            writeln!(w, "--- Degradation Status ---")?;
            writeln!(w, "State: {}", snapshot.state.display_label())?;
            writeln!(w, "Failure count: {}", snapshot.failure_count)?;
            if let Some(hc) = &snapshot.last_health_check {
                let time_str = chrono::DateTime::parse_from_rfc3339(&hc.checked_at).map_or_else(
                    |_| hc.checked_at.clone(),
                    |dt| dt.format("%H:%M:%S").to_string(),
                );
                writeln!(
                    w,
                    "Last health check: {} ({})",
                    time_str,
                    if hc.is_healthy {
                        "healthy"
                    } else {
                        "unhealthy"
                    }
                )?;
            }
        }

        writeln!(w)?;
        if mode.is_local_only() {
            writeln!(
                w,
                "Blocked operations: sync, publish, auth login/register, platform context assemble, explore"
            )?;
        }
        Ok(())
    }
}
