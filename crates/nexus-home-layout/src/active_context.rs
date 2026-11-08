//! Active creator / workspace selection readers (`~/.nexus42/config.toml`).
//!
//! Single implementation consumed by daemon, core, and CLI — includes default
//! workspace slug fallback and legacy `config.json` migration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// On-disk `[memory_provider]` table of `~/.nexus42/config.toml`.
///
/// Shape contract: `n3-provider-binding.md` §2.2. Remote carries only
/// references — never an identity seed, token, capability proof or credential
/// body. Absent table and `kind = "local"` both select the local provider.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MemoryProviderConfig {
    /// Built-in local provider.
    Local,
    /// Remote Connect provider reference.
    Remote {
        /// Provider space the selected authority serves.
        space_id: String,
        /// Operator-provisioned credential file id (not the secret itself).
        credential_id: String,
        /// `wss://` endpoint, or `ws://` for loopback tests only.
        connect_endpoint: String,
        /// Remote Ed25519 public key, 64 lowercase hex characters.
        remote_pubkey_hex: String,
        /// Trusted remote peer ids (fail-closed handshake).
        allowlist: Vec<String>,
    },
}

/// Subset of CLI config persisted under `~/.nexus42/config.toml`.
///
/// The `[memory_provider]` table is deliberately not part of this snapshot:
/// it is written and read by the memory-provider owner only
/// ([`read_memory_provider_config`]), so a malformed provider table cannot
/// change creator or workspace resolution for unrelated readers.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CliConfigSnapshot {
    #[serde(default)]
    pub active_creator_id: Option<String>,
    #[serde(default)]
    pub active_workspace_slug_by_creator: HashMap<String, String>,
}

impl CliConfigSnapshot {
    /// Load from `config.toml` with automatic migration from legacy `config.json`.
    ///
    /// A provisioned `config.toml` is read without any write or lock. The legacy
    /// migration publishes `config.toml`, so it takes the shared writer lock and
    /// re-checks the TOML inside it — a writer that committed while the
    /// acquisition waited wins over the stale legacy snapshot (the CLI's own
    /// migration has always done this, `apps/nexus42/src/config.rs`).
    ///
    /// # Errors
    ///
    /// Returns an error when reading `config.toml` or the legacy `config.json`
    /// fails, when `config.toml` contains invalid TOML, or when migrating a
    /// valid `config.json` fails to take the writer lock, write `config.toml` or
    /// rename the legacy file to `config.json.migrated`. Missing config files
    /// are not errors.
    pub fn load(nexus_root: &Path) -> Result<Self, anyhow::Error> {
        let toml_path = nexus_root.join("config.toml");
        let json_path = nexus_root.join("config.json");

        if toml_path.exists() {
            return Self::read_document(&toml_path);
        }

        if json_path.exists() {
            return Self::migrate_legacy_json(nexus_root, &toml_path, &json_path);
        }

        Ok(Self::default())
    }

    /// Read `config.toml` (absent/empty → defaults).
    fn read_document(toml_path: &Path) -> Result<Self, anyhow::Error> {
        let text = std::fs::read_to_string(toml_path)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(toml::from_str(&text)?)
    }

    /// Migrate legacy `config.json` under the shared writer lock.
    ///
    /// The migration replaces the whole shared document, so it must not run
    /// beside a locked writer: it acquires the same lock and re-checks the TOML
    /// inside it, and a concurrent commit that landed first is returned instead
    /// of overwritten.
    fn migrate_legacy_json(
        nexus_root: &Path,
        toml_path: &Path,
        json_path: &Path,
    ) -> Result<Self, anyhow::Error> {
        #[cfg(test)]
        migration_rendezvous::hold(nexus_root);
        let _lock = crate::config_lock::acquire(nexus_root)?;
        if toml_path.exists() {
            return Self::read_document(toml_path);
        }
        let text = match std::fs::read_to_string(json_path) {
            Ok(text) => text,
            // A concurrent loader can pass the same pre-lock check and complete
            // an *empty* legacy migration first: it renames the JSON away and
            // creates no TOML. Under this lock that is the documented "no legacy
            // document" state, not a read fault. Every other I/O error (and
            // every parse error below) is still returned unchanged.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        if text.trim().is_empty() {
            std::fs::rename(json_path, nexus_root.join("config.json.migrated"))?;
            return Ok(Self::default());
        }
        match serde_json::from_str::<Self>(&text) {
            Ok(cfg) => {
                let toml_str = toml::to_string_pretty(&cfg)?;
                // Publish atomically: a concurrent reader (including the
                // lock-free `ReadOnly` snapshot, I-000291) must never see a
                // truncated `config.toml` from this legacy migration.
                crate::config_lock::publish_document(nexus_root, &toml_str)?;
                std::fs::rename(json_path, nexus_root.join("config.json.migrated"))?;
                Ok(cfg)
            }
            Err(_) => Ok(Self::default()),
        }
    }

    #[must_use]
    pub fn workspace_slug_for_creator(&self, creator_id: &str) -> String {
        self.active_workspace_slug_by_creator
            .get(creator_id)
            .map(std::string::String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_WORKSPACE_SLUG)
            .to_string()
    }
}

/// Resolve workspace `state.db` under ADR-014.
///
/// # Errors
///
/// Returns an error when loading the CLI config fails (see
/// [`CliConfigSnapshot::load`]) or when no active creator is configured.
pub fn resolve_state_db_path(
    user_home: &Path,
    nexus_root: &Path,
) -> Result<PathBuf, anyhow::Error> {
    let cfg = CliConfigSnapshot::load(nexus_root)?;
    let cid = cfg.active_creator_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No active creator in ~/.nexus42/config.toml. Run `nexus42 init workspace` or `nexus42 creator use <id>`."
        )
    })?;
    let slug = cfg.workspace_slug_for_creator(cid);
    Ok(crate::workspace_state_db_path(user_home, cid, &slug))
}

/// Read the active `creator_id` from CLI config (migration-aware).
#[must_use]
pub fn read_active_creator_id(nexus_home: &Path) -> Option<String> {
    if !nexus_home.is_absolute()
        || nexus_home
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return None;
    }
    CliConfigSnapshot::load(nexus_home)
        .ok()
        .and_then(|cfg| cfg.active_creator_id)
}

/// Read the active workspace slug for `creator_id` (default `"default"`).
#[must_use]
pub fn read_active_workspace_slug(nexus_home: &Path, creator_id: &str) -> Option<String> {
    Some(
        CliConfigSnapshot::load(nexus_home)
            .unwrap_or_default()
            .workspace_slug_for_creator(creator_id),
    )
}

/// Read the optional `[memory_provider]` table.
///
/// Independent of [`CliConfigSnapshot`] so a malformed provider table fails
/// provider selection only, never creator/workspace resolution. An absent
/// table is the implicit local default, not an error.
///
/// # Errors
///
/// Returns an error when `config.toml` cannot be inspected, read or parsed, or
/// when the provider table does not match [`MemoryProviderConfig`].
pub fn read_memory_provider_config(
    nexus_root: &Path,
) -> Result<Option<MemoryProviderConfig>, anyhow::Error> {
    let path = nexus_root.join("config.toml");
    // I-000304: only a genuinely absent directory entry is the implicit local
    // default. `metadata` follows a final symlink, so an existing config link
    // whose target is gone also answers `NotFound`; classify the entry itself,
    // then let the read fail closed for a link that no longer resolves. Every
    // other filesystem error keeps the caller's failure path.
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let text = std::fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let document: toml::Table = toml::from_str(&text)?;
    let Some(table) = document.get("memory_provider") else {
        return Ok(None);
    };
    table.clone().try_into().map(Some).map_err(|error| {
        anyhow::anyhow!("invalid [memory_provider] in {}: {error}", path.display())
    })
}

/// Read `active_creator_id` without failing on missing config.
#[must_use]
pub fn try_active_creator_id(nexus_root: &Path) -> Option<String> {
    CliConfigSnapshot::load(nexus_root)
        .ok()
        .and_then(|cfg| cfg.active_creator_id)
}

/// Returns `None` when no creator is selected.
#[must_use]
pub fn try_resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> Option<PathBuf> {
    let cfg = CliConfigSnapshot::load(nexus_root).ok()?;
    let cid = cfg.active_creator_id.as_deref()?;
    let slug = cfg.workspace_slug_for_creator(cid);
    Some(crate::workspace_state_db_path(user_home, cid, &slug))
}

/// Test-only ordering rendezvous for the legacy migration.
///
/// The migration must be past its pre-lock existence check and still held before
/// it acquires the writer lock, so a test can commit a newer document into that
/// exact window without a fixed wait. Compiled out of every non-test build.
#[cfg(test)]
mod migration_rendezvous {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::Mutex;

    type Pause = (PathBuf, Sender<()>, Receiver<()>);

    static PAUSE: Mutex<Option<Pause>> = Mutex::new(None);

    /// Arm the rendezvous for one home. Returns the arrival receiver and the
    /// release sender; the caller observes the hold and then releases it.
    pub(super) fn arm(home: &Path) -> (Receiver<()>, Sender<()>) {
        let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *PAUSE.lock().expect("rendezvous") = Some((home.to_path_buf(), arrived_tx, release_rx));
        (arrived_rx, release_tx)
    }

    /// Hold the migration until the test that armed this home releases it.
    pub(super) fn hold(home: &Path) {
        let armed = {
            let mut slot = PAUSE.lock().expect("rendezvous");
            match slot.as_ref() {
                Some((path, _, _)) if path == home => slot.take(),
                _ => None,
            }
        };
        if let Some((_, arrived, release)) = armed {
            arrived.send(()).expect("rendezvous arrival");
            release.recv().expect("rendezvous release");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendezvous is process-global and armed per home; the tests that use
    /// it must not overlap or one would replace the other's arming.
    static RENDEZVOUS_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// BUG-02: a legacy migration must not overwrite a newer document that a
    /// locked writer committed while the migration waited. The migration is
    /// held deterministically between its pre-lock check and the acquisition.
    #[test]
    fn a_legacy_migration_does_not_overwrite_a_newer_committed_document() {
        let _serial = RENDEZVOUS_TEST.lock().expect("rendezvous test serial");
        let tmp = tempfile::tempdir().expect("temp home");
        let root = tmp.path().to_path_buf();
        std::fs::write(
            root.join("config.json"),
            "{\"active_creator_id\":\"ctr_legacy\"}",
        )
        .expect("legacy json");

        let (arrived, release) = migration_rendezvous::arm(&root);
        let loader_root = root.clone();
        let loader = std::thread::spawn(move || CliConfigSnapshot::load(&loader_root));

        // The migration passed its pre-check and is held before the lock.
        arrived.recv().expect("the migration reached the hold");

        // A writer commits a newer document while the migration cannot run.
        crate::config_lock::publish_document(
            &root,
            "active_creator_id = \"ctr_new\"\n\n[agent_profile]\nname = \"kept\"\n",
        )
        .expect("newer commit");
        release.send(()).expect("release the migration");

        let snapshot = loader
            .join()
            .expect("loader thread")
            .expect("the migration succeeds");
        assert_eq!(snapshot.active_creator_id.as_deref(), Some("ctr_new"));

        let stored = std::fs::read_to_string(root.join("config.toml")).expect("config");
        assert!(
            stored.contains("ctr_new"),
            "the newer commit wins the document: {stored}"
        );
        assert!(
            !stored.contains("ctr_legacy"),
            "the stale legacy snapshot is not published: {stored}"
        );
        assert!(
            stored.contains("agent_profile"),
            "the newer commit's other fields survive: {stored}"
        );
    }

    /// A provisioned `config.toml` is read without taking the writer lock or
    /// writing anything, so an unwritable home still resolves its selection.
    #[test]
    fn a_provisioned_document_is_read_without_the_lock() {
        let tmp = tempfile::tempdir().expect("temp home");
        let root = tmp.path().to_path_buf();
        std::fs::write(
            root.join("config.toml"),
            "active_creator_id = \"ctr_kept\"\n",
        )
        .expect("config");

        let snapshot = CliConfigSnapshot::load(&root).expect("the provisioned document reads");
        assert_eq!(snapshot.active_creator_id.as_deref(), Some("ctr_kept"));
        assert!(
            !root.join("config.toml.lock").exists(),
            "reading a provisioned document takes no lock"
        );
    }

    /// BUG-04: a loader that passed its pre-lock check while another loader
    /// completed an *empty* legacy migration must see the documented default,
    /// not a `NotFound` fault. Only the disappeared-JSON case is treated that
    /// way; every other read error still fails.
    #[test]
    fn a_disappeared_empty_legacy_document_reads_as_default() {
        let _serial = RENDEZVOUS_TEST.lock().expect("rendezvous test serial");
        let tmp = tempfile::tempdir().expect("temp home");
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join("config.json"), "").expect("empty legacy json");

        let (arrived, release) = migration_rendezvous::arm(&root);
        let loader_root = root.clone();
        let loader = std::thread::spawn(move || CliConfigSnapshot::load(&loader_root));

        // The loader passed its pre-check and is held before the lock.
        arrived.recv().expect("the migration reached the hold");

        // The concurrent loader completes the empty migration: the JSON is
        // renamed away and no TOML is created.
        {
            let _lock = crate::config_lock::acquire(&root).expect("the migration lock");
            std::fs::rename(root.join("config.json"), root.join("config.json.migrated"))
                .expect("complete the empty migration");
        }
        release.send(()).expect("release the migration");

        let snapshot = loader
            .join()
            .expect("loader thread")
            .expect("the empty legacy home reads as the default");
        assert_eq!(snapshot.active_creator_id, None);
    }
}
