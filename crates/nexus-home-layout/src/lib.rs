//! Shared operational path layout (`ADR-014`) under the user home directory.
//!
//! Used by `nexus42` CLI and the daemon runtime so `SQLite` and workspace dirs resolve identically.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Device ID generation and persistence (`~/.nexus42/device-id`, UUID v4).
///
/// V1.148 P3 N-C0: moved here from `nexus-cloud-sync` so the path-layout SSOT
/// also owns the value; `nexus-cloud-sync::device_id` re-exports it.
pub mod device_id;

const NEXUS_DIR: &str = ".nexus42";

/// Create the `~/.nexus42/` system directory layout and config skeleton if they
/// do not exist. Safe to call repeatedly — existing files are not overwritten.
///
/// Creates the following structure:
///
/// ```text
/// ~/.nexus42/
/// ├── config.toml          (empty/absent → default empty)
/// ├── skills/              (embedded skill sync target)
/// ├── presets/             (user-installed presets)
/// ├── modules/             (user-installed WASM modules)
/// ├── tls/                 (TLS cert + key)
/// ├── shared/              (global shared DB)
/// ├── acp/runs/            (ACP run trace storage)
/// └── rules/               (user writing-craft rules)
/// ```
///
/// If `config.toml` does not exist, an empty default is written so that
/// subsequent config reads succeed without error.
///
/// # Errors
///
/// Returns an I/O error if directory creation or config file write fails.
pub fn ensure_system_layout(nexus_home: &Path) -> std::io::Result<()> {
    // Create the nexus home root if missing
    std::fs::create_dir_all(nexus_home)?;

    // System subdirectories
    std::fs::create_dir_all(nexus_home.join("skills"))?;
    std::fs::create_dir_all(nexus_home.join("presets"))?;
    std::fs::create_dir_all(nexus_home.join("modules"))?;
    std::fs::create_dir_all(nexus_home.join("tls"))?;
    std::fs::create_dir_all(nexus_home.join("shared"))?;
    std::fs::create_dir_all(nexus_home.join("acp").join("runs"))?;
    std::fs::create_dir_all(nexus_home.join("rules"))?;

    // Write empty config.toml if it does not exist (no config.json either)
    let toml_path = nexus_home.join("config.toml");
    let json_path = nexus_home.join("config.json");
    if !toml_path.exists() && !json_path.exists() {
        std::fs::write(&toml_path, "")?;
    }

    Ok(())
}

/// Resolve `~/.nexus42` from the user's home directory.
#[must_use]
pub fn nexus_root_from_home(home: &Path) -> PathBuf {
    home.join(NEXUS_DIR)
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/`
#[must_use]
pub fn creator_workspaces_root(home: &Path, creator_id: &str) -> PathBuf {
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("workspaces")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`
#[must_use]
pub fn operational_workspace_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    creator_workspaces_root(home, creator_id).join(workspace_slug)
}

/// Workspace-local `SQLite`: `.../workspaces/<slug>/state.db`
#[must_use]
pub fn workspace_state_db_path(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("state.db")
}

/// Shared global `SQLite`: `$HOME/.nexus42/shared/global_state.db`
#[must_use]
pub fn shared_global_db_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home)
        .join("shared")
        .join("global_state.db")
}

/// `$HOME/.nexus42/skills/` — directory for synced embedded skills.
///
/// See `crates/nexus-orchestration/src/skill_sync.rs` for the sync logic that
/// populates this directory.
#[must_use]
pub fn user_skills_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("skills")
}

/// `$HOME/.nexus42/presets/` — base directory for user-installed presets.
///
/// Each subdirectory under this path is expected to contain a `preset.yaml`.
/// See `crates/nexus-orchestration/src/user_preset_dir.rs` for scanning logic.
/// Directories starting with `_` or `.` are reserved and skipped by the scanner.
#[must_use]
pub fn user_preset_base_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("presets")
}

/// `$HOME/.nexus42/modules/` — base directory for user-installed compute
/// modules (V1.61 P-last T3; compass Q2 User layer).
///
/// Each subdirectory `<id>/` is expected to contain `<id>.wasm` +
/// `manifest.json`. The daemon warms these into the daemon-wide module cache
/// at boot (see `nexus_wasm_host::ModuleCache::warm_dir`). A missing
/// directory is treated as empty (user modules are optional).
#[must_use]
pub fn user_modules_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("modules")
}

/// `$HOME/.nexus42/presets/<name>/` — path to a specific user preset bundle.
#[must_use]
pub fn user_preset_bundle_dir(home: &Path, name: &str) -> PathBuf {
    user_preset_base_dir(home).join(name)
}

/// List user preset IDs (directory names containing `preset.yaml`) under the presets directory.
///
/// Returns an empty vector if the presets directory doesn't exist or can't be read.
/// Directories starting with `_` or `.` are skipped.
#[must_use]
pub fn list_user_preset_ids(nexus_home: &Path) -> Vec<String> {
    let user_dir = user_preset_base_dir(nexus_home);

    if !user_dir.exists() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&user_dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // Skip system-prefixed and hidden dirs.
            if name.starts_with('_') || name.starts_with('.') {
                return None;
            }
            // Must contain a preset.yaml to be valid.
            if e.path().join("preset.yaml").exists() {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/Pool/Ideas/`
///
/// Creator-scoped inspiration items; lives under the operational workspace
/// (NOT under Works/, because inspiration is a creator-level concern that
/// outlives any single Work).
#[must_use]
pub fn creator_inspiration_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug)
        .join("Pool")
        .join("Ideas")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/kb/`
///
/// Knowledge base directory for a workspace (ADR-014 layout, flat files + JSON index).
#[must_use]
pub fn creator_kb_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("kb")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/kb/entries/`
///
/// Individual KB entry files (`<entry_id>.md`).
#[must_use]
pub fn creator_kb_entries_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    creator_kb_dir(home, creator_id, workspace_slug).join("entries")
}

/// `$HOME/.nexus42/tls/` — directory for persistent TLS certificate + key.
///
/// The daemon creates this directory with mode `0o700` on first boot.
/// Cert and key files are written with `0o600` (owner-only read).
/// Fingerprint is SHA-256 of the DER-encoded certificate, colon-hex.
#[must_use]
pub fn tls_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("tls")
}

/// `$HOME/.nexus42/tls/cert.pem` — persistent self-signed TLS certificate.
#[must_use]
pub fn tls_cert_path(home: &Path) -> PathBuf {
    tls_dir(home).join("cert.pem")
}

/// `$HOME/.nexus42/tls/key.pem` — persistent TLS private key.
#[must_use]
pub fn tls_key_path(home: &Path) -> PathBuf {
    tls_dir(home).join("key.pem")
}

/// `$HOME/.nexus42/device-id` — persistent machine identifier (`UUID` v4).
#[must_use]
pub fn device_id_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("device-id")
}

/// `$HOME/.nexus42/connect/` — Connect Host (DF-72 N-C0) state directory.
#[must_use]
pub fn connect_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("connect")
}

/// `$HOME/.nexus42/connect/identity.key` — Connect Host Ed25519 identity key
/// (libp2p protobuf encoding; created once with 0600).
#[must_use]
pub fn connect_identity_key_path(home: &Path) -> PathBuf {
    connect_dir(home).join("identity.key")
}

/// `$HOME/.nexus42/connect/allowlist.json` — Connect Host peer allowlist
/// (`{ "peer_ids": ["12D3…", …] }`; missing file ⇒ empty ⇒ fail-closed).
#[must_use]
pub fn connect_allowlist_path(home: &Path) -> PathBuf {
    connect_dir(home).join("allowlist.json")
}

/// `$HOME/.nexus42/connect/issuer.key` — Connect Host capability-token
/// issuer key (Ed25519 libp2p protobuf encoding; created once with 0600;
/// DISTINCT from `identity.key` — the node identity and the token issuer
/// are different trust roles, V1.155 P1 architect lock #4).
#[must_use]
pub fn connect_issuer_key_path(home: &Path) -> PathBuf {
    connect_dir(home).join("issuer.key")
}

/// `$HOME/.nexus42/connect/config.json` — Connect Host operator config.
///
/// V1.155 P1 capability-token surface (`trusted_issuers` /
/// `require_capability_token` / `capability_token_provider`); absent file ⇒
/// current defaults. `allowlist.json` stays the peer/scope file.
#[must_use]
pub fn connect_config_path(home: &Path) -> PathBuf {
    connect_dir(home).join("config.json")
}

/// `$HOME/.nexus42/creators/<creator_id>/SOUL.md` (`ADR-014`, `ADR-016` D1).
///
/// # Defense-in-depth
///
/// If `creator_id` contains path traversal components (`..`, `/`, `\`), this
/// function panics rather than silently resolving to an unexpected path.
/// Callers (e.g., `soul_io::validate_creator_id()`) should validate `creator_id`
/// before calling this, but this acts as a safety net.
#[must_use]
pub fn creator_soul_md_path(home: &Path, creator_id: &str) -> PathBuf {
    assert_creator_id_safe(creator_id);
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("SOUL.md")
}

/// `$HOME/.nexus42/creators/<creator_id>/references/`
///
/// Root directory for reference source bodies belonging to a specific creator
/// (V1.26 reference store layout).
///
/// # Defense-in-depth
///
/// If `creator_id` contains path traversal components, this function panics.
/// See [`creator_soul_md_path`] for the same defense pattern.
#[must_use]
pub fn creator_references_root(home: &Path, creator_id: &str) -> PathBuf {
    assert_creator_id_safe(creator_id);
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("references")
}

/// `$HOME/.nexus42/creators/<creator_id>/references/units/<reference_id>/`
///
/// Directory for a single reference source unit's body and metadata
/// (V1.26 reference store layout).
///
/// # Defense-in-depth
///
/// Panics if `creator_id` or `reference_id` contains path traversal components.
#[must_use]
pub fn reference_unit_dir(home: &Path, creator_id: &str, reference_id: &str) -> PathBuf {
    assert_creator_id_safe(creator_id);
    assert_reference_id_safe(reference_id);
    creator_references_root(home, creator_id)
        .join("units")
        .join(reference_id)
}

/// `$HOME/.nexus42/creators/<creator_id>/references/units/<reference_id>/body.md`
///
/// Canonical body file for a reference source (V1.26 reference store layout).
///
/// # Defense-in-depth
///
/// Panics if `creator_id` or `reference_id` contains path traversal components
/// (assertions are applied in [`reference_unit_dir`]).
#[must_use]
pub fn reference_body_path(home: &Path, creator_id: &str, reference_id: &str) -> PathBuf {
    reference_unit_dir(home, creator_id, reference_id).join("body.md")
}

/// Assert that `creator_id` does not contain path-traversal characters.
///
/// This is a low-overhead sanity check; `nexus-domain::is_valid_creator_id()`
/// is the authoritative validator. This catches the most dangerous patterns:
/// `/`, `\`, `..`, and control characters.
fn assert_creator_id_safe(id: &str) {
    for ch in id.chars() {
        assert!(
            ch != '/' && ch != '\\',
            "creator_id contains path separator: {id:?} — this would be a path-traversal vulnerability"
        );
    }
    assert!(
        !id.contains(".."),
        "creator_id contains '..': {id:?} — this would be a path-traversal vulnerability"
    );
    assert!(
        !id.chars().any(char::is_control),
        "creator_id contains control characters: {id:?} — this would be a path-traversal vulnerability"
    );
}

/// Assert that `reference_id` does not contain path-traversal characters.
///
/// Same safety checks as [`assert_creator_id_safe`] but also rejects empty strings,
/// which are not valid reference source IDs.
fn assert_reference_id_safe(id: &str) {
    assert!(!id.is_empty(), "reference_id must not be empty");
    for ch in id.chars() {
        assert!(
            ch != '/' && ch != '\\',
            "reference_id contains path separator: {id:?} — this would be a path-traversal vulnerability"
        );
    }
    assert!(
        !id.contains(".."),
        "reference_id contains '..': {id:?} — this would be a path-traversal vulnerability"
    );
    assert!(
        !id.chars().any(char::is_control),
        "reference_id contains control characters: {id:?} — this would be a path-traversal vulnerability"
    );
}

/// Non-panicking path-traversal validation for `creator_id`.
///
/// Returns `Ok(())` if the ID is safe, or `Err` with a description.
/// Same checks as [`assert_creator_id_safe`] but suitable for fallible call sites
/// (e.g. KB path construction) where panicking is undesirable.
///
/// # Errors
///
/// Returns `Err` if the ID contains `/`, `\`, `..`, or control characters.
pub fn validate_creator_id_safe(id: &str) -> std::result::Result<(), String> {
    for ch in id.chars() {
        if ch == '/' || ch == '\\' {
            return Err(format!(
                "creator_id contains path separator: {id:?} — rejected for safety"
            ));
        }
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

/// `$HOME/.nexus42/rules/writing-craft.md` — user override for Layer 1 rules.
#[must_use]
pub fn user_writing_craft_rules_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home)
        .join("rules")
        .join("writing-craft.md")
}

/// Workspace-relative: `Works/<work_ref>/AGENTS.md` (V1.47 normative Layer 2).
///
/// V1.48 P2: `AGENTS.md` at the Work root is the preferred Layer 2 location
/// per [novel-writing/workflow-profile.md §5.5.4]. The legacy
/// [`work_novel_rules_path`] is retained for read-only fallback only.
#[must_use]
pub fn work_agents_md_path(workspace_dir: &Path, work_ref: &str) -> PathBuf {
    workspace_dir.join("Works").join(work_ref).join("AGENTS.md")
}

/// Workspace-relative: `Works/<work_ref>/Rules/novel-rules.md` (legacy Layer 2).
///
/// V1.48 P2: this path is read-only fallback for Works scaffolded before
/// the `AGENTS.md` migration. New scaffolds write [`work_agents_md_path`]
/// instead. See [novel-writing/workflow-profile.md §5.5.4].
#[must_use]
pub fn work_novel_rules_path(workspace_dir: &Path, work_ref: &str) -> PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Rules")
        .join("novel-rules.md")
}

/// Workspace-relative: `Works/<work_ref>/Rules/novel-rules-history.md` (Layer 3 audit trail).
#[must_use]
pub fn work_novel_rules_history_path(workspace_dir: &Path, work_ref: &str) -> PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Rules")
        .join("novel-rules-history.md")
}

/// Workspace-relative: `Works/<work_ref>/Logs/<subdir>/` (DF-66).
///
/// Returns the path for one of the four Logs subdirectories.
/// `subdir` must be one of: `brainstorm`, `write`, `review`, `publish`.
#[must_use]
pub fn work_logs_subdir(workspace_dir: &Path, work_ref: &str, subdir: &str) -> PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Logs")
        .join(subdir)
}

/// `$HOME/.nexus42/acp/runs` — base directory for run trace storage.
#[must_use]
pub fn acp_runs_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("acp").join("runs")
}

/// `$HOME/.nexus42/acp/runs/<run_id>/` — directory for a specific run.
#[must_use]
pub fn acp_run_dir(home: &Path, run_id: &str) -> PathBuf {
    acp_runs_dir(home).join(run_id)
}

/// `$HOME/.nexus42/acp/runs/<run_id>/trace.jsonl` — trace event log for a run.
#[must_use]
pub fn acp_run_trace_file(home: &Path, run_id: &str) -> PathBuf {
    acp_run_dir(home, run_id).join("trace.jsonl")
}

// ── Workspace path validation (DF-31 skeleton) ─────────────────────────

/// Validate that a workspace-relative `path` is safe to use.
///
/// Rejects:
/// - Empty strings
/// - Absolute paths (starting with `/` or `\`)
/// - Path traversal sequences (`..`)
/// - Control characters
///
/// This is a **local path-safety guard** for the DF-31 workspace.open /
/// workspace.commit skeleton. It does NOT check whether the path exists
/// on disk — callers must do that separately.
///
/// # Errors
///
/// Returns `Err` with a human-readable reason if validation fails.
///
/// # Future expansion (DF-31 → DF-42)
///
/// The current skeleton only validates path-safety. Future iterations
/// (DF-42 full Daemon API redesign — pre-V1.90 historical reference) may add:
/// - Workspace root boundary enforcement (canonicalize + prefix check)
/// - Wildcard / glob pattern support
/// - Symlink resolution
/// - Whitelist-based path allowlisting
pub fn validate_workspace_path_safe(path: &str) -> std::result::Result<(), String> {
    if path.is_empty() {
        return Err("workspace path must not be empty".to_string());
    }
    // Reject absolute paths — workspace.open only accepts relative paths
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(format!(
            "workspace path must be relative, got absolute path: {path:?} — rejected for safety"
        ));
    }
    if path.contains("..") {
        return Err(format!(
            "workspace path contains '..': {path:?} — rejected for safety"
        ));
    }
    if path.chars().any(char::is_control) {
        return Err(format!(
            "workspace path contains control characters: {path:?} — rejected for safety"
        ));
    }
    Ok(())
}

/// Validate that a user-supplied run/capability-call ID is safe to use in a file path.
///
/// Rejects empty strings, path separators (`/`, `\`), traversal sequences (`..`),
/// and control characters.
///
/// # Errors
///
/// Returns `Err` if the ID is empty, contains `/`, `\`, `..`, or control characters.
pub fn validate_run_id_safe(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("run_id must not be empty".to_string());
    }
    for ch in id.chars() {
        if ch == '/' || ch == '\\' {
            return Err(format!(
                "run_id contains path separator: {id:?} — rejected for safety"
            ));
        }
    }
    if id.contains("..") {
        return Err(format!(
            "run_id contains '..': {id:?} — rejected for safety"
        ));
    }
    if id.chars().any(char::is_control) {
        return Err(format!(
            "run_id contains control characters: {id:?} — rejected for safety"
        ));
    }
    Ok(())
}

/// Validate that a user-supplied `entry_id` is safe to use in a file path.
///
/// Rejects path separators (`/`, `\`), traversal sequences (`..`), and control characters.
/// This prevents path-traversal attacks in KB entry operations.
///
/// # Errors
///
/// Returns `Err` if the ID is empty, contains `/`, `\`, `..`, or control characters.
pub fn validate_entry_id_safe(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("entry_id must not be empty".to_string());
    }
    for ch in id.chars() {
        if ch == '/' || ch == '\\' {
            return Err(format!(
                "entry_id contains path separator: {id:?} — rejected for safety"
            ));
        }
    }
    if id.contains("..") {
        return Err(format!(
            "entry_id contains '..': {id:?} — rejected for safety"
        ));
    }
    if id.chars().any(char::is_control) {
        return Err(format!(
            "entry_id contains control characters: {id:?} — rejected for safety"
        ));
    }
    Ok(())
}

/// Non-panicking path-traversal validation for `reference_id`.
///
/// Returns `Ok(())` if the ID is safe, or `Err` with a description.
/// Same checks as [`assert_reference_id_safe`] but suitable for fallible call sites.
///
/// # Errors
///
/// Returns `Err` if the ID is empty or contains `/`, `\`, `..`, or control characters.
pub fn validate_reference_id_safe(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("reference_id must not be empty".to_string());
    }
    for ch in id.chars() {
        if ch == '/' || ch == '\\' {
            return Err(format!(
                "reference_id contains path separator: {id:?} — rejected for safety"
            ));
        }
    }
    if id.contains("..") {
        return Err(format!(
            "reference_id contains '..': {id:?} — rejected for safety"
        ));
    }
    if id.chars().any(char::is_control) {
        return Err(format!(
            "reference_id contains control characters: {id:?} — rejected for safety"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            operational_workspace_dir(&home, "ctr_test", "default"),
            PathBuf::from("/fake/home/.nexus42/creators/ctr_test/workspaces/default")
        );
    }

    #[test]
    fn workspace_state_db_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            workspace_state_db_path(&home, "c", "w"),
            PathBuf::from("/h/.nexus42/creators/c/workspaces/w/state.db")
        );
    }

    #[test]
    fn db_separation_workspace_state_and_shared_global_are_distinct() {
        let home = PathBuf::from("/h");
        let workspace_db = workspace_state_db_path(&home, "c", "w");
        let shared_db = shared_global_db_path(&home);
        assert_ne!(
            workspace_db, shared_db,
            "workspace DB and shared global DB must never resolve to the same path"
        );
    }

    #[test]
    fn user_preset_base_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_preset_base_dir(&home),
            PathBuf::from("/fake/home/.nexus42/presets")
        );
    }

    #[test]
    fn user_preset_bundle_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_preset_bundle_dir(&home, "my-strategy"),
            PathBuf::from("/fake/home/.nexus42/presets/my-strategy")
        );
    }

    #[test]
    fn user_skills_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_skills_dir(&home),
            PathBuf::from("/fake/home/.nexus42/skills")
        );
    }

    #[test]
    fn device_id_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            device_id_path(&home),
            PathBuf::from("/fake/home/.nexus42/device-id")
        );
    }

    #[test]
    fn connect_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            connect_dir(&home),
            PathBuf::from("/fake/home/.nexus42/connect")
        );
        assert_eq!(
            connect_identity_key_path(&home),
            PathBuf::from("/fake/home/.nexus42/connect/identity.key")
        );
        assert_eq!(
            connect_allowlist_path(&home),
            PathBuf::from("/fake/home/.nexus42/connect/allowlist.json")
        );
        assert_eq!(
            connect_issuer_key_path(&home),
            PathBuf::from("/fake/home/.nexus42/connect/issuer.key")
        );
        assert_eq!(
            connect_config_path(&home),
            PathBuf::from("/fake/home/.nexus42/connect/config.json")
        );
        assert_ne!(
            connect_config_path(&home),
            connect_allowlist_path(&home),
            "config.json is DISTINCT from allowlist.json (V1.155 P1 lock #1)"
        );
        assert_ne!(
            connect_issuer_key_path(&home),
            connect_identity_key_path(&home),
            "issuer.key is DISTINCT from identity.key (V1.155 P1 lock #4)"
        );
    }

    #[test]
    fn tls_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(tls_dir(&home), PathBuf::from("/fake/home/.nexus42/tls"));
    }

    #[test]
    fn tls_cert_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            tls_cert_path(&home),
            PathBuf::from("/fake/home/.nexus42/tls/cert.pem")
        );
    }

    #[test]
    fn tls_key_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            tls_key_path(&home),
            PathBuf::from("/fake/home/.nexus42/tls/key.pem")
        );
    }

    #[test]
    fn soul_md_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_soul_md_path(&home, "ctr_test"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/SOUL.md")
        );
    }

    #[test]
    fn creator_inspiration_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_inspiration_dir(&home, "ctr_test", "ws1"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/workspaces/ws1/Pool/Ideas")
        );
    }

    #[test]
    fn creator_kb_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_kb_dir(&home, "ctr_test", "ws1"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/workspaces/ws1/kb")
        );
    }

    #[test]
    fn creator_kb_entries_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_kb_entries_dir(&home, "ctr_test", "ws1"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/workspaces/ws1/kb/entries")
        );
    }

    #[test]
    #[should_panic(expected = "path separator")]
    fn soul_md_path_rejects_forward_slash() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "../../etc/passwd");
    }

    #[test]
    #[should_panic(expected = "path separator")]
    fn soul_md_path_rejects_backslash() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_bad\\etc");
    }

    #[test]
    #[should_panic(expected = "'..'")]
    fn soul_md_path_rejects_dotdot() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_.._secret");
    }

    #[test]
    #[should_panic(expected = "control characters")]
    fn soul_md_path_rejects_control_chars() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_\x00null");
    }

    // ── validate_creator_id_safe tests ────────────────────────

    #[test]
    fn validate_creator_id_safe_accepts_valid() {
        assert!(validate_creator_id_safe("crt_abc123").is_ok());
    }

    #[test]
    fn validate_creator_id_safe_rejects_forward_slash() {
        let err = validate_creator_id_safe("../../etc/passwd").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_creator_id_safe_rejects_backslash() {
        let err = validate_creator_id_safe("ctr_bad\\etc").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_creator_id_safe_rejects_dotdot() {
        let err = validate_creator_id_safe("ctr_.._secret").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn validate_creator_id_safe_rejects_control_chars() {
        let err = validate_creator_id_safe("ctr_\x00null").unwrap_err();
        assert!(err.contains("control characters"));
    }

    // ── validate_entry_id_safe tests ──────────────────────────

    #[test]
    fn validate_entry_id_safe_accepts_valid() {
        assert!(validate_entry_id_safe("kb_a1b2c3d4").is_ok());
    }

    #[test]
    fn validate_entry_id_safe_rejects_empty() {
        let err = validate_entry_id_safe("").unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_entry_id_safe_rejects_dotdot() {
        let err = validate_entry_id_safe("kb_.._secret").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn validate_entry_id_safe_rejects_forward_slash() {
        let err = validate_entry_id_safe("kb_foo/bar").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_entry_id_safe_rejects_backslash() {
        let err = validate_entry_id_safe("kb_foo\\bar").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_entry_id_safe_rejects_control_chars() {
        let err = validate_entry_id_safe("kb_\x01ctrl").unwrap_err();
        assert!(err.contains("control characters"));
    }

    // ── ACP trace path helpers ──────────────────────────────────────────

    #[test]
    fn acp_runs_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            acp_runs_dir(&home),
            PathBuf::from("/fake/home/.nexus42/acp/runs")
        );
    }

    #[test]
    fn acp_run_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            acp_run_dir(&home, "run_abc"),
            PathBuf::from("/fake/home/.nexus42/acp/runs/run_abc")
        );
    }

    #[test]
    fn acp_run_trace_file_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            acp_run_trace_file(&home, "run_abc"),
            PathBuf::from("/fake/home/.nexus42/acp/runs/run_abc/trace.jsonl")
        );
    }

    // ── validate_run_id_safe tests ──────────────────────────────────────

    #[test]
    fn validate_run_id_safe_accepts_valid() {
        assert!(validate_run_id_safe("run_abcdef0123456789").is_ok());
    }

    #[test]
    fn validate_run_id_safe_rejects_empty() {
        let err = validate_run_id_safe("").unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_run_id_safe_rejects_forward_slash() {
        let err = validate_run_id_safe("../../etc/passwd").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_run_id_safe_rejects_backslash() {
        let err = validate_run_id_safe("run_bad\\etc").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_run_id_safe_rejects_dotdot() {
        let err = validate_run_id_safe("run_.._secret").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn validate_run_id_safe_rejects_control_chars() {
        let err = validate_run_id_safe("run_\x00null").unwrap_err();
        assert!(err.contains("control characters"));
    }

    // ── Reference store path helpers ───────────────────────────────────────

    #[test]
    fn creator_references_root_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_references_root(&home, "ctr_test"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/references")
        );
    }

    #[test]
    fn reference_unit_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            reference_unit_dir(&home, "ctr_test", "ref_abc123"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/references/units/ref_abc123")
        );
    }

    #[test]
    fn reference_body_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            reference_body_path(&home, "ctr_test", "ref_abc123"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/references/units/ref_abc123/body.md")
        );
    }

    // ── Reference path defense-in-depth (creator_id) ───────────────────────

    #[test]
    #[should_panic(expected = "path separator")]
    fn reference_path_rejects_traversal_creator_id_slash() {
        let home = PathBuf::from("/h");
        let _ = creator_references_root(&home, "../../etc/passwd");
    }

    #[test]
    #[should_panic(expected = "'..'")]
    fn reference_path_rejects_traversal_creator_id_dotdot() {
        let home = PathBuf::from("/h");
        let _ = reference_unit_dir(&home, "ctr_.._bad", "ref_ok");
    }

    // ── Reference path defense-in-depth (reference_id) ─────────────────────

    #[test]
    #[should_panic(expected = "must not be empty")]
    fn reference_path_rejects_empty_reference_id() {
        let home = PathBuf::from("/h");
        let _ = reference_unit_dir(&home, "ctr_ok", "");
    }

    #[test]
    #[should_panic(expected = "path separator")]
    fn reference_path_rejects_traversal_reference_id_slash() {
        let home = PathBuf::from("/h");
        let _ = reference_unit_dir(&home, "ctr_ok", "../../etc/passwd");
    }

    #[test]
    #[should_panic(expected = "'..'")]
    fn reference_path_rejects_traversal_reference_id_dotdot() {
        let home = PathBuf::from("/h");
        let _ = reference_body_path(&home, "ctr_ok", "ref_.._bad");
    }

    #[test]
    #[should_panic(expected = "control characters")]
    fn reference_path_rejects_control_char_reference_id() {
        let home = PathBuf::from("/h");
        let _ = reference_unit_dir(&home, "ctr_ok", "ref_\x00null");
    }

    // ── validate_reference_id_safe tests ───────────────────────────────────

    #[test]
    fn validate_reference_id_safe_accepts_valid() {
        assert!(validate_reference_id_safe("ref_abc123").is_ok());
    }

    #[test]
    fn validate_reference_id_safe_rejects_empty() {
        let err = validate_reference_id_safe("").unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_reference_id_safe_rejects_forward_slash() {
        let err = validate_reference_id_safe("ref_foo/bar").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_reference_id_safe_rejects_backslash() {
        let err = validate_reference_id_safe("ref_foo\\bar").unwrap_err();
        assert!(err.contains("path separator"));
    }

    #[test]
    fn validate_reference_id_safe_rejects_dotdot() {
        let err = validate_reference_id_safe("ref_.._secret").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn validate_reference_id_safe_rejects_control_chars() {
        let err = validate_reference_id_safe("ref_\x01ctrl").unwrap_err();
        assert!(err.contains("control characters"));
    }

    // ── Workspace path validation (DF-31 skeleton) ────────────────────────

    #[test]
    fn validate_workspace_path_safe_accepts_valid() {
        assert!(validate_workspace_path_safe("Works/my-novel").is_ok());
        assert!(validate_workspace_path_safe("data/config.json").is_ok());
        assert!(validate_workspace_path_safe("subdir").is_ok());
    }

    #[test]
    fn validate_workspace_path_safe_rejects_empty() {
        let err = validate_workspace_path_safe("").unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_workspace_path_safe_rejects_absolute() {
        let err = validate_workspace_path_safe("/etc/passwd").unwrap_err();
        assert!(err.contains("absolute path"));
    }

    #[test]
    fn validate_workspace_path_safe_rejects_absolute_backslash() {
        let err = validate_workspace_path_safe("\\windows\\system32").unwrap_err();
        assert!(err.contains("absolute path"));
    }

    #[test]
    fn validate_workspace_path_safe_rejects_dotdot() {
        let err = validate_workspace_path_safe("../../etc/passwd").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn validate_workspace_path_safe_rejects_control_chars() {
        let err = validate_workspace_path_safe("path_\x00null").unwrap_err();
        assert!(err.contains("control characters"));
    }

    // ── Rules and Logs path helpers (V1.39 P3) ───────────────────────────

    #[test]
    fn user_writing_craft_rules_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            user_writing_craft_rules_path(&home),
            PathBuf::from("/h/.nexus42/rules/writing-craft.md")
        );
    }

    #[test]
    fn work_novel_rules_path_layout() {
        let ws = PathBuf::from("/ws");
        assert_eq!(
            work_novel_rules_path(&ws, "my-novel"),
            PathBuf::from("/ws/Works/my-novel/Rules/novel-rules.md")
        );
    }

    #[test]
    fn work_agents_md_path_layout() {
        let ws = PathBuf::from("/ws");
        assert_eq!(
            work_agents_md_path(&ws, "my-novel"),
            PathBuf::from("/ws/Works/my-novel/AGENTS.md")
        );
    }

    #[test]
    fn work_novel_rules_history_path_layout() {
        let ws = PathBuf::from("/ws");
        assert_eq!(
            work_novel_rules_history_path(&ws, "my-novel"),
            PathBuf::from("/ws/Works/my-novel/Rules/novel-rules-history.md")
        );
    }

    #[test]
    fn work_logs_subdir_layout() {
        let ws = PathBuf::from("/ws");
        assert_eq!(
            work_logs_subdir(&ws, "my-novel", "brainstorm"),
            PathBuf::from("/ws/Works/my-novel/Logs/brainstorm")
        );
        assert_eq!(
            work_logs_subdir(&ws, "my-novel", "write"),
            PathBuf::from("/ws/Works/my-novel/Logs/write")
        );
    }
}
