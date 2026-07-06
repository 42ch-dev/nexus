//! `Nexus` `AgentScanEntry`
//!
//! `A` single `ACP` agent entry annotated with local `PATH`-install availability. `Returned` by `POST` /v1/daemon/agent-host/scan. `Each` entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version.
//!
//! `@schema_version` 1
//! `@source` agent-scan-entry.schema.json

use serde::{Deserialize, Serialize};

/// `A` single `ACP` agent entry annotated with local `PATH`-install availability. `Returned` by `POST` /v1/daemon/agent-host/scan. `Each` entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentScanEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_command: Option<String>,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}
