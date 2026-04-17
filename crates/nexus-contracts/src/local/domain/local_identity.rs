//! LocalIdentity — local-only creator identity.
//!
//! Local-only creator identity for local_only mode. Supports anonymous
//! (ephemeral) and persistent identities without platform dependency.
//! See ADR-017, ADR-014.

use serde::{Deserialize, Serialize};

/// Local-only creator identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LocalIdentity {
    pub schema_version: u32,
    /// Local creator identifier (ctr_anon* for anonymous, ctr_local* for persistent)
    pub creator_id: String,
    /// Type of local identity.
    pub identity_type: IdentityType,
    /// User-chosen display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Identity creation timestamp.
    pub created_at: String,
    /// Whether this local identity has been linked to a platform Creator.
    #[serde(default)]
    pub platform_linked: bool,
    /// Platform Creator ID after linking (null until linked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_creator_id: Option<String>,
}

/// Type of local identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityType {
    Anonymous,
    Persistent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_anonymous_identity() {
        let v = LocalIdentity {
            schema_version: 1,
            creator_id: "ctr_anon_abc123".to_string(),
            identity_type: IdentityType::Anonymous,
            display_name: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            platform_linked: false,
            platform_creator_id: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: LocalIdentity = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn roundtrip_persistent_identity() {
        let v = LocalIdentity {
            schema_version: 1,
            creator_id: "ctr_local_xyz".to_string(),
            identity_type: IdentityType::Persistent,
            display_name: Some("Test Creator".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            platform_linked: true,
            platform_creator_id: Some("ctr_platform_abc".to_string()),
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: LocalIdentity = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
