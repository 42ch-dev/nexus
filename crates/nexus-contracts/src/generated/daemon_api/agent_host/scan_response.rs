//! `Nexus` `AgentScanResponse`
//!
//! `Response` for `POST` /v1/daemon/agent-host/scan. `Returns` the `ACP` registry agent list annotated with local `PATH`-install availability. `Additive` `V1`.94 endpoint.
//!
//! `@schema_version` 1
//! `@source` scan-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::agent_host::agent_scan_entry::AgentScanEntry;

/// `Response` for `POST` /v1/daemon/agent-host/scan. `Returns` the `ACP` registry agent list annotated with local `PATH`-install availability. `Additive` `V1`.94 endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScanResponse {
    pub agents: Vec<AgentScanEntry>,
}
