//! `Nexus` `Pairing`
//!
//! `Pairing` entity describing `Creator` <-> `User` association. `Aligned` with data-model-v1.md §5.2A.
//!
//! `@schema_version` 1
//! `@source` pairing.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{PairingSource, PairingStatus};

/// `Pairing` entity describing `Creator` <-> `User` association. `Aligned` with data-model-v1.md §5.2A.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Pairing {
    pub schema_version: u32,
    pub pairing_id: String,
    pub creator_id: String,
    pub user_id: String,
    pub pairing_source: PairingSource,
    pub status: PairingStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}
