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
//! `SidecarManager`. The `.setup()` hook **always** auto-starts/attaches the
//! daemon on every launch (V1.105 D2 — rewrites V1.100 Rule 13). The
//! `setup_completed` marker no longer gates sidecar start; the web shell's
//! outer `DaemonLaunchGate` waits for Ready, then `SetupGate` routes `/setup`
//! vs main UI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

mod connection_config;
mod sidecar;

/// Set when the user has confirmed quit so the second `ExitRequested` (from
/// `app.exit`) does not re-open the confirmation dialog.
static EXIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

/// Set while the quit confirmation dialog is open so duplicate
/// `ExitRequested` events cannot spawn a second conflicting dialog.
static EXIT_PROMPT_ACTIVE: AtomicBool = AtomicBool::new(false);

const QUIT_STOP_LABEL: &str = "Stop Daemon & Quit";
const QUIT_KEEP_LABEL: &str = "Keep Daemon & Quit";
const QUIT_CANCEL_LABEL: &str = "Cancel";

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

/// Crash-safe file write: write to a sibling temp file on the same filesystem,
/// then rename it into place. `rename` is atomic on POSIX and avoids leaving a
/// truncated file if the process is killed mid-write.
fn atomic_write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Non-panicking path-traversal validation for `creator_id`.
///
/// Mirrors [`nexus_home_layout::validate_creator_id_safe`] so the Tauri switch
/// path and the daemon handlers enforce the same rules without adding a
/// cross-workspace dependency to the standalone desktop crate.
fn validate_creator_id_safe(id: &str) -> Result<(), String> {
    if id.chars().any(|ch| ch == '/' || ch == '\\') {
        return Err(format!(
            "creator_id contains path separator: {id:?} — rejected for safety"
        ));
    }
    if id.contains("..") {
        return Err(format!(
            "creator_id contains '..': {id:?} — rejected for safety"
        ));
    }
    if id.chars().any(char::is_control) {
        return Err(format!(
            "creator_id contains control characters: {id:?} — rejected for safety"
        ));
    }
    Ok(())
}

/// Cached active workspace root, resolved once at startup from
/// `~/.nexus42/config.toml` (`workspace_path`) — the same source of truth the
/// daemon uses at boot (`apps/nexus42/src/config.rs`). `None` when no
/// workspace is configured; every path-guard call then denies by default.
#[derive(Debug, Clone, Default)]
struct WorkspaceRoot(Option<PathBuf>);

/// Per-creator workspace path configuration read from `~/.nexus42/config.toml`.
#[derive(serde::Deserialize, Default)]
struct WorkspaceConfig {
    #[serde(default)]
    active_creator_id: Option<String>,
    #[serde(default)]
    workspace_path: Option<PathBuf>,
    #[serde(default)]
    workspace_path_by_creator: Option<HashMap<String, String>>,
}

/// Read workspace-related configuration from a config.toml path.
fn read_workspace_config_at(path: &Path) -> anyhow::Result<WorkspaceConfig> {
    if !path.exists() {
        return Ok(WorkspaceConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(toml::from_str::<WorkspaceConfig>(&content)?)
}

/// One-shot legacy migration: if `[workspace_path_by_creator]` is missing or
/// empty and a legacy `workspace_path` is set, copy it into the active creator's
/// map entry so the Default Profile keeps the existing folder (AD-P0-2).
fn migrate_legacy_workspace_path_at(
    path: &Path,
    creator_id: &str,
    legacy_path: &Path,
) -> anyhow::Result<()> {
    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    if doc.get("workspace_path_by_creator").is_none() {
        doc["workspace_path_by_creator"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let legacy_str = legacy_path.to_string_lossy().to_string();
    doc["workspace_path_by_creator"][creator_id] = toml_edit::value(&legacy_str);
    // Keep the legacy mirror intact; do not remove it in V1.117 (AD-P0-2).
    atomic_write(path, &doc.to_string())?;
    Ok(())
}

/// Resolve the active workspace root for a given home directory.
///
/// Read order (AD-P0-1): `[workspace_path_by_creator][active_creator_id]` →
/// fallback `workspace_path` → default home path (`~/Documents/nexus/default`).
/// On first access after upgrade, a legacy single `workspace_path` is migrated
/// into the active creator's map entry (AD-P0-2).
fn resolve_workspace_root_at(home: &Path, default_root: &Path) -> Option<PathBuf> {
    let config_path = home.join(".nexus42").join("config.toml");

    let mut config = read_workspace_config_at(&config_path).ok()?;

    // One-shot migration: map missing/empty + legacy path set → copy to active creator.
    let map_missing_or_empty = config
        .workspace_path_by_creator
        .as_ref()
        .map(HashMap::is_empty)
        .unwrap_or(true);
    if map_missing_or_empty {
        if let (Some(creator_id), Some(legacy_path)) = (
            config.active_creator_id.as_deref(),
            config.workspace_path.as_ref(),
        ) {
            if let Err(e) = migrate_legacy_workspace_path_at(&config_path, creator_id, legacy_path)
            {
                eprintln!("nexus-desktop: failed to migrate legacy workspace_path: {e}");
            }
            config = read_workspace_config_at(&config_path).ok()?;
        }
    }

    let resolved = config
        .active_creator_id
        .as_ref()
        .and_then(|id| {
            config
                .workspace_path_by_creator
                .as_ref()
                .and_then(|map| map.get(id))
                .map(PathBuf::from)
        })
        .or(config.workspace_path)
        .unwrap_or_else(|| default_root.to_path_buf());

    if let Err(e) = std::fs::create_dir_all(&resolved) {
        eprintln!(
            "nexus-desktop: failed to create workspace root {}: {e}",
            resolved.display()
        );
        // Return the path anyway so the rest of the app can surface the error.
    }
    Some(resolved)
}

/// Resolve the active workspace root exactly as the daemon/CLI do: read
/// `~/.nexus42/config.toml` and return the active Profile's path.
///
/// If no workspace path is configured for the active Profile, this function falls
/// back to `~/Documents/nexus/default/` (cross-platform via `dirs::document_dir()`)
/// and creates the directory if absent. The fallback matches
/// [`apps/nexus42/src/config.rs::resolve_default_workspace_path`].
fn resolve_workspace_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    resolve_workspace_root_at(&home, &default_workspace_root())
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

/// Validate URL scheme is `http:` or `https:` only. Rejects `file:`, `javascript:`,
/// etc. Used by `open_external_url` to prevent unsafe URL schemes from reaching the
/// system browser.
fn validate_url_scheme(url: &str) -> Result<(), String> {
    let Some(pos) = url.find("://") else {
        return Err("URL must have a scheme (e.g. http:// or https://)".to_string());
    };
    let scheme = &url[..pos];
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "URL scheme '{scheme}' is not allowed. Only http and https are supported."
        ));
    }
    Ok(())
}

/// `open_external_url` — open a URL in the system default browser after scheme
/// validation (http/https only). Unlike `open_with`, this command has **no**
/// workspace path guard — it is intended for outbound links (Install/Docs URLs),
/// not local files.
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    validate_url_scheme(&url)?;
    opener::open(&url).map_err(|e| format!("failed to open URL: {e}"))
}

/// `get_workspace_root` — read-only accessor the JS capability layer uses for
/// diagnostics (e.g. surfacing "no active workspace" before a right-click). The
/// authoritative guard still runs in `open_with`/`reveal_in_finder`; this only
/// drives affordance copy. The returned value is re-resolved from TOML on every
/// call so the SPA sees the latest persisted path (V1.117 QC1-F-006).
#[tauri::command]
fn get_workspace_root() -> Option<String> {
    resolve_workspace_root().map(|p| p.to_string_lossy().to_string())
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

/// `restart_daemon` — atomic restart of the sidecar (owned or attached).
#[tauri::command]
async fn restart_daemon(
    manager: State<'_, sidecar::SidecarManager>,
    app: AppHandle,
) -> Result<(), String> {
    manager.restart_daemon(&app).await
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

/// Default user-layer entrance (AR-15/AR-16) — mirrors `DEFAULT_ENTRANCE` in
/// `apps/web/src/components/layout/entrance-registry.ts`.
const DEFAULT_ENTRANCE: &str = "content-creator";

/// Read `~/.nexus42/config.toml` and return the `entrance` marker.
/// Missing field is treated as `content-creator` (default entrance, AR-16).
#[tauri::command]
fn get_entrance() -> String {
    read_entrance().unwrap_or_else(|| DEFAULT_ENTRANCE.to_string())
}

/// Write `entrance` to `~/.nexus42/config.toml`. Only the two valid
/// `EntranceId` values are accepted (AR-16) — anything else is rejected so a
/// malformed IPC call cannot corrupt the persisted config.
#[tauri::command]
fn set_entrance(value: String) -> Result<(), String> {
    if value != "developer" && value != "content-creator" {
        return Err(format!("invalid entrance value: {value}"));
    }
    write_entrance(&value).map_err(|e| format!("failed to write entrance: {e}"))
}

fn nexus_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".nexus42").join("config.toml"))
}

fn read_setup_completed() -> Option<bool> {
    let path = nexus_config_path()?;
    read_setup_completed_at(&path).ok()?
}

/// Whether `.setup()` should spawn `SidecarManager::start`.
///
/// V1.105 D2: always `true` regardless of `setup_completed` (absent / false / true).
/// Production `.setup()` always spawns; this helper exists so unit tests can pin
/// the always-start contract without a Tauri app handle.
#[cfg(test)]
fn setup_auto_starts_sidecar(_setup_completed: Option<bool>) -> bool {
    true
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

fn read_entrance() -> Option<String> {
    let path = nexus_config_path()?;
    read_entrance_at(&path).ok()?
}

fn read_entrance_at(path: &Path) -> anyhow::Result<Option<String>> {
    let content = std::fs::read_to_string(path)?;
    #[derive(serde::Deserialize, Default)]
    struct ConfigFile {
        #[serde(default)]
        entrance: Option<String>,
    }
    Ok(toml::from_str::<ConfigFile>(&content)?.entrance)
}

fn write_entrance(value: &str) -> anyhow::Result<()> {
    let path =
        nexus_config_path().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    write_entrance_at(&path, value)
}

fn write_entrance_at(path: &Path, value: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Preserve existing keys by round-tripping through a toml edit document
    // (same discipline as `write_setup_completed_at` — a parse failure is
    // propagated rather than overwriting a partially-written config).
    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    doc["entrance"] = toml_edit::value(value);
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
/// The profile is stored as the sole `native_cli` provider entry so the agent
/// host subsystem and Settings `get_agent_profile` (first `native_cli`) agree on
/// the active choice. Prior `native_cli` rows are removed (upsert/replace);
/// non-`native_cli` providers and other top-level keys are preserved.
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

    // Upsert/replace: keep a single active native_cli so first-entry read
    // matches the last Save (Settings reopen preselect).
    let mut idx = providers.len();
    while idx > 0 {
        idx -= 1;
        let is_native_cli = providers
            .get(idx)
            .and_then(|p| p.get("protocol"))
            .and_then(|v| v.as_str())
            == Some("native_cli");
        if is_native_cli {
            providers.remove(idx);
        }
    }

    let mut provider = toml_edit::Table::new();
    provider["id"] = toml_edit::value(name);
    provider["protocol"] = toml_edit::value("native_cli");
    if let Some(cmd) = launch_command {
        provider["command"] = toml_edit::value(cmd);
    }
    providers.push(provider);

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
        // Skip malformed rows (missing/empty id) so a later valid native_cli
        // can still preselect (qc3 F-002).
        let Some(name) = provider.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
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
///
/// V1.117: writes both `[workspace_path_by_creator][active_creator_id]` (SSOT)
/// and the top-level `workspace_path` legacy mirror (AD-P0-1 / AD-P0-3).
#[tauri::command]
fn set_workspace_path(path: String) -> Result<(), String> {
    let config_path = nexus_config_path().ok_or("cannot determine home directory")?;
    write_workspace_path_for_active_creator_at(&config_path, &path)
        .map_err(|e| format!("failed to write workspace_path: {e}"))
}

/// Write `workspace_path` to `~/.nexus42/config.toml`, preserving other keys.
///
/// Legacy helper used only by bootstrap tests; new code should prefer
/// `write_workspace_path_for_active_creator_at` which maintains the per-creator
/// map + mirror.
#[cfg(test)]
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

/// Write a workspace path for a specific creator and mirror it to the legacy
/// top-level `workspace_path` key (AD-P0-1).
fn write_workspace_path_by_creator_at(
    path: &Path,
    creator_id: &str,
    value: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    if doc.get("workspace_path_by_creator").is_none() {
        doc["workspace_path_by_creator"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["workspace_path_by_creator"][creator_id] = toml_edit::value(value);
    doc["workspace_path"] = toml_edit::value(value);
    atomic_write(path, &doc.to_string())?;
    Ok(())
}

/// Write the active Profile's workspace path to the per-creator map and mirror it
/// to the legacy `workspace_path` key. Errors if no `active_creator_id` exists.
fn write_workspace_path_for_active_creator_at(path: &Path, value: &str) -> anyhow::Result<()> {
    let config = read_workspace_config_at(path)?;
    let creator_id = config
        .active_creator_id
        .ok_or_else(|| anyhow::anyhow!("no active creator_id; run setup bootstrap first"))?;
    write_workspace_path_by_creator_at(path, &creator_id, value)
}

/// Switch the active creator in `~/.nexus42/config.toml`, ensuring the target has
/// a workspace path entry (creating a default one if missing) and mirroring that
/// path to the legacy `workspace_path` key (AD-P0-3 / AD-P0-6).
fn switch_active_creator_at(path: &Path, creator_id: &str) -> anyhow::Result<String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut doc = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        text.parse::<toml_edit::DocumentMut>()?
    } else {
        toml_edit::DocumentMut::new()
    };

    if doc.get("workspace_path_by_creator").is_none() {
        doc["workspace_path_by_creator"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let target_path = doc["workspace_path_by_creator"]
        .get(creator_id)
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            let default = default_workspace_root().to_string_lossy().to_string();
            doc["workspace_path_by_creator"][creator_id] = toml_edit::value(&default);
            default
        });

    doc["active_creator_id"] = toml_edit::value(creator_id);
    doc["workspace_path"] = toml_edit::value(&target_path);

    // Reset workspace slug to `"default"` (parity with daemon `set_active_creator`).
    // Profile switch must not leave a stale non-default slug, and must not leave
    // the entry absent for older read paths that lack the default fallback.
    if doc.get("active_workspace_slug_by_creator").is_none() {
        doc["active_workspace_slug_by_creator"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["active_workspace_slug_by_creator"][creator_id] = toml_edit::value("default");

    atomic_write(path, &doc.to_string())?;
    Ok(target_path)
}

/// `switch_active_creator` — Tauri command used by the footer Profile switch.
/// Updates `active_creator_id`, ensures the target Profile has a workspace path
/// entry, and mirrors that path to the legacy `workspace_path` key. Returns the
/// resolved workspace path for the switched-to Profile (AD-P0-3 / AC-P0-5).
#[tauri::command]
fn switch_active_creator(creator_id: String) -> Result<String, String> {
    validate_creator_id_safe(&creator_id)
        .map_err(|reason| format!("invalid creator_id: {reason}"))?;
    let config_path = nexus_config_path().ok_or("cannot determine home directory")?;
    switch_active_creator_at(&config_path, &creator_id)
        .map_err(|e| format!("failed to switch active creator: {e}"))
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
        .and_then(|map| {
            cfg.active_creator_id
                .as_ref()
                .and_then(|id| map.get(id).cloned())
        });
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

/// Desktop-only Tauri IPC command: optional wizard convenience — writes minimum
/// creator/workspace state for setup wizard Continue. **Not** a daemon boot
/// prerequisite after V1.118 (daemon boots without `active_creator_id`).
/// Idempotent — if a creator ID already exists, returns it without overwriting.
///
/// See `.mstar/iterations/v1.100/specs/desktop-first-launch-bootstrap.md`.
#[tauri::command]
fn ensure_setup_bootstrap() -> Result<BootstrapResult, String> {
    let config_path = nexus_config_path().ok_or("cannot determine home directory")?;
    ensure_setup_bootstrap_at(&config_path).map_err(|e| format!("bootstrap failed: {e}"))
}

/// Toggle the main webview window between maximized and restored. Wired from the
/// Chronos titlebar empty-paint double-click handler (Overlay titlebar AC-5).
///
/// Tauri v2 `WebviewWindow` exposes `is_maximized` / `maximize` / `unmaximize`
/// (no `toggle_maximize` method).
#[tauri::command]
fn toggle_maximize_window(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
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
            // V1.105 D2: always auto-start/attach sidecar (rewrites V1.100 Rule 13).
            // `setup_completed` only routes the web shell after Ready — it does not
            // gate whether Tauri starts the sidecar.
            let manager = setup_manager.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = manager.start(&handle).await {
                    eprintln!("nexus-desktop: sidecar failed to start: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_with,
            reveal_in_finder,
            open_external_url,
            get_workspace_root,
            get_daemon_status,
            start_daemon,
            stop_daemon,
            restart_daemon,
            reset_local_database,
            pick_directory,
            set_workspace_path,
            switch_active_creator,
            get_setup_completed,
            set_setup_completed,
            get_entrance,
            set_entrance,
            set_agent_profile,
            get_agent_profile,
            ensure_setup_bootstrap,
            toggle_maximize_window,
            connection_config::get_connection_config,
            connection_config::set_connection_config,
            connection_config::delete_connection_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Nexus desktop shell")
        // Tauri v2 idiomatic app-lifecycle hook: `RunEvent::ExitRequested` runs
        // before the async runtime shuts down. We ask whether to stop the
        // daemon (owned sidecar or attached external) instead of always killing
        // it — so `pnpm dev:desktop` workflows can leave the CLI running.
        .run(move |app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if EXIT_CONFIRMED.load(Ordering::SeqCst) {
                    return;
                }
                // Drop duplicate exit requests while a prompt is already open.
                if EXIT_PROMPT_ACTIVE.swap(true, Ordering::SeqCst) {
                    api.prevent_exit();
                    return;
                }
                api.prevent_exit();
                let manager = sidecar_manager.clone();
                let app = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    handle_quit_with_daemon_prompt(app, manager).await;
                });
            }
        });
}

/// Ask the user whether to stop the daemon when quitting the desktop shell.
async fn handle_quit_with_daemon_prompt(app: AppHandle, manager: sidecar::SidecarManager) {
    let running = manager.is_running().await;
    if !running {
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        app.exit(0);
        return;
    }

    let owned = manager.is_owned().await;
    let port = manager.port().await;
    let message = if owned {
        format!(
            "Nexus started the local daemon on port {port}.\n\n\
             Stop it when quitting, or keep it running for CLI / next launch?"
        )
    } else {
        format!(
            "Nexus is attached to a daemon already running on port {port}.\n\n\
             Stop that daemon when quitting, or leave it running?"
        )
    };

    let (tx, rx) = tokio::sync::oneshot::channel::<MessageDialogResult>();
    app.dialog()
        .message(message)
        .title("Quit Nexus")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            QUIT_STOP_LABEL.to_string(),
            QUIT_KEEP_LABEL.to_string(),
            QUIT_CANCEL_LABEL.to_string(),
        ))
        .show_with_result(move |result| {
            let _ = tx.send(result);
        });

    let Ok(result) = rx.await else {
        EXIT_PROMPT_ACTIVE.store(false, Ordering::SeqCst);
        return;
    };

    let stop_and_quit = match &result {
        MessageDialogResult::Yes | MessageDialogResult::Ok => true,
        MessageDialogResult::Custom(label) if label == QUIT_STOP_LABEL => true,
        _ => false,
    };
    let keep_and_quit = match &result {
        MessageDialogResult::No => true,
        MessageDialogResult::Custom(label) if label == QUIT_KEEP_LABEL => true,
        _ => false,
    };

    if stop_and_quit {
        if let Err(e) = manager.stop_listening_daemon().await {
            eprintln!("nexus-desktop: failed to stop daemon on quit: {e}");
        }
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        app.exit(0);
    } else if keep_and_quit {
        manager.release_without_stop().await;
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        app.exit(0);
    } else {
        // Cancel — leave the app running and allow a future quit prompt.
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        EXIT_PROMPT_ACTIVE.store(false, Ordering::SeqCst);
    }
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
        default_workspace_root, guard_path, read_agent_profile_at, read_entrance_at,
        read_setup_completed_at, reset_local_database_at, resolve_workspace_root_at,
        setup_auto_starts_sidecar, switch_active_creator_at, validate_url_scheme,
        write_agent_profile_at, write_entrance_at, write_setup_completed_at,
        write_workspace_path_at, write_workspace_path_by_creator_at,
        write_workspace_path_for_active_creator_at, AgentProfile, PathGuardError, WorkspaceRoot,
    };
    use super::{ensure_setup_bootstrap_at, generate_local_creator_id, read_bootstrap_state};
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

    // ── V1.117 P1: open_external_url scheme validation ────────────────

    #[test]
    fn open_external_url_accepts_http() {
        assert!(validate_url_scheme("http://example.com/install").is_ok());
    }

    #[test]
    fn open_external_url_accepts_https() {
        assert!(validate_url_scheme("https://example.com/install").is_ok());
    }

    #[test]
    fn open_external_url_rejects_file_scheme() {
        let err = validate_url_scheme("file:///etc/passwd").expect_err("file scheme rejected");
        assert!(
            err.contains("not allowed"),
            "error should mention scheme: {err}"
        );
    }

    #[test]
    fn open_external_url_rejects_javascript_scheme() {
        let err =
            validate_url_scheme("javascript:alert(1)").expect_err("javascript scheme rejected");
        assert!(err.contains("scheme"), "error should mention scheme: {err}");
    }

    #[test]
    fn open_external_url_rejects_ftp_scheme() {
        let err = validate_url_scheme("ftp://files.example.com").expect_err("ftp scheme rejected");
        assert!(
            err.contains("not allowed"),
            "error should mention scheme: {err}"
        );
    }

    #[test]
    fn open_external_url_rejects_missing_scheme() {
        let err = validate_url_scheme("no-scheme").expect_err("missing scheme rejected");
        assert!(err.contains("scheme"), "error should mention scheme: {err}");
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
    fn entrance_roundtrips_through_config_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        write_entrance_at(&config_path, "developer").expect("write developer");
        assert_eq!(
            read_entrance_at(&config_path).unwrap().as_deref(),
            Some("developer")
        );

        write_entrance_at(&config_path, "content-creator").expect("write content-creator");
        assert_eq!(
            read_entrance_at(&config_path).unwrap().as_deref(),
            Some("content-creator")
        );
    }

    #[test]
    fn entrance_write_preserves_existing_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "workspace_path = \"/existing/workspace\"\nruntime_mode = \"local_only\"\n",
        )
        .expect("write initial config");

        write_entrance_at(&config_path, "developer").expect("write entrance");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("workspace_path = \"/existing/workspace\""));
        assert!(text.contains("runtime_mode = \"local_only\""));
        assert!(text.contains("entrance = \"developer\""));
    }

    #[test]
    fn entrance_write_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        let original = "workspace_path = \"/existing/workspace\"\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = write_entrance_at(&config_path, "developer");
        assert!(result.is_err(), "malformed TOML should be rejected");

        // The corrupt file must NOT be overwritten with a single-key document.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("workspace_path = \"/existing/workspace\""),
            "existing keys must survive a failed write"
        );
        assert!(
            !text.contains("entrance"),
            "entrance must not be written on parse failure"
        );
    }

    #[test]
    fn entrance_missing_field_reads_none() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "workspace_path = \"/existing/workspace\"\n",
        )
        .expect("write config without entrance");

        assert_eq!(
            read_entrance_at(&config_path).unwrap(),
            None,
            "missing entrance key must read None (get_entrance defaults to content-creator)"
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
    fn agent_profile_write_replaces_prior_native_cli_so_read_returns_latest() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");

        write_agent_profile_at(&config_path, "claude-cli", Some("claude")).expect("write A");
        write_agent_profile_at(&config_path, "codex", Some("codex")).expect("write B");

        let profile = read_agent_profile_at(&config_path);
        assert_eq!(
            profile,
            Some(AgentProfile {
                name: "codex".to_owned(),
                launch_command: Some("codex".to_owned()),
            }),
            "Save B after A must round-trip for Settings reopen preselect"
        );

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert_eq!(
            text.matches("protocol = \"native_cli\"").count(),
            1,
            "only one native_cli provider should remain"
        );
        assert!(!text.contains("claude-cli"));
    }

    #[test]
    fn agent_profile_write_custom_launch_round_trips() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");

        write_agent_profile_at(&config_path, "claude-cli", Some("claude")).expect("write A");
        write_agent_profile_at(&config_path, "custom", Some("/usr/local/bin/my-agent"))
            .expect("write custom");

        let profile = read_agent_profile_at(&config_path);
        assert_eq!(
            profile,
            Some(AgentProfile {
                name: "custom".to_owned(),
                launch_command: Some("/usr/local/bin/my-agent".to_owned()),
            })
        );
    }

    #[test]
    fn agent_profile_write_preserves_non_native_cli_providers() {
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

        write_agent_profile_at(&config_path, "codex", Some("codex")).expect("replace native_cli");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("http-agent"));
        assert!(text.contains("protocol = \"http\""));
        assert!(text.contains("id = \"codex\""));
        assert!(!text.contains("claude-cli"));
        assert_eq!(
            read_agent_profile_at(&config_path),
            Some(AgentProfile {
                name: "codex".to_owned(),
                launch_command: Some("codex".to_owned()),
            })
        );
    }

    #[test]
    fn agent_profile_read_skips_native_cli_row_without_id() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("agent-host").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir");
        std::fs::write(
            &config_path,
            r#"
[[providers]]
protocol = "native_cli"
command = "broken"

[[providers]]
id = "codex"
protocol = "native_cli"
command = "codex"
"#,
        )
        .expect("write config");

        assert_eq!(
            read_agent_profile_at(&config_path),
            Some(AgentProfile {
                name: "codex".to_owned(),
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

    // ── V1.118 P0 T3: clean-home launch (AC-P0-7) ───────────────────────

    #[test]
    fn v118_clean_home_without_nexus42_dir_resolves_default_workspace() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let default_root = tmp.path().join("default-workspace");
        assert!(
            !tmp.path().join(".nexus42").exists(),
            "clean home must not have .nexus42"
        );
        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, default_root);
    }

    #[test]
    fn v118_always_start_sidecar_without_prior_bootstrap() {
        // AC-P0-7: V1.105 D2 always-start must not require wizard bootstrap.
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        assert!(!config_path.exists(), "clean home has no config yet");
        assert!(
            setup_auto_starts_sidecar(None),
            "absent config must still auto-start"
        );
        assert!(
            setup_auto_starts_sidecar(read_setup_completed_at(&config_path).ok().flatten()),
            "missing config file must still auto-start"
        );
    }

    // ── V1.105 P0: sidecar always-start (D2) + V1.100 bootstrap ─────────
    // Pins lifecycle branches per
    // `.mstar/iterations/v1.105/specs/daemon-fullscreen-gate.md`:
    //   1. setup_completed absent/false/true → .setup() ALWAYS auto-starts
    //   2. ensure_setup_bootstrap idempotency (V1.100, unchanged)
    //   3. Bootstrap failure: config write failure → no partial/corrupt state
    //
    // Always-start is tested via `setup_auto_starts_sidecar` (unit-testable
    // D2 policy mirror). Production `.setup()` always spawns unconditionally;
    // the Tauri closure itself remains smoke-tested.

    #[test]
    fn setup_completed_absent_still_auto_starts() {
        // Clean-state: no config file → read returns Err / None semantics.
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        assert!(read_setup_completed_at(&config_path).is_err());
        assert!(
            setup_auto_starts_sidecar(None),
            "absent marker must still auto-start (V1.105 D2)"
        );

        write_setup_completed_at(&config_path, false).expect("write false");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(false));
        assert!(
            setup_auto_starts_sidecar(Some(false)),
            "false marker must still auto-start (V1.105 D2)"
        );
    }

    #[test]
    fn setup_completed_true_still_auto_starts() {
        // Existing install: setup_completed = true → still auto-starts.
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        write_setup_completed_at(&config_path, true).expect("write true");
        assert_eq!(read_setup_completed_at(&config_path).unwrap(), Some(true));
        assert!(
            setup_auto_starts_sidecar(Some(true)),
            "true marker must still auto-start (V1.105 D2)"
        );
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
        write_workspace_path_at(&config_path, "/tmp/test-workspace").expect("write workspace_path");
        write_setup_completed_at(&config_path, false).expect("write setup_completed");

        let result = ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");
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
        let result = ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");
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

        let result = ensure_setup_bootstrap_at(&config_path).expect("bootstrap should succeed");

        let (creator_id, slug) = read_bootstrap_state(&config_path).expect("read state");
        assert_eq!(creator_id.as_deref(), Some(result.creator_id.as_str()));
        assert_eq!(slug.as_deref(), Some("default"));
    }

    // ── V1.117 P0: per-Profile workspace path (AD-P0-1..AD-P0-3) ─────────

    fn temp_home_with_default_root() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let default_root = tmp.path().join("Documents").join("nexus").join("default");
        std::fs::create_dir_all(&default_root).expect("mkdir default root");
        (tmp, default_root)
    }

    #[test]
    fn workspace_path_set_dual_writes_to_map_and_legacy() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\n",
        )
        .expect("write config");

        write_workspace_path_for_active_creator_at(&config_path, "/custom/profile/path")
            .expect("write active creator path");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains(r#"ctr_localABCD1234EF56 = "/custom/profile/path""#),
            "per-creator map should contain the path: {text}"
        );
        assert!(
            text.contains("workspace_path = \"/custom/profile/path\""),
            "legacy workspace_path should mirror the active Profile path: {text}"
        );
    }

    #[test]
    fn workspace_path_set_requires_active_creator_id() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "workspace_path = \"/legacy\"\n").expect("write config");

        let result = write_workspace_path_for_active_creator_at(&config_path, "/new");
        assert!(result.is_err(), "set without active_creator_id must fail");

        // The file must not be modified.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(!text.contains("workspace_path_by_creator"));
        assert!(text.contains("workspace_path = \"/legacy\""));
    }

    #[test]
    fn workspace_path_set_preserves_existing_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\nsetup_completed = true\nruntime_mode = \"local_only\"\n",
        )
        .expect("write config");

        write_workspace_path_for_active_creator_at(&config_path, "/custom/path")
            .expect("write active creator path");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("setup_completed = true"));
        assert!(text.contains("runtime_mode = \"local_only\""));
        assert!(text.contains("active_creator_id"));
    }

    #[test]
    fn workspace_path_set_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        let original = "active_creator_id = \"ctr_localABCD1234EF56\"\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = write_workspace_path_for_active_creator_at(&config_path, "/new");
        assert!(result.is_err(), "malformed TOML should be rejected");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("active_creator_id"),
            "existing keys must survive a failed write"
        );
        assert!(
            !text.contains("workspace_path_by_creator"),
            "per-creator map must not be written on parse failure"
        );
        assert!(
            !text.contains("workspace_path = \"/new\""),
            "legacy workspace_path must not be written on parse failure"
        );
    }

    #[test]
    fn workspace_path_by_creator_direct_write_mirrors_to_legacy() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");

        write_workspace_path_by_creator_at(&config_path, "ctr_localXYZ", "/path/xyz")
            .expect("write by creator");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains(r#"ctr_localXYZ = "/path/xyz""#));
        assert!(text.contains("workspace_path = \"/path/xyz\""));
    }

    #[test]
    fn resolve_workspace_root_migrates_legacy_to_active_profile() {
        let (tmp, default_root) = temp_home_with_default_root();
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\nworkspace_path = \"/legacy/workspace\"\n",
        )
        .expect("write legacy config");

        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, PathBuf::from("/legacy/workspace"));

        // Migration should have persisted the legacy path into the per-creator map.
        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains(r#"ctr_localABCD1234EF56 = "/legacy/workspace""#),
            "legacy path should migrate to workspace_path_by_creator: {text}"
        );
        assert!(
            text.contains("workspace_path = \"/legacy/workspace\""),
            "legacy mirror should remain: {text}"
        );
    }

    #[test]
    fn resolve_workspace_root_prefers_map_over_legacy() {
        let (tmp, default_root) = temp_home_with_default_root();
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\n\
             workspace_path = \"/legacy/workspace\"\n\
             [workspace_path_by_creator]\n\
             ctr_localABCD1234EF56 = \"/map/workspace\"\n",
        )
        .expect("write config");

        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, PathBuf::from("/map/workspace"));
    }

    #[test]
    fn resolve_workspace_root_does_not_migrate_when_map_is_populated() {
        let (tmp, default_root) = temp_home_with_default_root();
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\n\
             workspace_path = \"/legacy/workspace\"\n\
             [workspace_path_by_creator]\n\
             ctr_localABCD1234EF56 = \"/map/workspace\"\n",
        )
        .expect("write config");

        let text_before = std::fs::read_to_string(&config_path).expect("read config");
        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, PathBuf::from("/map/workspace"));

        let text_after = std::fs::read_to_string(&config_path).expect("read config");
        assert_eq!(
            text_before, text_after,
            "migration should not rewrite a populated map"
        );
    }

    #[test]
    fn resolve_workspace_root_fallback_to_default() {
        let (tmp, default_root) = temp_home_with_default_root();
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(
            &config_path,
            "active_creator_id = \"ctr_localABCD1234EF56\"\n",
        )
        .expect("write config");

        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, default_root);
    }

    #[test]
    fn resolve_workspace_root_returns_legacy_when_no_active_creator() {
        let (tmp, default_root) = temp_home_with_default_root();
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(&config_path, "workspace_path = \"/legacy/workspace\"\n")
            .expect("write config");

        // Without active_creator_id we cannot migrate, but we still fall back to legacy.
        let resolved =
            resolve_workspace_root_at(tmp.path(), &default_root).expect("resolve workspace root");
        assert_eq!(resolved, PathBuf::from("/legacy/workspace"));

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            !text.contains("workspace_path_by_creator"),
            "must not migrate without active_creator_id"
        );
    }

    #[test]
    fn switch_active_creator_updates_active_and_mirrors_target_path() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"profile_a\"\n\
             workspace_path = \"/path/a\"\n\
             [workspace_path_by_creator]\n\
             profile_a = \"/path/a\"\n\
             profile_b = \"/path/b\"\n",
        )
        .expect("write config");

        let path = switch_active_creator_at(&config_path, "profile_b").expect("switch");
        assert_eq!(path, "/path/b");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("active_creator_id = \"profile_b\""));
        assert!(text.contains("workspace_path = \"/path/b\""));
        assert!(text.contains("profile_a = \"/path/a\""));
        assert!(text.contains("profile_b = \"/path/b\""));
    }

    #[test]
    fn switch_active_creator_creates_default_entry_for_missing_profile() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"profile_a\"\n\
             workspace_path = \"/path/a\"\n\
             [workspace_path_by_creator]\n\
             profile_a = \"/path/a\"\n",
        )
        .expect("write config");

        let path = switch_active_creator_at(&config_path, "profile_b").expect("switch");
        let default_path = default_workspace_root().to_string_lossy().to_string();
        assert_eq!(path, default_path);

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("active_creator_id = \"profile_b\""));
        assert!(
            text.contains(&format!("profile_b = \"{default_path}\"")),
            "default entry should be created for profile_b: {text}"
        );
        assert!(
            text.contains(&format!("workspace_path = \"{default_path}\"")),
            "legacy workspace_path should mirror the new default path: {text}"
        );
    }

    #[test]
    fn switch_active_creator_preserves_existing_keys() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"profile_a\"\n\
             setup_completed = true\n\
             runtime_mode = \"local_only\"\n\
             [workspace_path_by_creator]\n\
             profile_a = \"/path/a\"\n",
        )
        .expect("write config");

        switch_active_creator_at(&config_path, "profile_b").expect("switch");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("setup_completed = true"));
        assert!(text.contains("runtime_mode = \"local_only\""));
    }

    #[test]
    fn switch_active_creator_rejects_malformed_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        let original = "active_creator_id = \"profile_a\"\nmalformed = \"unclosed\n";
        std::fs::write(&config_path, original).expect("write malformed config");

        let result = switch_active_creator_at(&config_path, "profile_b");
        assert!(result.is_err(), "malformed TOML should be rejected");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            text.contains("active_creator_id = \"profile_a\""),
            "existing keys must survive a failed switch"
        );
        assert!(
            !text.contains("profile_b"),
            "switch must not write on parse failure"
        );
    }

    #[test]
    fn switch_active_creator_resets_target_workspace_slug_to_default() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "active_creator_id = \"profile_a\"\n\
             workspace_path = \"/path/a\"\n\
             [workspace_path_by_creator]\n\
             profile_a = \"/path/a\"\n\
             profile_b = \"/path/b\"\n\
             [active_workspace_slug_by_creator]\n\
             profile_b = \"old-slug\"\n",
        )
        .expect("write config");

        switch_active_creator_at(&config_path, "profile_b").expect("switch");

        let text = std::fs::read_to_string(&config_path).expect("read config");
        assert!(text.contains("active_creator_id = \"profile_b\""));
        assert!(
            !text.contains("old-slug"),
            "stale workspace slug for target creator should be cleared"
        );
        assert!(
            text.contains("profile_b = \"default\"") || text.contains("profile_b = 'default'"),
            "target creator should be reset to the default workspace slug, got:\n{text}"
        );
    }

    #[test]
    fn get_workspace_root_re_resolves_from_toml() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let config_path = tmp.path().join(".nexus42").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).expect("mkdir config dir");
        std::fs::write(
            &config_path,
            "active_creator_id = \"profile_a\"\n\
             [workspace_path_by_creator]\n\
             profile_a = \"/custom/path\"\n",
        )
        .expect("write config");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());
        let result = crate::get_workspace_root();
        if let Some(original) = original_home {
            std::env::set_var("HOME", original);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(
            result,
            Some("/custom/path".to_string()),
            "get_workspace_root must re-resolve from TOML, not a startup snapshot"
        );
    }
}
