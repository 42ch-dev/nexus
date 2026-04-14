//! Nexus Local Creator Identity
//!
//! Local-only creator identity for local_only mode. Supports anonymous (ephemeral) and persistent identities without platform dependency. See ADR-017, ADR-014.
//!
//! @schema_version 1
//! @source local-identity.schema.json

use serde::{Deserialize, Serialize};

/// Local-only creator identity for local_only mode. Supports anonymous (ephemeral) and persistent identities without platform dependency. See ADR-017, ADR-014.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LocalIdentity {
    pub schema_version: u32,
    pub creator_id: String,
    pub identity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at: String,
    pub platform_linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_creator_id: Option<String>,
}
