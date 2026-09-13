//! Active creator / workspace selection readers (`~/.nexus42/config.toml`).
//!
//! Single implementation consumed by daemon, core, and CLI — includes default
//! workspace slug fallback and legacy `config.json` migration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Subset of CLI config persisted under `~/.nexus42/config.toml`.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CliConfigSnapshot {
    #[serde(default)]
    pub active_creator_id: Option<String>,
    #[serde(default)]
    pub active_workspace_slug_by_creator: HashMap<String, String>,
}

impl CliConfigSnapshot {
    /// Load from `config.toml` with automatic migration from legacy `config.json`.
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
pub fn resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> Result<PathBuf, anyhow::Error> {
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
