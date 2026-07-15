//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Read `~/.nexus42/config.toml` for active creator / workspace slug (same shape as CLI `CliConfig` subset).

use nexus_contracts::local::domain::RuntimeMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Resolve the default workspace root when `workspace_path` is unset.
///
/// Mirrors the CLI default: `dirs::document_dir().join("nexus").join("default")`.
#[must_use]
pub fn resolve_default_workspace_path() -> PathBuf {
    dirs::document_dir()
        .or_else(|| {
            tracing::warn!("dirs::document_dir() returned None; falling back to ~/Documents");
            dirs::home_dir().map(|home| home.join("Documents"))
        })
        .unwrap_or_else(|| {
            tracing::warn!("dirs::home_dir() returned None; using relative fallback");
            PathBuf::from("Documents")
        })
        .join("nexus")
        .join("default")
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CliConfigSnapshot {
    #[serde(default)]
    pub active_creator_id: Option<String>,
    #[serde(default)]
    pub active_workspace_slug_by_creator: HashMap<String, String>,
    #[serde(default)]
    pub runtime_mode: Option<RuntimeMode>,
    #[serde(default)]
    pub workspace_path: Option<PathBuf>,
}

impl CliConfigSnapshot {
    /// Load from `config.toml` with automatic migration from legacy `config.json`.
    pub fn load(nexus_root: &Path) -> anyhow::Result<Self> {
        let toml_path = nexus_root.join("config.toml");
        let json_path = nexus_root.join("config.json");

        // 1. Try loading config.toml
        if toml_path.exists() {
            let text = std::fs::read_to_string(&toml_path)?;
            if text.trim().is_empty() {
                return Ok(Self::default());
            }
            return Ok(toml::from_str(&text)?);
        }

        // 2. Migration: try loading legacy config.json
        if json_path.exists() {
            let text = std::fs::read_to_string(&json_path)?;
            if text.trim().is_empty() {
                std::fs::rename(&json_path, nexus_root.join("config.json.migrated"))?;
                return Ok(Self::default());
            }
            match serde_json::from_str::<Self>(&text) {
                Ok(cfg) => {
                    // Write config.toml and rename legacy file
                    let toml_str = toml::to_string_pretty(&cfg)?;
                    std::fs::write(&toml_path, toml_str)?;
                    std::fs::rename(&json_path, nexus_root.join("config.json.migrated"))?;
                    tracing::info!("Migrated config.json → config.toml (daemon)");
                    return Ok(cfg);
                }
                Err(e) => {
                    tracing::warn!("Legacy config.json corrupted: {}", e);
                    return Ok(Self::default());
                }
            };
        }

        // 3. No config file — return defaults
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

/// Resolve workspace `state.db` under ADR-014 (same rules as CLI `config::resolve_state_db_path`).
pub fn resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> anyhow::Result<PathBuf> {
    let cfg = CliConfigSnapshot::load(nexus_root)?;
    let cid = cfg.active_creator_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No active creator in ~/.nexus42/config.toml. Run `nexus42 init workspace` or `nexus42 creator use <id>`."
        )
    })?;
    let slug = cfg.workspace_slug_for_creator(cid);
    Ok(nexus_home_layout::workspace_state_db_path(
        user_home, cid, &slug,
    ))
}

/// Read `active_creator_id` from `~/.nexus42/config.toml` without failing.
///
/// Used by Tier-0/Tier-1 handlers and [`require_active_creator`](crate::api::middleware::require_active_creator)
/// to distinguish "no Profile selected yet" from fatal config errors. Returns `None` when the key is
/// absent or the config file cannot be read.
#[must_use]
pub fn try_active_creator_id(nexus_root: &Path) -> Option<String> {
    CliConfigSnapshot::load(nexus_root)
        .ok()
        .and_then(|cfg| cfg.active_creator_id)
}

/// Returns `None` when `active_creator_id` is absent, instead of failing
/// fatally. Used during boot to allow the daemon to start without a creator.
///
/// When `active_creator_id` is present, returns `Some(path)` using the same
/// ADR-014 path rules as the CLI.
#[must_use]
pub fn try_resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> Option<PathBuf> {
    let cfg = CliConfigSnapshot::load(nexus_root).ok()?;
    let cid = cfg.active_creator_id.as_deref()?;
    let slug = cfg.workspace_slug_for_creator(cid);
    Some(nexus_home_layout::workspace_state_db_path(
        user_home, cid, &slug,
    ))
}
