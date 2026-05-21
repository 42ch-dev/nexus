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

use crate::stage0::Stage0Assembly;
use nexus_kb::KbStore;
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
        match fetch_world_kb(kb_store, world_id).await {
            Ok(Some(kb_text)) => Some(kb_text),
            _ => None,
        }
    } else {
        None
    };

    // 4. User knowledge (if user_id provided)
    let user_knowledge = if let Some(ref user_id) = request.user_id {
        match fetch_user_knowledge(knowledge, user_id).await {
            Ok(Some(uk_text)) => Some(uk_text),
            _ => None,
        }
    } else {
        None
    };

    MomentContext {
        stage0_context,
        world_state,
        timeline,
        world_kb,
        user_knowledge,
    }
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

/// Fetch World KB assets and format as context text.
#[allow(clippy::future_not_send)]
async fn fetch_world_kb<K: KbStore>(
    kb_store: &K,
    world_id: &str,
) -> Result<Option<String>, nexus_kb::KbStoreError> {
    let blocks = kb_store.list_by_world(world_id).await?;
    if blocks.is_empty() {
        return Ok(None);
    }
    let lines: Vec<String> = blocks
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
) -> Result<Option<String>, nexus_knowledge::KnowledgeError> {
    let query = nexus_knowledge::KnowledgeQuery::for_user(user_id).with_limit(20);
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
}
