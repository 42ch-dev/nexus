//! Pairing aggregate — Creator ↔ User association.
//!
//! Tracks the relationship between Creators and Users. A Creator can have
//! multiple pairings over time, but only one active pairing at a time.
//! See data-model-v1.md §5.2A.

use crate::errors::DomainError;
use serde::{Deserialize, Serialize};

/// Pairing source enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PairingSource {
    AutoCli,
    ManualWeb,
    PlatformAuto,
}

impl PairingSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::AutoCli => "auto_cli",
            Self::ManualWeb => "manual_web",
            Self::PlatformAuto => "platform_auto",
        }
    }
}

/// Pairing aggregate — Creator-User association.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pairing {
    pub schema_version: u32,
    pub pairing_id: String,
    pub creator_id: String,
    pub user_id: String,
    pub pairing_source: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

impl Pairing {
    /// Create a new pairing between creator and user.
    pub fn new(
        pairing_id: &str,
        creator_id: &str,
        user_id: &str,
        pairing_source: PairingSource,
    ) -> Self {
        Self {
            schema_version: 1,
            pairing_id: pairing_id.to_string(),
            creator_id: creator_id.to_string(),
            user_id: user_id.to_string(),
            pairing_source: pairing_source.as_str().to_string(),
            status: "active".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            revoked_at: None,
        }
    }

    /// Revoke this pairing.
    /// Sets status to "revoked" and records revoked_at timestamp.
    pub fn revoke(&mut self) -> Result<(), DomainError> {
        if self.status != "active" {
            return Err(DomainError::AlreadyInState(self.status.clone()));
        }
        self.status = "revoked".to_string();
        self.revoked_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

    /// Check if this pairing is active.
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Validate that this pairing authorizes the given creator+user combination.
    pub fn authorizes(&self, creator_id: &str, user_id: &str) -> bool {
        self.status == "active" && self.creator_id == creator_id && self.user_id == user_id
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::Pairing> for Pairing {
    fn from(c: nexus_contracts::Pairing) -> Self {
        Self {
            schema_version: c.schema_version,
            pairing_id: c.pairing_id,
            creator_id: c.creator_id,
            user_id: c.user_id,
            pairing_source: c.pairing_source,
            status: c.status,
            created_at: c.created_at,
            revoked_at: c.revoked_at,
        }
    }
}

impl From<Pairing> for nexus_contracts::Pairing {
    fn from(d: Pairing) -> Self {
        Self {
            schema_version: d.schema_version,
            pairing_id: d.pairing_id,
            creator_id: d.creator_id,
            user_id: d.user_id,
            pairing_source: d.pairing_source,
            status: d.status,
            created_at: d.created_at,
            revoked_at: d.revoked_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_auto_cli_pairing() {
        let pairing = Pairing::new(
            "prg_test123",
            "ctr_creator1",
            "usr_user1",
            PairingSource::AutoCli,
        );
        assert_eq!(pairing.pairing_source, "auto_cli");
        assert_eq!(pairing.status, "active");
        assert!(pairing.revoked_at.is_none());
        assert!(pairing.is_active());
    }

    #[test]
    fn test_revoke_pairing() {
        let mut pairing = Pairing::new("prg_test456", "ctr_c1", "usr_u1", PairingSource::ManualWeb);
        pairing.revoke().unwrap();
        assert_eq!(pairing.status, "revoked");
        assert!(pairing.revoked_at.is_some());
        assert!(!pairing.is_active());
    }

    #[test]
    fn test_authorizes_correct_pair() {
        let pairing = Pairing::new("prg_1", "ctr_1", "usr_1", PairingSource::AutoCli);
        assert!(pairing.authorizes("ctr_1", "usr_1"));
    }

    #[test]
    fn test_authorizes_wrong_user() {
        let pairing = Pairing::new("prg_1", "ctr_1", "usr_1", PairingSource::AutoCli);
        assert!(!pairing.authorizes("ctr_1", "usr_wrong"));
    }

    #[test]
    fn test_authorizes_wrong_creator() {
        let pairing = Pairing::new("prg_1", "ctr_1", "usr_1", PairingSource::AutoCli);
        assert!(!pairing.authorizes("ctr_wrong", "usr_1"));
    }

    #[test]
    fn test_revoke_already_revoked() {
        let mut pairing = Pairing::new("prg_1", "ctr_1", "usr_1", PairingSource::AutoCli);
        pairing.revoke().unwrap();
        assert!(matches!(
            pairing.revoke(),
            Err(DomainError::AlreadyInState(_))
        ));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let pairing = Pairing::new("prg_abc", "ctr_abc", "usr_abc", PairingSource::PlatformAuto);
        let json = serde_json::to_string(&pairing).unwrap();
        let deserialized: Pairing = serde_json::from_str(&json).unwrap();
        assert_eq!(pairing, deserialized);
    }
}
