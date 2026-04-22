//! Read `~/.nexus42/config.toml` for active creator / workspace slug (same shape as CLI `CliConfig` subset).

use nexus_contracts::local::domain::RuntimeMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_WORKSPACE_SLUG: &str = "default";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CliConfigSnapshot {
    #[serde(default)]
    pub active_creator_id: Option<String>,
    #[serde(default)]
    pub active_workspace_slug_by_creator: HashMap<String, String>,
    #[serde(default)]
    pub runtime_mode: Option<RuntimeMode>,
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
            match serde_json::from_str::<CliConfigSnapshot>(&text) {
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

    pub fn workspace_slug_for_creator(&self, creator_id: &str) -> String {
        self.active_workspace_slug_by_creator
            .get(creator_id)
            .map(|s| s.as_str())
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
