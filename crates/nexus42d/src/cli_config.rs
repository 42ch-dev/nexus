//! Read `~/.nexus42/config.json` for active creator / workspace slug (same shape as CLI `CliConfig` subset).

use nexus_contracts::local::domain::RuntimeMode;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_WORKSPACE_SLUG: &str = "default";

#[derive(Debug, Deserialize, Default)]
pub struct CliConfigSnapshot {
    #[serde(default)]
    pub active_creator_id: Option<String>,
    #[serde(default)]
    pub active_workspace_slug_by_creator: HashMap<String, String>,
    #[serde(default)]
    pub runtime_mode: Option<RuntimeMode>,
}

impl CliConfigSnapshot {
    pub fn load(nexus_root: &Path) -> anyhow::Result<Self> {
        let p = nexus_root.join("config.json");
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&p)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&text)?)
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
            "No active creator in ~/.nexus42/config.json. Run `nexus42 init workspace` or `nexus42 creator use <id>`."
        )
    })?;
    let slug = cfg.workspace_slug_for_creator(cid);
    Ok(nexus_home_layout::workspace_state_db_path(
        user_home, cid, &slug,
    ))
}
