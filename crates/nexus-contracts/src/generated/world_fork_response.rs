//! `Nexus` `WorldForkResponse`
//!
//! `Response` body for `POST` /v1/worlds/fork — created `ForkBranch` record.
//!
//! `@schema_version` 1
//! `@source` world-fork-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::fork_branch::ForkBranch;

/// `Response` body for `POST` /v1/worlds/fork — created `ForkBranch` record.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldForkResponse {
    pub schema_version: u32,
    pub fork_branch: ForkBranch,
}
