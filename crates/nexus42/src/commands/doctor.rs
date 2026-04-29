//! Doctor Command — Diagnostic health checks
//!
//! Provides environment validation, daemon connectivity checks,
//! database status, and version compatibility reporting.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// Individual health check result.
struct HealthCheck {
    name: String,
    status: HealthStatus,
    detail: String,
}

#[derive(Debug, PartialEq)]
enum HealthStatus {
    Ok,
    Warning,
    Error,
}

impl HealthStatus {
    const fn label(&self) -> &str {
        match self {
            Self::Ok => "OK",
            Self::Warning => "WARN",
            Self::Error => "FAIL",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum DoctorCommand {
    /// Run all health checks (default)
    Check,
}

/// Run doctor command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Health checks fail critically
/// - Configuration cannot be loaded
pub async fn run(cmd: DoctorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        DoctorCommand::Check => run_checks(config).await,
    }
}

/// Run all diagnostic checks and print a summary table.
async fn run_checks(config: &CliConfig) -> Result<()> {
    let mut checks: Vec<HealthCheck> = Vec::new();

    println!("nexus42 doctor — diagnostic health checks");
    println!();

    // 1. Daemon connectivity
    checks.push(daemon_check(config).await);

    // 2. Config file
    checks.push(config_check());

    // 3. Database
    checks.push(database_check(config).await);

    // 4. Workspace directory structure
    checks.push(workspace_check(config));

    // 5. Version compatibility
    checks.push(version_check(config).await);

    // --- Print summary table ---
    println!("{:<30} {:>6}  Detail", "Check", "Status");
    println!("{}", "-".repeat(80));

    let mut ok_count = 0usize;
    let mut warn_count = 0usize;
    let mut fail_count = 0usize;

    for check in &checks {
        println!(
            "{:<30} {:>6}  {}",
            check.name,
            check.status.label(),
            check.detail
        );
        match check.status {
            HealthStatus::Ok => ok_count += 1,
            HealthStatus::Warning => warn_count += 1,
            HealthStatus::Error => fail_count += 1,
        }
    }

    println!("{}", "-".repeat(80));
    println!(
        "Summary: {ok_count} OK, {warn_count} warnings, {fail_count} failures"
    );

    if fail_count > 0 {
        println!();
        println!("Some checks failed. See details above for suggested fixes.");
    }

    Ok(())
}

/// Check daemon connectivity and responsiveness.
async fn daemon_check(config: &CliConfig) -> HealthCheck {
    let client = crate::api::DaemonClient::with_timeouts(
        &config.daemon_url,
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(5),
    );

    match client.health_check().await {
        Ok(true) => {
            // Try to get version info
            match client
                .get::<crate::api::models::RuntimeStatus>("/v1/local/runtime/status")
                .await
            {
                Ok(status) => HealthCheck {
                    name: "Daemon connectivity".to_string(),
                    status: HealthStatus::Ok,
                    detail: format!(
                        "running (v{}, up {}s)",
                        status.version, status.uptime_seconds
                    ),
                },
                Err(_) => HealthCheck {
                    name: "Daemon connectivity".to_string(),
                    status: HealthStatus::Warning,
                    detail: "running but status endpoint failed".to_string(),
                },
            }
        }
        Ok(false) => HealthCheck {
            name: "Daemon connectivity".to_string(),
            status: HealthStatus::Error,
            detail: format!(
                "daemon not responding at {} — run `nexus42 daemon start`",
                config.daemon_url
            ),
        },
        Err(e) => HealthCheck {
            name: "Daemon connectivity".to_string(),
            status: HealthStatus::Error,
            detail: format!("connection error: {e}"),
        },
    }
}

/// Check config file exists and is readable.
fn config_check() -> HealthCheck {
    match crate::config::CliConfig::path() {
        Ok(path) => {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        if content.trim().is_empty() {
                            HealthCheck {
                                name: "Config file".to_string(),
                                status: HealthStatus::Warning,
                                detail: format!("file is empty ({})", path.display()),
                            }
                        } else {
                            HealthCheck {
                                name: "Config file".to_string(),
                                status: HealthStatus::Ok,
                                detail: format!("found at {}", path.display()),
                            }
                        }
                    }
                    Err(e) => HealthCheck {
                        name: "Config file".to_string(),
                        status: HealthStatus::Error,
                        detail: format!("not readable: {e}"),
                    },
                }
            } else {
                HealthCheck {
                    name: "Config file".to_string(),
                    status: HealthStatus::Warning,
                    detail: format!("not found at {} (defaults will be used)", path.display()),
                }
            }
        }
        Err(e) => HealthCheck {
            name: "Config file".to_string(),
            status: HealthStatus::Error,
            detail: format!("cannot determine config path: {e}"),
        },
    }
}

/// Check `SQLite` database file.
async fn database_check(config: &CliConfig) -> HealthCheck {
    match crate::config::resolve_state_db_path(config) {
        Ok(db_path) => {
            if db_path.exists() {
                // Try to open and validate
                match crate::db::Schema::init(&db_path).await {
                    Ok(pool) => {
                        // Read schema version if available
                        let version_info = nexus_local_db::read_versions(&pool)
                            .await
                            .ok()
                            .map(|v| format!("(schema v{})", v.schema_version));
                        let detail = version_info.map_or_else(|| format!("found at {}", db_path.display()), |v| format!("found at {} {}", db_path.display(), v));
                        HealthCheck {
                            name: "Database".to_string(),
                            status: HealthStatus::Ok,
                            detail,
                        }
                    }
                    Err(e) => HealthCheck {
                        name: "Database".to_string(),
                        status: HealthStatus::Error,
                        detail: format!("schema init failed: {e}"),
                    },
                }
            } else {
                HealthCheck {
                    name: "Database".to_string(),
                    status: HealthStatus::Warning,
                    detail: format!("not found at {}", db_path.display()),
                }
            }
        }
        Err(e) => HealthCheck {
            name: "Database".to_string(),
            status: HealthStatus::Warning,
            detail: format!("path not resolvable: {e}"),
        },
    }
}

/// Check workspace directory structure.
fn workspace_check(config: &CliConfig) -> HealthCheck {
    match &config.workspace_path {
        Some(path) => {
            if path.exists() {
                if path.is_dir() {
                    // Check for .nexus42 subdirectory
                    let nexus_dir = crate::config::workspace_nexus_dir(path);
                    if nexus_dir.exists() {
                        HealthCheck {
                            name: "Workspace directory".to_string(),
                            status: HealthStatus::Ok,
                            detail: format!("initialized at {}", path.display()),
                        }
                    } else {
                        HealthCheck {
                            name: "Workspace directory".to_string(),
                            status: HealthStatus::Warning,
                            detail:
                                "directory exists but not initialized (no .nexus42/ subdirectory)"
                                    .to_string(),
                        }
                    }
                } else {
                    HealthCheck {
                        name: "Workspace directory".to_string(),
                        status: HealthStatus::Error,
                        detail: format!("workspace_path is not a directory: {}", path.display()),
                    }
                }
            } else {
                HealthCheck {
                    name: "Workspace directory".to_string(),
                    status: HealthStatus::Warning,
                    detail: format!("path does not exist: {}", path.display()),
                }
            }
        }
        None => {
            // Check if we're inside a workspace by walking up
            match crate::config::find_workspace_root() {
                Some(root) => HealthCheck {
                    name: "Workspace directory".to_string(),
                    status: HealthStatus::Ok,
                    detail: format!("detected at {}", root.display()),
                },
                None => HealthCheck {
                    name: "Workspace directory".to_string(),
                    status: HealthStatus::Warning,
                    detail: "no workspace configured or detected".to_string(),
                },
            }
        }
    }
}

/// Check version compatibility between CLI and daemon.
async fn version_check(config: &CliConfig) -> HealthCheck {
    let cli_version = env!("CARGO_PKG_VERSION");

    let client = crate::api::DaemonClient::with_timeouts(
        &config.daemon_url,
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(5),
    );

    match client.health_check().await {
        Ok(true) => {
            match client
                .get::<crate::api::models::RuntimeStatus>("/v1/local/runtime/status")
                .await
            {
                Ok(status) => {
                    let daemon_version = &status.version;
                    if cli_version == daemon_version {
                        HealthCheck {
                            name: "Version compatibility".to_string(),
                            status: HealthStatus::Ok,
                            detail: format!("CLI v{cli_version} == Daemon v{daemon_version}"),
                        }
                    } else {
                        HealthCheck {
                            name: "Version compatibility".to_string(),
                            status: HealthStatus::Warning,
                            detail: format!(
                                "CLI v{cli_version} != Daemon v{daemon_version} (may cause issues)"
                            ),
                        }
                    }
                }
                Err(_) => HealthCheck {
                    name: "Version compatibility".to_string(),
                    status: HealthStatus::Warning,
                    detail: format!(
                        "CLI v{cli_version} (daemon version unknown — status endpoint failed)"
                    ),
                },
            }
        }
        Ok(false) => HealthCheck {
            name: "Version compatibility".to_string(),
            status: HealthStatus::Warning,
            detail: format!(
                "CLI v{cli_version} (daemon not running, cannot compare — run `nexus42 daemon start`)"
            ),
        },
        Err(_) => HealthCheck {
            name: "Version compatibility".to_string(),
            status: HealthStatus::Warning,
            detail: format!(
                "CLI v{cli_version} (daemon not reachable, cannot compare — check daemon status)"
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_command_clap_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct App {
            #[command(subcommand)]
            cmd: Option<DoctorCommand>,
        }

        let app = App::try_parse_from(["test", "check"]);
        assert!(app.is_ok());
        match app.unwrap().cmd {
            Some(DoctorCommand::Check) => {}
            _ => panic!("Expected Check"),
        }
    }

    #[test]
    fn health_status_labels() {
        assert_eq!(HealthStatus::Ok.label(), "OK");
        assert_eq!(HealthStatus::Warning.label(), "WARN");
        assert_eq!(HealthStatus::Error.label(), "FAIL");
    }

    #[test]
    fn config_check_with_missing_file() {
        // The config file may or may not exist depending on environment,
        // but the check should never panic.
        let result = config_check();
        assert!(!result.name.is_empty());
        assert!(!result.detail.is_empty());
        // Status should be one of the three variants
        assert!(
            result.status == HealthStatus::Ok
                || result.status == HealthStatus::Warning
                || result.status == HealthStatus::Error
        );
    }

    #[test]
    fn workspace_check_with_default_config() {
        let config = CliConfig::default();
        let result = workspace_check(&config);
        assert_eq!(result.name, "Workspace directory");
        assert!(!result.detail.is_empty());
    }

    #[test]
    fn workspace_check_with_explicit_path() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let mut config = CliConfig::default();
        config.workspace_path = Some(tmp.path().to_path_buf());
        let result = workspace_check(&config);
        assert_eq!(result.name, "Workspace directory");
        // Directory exists but no .nexus42/ — should be Warning
        assert_eq!(result.status, HealthStatus::Warning);
    }

    #[test]
    fn workspace_check_with_initialized_workspace() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create .nexus42 dir");
        let mut config = CliConfig::default();
        config.workspace_path = Some(tmp.path().to_path_buf());
        let result = workspace_check(&config);
        assert_eq!(result.status, HealthStatus::Ok);
        assert!(result.detail.contains("initialized"));
    }

    #[tokio::test]
    async fn daemon_check_with_no_daemon() {
        let config = CliConfig::default();
        let result = daemon_check(&config).await;
        assert_eq!(result.name, "Daemon connectivity");
        // Daemon unlikely running in CI
        assert_eq!(result.status, HealthStatus::Error);
    }

    #[tokio::test]
    async fn version_check_with_no_daemon() {
        let config = CliConfig::default();
        let result = version_check(&config).await;
        assert_eq!(result.name, "Version compatibility");
        // Without daemon, should report CLI version with Warning status (N-002)
        assert_eq!(result.status, HealthStatus::Warning);
        assert!(result.detail.contains("CLI v"));
    }

    #[tokio::test]
    async fn run_checks_completes_without_error() {
        let config = CliConfig::default();
        // Should not panic even with no daemon or workspace
        let result = run_checks(&config).await;
        assert!(
            result.is_ok(),
            "run_checks should not error: {:?}",
            result.err()
        );
    }
}
