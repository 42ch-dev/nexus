//! `Nexus` `BatchUpdateFindingsRequest`
//!
//! `Request` body for `PATCH` /v1/daemon/findings/batch. `Bulk`-updates status and/or `target_executor` for up to 100 findings. `Creator`-scoped; each individual update reuses the existing `update_finding` `DAO` validation.
//!
//! `@schema_version` 1
//! `@source` batch-update-findings-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::findings::finding_batch_patch::FindingBatchPatch;

/// `Request` body for `PATCH` /v1/daemon/findings/batch. `Bulk`-updates status and/or `target_executor` for up to 100 findings. `Creator`-scoped; each individual update reuses the existing `update_finding` `DAO` validation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BatchUpdateFindingsRequest {
    pub finding_ids: Vec<String>,
    pub patch: FindingBatchPatch,
}
