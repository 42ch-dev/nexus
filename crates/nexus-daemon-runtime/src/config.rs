//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Read `~/.nexus42/config.toml` for active creator / workspace slug (same shape as CLI `CliConfig` subset).

use nexus_contracts::local::domain::RuntimeMode;
use nexus_home_layout::active_context::{
    read_active_creator_id as layout_read_active_creator_id,
    read_active_workspace_slug as layout_read_active_workspace_slug,
    resolve_state_db_path as layout_resolve_state_db_path,
    try_active_creator_id as layout_try_active_creator_id,
    try_resolve_state_db_path as layout_try_resolve_state_db_path,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Daemon-extended CLI config snapshot (reuses home-layout active-context readers).
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
    /// Load via the shared home-layout reader, then overlay daemon-only fields.
    pub fn load(nexus_root: &Path) -> anyhow::Result<Self> {
        let base = nexus_home_layout::active_context::CliConfigSnapshot::load(nexus_root)?;
        let toml_path = nexus_root.join("config.toml");
        if !toml_path.exists() {
            return Ok(Self {
                active_creator_id: base.active_creator_id,
                active_workspace_slug_by_creator: base.active_workspace_slug_by_creator,
                ..Self::default()
            });
        }
        let text = std::fs::read_to_string(&toml_path)?;
        if text.trim().is_empty() {
            return Ok(Self {
                active_creator_id: base.active_creator_id,
                active_workspace_slug_by_creator: base.active_workspace_slug_by_creator,
                ..Self::default()
            });
        }
        let mut cfg: Self = toml::from_str(&text)?;
        if cfg.active_creator_id.is_none() {
            cfg.active_creator_id = base.active_creator_id;
        }
        if cfg.active_workspace_slug_by_creator.is_empty() {
            cfg.active_workspace_slug_by_creator = base.active_workspace_slug_by_creator;
        }
        Ok(cfg)
    }

    #[must_use]
    pub fn workspace_slug_for_creator(&self, creator_id: &str) -> String {
        nexus_home_layout::active_context::CliConfigSnapshot {
            active_creator_id: self.active_creator_id.clone(),
            active_workspace_slug_by_creator: self.active_workspace_slug_by_creator.clone(),
        }
        .workspace_slug_for_creator(creator_id)
    }
}

/// Resolve workspace `state.db` under ADR-014 (same rules as CLI `config::resolve_state_db_path`).
pub fn resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> anyhow::Result<PathBuf> {
    layout_resolve_state_db_path(user_home, nexus_root)
}

/// Read the active `creator_id` from the CLI config (`config.toml`).
///
/// State-layer authority: this lives beside the rest of the config readers, NOT
/// in an HTTP handler module, so non-HTTP consumers (workspace state, the
/// agent-host probe owner) share one read/validation path with the handlers.
#[must_use]
pub fn read_active_creator_id(nexus_home: &Path) -> Option<String> {
    layout_read_active_creator_id(nexus_home)
}

/// Read the active workspace slug for `creator_id` from the CLI config.
///
/// Missing or empty slug entries fall back to `"default"` — the same contract as
/// [`CliConfigSnapshot::workspace_slug_for_creator`] and `resolve_state_db_path`.
/// Profile switch intentionally clears a stale slug (`set_active_creator` /
/// desktop `switch_active_creator`) and relies on this default; returning
/// `None` here surfaces a misleading `Authentication required`.
#[must_use]
pub fn read_active_workspace_slug(nexus_home: &Path, creator_id: &str) -> Option<String> {
    layout_read_active_workspace_slug(nexus_home, creator_id)
}

/// Read `active_creator_id` from `~/.nexus42/config.toml` without failing.
///
/// Used by Tier-0/Tier-1 handlers and [`require_active_creator`](crate::api::middleware::require_active_creator)
/// to distinguish "no Profile selected yet" from fatal config errors. Returns `None` when the key is
/// absent or the config file cannot be read.
#[must_use]
pub fn try_active_creator_id(nexus_root: &Path) -> Option<String> {
    layout_try_active_creator_id(nexus_root)
}

/// Returns `None` when `active_creator_id` is absent, instead of failing
/// fatally. Used during boot to allow the daemon to start without a creator.
///
/// When `active_creator_id` is present, returns `Some(path)` using the same
/// ADR-014 path rules as the CLI.
#[must_use]
pub fn try_resolve_state_db_path(user_home: &Path, nexus_root: &Path) -> Option<PathBuf> {
    layout_try_resolve_state_db_path(user_home, nexus_root)
}
