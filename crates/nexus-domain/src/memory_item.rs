//! MemoryItem aggregate — structured memory for creator experience and world context.
//!
//! MemoryItem carries canon, working, and experience memories with provenance
//! tracking. See data-model-v1.md §5.8, consistency-rules-v1.md §3.6.

use crate::errors::DomainError;
use crate::MemoryType;
use serde::{Deserialize, Serialize};

/// Memory status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Superseded,
    Archived,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }
}

/// Memory kind enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Generic,
    StorySummary,
    ResearchMaterial,
    ReviewNote,
}

impl MemoryKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Generic => "generic",
            Self::StorySummary => "story_summary",
            Self::ResearchMaterial => "research_material",
            Self::ReviewNote => "review_note",
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

/// MemoryItem aggregate — structured memory entity.
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

    /// Archive this memory item.
    pub fn archive(&mut self) -> Result<(), DomainError> {
        if self.status == MemoryStatus::Archived.as_str() {
            return Err(DomainError::AlreadyInState("archived".to_string()));
        }
        self.status = MemoryStatus::Archived.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

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
            memory_kind: c.memory_kind,
            status: c.status,
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

impl From<MemoryItem> for nexus_contracts::Memory {
    fn from(d: MemoryItem) -> Self {
        Self {
            schema_version: d.schema_version,
            memory_item_id: d.memory_item_id,
            creator_id: d.creator_id,
            world_id: d.world_id,
            memory_type: d.memory_type,
            memory_kind: d.memory_kind,
            status: d.status,
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
        let mi = MemoryItem::new("ctr_test", "wld_test", MemoryType::Canon, Some("generic"));
        assert_eq!(mi.memory_type, MemoryType::Canon);
        assert_eq!(mi.status, "active");
        assert!(mi.memory_item_id.starts_with("mem_"));
        assert_eq!(mi.schema_version, 1);
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
            let mi = MemoryItem::new("ctr_test", "wld_test", mt.clone(), None);
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
}
