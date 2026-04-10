//! Read `~/.nexus42/config.json` for active creator / workspace slug (same shape as CLI `CliConfig` subset).

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

/// Resolve `state.db` path mirroring CLI `config::resolve_state_db_path` semantics.
pub fn resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> anyhow::Result<PathBuf> {
    let cfg = CliConfigSnapshot::load(nexus_root)?;
    let legacy = nexus_home_layout::legacy_flat_state_db_path(user_home);

    if let Some(cid) = cfg.active_creator_id.as_deref() {
        let slug = cfg.workspace_slug_for_creator(cid);
        let new_path = nexus_home_layout::workspace_state_db_path(user_home, cid, &slug);
        let meta =
            nexus_home_layout::operational_workspace_dir(user_home, cid, &slug).join("meta.json");
        if new_path.exists() || meta.exists() {
            return Ok(new_path);
        }
    }

    if legacy.exists() {
        return Ok(legacy);
    }

    if let Some(cid) = cfg.active_creator_id.as_deref() {
        let slug = cfg.workspace_slug_for_creator(cid);
        return Ok(nexus_home_layout::workspace_state_db_path(
            user_home, cid, &slug,
        ));
    }

    anyhow::bail!(
        "No local Nexus database found. Run `nexus42 init workspace` or `nexus42 migrate local-fs` if upgrading."
    );
}
