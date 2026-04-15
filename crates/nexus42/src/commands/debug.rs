//! Debug Command — Internal debugging utilities
//!
//! Provides commands for dumping workspace state and replaying deltas
//! for troubleshooting sync behavior.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum DebugCommand {
    /// Dump workspace state to JSON for debugging
    DumpWorkspace {
        /// Output format: json or toml (default: json)
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Replay a specific delta for debugging sync behavior
    ReplayDelta {
        /// Delta ID to replay
        delta_id: String,
    },
}

/// Run debug command
pub async fn run(cmd: DebugCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        DebugCommand::DumpWorkspace { format } => dump_workspace(config, &format).await,
        DebugCommand::ReplayDelta { delta_id } => replay_delta(config, &delta_id).await,
    }
}

/// Recursively strip null values from a JSON value (TOML does not support nulls).
fn strip_nulls(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Null => serde_json::Value::String("(null)".to_string()),
        serde_json::Value::Object(map) => {
            let cleaned: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), strip_nulls(v)))
                .collect();
            serde_json::Value::Object(cleaned)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(strip_nulls).collect())
        }
        other => other.clone(),
    }
}

/// Serialize workspace state to JSON or TOML for debugging.
async fn dump_workspace(config: &CliConfig, format: &str) -> Result<()> {
    let mut state = serde_json::Map::new();

    // --- Config snapshot ---
    let config_snapshot = serde_json::json!({
        "workspace_path": config.workspace_path,
        "active_creator_id": config.active_creator_id,
        "platform_url": config.platform_url,
        "daemon_url": config.daemon_url,
        "runtime_mode": config.runtime_mode().to_string(),
    });
    state.insert("config".to_string(), config_snapshot);

    // --- Nexus home ---
    if let Ok(home) = crate::config::nexus_home() {
        state.insert(
            "nexus_home".to_string(),
            serde_json::Value::String(home.display().to_string()),
        );
    }

    // --- Workspace root ---
    if let Some(root) = crate::config::find_workspace_root() {
        state.insert(
            "workspace_root".to_string(),
            serde_json::Value::String(root.display().to_string()),
        );
    }

    // --- Daemon status (non-blocking, with short timeout) ---
    let client = crate::api::DaemonClient::with_timeouts(
        &config.daemon_url,
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(5),
    );
    match client.health_check().await {
        Ok(true) => {
            match client
                .get::<serde_json::Value>("/v1/local/runtime/status")
                .await
            {
                Ok(status) => {
                    state.insert("daemon_status".to_string(), status);
                }
                Err(e) => {
                    state.insert(
                        "daemon_status".to_string(),
                        serde_json::json!({"error": e.to_string()}),
                    );
                }
            }
        }
        Ok(false) => {
            state.insert(
                "daemon_status".to_string(),
                serde_json::json!({"running": false}),
            );
        }
        Err(e) => {
            state.insert(
                "daemon_status".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // --- Database state (best-effort) ---
    match crate::config::resolve_state_db_path(config) {
        Ok(db_path) => {
            let mut db_state = serde_json::Map::new();
            db_state.insert(
                "path".to_string(),
                serde_json::Value::String(db_path.display().to_string()),
            );
            db_state.insert(
                "exists".to_string(),
                serde_json::Value::Bool(db_path.exists()),
            );
            if db_path.exists() {
                if let Ok(metadata) = std::fs::metadata(&db_path) {
                    db_state.insert(
                        "size_bytes".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(metadata.len())),
                    );
                }
            }
            state.insert("database".to_string(), serde_json::Value::Object(db_state));
        }
        Err(e) => {
            state.insert(
                "database".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // --- Output ---
    let output = match format {
        "toml" => {
            let json_val = serde_json::Value::Object(state);
            // TOML does not support null values; strip them before serialization
            let cleaned = strip_nulls(&json_val);
            toml::to_string_pretty(&cleaned)
                .map_err(|e| anyhow::anyhow!("Failed to serialize to TOML: {}", e))?
        }
        _ => serde_json::to_string_pretty(&serde_json::Value::Object(state))
            .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON: {}", e))?,
    };

    println!("{}", output);
    Ok(())
}

/// Replay a specific delta for debugging sync behavior.
///
/// MVP: prints delta metadata retrieved from the daemon. Does not
/// perform actual state mutation.
async fn replay_delta(config: &CliConfig, delta_id: &str) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    // Check daemon availability
    if !client.health_check().await? {
        return Err(crate::errors::CliError::DaemonNotRunning);
    }

    println!("Replaying delta: {}", delta_id);
    println!();

    // Attempt to fetch delta info from daemon
    match client
        .get::<serde_json::Value>(&format!("/v1/local/delta/{}", delta_id))
        .await
    {
        Ok(delta) => {
            println!("Delta metadata:");
            println!(
                "{}",
                serde_json::to_string_pretty(&delta).unwrap_or_else(|_| delta.to_string())
            );
        }
        Err(e) => {
            println!("Could not fetch delta from daemon: {}", e);
            println!();
            println!("This may mean:");
            println!("  - The delta ID does not exist");
            println!("  - The daemon does not support delta replay yet");
            println!("  - Check available deltas with the sync command");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_command_clap_parsing_dump_workspace() {
        use clap::Parser;

        #[derive(Parser)]
        struct App {
            #[command(subcommand)]
            cmd: Option<DebugCommand>,
        }

        let app = App::try_parse_from(["test", "dump-workspace"]);
        assert!(app.is_ok());
        match app.unwrap().cmd {
            Some(DebugCommand::DumpWorkspace { format }) => {
                assert_eq!(format, "json");
            }
            _ => panic!("Expected DumpWorkspace"),
        }
    }

    #[test]
    fn debug_command_clap_parsing_dump_workspace_toml() {
        use clap::Parser;

        #[derive(Parser)]
        struct App {
            #[command(subcommand)]
            cmd: Option<DebugCommand>,
        }

        let app = App::try_parse_from(["test", "dump-workspace", "--format", "toml"]);
        assert!(app.is_ok());
        match app.unwrap().cmd {
            Some(DebugCommand::DumpWorkspace { format }) => {
                assert_eq!(format, "toml");
            }
            _ => panic!("Expected DumpWorkspace"),
        }
    }

    #[test]
    fn debug_command_clap_parsing_replay_delta() {
        use clap::Parser;

        #[derive(Parser)]
        struct App {
            #[command(subcommand)]
            cmd: Option<DebugCommand>,
        }

        let app = App::try_parse_from(["test", "replay-delta", "delta-123"]);
        assert!(app.is_ok());
        match app.unwrap().cmd {
            Some(DebugCommand::ReplayDelta { delta_id }) => {
                assert_eq!(delta_id, "delta-123");
            }
            _ => panic!("Expected ReplayDelta"),
        }
    }

    #[tokio::test]
    async fn dump_workspace_with_default_config() {
        let config = CliConfig::default();
        // Should not panic; may not have daemon or workspace
        let result = dump_workspace(&config, "json").await;
        assert!(
            result.is_ok(),
            "dump_workspace should not error: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn dump_workspace_toml_format() {
        let config = CliConfig::default();
        let result = dump_workspace(&config, "toml").await;
        assert!(
            result.is_ok(),
            "dump_workspace TOML should not error: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn replay_delta_daemon_not_running() {
        let config = CliConfig::default();
        // Default config points to localhost:8420 which is unlikely running in tests
        let result = replay_delta(&config, "nonexistent").await;
        // Should error because daemon is not running
        assert!(result.is_err());
    }
}
