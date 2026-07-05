//! `Nexus` `ListCreatorsResponse`
//!
//! `Response` for `GET` /v1/daemon/creators.
//!
//! `@schema_version` 1
//! `@source` list-creators-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::creators::creator_info::CreatorInfo;
use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/daemon/creators.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListCreatorsResponse {
    pub items: Vec<CreatorInfo>,
    pub pagination: PaginationInfo,
}
