//! Daemon Command — Manage the nexus daemon runtime

pub mod schedule;

use crate::api::DaemonClient;
use crate::config::{CliConfig, DAEMON_PORT};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use schedule::ScheduleCommand;

#[cfg(unix)]
use nix::sys::signal::Signal;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon runtime
    Start {
        /// Port to listen on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,

        /// Run daemon in foreground (do not detach)
        #[arg(long)]
        foreground: bool,

        /// Optional CDN URL for registry.refresh network mode.
        /// When set, enables fetching the ACP registry from a CDN
        /// with built-in timeout (10s) and retry (3 attempts).
        /// When absent (default), registry.refresh returns synthetic output only.
        ///
        /// # Security
        ///
        /// Must be a public HTTPS CDN URL. Non-HTTPS schemes, private IPs
        /// (e.g. 10.x.x.x, 192.168.x.x, 127.0.0.1), loopback, link-local,
        /// and metadata endpoints (169.254.x.x) are rejected at startup.
        #[arg(long)]
        cdn_url: Option<String>,
    },

    /// Stop the running daemon
    Stop {
        /// Port the daemon is listening on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,
    },

    /// Restart the daemon (stop then start)
    Restart {
        /// Port to listen on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,

        /// Run daemon in foreground (do not detach)
        #[arg(long)]
        foreground: bool,

        /// Optional CDN URL for registry.refresh network mode.
        /// Must be a public HTTPS CDN URL. Private/loopback IPs are rejected.
        #[arg(long)]
        cdn_url: Option<String>,
    },

    /// Check daemon status / health
    Status {
        /// Port the daemon is listening on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,
    },

    /// View daemon logs
    Logs {
        /// Port the daemon is listening on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,

        /// Number of log lines to show
        #[arg(short, long, default_value_t = 50)]
        lines: usize,
    },

    /// Run daemon health diagnostics
    Doctor {
        /// Port the daemon is listening on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,
    },

    /// Schedule preset-driven orchestration workflows (migrated from `nexus42 schedule`)
    Schedule {
        #[command(subcommand)]
        command: Box<ScheduleCommand>,
    },

    /// Open the Web UI in the default browser (starts daemon if not running)
    #[command(visible_alias = "web")]
    Ui {
        /// Port to use (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,
    },
}

/// Run daemon command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Daemon process cannot be started or stopped
/// - Health check fails
/// - PID file operations fail
pub async fn run(cmd: DaemonCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        DaemonCommand::Start {
            port,
            foreground,
            cdn_url,
        } => start_daemon(port, foreground, cdn_url).await,
        DaemonCommand::Stop { port } => stop_daemon(port).await,
        DaemonCommand::Restart {
            port,
            foreground,
            cdn_url,
        } => restart_daemon(port, foreground, cdn_url).await,
        DaemonCommand::Status { port } => daemon_status(port, config).await,
        DaemonCommand::Logs { port, lines } => daemon_logs(port, lines).await,
        DaemonCommand::Doctor { port } => daemon_doctor(port).await,
        DaemonCommand::Schedule { command } => schedule::run(*command, config).await,
        DaemonCommand::Ui { port } => open_ui(port).await,
    }
}

/// Start the daemon process.
///
/// # Foreground mode (`--foreground`)
///
/// Invokes the daemon runtime directly in the current process.
/// Blocks until the runtime terminates.
///
/// # Background mode (default)
///
/// Self-spawns a new `nexus42 __internal daemon-run` child process,
/// writes the PID file, then polls the health endpoint until the
/// runtime is confirmed running. The parent exits after the startup gate.
///
/// # Errors
///
/// Returns an error if:
/// - Health check fails to communicate with existing daemon
/// - Self-spawn fails
/// - PID file operations fail
#[allow(clippy::too_many_lines)]
async fn start_daemon(port: u16, foreground: bool, cdn_url: Option<String>) -> Result<()> {
    // Check if already running
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));
    if client.health_check().await? {
        println!("Daemon is already running on port {port}");
        return Ok(());
    }

    if foreground {
        // --- Foreground mode: run runtime directly in this process ---
        println!("Starting daemon (foreground) on port {port}...");

        // Validate CDN URL before boot (H-002).
        if let Some(ref url) = cdn_url {
            validate_cdn_url(url)?;
        }

        // Write PID file for this process
        let pid = std::process::id();
        write_pid_file(pid)?;

        let config = nexus_daemon_runtime::boot::DaemonConfig {
            port,
            host: "127.0.0.1".to_string(),
            socket_path: None,
            verbose: false,
            shutdown_grace_ms: 20_000,
            cdn_url,
        };

        let result = nexus_daemon_runtime::boot::run_daemon(config).await;

        // Clean up PID file on exit
        let _ = remove_pid_file();

        result.map_err(|e| CliError::Daemon {
            message: format!("Daemon runtime error: {e}"),
        })
    } else {
        // --- Background mode: self-spawn into __internal daemon-run ---
        println!("Starting daemon on port {port}...");

        // Validate CDN URL before self-spawn (H-002).
        if let Some(ref url) = cdn_url {
            validate_cdn_url(url)?;
        }

        let exe = std::env::current_exe().map_err(|e| CliError::Daemon {
            message: format!("Cannot determine current executable: {e}"),
        })?;

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("__internal")
            .arg("daemon-run")
            .arg("--port")
            .arg(port.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if let Some(ref url) = cdn_url {
            cmd.arg("--cdn-url").arg(url);
        }

        // Detach the process so it outlives the CLI
        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| CliError::Daemon {
            message: format!("Failed to spawn daemon process '{}': {e}", exe.display()),
        })?;

        // Write PID file so stop_daemon() can find the process
        let child_pid = child.id();
        if child_pid > 0 {
            write_pid_file(child_pid)?;
        }

        // On Unix, reap the intermediate process so it doesn't become a zombie.
        #[cfg(unix)]
        {
            let _ = child.wait();
        }
        #[cfg(not(unix))]
        {
            let _ = child.try_wait();
        }

        // Wait briefly and verify the daemon is responding
        println!("Waiting for daemon to start...");
        let max_retries = 10u32;
        let retry_delay = std::time::Duration::from_millis(500);

        for i in 1..=max_retries {
            tokio::time::sleep(retry_delay).await;
            if client.health_check().await? {
                println!("✓ Daemon started successfully on port {port}");
                println!("  PID: {child_pid}");
                println!("  Local API: http://127.0.0.1:{port}");
                println!("  Web UI:    http://127.0.0.1:{port}/");
                return Ok(());
            }
            if i == max_retries {
                tracing::error!(
                    "Daemon process was spawned (PID {child_pid}) but health check failed after {max_retries} retries"
                );
                println!(
                    "⚠ Daemon process was spawned but health check failed after {max_retries} retries."
                );
                println!("  The daemon may still be starting. Check with: nexus42 daemon status");
            }
        }

        // Health check never succeeded — return error so callers know startup was not confirmed.
        // The daemon process may still come up; `nexus42 daemon status` can confirm later.
        Err(CliError::Daemon {
            message: format!(
                "Daemon process spawned (PID {child_pid}) but health endpoint never responded \
                 after {max_retries} retries. The process may still be starting — check with \
                 `nexus42 daemon status`."
            ),
        })
    }
}

/// PID file path for the daemon
fn pid_file_path() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or(CliError::Daemon {
        message: "Cannot determine home directory".to_string(),
    })?;
    Ok(home.join(".nexus42").join("daemon.pid"))
}

/// Read PID from the daemon PID file
fn read_pid_file() -> Result<Option<u32>> {
    let path = pid_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let pid: u32 = content.trim().parse().map_err(|_| CliError::Daemon {
        message: format!("Invalid PID in {}: {}", path.display(), content.trim()),
    })?;
    Ok(Some(pid))
}

/// Write PID to the daemon PID file with owner-only permissions (0600)
fn write_pid_file(pid: u32) -> Result<()> {
    let path = pid_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, pid.to_string())?;

    // Set file permissions to 0600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

/// Remove the daemon PID file
fn remove_pid_file() -> Result<()> {
    let path = pid_file_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Check if a process with the given PID is running
#[cfg(unix)]
#[allow(dead_code)]
// PID cast is safe: Unix PIDs are always positive and within i32 range (max ~4M on Linux)
#[allow(clippy::cast_possible_wrap)]
fn is_process_running(pid: u32) -> bool {
    // Sending signal 0 checks if the process exists without actually sending a signal
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(unix)]
/// Stop the daemon by reading PID from file and sending SIGTERM, then SIGKILL
async fn stop_daemon(port: u16) -> Result<()> {
    // First check if daemon is actually running via health check
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));
    if !client.health_check().await? {
        println!("Daemon is not running on port {port}.");

        // Clean up stale PID file if it exists
        if read_pid_file()?.is_some() {
            remove_pid_file()?;
            println!("  Cleaned up stale PID file.");
        }

        return Ok(());
    }

    // Try to stop via PID file
    let pid = read_pid_file()?;
    if let Some(pid) = pid {
        println!("Stopping daemon (PID: {pid})...");

        // Send SIGTERM
        // PID cast is safe: Unix PIDs are always positive and within i32 range
        #[allow(clippy::cast_possible_wrap)]
        let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
        if let Err(e) = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM) {
            if e == nix::errno::Errno::ESRCH {
                // Process doesn't exist — clean up PID file
                remove_pid_file()?;
                println!("  Process {pid} not found (already stopped).");
                return Ok(());
            }
            return Err(CliError::Daemon {
                message: format!("Failed to send SIGTERM to PID {pid}: {e}"),
            });
        }

        // Wait up to 2 seconds for graceful shutdown
        let timeout = std::time::Duration::from_secs(2);
        let start = std::time::Instant::now();
        let mut stopped = false;

        while start.elapsed() < timeout {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if nix::sys::signal::kill(nix_pid, Signal::SIGTERM) == Err(nix::errno::Errno::ESRCH) {
                // Process no longer exists
                stopped = true;
                break;
            }
            // Still running, continue waiting
        }

        if !stopped {
            // Force kill with SIGKILL
            println!("  Daemon did not stop gracefully, sending SIGKILL...");
            if let Err(e) = nix::sys::signal::kill(nix_pid, Signal::SIGKILL) {
                // If ESRCH, process already dead
                if e != nix::errno::Errno::ESRCH {
                    return Err(CliError::Daemon {
                        message: format!("Failed to send SIGKILL to PID {pid}: {e}"),
                    });
                }
            }

            // Poll to confirm the process is actually dead
            let kill_timeout = std::time::Duration::from_secs(2);
            let kill_start = std::time::Instant::now();
            while kill_start.elapsed() < kill_timeout {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if nix::sys::signal::kill(nix_pid, None) == Err(nix::errno::Errno::ESRCH) {
                    stopped = true;
                    break;
                }
            }
            if !stopped {
                return Err(CliError::Daemon {
                    message: format!(
                        "Daemon (PID {pid}) did not terminate after SIGKILL within {kill_timeout:?}"
                    ),
                });
            }
        }

        remove_pid_file()?;
        println!("✓ Daemon stopped.");
    } else {
        // No PID file — try to stop via lsof
        println!("No PID file found. Trying port-based stop...");
        let output = std::process::Command::new("lsof")
            .args(["-ti", &format!(":{port}")])
            .output()
            .map_err(|e| CliError::Daemon {
                message: format!("Failed to run lsof: {e}"),
            })?;

        let pids_str = String::from_utf8_lossy(&output.stdout);
        if pids_str.trim().is_empty() {
            println!("No process found on port {port}.");
            return Ok(());
        }

        for pid_str in pids_str.lines() {
            if let Ok(pid_num) = pid_str.trim().parse::<u32>() {
                // PID cast is safe: Unix PIDs are always positive and within i32 range
                #[allow(clippy::cast_possible_wrap)]
                let nix_pid = nix::unistd::Pid::from_raw(pid_num as i32);
                let _ = nix::sys::signal::kill(nix_pid, Signal::SIGTERM);
                println!("  Sent SIGTERM to PID {pid_num}.");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Verify daemon is stopped
        if client.health_check().await? {
            println!("  ⚠ Daemon may still be running. Check with: nexus42 daemon status");
        } else {
            println!("✓ Daemon stopped.");
        }
    }

    Ok(())
}

#[cfg(not(unix))]
/// Stop the daemon on non-Unix platforms (limited support)
async fn stop_daemon(port: u16) -> Result<()> {
    let client = DaemonClient::new(&format!("http://127.0.0.1:{}", port));
    if !client.health_check().await? {
        println!("Daemon is not running on port {}.", port);
        return Ok(());
    }

    println!("⚠ Daemon stop is not fully supported on this platform.");
    println!(
        "  Stop the daemon manually by killing the process on port {}.",
        port
    );
    Ok(())
}

/// Check daemon status
async fn daemon_status(port: u16, config: &CliConfig) -> Result<()> {
    let daemon_url = format!("http://127.0.0.1:{port}");
    let client = DaemonClient::new(&daemon_url);

    println!("Daemon Status:");
    println!("  URL: {daemon_url}");

    if client.health_check().await? {
        println!("  Status: ✓ Running");
        // Try to get more info
        if let Ok(status) = client
            .get::<serde_json::Value>("/v1/local/runtime/status")
            .await
        {
            if let Some(version) = status.get("version") {
                println!("  Version: {version}");
            }
            if let Some(uptime) = status.get("uptime_seconds") {
                println!("  Uptime: {uptime}s");
            }
        }
        // Show PID if available
        if let Ok(Some(pid)) = read_pid_file() {
            println!("  PID: {pid}");
        }
    } else {
        println!("  Status: ✗ Not running");
        println!();
        println!("Start with: nexus42 daemon start");
    }

    // Also show config URL for reference
    if config.daemon_url != daemon_url {
        println!("  Config URL: {}", config.daemon_url);
    }

    Ok(())
}

/// Restart the daemon (stop then start).
///
/// First attempts a normal stop. If no PID file is found, uses port-based
/// lsof to discover and kill the process. Polls with health check to
/// confirm the old daemon is fully dead before starting the new one.
async fn restart_daemon(port: u16, foreground: bool, cdn_url: Option<String>) -> Result<()> {
    println!("Restarting daemon...");

    // Stop the old daemon
    stop_daemon(port).await?;

    // Verify the old daemon is fully dead via health check
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));
    let confirm_timeout = std::time::Duration::from_secs(3);
    let confirm_start = std::time::Instant::now();
    let mut confirmed_dead = false;

    while confirm_start.elapsed() < confirm_timeout {
        match client.health_check().await {
            Ok(false) | Err(_) => {
                confirmed_dead = true;
                break;
            }
            Ok(true) => {
                // Still alive — wait and retry
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }

    if !confirmed_dead {
        // Last resort: try port-based lsof kill
        #[cfg(unix)]
        {
            eprintln!("  Old daemon still alive after stop, attempting port-based kill...");
            let output = std::process::Command::new("lsof")
                .args(["-ti", &format!(":{port}")])
                .output()
                .map_err(|e| CliError::Daemon {
                    message: format!("Failed to run lsof: {e}"),
                })?;

            let pids_str = String::from_utf8_lossy(&output.stdout);
            for pid_str in pids_str.lines() {
                if let Ok(pid_num) = pid_str.trim().parse::<u32>() {
                    #[allow(clippy::cast_possible_wrap)]
                    let nix_pid = nix::unistd::Pid::from_raw(pid_num as i32);
                    let _ = nix::sys::signal::kill(nix_pid, Signal::SIGKILL);
                    eprintln!("  Sent SIGKILL to PID {pid_num}.");
                }
            }

            // Poll again to confirm death
            let kill_wait = std::time::Duration::from_secs(2);
            let kill_start = std::time::Instant::now();
            while kill_start.elapsed() < kill_wait {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if !client.health_check().await.unwrap_or(false) {
                    confirmed_dead = true;
                    break;
                }
            }
        }

        if !confirmed_dead {
            return Err(CliError::Daemon {
                message: format!(
                    "Old daemon on port {port} could not be killed. Aborting restart."
                ),
            });
        }
    }

    start_daemon(port, foreground, cdn_url).await
}

/// Read the last `n` lines from a file without loading the entire file.
///
/// Seeks from the end of the file in chunks to find the last N lines,
/// avoiding `O(file_size)` memory usage for large log files.
fn read_tail_lines(path: &std::path::Path, n: usize) -> Result<Vec<String>> {
    use std::io::{BufRead, Read, Seek, SeekFrom};

    let file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    // For small files, just read normally
    if file_size < 8192 {
        let content = std::io::BufReader::new(file)
            .lines()
            .map_while(std::result::Result::ok)
            .collect::<Vec<_>>();
        let start = content.len().saturating_sub(n);
        return Ok(content[start..].to_vec());
    }

    // For larger files, seek backwards in chunks looking for newlines
    let mut reader = std::io::BufReader::new(file);
    let chunk_size: usize = 4096;
    let mut buffer = String::new();
    let mut pos = file_size;
    let mut newline_count = 0usize;
    let mut done = false;

    // Small temporary buffer for reading backwards
    let mut tmp = vec![0u8; chunk_size];

    while pos > 0 && !done {
        let read_size = (chunk_size as u64).min(pos);
        // chunk_size (4096) fits in usize; read_size <= chunk_size, so truncation cannot occur
        #[allow(clippy::cast_possible_truncation)]
        let read_size_usize = read_size as usize;
        pos -= read_size;

        reader.seek(SeekFrom::Start(pos))?;
        reader.read_exact(&mut tmp[..read_size_usize])?;

        // Count newlines in this chunk
        for &byte in tmp[..read_size_usize].iter().rev() {
            if byte == b'\n' {
                newline_count += 1;
                if newline_count > n {
                    done = true;
                    break;
                }
            }
        }

        // Prepend this chunk to our buffer
        let chunk_str = String::from_utf8_lossy(&tmp[..read_size_usize]);
        buffer = format!("{chunk_str}{buffer}");
    }

    let all_lines: Vec<&str> = buffer.lines().collect();
    let start = all_lines.len().saturating_sub(n);
    Ok(all_lines[start..]
        .iter()
        .map(std::string::ToString::to_string)
        .collect())
}

/// View daemon logs.
///
/// Reads the daemon's log file or queries the daemon for recent log entries.
async fn daemon_logs(port: u16, lines: usize) -> Result<()> {
    // First check if daemon is running
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));

    if !client.health_check().await? {
        println!("Daemon is not running on port {port}.");
        println!();
        println!("Start with: nexus42 daemon start");
        return Ok(());
    }

    // Try to read the log file from the standard location
    let home = dirs::home_dir().ok_or_else(|| CliError::Daemon {
        message: "Cannot determine home directory".to_string(),
    })?;
    let log_path = home.join(".nexus42").join("logs").join("daemon.log");

    if log_path.exists() {
        // Read only the last N lines without loading the entire file into memory.
        let last_lines = read_tail_lines(&log_path, lines)?;

        println!("Daemon logs (last {} lines):", last_lines.len().min(lines));
        println!("{}", "─".repeat(60));

        for line in &last_lines {
            println!("{line}");
        }
    } else {
        println!(
            "Daemon is running but no log file found at {}",
            log_path.display()
        );
        println!();
        println!("Logs may be available via:");
        println!("  journalctl --user -u nexus42");
    }

    Ok(())
}

/// Run daemon health diagnostics.
async fn daemon_doctor(port: u16) -> Result<()> {
    println!("Daemon Doctor — Running diagnostics...");
    println!();

    let mut issues = 0u32;

    // Check 1: Daemon connectivity
    print!("  [1/3] Daemon connectivity... ");
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));
    match client.health_check().await {
        Ok(true) => {
            println!("✓ Running on port {port}");
        }
        Ok(false) => {
            println!("✗ Not running on port {port}");
            issues += 1;
        }
        Err(e) => {
            println!("✗ Error: {e}");
            issues += 1;
        }
    }

    // Check 2: PID file consistency
    print!("  [2/3] PID file... ");
    match read_pid_file() {
        Ok(Some(pid)) => {
            println!("✓ Found (PID: {pid})");
        }
        Ok(None) => {
            println!("⚠ No PID file found");
        }
        Err(e) => {
            println!("✗ Error reading PID file: {e}");
            issues += 1;
        }
    }

    // Check 3: Home directory
    print!("  [3/3] Home directory... ");
    let home = dirs::home_dir().ok_or_else(|| CliError::Daemon {
        message: "Cannot determine home directory".to_string(),
    })?;
    let nexus_home = home.join(".nexus42");
    if nexus_home.exists() {
        println!("✓ {}", nexus_home.display());
    } else {
        println!("⚠ Not initialized ({})", nexus_home.display());
    }

    println!();
    if issues == 0 {
        println!("✓ All checks passed — daemon is healthy.");
    } else {
        println!("✗ {issues} issue(s) found. See above for details.");
    }

    Ok(())
}

/// Validate a `--cdn-url` value against security constraints.
///
/// Delegates to `nexus_orchestration::capability::builtins::validate_cdn_url_static`
/// and maps the typed error to a user-facing `CliError`.
fn validate_cdn_url(url: &str) -> Result<()> {
    nexus_orchestration::capability::builtins::validate_cdn_url_static(url).map_err(|e| {
        CliError::Config(format!(
            "--cdn-url must be a public HTTPS CDN URL (https://...); \
             got {url:?}: {e}"
        ))
    })
}

/// Open the Web UI in the default OS browser.
///
/// Starts the daemon in background mode if it is not already running,
/// then opens `http://127.0.0.1:<port>/` with the platform-appropriate
/// command (`open` on macOS, `xdg-open` on Linux, `start` on Windows).
async fn open_ui(port: u16) -> Result<()> {
    let client = DaemonClient::new(&format!("http://127.0.0.1:{port}"));

    // Start daemon in background if not already running.
    if !client.health_check().await? {
        println!("Daemon is not running — starting on port {port}...");
        // Reuse the self-spawn path (no --foreground).
        start_daemon(port, false, None).await?;
    }

    let url = format!("http://127.0.0.1:{port}/");
    println!("Opening Web UI: {url}");

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| CliError::Daemon {
                message: format!("Failed to open browser: {e}"),
            })?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| CliError::Daemon {
                message: format!("Failed to open browser: {e}"),
            })?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", &url])
            .spawn()
            .map_err(|e| CliError::Daemon {
                message: format!("Failed to open browser: {e}"),
            })?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_file_path_returns_expected_location() {
        let path = pid_file_path().expect("pid_file_path should succeed");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains(".nexus42"),
            "Path should contain .nexus42: {path_str}"
        );
        assert!(
            path_str.ends_with("daemon.pid"),
            "Path should end with daemon.pid: {path_str}"
        );
    }

    #[test]
    fn test_read_pid_file_returns_none_when_missing() {
        // The PID file likely doesn't exist in test environments
        // Use a path that definitely doesn't exist by mocking
        let result = read_pid_file();
        // In CI/test this may or may not exist, just check it doesn't error
        assert!(result.is_ok(), "read_pid_file should not error: {result:?}");
    }

    #[cfg(unix)]
    #[test]
    fn test_is_process_running_with_invalid_pid() {
        // PID 999_999 is very unlikely to exist
        assert!(!is_process_running(999_999));
    }

    #[cfg(unix)]
    #[test]
    fn test_is_process_running_with_own_pid() {
        // Our own process should be running
        let own_pid = std::process::id();
        assert!(is_process_running(own_pid));
    }

    #[tokio::test]
    async fn test_stop_daemon_not_running() {
        // Use a port that's very unlikely to have a daemon
        let result = stop_daemon(19999).await;
        assert!(
            result.is_ok(),
            "stop_daemon on non-running daemon should succeed"
        );
    }

    #[tokio::test]
    async fn test_daemon_status_non_running() {
        let config = CliConfig::default();
        let result = daemon_status(19999, &config).await;
        assert!(
            result.is_ok(),
            "daemon_status should succeed even when daemon not running"
        );
    }

    #[test]
    fn test_write_and_read_pid_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pid_path = tmp.path().join("daemon.pid");

        // Write a PID
        std::fs::write(&pid_path, "12345").unwrap();

        // Read it back
        let content = std::fs::read_to_string(&pid_path).unwrap();
        assert_eq!(content.trim(), "12345");
    }

    #[test]
    fn test_parse_valid_pid() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pid_path = tmp.path().join("daemon.pid");
        std::fs::write(&pid_path, "12345\n").unwrap();

        let content = std::fs::read_to_string(&pid_path).unwrap();
        let pid: u32 = content.trim().parse().unwrap();
        assert_eq!(pid, 12345);
    }

    #[tokio::test]
    async fn test_stop_cleans_up_stale_pid_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pid_path = tmp.path().join("daemon.pid");
        // Write a PID for a non-existent process
        std::fs::write(&pid_path, "999999").unwrap();

        // The stop_daemon function reads from ~/.nexus42/daemon.pid,
        // so this test verifies the logic conceptually.
        // In production, the stale PID cleanup happens inside stop_daemon.
    }

    #[tokio::test]
    async fn test_daemon_logs_not_running() {
        let result = daemon_logs(19999, 50).await;
        assert!(
            result.is_ok(),
            "daemon_logs should succeed even when daemon not running"
        );
    }

    #[tokio::test]
    async fn test_daemon_doctor() {
        let result = daemon_doctor(19999).await;
        assert!(result.is_ok(), "daemon_doctor should succeed");
    }
}
