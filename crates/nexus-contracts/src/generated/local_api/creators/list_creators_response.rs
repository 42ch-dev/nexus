//! `Nexus` `ListCreatorsResponse`
//!
//! `Response` for `GET` /v1/local/creators.
//!
//! `@schema_version` 1
//! `@source` list-creators-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::creators::creator_info::CreatorInfo;
use crate::generated::local_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/local/creators.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListCreatorsResponse {
    pub items: Vec<CreatorInfo>,
    pub pagination: PaginationInfo,
}
