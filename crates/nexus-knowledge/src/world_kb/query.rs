//! KB query and result types for World-scoped narrative KB operations.
// note: query types are re-exported from the crate root.

use nexus_contracts::BlockType;

/// note: Query parameters for KB search operations.
///
/// All queries are scoped by `world_id` (required).
/// Additional filters are optional and combined with AND logic.
#[derive(Debug, Clone)]
pub struct KbQuery {
    /// note: World scope — required for every query.
    pub world_id: String,
    /// note: Filter by block type.
    pub block_type: Option<BlockType>,
    /// note: Filter by exact canonical name.
    pub canonical_name: Option<String>,
    /// note: Text search within block content (summary, tags, `canonical_name`).
    pub text_search: Option<String>,
    /// note: Filter by computable flag (V1.61 P1).
    /// - `Some(true)` — only blocks with `computable: true`
    /// - `Some(false)` — only blocks with `computable: false` or absent
    /// - `None` — no filter (default)
    pub computable: Option<bool>,
    /// note: Maximum number of results.
    pub limit: Option<usize>,
    /// note: Number of results to skip (for pagination).
    pub offset: Option<usize>,
}

impl KbQuery {
    /// note: Create a new query scoped to the given world.
    #[must_use]
    pub fn new(world_id: &str) -> Self {
        Self {
            world_id: world_id.to_string(),
            block_type: None,
            canonical_name: None,
            text_search: None,
            computable: None,
            limit: None,
            offset: None,
        }
    }

    /// note: Filter by block type.
    #[must_use]
    pub const fn with_block_type(mut self, block_type: BlockType) -> Self {
        self.block_type = Some(block_type);
        self
    }

    /// note: Filter by exact canonical name.
    #[must_use]
    pub fn with_canonical_name(mut self, name: &str) -> Self {
        self.canonical_name = Some(name.to_string());
        self
    }

    /// note: Add text search filter (case-insensitive substring match).
    #[must_use]
    pub fn with_text_search(mut self, text: &str) -> Self {
        self.text_search = Some(text.to_string());
        self
    }

    /// note: Filter by computable flag (V1.61 P1).
    ///
    /// - `Some(true)` — only blocks with `body.computable == true`
    /// - `Some(false)` — only blocks with `body.computable` absent or false
    /// - `None` — no filter (default)
    #[must_use]
    pub const fn with_computable(mut self, computable: Option<bool>) -> Self {
        self.computable = computable;
        self
    }

    /// note: Set result limit.
    #[must_use]
    pub const fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// note: Set result offset.
    #[must_use]
    pub const fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// note: Result of inserting a `KnowledgeEntryRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbInsertResult {
    /// note: ID of the created `KnowledgeEntryRecord`.
    pub entry_id: String,
    /// note: Canonical owner of the created record (v1.184 P1).
    pub owner: crate::world_kb::knowledge_entry::KnowledgeOwnerRef,
    /// note: Creation timestamp.
    pub created_at: String,
}

/// note: Result of querying `KnowledgeEntryRecord`s.
#[derive(Debug, Clone)]
pub struct KbQueryResult {
    /// note: Matching `KnowledgeEntryRecord`s (after pagination).
    pub items: Vec<crate::world_kb::knowledge_entry::KnowledgeEntryRecord>,
    /// note: Total number of matching items (ignoring limit/offset).
    pub total_count: usize,
    /// note: Whether more results exist beyond the current page.
    pub has_more: bool,
}
