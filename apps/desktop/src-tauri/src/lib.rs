//! Nexus Tauri v2 desktop shell — application entry + native commands.
//!
//! Scope (V1.66 P0): wraps the bundled `apps/web/dist` (served via
//! `build.frontendDist`) and exposes the desktop-only `NexusClient` extensions
//! the browser sandbox cannot perform (compass §5 #1/#8, desktop-shell.md
//! §5/§9). The two custom commands are `open_with(path)` (open in the system
//! default editor) and `reveal_in_finder(path)` (reveal in Finder).
//!
//! Both commands enforce an AUTHORITATIVE runtime path guard (canonicalize +
//! prefix-check against the active workspace root) before delegating to the
//! opener engine. The Tauri capability/opener `scope` is defense-in-depth only
//! — it is static and cannot encode a dynamic workspace root (§5 #8).
//!
//! Daemon lifecycle (sidecar autostart/stop/restart) is P1 and intentionally
//! absent here; P0 runs against an externally-started daemon
//! (`nexus42 daemon start --foreground`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

mod sidecar;

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
/// daemon uses at boot (`crates/nexus42/src/config.rs`). `None` when no
/// workspace is configured; every path-guard call then denies by default.
#[derive(Debug, Clone, Default)]
struct WorkspaceRoot(Option<PathBuf>);

/// Resolve the active workspace root exactly as the daemon/CLI do: read
/// `~/.nexus42/config.toml` and return its `workspace_path`.
///
/// Kept deliberately narrow (string-only parse) so this standalone Tauri crate
/// does not depend on the `nexus42` config type graph. If the file is absent,
/// unreadable, or lacks `workspace_path`, this returns `None` and the path
/// guard denies by default (fail-closed).
fn resolve_workspace_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".nexus42").join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    #[derive(serde::Deserialize)]
    struct ConfigFile {
        workspace_path: Option<PathBuf>,
    }
    toml::from_str::<ConfigFile>(&content)
        .ok()?
        .workspace_path
        .filter(|p| !p.as_os_str().is_empty())
}

/// Authoritative path guard (compass §5 #8, desktop-shell.md §9).
///
/// Mirrors W-002 in `host_tool_handlers.rs` (~L710-755): canonicalize the
/// requested path AND the workspace root, then require the requested path to
/// lie within the workspace root prefix. Relative paths (the daemon stores
/// `body_path`/`outline_path` workspace-relative, e.g. `Works/<ref>/Stories/…`)
/// are resolved against the workspace root first.
fn guard_path(requested: &str, workspace_root: &WorkspaceRoot) -> Result<PathBuf, PathGuardError> {
    let root = workspace_root
        .0
        .as_ref()
        .ok_or(PathGuardError::WorkspaceRootUnknown)?;
    let root = root
        .canonicalize()
        .map_err(|_| PathGuardError::WorkspaceRootUnknown)?;

    let requested_path = Path::new(requested);
    // Resolve relative paths against the workspace root (the form the Local API
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
    manager.start(&app).await
}

/// `stop_daemon` — graceful stop of the owned sidecar.
#[tauri::command]
async fn stop_daemon(manager: State<'_, sidecar::SidecarManager>) -> Result<(), String> {
    manager.stop().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        // Shell plugin drives the bundled `nexus42` sidecar via
        // `tauri_plugin_shell::ShellExt::sidecar` (P1).
        .plugin(tauri_plugin_shell::init())
        .plugin(port_plugin)
        .manage(workspace_root)
        .manage(sidecar_manager.clone())
        .setup(move |app| {
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
            get_workspace_root,
            get_daemon_status,
            start_daemon,
            stop_daemon,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Nexus desktop shell")
        .run(move |_app_handle, event| {
            // Gracefully stop the owned sidecar *before* the Tauri async runtime
            // shuts down. Running this cleanup after `run()` returns races with
            // tokio teardown and can panic with "Cannot start a runtime from
            // within a runtime" (Greptile P1; qc1 S-5). The SIGTERM → bounded
            // timeout → SIGKILL path in `SidecarManager::stop()` still fires.
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

    use super::{guard_path, PathGuardError, WorkspaceRoot};
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
}
