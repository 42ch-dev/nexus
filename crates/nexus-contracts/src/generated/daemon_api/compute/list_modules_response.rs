//! `Nexus` `ListModulesResponse`
//!
//! `Response` for `GET` /v1/daemon/compute/modules.
//!
//! `@schema_version` 1
//! `@source` list-modules-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::compute::module_summary::ModuleSummary;

/// `Response` for `GET` /v1/daemon/compute/modules.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListModulesResponse {
    pub items: Vec<ModuleSummary>,
    pub has_more: bool,
}
