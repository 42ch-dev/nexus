//! KB query and result types for World-scoped narrative KB operations.

use nexus_contracts::BlockType;

/// Query parameters for KB search operations.
///
/// All queries are scoped by `world_id` (required).
/// Additional filters are optional and combined with AND logic.
#[derive(Debug, Clone)]
pub struct KbQuery {
    /// World scope — required for every query.
    pub world_id: String,
    /// Filter by block type.
    pub block_type: Option<BlockType>,
    /// Filter by exact canonical name.
    pub canonical_name: Option<String>,
    /// Text search within block content (summary, tags, `canonical_name`).
    pub text_search: Option<String>,
    /// Filter by computable flag (V1.61 P1).
    /// - `Some(true)` — only blocks with `computable: true`
    /// - `Some(false)` — only blocks with `computable: false` or absent
    /// - `None` — no filter (default)
    pub computable: Option<bool>,
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Number of results to skip (for pagination).
    pub offset: Option<usize>,
}

impl KbQuery {
    /// Create a new query scoped to the given world.
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

    /// Filter by block type.
    #[must_use]
    pub const fn with_block_type(mut self, block_type: BlockType) -> Self {
        self.block_type = Some(block_type);
        self
    }

    /// Filter by exact canonical name.
    #[must_use]
    pub fn with_canonical_name(mut self, name: &str) -> Self {
        self.canonical_name = Some(name.to_string());
        self
    }

    /// Add text search filter (case-insensitive substring match).
    #[must_use]
    pub fn with_text_search(mut self, text: &str) -> Self {
        self.text_search = Some(text.to_string());
        self
    }

    /// Filter by computable flag (V1.61 P1).
    ///
    /// - `Some(true)` — only blocks with `body.computable == true`
    /// - `Some(false)` — only blocks with `body.computable` absent or false
    /// - `None` — no filter (default)
    #[must_use]
    pub const fn with_computable(mut self, computable: Option<bool>) -> Self {
        self.computable = computable;
        self
    }

    /// Set result limit.
    #[must_use]
    pub const fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set result offset.
    #[must_use]
    pub const fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Result of inserting a `KeyBlock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbInsertResult {
    /// ID of the created `KeyBlock`.
    pub key_block_id: String,
    /// World the `KeyBlock` belongs to.
    pub world_id: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Result of querying `KeyBlocks`.
#[derive(Debug, Clone)]
pub struct KbQueryResult {
    /// Matching `KeyBlocks` (after pagination).
    pub items: Vec<crate::key_block::KeyBlock>,
    /// Total number of matching items (ignoring limit/offset).
    pub total_count: usize,
    /// Whether more results exist beyond the current page.
    pub has_more: bool,
}
