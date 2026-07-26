//! Process PATH enrichment for agent CLI discovery.
//!
//! macOS GUI apps (including the Tauri desktop shell) often inherit a minimal
//! PATH such as `/usr/bin:/bin:/usr/sbin:/sbin`. Homebrew and user-local agent
//! CLIs under `/opt/homebrew/bin` or `~/.local/bin` are then invisible to
//! `which::which` during `POST /v1/daemon/agent-host/scan`.
//!
//! This module merges a login-shell-equivalent set of common user bin dirs into
//! the process PATH **once at daemon boot**, before any scan probe runs. No
//! shell-out; no wire/schema change (V1.101 Class B).

use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Version-manager name used for the enrichment diagnostics field.
type ManagerName = &'static str;

/// Resolve the active Node.js bin directory for nvm without shelling out.
///
/// 1. Uses `$NVM_DIR` if set, otherwise falls back to `~/.nvm`.
/// 2. Reads `<nvm_root>/alias/default` and follows up to two alias hops.
/// 3. If `<nvm_root>/versions/node/<target>/bin` exists, returns it.
/// 4. Otherwise globs `versions/node/*/bin` and returns the single
///    highest-semver match (never all matches).
fn resolve_nvm_bin() -> Option<PathBuf> {
    let nvm_root = env::var_os("NVM_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".nvm")))?;
    resolve_nvm_alias_or_glob(&nvm_root)
}

fn resolve_nvm_alias_or_glob(nvm_root: &Path) -> Option<PathBuf> {
    let active = nvm_root.join("alias").join("default");
    if let Some(target) = std::fs::read_to_string(&active)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        if let Some(bin) = resolve_nvm_alias_target(nvm_root, &target, 0) {
            if bin.is_dir() {
                return Some(bin);
            }
        }
    }
    highest_semver_nvm_bin(nvm_root)
}

fn resolve_nvm_alias_target(nvm_root: &Path, target: &str, depth: u8) -> Option<PathBuf> {
    if depth > 2 {
        return None;
    }
    let bin = nvm_root
        .join("versions")
        .join("node")
        .join(target)
        .join("bin");
    if bin.is_dir() {
        return Some(bin);
    }
    // Target may itself be an alias (e.g. `default -> lts/iron -> v20.11.0`).
    let alias = nvm_root.join("alias").join(target);
    let next = std::fs::read_to_string(&alias)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    resolve_nvm_alias_target(nvm_root, &next, depth + 1)
}

fn highest_semver_nvm_bin(nvm_root: &Path) -> Option<PathBuf> {
    let versions = nvm_root.join("versions").join("node");
    let mut best: Option<(u64, u64, u64, PathBuf)> = None;
    for entry in std::fs::read_dir(&versions).ok()? {
        let Ok(entry) = entry else { continue };
        let bin = entry.path().join("bin");
        if !bin.is_dir() {
            continue;
        }
        let Some(version) = parse_semver_prefix(&entry.file_name().to_string_lossy()) else {
            continue;
        };
        best = Some(match best {
            None => (version.0, version.1, version.2, bin),
            Some((maj, min, patch, _)) if version > (maj, min, patch) => {
                (version.0, version.1, version.2, bin)
            }
            Some(existing) => existing,
        });
    }
    best.map(|(_, _, _, bin)| bin)
}

fn parse_semver_prefix(name: &str) -> Option<(u64, u64, u64)> {
    let stripped = name.strip_prefix('v').unwrap_or(name);
    let mut parts = stripped.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch_part = parts.next()?;
    // Stop at the first non-digit so `20.11.0-rc.1` parses as (20, 11, 0).
    let patch_digits: String = patch_part
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let patch = patch_digits.parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn resolve_volta_bin(home: &Path) -> Option<PathBuf> {
    let volta_home = env::var_os("VOLTA_HOME").map_or_else(|| home.join(".volta"), PathBuf::from);
    let bin = volta_home.join("bin");
    if bin.is_dir() {
        Some(bin)
    } else {
        None
    }
}

fn resolve_fnm_bin(home: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let primary = home.join("Library/Application Support/fnm/aliases/default/bin");
    #[cfg(target_os = "linux")]
    let primary = home.join(".local/share/fnm/aliases/default/bin");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let primary = home.join(".fnm/current/bin");

    if primary.is_dir() {
        return Some(primary);
    }
    let fallback = home.join(".fnm/current/bin");
    if fallback.is_dir() {
        Some(fallback)
    } else {
        None
    }
}

fn resolve_pnpm_bin(home: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let default = home.join("Library/pnpm");
    #[cfg(target_os = "linux")]
    let default = home.join(".local/share/pnpm");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let default = home.join(".local/share/pnpm");

    let pnpm_home = env::var_os("PNPM_HOME").map_or(default, PathBuf::from);
    if pnpm_home.is_dir() {
        Some(pnpm_home)
    } else {
        None
    }
}

/// Common user / package-manager bin directories that login shells typically
/// include but GUI-launched processes often omit.
///
/// Only directories that currently exist are returned so PATH stays free of
/// dead entries on machines without Homebrew / cargo / etc.
#[must_use]
pub fn login_equivalent_bin_dirs() -> Vec<PathBuf> {
    login_equivalent_bin_dirs_with_sources()
        .into_iter()
        .map(|(path, _manager)| path)
        .collect()
}

fn login_equivalent_bin_dirs_with_sources() -> Vec<(PathBuf, Option<ManagerName>)> {
    let mut dirs = Vec::new();

    if let Some(home) = dirs::home_dir() {
        for rel in [
            ".local/bin",
            "bin",
            ".cargo/bin",
            ".npm-global/bin",
            ".bun/bin",
            // Vendor agent CLIs that install outside Homebrew / npm-global.
            ".kimi-code/bin",
            // Version-manager shims (existence-gated; QC B5).
            ".asdf/shims",
            ".local/share/mise/shims",
        ] {
            let candidate = home.join(rel);
            if candidate.is_dir() {
                dirs.push((candidate, None));
            }
        }

        if let Some(dir) = resolve_volta_bin(&home) {
            dirs.push((dir, Some("volta")));
        }
        if let Some(dir) = resolve_fnm_bin(&home) {
            dirs.push((dir, Some("fnm")));
        }
        if let Some(dir) = resolve_pnpm_bin(&home) {
            dirs.push((dir, Some("pnpm")));
        }
        let yarn = home.join(".yarn/bin");
        if yarn.is_dir() {
            dirs.push((yarn, Some("yarn")));
        }
    }

    if let Some(dir) = resolve_nvm_bin() {
        dirs.push((dir, Some("nvm")));
    }

    #[cfg(target_os = "macos")]
    {
        for absolute in [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
        ] {
            let candidate = PathBuf::from(absolute);
            if candidate.is_dir() {
                dirs.push((candidate, None));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for absolute in [
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/snap/bin",
        ] {
            let candidate = PathBuf::from(absolute);
            if candidate.is_dir() {
                dirs.push((candidate, None));
            }
        }
    }

    #[cfg(windows)]
    {
        // Windows GUI PATH is usually already complete; keep a light touch.
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            let npm = PathBuf::from(local).join("npm");
            if npm.is_dir() {
                dirs.push((npm, None));
            }
        }
    }

    dirs
}

/// Merge `extra_dirs` (prepended, de-duplicated) with an existing PATH value.
///
/// Existing entries keep their relative order after any newly prepended dirs.
/// Duplicates (by string equality of the path component) are skipped.
///
/// # Errors
///
/// Returns [`env::JoinPathsError`] when any collected component contains the
/// OS path-separator character. Callers must not treat that as "already
/// enriched" — the merge did not succeed.
pub fn merge_path(
    existing: Option<&OsStr>,
    extra_dirs: impl IntoIterator<Item = PathBuf>,
) -> Result<OsString, env::JoinPathsError> {
    let mut ordered: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for dir in extra_dirs {
        let key = dir.to_string_lossy().into_owned();
        if seen.insert(key) {
            ordered.push(dir);
        }
    }

    if let Some(existing) = existing {
        for dir in env::split_paths(existing) {
            let key = dir.to_string_lossy().into_owned();
            if seen.insert(key) {
                ordered.push(dir);
            }
        }
    }

    env::join_paths(&ordered)
}

/// Build the directory list used for agent CLI PATH probes at scan time.
///
/// Order: current process `PATH` entries first (so test isolation and
/// user overrides win), then [`login_equivalent_bin_dirs`] that are not
/// already present. Unlike [`apply_process_path_enrichment`], this does
/// **not** mutate the process environment — safe to call from a live
/// Tokio runtime (e.g. the scan handler).
#[must_use]
pub fn probe_path_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(existing) = env::var_os("PATH") {
        for dir in env::split_paths(&existing) {
            let key = dir.to_string_lossy().into_owned();
            if seen.insert(key) {
                dirs.push(dir);
            }
        }
    }

    for dir in login_equivalent_bin_dirs() {
        let key = dir.to_string_lossy().into_owned();
        if seen.insert(key) {
            dirs.push(dir);
        }
    }

    dirs
}

/// Enrich the current process `PATH` with [`login_equivalent_bin_dirs`].
///
/// Idempotent: directories already present are not duplicated.
///
/// **Call before starting a Tokio multi-threaded runtime.** On POSIX,
/// `setenv(3)` is not thread-safe against concurrent `getenv(3)`; the
/// `nexus42` binary invokes this from sync `main` before
/// `tokio::runtime::Runtime::new`. Do not call from inside an already-running
/// async runtime (including [`crate::boot::run_daemon`]).
pub fn apply_process_path_enrichment() {
    let existing = env::var_os("PATH");
    let sources = login_equivalent_bin_dirs_with_sources();
    if sources.is_empty() {
        return;
    }

    let extras: Vec<PathBuf> = sources
        .iter()
        .map(|(path, _manager)| path.clone())
        .collect();
    let managers: Vec<&str> = sources
        .iter()
        .filter_map(|(_path, manager)| *manager)
        .collect();

    let before_count = existing.as_ref().map_or(0, |p| env::split_paths(p).count());
    let enriched = match merge_path(existing.as_deref(), extras) {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "PATH enrichment: join_paths failed; leaving PATH unchanged"
            );
            return;
        }
    };
    let after_count = env::split_paths(&enriched).count();

    // Process-global PATH update before Tokio (and before any agent scan).
    // Concurrent tests that mutate PATH must serialize (see module tests).
    // Workspace forbids `unsafe`, so we use the safe `set_var` call — but only
    // from a single-threaded context (binary `main` before Runtime::new).
    env::set_var("PATH", &enriched);

    if after_count > before_count {
        tracing::info!(
            before = before_count,
            after = after_count,
            added = after_count - before_count,
            vm_managers = ?managers,
            "Enriched process PATH with login-equivalent user bin dirs for agent discovery"
        );
    } else {
        tracing::debug!(
            entries = after_count,
            "Process PATH already includes login-equivalent user bin dirs"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static PATH_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Remove version-manager env vars that would override the temp HOME path,
    /// returning their previous values so callers can restore them.
    fn stash_manager_env_vars() -> [Option<OsString>; 3] {
        let nvm = env::var_os("NVM_DIR");
        let volta = env::var_os("VOLTA_HOME");
        let pnpm = env::var_os("PNPM_HOME");
        env::remove_var("NVM_DIR");
        env::remove_var("VOLTA_HOME");
        env::remove_var("PNPM_HOME");
        [nvm, volta, pnpm]
    }

    fn restore_manager_env_vars(stashed: [Option<OsString>; 3]) {
        let [nvm, volta, pnpm] = stashed;
        match nvm {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
        match volta {
            Some(p) => env::set_var("VOLTA_HOME", p),
            None => env::remove_var("VOLTA_HOME"),
        }
        match pnpm {
            Some(p) => env::set_var("PNPM_HOME", p),
            None => env::remove_var("PNPM_HOME"),
        }
    }

    #[test]
    fn merge_path_prepends_extras_and_dedupes() {
        let existing = OsString::from("/usr/bin:/bin:/opt/homebrew/bin");
        let extras = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/custom/bin"),
        ];
        let merged = merge_path(Some(existing.as_os_str()), extras).unwrap();
        let parts: Vec<_> = env::split_paths(&merged).collect();
        assert_eq!(parts[0], PathBuf::from("/opt/homebrew/bin"));
        assert_eq!(parts[1], PathBuf::from("/custom/bin"));
        assert_eq!(parts[2], PathBuf::from("/usr/bin"));
        assert_eq!(parts[3], PathBuf::from("/bin"));
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.as_os_str() == "/opt/homebrew/bin")
                .count(),
            1,
            "homebrew bin must appear once"
        );
    }

    #[test]
    fn merge_path_handles_empty_existing() {
        let extras = vec![PathBuf::from("/opt/homebrew/bin")];
        let merged = merge_path(None, extras).unwrap();
        let parts: Vec<_> = env::split_paths(&merged).collect();
        assert_eq!(parts, vec![PathBuf::from("/opt/homebrew/bin")]);
    }

    #[cfg(unix)]
    #[test]
    fn enrichment_makes_binary_resolvable_via_which() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().to_path_buf();
        let binary = bin_dir.join("nexus-path-probe-agent");
        fs::write(&binary, b"#!/bin/sh\necho ok\n").unwrap();
        let mut perms = fs::metadata(&binary).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary, perms).unwrap();

        let previous = env::var_os("PATH");
        // Simulate a stripped macOS GUI PATH that cannot see the temp bin.
        env::set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        assert!(
            which::which("nexus-path-probe-agent").is_err(),
            "stripped PATH must not resolve the probe binary"
        );

        let enriched = merge_path(env::var_os("PATH").as_deref(), vec![bin_dir]).unwrap();
        env::set_var("PATH", &enriched);
        let found = which::which("nexus-path-probe-agent");
        assert!(
            found.is_ok(),
            "enriched PATH must resolve the probe binary: {found:?}"
        );
        assert_eq!(found.unwrap(), binary);

        // Restore.
        match previous {
            Some(p) => env::set_var("PATH", p),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn apply_process_path_enrichment_is_idempotent() {
        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("PATH");
        env::set_var("PATH", "/usr/bin:/bin");
        apply_process_path_enrichment();
        let once = env::var_os("PATH").unwrap();
        apply_process_path_enrichment();
        let twice = env::var_os("PATH").unwrap();
        assert_eq!(once, twice, "second enrichment must not duplicate entries");

        match previous {
            Some(p) => env::set_var("PATH", p),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn login_equivalent_bin_dirs_includes_asdf_and_mise_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let asdf = home.join(".asdf/shims");
        let mise = home.join(".local/share/mise/shims");
        std::fs::create_dir_all(&asdf).unwrap();
        std::fs::create_dir_all(&mise).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        env::set_var("HOME", home);

        let dirs = login_equivalent_bin_dirs();
        assert!(
            dirs.iter().any(|d| d.ends_with(".asdf/shims")),
            "expected asdf shims in {dirs:?}"
        );
        assert!(
            dirs.iter().any(|d| d.ends_with(".local/share/mise/shims")),
            "expected mise shims in {dirs:?}"
        );

        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn login_equivalent_bin_dirs_includes_kimi_code_bin_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let kimi = home.join(".kimi-code/bin");
        std::fs::create_dir_all(&kimi).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        let stashed = stash_manager_env_vars();
        env::set_var("HOME", home);

        let dirs = login_equivalent_bin_dirs();
        assert!(
            dirs.iter().any(|d| d.ends_with(".kimi-code/bin")),
            "expected ~/.kimi-code/bin in {dirs:?}"
        );

        restore_manager_env_vars(stashed);
        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn probe_path_dirs_keeps_process_path_before_enrichment() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let kimi = home.join(".kimi-code/bin");
        std::fs::create_dir_all(&kimi).unwrap();
        let isolated = home.join("isolated-bin");
        std::fs::create_dir_all(&isolated).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        let previous_path = env::var_os("PATH");
        let stashed = stash_manager_env_vars();
        env::set_var("HOME", home);
        env::set_var("PATH", &isolated);

        let dirs = probe_path_dirs();
        assert_eq!(
            dirs.first().map(PathBuf::as_path),
            Some(isolated.as_path()),
            "process PATH entry must stay first for test isolation / overrides"
        );
        assert!(
            dirs.iter().any(|d| d == &kimi),
            "enrichment dirs must be appended: {dirs:?}"
        );

        restore_manager_env_vars(stashed);
        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
        match previous_path {
            Some(p) => env::set_var("PATH", p),
            None => env::remove_var("PATH"),
        }
    }

    #[test]
    fn resolve_nvm_bin_reads_default_alias() {
        let tmp = tempfile::tempdir().unwrap();
        let nvm_root = tmp.path();
        let bin_dir = nvm_root.join("versions/node/v20.11.0/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(nvm_root.join("alias")).unwrap();
        std::fs::write(nvm_root.join("alias/default"), "v20.11.0\n").unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("NVM_DIR");
        env::set_var("NVM_DIR", nvm_root);

        let resolved = resolve_nvm_bin();
        assert_eq!(resolved, Some(bin_dir));

        match previous {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
    }

    #[test]
    fn resolve_nvm_bin_follows_alias_hops() {
        let tmp = tempfile::tempdir().unwrap();
        let nvm_root = tmp.path();
        let bin_dir = nvm_root.join("versions/node/v20.11.0/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(nvm_root.join("alias/lts")).unwrap();
        std::fs::write(nvm_root.join("alias/default"), "lts/iron\n").unwrap();
        std::fs::write(nvm_root.join("alias/lts/iron"), "v20.11.0\n").unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("NVM_DIR");
        env::set_var("NVM_DIR", nvm_root);

        let resolved = resolve_nvm_bin();
        assert_eq!(resolved, Some(bin_dir));

        match previous {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
    }

    #[test]
    fn resolve_nvm_bin_falls_back_to_highest_semver_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let nvm_root = tmp.path();
        let old_bin = nvm_root.join("versions/node/v18.20.2/bin");
        let new_bin = nvm_root.join("versions/node/v20.11.0/bin");
        std::fs::create_dir_all(&old_bin).unwrap();
        std::fs::create_dir_all(&new_bin).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("NVM_DIR");
        env::set_var("NVM_DIR", nvm_root);

        let resolved = resolve_nvm_bin();
        assert_eq!(resolved, Some(new_bin));

        match previous {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
    }

    #[test]
    fn resolve_nvm_bin_ignores_non_semver_entries_in_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let nvm_root = tmp.path();
        let valid_bin = nvm_root.join("versions/node/v20.11.0/bin");
        let current_bin = nvm_root.join("versions/node/current/bin");
        std::fs::create_dir_all(&valid_bin).unwrap();
        // `current` symlink target does not matter; the name itself is non-semver.
        std::fs::create_dir_all(&current_bin).unwrap();
        std::fs::write(nvm_root.join("versions/node/.DS_Store"), b"junk").unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("NVM_DIR");
        env::set_var("NVM_DIR", nvm_root);

        let resolved = resolve_nvm_bin();
        assert_eq!(
            resolved,
            Some(valid_bin),
            "non-semver entries must not abort the glob"
        );

        match previous {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
    }

    #[test]
    fn resolve_nvm_bin_handles_bare_version_names() {
        let tmp = tempfile::tempdir().unwrap();
        let nvm_root = tmp.path();
        let bin_dir = nvm_root.join("versions/node/20.11.0/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("NVM_DIR");
        env::set_var("NVM_DIR", nvm_root);

        let resolved = resolve_nvm_bin();
        assert_eq!(resolved, Some(bin_dir));

        match previous {
            Some(p) => env::set_var("NVM_DIR", p),
            None => env::remove_var("NVM_DIR"),
        }
    }

    #[test]
    fn login_equivalent_bin_dirs_includes_version_managers_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let volta = home.join(".volta/bin");
        let yarn = home.join(".yarn/bin");
        std::fs::create_dir_all(&volta).unwrap();
        std::fs::create_dir_all(&yarn).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        let previous_managers = stash_manager_env_vars();
        env::set_var("HOME", home);

        let dirs = login_equivalent_bin_dirs();
        assert!(dirs.contains(&volta), "expected volta in {dirs:?}");
        assert!(dirs.contains(&yarn), "expected yarn in {dirs:?}");

        restore_manager_env_vars(previous_managers);
        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn login_equivalent_bin_dirs_includes_fnm_and_pnpm_on_macos() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let fnm = home.join("Library/Application Support/fnm/aliases/default/bin");
        let pnpm = home.join("Library/pnpm");
        std::fs::create_dir_all(&fnm).unwrap();
        std::fs::create_dir_all(&pnpm).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        let previous_managers = stash_manager_env_vars();
        env::set_var("HOME", home);

        let dirs = login_equivalent_bin_dirs();
        assert!(dirs.contains(&fnm), "expected fnm in {dirs:?}");
        assert!(dirs.contains(&pnpm), "expected pnpm in {dirs:?}");

        restore_manager_env_vars(previous_managers);
        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn login_equivalent_bin_dirs_tracks_manager_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let volta = home.join(".volta/bin");
        let yarn = home.join(".yarn/bin");
        std::fs::create_dir_all(&volta).unwrap();
        std::fs::create_dir_all(&yarn).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous_home = env::var_os("HOME");
        let previous_managers = stash_manager_env_vars();
        env::set_var("HOME", home);

        let sources = login_equivalent_bin_dirs_with_sources();
        let managers: Vec<_> = sources.iter().filter_map(|(_, m)| *m).collect();
        assert!(
            managers.contains(&"volta"),
            "expected volta manager in {managers:?}"
        );
        assert!(
            managers.contains(&"yarn"),
            "expected yarn manager in {managers:?}"
        );

        restore_manager_env_vars(previous_managers);
        match previous_home {
            Some(p) => env::set_var("HOME", p),
            None => env::remove_var("HOME"),
        }
    }

    #[test]
    fn resolve_volta_bin_honors_volta_home() {
        let tmp = tempfile::tempdir().unwrap();
        let volta_home = tmp.path().join("custom-volta");
        let bin = volta_home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let _guard = PATH_TEST_LOCK.lock().unwrap();
        let previous = env::var_os("VOLTA_HOME");
        env::set_var("VOLTA_HOME", &volta_home);

        let resolved = resolve_volta_bin(&dirs::home_dir().unwrap_or_default());
        assert_eq!(resolved, Some(bin));

        match previous {
            Some(p) => env::set_var("VOLTA_HOME", p),
            None => env::remove_var("VOLTA_HOME"),
        }
    }
}
