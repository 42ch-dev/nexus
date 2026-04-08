//! Creator aggregate — first-class creative agent.
//!
//! A Creator can be user-owned or agent-registered, operating independently
//! or through a Pairing relationship. See data-model-v1.md §5.2.

use crate::errors::DomainError;
use crate::pairing::{Pairing, PairingSource};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Creator status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreatorStatus {
    Active,
    Archived,
    Locked,
}

impl CreatorStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Locked => "locked",
        }
    }
}

/// Registration source enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationSource {
    Cli,
    WebAgent,
    Platform,
}

impl RegistrationSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cli => "cli",
            Self::WebAgent => "web_agent",
            Self::Platform => "platform",
        }
    }
}

/// Creator style profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StyleProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative_preferences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_patterns: Option<Vec<String>>,
}

/// Creator aggregate — a first-class creative agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Creator {
    pub schema_version: u32,
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub display_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_platform_owned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_ref: Option<String>,
    pub registration_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_profile: Option<StyleProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experience_revision: Option<u64>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl Creator {
    /// Register a new creator (independent of User).
    /// Per roadmap §3.1.2: Creator can register without User login.
    pub fn register(
        creator_id: &str,
        display_name: &str,
        registration_source: RegistrationSource,
        is_platform_owned: bool,
    ) -> Self {
        Self {
            schema_version: 1,
            creator_id: creator_id.to_string(),
            user_id: None,
            display_name: display_name.to_string(),
            status: CreatorStatus::Active.as_str().to_string(),
            is_platform_owned: Some(is_platform_owned),
            api_key_ref: None,
            registration_source: registration_source.as_str().to_string(),
            persona_summary: None,
            style_profile: None,
            experience_revision: Some(0),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Pair this creator with a user.
    /// Creates a Pairing record and updates user_id.
    pub fn pair_with_user(
        &mut self,
        user_id: &str,
        pairing_source: PairingSource,
    ) -> Result<Pairing, DomainError> {
        if self.status != CreatorStatus::Active.as_str() {
            return Err(DomainError::InvalidState {
                expected: "active".to_string(),
                actual: self.status.clone(),
            });
        }
        self.user_id = Some(user_id.to_string());
        let pairing_id = format!("prg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let pairing = Pairing::new(&pairing_id, &self.creator_id, user_id, pairing_source);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(pairing)
    }

    /// Unpair from current user (revokes pairing).
    pub fn unpair(&mut self) -> Result<(), DomainError> {
        if self.user_id.is_none() {
            return Err(DomainError::CreatorNotPaired);
        }
        self.user_id = None;
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

    /// Update style profile.
    pub fn update_style_profile(
        &mut self,
        tone: Vec<String>,
        narrative_prefs: Vec<String>,
        forbidden: Vec<String>,
    ) {
        self.style_profile = Some(StyleProfile {
            tone: if tone.is_empty() { None } else { Some(tone) },
            narrative_preferences: if narrative_prefs.is_empty() {
                None
            } else {
                Some(narrative_prefs)
            },
            forbidden_patterns: if forbidden.is_empty() {
                None
            } else {
                Some(forbidden)
            },
        });
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Increment experience revision (after experience distillation).
    pub fn increment_experience_revision(&mut self) {
        let rev = self.experience_revision.unwrap_or(0);
        self.experience_revision = Some(rev + 1);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Check if creator can persist experience (requires active pairing).
    pub fn can_persist_experience(&self) -> bool {
        self.user_id.is_some() && self.status == CreatorStatus::Active.as_str()
    }

    /// Archive this creator.
    pub fn archive(&mut self) -> Result<(), DomainError> {
        if self.status == CreatorStatus::Archived.as_str() {
            return Err(DomainError::AlreadyInState("archived".to_string()));
        }
        self.status = CreatorStatus::Archived.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

    /// Lock this creator (admin action).
    pub fn lock(&mut self) -> Result<(), DomainError> {
        if self.status == CreatorStatus::Locked.as_str() {
            return Err(DomainError::AlreadyInState("locked".to_string()));
        }
        self.status = CreatorStatus::Locked.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::Creator> for Creator {
    fn from(c: nexus_contracts::Creator) -> Self {
        Self {
            schema_version: c.schema_version,
            creator_id: c.creator_id,
            user_id: c.user_id,
            display_name: c.display_name,
            status: c.status.as_str().to_string(),
            is_platform_owned: c.is_platform_owned,
            api_key_ref: c.api_key_ref,
            registration_source: c.registration_source.as_str().to_string(),
            persona_summary: c.persona_summary,
            style_profile: c.style_profile.map(|v| {
                serde_json::from_value(v).unwrap_or(StyleProfile {
                    tone: None,
                    narrative_preferences: None,
                    forbidden_patterns: None,
                })
            }),
            experience_revision: c.experience_revision,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<Creator> for nexus_contracts::Creator {
    fn from(d: Creator) -> Self {
        Self {
            schema_version: d.schema_version,
            creator_id: d.creator_id,
            user_id: d.user_id,
            display_name: d.display_name,
            status: nexus_contracts::CreatorStatus::from_str(&d.status).unwrap(),
            is_platform_owned: d.is_platform_owned,
            api_key_ref: d.api_key_ref,
            registration_source: nexus_contracts::RegistrationSource::from_str(
                &d.registration_source,
            )
            .unwrap(),
            persona_summary: d.persona_summary,
            style_profile: d.style_profile.map(|sp| {
                serde_json::to_value(&StyleProfileJson {
                    tone: sp.tone,
                    narrative_preferences: sp.narrative_preferences,
                    forbidden_patterns: sp.forbidden_patterns,
                })
                .unwrap_or_default()
            }),
            experience_revision: d.experience_revision,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[derive(Serialize)]
struct StyleProfileJson {
    tone: Option<Vec<String>>,
    narrative_preferences: Option<Vec<String>>,
    forbidden_patterns: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_creator_id() -> String {
        format!("ctr_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
    }

    #[test]
    fn test_register_independent_creator() {
        let creator = Creator::register(
            &make_creator_id(),
            "Test Creator",
            RegistrationSource::Cli,
            false,
        );
        assert_eq!(creator.status, "active");
        assert!(creator.user_id.is_none());
        assert_eq!(creator.experience_revision, Some(0));
        assert_eq!(creator.registration_source, "cli");
        assert_eq!(creator.schema_version, 1);
    }

    #[test]
    fn test_pair_with_user() {
        let cid = make_creator_id();
        let mut creator = Creator::register(&cid, "Test", RegistrationSource::Cli, false);
        let pairing = creator
            .pair_with_user("usr_test123", PairingSource::AutoCli)
            .unwrap();
        assert_eq!(creator.user_id, Some("usr_test123".to_string()));
        assert_eq!(pairing.creator_id, cid);
        assert_eq!(pairing.status, "active");
    }

    #[test]
    fn test_persist_experience_requires_pairing() {
        let creator = Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        assert!(!creator.can_persist_experience());
    }

    #[test]
    fn test_paired_creator_can_persist_experience() {
        let mut creator =
            Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        creator
            .pair_with_user("usr_test", PairingSource::AutoCli)
            .unwrap();
        assert!(creator.can_persist_experience());
    }

    #[test]
    fn test_platform_owned_creator() {
        let creator = Creator::register(
            &make_creator_id(),
            "Bot",
            RegistrationSource::Platform,
            true,
        );
        assert_eq!(creator.is_platform_owned, Some(true));
        assert_eq!(creator.registration_source, "platform");
    }

    #[test]
    fn test_style_profile_update() {
        let mut creator =
            Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        creator.update_style_profile(
            vec!["dark".to_string()],
            vec!["first-person".to_string()],
            vec!["cliché".to_string()],
        );
        let sp = creator.style_profile.as_ref().unwrap();
        assert_eq!(sp.tone.as_deref(), Some(&["dark".to_string()][..]));
        assert_eq!(
            sp.narrative_preferences.as_deref(),
            Some(&["first-person".to_string()][..])
        );
        assert_eq!(
            sp.forbidden_patterns.as_deref(),
            Some(&["cliché".to_string()][..])
        );
    }

    #[test]
    fn test_registration_sources() {
        let cli = Creator::register(&make_creator_id(), "C", RegistrationSource::Cli, false);
        let web = Creator::register(&make_creator_id(), "W", RegistrationSource::WebAgent, false);
        let plat = Creator::register(&make_creator_id(), "P", RegistrationSource::Platform, false);
        assert_eq!(cli.registration_source, "cli");
        assert_eq!(web.registration_source, "web_agent");
        assert_eq!(plat.registration_source, "platform");
    }

    #[test]
    fn test_archive_and_lock() {
        let mut creator =
            Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        creator.archive().unwrap();
        assert_eq!(creator.status, "archived");

        // Locking archived should work (admin override)
        creator.lock().unwrap();
        assert_eq!(creator.status, "locked");
    }

    #[test]
    fn test_unpair_non_paired() {
        let mut creator =
            Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        assert!(matches!(
            creator.unpair(),
            Err(DomainError::CreatorNotPaired)
        ));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let creator = Creator::register(&make_creator_id(), "Test", RegistrationSource::Cli, false);
        let json = serde_json::to_string(&creator).unwrap();
        let deserialized: Creator = serde_json::from_str(&json).unwrap();
        assert_eq!(creator.creator_id, deserialized.creator_id);
        assert_eq!(creator.status, deserialized.status);
    }
}
