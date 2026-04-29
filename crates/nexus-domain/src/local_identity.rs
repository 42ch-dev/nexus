//! Local Identity — local-only creator identity for `local_only` mode.
//!
//! Supports two identity types:
//! - **Anonymous**: ephemeral, generated on-the-fly, no persistence required
//! - **Persistent**: stored in `SQLite`, survives restarts, full creator workspace
//!
//! See ADR-017 (`local_only` registration), ADR-014 (local FS layout).

use crate::errors::DomainError;
use serde::{Deserialize, Serialize};

/// Local identity type enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalIdentityType {
    /// Ephemeral identity — no persistent storage, disposable
    Anonymous,
    /// Persistent identity — stored in `SQLite`, survives restarts
    Persistent,
}

impl LocalIdentityType {
    #[must_use]
    /// String representation matching JSON Schema enum values.
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Anonymous => "anonymous",
            Self::Persistent => "persistent",
        }
    }
}

impl std::fmt::Display for LocalIdentityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for LocalIdentityType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "anonymous" => Ok(Self::Anonymous),
            "persistent" => Ok(Self::Persistent),
            _ => Err(format!("Invalid LocalIdentityType: {}", s)),
        }
    }
}

/// Local creator identity aggregate.
///
/// Represents a creator identity that exists entirely locally, without
/// platform dependency. Can optionally be linked to a platform Creator
/// via the Pairing flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalIdentity {
    pub schema_version: u32,
    pub creator_id: String,
    pub identity_type: LocalIdentityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at: String,
    pub platform_linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_creator_id: Option<String>,
}

impl LocalIdentity {
    /// Create an anonymous (ephemeral) identity.
    ///
    /// Generates a `ctr_anon` + random hex ID. The caller may choose
    /// not to persist this identity — it is designed for disposable use.
    #[must_use]
    pub fn create_anonymous() -> Self {
        let id = generate_anonymous_id();
        Self {
            schema_version: 1,
            creator_id: id,
            identity_type: LocalIdentityType::Anonymous,
            display_name: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            platform_linked: false,
            platform_creator_id: None,
        }
    }

    /// Create a persistent local identity.
    ///
    /// Generates a `ctr_local` + random hex ID. The caller should persist
    /// this identity in the local `SQLite` database.
    #[must_use]
    pub fn create_persistent(display_name: Option<&str>) -> Self {
        let id = generate_local_id();
        Self {
            schema_version: 1,
            creator_id: id,
            identity_type: LocalIdentityType::Persistent,
            display_name: display_name.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            platform_linked: false,
            platform_creator_id: None,
        }
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Link this local identity to a platform Creator.
    ///
    /// Once linked, the identity has a corresponding platform Creator
    /// and can participate in platform features.
    pub fn link_to_platform(&mut self, platform_creator_id: &str) -> Result<(), DomainError> {
        if self.platform_linked {
            return Err(DomainError::AlreadyInState("platform_linked".to_string()));
        }
        if !is_valid_creator_id(platform_creator_id) {
            return Err(DomainError::InvalidIdFormat(format!(
                "platform_creator_id '{}' does not match CreatorId pattern",
                platform_creator_id
            )));
        }
        self.platform_linked = true;
        self.platform_creator_id = Some(platform_creator_id.to_string());
        Ok(())
    }
    #[must_use]
    /// Check if this identity is anonymous (ephemeral).
    pub fn is_anonymous(&self) -> bool {
        self.identity_type == LocalIdentityType::Anonymous
    }

    /// Check if this identity is persistent.
    #[must_use]
    pub fn is_persistent(&self) -> bool {
        self.identity_type == LocalIdentityType::Persistent
    }

    /// Check if this identity is linked to a platform Creator.
    pub const fn is_linked(&self) -> bool {
        self.platform_linked
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

use nexus_contracts::local::domain::local_identity::IdentityType as ContractIdentityType;

impl TryFrom<nexus_contracts::local::domain::LocalIdentity> for LocalIdentity {
    type Error = DomainError;

    fn try_from(c: nexus_contracts::local::domain::LocalIdentity) -> Result<Self, Self::Error> {
        let identity_type = match c.identity_type {
            ContractIdentityType::Anonymous => LocalIdentityType::Anonymous,
            ContractIdentityType::Persistent => LocalIdentityType::Persistent,
        };
        Ok(Self {
            schema_version: c.schema_version,
            creator_id: c.creator_id,
            identity_type,
            display_name: c.display_name,
            created_at: c.created_at,
            platform_linked: c.platform_linked,
            platform_creator_id: c.platform_creator_id,
        })
    }
}

impl From<LocalIdentity> for nexus_contracts::local::domain::LocalIdentity {
    fn from(d: LocalIdentity) -> Self {
        let identity_type = match d.identity_type {
            LocalIdentityType::Anonymous => ContractIdentityType::Anonymous,
            LocalIdentityType::Persistent => ContractIdentityType::Persistent,
        };
        Self {
            schema_version: d.schema_version,
            creator_id: d.creator_id,
            identity_type,
            display_name: d.display_name,
            created_at: d.created_at,
            platform_linked: d.platform_linked,
            platform_creator_id: d.platform_creator_id,
        }
    }
}

// ── ID generation helpers ──────────────────────────────────────────────

// V1.2 residual R5 (identity, nit): UUID v4 vs counter-based ID generation
// UUID v4 is collision-safe for expected scale; counter not required by spec

/// Generate an anonymous identity ID: `ctr_anon` + 12 random hex chars.
///
/// Format: `ctr_anonA1b2C3d4E5f6` (within `CreatorId` `^ctr_[a-zA-Z0-9]+$` pattern).
fn generate_anonymous_id() -> String {
    let random: String = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(12)
        .collect();
    format!("ctr_anon{}", random)
}

/// Generate a persistent local identity ID: `ctr_local` + 12 random hex chars.
///
/// Format: `ctr_localA1b2C3d4E5f6` (within `CreatorId` `^ctr_[a-zA-Z0-9]+$` pattern).
fn generate_local_id() -> String {
    let random: String = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(12)
        .collect();
    format!("ctr_local{}", random)
}

/// Validate a string matches the `CreatorId` pattern `^ctr_[a-zA-Z0-9]+$`.
pub fn is_valid_creator_id(s: &str) -> bool {
    s.starts_with("ctr_") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_create_anonymous() {
        let identity = LocalIdentity::create_anonymous();
        assert!(identity.creator_id.starts_with("ctr_anon"));
        assert_eq!(identity.identity_type, LocalIdentityType::Anonymous);
        assert!(identity.display_name.is_none());
        assert!(!identity.platform_linked);
        assert!(identity.platform_creator_id.is_none());
        assert_eq!(identity.schema_version, 1);
    }

    #[test]
    fn test_create_persistent_without_name() {
        let identity = LocalIdentity::create_persistent(None);
        assert!(identity.creator_id.starts_with("ctr_local"));
        assert_eq!(identity.identity_type, LocalIdentityType::Persistent);
        assert!(identity.display_name.is_none());
        assert!(!identity.platform_linked);
    }

    #[test]
    fn test_create_persistent_with_name() {
        let identity = LocalIdentity::create_persistent(Some("Test Creator"));
        assert_eq!(identity.display_name, Some("Test Creator".to_string()));
    }

    #[test]
    fn test_anonymous_id_matches_creator_id_pattern() {
        for _ in 0..100 {
            let id = generate_anonymous_id();
            assert!(
                is_valid_creator_id(&id),
                "ID '{}' should match CreatorId pattern",
                id
            );
        }
    }

    #[test]
    fn test_local_id_matches_creator_id_pattern() {
        for _ in 0..100 {
            let id = generate_local_id();
            assert!(
                is_valid_creator_id(&id),
                "ID '{}' should match CreatorId pattern",
                id
            );
        }
    }

    #[test]
    fn test_link_to_platform() {
        let mut identity = LocalIdentity::create_persistent(Some("Test"));
        identity.link_to_platform("ctr_Platform123").unwrap();
        assert!(identity.platform_linked);
        assert_eq!(
            identity.platform_creator_id,
            Some("ctr_Platform123".to_string())
        );
    }

    #[test]
    fn test_link_to_platform_already_linked() {
        let mut identity = LocalIdentity::create_persistent(Some("Test"));
        identity.link_to_platform("ctr_Platform123").unwrap();
        let result = identity.link_to_platform("ctr_Another456");
        assert!(result.is_err());
        assert!(matches!(result, Err(DomainError::AlreadyInState(_))));
    }

    #[test]
    fn test_link_to_platform_invalid_id() {
        let mut identity = LocalIdentity::create_persistent(Some("Test"));
        let result = identity.link_to_platform("invalid_id");
        assert!(result.is_err());
        assert!(matches!(result, Err(DomainError::InvalidIdFormat(_))));
    }

    #[test]
    fn test_is_anonymous() {
        let identity = LocalIdentity::create_anonymous();
        assert!(identity.is_anonymous());
        assert!(!identity.is_persistent());
    }

    #[test]
    fn test_is_persistent() {
        let identity = LocalIdentity::create_persistent(Some("Test"));
        assert!(identity.is_persistent());
        assert!(!identity.is_anonymous());
    }

    #[test]
    fn test_is_linked() {
        let mut identity = LocalIdentity::create_persistent(Some("Test"));
        assert!(!identity.is_linked());
        identity.link_to_platform("ctr_Platform123").unwrap();
        assert!(identity.is_linked());
    }

    #[test]
    fn test_identity_type_as_str() {
        assert_eq!(LocalIdentityType::Anonymous.as_str(), "anonymous");
        assert_eq!(LocalIdentityType::Persistent.as_str(), "persistent");
    }

    #[test]
    fn test_identity_type_from_str() {
        assert_eq!(
            LocalIdentityType::from_str("anonymous").unwrap(),
            LocalIdentityType::Anonymous
        );
        assert_eq!(
            LocalIdentityType::from_str("persistent").unwrap(),
            LocalIdentityType::Persistent
        );
        assert!(LocalIdentityType::from_str("invalid").is_err());
    }

    #[test]
    fn test_identity_type_display() {
        assert_eq!(format!("{}", LocalIdentityType::Anonymous), "anonymous");
        assert_eq!(format!("{}", LocalIdentityType::Persistent), "persistent");
    }

    #[test]
    fn test_serialize_roundtrip() {
        let identity = LocalIdentity::create_persistent(Some("Test Creator"));
        let json = serde_json::to_string(&identity).unwrap();
        let deserialized: LocalIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(identity.creator_id, deserialized.creator_id);
        assert_eq!(identity.identity_type, deserialized.identity_type);
    }

    #[test]
    fn test_contract_roundtrip() {
        let identity = LocalIdentity::create_persistent(Some("Test"));
        let contract: nexus_contracts::local::domain::LocalIdentity = identity.clone().into();
        let back: LocalIdentity = contract.try_into().unwrap();
        assert_eq!(identity.creator_id, back.creator_id);
        assert_eq!(identity.identity_type, back.identity_type);
        assert_eq!(identity.display_name, back.display_name);
        assert_eq!(identity.platform_linked, back.platform_linked);
    }

    #[test]
    fn test_valid_creator_id() {
        assert!(is_valid_creator_id("ctr_abc123"));
        assert!(is_valid_creator_id("ctr_ABC"));
        assert!(!is_valid_creator_id("usr_abc123"));
        assert!(!is_valid_creator_id("ctr_"));
        assert!(!is_valid_creator_id("ctr_abc_def"));
        assert!(!is_valid_creator_id("abc"));
    }
}
