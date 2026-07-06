//! `Nexus` `AgentScanRequest`
//!
//! `Request` body for `POST` /v1/daemon/agent-host/scan. `Triggers` a combined registry-list + `PATH`-probe operation that returns `ACP` agent entries annotated with local-install availability. `Additive` `V1`.94 endpoint — no breaking change to existing agent-host routes.
//!
//! `@schema_version` 1
//! `@source` scan-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/daemon/agent-host/scan. `Triggers` a combined registry-list + `PATH`-probe operation that returns `ACP` agent entries annotated with local-install availability. `Additive` `V1`.94 endpoint — no breaking change to existing agent-host routes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScanRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_refresh: Option<bool>,
}
