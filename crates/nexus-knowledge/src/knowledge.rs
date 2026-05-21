//! Core domain types for User-scoped global knowledge.
//!
//! `nexus-knowledge` owns User-scoped knowledge entries: tag-driven items
//! that may be pulled into Moment context assembly. This is NOT Creator-scoped
//! and does NOT own narrative `KeyBlocks` (those live in `nexus-kb`).

use serde::{Deserialize, Serialize};

/// A User-scoped knowledge entry — tag-driven, globally indexed per user.
///
/// Each entry belongs to exactly one `user_id` (User scope per entity-scope-model §5.2).
/// Tags drive classification and lookup; content is inline text or a reference URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeEntry {
    /// Unique entry identifier (UUID v4, prefixed `kno_`).
    pub id: String,
    /// Owning user scope — all operations are scoped by this field.
    pub user_id: String,
    /// Classification tags for index lookup.
    pub tags: Vec<KnowledgeTag>,
    /// Inline knowledge content (text, excerpt, or summary).
    pub content: String,
    /// Optional URI linking back to an external source or reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_uri: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp (set on creation too).
    pub updated_at: String,
}

impl KnowledgeEntry {
    /// Create a new knowledge entry for the given user.
    ///
    /// Generates a UUID-based ID and sets timestamps to now.
    #[must_use]
    pub fn new(user_id: &str, tags: Vec<KnowledgeTag>, content: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: format!("kno_{}", uuid::Uuid::new_v4().simple()),
            user_id: user_id.to_string(),
            tags,
            content: content.to_string(),
            reference_uri: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Attach a reference URI to this entry.
    pub fn with_reference_uri(&mut self, uri: String) {
        self.reference_uri = Some(uri);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Check if this entry contains **all** the given tags.
    pub fn has_all_tags(&self, required: &[KnowledgeTag]) -> bool {
        required.iter().all(|t| self.tags.contains(t))
    }

    /// Check if the content contains the given substring (case-insensitive).
    pub fn content_contains(&self, query: &str) -> bool {
        self.content.to_lowercase().contains(&query.to_lowercase())
    }
}

/// A classification tag for knowledge entries.
///
/// Tags are simple strings used for index-driven lookup.
/// Uniqueness is by exact string match (case-sensitive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KnowledgeTag(pub String);

impl KnowledgeTag {
    /// Create a new tag.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        Self(tag.to_string())
    }

    /// Get the tag string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for KnowledgeTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Parameters for querying knowledge entries.
///
/// All queries are scoped to a single `user_id` (User scope invariant).
/// Filters are applied as AND conditions: entries must match all specified criteria.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeQuery {
    /// Required: the user whose knowledge to search.
    pub user_id: String,
    /// Optional: only return entries containing ALL of these tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<KnowledgeTag>>,
    /// Optional: only return entries whose content contains this text (case-insensitive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Maximum number of entries to return. Defaults to 50 if not set.
    pub limit: Option<u32>,
    /// Number of entries to skip (for pagination). Defaults to 0 if not set.
    pub offset: Option<u32>,
}

impl KnowledgeQuery {
    /// Create a query scoped to a specific user.
    #[must_use]
    pub fn for_user(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            tags: None,
            text: None,
            limit: None,
            offset: None,
        }
    }

    /// Add a tag filter (entries must contain ALL specified tags).
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<KnowledgeTag>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Add a text filter (substring match, case-insensitive).
    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// Set pagination limit.
    #[must_use]
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set pagination offset.
    #[must_use]
    pub const fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Effective limit, defaulting to 50.
    #[must_use]
    pub fn effective_limit(&self) -> u32 {
        self.limit.unwrap_or(50)
    }

    /// Effective offset, defaulting to 0.
    #[must_use]
    pub fn effective_offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

/// Paginated result of a knowledge query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeResult {
    /// Matching entries for the current page.
    pub entries: Vec<KnowledgeEntry>,
    /// Total number of matching entries (across all pages).
    pub total_count: u32,
    /// The limit used for this query.
    pub limit: u32,
    /// The offset used for this query.
    pub offset: u32,
}

impl KnowledgeResult {
    /// Create a result from matched entries and total count.
    #[must_use]
    pub const fn new(
        entries: Vec<KnowledgeEntry>,
        total_count: u32,
        limit: u32,
        offset: u32,
    ) -> Self {
        Self {
            entries,
            total_count,
            limit,
            offset,
        }
    }

    /// Whether there are more pages available.
    #[must_use]
    pub fn has_more(&self) -> bool {
        let entry_count: u32 = self.entries.len().try_into().unwrap_or(u32::MAX);
        self.offset + entry_count < self.total_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_entry_new_generates_id_and_timestamps() {
        let entry = KnowledgeEntry::new(
            "user_abc",
            vec![KnowledgeTag::new("rust"), KnowledgeTag::new("tutorial")],
            "Rust ownership model basics",
        );
        assert!(entry.id.starts_with("kno_"));
        assert_eq!(entry.user_id, "user_abc");
        assert_eq!(entry.tags.len(), 2);
        assert_eq!(entry.content, "Rust ownership model basics");
        assert!(entry.reference_uri.is_none());
        assert!(!entry.created_at.is_empty());
        assert!(!entry.updated_at.is_empty());
    }

    #[test]
    fn knowledge_entry_has_all_tags() {
        let entry = KnowledgeEntry::new(
            "user_1",
            vec![KnowledgeTag::new("a"), KnowledgeTag::new("b")],
            "content",
        );
        assert!(entry.has_all_tags(&[KnowledgeTag::new("a")]));
        assert!(entry.has_all_tags(&[KnowledgeTag::new("a"), KnowledgeTag::new("b")]));
        assert!(!entry.has_all_tags(&[KnowledgeTag::new("a"), KnowledgeTag::new("c")]));
        assert!(entry.has_all_tags(&[]));
    }

    #[test]
    fn knowledge_entry_content_contains_case_insensitive() {
        let entry = KnowledgeEntry::new("user_1", vec![], "Hello World");
        assert!(entry.content_contains("hello"));
        assert!(entry.content_contains("WORLD"));
        assert!(entry.content_contains("lo wo"));
        assert!(!entry.content_contains("xyz"));
    }

    #[test]
    fn knowledge_entry_with_reference_uri() {
        let mut entry = KnowledgeEntry::new("user_1", vec![], "content");
        entry.with_reference_uri("https://example.com".to_string());
        assert_eq!(entry.reference_uri, Some("https://example.com".to_string()));
    }

    #[test]
    fn knowledge_query_builder() {
        let query = KnowledgeQuery::for_user("user_1")
            .with_tags(vec![KnowledgeTag::new("rust")])
            .with_text("ownership")
            .with_limit(10)
            .with_offset(20);
        assert_eq!(query.user_id, "user_1");
        assert_eq!(query.tags.as_ref().map(|t| t.len()), Some(1));
        assert_eq!(query.text.as_deref(), Some("ownership"));
        assert_eq!(query.effective_limit(), 10);
        assert_eq!(query.effective_offset(), 20);
    }

    #[test]
    fn knowledge_query_defaults() {
        let query = KnowledgeQuery::for_user("user_1");
        assert_eq!(query.effective_limit(), 50);
        assert_eq!(query.effective_offset(), 0);
    }

    #[test]
    fn knowledge_result_has_more() {
        let entries = vec![KnowledgeEntry::new("u1", vec![], "a")];
        let result = KnowledgeResult::new(entries, 10, 1, 0);
        assert!(result.has_more());
    }

    #[test]
    fn knowledge_result_no_more() {
        let entries = vec![KnowledgeEntry::new("u1", vec![], "a")];
        let result = KnowledgeResult::new(entries, 1, 50, 0);
        assert!(!result.has_more());
    }

    #[test]
    fn knowledge_tag_display() {
        let tag = KnowledgeTag::new("my-tag");
        assert_eq!(tag.to_string(), "my-tag");
        assert_eq!(tag.as_str(), "my-tag");
    }

    #[test]
    fn serialize_roundtrip() {
        let entry = KnowledgeEntry::new("user_1", vec![KnowledgeTag::new("rust")], "Some content");
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: KnowledgeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }
}
