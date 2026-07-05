//! `Nexus` `ListMemoryFragmentsResponse`
//!
//! `Response` body for `GET` /v1/daemon/memory/fragments. `Fragments` are produced only by the `review` route (no `CRUD` on this surface). `Unlike` the pending-review list, this response is `NOT` paginated (returns up to `limit` rows).
//!
//! `@schema_version` 1
//! `@source` list-memory-fragments-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::memory::memory_fragment_info::MemoryFragmentInfo;

/// `Response` body for `GET` /v1/daemon/memory/fragments. `Fragments` are produced only by the `review` route (no `CRUD` on this surface). `Unlike` the pending-review list, this response is `NOT` paginated (returns up to `limit` rows).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListMemoryFragmentsResponse {
    pub fragments: Vec<MemoryFragmentInfo>,
}
