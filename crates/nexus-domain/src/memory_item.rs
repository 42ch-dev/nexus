//! `MemoryItem` aggregate — structured memory for creator experience and world context.
//!
//! `MemoryItem` carries canon, working, and experience memories with provenance
//! tracking. See data-model-v1.md §5.8, consistency-rules-v1.md §3.6.

use crate::errors::DomainError;
use crate::MemoryType;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Memory status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Superseded,
    Archived,
}

impl MemoryStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }
}

/// Memory kind enum - matches v1-spec §5.8 and ADR-001.
///
/// Schema defines: `story_summary`, `research_material`, review_note, character_note,
/// `world_building`, `plot_outline`, theme_analysis, personality_core, custom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MemoryKind {
    StorySummary,
    ResearchMaterial,
    ReviewNote,
    CharacterNote,
    WorldBuilding,
    PlotOutline,
    ThemeAnalysis,
    /// Personality track pushed from SOUL.md (spec §4.2).
    PersonalityCore,
    Custom,
}

impl MemoryKind {
    /// Get the string representation (same as Display via strum).
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::StorySummary => "story_summary",
            Self::ResearchMaterial => "research_material",
            Self::ReviewNote => "review_note",
            Self::CharacterNote => "character_note",
            Self::WorldBuilding => "world_building",
            Self::PlotOutline => "plot_outline",
            Self::ThemeAnalysis => "theme_analysis",
            Self::PersonalityCore => "personality_core",
            Self::Custom => "custom",
        }
    }
    #[must_use]
    /// Get all valid memory kinds as strings.
    pub fn all_as_strings() -> Vec<String> {
        vec![
            "story_summary".to_string(),
            "research_material".to_string(),
            "review_note".to_string(),
            "character_note".to_string(),
            "world_building".to_string(),
            "plot_outline".to_string(),
            "theme_analysis".to_string(),
            "personality_core".to_string(),
            "custom".to_string(),
        ]
    }
}

/// Error type for `MemoryKind` parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMemoryKindError(String);

impl std::fmt::Display for ParseMemoryKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid memory kind: {}", self.0)
    }
}

impl std::error::Error for ParseMemoryKindError {}

impl FromStr for MemoryKind {
    type Err = ParseMemoryKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "story_summary" => Ok(Self::StorySummary),
            "research_material" => Ok(Self::ResearchMaterial),
            "review_note" => Ok(Self::ReviewNote),
            "character_note" => Ok(Self::CharacterNote),
            "world_building" => Ok(Self::WorldBuilding),
            "plot_outline" => Ok(Self::PlotOutline),
            "theme_analysis" => Ok(Self::ThemeAnalysis),
            "personality_core" => Ok(Self::PersonalityCore),
            "custom" => Ok(Self::Custom),
            _ => Err(ParseMemoryKindError(s.to_string())),
        }
    }
}

/// Source reference for provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRef {
    pub kind: String,
    pub id: String,
}

/// Creator quota for validation.
#[derive(Debug, Clone)]
pub struct CreatorQuota {
    pub max_canon_memories: usize,
    pub max_working_memories: usize,
    pub max_experience_memories: usize,
}

impl Default for CreatorQuota {
    fn default() -> Self {
        Self {
            max_canon_memories: 1000,
            max_working_memories: 500,
            max_experience_memories: 200,
        }
    }
}

/// `MemoryItem` aggregate — structured memory entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub schema_version: u32,
    pub memory_item_id: String,
    pub creator_id: String,
    pub world_id: String,
    pub memory_type: MemoryType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_kind: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_refs: Option<Vec<SourceRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reinforced_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl MemoryItem {
    /// Create a new memory item.
    /// Precondition: creator must have active pairing (for persistence).
    #[must_use]
    pub fn new(
        creator_id: &str,
        world_id: &str,
        memory_type: MemoryType,
        memory_kind: Option<&str>,
    ) -> Self {
        let memory_item_id = format!("mem_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            memory_item_id,
            creator_id: creator_id.to_string(),
            world_id: world_id.to_string(),
            memory_type,
            memory_kind: memory_kind.map(|s| s.to_string()),
            status: MemoryStatus::Active.as_str().to_string(),
            summary: None,
            embedding_ref: None,
            source_refs: None,
            last_accessed_at: None,
            last_reinforced_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Transition status: active → superseded.
    pub fn supersede(&mut self, _replacement_id: &str) -> Result<(), DomainError> {
        if self.status != MemoryStatus::Active.as_str() {
            return Err(DomainError::InvalidState {
                expected: "active".to_string(),
                actual: self.status.clone(),
            });
        }
        self.status = MemoryStatus::Superseded.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Archive this memory item.
    pub fn archive(&mut self) -> Result<(), DomainError> {
        if self.status == MemoryStatus::Archived.as_str() {
            return Err(DomainError::AlreadyInState("archived".to_string()));
        }
        self.status = MemoryStatus::Archived.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Record access for decay/reinforcement weighting.
    pub fn record_access(&mut self) {
        self.last_accessed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Record reinforcement (e.g., from context assembly hit).
    pub fn record_reinforcement(&mut self) {
        self.last_reinforced_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Add a source reference for provenance.
    pub fn add_source_ref(&mut self, kind: &str, id: &str) {
        let refs = self.source_refs.get_or_insert_with(Vec::new);
        if !refs.iter().any(|r| r.kind == kind && r.id == id) {
            refs.push(SourceRef {
                kind: kind.to_string(),
                id: id.to_string(),
            });
        }
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Validate creator/world scope and quota.
    /// Per consistency-rules-v1.md §3.6.
    pub fn validate_scope(&self, _creator_quota: &CreatorQuota) -> Result<(), DomainError> {
        // Domain-level validation: ensure active status allows operations
        if self.status == MemoryStatus::Archived.as_str() {
            return Err(DomainError::ValidationError(
                "archived memory cannot be used".to_string(),
            ));
        }
        // Full quota validation would require a repository query
        // (how many active memories of this type exist for this creator/world)
        // Here we validate structural correctness only.
        Ok(())
    }

    /// Check if this memory is active and accessible.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == MemoryStatus::Active.as_str()
    }

    /// Set summary text.
    pub fn set_summary(&mut self, summary: &str) {
        self.summary = Some(summary.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Set embedding reference.
    pub fn set_embedding_ref(&mut self, ref_id: &str) {
        self.embedding_ref = Some(ref_id.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::Memory> for MemoryItem {
    fn from(c: nexus_contracts::Memory) -> Self {
        Self {
            schema_version: c.schema_version,
            memory_item_id: c.memory_item_id,
            creator_id: c.creator_id,
            world_id: c.world_id,
            memory_type: c.memory_type,
            memory_kind: c.memory_kind.map(|k| k.as_str().to_string()),
            status: c.status.as_str().to_string(),
            summary: c.summary,
            embedding_ref: c.embedding_ref,
            source_refs: c.source_refs.map(|refs| {
                refs.into_iter()
                    .map(|r| SourceRef {
                        kind: r.kind,
                        id: r.id,
                    })
                    .collect()
            }),
            last_accessed_at: c.last_accessed_at,
            last_reinforced_at: c.last_reinforced_at,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<MemoryItem> for nexus_contracts::Memory {
    fn from(d: MemoryItem) -> Self {
        Self {
            schema_version: d.schema_version,
            memory_item_id: d.memory_item_id,
            creator_id: d.creator_id,
            world_id: d.world_id,
            memory_type: d.memory_type,
            memory_kind: d.memory_kind.as_ref().map(|s| {
                let wire = if s == "generic" { "custom" } else { s.as_str() };
                nexus_contracts::MemoryKind::from_str(wire).unwrap()
            }),
            status: nexus_contracts::MemoryStatus::from_str(&d.status).unwrap(),
            summary: d.summary,
            embedding_ref: d.embedding_ref,
            source_refs: d.source_refs.map(|refs| {
                refs.into_iter()
                    .map(|r| nexus_contracts::MemorySourceRef {
                        kind: r.kind,
                        id: r.id,
                    })
                    .collect()
            }),
            last_accessed_at: d.last_accessed_at,
            last_reinforced_at: d.last_reinforced_at,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_canon_memory() {
        let mi = MemoryItem::new(
            "ctr_test",
            "wld_test",
            MemoryType::Canon,
            Some("story_summary"),
        );
        assert_eq!(mi.memory_type, MemoryType::Canon);
        assert_eq!(mi.status, "active");
        assert!(mi.memory_item_id.starts_with("mem_"));
        assert_eq!(mi.schema_version, 1);
        assert_eq!(mi.memory_kind, Some("story_summary".to_string()));
    }

    #[test]
    fn test_supersede_transition() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
        mi.supersede("mem_replacement").unwrap();
        assert_eq!(mi.status, "superseded");
    }

    #[test]
    fn test_archive_active_memory() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Working, None);
        mi.archive().unwrap();
        assert_eq!(mi.status, "archived");
    }

    #[test]
    fn test_record_access_updates_timestamp() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Experience, None);
        assert!(mi.last_accessed_at.is_none());
        mi.record_access();
        assert!(mi.last_accessed_at.is_some());
    }

    #[test]
    fn test_all_memory_types() {
        let types = vec![
            MemoryType::Canon,
            MemoryType::Working,
            MemoryType::Experience,
        ];
        for mt in types {
            let mi = MemoryItem::new("ctr_test", "wld_test", mt, None);
            let json = serde_json::to_string(&mi).unwrap();
            let deserialized: MemoryItem = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.memory_type, mt);
        }
    }

    #[test]
    fn test_source_refs_accumulation() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
        mi.add_source_ref("command", "cmd_1");
        mi.add_source_ref("command", "cmd_2");
        mi.add_source_ref("command", "cmd_3");
        assert_eq!(mi.source_refs.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_source_refs_deduplication() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
        mi.add_source_ref("command", "cmd_1");
        mi.add_source_ref("command", "cmd_1");
        assert_eq!(mi.source_refs.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_record_reinforcement() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
        assert!(mi.last_reinforced_at.is_none());
        mi.record_reinforcement();
        assert!(mi.last_reinforced_at.is_some());
    }

    #[test]
    fn test_validate_scope_archived() {
        let mut mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, None);
        mi.archive().unwrap();
        let quota = CreatorQuota::default();
        assert!(mi.validate_scope(&quota).is_err());
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut mi = MemoryItem::new(
            "ctr_test",
            "wld_test",
            MemoryType::Working,
            Some("story_summary"),
        );
        mi.set_summary("Chapter summary");
        mi.add_source_ref("command", "cmd_1");
        let json = serde_json::to_string(&mi).unwrap();
        let deserialized: MemoryItem = serde_json::from_str(&json).unwrap();
        assert_eq!(mi, deserialized);
    }

    // ── MemoryKind Enum Tests ──────────────────────────────────────

    #[test]
    fn test_memory_kind_all_variants() {
        let kinds = vec![
            MemoryKind::StorySummary,
            MemoryKind::ResearchMaterial,
            MemoryKind::ReviewNote,
            MemoryKind::CharacterNote,
            MemoryKind::WorldBuilding,
            MemoryKind::PlotOutline,
            MemoryKind::ThemeAnalysis,
            MemoryKind::PersonalityCore,
            MemoryKind::Custom,
        ];
        assert_eq!(kinds.len(), 9);
    }

    #[test]
    fn test_memory_kind_serialize_snake_case() {
        assert_eq!(MemoryKind::StorySummary.to_string(), "story_summary");
        assert_eq!(
            MemoryKind::ResearchMaterial.to_string(),
            "research_material"
        );
        assert_eq!(MemoryKind::ReviewNote.to_string(), "review_note");
        assert_eq!(MemoryKind::CharacterNote.to_string(), "character_note");
        assert_eq!(MemoryKind::WorldBuilding.to_string(), "world_building");
        assert_eq!(MemoryKind::PlotOutline.to_string(), "plot_outline");
        assert_eq!(MemoryKind::ThemeAnalysis.to_string(), "theme_analysis");
        assert_eq!(MemoryKind::PersonalityCore.to_string(), "personality_core");
        assert_eq!(MemoryKind::Custom.to_string(), "custom");
    }

    #[test]
    fn test_memory_kind_deserialize_snake_case() {
        let json = r#""story_summary""#;
        let kind: MemoryKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, MemoryKind::StorySummary);

        let json = r#""character_note""#;
        let kind: MemoryKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, MemoryKind::CharacterNote);

        let json = r#""custom""#;
        let kind: MemoryKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, MemoryKind::Custom);
    }

    #[test]
    fn test_memory_kind_roundtrip_json() {
        for kind_str in &[
            "story_summary",
            "research_material",
            "review_note",
            "character_note",
            "world_building",
            "plot_outline",
            "theme_analysis",
            "personality_core",
            "custom",
        ] {
            let json = format!(r#""{}""#, kind_str);
            let parsed: MemoryKind = serde_json::from_str(&json).unwrap();
            let serialized = serde_json::to_string(&parsed).unwrap();
            assert_eq!(serialized, json);
        }
    }

    #[test]
    fn test_memory_kind_from_str() {
        use std::str::FromStr;
        assert!(MemoryKind::from_str("story_summary").is_ok());
        assert!(MemoryKind::from_str("personality_core").is_ok());
        assert!(MemoryKind::from_str("invalid_kind").is_err());
        assert_eq!(MemoryKind::from_str("custom").unwrap(), MemoryKind::Custom);
    }

    #[test]
    fn test_memory_kind_all_as_strings() {
        let strings = MemoryKind::all_as_strings();
        assert_eq!(strings.len(), 9);
        assert!(strings.contains(&"story_summary".to_string()));
        assert!(strings.contains(&"personality_core".to_string()));
        assert!(strings.contains(&"custom".to_string()));
    }
}
