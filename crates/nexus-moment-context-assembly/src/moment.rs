//! Moment-scoped context assembly — aggregates from all local domains.
//!
//! The [`MomentAssembly`] pulls context from four domain sources:
//!
//! 1. **Creator Memory** (`nexus-creator-memory`): SOUL sections, long-term memories,
//!    fragment keywords (via [`Stage0Assembly`]).
//! 2. **Narrative** (`nexus-narrative`): world state, timeline position, event snapshot
//!    (via [`NarrativeGateway`](nexus_narrative::NarrativeGateway)).
//! 3. **Knowledge Base** (`nexus-kb`): World-scoped KB assets / key blocks
//!    (via [`KbStore`](nexus_knowledge::world_kb::KbStore)).
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

use crate::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, MomentDirectiveStatus, NoDirectiveStore,
};
use crate::generation::GenerationStage;
use crate::slots::{self, SlotMapEntry};
use crate::stage0::{Stage0Assembly, STAGE0_PERSONALITY_END, STAGE0_PERSONALITY_START};
use crate::world_context::WorldKbQueryBuilder;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::KbStore;
use nexus_knowledge::KnowledgeStore;
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::adapter::activation::{
    apply_activation_with_hops, ActivationBudget, HopConfig, HopEdge,
};

/// Section heading for World State in assembled context.
const WORLD_STATE_HEADING: &str = "## World State";

/// Section heading for Timeline in assembled context.
const TIMELINE_HEADING: &str = "## Timeline";

/// Section heading for World Knowledge Base in assembled context.
const WORLD_KB_HEADING: &str = "## World Knowledge Base";

/// Section heading for User Knowledge in assembled context.
const USER_KNOWLEDGE_HEADING: &str = "## User Knowledge";

/// Section heading for the reserved Moment Directive slot (V1.150 P0 —
/// spec §2 / Q1 provisional lock). Placed between `## Timeline` and
/// `## World Knowledge Base`. P0 reserves the position but never renders it
/// (no directive active); P1 fills the slot.
const MOMENT_DIRECTIVE_HEADING: &str = "## Moment Directive";

/// Parameters for a single moment context assembly request.
///
/// All IDs are strings for now (matching current domain APIs).
/// Fields left as `None` indicate that domain source should be skipped.
#[derive(Debug, Clone)]
pub struct MomentRequest {
    /// World ID to pull narrative state and KB assets for.
    pub world_id: Option<String>,
    /// Work ID for the work-bound moment (V1.150 P1): scope resolution of the
    /// Moment Directive (spec §3.2) and the chapter-advance TTL signal
    /// (spec §3.3) are keyed on it. Optional — the observation path may
    /// assemble a raw world without a Work.
    pub work_id: Option<String>,
    /// Creator ID owning the moment (V1.150 P1): directives are
    /// creator-scoped; `None` means no directive can be in scope.
    pub creator_id: Option<String>,
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
    /// Enable lore activation filtering on `WorldKB` entries (V1.146 P4 T2,
    /// default-on since V1.149 P0 T2).
    ///
    /// When `true`, the activation pass runs between `WorldKB` fetch and
    /// User Knowledge assembly, calling `apply_activation` to filter/inspect
    /// entries by their `modules.activation` fire-conditions.
    /// Default `true` — activation is the shipped product behavior; `false`
    /// restores V1.146 flag-off semantics (all entries returned unchanged).
    pub activation_enabled: bool,
    /// Preloaded confirmed relation edges of the world for relation-hop
    /// expansion (V1.149 P1, spec §5). `None` ⇒ activation-only pass
    /// (P0 behavior); `Some(..)` ⇒ `apply_activation_with_hops` — primary-
    /// fired / `constant` entries BFS-expand up to 2 graph hops within the
    /// hop token budget, and graph-adjacent entries join `matched` without
    /// re-firing keyword activation.
    ///
    /// Edges are loaded by the CLI/wire layer via
    /// `NexusAdapter::list_hop_edges_for_world` (the `RelationPort` gap:
    /// spoke's port is get/put only); MCA itself never walks relations.
    pub hop_edges: Option<Vec<HopEdge>>,
    /// Caller-provided cap on the hop token budget (chars/4, spec Q1).
    /// When `max_tokens` is set, the effective budget is the cross-domain
    /// remainder after personality (never truncated) + `world_state` +
    /// `timeline` reservations, bounded by this cap; when `max_tokens` is
    /// absent, only this cap applies (`None` ⇒ depth + cycle only).
    /// See [`hop_budget_tokens`].
    pub hop_max_tokens: Option<usize>,
    /// Generation type for spec §4 slot gating (V1.150 P2, DF-75 — guide
    /// `mca-section-audit.md` Q4 lock).
    ///
    /// `None` (the default) is treated as [`GenerationStage::Unspecified`] —
    /// every slot fills, which is current behavior and the neutral golden /
    /// direct-CLI / inspector path (AC-I1b). `run_intent` is **derivable**
    /// from the stage (creator-workflow §3.1) — there is deliberately no
    /// separate field. Wired from the CLI `assemble-moment --stage` flag;
    /// the preset runner / schedule path threads the executing stage when it
    /// drives assembly (see `guides/generation-trigger-wiring.md`).
    pub generation_stage: Option<GenerationStage>,
}

impl MomentRequest {
    /// Create a minimal request with only Stage-0 inputs.
    #[must_use]
    pub const fn new(stage0: Stage0Assembly) -> Self {
        Self {
            world_id: None,
            work_id: None,
            creator_id: None,
            branch_id: None,
            event_id: None,
            user_id: None,
            stage0,
            max_tokens: None,
            kb_limit: None,
            kb_text_search: None,
            kb_block_type: None,
            knowledge_limit: None,
            activation_enabled: true,
            hop_edges: None,
            hop_max_tokens: None,
            generation_stage: None,
        }
    }

    /// Set the world context (world ID, optional branch, optional event).
    #[must_use]
    pub fn with_world(mut self, world_id: impl Into<String>) -> Self {
        self.world_id = Some(world_id.into());
        self
    }

    /// Set the work ID for the work-bound moment (V1.150 P1 — Moment
    /// Directive scope resolution + chapter-advance TTL).
    #[must_use]
    pub fn with_work(mut self, work_id: impl Into<String>) -> Self {
        self.work_id = Some(work_id.into());
        self
    }

    /// Set the owning creator ID (V1.150 P1 — Moment Directives are
    /// creator-scoped; `None` ⇒ no directive can be in scope).
    #[must_use]
    pub fn with_creator(mut self, creator_id: impl Into<String>) -> Self {
        self.creator_id = Some(creator_id.into());
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

    /// Enable lore activation filtering (V1.146 P4 T2).
    #[must_use]
    pub const fn with_activation_enabled(mut self, enabled: bool) -> Self {
        self.activation_enabled = enabled;
        self
    }

    /// Provide preloaded relation-hop edges (V1.149 P1, spec §5). `None`
    /// (the default) keeps the activation-only P0 behavior.
    #[must_use]
    pub fn with_hop_edges(mut self, edges: Vec<HopEdge>) -> Self {
        self.hop_edges = Some(edges);
        self
    }

    /// Cap the hop token budget (chars/4, spec Q1). See [`hop_budget_tokens`]
    /// for how the cap combines with `max_tokens`.
    #[must_use]
    pub const fn with_hop_max_tokens(mut self, cap: usize) -> Self {
        self.hop_max_tokens = Some(cap);
        self
    }

    /// Set the generation stage for spec §4 slot gating (V1.150 P2).
    ///
    /// `None` (the default — don't call this) keeps every slot on
    /// ([`GenerationStage::Unspecified`]).
    #[must_use]
    pub const fn with_generation_stage(mut self, stage: GenerationStage) -> Self {
        self.generation_stage = Some(stage);
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
    /// Moment Directive slot (V1.150 P0 reserved — V1.150 P1 fills it): the
    /// `## Moment Directive` top-level section within the directive region
    /// (above lore, below system/personality). `None` means no directive is
    /// active and the section is never rendered — the neutral-only
    /// byte-equivalence anchor (AC-I1b).
    pub moment_directive: Option<String>,
    /// Placement of the directive section within the directive region
    /// (V1.150 P1). Defaults to [`DirectiveDepth::Tail`] — P0's reserved
    /// position between `## Timeline` and `## World Knowledge Base` — so
    /// contexts that set only the body keep the P0 layout.
    pub moment_directive_depth: DirectiveDepth,
    /// Per-entry activation trace (populated when `activation_enabled` is true).
    /// V1.146 P4 T3: exposed for inspector packet emission.
    pub activation_trace:
        Option<Vec<nexus_spoke_adapter::adapter::activation::ActivationTraceEntry>>,
    /// Slot map (V1.151 P0, DF-76 spec §2 H2): every accepted entry that
    /// survived the generation-stage gate mapped to its slot id — captured
    /// **post stage-gate** so the map reflects what actually rendered.
    /// `None` when no slot routing ran (no World-KB, activation off, or all
    /// entries gated off). A synthetic
    /// `{ entry_id: <directive_id>, slot: "moment.directive" }` entry is
    /// appended when a directive injected this assembly.
    pub slot_map: Option<Vec<SlotMapEntry>>,
    /// Activation token-budget accounting (spec §2 H3): chars/4 estimates
    /// for primary matches vs. relation hops + cap/remaining. `Some` whenever
    /// activation ran; `None` when activation is disabled. Additive — never
    /// part of `to_full_context()` (AC-I6).
    pub activation_budget: Option<ActivationBudget>,
    /// Status-only Moment Directive metadata for the inspector packet (spec
    /// §2 H6) — **NEVER the directive body** (AC-I3; body exclusion is by
    /// construction — the packet builder reads only this field). `None` when
    /// no directive is active. Additive — never part of `to_full_context()`.
    pub moment_directive_meta: Option<MomentDirectiveStatus>,
    /// Hygiene trace (DF-79): per-entry rows for carrier-bearing entries —
    /// `{entry_id, applied, skipped, notes}`. `None` when no hygiene pass
    /// ran (no World-KB, activation off, or all entries gated off).
    /// Additive — never part of `to_full_context()` (AC-I6).
    pub hygiene_trace: Option<Vec<crate::hygiene::HygieneTraceEntry>>,
}

impl MomentContext {
    /// Assemble the full context string from all sources.
    ///
    /// Follows the spec ordering (§9.2) with domain extensions:
    /// 1. Stage-0 context (system prefix, personality, memories, keywords, experience, prompt)
    /// 2. World state (narrative)
    /// 3. Timeline (narrative)
    /// 4. Moment Directive (V1.150 P0 reserved — only rendered when P1
    ///    fills the slot)
    /// 5. World KB (key blocks — V1.150 P0 slots subdivide this section)
    /// 6. User knowledge
    ///
    /// Empty sections are omitted.
    ///
    /// The Moment Directive section is positioned by `moment_directive_depth`
    /// **within the directive region** (between the Stage-0 block above and
    /// the World Knowledge Base below, spec §1.2 / §3.3): `head` directly
    /// below Stage-0, `mid` between World State and Timeline, `tail` between
    /// Timeline and World KB — P0's reserved position (the default). The
    /// directive can never move below lore or above system.
    #[must_use]
    pub fn to_full_context(&self) -> String {
        let section = |text: Option<&String>, heading: &str| {
            text.filter(|s| !s.is_empty())
                .map(|s| format!("{heading}\n\n{s}\n"))
        };
        let stage0 = (!self.stage0_context.is_empty()).then(|| self.stage0_context.clone());
        let world_state = section(self.world_state.as_ref(), WORLD_STATE_HEADING);
        let timeline = section(self.timeline.as_ref(), TIMELINE_HEADING);
        let directive = section(self.moment_directive.as_ref(), MOMENT_DIRECTIVE_HEADING);
        let world_kb = section(self.world_kb.as_ref(), WORLD_KB_HEADING);
        let user_knowledge = section(self.user_knowledge.as_ref(), USER_KNOWLEDGE_HEADING);

        let mut parts: Vec<Option<String>> = vec![stage0];
        match self.moment_directive_depth {
            DirectiveDepth::Head => {
                parts.push(directive);
                parts.push(world_state);
                parts.push(timeline);
                parts.push(world_kb);
                parts.push(user_knowledge);
            }
            DirectiveDepth::Mid => {
                parts.push(world_state);
                parts.push(directive);
                parts.push(timeline);
                parts.push(world_kb);
                parts.push(user_knowledge);
            }
            DirectiveDepth::Tail => {
                parts.push(world_state);
                parts.push(timeline);
                parts.push(directive);
                parts.push(world_kb);
                parts.push(user_knowledge);
            }
        }
        parts.into_iter().flatten().collect::<Vec<_>>().join("\n")
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
        split_stage0_personality(&self.stage0_context)
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

/// Extract the personality section from a Stage-0 context string — the shared
/// helper behind [`MomentContext::split_stage0_personality`] (delimiter
/// protocol first, legacy `## Personality` heuristic fallback). The relation-
/// hop budget reuses it so the personality reservation matches exactly what
/// cross-domain truncation protects (personality is **never** truncated).
fn split_stage0_personality(ctx: &str) -> (String, String) {
    // Primary path: structured delimiters
    if let (Some(start_pos), Some(end_pos)) = (
        ctx.find(STAGE0_PERSONALITY_START),
        ctx.find(STAGE0_PERSONALITY_END),
    ) {
        let content_start = start_pos + STAGE0_PERSONALITY_START.len();
        if end_pos > content_start {
            let personality_section = ctx[content_start..end_pos].to_string();
            let rest = format!(
                "{}{}",
                &ctx[..start_pos],
                &ctx[end_pos + STAGE0_PERSONALITY_END.len()..]
            );
            return (personality_section, rest);
        }
    }

    // Legacy fallback: markdown-header heuristic
    ctx.find("## Personality").map_or_else(
        || (String::new(), ctx.to_string()),
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

/// Hop token budget per iteration spec Q1 (architect lock).
///
/// Formula: when `max_tokens` is set, the hop budget is the cross-domain
/// remainder **after** reserving personality (never truncated) +
/// `world_state` + `timeline`, all estimated at chars/4:
///
/// `hop_budget = (max_tokens*4 − personality_chars − world_state_chars −
/// timeline_chars) / 4`, bounded by the caller-provided `hop_max_tokens` cap.
///
/// The engine then further subtracts the primary-matched KB estimate
/// (chars/4 of summary-or-name) before the hop pass, so the effective cap
/// honors "hop remainder after primary KB + `world_state` + `timeline`
/// estimates". When `max_tokens` is absent, only the caller cap applies
/// (`None` ⇒ depth + cycle only).
fn hop_budget_tokens(
    request: &MomentRequest,
    stage0_context: &str,
    world_state: Option<&str>,
    timeline: Option<&str>,
) -> Option<usize> {
    let Some(max_tokens) = request.max_tokens else {
        return request.hop_max_tokens;
    };
    let max_chars = max_tokens.saturating_mul(4);
    let (personality, _) = split_stage0_personality(stage0_context);
    let reserved = personality.chars().count()
        + world_state.map_or(0, |text| text.chars().count())
        + timeline.map_or(0, |text| text.chars().count());
    let remainder = max_chars.saturating_sub(reserved) / 4;
    Some(
        request
            .hop_max_tokens
            .map_or(remainder, |cap| remainder.min(cap)),
    )
}

/// Assemble moment context from all domain sources.
///
/// This is the primary entry point for full moment context assembly.
/// It queries each domain source in sequence and combines the results.
///
/// Uses a [`NoDirectiveStore`] — the Moment Directive slot can never fill on
/// this path (AC-I1b neutral-only byte-equivalence promise). Callers that
/// need the directive inject through
/// [`assemble_moment_with_directive`].
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
    assemble_moment_with_directive(request, narrative, kb_store, knowledge, &NoDirectiveStore).await
}

/// Assemble moment context with a Moment Directive store (V1.150 P1, DF-75).
///
/// Same flow as [`assemble_moment`], plus the directive step (spec §3):
/// resolve + load the active directive for the work-bound moment, render it
/// into the reserved `moment.directive` slot at its `insert_depth`, and run
/// the post-injection lifecycle (TTL decrement / chapter-advance /
/// scene-clear bookkeeping) after a successful injecting assembly.
///
/// When no directive is active — or the store resolves none — the slot stays
/// empty and the output is byte-identical to [`assemble_moment`] (AC-I1b).
/// The directive section is never truncated (author instruction; like
/// personality, it survives cross-domain truncation).
///
/// # Caller wiring (R-V1150P2-011 accepted — future-wiring note)
///
/// Today only the `platform context assemble-moment` CLI path calls this
/// entry point, threading `creator_id` / `work_id` / `world_id` and the
/// generation stage through [`MomentRequest`]. A future daemon/ACP caller
/// MUST thread the same fields (`creator_id` / `work_id` / `event_id` +
/// `generation_stage`) when it wires MCA assembly — tracked in DF-76 / the
/// daemon-route iteration, not an open defect in shipped code.
///
/// # Type parameters
///
/// - `G`, `K`, `S`: as in [`assemble_moment`].
/// - `D`: A [`DirectiveStore`] implementation (composition root adapter over
///   `nexus-local-db`; in-memory stub in tests).
#[allow(clippy::future_not_send)]
// Four-domain assembly orchestrator (stage-0 → world_state → timeline →
// world-kb → user-knowledge → directive) — the per-section blocks keep the
// function at ~114 lines; splitting would scatter one assembly's steps.
#[allow(clippy::too_many_lines)]
pub async fn assemble_moment_with_directive<G, K, S, D>(
    request: &MomentRequest,
    narrative: &G,
    kb_store: &K,
    knowledge: &S,
    directives: &D,
) -> MomentContext
where
    G: NarrativeGateway,
    K: KbStore,
    S: KnowledgeStore,
    D: DirectiveStore,
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

    // V1.149 P0 T2: extended activation scan — Stage-0 full text + outline
    // beats (timeline title/summary). The timeline was already fetched above;
    // reuse it here BEFORE it is stored on the context. Manuscript body is not
    // on the MCA path (documented gap, spec §3) — Stage-0 fallback only.
    let activation_scan_text = if request.activation_enabled {
        timeline.as_ref().map_or_else(
            || stage0_context.clone(),
            |tl| format!("{stage0_context}\n{tl}"),
        )
    } else {
        String::new()
    };

    // 3. World KB (if world_id provided)
    // V1.146 P4 T3: capture activation trace for inspector packet emission.
    let mut activation_trace: Option<
        Vec<nexus_spoke_adapter::adapter::activation::ActivationTraceEntry>,
    > = None;
    // V1.151 P0 (DF-76 spec §2 H3): capture the activation token-budget
    // accounting (primary/hop estimates + cap/remaining) alongside the trace.
    let mut activation_budget: Option<ActivationBudget> = None;
    // V1.151 P0 (spec §2 H2): capture the post stage-gate slot map (which
    // accepted entry landed in which slot) for the inspector packet.
    let mut slot_map: Option<Vec<SlotMapEntry>> = None;
    // DF-79: capture the per-entry hygiene trace (applied/skipped/notes)
    // for the inspector packet — `Some` whenever the hygiene pass ran.
    let mut hygiene_trace: Option<Vec<crate::hygiene::HygieneTraceEntry>> = None;
    let world_kb = if let Some(ref world_id) = request.world_id {
        match fetch_world_kb_entries(kb_store, world_id, request).await {
            Ok(entries) if !entries.is_empty() => {
                let entries = if request.activation_enabled {
                    // V1.149 P0 T2: default-on lore activation between WorldKB
                    // fetch and User Knowledge assembly. Scan text = Stage-0 +
                    // timeline outline beats (reused from step 2). Unmatched
                    // entries are filtered out (activation gate). Neutral
                    // entries (no activation module) remain in matched.
                    // V1.146 P4 T3: capture the full ActivationResult for
                    // diagnostic trace emission.
                    // V1.149 P1: when preloaded relation-hop edges are present,
                    // the engine also BFS-expands up to 2 graph hops from
                    // primary-fired/constant entries within the hop token
                    // budget (spec Q1 — `hop_budget_tokens`); hop-pulled
                    // entries join matched without re-firing keys.
                    let activation_result = request.hop_edges.as_deref().map_or_else(
                        || {
                            nexus_spoke_adapter::adapter::activation::apply_activation(
                                &entries,
                                &activation_scan_text,
                                &[],
                            )
                        },
                        |edges| {
                            let hop_config = HopConfig {
                                max_hops: 2, // architect lock Q1
                                max_hop_tokens: hop_budget_tokens(
                                    request,
                                    &stage0_context,
                                    world_state.as_deref(),
                                    timeline.as_deref(),
                                ),
                            };
                            apply_activation_with_hops(
                                &entries,
                                &activation_scan_text,
                                &[],
                                edges,
                                &hop_config,
                            )
                        },
                    );
                    activation_trace = Some(activation_result.trace);
                    activation_budget = activation_result.budget;
                    activation_result.matched
                } else {
                    entries
                };
                if entries.is_empty() {
                    None
                } else if request.activation_enabled {
                    // V1.150 P0/P2 (DF-75): shape the activation-matched
                    // entries into the World-KB body — spec §4 generation-
                    // stage gate first (`slots::apply_stage_gate`), then
                    // slot routing + render (spec §2 / Q5). Neutral-only
                    // Worlds render byte-identically to the V1.149 flat
                    // block (AC-I1b); the gate runs only on the activation-
                    // on path (the off-switch below keeps every entry
                    // unchanged); `None` stage ⇒ all slots on.
                    // V1.151 P0 (spec §2 H2): capture the slot map post
                    // stage-gate — it reflects what actually rendered.
                    // DF-79: the hygiene pass runs inside `render_gated_slots`
                    // (between stage gate and slot routing); capture its trace.
                    let (rendered, map, hygiene) =
                        render_gated_slots(entries, request.generation_stage);
                    slot_map = Some(map);
                    hygiene_trace = Some(hygiene);
                    rendered
                } else {
                    // Off-switch (V1.149 escape hatch, lock #1 — "off ⇒ every
                    // candidate entry unchanged", V1.146 flag-off semantics):
                    // slot routing is an activation-product shaping step and
                    // must not run when activation is disabled. Every entry is
                    // emitted UNCHANGED as the V1.149 flat block (no
                    // `### World (Before)` / `### Outlet:` /
                    // `### Style (Post-History)` sub-headings).
                    Some(slots::format_entries(&entries))
                }
            }
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

    // 5. Moment Directive (V1.150 P1, spec §3): resolve + load the active
    //    directive for the work-bound moment (scope resolution spec §3.2 —
    //    Work wins, else World override for the Work's bound World, else
    //    none), then run the post-injection lifecycle. The plain
    //    `assemble_moment` path passes a `NoDirectiveStore`, which always
    //    resolves `None` — nothing renders, nothing decrements (AC-I1b).
    let directive = apply_directive(directives, request).await;

    // V1.151 P0 (spec §2 H2): the directive occupies the reserved
    // `moment.directive` slot — a synthetic slot-map entry appears only
    // when a directive injected this assembly (`moment.directive` is a
    // top-level section, never a World-KB routing slot).
    if let Some(d) = &directive {
        slot_map.get_or_insert_with(Vec::new).push(SlotMapEntry {
            entry_id: d.directive_id.clone(),
            slot: "moment.directive".to_string(),
        });
    }

    let mut ctx = MomentContext {
        stage0_context,
        world_state,
        timeline,
        world_kb,
        user_knowledge,
        // V1.150 P0 reserved the slot; P1 fills it with the active directive
        // body. `None` ⇒ no `## Moment Directive` section (neutral-only).
        moment_directive: directive.as_ref().map(|d| d.body.clone()),
        moment_directive_depth: directive
            .as_ref()
            .map_or_else(DirectiveDepth::default, |d| d.insert_depth),
        activation_trace,
        // V1.151 P0 (spec §2 H3/H6): additive inspector surface — budget
        // accounting + status-only directive metadata (never the body).
        slot_map,
        activation_budget,
        moment_directive_meta: directive.as_ref().map(MomentDirectiveStatus::from),
        hygiene_trace,
    };

    // 6. Cross-domain truncation if max_tokens set (the directive section is
    //    never truncated — it is author instruction, like personality).
    if let Some(max_tokens) = request.max_tokens {
        ctx.apply_cross_domain_truncation(max_tokens);
    }

    ctx
}

/// Resolve the active Moment Directive for a request and — when one was
/// injected — run the post-injection lifecycle (spec §3.3: TTL decrement /
/// chapter-advance / scene-clear bookkeeping, best-effort; the store never
/// fails the assembly).
///
/// # Threat model — single-author, local-only (PR #198 P1)
///
/// The load-then-decrement pair is deliberately **non-atomic**: two
/// overlapping same-scope assembles can both load a directive with one use
/// remaining and both render it before either decrements. That is not a
/// realistic local threat — Nexus is single-author, single-active-creator,
/// local-only, with no parallel creators and no background jobs (the same
/// synchronous/proportional discipline V1.80 established for
/// `POST /v1/local/memory/review`). Locking the directive lifecycle would
/// be over-engineering for this threat model, so none is added by design.
#[allow(clippy::future_not_send)]
async fn apply_directive<D: DirectiveStore>(
    directives: &D,
    request: &MomentRequest,
) -> Option<ActiveDirective> {
    let directive = directives
        .load_active(
            request.creator_id.as_deref(),
            request.work_id.as_deref(),
            request.world_id.as_deref(),
        )
        .await;
    if let Some(d) = directive.as_ref() {
        directives
            .after_injection(
                &d.directive_id,
                request.event_id.as_deref(),
                request.work_id.as_deref(),
            )
            .await;
    }
    directive
}

/// V1.150 P2 (DF-75, spec §4 / Q5) — shape the activation-matched World-KB
/// entries into the section body: apply the generation-stage gate
/// ([`slots::apply_stage_gate`]), route the eligible entries into named,
/// ordered slots, and render. `None` (all entries gated off, e.g.
/// `system_maintenance`) yields `None` — the caller omits the whole
/// World-KB section.
///
/// Returns the rendered body plus the post-gate slot map (V1.151 P0, spec
/// §2 H2): which entry landed in which slot **after** the gate ran — the
/// map reflects what actually rendered, not what activation matched — plus
/// the DF-79 hygiene trace (per-entry applied/skipped/notes).
fn render_gated_slots(
    entries: Vec<KnowledgeEntryRecord>,
    stage: Option<GenerationStage>,
) -> (
    Option<String>,
    Vec<SlotMapEntry>,
    Vec<crate::hygiene::HygieneTraceEntry>,
) {
    // DF-79: the hygiene pass runs between the generation-stage gate and
    // slot routing — transforms shape the emitted `body.summary` text on
    // the owned assembly-local copies (read-path only).
    let gated = slots::apply_stage_gate(entries, stage);
    let (hygiened, trace) = crate::hygiene::apply_hygiene(gated);
    let routing = slots::route_slots(hygiened);
    let map = routing.to_slot_map();
    (slots::render_slots(&routing), map, trace)
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

    let timeline_text = match gateway.get_timeline(world_id, branch_id, None).await {
        Ok(events) if !events.is_empty() => Some(format_timeline(&events)),
        _ => None,
    };

    Ok((world_state_text, timeline_text))
}

/// Fetch World KB entries using structured query (no formatting).
///
/// V1.146 P4 T2: extracted so the activation pass can operate on entries
/// before formatting. Callers use `format_entries` (in `slots.rs`) to produce
/// context text.
#[allow(clippy::future_not_send)]
async fn fetch_world_kb_entries<K: KbStore>(
    kb_store: &K,
    world_id: &str,
    request: &MomentRequest,
) -> Result<Vec<KnowledgeEntryRecord>, nexus_knowledge::world_kb::KbStoreError> {
    let builder = WorldKbQueryBuilder::new(world_id);
    let mut query = builder.query_all();
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
    Ok(result.items)
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
    use crate::directive::DirectiveTtlKind;
    use nexus_knowledge::world_kb::InMemoryKbStore;
    use nexus_knowledge::InMemoryKnowledgeStore;
    use nexus_narrative::InMemoryNarrativeGateway;
    use std::sync::Arc;

    /// Helper: create a `Stage0Assembly` with minimal content.
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
        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        // Set up knowledge
        let entry = nexus_knowledge::UserKnowledgeEntry::new(
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

    // ── V1.150 P0: reserved Moment Directive slot (spec §2 / Q1) ──────

    #[tokio::test]
    async fn assemble_moment_reserves_directive_slot_empty() {
        // P0 reserves the `moment.directive` slot but never fills it: a full
        // assembly leaves `moment_directive` None and renders no
        // `## Moment Directive` section (AC-I1b neutral-only promise).
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_user("user_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        assert!(
            ctx.moment_directive.is_none(),
            "P0 must leave the reserved directive slot empty"
        );
        let full = ctx.to_full_context();
        assert!(
            !full.contains(MOMENT_DIRECTIVE_HEADING),
            "P0 must not render the Moment Directive section"
        );
    }

    #[test]
    fn to_full_context_renders_directive_between_timeline_and_world_kb() {
        // The reserved slot's emit position: between `## Timeline` and
        // `## World Knowledge Base` (above lore, below system) — the stable
        // insertion point P1 fills.
        let ctx = MomentContext {
            stage0_context: "stage0".to_string(),
            world_state: Some("ws".to_string()),
            timeline: Some("tl".to_string()),
            moment_directive: Some("Keep the prose terse.".to_string()),
            world_kb: Some("kb".to_string()),
            ..MomentContext::default()
        };
        let full = ctx.to_full_context();
        let timeline_pos = full.find(TIMELINE_HEADING).expect("timeline heading");
        let directive_pos = full
            .find(MOMENT_DIRECTIVE_HEADING)
            .expect("directive heading");
        let world_kb_pos = full.find(WORLD_KB_HEADING).expect("world KB heading");
        assert!(
            timeline_pos < directive_pos && directive_pos < world_kb_pos,
            "directive must sit between Timeline and World Knowledge Base"
        );
        assert!(
            full.contains("Keep the prose terse."),
            "directive body present"
        );
    }

    #[test]
    fn to_full_context_omits_empty_directive() {
        // An empty directive body renders nothing (guards P1 drift).
        let ctx = MomentContext {
            stage0_context: "stage0".to_string(),
            moment_directive: Some(String::new()),
            ..MomentContext::default()
        };
        assert!(!ctx.to_full_context().contains(MOMENT_DIRECTIVE_HEADING));
    }

    // ── V1.150 P1: Moment Directive injection (spec §3) ──────────────

    /// In-memory `DirectiveStore` stub: serves a fixed directive and records
    /// `after_injection` calls (`directive_id`, `event_id`, `work_id`).
    #[derive(Default)]
    struct TestDirectiveStore {
        active: Option<ActiveDirective>,
        // Test stub records (directive_id, event_id, work_id) triples.
        #[allow(clippy::type_complexity)]
        calls: Arc<std::sync::Mutex<Vec<(String, Option<String>, Option<String>)>>>,
    }

    impl TestDirectiveStore {
        fn with_directive(active: ActiveDirective) -> Self {
            Self {
                active: Some(active),
                ..Self::default()
            }
        }

        fn after_injection_calls(&self) -> Vec<(String, Option<String>, Option<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    // `unused_async_trait_impl` (clippy 1.98): test stub, no async I/O — trait contract.
    #[allow(clippy::unused_async_trait_impl)]
    impl DirectiveStore for TestDirectiveStore {
        async fn load_active(
            &self,
            _creator_id: Option<&str>,
            _work_id: Option<&str>,
            _world_id: Option<&str>,
        ) -> Option<ActiveDirective> {
            self.active.clone()
        }

        async fn after_injection(
            &self,
            directive_id: &str,
            event_id: Option<&str>,
            work_id: Option<&str>,
        ) {
            self.calls.lock().unwrap().push((
                directive_id.to_string(),
                event_id.map(ToOwned::to_owned),
                work_id.map(ToOwned::to_owned),
            ));
        }
    }

    /// Helper: a directive with the given depth and a distinctive body.
    fn active_directive(depth: DirectiveDepth) -> ActiveDirective {
        ActiveDirective {
            directive_id: "dir_1".to_string(),
            body: "Keep the prose terse.".to_string(),
            insert_depth: depth,
            ttl_kind: DirectiveTtlKind::Generations,
            clear_on_scene_change: false,
            ttl_remaining: Some(3),
            status: "active".to_string(),
            scope_kind: "work".to_string(),
            scope_id: "wrk_1".to_string(),
        }
    }

    /// Seed a world + a timeline event so `world_state` and `timeline` are
    /// both present (the directive region has three interior positions).
    // Kept async for symmetric call shape with the other seed helpers; the
    // body is synchronous.
    #[allow(clippy::unused_async)]
    async fn seed_world_and_timeline(stores: &TestStores) {
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
    }

    #[tokio::test]
    async fn directive_injects_into_reserved_slot_with_lifecycle() {
        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();
        let store = TestDirectiveStore::with_directive(active_directive(DirectiveDepth::Tail));

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_creator("ctr_1")
            .with_work("wrk_1")
            .with_event("evt_1");
        let ctx = assemble_moment_with_directive(
            &request,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
            &store,
        )
        .await;

        assert_eq!(
            ctx.moment_directive.as_deref(),
            Some("Keep the prose terse."),
            "the active directive body fills the reserved slot"
        );
        assert_eq!(ctx.moment_directive_depth, DirectiveDepth::Tail);

        let full = ctx.to_full_context();
        let timeline_pos = full.find(TIMELINE_HEADING).expect("timeline heading");
        let directive_pos = full
            .find(MOMENT_DIRECTIVE_HEADING)
            .expect("directive heading");
        let world_kb_pos = full.find(WORLD_KB_HEADING).expect("world KB heading");
        assert!(
            timeline_pos < directive_pos && directive_pos < world_kb_pos,
            "tail directive sits between Timeline and World Knowledge Base"
        );
        assert!(
            ctx.stage0_context.contains("A creative writer."),
            "personality is never replaced or truncated"
        );

        // Post-injection lifecycle ran exactly once with the request anchors.
        assert_eq!(
            store.after_injection_calls(),
            vec![(
                "dir_1".to_string(),
                Some("evt_1".to_string()),
                Some("wrk_1".to_string())
            )],
            "after_injection must run once per injecting assemble"
        );
    }

    #[tokio::test]
    async fn directive_depth_positions_section_within_region() {
        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        for (depth, expected) in [
            // head: directly below Stage-0 (nearest system) — before World State
            (DirectiveDepth::Head, ("stage0", "directive", "world state")),
            // mid: between World State and Timeline
            (
                DirectiveDepth::Mid,
                ("world state", "directive", "timeline"),
            ),
            // tail: between Timeline and World KB (nearest lore — P0's reserved position)
            (DirectiveDepth::Tail, ("timeline", "directive", "world KB")),
        ] {
            let store = TestDirectiveStore::with_directive(active_directive(depth));
            let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
            let ctx = assemble_moment_with_directive(
                &request,
                &stores.narrative,
                &stores.kb,
                &stores.knowledge,
                &store,
            )
            .await;
            let full = ctx.to_full_context();

            let marker = |label: &str| -> usize {
                match label {
                    "stage0" => full.find("Write chapter 3.").expect("stage0 prompt"),
                    "world state" => full.find(WORLD_STATE_HEADING).expect("world state heading"),
                    "timeline" => full.find(TIMELINE_HEADING).expect("timeline heading"),
                    "directive" => full
                        .find(MOMENT_DIRECTIVE_HEADING)
                        .expect("directive heading"),
                    "world KB" => full.find(WORLD_KB_HEADING).expect("world KB heading"),
                    other => panic!("unknown marker {other:?}"),
                }
            };
            let (a, b, c) = expected;
            assert!(
                marker(a) < marker(b) && marker(b) < marker(c),
                "depth {depth:?}: expected order {a} < {b} < {c}, got:\n{full}"
            );
        }
    }

    #[tokio::test]
    async fn no_active_directive_is_byte_equivalent_to_plain_assembly() {
        // AC-I1b with a store present but empty: a `DirectiveStore` that
        // resolves nothing must not change the assembled bytes at all —
        // no `## Moment Directive` wrapper, no lifecycle call.
        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        let store = TestDirectiveStore::default();

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_creator("ctr_1")
            .with_work("wrk_1")
            .with_event("evt_1");
        let plain =
            assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let with_store = assemble_moment_with_directive(
            &request,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
            &store,
        )
        .await;

        assert_eq!(
            plain.to_full_context(),
            with_store.to_full_context(),
            "empty directive store must be byte-equivalent to the no-store path"
        );
        assert!(with_store.moment_directive.is_none());
        assert!(
            store.after_injection_calls().is_empty(),
            "no directive injected ⇒ no lifecycle call"
        );
    }

    #[tokio::test]
    async fn directive_never_leaks_into_activation_trace_or_world_kb() {
        // AC-I3: the Moment Directive is product-local — it must never appear
        // in the activation trace (the AssemblePacket `activation_trace[]`
        // source) nor in the World-KB text (the `modules.placement[]` source).
        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Character,
                "Hero",
                "kb_hero",
                Some(serde_json::json!({"activation": {"keys": ["king"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();

        let stage0 = Stage0Assembly {
            personality: "A king rules the land.".to_string(),
            ..minimal_stage0()
        };
        let store = TestDirectiveStore::with_directive(active_directive(DirectiveDepth::Head));
        let request = MomentRequest::new(stage0)
            .with_world("wld_1")
            .with_creator("ctr_1");
        let ctx = assemble_moment_with_directive(
            &request,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
            &store,
        )
        .await;

        // The directive IS injected into the slot.
        assert_eq!(
            ctx.moment_directive.as_deref(),
            Some("Keep the prose terse.")
        );
        // The trace is present (activation fired on "king") and carries the
        // entry — but never the directive body.
        let trace = ctx.activation_trace.expect("activation trace present");
        assert!(
            trace.iter().any(|t| t.entry_id == "kb_hero" && t.accepted),
            "activation trace still carries KB entries"
        );
        let trace_json = serde_json::to_string(&trace).expect("trace serializes");
        assert!(
            !trace_json.contains("Keep the prose terse."),
            "directive body must never appear in activation_trace (AC-I3)"
        );
        // The directive body is not lore: absent from the World-KB text.
        let kb_text = ctx.world_kb.expect("world KB present");
        assert!(
            !kb_text.contains("Keep the prose terse."),
            "directive body must never appear in the World-KB section (AC-I3)"
        );
        assert!(
            kb_text.contains("Hero"),
            "activated KB entry still renders normally"
        );
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
        let kb1 = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        let kb2 = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Scene,
            "Castle",
        );
        stores.kb.insert_knowledge_entry(kb1).await.unwrap();
        stores.kb.insert_knowledge_entry(kb2).await.unwrap();

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

        let kb1 = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        let kb2 = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Scene,
            "Castle",
        );
        stores.kb.insert_knowledge_entry(kb1).await.unwrap();
        stores.kb.insert_knowledge_entry(kb2).await.unwrap();

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
        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero with a long description",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        // Set up knowledge
        let entry = nexus_knowledge::UserKnowledgeEntry::new(
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
            moment_directive: None,
            moment_directive_depth: DirectiveDepth::default(),
            activation_trace: None,
            slot_map: None,
            activation_budget: None,
            moment_directive_meta: None,
            hygiene_trace: None,
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
        assert!(rest.contains("10 years."), "rest must contain experience");
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
            moment_directive: None,
            moment_directive_depth: DirectiveDepth::default(),
            activation_trace: None,
            slot_map: None,
            activation_budget: None,
            moment_directive_meta: None,
            hygiene_trace: None,
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
        let legacy_content =
            "System prefix.\n\n## Personality\n\nA creative soul.\n\n## Experience\n\n10 years.\n";

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
        assert!(rest.contains("10 years"), "rest must contain experience");
    }

    // ── V1.146 P4 T2: activation flag tests ────────────────────────

    /// Helper: create a `KnowledgeEntryRecord` with optional `modules.activation` JSON.
    fn kb_entry_with_modules(
        world_id: &str,
        block_type: nexus_contracts::BlockType,
        name: &str,
        entry_id: &str,
        modules: Option<serde_json::Value>,
    ) -> KnowledgeEntryRecord {
        let mut entry = KnowledgeEntryRecord::new(world_id, block_type, name);
        entry.entry_id = entry_id.to_string();
        entry.modules = modules;
        entry
    }

    #[tokio::test]
    async fn activation_flag_off_includes_all_entries() {
        // Explicit OFF (off-switch semantics): entries with activation modules
        // all appear — byte-identical to V1.146 flag-off behavior.
        let stores = TestStores::new();

        let hero = kb_entry_with_modules(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
            "kb_h",
            Some(serde_json::json!({"activation": {"keys": ["hero"], "logic": "and_any"}})),
        );
        stores.kb.insert_knowledge_entry(hero).await.unwrap();

        let castle = kb_entry_with_modules(
            "wld_1",
            nexus_contracts::BlockType::Scene,
            "Castle",
            "kb_c",
            None,
        );
        stores.kb.insert_knowledge_entry(castle).await.unwrap();

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_activation_enabled(false);
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(kb_text.contains("Hero"), "flag OFF: all entries appear");
        assert!(kb_text.contains("Castle"), "flag OFF: all entries appear");
    }

    #[tokio::test]
    async fn activation_off_hinted_entries_emit_flat_v149_block() {
        // Off-switch (V1.149 escape hatch, lock #1 — "off ⇒ every candidate
        // entry unchanged", V1.146 flag-off semantics): slot routing is an
        // activation-product shaping step and MUST NOT run when activation is
        // disabled. Entries carrying `position_hint` / `outlet` hints render
        // as the V1.149 flat block — byte-identical, with no `### World
        // (Before)` / `### Outlet:` / `### Style (Post-History)` sub-headings.
        let stores = TestStores::new();
        for (name, id, modules) in [
            (
                "Rules",
                "kb_rules",
                Some(serde_json::json!({"activation": {"position_hint": "before_defs"}})),
            ),
            (
                "Style Note",
                "kb_style",
                Some(
                    serde_json::json!({"activation": {"position_hint": "outlet", "outlet": "style.post_history"}}),
                ),
            ),
            (
                "Open Lore",
                "kb_open",
                Some(
                    serde_json::json!({"activation": {"position_hint": "outlet", "outlet": "zone.z"}}),
                ),
            ),
            ("Neutral", "kb_neutral", None),
        ] {
            stores
                .kb
                .insert_knowledge_entry(kb_entry_with_modules(
                    "wld_1",
                    nexus_contracts::BlockType::Character,
                    name,
                    id,
                    modules,
                ))
                .await
                .unwrap();
        }

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_activation_enabled(false);
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.expect("world_kb must be present");

        // Every candidate entry appears unchanged (off-switch: no filtering).
        for name in ["Rules", "Style Note", "Open Lore", "Neutral"] {
            assert!(kb_text.contains(name), "off-switch: {name} must appear");
        }
        // No slot sub-headings — the off-switch output is the flat block.
        assert!(
            !kb_text.contains("### "),
            "off-switch must not render slot sub-headings, got: {kb_text}"
        );

        // Byte-identical to the V1.149 flat block for the same entries: the
        // store read order is shared between the assembly and this re-query
        // (same `InMemoryKbStore` instance, no mutation in between), so the
        // expected string is deterministic.
        let items = stores
            .kb
            .query(&nexus_knowledge::world_kb::KbQuery::new("wld_1"))
            .await
            .expect("query succeeds")
            .items;
        let expected = slots::format_entries(&items);
        assert_eq!(
            kb_text, expected,
            "off-switch output must be byte-identical to the V1.149 flat block"
        );
    }

    #[tokio::test]
    async fn activation_flag_on_no_activation_module_includes_all() {
        // Flag ON but no entries carry activation modules → same output as OFF.
        let stores = TestStores::new();

        for (id, name, bt) in [
            ("kb_a", "Hero", nexus_contracts::BlockType::Character),
            ("kb_b", "Castle", nexus_contracts::BlockType::Scene),
            ("kb_c", "Forest", nexus_contracts::BlockType::Scene),
        ] {
            let entry = kb_entry_with_modules("wld_1", bt, name, id, None);
            stores.kb.insert_knowledge_entry(entry).await.unwrap();
        }

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_activation_enabled(true);

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(kb_text.contains("Hero"));
        assert!(kb_text.contains("Castle"));
        assert!(kb_text.contains("Forest"));
    }

    #[tokio::test]
    async fn activation_default_on_filters_unmatched_entries() {
        // Default-ON (no explicit flag): stage0 mentions "king" → Hero matches
        // (key "king"), Castle is unmatched (key "dragon"), Forest is neutral.
        let stores = TestStores::new();

        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Character,
                "Hero",
                "kb_hero",
                Some(serde_json::json!({"activation": {"keys": ["king"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Scene,
                "Castle",
                "kb_castle",
                Some(serde_json::json!({"activation": {"keys": ["dragon"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Scene,
                "Forest",
                "kb_forest",
                None,
            ))
            .await
            .unwrap();

        let stage0 = Stage0Assembly {
            personality: "A king rules the land.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        };
        // No with_activation_enabled call — the default is ON since V1.149.
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(
            kb_text.contains("Hero"),
            "Hero should match 'king' in stage0"
        );
        assert!(
            !kb_text.contains("Castle"),
            "Castle should be filtered (no 'dragon' match)"
        );
        assert!(
            kb_text.contains("Forest"),
            "Forest (neutral, no modules) should survive activation"
        );
    }
    // ── DF-79: hygiene transforms (read-path only) ───────────────────

    #[tokio::test]
    async fn hygiene_transform_applies_at_emission_and_stored_body_unchanged() {
        // The transform applies to the emitted `body.summary` text; the
        // stored World-KB body stays byte-identical (read-path invariant).
        let stores = TestStores::new();
        let mut kb = KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, "Hero");
        kb.entry_id = "kb_hygiene".to_string();
        kb.body = Some(nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
            summary: Some("The hero fights the dragon".to_string()),
            attributes: Some(serde_json::json!({
                "hygiene": [{ "pattern": "dragon", "replacement": "wyrm" }]
            })),
            ..Default::default()
        });
        kb.modules =
            Some(serde_json::json!({"activation": {"keys": ["hero"], "logic": "and_any"}}));
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        let stage0 = Stage0Assembly {
            personality: "A hero rises.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(
            kb_text.contains("wyrm"),
            "emitted summary must carry the transform"
        );
        assert!(
            !kb_text.contains("dragon"),
            "emitted summary must not carry the raw text"
        );

        // Trace row captured on the context.
        let trace = ctx.hygiene_trace.expect("hygiene trace must be captured");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].entry_id, "kb_hygiene");
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);

        // Read-path invariant: the stored body is byte-identical.
        let stored_entry = stores.kb.get_knowledge_entry("kb_hygiene").await.unwrap();
        let stored_summary = stored_entry
            .body
            .expect("stored body")
            .summary
            .expect("stored summary");
        assert_eq!(stored_summary, "The hero fights the dragon");
    }

    #[tokio::test]
    async fn hygiene_neutral_entries_byte_identical_and_no_trace_rows() {
        // No carrier → no trace rows; assembly output is the plain flat
        // block (byte-identical to the no-hygiene path).
        let stores = TestStores::new();
        for (id, name) in [("kb_a", "Hero"), ("kb_b", "Castle")] {
            let mut kb = KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, name);
            kb.entry_id = id.to_string();
            kb.body = Some(nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            stores.kb.insert_knowledge_entry(kb).await.unwrap();
        }

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(kb_text.contains("Hero summary"));
        assert!(kb_text.contains("Castle summary"));
        // The pass ran (activation-on path) but no carrier → empty trace.
        let trace = ctx
            .hygiene_trace
            .expect("hygiene pass ran on the activation-on path");
        assert!(trace.is_empty());
    }

    #[tokio::test]
    async fn hygiene_carrier_survives_edit_patch_round_trip() {
        // Simulates `creator world kb edit --body`: JSON round-trip through
        // `KnowledgeEntryBody` + `update_knowledge_entry` (the kb_edit code path).
        // The `attributes.hygiene` carrier must survive the store round-trip
        // and drive the emission transform.
        let stores = TestStores::new();
        let mut kb = KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, "Hero");
        kb.entry_id = "kb_hygiene".to_string();
        kb.body = Some(nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
            summary: Some("The hero fights the dragon".to_string()),
            ..Default::default()
        });
        kb.modules =
            Some(serde_json::json!({"activation": {"keys": ["hero"], "logic": "and_any"}}));
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        // Author patches the body with a hygiene carrier (kb_edit flow).
        let patched: nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody =
            serde_json::from_str(
                r#"{"summary":"The hero fights the dragon","attributes":{"hygiene":[{"pattern":"dragon","replacement":"wyrm"}]}}"#,
            )
            .expect("patch body must parse");
        let mut stored_entry = stores.kb.get_knowledge_entry("kb_hygiene").await.unwrap();
        stored_entry.body = Some(patched);
        stores
            .kb
            .update_knowledge_entry(stored_entry)
            .await
            .unwrap();

        // Re-read: the carrier survived the store round-trip.
        let re_read = stores.kb.get_knowledge_entry("kb_hygiene").await.unwrap();
        let attrs = re_read
            .body
            .expect("stored body")
            .attributes
            .expect("stored attributes");
        assert_eq!(attrs["hygiene"][0]["pattern"], "dragon");

        // Assembly applies the transform.
        let stage0 = Stage0Assembly {
            personality: "A hero rises.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(
            kb_text.contains("wyrm"),
            "patched carrier must drive the transform"
        );
    }

    #[tokio::test]
    async fn activation_default_on_and_all_requires_all_keys_with_secondary() {
        // and_all with secondary_keys: entry needs ALL primary AND ALL
        // secondary keys to match (handbook truth table §2.1).
        let stores = TestStores::new();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Character,
                "Royal Guard",
                "kb_guard",
                Some(
                    serde_json::json!({"activation": {"keys": ["king", "throne"], "secondary_keys": ["guard"], "logic": "and_all"}}),
                ),
            ))
            .await
            .unwrap();

        let stage0 = Stage0Assembly {
            personality: "The king sat on the throne while the guard stood watch.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        assert!(ctx.world_kb.is_some(), "all 3 keys matched → entry present");
        assert!(ctx.world_kb.unwrap().contains("Royal Guard"));
    }

    #[tokio::test]
    async fn activation_default_on_not_any_excludes_when_secondary_matches() {
        // not_any with secondary_keys: entry excluded when a secondary key
        // matches (primary-any + no secondary = fire; spec §2.1).
        let stores = TestStores::new();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Character,
                "Orc Warlord",
                "kb_orc",
                Some(
                    serde_json::json!({"activation": {"keys": ["orc"], "secondary_keys": ["army"], "logic": "not_any"}}),
                ),
            ))
            .await
            .unwrap();

        let stage0 = Stage0Assembly {
            personality: "The orc army marched forward.".to_string(),
            ..Stage0Assembly::default()
        };
        // Default-on (no explicit flag).
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        assert!(
            ctx.world_kb.is_none(),
            "Orc Warlord excluded by not_any (secondary 'army' matched) → no entries remain"
        );
    }

    #[tokio::test]
    async fn activation_empty_world_yields_none() {
        // Default-on but no KB entries → world_kb is None.
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_ghost");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        assert!(ctx.world_kb.is_none());
    }

    #[tokio::test]
    async fn activation_timeline_participates_in_scan() {
        // Extended scan (V1.149 P0 T2): the activation key matches ONLY the
        // timeline outline-beat text (event title), not stage0 and not the
        // entry's own self-match text — the entry fires via timeline text.
        use nexus_narrative::timeline_event::{TimelineEvent, TimelineEventType};
        let stores = TestStores::new();

        let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 1);
        event.title = Some("The dawn dock heist".to_string());
        stores.narrative.insert_event(event);

        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Scene,
                "Dawn Dock",
                "kb_dock",
                Some(serde_json::json!({"activation": {"keys": ["heist"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
        // A second entry whose key appears nowhere → filtered.
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Character,
                "Ghost",
                "kb_ghost",
                Some(serde_json::json!({"activation": {"keys": ["necromancer"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();

        let stage0 = Stage0Assembly {
            personality: "A quiet village morning.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write the next beat.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
        let kb_text = ctx.world_kb.expect("world_kb must be present");
        assert!(
            kb_text.contains("Dawn Dock"),
            "timeline title 'dawn dock' must activate the entry under default-on"
        );
        assert!(
            !kb_text.contains("Ghost"),
            "Ghost must be filtered (key appears nowhere)"
        );
    }

    // ── V1.149 P1: relation-hop expansion (fixture edges, no DB) ──────

    fn hop_edge(from_id: &str, to_id: &str, relation_type: &str) -> HopEdge {
        HopEdge {
            relation_id: format!("rel_{from_id}_{to_id}"),
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            relation_type: relation_type.to_string(),
        }
    }

    fn stage0_with(personality: &str) -> Stage0Assembly {
        Stage0Assembly {
            personality: personality.to_string(),
            ..minimal_stage0()
        }
    }

    /// Seed: Harbor (fires on "king"), Dawn Dock (key "dragon" — no match),
    /// Harbor Guild (key "elf" — no match). Edges: Harbor→Dawn Dock→Guild.
    async fn seed_hop_fixture(stores: &TestStores) {
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Scene,
                "Harbor",
                "kb_harbor",
                Some(serde_json::json!({"activation": {"keys": ["king"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Scene,
                "Dawn Dock",
                "kb_dock",
                Some(serde_json::json!({"activation": {"keys": ["dragon"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
        stores
            .kb
            .insert_knowledge_entry(kb_entry_with_modules(
                "wld_1",
                nexus_contracts::BlockType::Organization,
                "Harbor Guild",
                "kb_guild",
                Some(serde_json::json!({"activation": {"keys": ["elf"], "logic": "and_any"}})),
            ))
            .await
            .unwrap();
    }

    fn hop_fixture_edges() -> Vec<HopEdge> {
        vec![
            hop_edge("kb_harbor", "kb_dock", "located_in"),
            hop_edge("kb_dock", "kb_guild", "member_of"),
        ]
    }

    #[tokio::test]
    async fn activation_hops_pull_graph_neighbors() {
        // Harbor fires on "king" → BFS pulls Dawn Dock (1 hop) and Harbor
        // Guild (2 hops) without keyword-tagging them (spec §5 product story).
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land."))
            .with_world("wld_1")
            .with_hop_edges(hop_fixture_edges());
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        let kb_text = ctx.world_kb.expect("world_kb must be present");
        assert!(kb_text.contains("Harbor"), "primary hit present");
        assert!(
            kb_text.contains("Dawn Dock"),
            "1-hop neighbor pulled without keyword match"
        );
        assert!(
            kb_text.contains("Harbor Guild"),
            "2-hop neighbor pulled without keyword match"
        );
        // Hop trace rows carry the hop fields.
        let trace = ctx.activation_trace.expect("trace present");
        let dock_hop = trace
            .iter()
            .find(|t| t.entry_id == "kb_dock" && t.accepted)
            .expect("Dawn Dock hop row");
        assert_eq!(dock_hop.hop_origin_entry_id.as_deref(), Some("kb_harbor"));
        assert_eq!(dock_hop.hop_depth, Some(1));
        let guild_hop = trace
            .iter()
            .find(|t| t.entry_id == "kb_guild" && t.accepted)
            .expect("Harbor Guild hop row");
        assert_eq!(guild_hop.hop_origin_entry_id.as_deref(), Some("kb_dock"));
        assert_eq!(guild_hop.hop_depth, Some(2));
    }

    #[tokio::test]
    async fn activation_hops_without_edges_is_p0_only() {
        // No preloaded edges ⇒ activation-only behavior (P0): neighbors stay
        // filtered out.
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land.")).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        let kb_text = ctx.world_kb.expect("world_kb must be present");
        assert!(kb_text.contains("Harbor"), "primary hit present");
        assert!(!kb_text.contains("Dawn Dock"), "no edges ⇒ no hop pull");
        assert!(!kb_text.contains("Harbor Guild"), "no edges ⇒ no hop pull");
    }

    #[tokio::test]
    async fn activation_hops_off_switch_returns_all_entries_no_hop() {
        // Off-switch (V1.146 flag-off semantics): even with hop edges
        // preloaded, every entry is returned unchanged and nothing hops.
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land."))
            .with_world("wld_1")
            .with_activation_enabled(false)
            .with_hop_edges(hop_fixture_edges());
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        let kb_text = ctx.world_kb.expect("world_kb must be present");
        assert!(kb_text.contains("Harbor"));
        assert!(kb_text.contains("Dawn Dock"), "off-switch: all entries");
        assert!(kb_text.contains("Harbor Guild"), "off-switch: all entries");
        assert!(
            ctx.activation_trace.is_none(),
            "off-switch: no activation trace at all"
        );
    }

    #[tokio::test]
    async fn activation_hops_neutral_only_byte_equivalent() {
        // Neutral-only golden under hops: no activation modules ⇒ no seeds ⇒
        // no hop pull ⇒ world_kb byte-identical with and without edges
        // (the neutral-only ship guarantee, spec §1, holds under hops).
        //
        // Both runs share ONE store: `InMemoryKbStore` iterates a
        // per-instance-seeded `HashMap`, so two separately-seeded stores can
        // return identical entries in different orders (flaky test, fixed
        // T3); reads are pure, so a single store keeps the input order
        // identical and the byte-comparison meaningful.
        let stores = TestStores::new();
        for (name, id) in [("Hero", "kb_n1"), ("Castle", "kb_n2")] {
            stores
                .kb
                .insert_knowledge_entry(kb_entry_with_modules(
                    "wld_1",
                    nexus_contracts::BlockType::Scene,
                    name,
                    id,
                    None,
                ))
                .await
                .unwrap();
        }

        let request_plain = MomentRequest::new(stage0_with("Any text.")).with_world("wld_1");
        let ctx_plain = assemble_moment(
            &request_plain,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
        )
        .await;

        let request_edges = MomentRequest::new(stage0_with("Any text."))
            .with_world("wld_1")
            .with_hop_edges(hop_fixture_edges());
        let ctx_edges = assemble_moment(
            &request_edges,
            &stores.narrative,
            &stores.kb,
            &stores.knowledge,
        )
        .await;

        assert_eq!(
            ctx_edges.world_kb, ctx_plain.world_kb,
            "neutral-only World: hop edges must not change assembled output bytes"
        );
        let trace_edges = ctx_edges
            .activation_trace
            .as_ref()
            .map(|t| serde_json::to_string(t).expect("trace serializes"));
        let trace_plain = ctx_plain
            .activation_trace
            .as_ref()
            .map(|t| serde_json::to_string(t).expect("trace serializes"));
        assert_eq!(
            trace_edges, trace_plain,
            "neutral-only World: trace identical (no hop rows)"
        );
    }

    // ── V1.149 P1: hop budget (spec Q1 — AC-I2 #4, MCA half) ──────────

    #[test]
    fn hop_budget_personality_reservation_can_zero_the_budget() {
        // AC-I2 #4: personality is reserved FIRST (never truncated), so a
        // personality section that alone exceeds max_chars leaves zero hop
        // budget — a large caller cap cannot override the reservation.
        let stage0 = stage0_with(&"P".repeat(200)); // section ≈ 219 chars
        let request = MomentRequest::new(stage0.clone())
            .with_max_tokens(50) // max_chars 200 < personality section
            .with_hop_max_tokens(1000);
        assert_eq!(
            hop_budget_tokens(&request, &stage0.assemble(), None, None),
            Some(0)
        );
    }

    #[test]
    fn hop_budget_caller_cap_bounds_the_remainder() {
        // The caller-provided hop cap is an upper bound on the computed
        // cross-domain remainder.
        let stage0 = stage0_with("small");
        let request = MomentRequest::new(stage0.clone())
            .with_max_tokens(1000) // remainder ≈ 994 tokens
            .with_hop_max_tokens(7);
        assert_eq!(
            hop_budget_tokens(&request, &stage0.assemble(), None, None),
            Some(7)
        );
    }

    #[test]
    fn hop_budget_no_max_tokens_passes_through_caller_cap() {
        // No `max_tokens` ⇒ no cross-domain remainder to compute: the caller
        // cap passes through unchanged (`None` stays `None` — depth + cycle
        // only, spec Q1).
        let stage0 = stage0_with("Any");
        let capped = MomentRequest::new(stage0.clone()).with_hop_max_tokens(9);
        assert_eq!(
            hop_budget_tokens(&capped, &stage0.assemble(), None, None),
            Some(9)
        );
        let uncapped = MomentRequest::new(stage0.clone());
        assert_eq!(
            hop_budget_tokens(&uncapped, &stage0.assemble(), None, None),
            None
        );
    }

    #[test]
    fn hop_budget_subtracts_world_state_and_timeline() {
        // Formula (spec Q1): `(max_tokens*4 − personality_section −
        // world_state − timeline) / 4`, floored. The personality section
        // length is derived from the assembled delimiter format so the
        // assertion pins the exact arithmetic.
        let stage0 = stage0_with("A king rules the land.");
        let assembled = stage0.assemble();
        let section_chars = {
            let start = assembled
                .find(STAGE0_PERSONALITY_START)
                .expect("start token")
                + STAGE0_PERSONALITY_START.len();
            let end = assembled.find(STAGE0_PERSONALITY_END).expect("end token");
            assembled[start..end].chars().count()
        };
        let ws = "ws";
        let tl = "tl";
        let request = MomentRequest::new(stage0).with_max_tokens(100); // 400 chars
        let budget = hop_budget_tokens(&request, &assembled, Some(ws), Some(tl));
        let expected = (400 - section_chars - ws.chars().count() - tl.chars().count()) / 4;
        assert_eq!(budget, Some(expected));
    }

    #[tokio::test]
    async fn activation_hops_tight_budget_keeps_personality_and_gates_pulls() {
        // AC-I2 #4 end-to-end: with `max_tokens` set, the hop budget is the
        // cross-domain remainder AFTER reserving personality (never
        // truncated), and the engine stops pulling when the remainder is
        // exhausted.
        //
        // Arithmetic (spec Q1): personality section = "\n## Personality\n\n"
        // + "A king rules the land." (21 chars) + "\n\n" = 40 chars.
        // max_tokens 14 ⇒ max_chars 56 ⇒ hop budget (56 − 40) / 4 = 4. The
        // engine reserves Harbor's primary-matched estimate (5/4 = 1) ⇒ 3
        // left. Dawn Dock (9/4 = 2) fits ⇒ pulled; Harbor Guild (12/4 = 3)
        // exceeds ⇒ skipped.
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land."))
            .with_world("wld_1")
            .with_max_tokens(14)
            .with_hop_edges(hop_fixture_edges());
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

        // Personality survives the tight budget untouched (never truncated):
        // the section content + heading are preserved (the truncation
        // reconstruction keeps the personality section verbatim — delimiter
        // tokens themselves live in the truncated remainder).
        assert!(ctx.stage0_context.contains("A king rules the land."));
        assert!(ctx.stage0_context.contains("## Personality"));

        // Budget-gated pull: Dock (fits the remainder) is accepted via hop,
        // Guild (exceeds it) is not.
        let trace = ctx.activation_trace.expect("trace present");
        assert!(trace.iter().any(|t| t.entry_id == "kb_dock" && t.accepted));
        assert!(trace
            .iter()
            .any(|t| t.entry_id == "kb_guild" && !t.accepted));
        assert!(!trace.iter().any(|t| t.entry_id == "kb_guild" && t.accepted));
    }
}
