//! `Nexus` `BatchUpdateFindingsResponse`
//!
//! `Response` for `PATCH` /v1/daemon/findings/batch. `Returns` partial-success counts and lists of `IDs` that could not be updated. `Always` `HTTP` 200 unless the request exceeds the cap or a `DB` error occurs.
//!
//! `@schema_version` 1
//! `@source` batch-update-findings-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `PATCH` /v1/daemon/findings/batch. `Returns` partial-success counts and lists of `IDs` that could not be updated. `Always` `HTTP` 200 unless the request exceeds the cap or a `DB` error occurs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BatchUpdateFindingsResponse {
    pub updated: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_found: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<Vec<String>>,
}
