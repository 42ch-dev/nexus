//! Moment-scoped context assembly — aggregates from all local domains.
//!
//! The [`MomentAssembly`] pulls context from four domain sources:
//!
//! 1. **Creator Memory** (`nexus-creator-memory`): SOUL sections, long-term memories,
//!    fragment keywords (via [`Stage0Assembly`]).
//! 2. **Narrative** (`nexus-narrative`): world state, timeline position, event snapshot
//!    (via [`NarrativeGateway`](nexus_narrative::NarrativeGateway)).
//! 3. **Knowledge Base** (`nexus-kb`): World-scoped KB assets / key blocks
//!    (via [`KbStore`](nexus_kb::KbStore)).
//! 4. **Knowledge** (`nexus-knowledge`): User-scoped knowledge entries
//!    (via [`KnowledgeStore`](nexus_knowledge::KnowledgeStore)).
//!
//! # Entity scope model (§4)
//!
//! `nexus-moment-context-assembly` owns the **Moment** scope — the per-interaction
//! context window assembled from all domain sources for a single ACP session turn.
//!
//! # Async
//!
//! Domain store queries are async. Callers must provide concrete implementations
//! of the store traits. The crate provides no default runtime or storage backend.

use crate::stage0::{Stage0Assembly, STAGE0_PERSONALITY_END, STAGE0_PERSONALITY_START};
use nexus_contracts::BlockType;
use nexus_kb::{KbQuery, KbStore};
use nexus_knowledge::KnowledgeStore;
use nexus_narrative::NarrativeGateway;

/// Section heading for World State in assembled context.
const WORLD_STATE_HEADING: &str = "## World State";

/// Section heading for Timeline in assembled context.
const TIMELINE_HEADING: &str = "## Timeline";

/// Section heading for World Knowledge Base in assembled context.
const WORLD_KB_HEADING: &str = "## World Knowledge Base";

/// Section heading for User Knowledge in assembled context.
const USER_KNOWLEDGE_HEADING: &str = "## User Knowledge";

/// Parameters for a single moment context assembly request.
///
/// All IDs are strings for now (matching current domain APIs).
/// Fields left as `None` indicate that domain source should be skipped.
#[derive(Debug, Clone)]
pub struct MomentRequest {
    /// World ID to pull narrative state and KB assets for.
    pub world_id: Option<String>,
    /// Branch ID within the world (optional, for fork-specific context).
    pub branch_id: Option<String>,
    /// Event ID to focus context around (optional).
    pub event_id: Option<String>,
    /// User ID to pull knowledge entries for.
    pub user_id: Option<String>,
    /// Stage-0 assembly inputs (SOUL, memories, fragments, prompt).
    pub stage0: Stage0Assembly,
    /// Cross-domain token budget (approximate chars/4 heuristic).
    /// When set, applies truncation to domain sections after Stage-0 personality.
    /// Personality section inside Stage-0 is never truncated.
    pub max_tokens: Option<usize>,
    /// KB query: maximum number of key blocks to return.
    pub kb_limit: Option<usize>,
    /// KB query: text search filter (case-insensitive substring).
    pub kb_text_search: Option<String>,
    /// KB query: filter by block type.
    pub kb_block_type: Option<BlockType>,
    /// User knowledge query: maximum number of entries to return.
    pub knowledge_limit: Option<usize>,
}

impl MomentRequest {
    /// Create a minimal request with only Stage-0 inputs.
    #[must_use]
    pub const fn new(stage0: Stage0Assembly) -> Self {
        Self {
            world_id: None,
            branch_id: None,
            event_id: None,
            user_id: None,
            stage0,
            max_tokens: None,
            kb_limit: None,
            kb_text_search: None,
            kb_block_type: None,
            knowledge_limit: None,
        }
    }

    /// Set the world context (world ID, optional branch, optional event).
    #[must_use]
    pub fn with_world(mut self, world_id: impl Into<String>) -> Self {
        self.world_id = Some(world_id.into());
        self
    }

    /// Set the branch ID within the world.
    #[must_use]
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// Set the focused event ID.
    #[must_use]
    pub fn with_event(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    /// Set the user ID for knowledge lookup.
    #[must_use]
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set cross-domain token budget.
    #[must_use]
    pub const fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set KB result limit.
    #[must_use]
    pub const fn with_kb_limit(mut self, limit: usize) -> Self {
        self.kb_limit = Some(limit);
        self
    }

    /// Set KB text search filter.
    #[must_use]
    pub fn with_kb_text_search(mut self, text: impl Into<String>) -> Self {
        self.kb_text_search = Some(text.into());
        self
    }

    /// Set KB block type filter.
    #[must_use]
    pub const fn with_kb_block_type(mut self, block_type: BlockType) -> Self {
        self.kb_block_type = Some(block_type);
        self
    }

    /// Set user knowledge result limit.
    #[must_use]
    pub const fn with_knowledge_limit(mut self, limit: usize) -> Self {
        self.knowledge_limit = Some(limit);
        self
    }
}

/// Assembled context from all domain sources for a single moment.
///
/// Each field is `Some(...)` if that domain source was queried successfully,
/// or `None` if it was skipped, failed, or returned no data.
#[derive(Debug, Clone, Default)]
pub struct MomentContext {
    /// Stage-0 context (always present — SOUL, memories, fragments, prompt).
    pub stage0_context: String,
    /// Narrative world state (if a `world_id` was provided and found).
    pub world_state: Option<String>,
    /// Timeline summary text (if available).
    pub timeline: Option<String>,
    /// World KB summary text (key blocks for the world).
    pub world_kb: Option<String>,
    /// User knowledge summary text (entries for the user).
    pub user_knowledge: Option<String>,
}

impl MomentContext {
    /// Assemble the full context string from all sources.
    ///
    /// Follows the spec ordering (§9.2) with domain extensions:
    /// 1. Stage-0 context (system prefix, personality, memories, keywords, experience, prompt)
    /// 2. World state (narrative)
    /// 3. Timeline (narrative)
    /// 4. World KB (key blocks)
    /// 5. User knowledge
    ///
    /// Empty sections are omitted.
    #[must_use]
    pub fn to_full_context(&self) -> String {
        let mut parts = Vec::new();

        if !self.stage0_context.is_empty() {
            parts.push(self.stage0_context.clone());
        }

        if let Some(ref ws) = self.world_state {
            if !ws.is_empty() {
                parts.push(format!("{WORLD_STATE_HEADING}\n\n{ws}\n"));
            }
        }

        if let Some(ref tl) = self.timeline {
            if !tl.is_empty() {
                parts.push(format!("{TIMELINE_HEADING}\n\n{tl}\n"));
            }
        }

        if let Some(ref kb) = self.world_kb {
            if !kb.is_empty() {
                parts.push(format!("{WORLD_KB_HEADING}\n\n{kb}\n"));
            }
        }

        if let Some(ref uk) = self.user_knowledge {
            if !uk.is_empty() {
                parts.push(format!("{USER_KNOWLEDGE_HEADING}\n\n{uk}\n"));
            }
        }

        parts.join("\n")
    }

    /// Apply cross-domain token budget truncation.
    ///
    /// Personality section inside Stage-0 is never truncated.
    /// The remaining budget (after personality) is distributed across
    /// `world_state`, `timeline`, `world_kb`, and `user_knowledge` in order.
    ///
    /// Token count uses chars/4 heuristic (spec §9.3).
    pub fn apply_cross_domain_truncation(&mut self, max_tokens: usize) {
        let max_chars = max_tokens.saturating_mul(4);

        // Extract personality from stage0_context — personality section is never truncated.
        let (personality_part, rest_stage0) = self.split_stage0_personality();

        let personality_chars = personality_part.chars().count();
        let mut remaining = max_chars.saturating_sub(personality_chars);

        // Truncate domain sections in priority order
        remaining = Self::truncate_section(&mut self.world_state, remaining);
        remaining = Self::truncate_section(&mut self.timeline, remaining);
        remaining = Self::truncate_section(&mut self.world_kb, remaining);
        remaining = Self::truncate_section(&mut self.user_knowledge, remaining);

        // Truncate remaining stage0 content (non-personality)
        if rest_stage0.chars().count() > remaining {
            self.stage0_context = if personality_part.is_empty() {
                Self::truncate_text(&rest_stage0, remaining)
            } else {
                format!(
                    "{personality_part}\n\n{}",
                    Self::truncate_text(&rest_stage0, remaining)
                )
            };
        }
    }

    /// Split `stage0_context` into (`personality_section`, rest).
    ///
    /// Prefers structured delimiter split (`---STAGE0:PERSONALITY:START---` /
    /// `---STAGE0:PERSONALITY:END---`). Falls back to the markdown-header
    /// heuristic for legacy content without delimiters.
    fn split_stage0_personality(&self) -> (String, String) {
        let ctx = &self.stage0_context;

        // Primary path: structured delimiters
        if let (Some(start_pos), Some(end_pos)) = (
            ctx.find(STAGE0_PERSONALITY_START),
            ctx.find(STAGE0_PERSONALITY_END),
        ) {
            let content_start = start_pos + STAGE0_PERSONALITY_START.len();
            if end_pos > content_start {
                let personality_section = ctx[content_start..end_pos].to_string();
                let rest = format!("{}{}", &ctx[..start_pos], &ctx[end_pos + STAGE0_PERSONALITY_END.len()..]);
                return (personality_section, rest);
            }
        }

        // Legacy fallback: markdown-header heuristic
        ctx.find("## Personality").map_or_else(
            || (String::new(), ctx.clone()),
            |pos| {
                let after_personality_header = &ctx[pos..];
                let end_of_personality = after_personality_header[14..] // skip "## Personality"
                    .find("\n## ")
                    .map_or(after_personality_header.len(), |i| 14 + i);

                let personality_section = ctx[pos..pos + end_of_personality].to_string();
                let rest = format!("{}{}", &ctx[..pos], &ctx[pos + end_of_personality..]);
                (personality_section, rest)
            },
        )
    }

    /// Truncate a section to fit within `max_chars`, returning remaining chars.
    fn truncate_section(section: &mut Option<String>, max_chars: usize) -> usize {
        section.as_mut().map_or(max_chars, |text| {
            let len = text.chars().count();
            if len > max_chars {
                *text = Self::truncate_text(text, max_chars);
                0
            } else {
                max_chars - len
            }
        })
    }

    /// Truncate text to at most `max_chars` characters, trying to break at line boundaries.
    fn truncate_text(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }
        let truncated: String = text.chars().take(max_chars).collect();
        // Try to break at last newline
        if let Some(pos) = truncated.rfind('\n') {
            truncated[..pos].to_string()
        } else {
            truncated
        }
    }
}

/// Assemble moment context from all domain sources.
///
/// This is the primary entry point for full moment context assembly.
/// It queries each domain source in sequence and combines the results.
///
/// # Errors
///
/// Individual domain failures are logged but do not fail the entire assembly.
/// If a domain source returns an error, its section is simply omitted from
/// the output. Only the Stage-0 assembly is guaranteed to be present.
///
/// # Type parameters
///
/// - `G`: A [`NarrativeGateway`] implementation for narrative state queries.
/// - `K`: A [`KbStore`] implementation for World-scoped KB queries.
/// - `S`: A [`KnowledgeStore`] implementation for User-scoped knowledge queries.
#[allow(clippy::future_not_send)]
pub async fn assemble_moment<G, K, S>(
    request: &MomentRequest,
    narrative: &G,
    kb_store: &K,
    knowledge: &S,
) -> MomentContext
where
    G: NarrativeGateway,
    K: KbStore,
    S: KnowledgeStore,
{
    // 1. Stage-0: always assemble from creator memory inputs
    let stage0_context = if request.stage0.max_tokens.is_some() {
        request.stage0.assemble_with_truncation()
    } else {
        request.stage0.assemble()
    };

    // 2. Narrative context (if world_id provided)
    let (world_state, timeline) = if let Some(ref world_id) = request.world_id {
        match fetch_narrative_context(narrative, world_id, request.branch_id.as_deref()).await {
            Ok((ws, tl)) => (ws, tl),
            Err(_) => (None, None),
        }
    } else {
        (None, None)
    };

    // 3. World KB (if world_id provided)
    let world_kb = if let Some(ref world_id) = request.world_id {
        match fetch_world_kb(kb_store, world_id, request).await {
            Ok(Some(kb_text)) => Some(kb_text),
            _ => None,
        }
    } else {
        None
    };

    // 4. User knowledge (if user_id provided)
    let user_knowledge = if let Some(ref user_id) = request.user_id {
        match fetch_user_knowledge(knowledge, user_id, request.knowledge_limit).await {
            Ok(Some(uk_text)) => Some(uk_text),
            _ => None,
        }
    } else {
        None
    };

    let mut ctx = MomentContext {
        stage0_context,
        world_state,
        timeline,
        world_kb,
        user_knowledge,
    };

    // 5. Cross-domain truncation if max_tokens set
    if let Some(max_tokens) = request.max_tokens {
        ctx.apply_cross_domain_truncation(max_tokens);
    }

    ctx
}

/// Fetch narrative context (world state + timeline) from the gateway.
// Traits use async fn in trait without Send bounds — same pattern as nexus-narrative.
#[allow(clippy::future_not_send)]
async fn fetch_narrative_context<G: NarrativeGateway>(
    gateway: &G,
    world_id: &str,
    branch_id: Option<&str>,
) -> Result<(Option<String>, Option<String>), nexus_narrative::NarrativeError> {
    let world_state_result = gateway.get_world_state(world_id).await;

    let world_state_text = world_state_result.ok().map(|ws| format_world_state(&ws));

    let timeline_text = match gateway.get_timeline(world_id, branch_id).await {
        Ok(events) if !events.is_empty() => Some(format_timeline(&events)),
        _ => None,
    };

    Ok((world_state_text, timeline_text))
}

/// Fetch World KB assets using structured query and format as context text.
#[allow(clippy::future_not_send)]
async fn fetch_world_kb<K: KbStore>(
    kb_store: &K,
    world_id: &str,
    request: &MomentRequest,
) -> Result<Option<String>, nexus_kb::KbStoreError> {
    let mut query = KbQuery::new(world_id);
    if let Some(limit) = request.kb_limit {
        query = query.with_limit(limit);
    }
    if let Some(ref text) = request.kb_text_search {
        query = query.with_text_search(text);
    }
    if let Some(block_type) = request.kb_block_type {
        query = query.with_block_type(block_type);
    }
    let result = kb_store.query(&query).await?;
    if result.items.is_empty() {
        return Ok(None);
    }
    let lines: Vec<String> = result
        .items
        .iter()
        .map(|kb| {
            let summary = kb
                .body
                .as_ref()
                .and_then(|b| b.summary.as_ref())
                .map_or("(no summary)", std::string::String::as_str);
            format!(
                "- **{}** [{:?}]: {summary}",
                kb.canonical_name, kb.block_type
            )
        })
        .collect();
    Ok(Some(lines.join("\n")))
}

/// Fetch User knowledge entries and format as context text.
async fn fetch_user_knowledge<S: KnowledgeStore>(
    knowledge: &S,
    user_id: &str,
    knowledge_limit: Option<usize>,
) -> Result<Option<String>, nexus_knowledge::KnowledgeError> {
    let limit = knowledge_limit.unwrap_or(20);
    let query = nexus_knowledge::KnowledgeQuery::for_user(user_id)
        .with_limit(u32::try_from(limit).unwrap_or(u32::MAX));
    let result = knowledge.list(&query).await?;
    if result.entries.is_empty() {
        return Ok(None);
    }
    let lines: Vec<String> = result
        .entries
        .iter()
        .map(|entry| {
            let tags = entry
                .tags
                .iter()
                .map(nexus_knowledge::KnowledgeTag::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            if tags.is_empty() {
                format!("- {}", entry.content)
            } else {
                format!("- [{}] {}", tags, entry.content)
            }
        })
        .collect();
    Ok(Some(lines.join("\n")))
}

/// Format a [`WorldState`] into a human-readable context string.
fn format_world_state(ws: &nexus_narrative::WorldState) -> String {
    let mut parts = Vec::new();
    parts.push(format!("**{}** ({})", ws.title, ws.world_id));
    parts.push(format!("Status: {}", ws.status));
    if ws.is_fork {
        parts.push("Fork: yes".to_string());
        if let Some(ref parent) = ws.parent_world_id {
            parts.push(format!("Parent: {parent}"));
        }
    }
    if let Some(ref head) = ws.current_timeline_head_id {
        parts.push(format!("Timeline head: {head}"));
    }
    parts.join("\n")
}

/// Format timeline events into a human-readable context string.
fn format_timeline(events: &[nexus_narrative::timeline_event::TimelineEvent]) -> String {
    let mut lines = Vec::new();
    for event in events {
        let title = event.title.as_deref().unwrap_or("(untitled)");
        let summary = event.summary.as_deref().unwrap_or("");
        let line = if summary.is_empty() {
            format!("- [{}] {} ({})", event.sequence_no, title, event.event_type)
        } else {
            format!(
                "- [{}] {} — {} ({})",
                event.sequence_no, title, summary, event.event_type
            )
        };
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_kb::InMemoryKbStore;
    use nexus_knowledge::InMemoryKnowledgeStore;
    use nexus_narrative::InMemoryNarrativeGateway;

    /// Helper: create a Stage0Assembly with minimal content.
    fn minimal_stage0() -> Stage0Assembly {
        Stage0Assembly {
            personality: "A creative writer.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        }
    }

    /// Helper: set up in-memory stores for testing.
    struct TestStores {
        narrative: InMemoryNarrativeGateway<InMemoryKbStore>,
        kb: InMemoryKbStore,
        knowledge: InMemoryKnowledgeStore,
    }

    impl TestStores {
        fn new() -> Self {
            Self {
                narrative: InMemoryNarrativeGateway::new(InMemoryKbStore::new()),
                kb: InMemoryKbStore::new(),
                knowledge: InMemoryKnowledgeStore::new(),
            }
        }
    }

    #[tokio::test]
    async fn moment_assembly_stage0_only_when_no_ids() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0());

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        assert!(!ctx.stage0_context.contains("World State"));
        assert!(ctx.world_state.is_none());
        assert!(ctx.timeline.is_none());
        assert!(ctx.world_kb.is_none());
        assert!(ctx.user_knowledge.is_none());
    }

    #[tokio::test]
    async fn moment_assembly_includes_world_state() {
        let stores = TestStores::new();
        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "Test World",
            "test-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        assert!(ctx.world_state.is_some());
        let ws = ctx.world_state.unwrap();
        assert!(ws.contains("Test World"));
        assert!(ws.contains("wld_1"));
    }

    #[tokio::test]
    async fn moment_assembly_includes_timeline() {
        let stores = TestStores::new();
        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "Test World",
            "test-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        let event = nexus_narrative::timeline_event::TimelineEvent::new(
            "wld_1",
            "fbk_root",
            nexus_narrative::timeline_event::TimelineEventType::StoryAdvance,
            1,
        );
        stores.narrative.insert_event(event);

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        assert!(ctx.timeline.is_some());
        let tl = ctx.timeline.unwrap();
        assert!(tl.contains("story_advance"));
    }

    #[tokio::test]
    async fn moment_assembly_world_not_found_gives_none() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_ghost");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        assert!(ctx.world_state.is_none());
        assert!(ctx.timeline.is_none());
    }

    #[tokio::test]
    async fn full_context_assembles_all_sections() {
        let stores = TestStores::new();

        // Set up world
        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "Full World",
            "full-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        // Set up event
        let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
            "wld_1",
            "fbk_root",
            nexus_narrative::timeline_event::TimelineEventType::StoryAdvance,
            1,
        );
        event.title = Some("The Beginning".to_string());
        stores.narrative.insert_event(event);

        // Set up KB
        let kb = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        stores.kb.insert_key_block(kb).await.unwrap();

        // Set up knowledge
        let entry = nexus_knowledge::KnowledgeEntry::new(
            "user_1",
            vec![nexus_knowledge::KnowledgeTag::new("writing")],
            "Show, don't tell.",
        );
        stores.knowledge.store(entry).await.unwrap();

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_user("user_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        let full = ctx.to_full_context();
        assert!(
            full.contains(WORLD_STATE_HEADING),
            "should have world state"
        );
        assert!(full.contains(TIMELINE_HEADING), "should have timeline");
        assert!(full.contains(WORLD_KB_HEADING), "should have world KB");
        assert!(
            full.contains(USER_KNOWLEDGE_HEADING),
            "should have user knowledge"
        );
        assert!(full.contains("Full World"));
        assert!(full.contains("The Beginning"));
        assert!(full.contains("Hero"));
        assert!(full.contains("Show, don't tell."));
    }

    #[tokio::test]
    async fn full_context_omits_empty_sections() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0());

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let full = ctx.to_full_context();

        assert!(!full.contains(WORLD_STATE_HEADING));
        assert!(!full.contains(TIMELINE_HEADING));
        assert!(!full.contains(WORLD_KB_HEADING));
        assert!(!full.contains(USER_KNOWLEDGE_HEADING));
    }

    #[tokio::test]
    async fn moment_context_preserves_stage0_content() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0());

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        assert!(ctx.stage0_context.contains("A creative writer."));
        assert!(ctx.stage0_context.contains("Write chapter 3."));
    }

    /// C2.2: KB query respects `kb_limit` — seeded multi-block, limit 1 yields single line.
    #[tokio::test]
    async fn kb_query_respects_limit() {
        let stores = TestStores::new();

        // Seed two KB blocks
        let kb1 = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        let kb2 = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Scene,
            "Castle",
        );
        stores.kb.insert_key_block(kb1).await.unwrap();
        stores.kb.insert_key_block(kb2).await.unwrap();

        // Without limit: both blocks
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(kb_text.contains("Hero"), "unlimited KB should contain Hero");
        assert!(
            kb_text.contains("Castle"),
            "unlimited KB should contain Castle"
        );

        // With limit 1: only one block
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_kb_limit(1);
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        // One of them, not both
        let has_hero = kb_text.contains("Hero");
        let has_castle = kb_text.contains("Castle");
        assert!(
            has_hero ^ has_castle,
            "kb_limit=1 should return exactly one block, got: {kb_text}"
        );
    }

    /// C2.2: KB query respects `kb_text_search` filter.
    #[tokio::test]
    async fn kb_query_respects_text_search() {
        let stores = TestStores::new();

        let kb1 = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        let kb2 = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Scene,
            "Castle",
        );
        stores.kb.insert_key_block(kb1).await.unwrap();
        stores.kb.insert_key_block(kb2).await.unwrap();

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_kb_text_search("her");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(
            kb_text.contains("Hero"),
            "text_search='her' should match Hero"
        );
        assert!(
            !kb_text.contains("Castle"),
            "text_search='her' should not match Castle"
        );
    }

    /// C2.3: Cross-domain truncation bounds total output.
    #[tokio::test]
    async fn cross_domain_truncation_respects_budget() {
        let stores = TestStores::new();

        // Set up world
        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "A very long world title that should be truncated",
            "test-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        // Set up KB
        let kb = nexus_kb::key_block::KeyBlock::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero with a long description",
        );
        stores.kb.insert_key_block(kb).await.unwrap();

        // Set up knowledge
        let entry = nexus_knowledge::KnowledgeEntry::new(
            "user_1",
            vec![nexus_knowledge::KnowledgeTag::new("writing")],
            "A long knowledge entry that should also be truncated when budget is tight.",
        );
        stores.knowledge.store(entry).await.unwrap();

        // Without truncation: full content
        let request_no_budget = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_user("user_1");
        let ctx_full = assemble_moment(
            &request_no_budget,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
        )
        .await;
        let full_len = ctx_full.to_full_context().chars().count();

        // With small budget
        let request_budget = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_user("user_1")
            .with_max_tokens(50); // 200 chars budget
        let ctx_budget = assemble_moment(
            &request_budget,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
        )
        .await;
        let budget_len = ctx_budget.to_full_context().chars().count();

        assert!(
            budget_len <= full_len,
            "truncated output ({budget_len}) should not exceed full output ({full_len})"
        );

        // Personality should still be present (never truncated)
        assert!(
            ctx_budget.stage0_context.contains("A creative writer."),
            "personality must survive truncation"
        );
    }

    // --- A3.2: Delimiter-based personality split tests ---

    #[test]
    fn split_personality_uses_delimiter_path() {
        // Stage0 assembly now emits delimiters, so split should use them
        let asm = Stage0Assembly {
            personality: "Bold and creative writer.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        };
        let stage0_text = asm.assemble();

        let ctx = MomentContext {
            stage0_context: stage0_text,
            world_state: Some("World state data.".to_string()),
            timeline: None,
            world_kb: None,
            user_knowledge: None,
        };

        let (personality, rest) = ctx.split_stage0_personality();
        assert!(
            personality.contains("Bold and creative writer."),
            "personality section must contain the personality body"
        );
        assert!(
            personality.contains("## Personality"),
            "personality section must contain the heading"
        );
        assert!(
            !rest.contains("Bold and creative writer."),
            "rest must not contain personality body"
        );
        assert!(
            rest.contains("10 years."),
            "rest must contain experience"
        );
    }

    #[test]
    fn split_personality_delimiter_round_trip() {
        // Full round-trip: assemble → to_full_context → split_stage0_personality
        let asm = Stage0Assembly {
            system_prefix: "System prefix.".to_string(),
            personality: "Creative soul.".to_string(),
            experience: "5 years.".to_string(),
            user_prompt: "Do task.".to_string(),
            ..Stage0Assembly::default()
        };
        let stage0_text = asm.assemble();

        let mut ctx = MomentContext {
            stage0_context: stage0_text,
            world_state: Some("Some world state.".to_string()),
            timeline: Some("Timeline events.".to_string()),
            world_kb: None,
            user_knowledge: None,
        };

        // apply_cross_domain_truncation uses split_stage0_personality internally
        ctx.apply_cross_domain_truncation(50);

        // Personality must survive truncation
        assert!(
            ctx.stage0_context.contains("Creative soul."),
            "personality must survive truncation round-trip"
        );
    }

    #[test]
    fn split_personality_r13_scenario_no_false_split() {
        // R13: personality containing "## " sub-headers must not cause premature split.
        // With delimiters, the split is structural, not heuristic.
        let asm = Stage0Assembly {
            personality: "A writer with goals.\n\n## Goals\n- Write daily\n- Be bold".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Continue.".to_string(),
            ..Stage0Assembly::default()
        };
        let stage0_text = asm.assemble();

        let ctx = MomentContext {
            stage0_context: stage0_text,
            ..MomentContext::default()
        };

        let (personality, _rest) = ctx.split_stage0_personality();
        assert!(
            personality.contains("Write daily"),
            "personality with embedded ## sub-headers must not be prematurely truncated"
        );
        assert!(
            personality.contains("Be bold"),
            "full personality content must be captured"
        );
    }

    #[test]
    fn split_personality_legacy_heuristic_fallback() {
        // Content without delimiters should fall back to heuristic
        let legacy_content = "System prefix.\n\n## Personality\n\nA creative soul.\n\n## Experience\n\n10 years.\n";

        let ctx = MomentContext {
            stage0_context: legacy_content.to_string(),
            ..MomentContext::default()
        };

        let (personality, rest) = ctx.split_stage0_personality();
        assert!(
            personality.contains("A creative soul."),
            "legacy heuristic must extract personality"
        );
        assert!(
            !personality.contains("10 years"),
            "legacy heuristic must not include experience"
        );
        assert!(
            rest.contains("10 years"),
            "rest must contain experience"
        );
    }
}
