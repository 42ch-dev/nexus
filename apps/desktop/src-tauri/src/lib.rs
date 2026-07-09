//! Nexus Tauri v2 desktop shell — application entry + native commands.
//!
//! Scope (V1.66 P0): wraps the bundled `apps/web/dist` (served via
//! `build.frontendDist`) and exposes the desktop-only `NexusClient` extensions
//! the browser sandbox cannot perform (compass §5 #1/#8, desktop-shell.md
//! §5/§9). Custom commands include `open_with(path)` (open in the system
//! default editor) and `reveal_in_finder(path)` (reveal in Finder).
//!
//! Both path-guard commands enforce an AUTHORITATIVE runtime path guard
//! (canonicalize + prefix-check against the active workspace root) before
//! delegating to the opener engine. The Tauri capability/opener `scope` is
//! defense-in-depth only — it is static and cannot encode a dynamic workspace
//! root (§5 #8).
//!
//! Daemon lifecycle (sidecar autostart/stop/restart) is owned here via
//! `SidecarManager`. The `.setup()` hook auto-starts the daemon only when
//! `setup_completed` is `true` (existing install); on clean-state first launch
//! the wizard owns the daemon start after `ensure_setup_bootstrap`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

mod connection_config;
mod sidecar;

/// Default workspace root when `workspace_path` is unset in `~/.nexus42/config.toml`.
///
/// Mirrors [`apps/nexus42/src/config.rs::resolve_default_workspace_path`] so the
/// Tauri shell and CLI/daemon agree on first-launch workspace location.
fn default_workspace_root() -> PathBuf {
    dirs::document_dir()
        .or_else(|| {
            eprintln!(
                "nexus-desktop: dirs::document_dir() returned None; falling back to ~/Documents"
            );
            dirs::home_dir().map(|home| home.join("Documents"))
        })
        .unwrap_or_else(|| {
            eprintln!("nexus-desktop: dirs::home_dir() returned None; using relative fallback");
            PathBuf::from("Documents")
        })
        .join("nexus")
        .join("default")
}

/// Path-guard rejection reason surfaced to the JS layer. Serializes as
/// `{ code, message }` so the SPA reads a stable envelope (mirrors the Local
/// API `ErrorResponse` shape). Plain-language copy per design-requirements §6.4
/// + DESIGN.md Desktop Context Menu rules.
#[derive(Debug)]
enum PathGuardError {
    /// Active workspace root is unknown (`~/.nexus42/config.toml` has no
    /// `workspace_path`, or the file is unreadable). The guard cannot make a
    /// safety decision, so it denies by default (fail-closed).
    WorkspaceRootUnknown,
    /// The requested path resolves outside the active workspace root.
    OutsideWorkspace,
    /// The requested path could not be canonicalized (does not exist / IO).
    PathUnresolvable,
}

impl PathGuardError {
    fn code_str(&self) -> &'static str {
        match self {
            Self::WorkspaceRootUnknown => "workspace_root_unknown",
            Self::OutsideWorkspace => "path_outside_workspace",
            Self::PathUnresolvable => "path_unresolvable",
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::WorkspaceRootUnknown => "No active workspace is configured. Set a workspace with the nexus42 CLI before opening files.",
            Self::OutsideWorkspace => "Path not opened. The file is outside the active workspace.",
            Self::PathUnresolvable => "Path not opened. The file could not be found.",
        }
    }
}

impl Serialize for PathGuardError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("PathGuardError", 2)?;
        st.serialize_field("code", self.code_str())?;
        st.serialize_field("message", self.message())?;
        st.end()
    }
}

/// Cached active workspace root, resolved once at startup from
/// `~/.nexus42/config.toml` (`workspace_path`) — the same source of truth the
/// daemon uses at boot (`apps/nexus42/src/config.rs`). `None` when no
/// workspace is configured; every path-guard call then denies by default.
#[derive(Debug, Clone, Default)]
struct WorkspaceRoot(Option<PathBuf>);

/// Resolve the active workspace root exactly as the daemon/CLI do: read
/// `~/.nexus42/config.toml` and return its `workspace_path`.
///
/// If `workspace_path` is unset, this function falls back to
/// `~/Documents/nexus/default/` (cross-platform via `dirs::document_dir()`) and
/// creates the directory if absent. The fallback matches
/// [`apps/nexus42/src/config.rs::resolve_default_workspace_path`].
fn resolve_workspace_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".nexus42").join("config.toml");

    let configured = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| {
            #[derive(serde::Deserialize)]
            struct ConfigFile {
                workspace_path: Option<PathBuf>,
            }
            toml::from_str::<ConfigFile>(&content).ok()
        })
        .and_then(|cfg| cfg.workspace_path)
        .filter(|p| !p.as_os_str().is_empty());

    let root = configured.unwrap_or_else(default_workspace_root);
    if let Err(e) = std::fs::create_dir_all(&root) {
        eprintln!(
            "nexus-desktop: failed to create workspace root {}: {e}",
            root.display()
        );
        // Return the path anyway so the rest of the app can surface the error.
    }
    Some(root)
}

/// Authoritative path guard (compass §5 #8, desktop-shell.md §9).
///
/// Mirrors W-002 in `host_tool_handlers.rs` (~L710-755): canonicalize the
/// requested path AND the workspace root, then require the requested path to
/// lie within the workspace root prefix. Relative paths (the daemon stores
/// `body_path`/`outline_path` workspace-relative, e.g. `Works/<ref>/Stories/…`)
/// are resolved against the workspace root first.
///
/// # TOCTOU note
///
/// There is a small race window between canonicalizing the workspace root and
/// canonicalizing the requested path: a local attacker with filesystem access
/// could replace either path during that window. Per the V1.86 trust-boundary
/// spec, this is "racy-correct" rather than "racy-incorrect" for the
/// single-user local desktop context: the practical risk is bounded by that
/// threat model, while adversarial multi-user FS access is out of scope
/// (`R-V166-QC2-TOCTOU`).
fn guard_path(requested: &str, workspace_root: &WorkspaceRoot) -> Result<PathBuf, PathGuardError> {
    let root = workspace_root
        .0
        .as_ref()
        .ok_or(PathGuardError::WorkspaceRootUnknown)?;
    let root = root
        .canonicalize()
        .map_err(|_| PathGuardError::WorkspaceRootUnknown)?;

    let requested_path = Path::new(requested);
    // Resolve relative paths against the workspace root (the form the Daemon API
    // returns). Absolute paths are taken as-is; the prefix check still applies.
    let candidate = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };

    let canonical = candidate
        .canonicalize()
        .map_err(|_| PathGuardError::PathUnresolvable)?;
    if !canonical.starts_with(&root) {
        return Err(PathGuardError::OutsideWorkspace);
    }
    Ok(canonical)
}

/// `open_with` — open `path` in the system default application after the
/// runtime path guard accepts it. Per desktop-shell.md §5 the contract maps to
/// `plugin-opener.openPath()`; the command calls the underlying `opener` engine
/// (the same crate the plugin wraps) directly so the guard runs first.
#[tauri::command]
fn open_with(path: String, workspace_root: State<'_, WorkspaceRoot>) -> Result<(), PathGuardError> {
    let resolved = guard_path(&path, &workspace_root)?;
    opener::open(&resolved).map_err(|_| PathGuardError::PathUnresolvable)
}

/// `reveal_in_finder` — reveal `path` in Finder after the runtime path guard
/// accepts it. Maps to `plugin-opener.revealItemInDir()`. On macOS the canonical
/// "reveal in Finder" is `open -R <path>` (V1.66 is macOS-only per §5 #10;
/// cross-OS reveal wording/mechanism is V1.67+).
#[tauri::command]
fn reveal_in_finder(
    path: String,
    workspace_root: State<'_, WorkspaceRoot>,
) -> Result<(), PathGuardError> {
    let resolved = guard_path(&path, &workspace_root)?;
    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(&resolved)
        .status()
        .map_err(|_| PathGuardError::PathUnresolvable)?;
    if status.success() {
        Ok(())
    } else {
        Err(PathGuardError::PathUnresolvable)
    }
}

/// `get_workspace_root` — read-only accessor the JS capability layer uses for
/// diagnostics (e.g. surfacing "no active workspace" before a right-click). The
/// authoritative guard still runs in `open_with`/`reveal_in_finder`; this only
/// drives affordance copy.
#[tauri::command]
fn get_workspace_root(workspace_root: State<'_, WorkspaceRoot>) -> Option<String> {
    workspace_root
        .0
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
}

/// `get_daemon_status` — surface the resolved port + lifecycle state to the SPA.
#[tauri::command]
async fn get_daemon_status(
    manager: State<'_, sidecar::SidecarManager>,
) -> Result<sidecar::DaemonStatus, String> {
    Ok(manager.status().await)
}

/// `start_daemon` — manual (re)start of the owned sidecar.
#[tauri::command]
async fn start_daemon(
    manager: State<'_, sidecar::SidecarManager>,
    app: AppHandle,
) -> Result<(), String> {
    manager.start_daemon(&app).await
}

/// `stop_daemon` — graceful stop of the owned sidecar.
#[tauri::command]
async fn stop_daemon(manager: State<'_, sidecar::SidecarManager>) -> Result<(), String> {
    manager.stop().await
}

/// Read `~/.nexus42/config.toml` and return the `setup_completed` marker.
/// Missing field is treated as `false` (first-launch semantics).
#[tauri::command]
fn get_setup_completed() -> bool {
    read_setup_completed().unwrap_or(false)
}

/// Write `setup_completed` to `~/.nexus42/config.toml`.
#[tauri::command]
fn set_setup_completed(value: bool) -> Result<(), String> {
    write_setup_completed(value).map_err(|e| format!("failed to write setup_completed: {e}"))
}

fn nexus_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".nexus42").join("config.toml"))
}

fn read_setup_completed() -> Option<bool> {
    let path = nexus_config_path()?;
    read_setup_completed_at(&path).ok()?
}

fn read_setup_completed_at(path: &Path) -> anyhow::Result<Option<bool>> {
    let content = std::fs::read_to_string(path)?;
    #[derive(serde::Deserialize, Default)]
    struct ConfigFile {
        #[serde(default)]
        setup_completed: Option<bool>,
    }
    Ok(toml::from_str::<ConfigFile>(&content)?.setup_completed)
}

fn write_setup_completed(value: bool) -> anyhow::Result<()> {
    let path =
        nexus_config_path().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    write_setup_completed_at(&path, value)
}

fn write_setup_completed_at(path: &Path, value: bool) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Preserve existing keys by round-tripping through a toml edit document.
    // TOML parse failures are propagated rather than falling back to an empty
    // document, which would silently wipe persisted configuration on a
    // partially-written or corrupt config file (greploop #3 P1).
    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    doc["setup_completed"] = toml_edit::value(value);
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

/// Path to the agent-host configuration file.
///
/// `$HOME/.nexus42/agent-host/config.toml`
fn agent_profile_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".nexus42").join("agent-host").join("config.toml"))
}

/// Write the selected agent profile to `~/.nexus42/agent-host/config.toml`.
///
/// The profile is stored as a `native_cli` provider entry so the agent host
/// subsystem can use it on the next daemon start. Existing provider entries with
/// the same `id` are updated in place; other keys in the file are preserved.
fn write_agent_profile(name: String, launch_command: Option<String>) -> anyhow::Result<()> {
    let path = agent_profile_config_path()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    write_agent_profile_at(&path, &name, launch_command.as_deref())
}

fn write_agent_profile_at(
    path: &Path,
    name: &str,
    launch_command: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Preserve existing keys by round-tripping through a toml edit document.
    // TOML parse failures are propagated rather than falling back to an empty
    // document, which would silently wipe persisted configuration on a
    // partially-written or corrupt config file (greploop #3 P1).
    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    if doc.get("providers").is_none() {
        doc["providers"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }

    let providers = doc["providers"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("providers is not an array of tables"))?;

    let mut updated = false;
    for provider in providers.iter_mut() {
        if provider.get("id").and_then(|v| v.as_str()) == Some(name) {
            if let Some(cmd) = launch_command {
                provider["command"] = toml_edit::value(cmd);
            } else {
                provider.remove("command");
            }
            updated = true;
            break;
        }
    }

    if !updated {
        let mut provider = toml_edit::Table::new();
        provider["id"] = toml_edit::value(name);
        provider["protocol"] = toml_edit::value("native_cli");
        if let Some(cmd) = launch_command {
            provider["command"] = toml_edit::value(cmd);
        }
        providers.push(provider);
    }

    std::fs::write(path, doc.to_string())?;
    Ok(())
}

/// Persist the agent selected during setup wizard step 3.
#[tauri::command]
fn set_agent_profile(name: String, launch_command: Option<String>) -> Result<(), String> {
    write_agent_profile(name, launch_command)
        .map_err(|e| format!("failed to write agent profile: {e}"))
}

/// Saved agent profile returned by `get_agent_profile` (Settings preselect).
///
/// Serializes with camelCase so the SPA reads `{ name, launchCommand? }` without
/// a second mapping layer. Missing/`null` means "no usable saved profile".
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AgentProfile {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_command: Option<String>,
}

/// Read the first `native_cli` provider from `~/.nexus42/agent-host/config.toml`.
///
/// Preselect path: missing file, empty providers, no `native_cli` entry, or TOML
/// parse/IO failure all return `None` (never an error) so Settings can fall back
/// without crashing.
#[tauri::command]
fn get_agent_profile() -> Option<AgentProfile> {
    let path = agent_profile_config_path()?;
    read_agent_profile_at(&path)
}

fn read_agent_profile_at(path: &Path) -> Option<AgentProfile> {
    if !path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let providers = doc.get("providers")?.as_array_of_tables()?;
    for provider in providers.iter() {
        let protocol = provider.get("protocol").and_then(|v| v.as_str());
        if protocol != Some("native_cli") {
            continue;
        }
        let name = provider.get("id").and_then(|v| v.as_str())?;
        if name.is_empty() {
            continue;
        }
        let launch_command = provider
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        return Some(AgentProfile {
            name: name.to_owned(),
            launch_command,
        });
    }
    None
}

/// Wipe the daemon's local state DB(s) under `~/.nexus42/creators/*/workspaces/*/`,
/// plus their SQLite WAL/SHM siblings.
///
/// This is a glob-only reset: it covers the setup-wizard scenario (no active
/// creator yet) by deleting every `state.db` under each creator/workspace. Only
/// files exactly named `state.db`, `state.db-wal`, or `state.db-shm` are removed;
/// the user workspace (`~/Documents/nexus/...`) is never touched.
#[tauri::command]
fn reset_local_database() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("cannot determine home directory")?;
    reset_local_database_at(&home)
        .map(|_| ())
        .map_err(|e| format!("failed to reset local database: {e}"))
}

fn reset_local_database_at(home: &Path) -> std::io::Result<usize> {
    let creators_dir = home.join(".nexus42").join("creators");
    if !creators_dir.is_dir() {
        return Ok(0);
    }

    let mut wiped = 0;
    for creator in std::fs::read_dir(creators_dir)? {
        let creator = creator?;
        if !creator.file_type()?.is_dir() {
            continue;
        }
        let workspaces_dir = creator.path().join("workspaces");
        if !workspaces_dir.is_dir() {
            continue;
        }
        for workspace in std::fs::read_dir(workspaces_dir)? {
            let workspace = workspace?;
            if !workspace.file_type()?.is_dir() {
                continue;
            }
            for name in ["state.db", "state.db-wal", "state.db-shm"] {
                let path = workspace.path().join(name);
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        if name == "state.db" {
                            wiped += 1;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(wiped)
}

/// Open a native directory picker and return the selected path, or `None` if the
/// user cancelled. The `default_path` is used as the starting directory.
///
/// Uses the async callback API so the tokio runtime keeps processing events
/// (e.g. daemon status updates) while the native modal is open.
#[tauri::command]
async fn pick_directory(app: AppHandle, default_path: String) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_directory(&default_path)
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let picked = rx
        .await
        .map_err(|e| format!("dialog result channel closed: {e}"))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("invalid directory path: {e}"))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

/// Write `workspace_path` to `~/.nexus42/config.toml`, preserving other keys.
#[tauri::command]
fn set_workspace_path(path: String) -> Result<(), String> {
    let config_path = nexus_config_path().ok_or("cannot determine home directory")?;
    write_workspace_path_at(&config_path, &path)
        .map_err(|e| format!("failed to write workspace_path: {e}"))
}

fn write_workspace_path_at(path: &Path, value: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Preserve existing keys by round-tripping through a toml edit document.
    // TOML parse failures are propagated rather than falling back to an empty
    // document, which would silently wipe persisted configuration on a
    // partially-written or corrupt config file.
    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    doc["workspace_path"] = toml_edit::value(value);
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

// ── Setup bootstrap (V1.100 P0) ────────────────────────────────────────

/// Returned by `ensure_setup_bootstrap` to inform the wizard whether bootstrap
/// created a new creator ID or detected an already-bootstrapped state.
#[derive(Debug, Clone, Serialize)]
struct BootstrapResult {
    creator_id: String,
    already_bootstrapped: bool,
}

/// Generate a `ctr_local` + 12 random hex chars creator ID, matching the
/// generation pattern in `nexus-creator/src/local_identity.rs:214-221`.
fn generate_local_creator_id() -> String {
    let random: String = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(12)
        .collect();
    format!("ctr_local{random}")
}

/// Read the current bootstrap state from a config.toml path.
///
/// Returns `(active_creator_id, active_workspace_slug)` — both `Option`.
fn read_bootstrap_state(path: &Path) -> anyhow::Result<(Option<String>, Option<String>)> {
    if !path.exists() {
        return Ok((None, None));
    }
    let content = std::fs::read_to_string(path)?;
    #[derive(serde::Deserialize, Default)]
    struct ConfigFile {
        #[serde(default)]
        active_creator_id: Option<String>,
        #[serde(default)]
        active_workspace_slug_by_creator: Option<std::collections::HashMap<String, String>>,
    }
    let cfg = toml::from_str::<ConfigFile>(&content)?;
    let slug = cfg
        .active_workspace_slug_by_creator
        .as_ref()
        .and_then(|map| cfg.active_creator_id.as_ref().and_then(|id| map.get(id).cloned()));
    Ok((cfg.active_creator_id, slug))
}

/// Idempotent creator/workspace bootstrap to `~/.nexus42/config.toml`.
///
/// On first call (no `active_creator_id` in config):
///   - generates a new `ctr_local*` creator ID
///   - writes `active_creator_id` + `active_workspace_slug_by_creator` to config.toml
///   - returns `already_bootstrapped: false`
///
/// On subsequent calls (creator ID already present):
///   - returns the existing `creator_id` with `already_bootstrapped: true`
///   - never overwrites an existing `active_creator_id`
fn ensure_setup_bootstrap_at(path: &Path) -> anyhow::Result<BootstrapResult> {
    let (existing_id, _slug) = read_bootstrap_state(path)?;

    if let Some(creator_id) = existing_id {
        return Ok(BootstrapResult {
            creator_id,
            already_bootstrapped: true,
        });
    }

    // Generate new creator ID.
    let creator_id = generate_local_creator_id();

    // Write to config.toml, preserving existing keys via toml_edit round-trip.
    // TOML parse failures are propagated rather than silently wiping the file.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    doc["active_creator_id"] = toml_edit::value(&creator_id);

    // Write the [active_workspace_slug_by_creator] table with the new creator.
    if doc.get("active_workspace_slug_by_creator").is_none() {
        doc["active_workspace_slug_by_creator"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["active_workspace_slug_by_creator"][&creator_id] = toml_edit::value("default");

    std::fs::write(path, doc.to_string())?;

    Ok(BootstrapResult {
        creator_id,
        already_bootstrapped: false,
    })
}

/// Desktop-only Tauri IPC command: bootstrap the minimum creator/workspace
/// state (`active_creator_id` + `active_workspace_slug_by_creator` in
/// `~/.nexus42/config.toml`) so the daemon can start without "No active
/// creator". Idempotent — if a creator ID already exists, returns it without
/// overwriting.
///
/// See `.mstar/iterations/v1.100/specs/desktop-first-launch-bootstrap.md`.
#[tauri::command]
fn ensure_setup_bootstrap() -> Result<BootstrapResult, String> {
    let config_path = nexus_config_path().ok_or("cannot determine home directory")?;
    ensure_setup_bootstrap_at(&config_path).map_err(|e| format!("bootstrap failed: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The workspace root is captured once at startup and stored as managed
    // state for the lifetime of the app. If the user changes the active
    // workspace while the app is running (e.g. `nexus42 config set
    // workspace_path ...`), the new root does not take effect in the desktop
    // context-menu path guard until the app is restarted. This is intentional
    // for V1.66; live refresh of the workspace root is V1.67+ scope.
    let workspace_root = WorkspaceRoot(resolve_workspace_root());
    let port = sidecar::resolve_port();
    let sidecar_manager = sidecar::SidecarManager::new(port);
    let setup_manager = sidecar_manager.clone();

    // Inject the Rust-resolved daemon port into the webview before any page
    // loads. This makes `window.__NEXUS_DAEMON_PORT__` the authoritative source
    // for `TauriClient`; `process.env` is undefined in the Tauri webview so the
    // SPA would otherwise always fall back to 8420 (daemon-runtime.md §12.3).
    let port_script = format!("window.__NEXUS_DAEMON_PORT__ = {port};");
    let port_plugin = tauri::plugin::Builder::<tauri::Wry>::new("nexus-desktop-port")
        .js_init_script(port_script)
        .build();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Dialog plugin provides the native folder picker via Rust extension traits.
        .plugin(tauri_plugin_dialog::init())
        // Shell plugin drives the bundled `nexus42` sidecar via
        // `tauri_plugin_shell::ShellExt::sidecar` (P1).
        .plugin(tauri_plugin_shell::init())
        .plugin(port_plugin)
        .manage(workspace_root)
        .manage(sidecar_manager.clone())
        .setup(move |app| {
            setup_manager.set_app_handle(app.handle().clone());
            // Gate daemon auto-start behind setup_completed.
            // - true (existing install): preserve current auto-start/attach behavior.
            // - false / absent (clean-state): no-op; wizard owns daemon start after bootstrap.
            if read_setup_completed().unwrap_or(false) {
                let manager = setup_manager.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = manager.start(&handle).await {
                        eprintln!("nexus-desktop: sidecar failed to start: {e}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_with,
            reveal_in_finder,
            get_workspace_root,
            get_daemon_status,
            start_daemon,
            stop_daemon,
            reset_local_database,
            pick_directory,
            set_workspace_path,
            get_setup_completed,
            set_setup_completed,
            set_agent_profile,
            get_agent_profile,
            ensure_setup_bootstrap,
            connection_config::get_connection_config,
            connection_config::set_connection_config,
            connection_config::delete_connection_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Nexus desktop shell")
        // Tauri v2 idiomatic app-lifecycle hook: `RunEvent::ExitRequested` runs
        // before the async runtime shuts down, so we can gracefully stop the
        // owned sidecar. The previous "trailing" cleanup pattern that ran after
        // `run()` returned raced with tokio teardown (qc1 S-5).
        .run(move |_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let _ = tauri::async_runtime::block_on(sidecar_manager.stop());
            }
        });
}

#[cfg(test)]
mod tests {
    //! Path-guard unit tests (compass §5 #8, desktop-shell.md §9).
    //!
    //! `guard_path` is the AUTHORITATIVE security check; it must mirror W-002 in
    //! `host_tool_handlers.rs`. These tests pin the accept/reject contract
    //! against a temp workspace root, incl. the workspace-relative form the
    //! daemon actually stores (`Works/<ref>/Stories/…`) and traversal attempts.

    use super::{
        default_workspace_root, guard_path, read_agent_profile_at, read_setup_completed_at,
        reset_local_database_at, write_agent_profile_at, write_setup_completed_at, AgentProfile,
        PathGuardError, WorkspaceRoot,
    };
    use super::{
        ensure_setup_bootstrap_at, generate_local_creator_id, read_bootstrap_state,
        write_workspace_path_at,
    };
    use std::fs;
    use std::path::PathBuf;

    /// Build a temp workspace root with a realistic chapter file layout and
    /// return the canonical root (what `resolve_workspace_root` would yield).
    fn temp_workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonicalize root");
        let chapter = root.join("Works").join("WRK").join("Stories");
        fs::create_dir_all(&chapter).expect("mkdir chapter");
        fs::write(chapter.join("ch01-ch01.md"), "# body").expect("write body");
        fs::create_dir_all(
            root.join("Works")
                .join("WRK")
                .join("Outlines")
                .join("chapters"),
        )
        .expect("mkdir outline");
        fs::write(
            root.join("Works")
                .join("WRK")
                .join("Outlines")
                .join("chapters")
                .join("ch01-outline.md"),
            "# outline",
        )
        .expect("write outline");
        (dir, root)
    }

    #[test]
    fn accepts_workspace_relative_body_path() {
        let (_live, root) = temp_workspace();
        let ws = WorkspaceRoot(Some(root.clone()));
        // The daemon stores body_path workspace-relative.
        let resolved =
            guard_path("Works/WRK/Stories/ch01-ch01.md", &ws).expect("relative body path accepted");
        assert!(resolved.starts_with(&root));
        assert!(resolved.ends_with("Stories/ch01-ch01.md"));
    }

    #[test]
    fn accepts_absolute_path_inside_workspace() {
        let (_live, root) = temp_workspace();
        let ws = WorkspaceRoot(Some(root.clone()));
        let abs = root
            .join("Works")
            .join("WRK")
            .join("Outlines")
            .join("chapters")
            .join("ch01-outline.md");
        let resolved =
            guard_path(abs.to_str().unwrap(), &ws).expect("absolute in-workspace accepted");
        assert!(resolved.ends_with("ch01-outline.md"));
    }

    #[test]
    fn rejects_traversal_outside_workspace_root() {
        let (_live, root) = temp_workspace();
        let ws = WorkspaceRoot(Some(root));
        // /etc/passwd is canonical and exists, but is outside the workspace.
        // Guard against a workspace that happens to live under /etc by using a
        // sibling temp path we know is outside the root.
        let outside = tempfile::tempdir().expect("outside temp");
        let outside_file = outside.path().join("secret.md");
        fs::write(&outside_file, "x").expect("write outside");
        let err = guard_path(outside_file.to_str().unwrap(), &ws).expect_err("outside rejected");
        assert!(matches!(err, PathGuardError::OutsideWorkspace));
        assert_eq!(err.code_str(), "path_outside_workspace");
    }

    #[test]
    fn rejects_relative_traversal_escape() {
        let (_live, root) = temp_workspace();
        let ws = WorkspaceRoot(Some(root));
        // Classic traversal: the joined candidate escapes the workspace. Because
        // the target does not exist under the workspace, canonicalize fails →
        // PathUnresolvable (the guard never grants access to a non-existent path
        // outside the resolved tree).
        let err = guard_path("../../etc/hosts", &ws).expect_err("traversal rejected");
        // /etc/hosts usually exists → OutsideWorkspace; if not, PathUnresolvable.
        // Either way, the guard must DENY — never accept.
        assert!(matches!(
            err,
            PathGuardError::OutsideWorkspace | PathGuardError::PathUnresolvable
        ));
    }

    #[test]
    fn denies_by_default_when_workspace_root_is_unknown() {
        // Fail-closed: no active workspace configured → deny everything.
        let ws = WorkspaceRoot(None);
        let err =
            guard_path("Works/WRK/Stories/ch01-ch01.md", &ws).expect_err("unknown root denied");
        assert!(matches!(err, PathGuardError::WorkspaceRootUnknown));
        assert_eq!(err.code_str(), "workspace_root_unknown");
    }

    #[test]
    fn rejects_nonexistent_path_inside_workspace() {
        let (_live, root) = temp_workspace();
        let ws = WorkspaceRoot(Some(root));
        let err =
            guard_path("Works/WRK/Stories/does-not-exist.md", &ws).expect_err("nonexistent denied");
        assert!(matches!(err, PathGuardError::PathUnresolvable));
    }

    #[test]
    fn setup_completed_roundtrips_through_config_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        write_setup_completed_at(&config_path, true).expect("write true");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(true));

        write_setup_completed_at(&config_path, false).expect("write false");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(false));
    }

    #[test]
    fn setup_completed_write_preserves_existing_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "workspace_path = \"/existing/workspace\"\nruntime_mode = \"local_only\"\n",
        )
        .expect("write initial config");

        write_setup_completed_at(&config_path, true).expect("write setup_completed");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("workspace_path = \"/existing/workspace\""));
        assert!(text.contains("runtime_mode = \"local_only\""));
        assert!(text.contains("setup_completed = true"));
    }

    #[test]
    fn setup_completed_write_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        let original = "workspace_path = \"/existing/workspace\"\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = write_setup_completed_at(&config_path, true);
        assert!(result.is_err(), "malformed TOML should be rejected");

        // The corrupt file must NOT be overwritten with a single-key document.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("workspace_path = \"/existing/workspace\""),
            "existing keys must survive a failed write"
        );
        assert!(
            !text.contains("setup_completed"),
            "setup_completed must not be written on parse failure"
        );
    }

    #[test]
    fn default_workspace_root_ends_with_nexus_default() {
        let path = default_workspace_root();
        let s = path.to_string_lossy();
        assert!(
            s.ends_with("nexus/default") || s.ends_with("nexus\\default"),
            "default workspace root should end with nexus/default, got: {s}"
        );
    }

    #[test]
    fn agent_profile_roundtrips_through_config_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");

        write_agent_profile_at(&config_path, "codex-acp", Some("codex")).expect("write profile");
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("id = \"codex-acp\""));
        assert!(text.contains("protocol = \"native_cli\""));
        assert!(text.contains("command = \"codex\""));

        // Update same id preserves array length (one provider).
        write_agent_profile_at(&config_path, "codex-acp", Some("codex --verbose"))
            .expect("update profile");
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("command = \"codex --verbose\""));
        // Only one provider table should be present.
        assert_eq!(text.matches("id = \"codex-acp\"").count(), 1);
    }

    #[test]
    fn agent_profile_write_preserves_existing_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir");
        std::fs::write(&config_path, "max_sessions = 2\n").expect("write initial config");

        write_agent_profile_at(&config_path, "claude-cli", Some("claude")).expect("write profile");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("max_sessions = 2"));
        assert!(text.contains("id = \"claude-cli\""));
        assert!(text.contains("protocol = \"native_cli\""));
        assert!(text.contains("command = \"claude\""));
    }

    #[test]
    fn agent_profile_write_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir");
        let original = "max_sessions = 2\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = write_agent_profile_at(&config_path, "claude-cli", Some("claude"));
        assert!(result.is_err(), "malformed TOML should be rejected");

        // The corrupt file must NOT be overwritten with a single-provider document.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("max_sessions = 2"),
            "existing keys must survive a failed write"
        );
        assert!(
            !text.contains("claude-cli"),
            "agent profile must not be written on parse failure"
        );
    }

    #[test]
    fn agent_profile_read_returns_first_native_cli_provider() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");
        write_agent_profile_at(&config_path, "codex-acp", Some("codex")).expect("write profile");

        let profile = read_agent_profile_at(&config_path);
        assert_eq!(
            profile,
            Some(AgentProfile {
                name: "codex-acp".to_owned(),
                launch_command: Some("codex".to_owned()),
            })
        );
    }

    #[test]
    fn agent_profile_read_skips_non_native_cli_providers() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir");
        std::fs::write(
            &config_path,
            r#"
[[providers]]
id = "http-agent"
protocol = "http"

[[providers]]
id = "claude-cli"
protocol = "native_cli"
command = "claude"
"#,
        )
        .expect("write config");

        let profile = read_agent_profile_at(&config_path);
        assert_eq!(
            profile,
            Some(AgentProfile {
                name: "claude-cli".to_owned(),
                launch_command: Some("claude".to_owned()),
            })
        );
    }

    #[test]
    fn agent_profile_read_returns_none_when_missing_or_malformed() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let missing = tmp.path().join("missing.toml");
        assert_eq!(read_agent_profile_at(&missing), None);

        let empty = tmp.path().join("empty.toml");
        std::fs::write(&empty, "max_sessions = 2\n").expect("write empty providers");
        assert_eq!(read_agent_profile_at(&empty), None);

        let malformed = tmp.path().join("malformed.toml");
        std::fs::write(&malformed, "max_sessions = 2\nmalformed = \"unclosed\n")
            .expect("write malformed");
        assert_eq!(read_agent_profile_at(&malformed), None);
    }

    #[test]
    fn reset_local_database_wipes_only_state_db_under_nexus42() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let home = tmp.path();

        // Simulate the ADR-014 layout: ~/.nexus42/creators/<id>/workspaces/<slug>/state.db
        let db_dir = home
            .join(".nexus42")
            .join("creators")
            .join("creator-1")
            .join("workspaces")
            .join("default");
        std::fs::create_dir_all(&db_dir).expect("mkdir db dir");
        std::fs::write(db_dir.join("state.db"), "db").expect("write state.db");
        std::fs::write(db_dir.join("state.db-wal"), "wal").expect("write wal");
        std::fs::write(db_dir.join("state.db-shm"), "shm").expect("write shm");

        // A file outside the ~/.nexus42 tree must be untouched (e.g. user workspace).
        let user_workspace = home.join("Documents").join("nexus").join("default");
        std::fs::create_dir_all(&user_workspace).expect("mkdir user workspace");
        let user_file = user_workspace.join("creative.md");
        std::fs::write(&user_file, "creative").expect("write creative file");

        let wiped = reset_local_database_at(home).expect("reset should succeed");
        assert_eq!(wiped, 1, "one main state.db should be wiped");

        assert!(
            !db_dir.join("state.db").exists(),
            "state.db should be deleted"
        );
        assert!(
            !db_dir.join("state.db-wal").exists(),
            "state.db-wal should be deleted"
        );
        assert!(
            !db_dir.join("state.db-shm").exists(),
            "state.db-shm should be deleted"
        );
        assert!(user_file.exists(), "user workspace files must be untouched");
    }

    // ── V1.100 P0: setup bootstrap tests ────────────────────────────────
    // Pins three lifecycle branches per
    // `.mstar/iterations/v1.100/specs/desktop-first-launch-bootstrap.md`
    // § Verification Strategy:
    //   1. setup_completed=false → .setup() does NOT auto-start sidecar
    //   2. setup_completed=true  → preserves auto-start/attach behavior
    //   3. ensure_setup_bootstrap idempotency
    //   4. Bootstrap failure: config write failure → no partial/corrupt state
    //
    // Branch 1 + 2 are tested via the read_setup_completed boolean logic
    // that gates .setup() (the Tauri closure itself is integration-tested
    // via interactive smoke, T4).

    #[test]
    fn setup_completed_absent_means_no_auto_start() {
        // Clean-state: no config file → read_setup_completed returns None
        // → .setup() treats as false → no sidecar spawn.
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        // No config file exists → read returns Err (which .setup() handles
        // via unwrap_or(false) → effectively false → no sidecar spawn).
        assert!(read_setup_completed_at(&config_path).is_err());

        // Write setup_completed = false explicitly — same gating semantics.
        write_setup_completed_at(&config_path, false).expect("write false");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(false));
    }

    #[test]
    fn setup_completed_true_preserves_auto_start_behavior() {
        // Existing install: setup_completed = true → .setup() auto-starts
        // the sidecar (current behavior, byte-for-byte preserved).
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        write_setup_completed_at(&config_path, true).expect("write true");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(true));
    }

    #[test]
    fn bootstrap_creates_creator_id_on_first_call() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        let result =
            ensure_setup_bootstrap_at(&config_path).expect("first bootstrap should succeed");
        assert!(!result.already_bootstrapped);
        assert!(result.creator_id.starts_with("ctr_local"));
        assert_eq!(result.creator_id.len(), 21); // "ctr_local" + 12 hex

        // Config must contain both keys.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("active_creator_id"));
        assert!(text.contains(&result.creator_id));
        assert!(text.contains("active_workspace_slug_by_creator"));
        assert!(text.contains("default"));
    }

    #[test]
    fn bootstrap_is_idempotent() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        let first =
            ensure_setup_bootstrap_at(&config_path).expect("first bootstrap should succeed");
        assert!(!first.already_bootstrapped);

        let second =
            ensure_setup_bootstrap_at(&config_path).expect("second bootstrap should succeed");
        assert!(second.already_bootstrapped);
        assert_eq!(second.creator_id, first.creator_id);

        // Verify the config still has exactly one creator_id.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        let count = text.matches("active_creator_id").count();
        assert_eq!(count, 1, "should have exactly one active_creator_id key");
    }

    #[test]
    fn bootstrap_preserves_existing_config_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        // Pre-populate with keys written by wizard step 1 (set_workspace_path).
        write_workspace_path_at(&config_path, "/tmp/test-workspace")
            .expect("write workspace_path");
        write_setup_completed_at(&config_path, false).expect("write setup_completed");

        let result =
            ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");
        assert!(!result.already_bootstrapped);

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("workspace_path = \"/tmp/test-workspace\""),
            "workspace_path must survive bootstrap"
        );
        assert!(text.contains("active_creator_id"));
        assert!(text.contains("active_workspace_slug_by_creator"));
    }

    #[test]
    fn bootstrap_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        let original = "workspace_path = \"/existing/workspace\"\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = ensure_setup_bootstrap_at(&config_path);
        assert!(result.is_err(), "malformed TOML must be rejected");

        // The corrupt file must NOT be overwritten.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("workspace_path = \"/existing/workspace\""),
            "existing keys must survive a failed bootstrap"
        );
        assert!(
            !text.contains("active_creator_id"),
            "bootstrap must not write on parse failure"
        );
    }

    #[test]
    fn bootstrap_reads_existing_creator_id_as_already_bootstrapped() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        // Simulate an already-bootstrapped config with a creator ID.
        let (creator_id, _slug) = read_bootstrap_state(&config_path).expect("read state");
        assert!(creator_id.is_none(), "clean config has no creator");

        // Write a pre-existing creator ID directly (simulating a previous bootstrap).
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\n",
        )
        .expect("write pre-bootstrapped config");

        let (creator_id, _slug) = read_bootstrap_state(&config_path).expect("read state");
        assert_eq!(creator_id.as_deref(), Some("ctr_localABCD1234EF56"));

        // ensure_setup_bootstrap must detect and return already_bootstrapped.
        let result =
            ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");
        assert!(result.already_bootstrapped);
        assert_eq!(result.creator_id, "ctr_localABCD1234EF56");
    }

    #[test]
    fn generate_local_creator_id_matches_pattern() {
        // Verify the generation matches the `ctr_local` + 12 hex chars contract.
        for _ in 0..20 {
            let id = generate_local_creator_id();
            assert!(
                id.starts_with("ctr_local"),
                "ID '{id}' should start with ctr_local"
            );
            assert_eq!(id.len(), 21, "ID '{id}': ctr_local + 12 hex chars = 21");
            let hex_part = &id[9..]; // strip "ctr_local"
            assert!(
                hex_part.chars().all(|c| c.is_ascii_hexdigit()),
                "ID '{id}' hex part should be all hex digits"
            );
        }
    }

    #[test]
    fn bootstrap_writes_active_workspace_slug_table() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        let result =
            ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");

        let (creator_id, slug) = read_bootstrap_state(&config_path).expect("read state");
        assert_eq!(creator_id.as_deref(), Some(result.creator_id.as_str()));
        assert_eq!(slug.as_deref(), Some("default"));
    }
}
