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
    /// # Errors
    ///
    /// Returns an error when reading `config.toml` or the legacy `config.json`
    /// fails, when `config.toml` contains invalid TOML, or when migrating a
    /// valid `config.json` fails to write `config.toml` or rename the legacy
    /// file to `config.json.migrated`. Missing config files are not errors.
    pub fn load(nexus_root: &Path) -> Result<Self, anyhow::Error> {
        let toml_path = nexus_root.join("config.toml");
        let json_path = nexus_root.join("config.json");

        if toml_path.exists() {
            let text = std::fs::read_to_string(&toml_path)?;
            if text.trim().is_empty() {
                return Ok(Self::default());
            }
            return Ok(toml::from_str(&text)?);
        }

        if json_path.exists() {
            let text = std::fs::read_to_string(&json_path)?;
            if text.trim().is_empty() {
                std::fs::rename(&json_path, nexus_root.join("config.json.migrated"))?;
                return Ok(Self::default());
            }
            match serde_json::from_str::<Self>(&text) {
                Ok(cfg) => {
                    let toml_str = toml::to_string_pretty(&cfg)?;
                    std::fs::write(&toml_path, toml_str)?;
                    std::fs::rename(&json_path, nexus_root.join("config.json.migrated"))?;
                    return Ok(cfg);
                }
                Err(_) => return Ok(Self::default()),
            }
        }

        Ok(Self::default())
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
/// Returns an error when `config.toml` cannot be read or parsed, or when the
/// provider table does not match [`MemoryProviderConfig`].
pub fn read_memory_provider_config(
    nexus_root: &Path,
) -> Result<Option<MemoryProviderConfig>, anyhow::Error> {
    let path = nexus_root.join("config.toml");
    if !path.exists() {
        return Ok(None);
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
        anyhow::anyhow!(
            "invalid [memory_provider] in {}: {error}",
            path.display()
        )
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
