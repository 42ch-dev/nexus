//! Moment-scoped context assembly — aggregates from all local domains.
//!
//! The [`MomentAssembly`] pulls context from three domain sources:
//!
//! 1. **Creator Memory** (`nexus-creator-memory`): SOUL sections, long-term memories,
//!    fragment keywords (via [`Stage0Assembly`]).
//! 2. **Narrative** (`nexus-narrative`): world state, timeline position, event snapshot
//!    (via [`NarrativeGateway`](nexus_narrative::NarrativeGateway)).
//! 3. **Knowledge Base** (`nexus-kb`): World-scoped KB assets / key blocks
//!    (via [`KbStore`](nexus_knowledge::world_kb::KbStore)).
//!
//! # Entity scope model (§4)
//!
//! `nexus-moment-context-assembly` owns the **Moment** scope — the per-interaction
//! context window assembled from all domain sources for a single ACP session turn.
//!
//! # Async
//!
//! Domain store queries are async.
// invariant: callers inject concrete store implementations — the crate ships
// no default runtime or storage backend.

use crate::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, MomentDirectiveStatus, NoDirectiveStore,
};
use crate::generation::GenerationStage;
use crate::slots::{self, SlotMapEntry};
use crate::stage0::{Stage0Assembly, STAGE0_PERSONALITY_END, STAGE0_PERSONALITY_START};
use crate::world_context::CharacterViewInput;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::{KbQuery, KbStore};
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::adapter::activation::{
    apply_activation_with_hops, ActivationBudget, HopConfig, HopEdge,
};

/// note: section heading for World State in assembled context.
const WORLD_STATE_HEADING: &str = "## World State";

/// note: section heading for Timeline in assembled context.
const TIMELINE_HEADING: &str = "## Timeline";

/// note: section heading for World Knowledge Base in assembled context.
const WORLD_KB_HEADING: &str = "## World Knowledge Base";

/// note: reserved Moment Directive slot (V1.150 P0 — spec §2 / Q1 provisional
/// lock), placed between `## Timeline` and `## World Knowledge Base`. P0
/// reserves the position but never renders it (no directive active); P1 fills
/// the slot.
const MOMENT_DIRECTIVE_HEADING: &str = "## Moment Directive";

/// note: P2 Character mind headings (empty rows; P3/P4 fill the same slots).
const CHARACTER_SOUL_HEADING: &str = "## Character SOUL";
const CHARACTER_MEMORY_HEADING: &str = "## Character Memory";
const CHARACTER_TOM_L1_HEADING: &str = "## Character ToM — L1";
const CHARACTER_TOM_L2_HEADING: &str = "## Character ToM — L2";

/// note: P3/P4 Character SOUL/Memory/ToM projection into the four fixed mind
/// slots.
///
/// Immutable once built: [`Self::new`] applies fixed deterministic bounds
/// (UTF-8-safe truncation + entry cap) so the projected slots are stable and
/// bounded when rendered. `None`/empty keep the P2 empty-heading bytes
/// identical (missing optional memory/ToM is an honest empty section).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterMindInput {
    /// note: SOUL.md text for the executing Character, bounded and truncated.
    pub soul: Option<String>,
    /// note: memory lines for the admitted scope, bounded and caller-ordered.
    pub memory: Vec<String>,
    /// note: L1 self-belief lines for the executing Character, bounded and in
    /// the Task 2 service's deterministic keyset order (v1.184 P4).
    pub tom_l1: Vec<String>,
    /// note: L2 lines — the executing Character's recorded model of other
    /// Characters, bounded and in the same keyset order (v1.184 P4).
    pub tom_l2: Vec<String>,
}

/// note: max Unicode scalar chars for the projected SOUL slot (4 KiB).
pub const CHARACTER_MIND_MAX_SOUL_CHARS: usize = 4096;
/// note: max memory lines projected into the `## Character Memory` slot.
pub const CHARACTER_MIND_MAX_MEMORY_ENTRIES: usize = 20;
/// note: max Unicode scalar chars per projected memory line.
pub const CHARACTER_MIND_MAX_ENTRY_CHARS: usize = 280;
/// note: max L1/L2 `ToM` lines projected into each `## Character ToM` slot.
pub const CHARACTER_MIND_MAX_TOM_ENTRIES: usize = 20;
/// note: max Unicode scalar chars per projected `ToM` line.
pub const CHARACTER_MIND_MAX_TOM_CHARS: usize = 280;

impl CharacterMindInput {
    /// note: builds a bounded, deterministic mind input.
    #[must_use]
    pub fn new(soul: Option<String>, memory: Vec<String>) -> Self {
        let soul = soul.map(|s| truncate_chars(&s, CHARACTER_MIND_MAX_SOUL_CHARS));
        let memory: Vec<String> = memory
            .into_iter()
            .take(CHARACTER_MIND_MAX_MEMORY_ENTRIES)
            .map(|line| truncate_chars(&line, CHARACTER_MIND_MAX_ENTRY_CHARS))
            .collect();
        Self {
            soul,
            memory,
            tom_l1: Vec::new(),
            tom_l2: Vec::new(),
        }
    }

    /// note: attaches bounded, deterministic L1/L2 `ToM` lines (v1.184 P4).
    ///
    /// L1 is projected before L2 by construction: the caller passes the
    /// already keyset-ordered `(order, carrier_entry_id, row_ordinal)` split;
    /// this builder only caps and truncates. Empty vecs keep the P2/P3
    /// empty-heading bytes identical (honest empty sections).
    #[must_use]
    pub fn with_tom(mut self, tom_l1: Vec<String>, tom_l2: Vec<String>) -> Self {
        self.tom_l1 = tom_l1
            .into_iter()
            .take(CHARACTER_MIND_MAX_TOM_ENTRIES)
            .map(|line| truncate_chars(&line, CHARACTER_MIND_MAX_TOM_CHARS))
            .collect();
        self.tom_l2 = tom_l2
            .into_iter()
            .take(CHARACTER_MIND_MAX_TOM_ENTRIES)
            .map(|line| truncate_chars(&line, CHARACTER_MIND_MAX_TOM_CHARS))
            .collect();
        self
    }

    /// note: whether neither slot carries content (used by tests to assert the
    /// legacy/honest-empty path).
    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.soul.as_deref().is_none_or(str::is_empty)
            && self.memory.is_empty()
            && self.tom_l1.is_empty()
            && self.tom_l2.is_empty()
    }
}

/// note: truncates `text` to at most `max_chars` Unicode scalar chars.
///
/// Counts scalar characters (never bytes) and, when truncating, reserves one
/// scalar for the `…` marker so the total is exactly `max_chars` — the bound
/// is a character cap, not a byte cap, and is UTF-8 safe (never splits a
/// multi-byte code point).
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut s: String = text.chars().take(keep).collect();
    s.push('…');
    s
}

/// note: actor discriminant for internal MCA context; authorization is
/// caller-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MomentActorKind {
    /// note: explicit Creator actor (no Character mind headings).
    Creator,
    /// note: Character actor — bounded view + four fixed empty mind headings.
    Character,
}

/// note: internal Actor context carried into [`assemble_moment`]; MCA does not
/// authorize — the daemon admission service must populate this.
///
/// # ActorView-only snapshot (HARD, v1.191 P1 T11 — durable §4.2)
///
/// Every Actor context carries its admitted snapshot, and there is no
/// constructor for an Actor without one: "missing/incomplete snapshot" is not
/// representable, so an Actor-bearing assembly can never fall back to a
/// World-wide `KbStore` query. An empty snapshot means "this actor sees
/// nothing" (fail-closed, empty World-KB section) — never "read the World
/// instead". A Character's snapshot is the rows of its World + Character +
/// binding containers its holder is admitted to; a Creator's is the same
/// selection over the owned containers.
#[derive(Debug, Clone)]
pub struct MomentActorContext {
    /// note: admitted actor kind.
    pub kind: MomentActorKind,
    /// note: the admitted `ActorView` snapshot — the complete filtered rows this
    /// actor may see (Character-global shared rows included, other holders'
    /// private rows excluded).
    pub character_view: CharacterViewInput,
    /// note: P3 SOUL/Memory projection for [`MomentActorKind::Character`];
    /// `None` means no projection (the P2 empty-heading bytes). Ignored for
    /// Creator.
    pub character_mind: Option<CharacterMindInput>,
}

impl MomentActorContext {
    /// note: Character actor with a bounded admitted view (may be empty rows).
    #[must_use]
    pub fn character(view: CharacterViewInput) -> Self {
        Self::character_with_mind(view, CharacterMindInput::default())
    }

    /// note: Character actor with a bounded admitted SOUL/Memory projection.
    #[must_use]
    pub const fn character_with_mind(view: CharacterViewInput, mind: CharacterMindInput) -> Self {
        Self {
            kind: MomentActorKind::Character,
            character_view: view,
            character_mind: Some(mind),
        }
    }

    /// note: Creator actor consuming its admitted `ActorView` snapshot.
    #[must_use]
    pub const fn creator_with_view(view: CharacterViewInput) -> Self {
        Self {
            kind: MomentActorKind::Creator,
            character_view: view,
            character_mind: None,
        }
    }

    const fn is_character(&self) -> bool {
        matches!(self.kind, MomentActorKind::Character)
    }
}

/// note: parameters for a single moment context assembly request.
///
/// All IDs are strings for now (matching current domain APIs).
/// Fields left as `None` indicate that domain source should be skipped.
#[derive(Debug, Clone)]
pub struct MomentRequest {
    /// note: World ID to pull narrative state and KB assets for.
    pub world_id: Option<String>,
    /// note: the work-bound moment's Work ID (V1.150 P1) — Moment Directive
    /// scope resolution (spec §3.2) and the chapter-advance TTL signal
    /// (spec §3.3) are keyed on it; optional, since the observation path may
    /// assemble a raw world without a Work.
    pub work_id: Option<String>,
    /// note: creator owning the moment (V1.150 P1) — directives are
    /// creator-scoped, so `None` means no directive can be in scope.
    pub creator_id: Option<String>,
    /// note: branch ID within the world (optional, for fork-specific context).
    pub branch_id: Option<String>,
    /// note: event ID to focus context around (optional).
    pub event_id: Option<String>,
    /// note: Stage-0 assembly inputs (SOUL, memories, fragments, prompt).
    pub stage0: Stage0Assembly,
    /// note: cross-domain token budget (approximate chars/4 heuristic); when
    /// set, domain sections are truncated after the never-truncated Stage-0
    /// personality section.
    pub max_tokens: Option<usize>,
    /// note: KB query cap — maximum number of key blocks to return.
    pub kb_limit: Option<usize>,
    /// note: KB query text filter (case-insensitive substring).
    pub kb_text_search: Option<String>,
    /// note: KB query filter by block type.
    pub kb_block_type: Option<BlockType>,
    /// note: lore activation filtering on `WorldKB` entries (V1.146 P4 T2,
    /// default-on since V1.149 P0 T2).
    ///
    /// When `true`, the activation pass runs after the `WorldKB` fetch,
    /// calling `apply_activation` to filter/inspect entries by their
    /// `modules.activation` fire-conditions.
    /// Default `true` — activation is the shipped product behavior; `false`
    /// restores V1.146 flag-off semantics (all entries returned unchanged).
    pub activation_enabled: bool,
    /// note: preloaded confirmed relation edges for relation-hop expansion
    /// (V1.149 P1, spec §5). `None` ⇒ activation-only pass
    /// (P0 behavior); `Some(..)` ⇒ `apply_activation_with_hops` — primary-
    /// fired / `constant` entries BFS-expand up to 2 graph hops within the
    /// hop token budget, and graph-adjacent entries join `matched` without
    /// re-firing keyword activation.
    ///
    /// Edges are loaded by the CLI/wire layer via
    /// `NexusAdapter::list_hop_edges_for_world` (the `RelationPort` gap:
    /// spoke's port is get/put only); MCA itself never walks relations.
    pub hop_edges: Option<Vec<HopEdge>>,
    /// note: caller-provided cap on the hop token budget (chars/4, spec Q1).
    /// When `max_tokens` is set, the effective budget is the cross-domain
    /// remainder after personality (never truncated) + `world_state` +
    /// `timeline` reservations, bounded by this cap; when `max_tokens` is
    /// absent, only this cap applies (`None` ⇒ depth + cycle only).
    /// See [`hop_budget_tokens`].
    pub hop_max_tokens: Option<usize>,
    /// note: generation stage for spec §4 slot gating (V1.150 P2, DF-75 —
    /// guide `mca-section-audit.md` Q4 lock).
    ///
    /// `None` (the default) is treated as [`GenerationStage::Unspecified`] —
    /// every slot fills, which is current behavior and the neutral golden /
    /// direct-CLI / inspector path (AC-I1b). `run_intent` is **derivable**
    /// from the stage (creator-workflow §3.1) — there is deliberately no
    /// separate field. Wired from the CLI `assemble-moment --stage` flag;
    /// the preset runner / schedule path threads the executing stage when it
    /// drives assembly (see `guides/generation-trigger-wiring.md`).
    pub generation_stage: Option<GenerationStage>,
    /// note: optional admitted Actor context (v1.184 P2); `None` is exact
    /// legacy.
    pub actor: Option<MomentActorContext>,
}

impl MomentRequest {
    /// note: minimal request with only Stage-0 inputs.
    #[must_use]
    pub const fn new(stage0: Stage0Assembly) -> Self {
        Self {
            world_id: None,
            work_id: None,
            creator_id: None,
            branch_id: None,
            event_id: None,
            stage0,
            max_tokens: None,
            kb_limit: None,
            kb_text_search: None,
            kb_block_type: None,
            activation_enabled: true,
            hop_edges: None,
            hop_max_tokens: None,
            generation_stage: None,
            actor: None,
        }
    }

    /// note: sets the world ID; branch and event have their own setters.
    #[must_use]
    pub fn with_world(mut self, world_id: impl Into<String>) -> Self {
        self.world_id = Some(world_id.into());
        self
    }

    /// note: sets the work ID for the work-bound moment (V1.150 P1 — Moment
    /// Directive scope resolution + chapter-advance TTL).
    #[must_use]
    pub fn with_work(mut self, work_id: impl Into<String>) -> Self {
        self.work_id = Some(work_id.into());
        self
    }

    /// note: sets the owning creator ID (V1.150 P1 — Moment Directives are
    /// creator-scoped; `None` ⇒ no directive can be in scope).
    #[must_use]
    pub fn with_creator(mut self, creator_id: impl Into<String>) -> Self {
        self.creator_id = Some(creator_id.into());
        self
    }

    /// note: sets the branch ID within the world.
    #[must_use]
    pub fn with_branch(mut self, branch_id: impl Into<String>) -> Self {
        self.branch_id = Some(branch_id.into());
        self
    }

    /// note: sets the focused event ID.
    #[must_use]
    pub fn with_event(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    /// note: sets the cross-domain token budget.
    #[must_use]
    pub const fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// note: sets the KB result limit.
    #[must_use]
    pub const fn with_kb_limit(mut self, limit: usize) -> Self {
        self.kb_limit = Some(limit);
        self
    }

    /// note: sets the KB text search filter.
    #[must_use]
    pub fn with_kb_text_search(mut self, text: impl Into<String>) -> Self {
        self.kb_text_search = Some(text.into());
        self
    }

    /// note: sets the KB block type filter.
    #[must_use]
    pub const fn with_kb_block_type(mut self, block_type: BlockType) -> Self {
        self.kb_block_type = Some(block_type);
        self
    }

    /// note: enables lore activation filtering (V1.146 P4 T2).
    #[must_use]
    pub const fn with_activation_enabled(mut self, enabled: bool) -> Self {
        self.activation_enabled = enabled;
        self
    }

    /// note: provides preloaded relation-hop edges (V1.149 P1, spec §5);
    /// omitting it keeps the activation-only P0 behavior.
    #[must_use]
    pub fn with_hop_edges(mut self, edges: Vec<HopEdge>) -> Self {
        self.hop_edges = Some(edges);
        self
    }

    /// note: caps the hop token budget (chars/4, spec Q1); see
    /// [`hop_budget_tokens`] for how the cap combines with `max_tokens`.
    #[must_use]
    pub const fn with_hop_max_tokens(mut self, cap: usize) -> Self {
        self.hop_max_tokens = Some(cap);
        self
    }

    /// note: sets the generation stage for spec §4 slot gating (V1.150 P2);
    /// leaving it unset keeps every slot on
    /// ([`GenerationStage::Unspecified`]).
    #[must_use]
    pub const fn with_generation_stage(mut self, stage: GenerationStage) -> Self {
        self.generation_stage = Some(stage);
        self
    }

    /// note: attaches admitted Actor context (Character or Creator).
    #[must_use]
    pub fn with_actor(mut self, actor: MomentActorContext) -> Self {
        self.actor = Some(actor);
        self
    }
}

/// note: assembled context from all domain sources for a single moment.
///
/// Each field is `Some(...)` if that domain source was queried successfully,
/// or `None` if it was skipped, failed, or returned no data.
#[derive(Debug, Clone, Default)]
pub struct MomentContext {
    /// note: Stage-0 context (always present — SOUL, memories, fragments,
    /// prompt).
    pub stage0_context: String,
    /// note: narrative world state (present when a `world_id` was provided and
    /// found).
    pub world_state: Option<String>,
    /// note: timeline summary text (when available).
    pub timeline: Option<String>,
    /// note: World KB summary text (key blocks for the world).
    pub world_kb: Option<String>,
    /// note: Moment Directive slot (V1.150 P0 reserved — V1.150 P1 fills it) —
    /// the `## Moment Directive` top-level section within the directive region
    /// (above lore, below system/personality). `None` means no directive is
    /// active and the section is never rendered — the neutral-only
    /// byte-equivalence anchor (AC-I1b).
    pub moment_directive: Option<String>,
    /// note: placement of the directive section within the directive region
    /// (V1.150 P1); defaults to [`DirectiveDepth::Tail`] — P0's reserved
    /// position between `## Timeline` and `## World Knowledge Base` — so
    /// contexts that set only the body keep the P0 layout.
    pub moment_directive_depth: DirectiveDepth,
    /// note: per-entry activation trace (populated when `activation_enabled` is
    /// true; V1.146 P4 T3 exposes it for inspector packet emission).
    pub activation_trace:
        Option<Vec<nexus_spoke_adapter::adapter::activation::ActivationTraceEntry>>,
    /// note: slot map (V1.151 P0, DF-76 spec §2 H2) — every accepted entry that
    /// survived the generation-stage gate mapped to its slot id, captured
    /// **post stage-gate** so the map reflects what actually rendered.
    /// `None` when no slot routing ran (no World-KB, activation off, or all
    /// entries gated off). A synthetic
    /// `{ entry_id: <directive_id>, slot: "moment.directive" }` entry is
    /// appended when a directive injected this assembly.
    pub slot_map: Option<Vec<SlotMapEntry>>,
    /// note: activation token-budget accounting (spec §2 H3) — chars/4
    /// estimates for primary matches vs. relation hops + cap/remaining. `Some`
    /// whenever activation ran; `None` when activation is disabled. Additive —
    /// never part of `to_full_context()` (AC-I6).
    pub activation_budget: Option<ActivationBudget>,
    /// note: status-only Moment Directive metadata for the inspector packet
    /// (spec §2 H6) — **NEVER the directive body** (AC-I3; body exclusion is by
    /// construction, since the packet builder reads only this field). `None`
    /// when no directive is active. Additive — never part of
    /// `to_full_context()`.
    pub moment_directive_meta: Option<MomentDirectiveStatus>,
    /// note: hygiene trace (DF-79) — per-entry rows for carrier-bearing
    /// entries, `{entry_id, applied, skipped, notes}`. `None` when no hygiene
    /// pass ran (no World-KB, activation off, or all entries gated off).
    /// Additive — never part of `to_full_context()` (AC-I6).
    pub hygiene_trace: Option<Vec<crate::hygiene::HygieneTraceEntry>>,
    /// note: P3 SOUL/Memory projection into the four fixed Character mind
    /// headings. `Some(mind)` emits the headings (SOUL/Memory bodies when
    /// present; the two `ToM` headings always empty); `None` is legacy — empty
    /// sections stay omitted.
    pub character_mind: Option<CharacterMindInput>,
}

impl MomentContext {
    /// note: assembles the full context string from all sources.
    ///
    /// Follows the spec ordering (§9.2) with domain extensions:
    /// 1. Stage-0 context (system prefix, personality, memories, keywords, experience, prompt)
    /// 2. World state (narrative)
    /// 3. Timeline (narrative)
    /// 4. Moment Directive (V1.150 P0 reserved — only rendered when P1
    ///    fills the slot)
    /// 5. World KB (key blocks — V1.150 P0 slots subdivide this section)
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
        let character_mind = self
            .character_mind
            .as_ref()
            .map(render_character_mind_headings);

        let mut parts: Vec<Option<String>> = vec![stage0, character_mind];
        match self.moment_directive_depth {
            DirectiveDepth::Head => {
                parts.push(directive);
                parts.push(world_state);
                parts.push(timeline);
                parts.push(world_kb);
            }
            DirectiveDepth::Mid => {
                parts.push(world_state);
                parts.push(directive);
                parts.push(timeline);
                parts.push(world_kb);
            }
            DirectiveDepth::Tail => {
                parts.push(world_state);
                parts.push(timeline);
                parts.push(directive);
                parts.push(world_kb);
            }
        }
        parts.into_iter().flatten().collect::<Vec<_>>().join("\n")
    }

    /// note: applies cross-domain token budget truncation.
    ///
    /// Personality section inside Stage-0 is never truncated.
    /// The remaining budget (after personality) is distributed across
    /// `world_state`, `timeline`, and `world_kb` in order.
    ///
    /// Token count uses chars/4 heuristic (spec §9.3).
    pub fn apply_cross_domain_truncation(&mut self, max_tokens: usize) {
        let max_chars = max_tokens.saturating_mul(4);

        // why: the personality section is never truncated, so it is reserved
        // before any domain section draws on the budget.
        let (personality_part, rest_stage0) = self.split_stage0_personality();

        let personality_chars = personality_part.chars().count();
        let mut remaining = max_chars.saturating_sub(personality_chars);

        remaining = Self::truncate_section(&mut self.world_state, remaining);
        remaining = Self::truncate_section(&mut self.timeline, remaining);
        remaining = Self::truncate_section(&mut self.world_kb, remaining);

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

    /// note: splits `stage0_context` into (`personality_section`, rest).
    ///
    /// Prefers structured delimiter split (`---STAGE0:PERSONALITY:START---` /
    /// `---STAGE0:PERSONALITY:END---`). Falls back to the markdown-header
    /// heuristic for legacy content without delimiters.
    fn split_stage0_personality(&self) -> (String, String) {
        split_stage0_personality(&self.stage0_context)
    }

    /// note: truncates a section to fit within `max_chars`, returning remaining
    /// chars.
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

    /// note: truncates text to at most `max_chars` characters, preferring a
    /// line-boundary break.
    fn truncate_text(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }
        let truncated: String = text.chars().take(max_chars).collect();
        if let Some(pos) = truncated.rfind('\n') {
            truncated[..pos].to_string()
        } else {
            truncated
        }
    }
}

/// note: extracts the personality section from a Stage-0 context string — the
/// shared helper behind [`MomentContext::split_stage0_personality`] (delimiter
/// protocol first, legacy `## Personality` heuristic fallback). The relation-
/// hop budget reuses it so the personality reservation matches exactly what
/// cross-domain truncation protects (personality is **never** truncated).
fn split_stage0_personality(ctx: &str) -> (String, String) {
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

    ctx.find("## Personality").map_or_else(
        || (String::new(), ctx.to_string()),
        |pos| {
            let after_personality_header = &ctx[pos..];
            let end_of_personality = after_personality_header[14..] // note: 14 skips the "## Personality" header.
                .find("\n## ")
                .map_or(after_personality_header.len(), |i| 14 + i);

            let personality_section = ctx[pos..pos + end_of_personality].to_string();
            let rest = format!("{}{}", &ctx[..pos], &ctx[pos + end_of_personality..]);
            (personality_section, rest)
        },
    )
}

/// note: hop token budget per iteration spec Q1 (architect lock).
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

/// note: assembles moment context from all domain sources — the primary entry
/// point for full moment context assembly.
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
#[allow(clippy::future_not_send)]
pub async fn assemble_moment<G, K>(
    request: &MomentRequest,
    narrative: &G,
    kb_store: &K,
) -> MomentContext
where
    G: NarrativeGateway,
    K: KbStore,
{
    assemble_moment_with_directive(request, narrative, kb_store, &NoDirectiveStore).await
}

/// note: assembles moment context with a Moment Directive store (V1.150 P1,
/// DF-75).
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
/// - `G`, `K`: as in [`assemble_moment`].
/// - `D`: A [`DirectiveStore`] implementation (composition root adapter over
///   `nexus-local-db`; in-memory stub in tests).
#[allow(clippy::future_not_send)]
// why: three-domain assembly orchestrator (stage-0 → world_state → timeline →
// world-kb → directive); the per-section blocks keep the function at ~114
// lines, and splitting would scatter one assembly's steps.
#[allow(clippy::too_many_lines)]
pub async fn assemble_moment_with_directive<G, K, D>(
    request: &MomentRequest,
    narrative: &G,
    kb_store: &K,
    directives: &D,
) -> MomentContext
where
    G: NarrativeGateway,
    K: KbStore,
    D: DirectiveStore,
{
    let stage0_context = if request.stage0.max_tokens.is_some() {
        request.stage0.assemble_with_truncation()
    } else {
        request.stage0.assemble()
    };

    // invariant: a Character Actor never loads unrestricted World State or
    // Timeline.
    let skip_unrestricted_narrative = request
        .actor
        .as_ref()
        .is_some_and(MomentActorContext::is_character);
    let (world_state, timeline) = if skip_unrestricted_narrative {
        (None, None)
    } else if let Some(ref world_id) = request.world_id {
        match fetch_narrative_context(narrative, world_id, request.branch_id.as_deref()).await {
            Ok((ws, tl)) => (ws, tl),
            Err(_) => (None, None),
        }
    } else {
        (None, None)
    };

    // why: V1.149 P0 T2 activation scans Stage-0 full text plus the timeline's
    // outline beats, reused here before the timeline is stored on the context;
    // the manuscript body is not on the MCA path (documented gap, spec §3), so
    // Stage-0 is the only fallback text.
    let activation_scan_text = if request.activation_enabled {
        timeline.as_ref().map_or_else(
            || stage0_context.clone(),
            |tl| format!("{stage0_context}\n{tl}"),
        )
    } else {
        String::new()
    };

    // note: World KB is queried only when a `world_id` is given; V1.146 P4 T3
    // captures the activation trace for inspector packet emission.
    let mut activation_trace: Option<
        Vec<nexus_spoke_adapter::adapter::activation::ActivationTraceEntry>,
    > = None;
    // note: V1.151 P0 (DF-76 spec §2 H3) activation token-budget accounting
    // (primary/hop estimates + cap/remaining), captured alongside the trace.
    let mut activation_budget: Option<ActivationBudget> = None;
    // note: V1.151 P0 (spec §2 H2) post stage-gate slot map — which accepted
    // entry landed in which slot — for the inspector packet.
    let mut slot_map: Option<Vec<SlotMapEntry>> = None;
    // note: DF-79 per-entry hygiene trace (applied/skipped/notes) for the
    // inspector packet — `Some` whenever the hygiene pass ran.
    let mut hygiene_trace: Option<Vec<crate::hygiene::HygieneTraceEntry>> = None;
    let world_kb = if let Some(ref world_id) = request.world_id {
        match fetch_world_kb_entries(kb_store, world_id, request).await {
            Ok(entries) if !entries.is_empty() => {
                let entries = if request.activation_enabled {
                    // note: V1.149 P0 T2 default-on lore activation over the
                    // fetched WorldKB entries; the scan text is Stage-0 plus
                    // timeline outline beats (from step 2), unmatched entries
                    // are filtered out, and neutral entries (no activation
                    // module) remain in `matched`. V1.146 P4 T3 captures the
                    // full `ActivationResult` for trace emission; V1.149 P1
                    // additionally BFS-expands up to 2 graph hops from
                    // primary-fired/constant entries within the hop token
                    // budget (spec Q1 — `hop_budget_tokens`), hop-pulled
                    // entries joining `matched` without re-firing keys.
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
                                max_hops: 2, // note: architect lock Q1.
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
                    // note: V1.150 P0/P2 (DF-75) shapes the activation-matched
                    // entries into the World-KB body — spec §4 generation-stage
                    // gate first (`slots::apply_stage_gate`), then slot routing
                    // plus render (spec §2 / Q5). Neutral-only Worlds render
                    // byte-identically to the V1.149 flat block (AC-I1b); the
                    // gate runs only on the activation-on path (the off-switch
                    // below keeps every entry unchanged); `None` stage ⇒ all
                    // slots on. V1.151 P0 (spec §2 H2) captures the slot map
                    // post stage-gate so it reflects what actually rendered;
                    // DF-79 runs the hygiene pass inside `render_gated_slots`
                    // (between stage gate and slot routing) and captures its
                    // trace.
                    let (rendered, map, hygiene) =
                        render_gated_slots(entries, request.generation_stage);
                    slot_map = Some(map);
                    hygiene_trace = Some(hygiene);
                    rendered
                } else {
                    // invariant: with activation off (V1.149 escape hatch,
                    // lock #1 — "off ⇒ every candidate entry unchanged",
                    // V1.146 flag-off semantics) every entry is emitted
                    // UNCHANGED as the V1.149 flat block with no
                    // `### World (Before)` / `### Outlet:` /
                    // `### Style (Post-History)` sub-headings — slot routing is
                    // an activation-product shaping step and must not run.
                    Some(slots::format_entries(&entries))
                }
            }
            _ => None,
        }
    } else {
        None
    };

    // note: V1.150 P1 (spec §3) resolves and loads the active directive for
    // the work-bound moment (scope resolution spec §3.2 — Work wins, else
    // World override for the Work's bound World, else none), then runs the
    // post-injection lifecycle. The plain `assemble_moment` path passes a
    // `NoDirectiveStore`, which always resolves `None` — nothing renders,
    // nothing decrements (AC-I1b).
    let directive = apply_directive(directives, request).await;

    // note: V1.151 P0 (spec §2 H2) — the directive occupies the reserved
    // `moment.directive` slot, so a synthetic slot-map entry appears only when
    // a directive injected this assembly (`moment.directive` is a top-level
    // section, never a World-KB routing slot).
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
        // note: V1.150 P0 reserved the slot; P1 fills it with the active
        // directive body — `None` ⇒ no `## Moment Directive` section
        // (neutral-only).
        moment_directive: directive.as_ref().map(|d| d.body.clone()),
        moment_directive_depth: directive
            .as_ref()
            .map_or_else(DirectiveDepth::default, |d| d.insert_depth),
        activation_trace,
        // note: V1.151 P0 (spec §2 H3/H6) additive inspector surface — budget
        // accounting plus status-only directive metadata (never the body).
        slot_map,
        activation_budget,
        moment_directive_meta: directive.as_ref().map(MomentDirectiveStatus::from),
        hygiene_trace,
        character_mind: request.actor.as_ref().and_then(|a| {
            a.is_character()
                .then(|| a.character_mind.clone().unwrap_or_default())
        }),
    };

    // invariant: cross-domain truncation never touches the directive section —
    // it is author instruction, like personality.
    if let Some(max_tokens) = request.max_tokens {
        ctx.apply_cross_domain_truncation(max_tokens);
    }

    ctx
}

/// note: resolves the active Moment Directive for a request and — when one was
/// injected — runs the post-injection lifecycle (spec §3.3: TTL decrement /
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

/// note: V1.150 P2 (DF-75, spec §4 / Q5) — shapes the activation-matched
/// World-KB entries into the section body: apply the generation-stage gate
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
    // note: DF-79 — the hygiene pass runs between the generation-stage gate
    // and slot routing; transforms shape the emitted `body.summary` text on the
    // owned assembly-local copies (read-path only).
    let gated = slots::apply_stage_gate(entries, stage);
    let (hygiened, trace) = crate::hygiene::apply_hygiene(gated);
    let routing = slots::route_slots(hygiened);
    let map = routing.to_slot_map();
    (slots::render_slots(&routing), map, trace)
}

fn render_character_mind_headings(mind: &CharacterMindInput) -> String {
    let soul = mind
        .soul
        .as_deref()
        .filter(|s| !s.is_empty())
        .map_or_default(|s| format!("{s}\n"));
    let memory = if mind.memory.is_empty() {
        String::new()
    } else {
        let lines = mind.memory.join("\n");
        format!("{lines}\n")
    };
    // note: ToM bodies (v1.184 P4) — absent rows keep the exact P2/P3
    // empty-heading bytes (`heading\n\n` straight into the next heading).
    let tom_l1 = if mind.tom_l1.is_empty() {
        String::new()
    } else {
        format!("{}\n", mind.tom_l1.join("\n"))
    };
    // note: the L2 section keeps the legacy trailing bytes exactly — empty
    // renders `heading\n`, filled renders `heading\n\n<lines>\n` (memory
    // pattern).
    let tom_l2_section = if mind.tom_l2.is_empty() {
        format!("{CHARACTER_TOM_L2_HEADING}\n")
    } else {
        format!("{CHARACTER_TOM_L2_HEADING}\n\n{}\n", mind.tom_l2.join("\n"))
    };
    format!(
        "{CHARACTER_SOUL_HEADING}\n\n{soul}{CHARACTER_MEMORY_HEADING}\n\n{memory}{CHARACTER_TOM_L1_HEADING}\n\n{tom_l1}{tom_l2_section}"
    )
}

/// note: fetches narrative context (world state + timeline) from the gateway.
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

/// note: fetches World KB entries as records, unformatted, so the activation
/// pass can operate on entries before formatting; callers use
/// `format_entries` (in `slots.rs`) to produce context text.
///
/// V1.146 P4 T2 extracted this step out of the formatting path.
///
/// # ActorView-only snapshot (HARD, v1.191 P1 T11 — durable §4.2)
///
/// An Actor-bearing assembly returns exactly its admitted snapshot: the rows
/// the caller's `KnowledgeReadScope` already filtered. It never issues a
/// World-wide `KbStore` query, so a missing or incomplete snapshot cannot
/// widen into one; an empty snapshot yields no World-KB section.
///
/// The non-Actor lane (raw World assembly / CLI preview, `actor == None`) is
/// the caller's own admitted store read: `kb_store` must be a scoped store
/// (e.g. `SpokeBackedKbStore`), and MCA holds no authority to widen it.
#[allow(clippy::future_not_send)]
async fn fetch_world_kb_entries<K: KbStore>(
    kb_store: &K,
    world_id: &str,
    request: &MomentRequest,
) -> Result<Vec<KnowledgeEntryRecord>, nexus_knowledge::world_kb::KbStoreError> {
    if let Some(actor) = request.actor.as_ref() {
        return Ok(actor.character_view.admitted_entries().to_vec());
    }
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
    Ok(result.items)
}

/// note: formats a [`WorldState`] into a human-readable context string.
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

/// note: formats timeline events into a human-readable context string.
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
    use nexus_narrative::InMemoryNarrativeGateway;
    use std::sync::Arc;

    /// note: creates a `Stage0Assembly` with minimal content.
    fn minimal_stage0() -> Stage0Assembly {
        Stage0Assembly {
            personality: "A creative writer.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        }
    }

    /// note: in-memory stores for testing.
    struct TestStores {
        narrative: InMemoryNarrativeGateway<InMemoryKbStore>,
        kb: InMemoryKbStore,
    }

    impl TestStores {
        fn new() -> Self {
            Self {
                narrative: InMemoryNarrativeGateway::new(InMemoryKbStore::new()),
                kb: InMemoryKbStore::new(),
            }
        }
    }

    #[tokio::test]
    async fn moment_assembly_stage0_only_when_no_ids() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0());

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        assert!(!ctx.stage0_context.contains("World State"));
        assert!(ctx.world_state.is_none());
        assert!(ctx.timeline.is_none());
        assert!(ctx.world_kb.is_none());
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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        assert!(ctx.timeline.is_some());
        let tl = ctx.timeline.unwrap();
        assert!(tl.contains("story_advance"));
    }

    #[tokio::test]
    async fn moment_assembly_world_not_found_gives_none() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_ghost");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        assert!(ctx.world_state.is_none());
        assert!(ctx.timeline.is_none());
    }

    #[tokio::test]
    async fn full_context_assembles_all_sections() {
        let stores = TestStores::new();

        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "Full World",
            "full-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
            "wld_1",
            "fbk_root",
            nexus_narrative::timeline_event::TimelineEventType::StoryAdvance,
            1,
        );
        event.title = Some("The Beginning".to_string());
        stores.narrative.insert_event(event);

        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        let full = ctx.to_full_context();
        assert!(
            full.contains(WORLD_STATE_HEADING),
            "should have world state"
        );
        assert!(full.contains(TIMELINE_HEADING), "should have timeline");
        assert!(full.contains(WORLD_KB_HEADING), "should have world KB");
        assert!(full.contains("Full World"));
        assert!(full.contains("The Beginning"));
        assert!(full.contains("Hero"));
    }

    #[tokio::test]
    async fn full_context_omits_empty_sections() {
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0());

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let full = ctx.to_full_context();

        assert!(!full.contains(WORLD_STATE_HEADING));
        assert!(!full.contains(TIMELINE_HEADING));
        assert!(!full.contains(WORLD_KB_HEADING));
        assert!(!full.contains(CHARACTER_SOUL_HEADING));
        assert!(!full.contains(CHARACTER_MEMORY_HEADING));
        assert!(!full.contains(CHARACTER_TOM_L1_HEADING));
        assert!(!full.contains(CHARACTER_TOM_L2_HEADING));
    }

    #[tokio::test]
    async fn character_mode_renders_empty_mind_headings_without_world_fallback() {
        let stores = TestStores::new();
        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "SecretNarrativeWorld",
            "secret-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);
        let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
            "wld_1",
            "fbk_root",
            nexus_narrative::timeline_event::TimelineEventType::StoryAdvance,
            1,
        );
        event.title = Some("SecretTimelineBeat".to_string());
        stores.narrative.insert_event(event);
        let secret = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "SecretWorldRow",
        );
        stores.kb.insert_knowledge_entry(secret).await.unwrap();
        let allowed = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "AdmittedAda",
        );
        let request = MomentRequest::new(Stage0Assembly {
            user_prompt: "Act.".to_string(),
            personality: "Creator SOUL must not leak.".to_string(),
            ..Stage0Assembly::default()
        })
        .with_world("wld_1")
        .with_actor(MomentActorContext::character(
            CharacterViewInput::from_entries(vec![allowed]),
        ));

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let full = ctx.to_full_context();
        assert!(full.contains(CHARACTER_SOUL_HEADING));
        assert!(full.contains(CHARACTER_MEMORY_HEADING));
        assert!(full.contains(CHARACTER_TOM_L1_HEADING));
        assert!(full.contains(CHARACTER_TOM_L2_HEADING));
        assert!(full.contains("AdmittedAda"));
        assert!(!full.contains("SecretWorldRow"));
        assert!(!full.contains("SecretNarrativeWorld"));
        assert!(!full.contains("SecretTimelineBeat"));
        assert!(!full.contains(WORLD_STATE_HEADING));
        assert!(!full.contains(TIMELINE_HEADING));
        let soul = full.find(CHARACTER_SOUL_HEADING).unwrap();
        let mem = full.find(CHARACTER_MEMORY_HEADING).unwrap();
        let l1 = full.find(CHARACTER_TOM_L1_HEADING).unwrap();
        let l2 = full.find(CHARACTER_TOM_L2_HEADING).unwrap();
        assert!(soul < mem && mem < l1 && l1 < l2);
    }

    #[tokio::test]
    async fn creator_actor_mode_renders_admitted_union_not_unrestricted_kb() {
        let stores = TestStores::new();
        let secret = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Item,
            "SecretWorldRow",
        );
        stores.kb.insert_knowledge_entry(secret).await.unwrap();
        let admitted = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Item,
            "AdmittedUnionRow",
        );
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_actor(MomentActorContext::creator_with_view(
                CharacterViewInput::from_entries(vec![admitted]),
            ));
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let full = ctx.to_full_context();
        assert!(full.contains("AdmittedUnionRow"));
        assert!(!full.contains("SecretWorldRow"));
        assert!(!full.contains(CHARACTER_SOUL_HEADING));
    }

    #[tokio::test]
    async fn actor_token_budget_truncates_oversized_admitted_content() {
        let stores = TestStores::new();
        let mut huge = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Item,
            "HugeAdmitted",
        );
        huge.body = Some(
            nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                summary: Some("X".repeat(8000)),
                ..Default::default()
            },
        );
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_max_tokens(20)
            .with_actor(MomentActorContext::character(
                CharacterViewInput::from_entries(vec![huge]),
            ));
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let full = ctx.to_full_context();
        assert!(full.chars().count() < 8000);
        assert!(full.contains(CHARACTER_SOUL_HEADING));
    }

    #[test]
    fn stage0_legacy_empty_section_omission_unchanged() {
        let output = Stage0Assembly {
            personality: String::new(),
            experience: String::new(),
            user_prompt: "Task.".to_string(),
            ..Stage0Assembly::default()
        }
        .assemble();
        assert!(!output.contains("## Personality"));
        assert!(!output.contains(CHARACTER_SOUL_HEADING));
        assert!(output.contains("Task."));
    }

    #[tokio::test]
    async fn assemble_moment_reserves_directive_slot_empty() {
        // invariant: P0 reserves the `moment.directive` slot but never fills
        // it — a full assembly leaves `moment_directive` None and renders no
        // `## Moment Directive` section (AC-I1b neutral-only promise).
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
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
        // note: the reserved slot's emit position — between `## Timeline` and
        // `## World Knowledge Base` (above lore, below system), the stable
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
        // note: an empty directive body renders nothing (guards P1 drift).
        let ctx = MomentContext {
            stage0_context: "stage0".to_string(),
            moment_directive: Some(String::new()),
            ..MomentContext::default()
        };
        assert!(!ctx.to_full_context().contains(MOMENT_DIRECTIVE_HEADING));
    }

    /// note: in-memory `DirectiveStore` stub — serves a fixed directive and
    /// records `after_injection` calls (`directive_id`, `event_id`, `work_id`).
    #[derive(Default)]
    struct TestDirectiveStore {
        active: Option<ActiveDirective>,
        // note: `after_injection` records (directive_id, event_id, work_id)
        // triples.
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

    // why: the test stub has no async I/O, but the trait contract is async
    // (clippy 1.98 `unused_async_trait_impl`).
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

    /// note: a directive with the given depth and a distinctive body.
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

    /// note: seeds a world and a timeline event so `world_state` and `timeline`
    /// are both present (the directive region has three interior positions).
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
        let ctx =
            assemble_moment_with_directive(&request, &stores.narrative, &stores.kb, &store).await;

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
            // note: head — directly below Stage-0 (nearest system), before
            // World State.
            (DirectiveDepth::Head, ("stage0", "directive", "world state")),
            // note: mid — between World State and Timeline.
            (
                DirectiveDepth::Mid,
                ("world state", "directive", "timeline"),
            ),
            // note: tail — between Timeline and World KB (nearest lore; P0's
            // reserved position).
            (DirectiveDepth::Tail, ("timeline", "directive", "world KB")),
        ] {
            let store = TestDirectiveStore::with_directive(active_directive(depth));
            let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
            let ctx =
                assemble_moment_with_directive(&request, &stores.narrative, &stores.kb, &store)
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
        // note: AC-I1b with a store present but empty — a `DirectiveStore` that
        // resolves nothing must not change the assembled bytes at all, and must
        // make no lifecycle call.
        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        let store = TestDirectiveStore::default();

        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_creator("ctr_1")
            .with_work("wrk_1")
            .with_event("evt_1");
        let plain = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let with_store =
            assemble_moment_with_directive(&request, &stores.narrative, &stores.kb, &store).await;

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
        // invariant: AC-I3 — the Moment Directive is product-local, so it must
        // never appear in the activation trace (the AssemblePacket
        // `activation_trace[]` source) nor in the World-KB text (the
        // `modules.placement[]` source).
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
        let ctx =
            assemble_moment_with_directive(&request, &stores.narrative, &stores.kb, &store).await;

        assert_eq!(
            ctx.moment_directive.as_deref(),
            Some("Keep the prose terse.")
        );
        // note: the trace is present (activation fired on "king") and carries
        // the entry, but never the directive body.
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
        // note: the directive body is not lore, so it is absent from the
        // World-KB text.
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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        assert!(ctx.stage0_context.contains("A creative writer."));
        assert!(ctx.stage0_context.contains("Write chapter 3."));
    }

    /// note: C2.2 — KB query respects `kb_limit`; a seeded multi-block store
    /// with limit 1 yields a single line.
    #[tokio::test]
    async fn kb_query_respects_limit() {
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

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(kb_text.contains("Hero"), "unlimited KB should contain Hero");
        assert!(
            kb_text.contains("Castle"),
            "unlimited KB should contain Castle"
        );

        // note: with limit 1 exactly one of the two blocks is returned.
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_kb_limit(1);
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.unwrap();
        let has_hero = kb_text.contains("Hero");
        let has_castle = kb_text.contains("Castle");
        assert!(
            has_hero ^ has_castle,
            "kb_limit=1 should return exactly one block, got: {kb_text}"
        );
    }

    /// note: C2.2 — KB query respects the `kb_text_search` filter.
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
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
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

    /// note: C2.3 — cross-domain truncation bounds total output.
    #[tokio::test]
    async fn cross_domain_truncation_respects_budget() {
        let stores = TestStores::new();

        let world = nexus_narrative::world::World::new(
            "wld_1",
            "ctr_test",
            "A very long world title that should be truncated",
            "test-world",
            nexus_contracts::Visibility::Private,
            nexus_contracts::TimePolicy::Manual,
        );
        stores.narrative.insert_world(world);

        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "Hero with a long description",
        );
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        let request_no_budget = MomentRequest::new(minimal_stage0()).with_world("wld_1");
        let ctx_full = assemble_moment(&request_no_budget, &stores.narrative, &stores.kb).await;
        let full_len = ctx_full.to_full_context().chars().count();

        let request_budget = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_max_tokens(50); // note: 50 tokens ≈ 200 chars at chars/4.
        let ctx_budget = assemble_moment(&request_budget, &stores.narrative, &stores.kb).await;
        let budget_len = ctx_budget.to_full_context().chars().count();

        assert!(
            budget_len <= full_len,
            "truncated output ({budget_len}) should not exceed full output ({full_len})"
        );

        // invariant: the personality section is never truncated, so it survives
        // any budget.
        assert!(
            ctx_budget.stage0_context.contains("A creative writer."),
            "personality must survive truncation"
        );
    }

    #[test]
    fn split_personality_uses_delimiter_path() {
        // note: Stage-0 assembly emits delimiters, so the split takes the
        // structured path.
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
            moment_directive: None,
            moment_directive_depth: DirectiveDepth::default(),
            activation_trace: None,
            slot_map: None,
            activation_budget: None,
            moment_directive_meta: None,
            hygiene_trace: None,
            character_mind: None,
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
        // note: full round-trip — assemble → to_full_context →
        // split_stage0_personality.
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
            moment_directive: None,
            moment_directive_depth: DirectiveDepth::default(),
            activation_trace: None,
            slot_map: None,
            activation_budget: None,
            moment_directive_meta: None,
            hygiene_trace: None,
            character_mind: None,
        };

        // note: `apply_cross_domain_truncation` uses
        // `split_stage0_personality` internally.
        ctx.apply_cross_domain_truncation(50);

        // invariant: the personality section survives truncation.
        assert!(
            ctx.stage0_context.contains("Creative soul."),
            "personality must survive truncation round-trip"
        );
    }

    #[test]
    fn split_personality_r13_scenario_no_false_split() {
        // note: R13 — a personality containing "## " sub-headers must not
        // cause a premature split; with delimiters the split is structural,
        // not heuristic.
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
        // note: content without delimiters takes the heuristic fallback.
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

    /// note: creates a `KnowledgeEntryRecord` with optional
    /// `modules.activation` JSON.
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
        // note: explicit OFF (off-switch semantics) — entries with activation
        // modules all appear, byte-identical to V1.146 flag-off behavior.
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
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
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
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.expect("world_kb must be present");

        // invariant: every candidate entry appears unchanged — the off-switch
        // performs no filtering.
        for name in ["Rules", "Style Note", "Open Lore", "Neutral"] {
            assert!(kb_text.contains(name), "off-switch: {name} must appear");
        }
        // invariant: the off-switch output is the flat block, with no slot
        // sub-headings.
        assert!(
            !kb_text.contains("### "),
            "off-switch must not render slot sub-headings, got: {kb_text}"
        );

        // why: the store read order is shared between the assembly and this
        // re-query (same `InMemoryKbStore` instance, no mutation in between),
        // so the expected string is deterministic and the comparison to the
        // V1.149 flat block is byte-exact.
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
        // note: flag ON but no entries carry activation modules gives the same
        // output as OFF.
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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.unwrap();
        assert!(kb_text.contains("Hero"));
        assert!(kb_text.contains("Castle"));
        assert!(kb_text.contains("Forest"));
    }

    #[tokio::test]
    async fn activation_default_on_filters_unmatched_entries() {
        // note: default-ON (no explicit flag) — stage0 mentions "king", so Hero
        // matches (key "king"), Castle is unmatched (key "dragon"), and Forest
        // is neutral.
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
        // note: no `with_activation_enabled` call — the default is ON since
        // V1.149.
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
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

    #[tokio::test]
    async fn hygiene_transform_applies_at_emission_and_stored_body_unchanged() {
        // invariant: the transform applies to the emitted `body.summary` text;
        // the stored World-KB body stays byte-identical (read-path only).
        let stores = TestStores::new();
        let mut kb =
            KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, "Hero");
        kb.entry_id = "kb_hygiene".to_string();
        kb.body = Some(
            nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                summary: Some("The hero fights the dragon".to_string()),
                attributes: Some(serde_json::json!({
                    "hygiene": [{ "pattern": "dragon", "replacement": "wyrm" }]
                })),
                ..Default::default()
            },
        );
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
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(
            kb_text.contains("wyrm"),
            "emitted summary must carry the transform"
        );
        assert!(
            !kb_text.contains("dragon"),
            "emitted summary must not carry the raw text"
        );

        let trace = ctx.hygiene_trace.expect("hygiene trace must be captured");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].entry_id, "kb_hygiene");
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);

        // invariant: the stored body stays byte-identical (read-path transform).
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
        // note: no carrier means no trace rows, and the assembly output is the
        // plain flat block (byte-identical to the no-hygiene path).
        let stores = TestStores::new();
        for (id, name) in [("kb_a", "Hero"), ("kb_b", "Castle")] {
            let mut kb =
                KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, name);
            kb.entry_id = id.to_string();
            kb.body = Some(
                nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                    summary: Some(format!("{name} summary")),
                    ..Default::default()
                },
            );
            stores.kb.insert_knowledge_entry(kb).await.unwrap();
        }

        let request = MomentRequest::new(minimal_stage0()).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(kb_text.contains("Hero summary"));
        assert!(kb_text.contains("Castle summary"));
        // note: the pass ran (activation-on path) but found no carrier, so the
        // trace is empty.
        let trace = ctx
            .hygiene_trace
            .expect("hygiene pass ran on the activation-on path");
        assert_eq!(trace, Vec::<crate::hygiene::HygieneTraceEntry>::new());
    }

    #[tokio::test]
    async fn hygiene_carrier_survives_edit_patch_round_trip() {
        // note: simulates `creator world kb edit --body` — a JSON round-trip
        // through `KnowledgeEntryBody` plus `update_knowledge_entry` (the
        // kb_edit code path). The `attributes.hygiene` carrier must survive the
        // store round-trip and drive the emission transform.
        let stores = TestStores::new();
        let mut kb =
            KnowledgeEntryRecord::new("wld_1", nexus_contracts::BlockType::Character, "Hero");
        kb.entry_id = "kb_hygiene".to_string();
        kb.body = Some(
            nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                summary: Some("The hero fights the dragon".to_string()),
                ..Default::default()
            },
        );
        kb.modules =
            Some(serde_json::json!({"activation": {"keys": ["hero"], "logic": "and_any"}}));
        stores.kb.insert_knowledge_entry(kb).await.unwrap();

        // note: the author patches the body with a hygiene carrier (kb_edit
        // flow).
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

        let re_read = stores.kb.get_knowledge_entry("kb_hygiene").await.unwrap();
        let attrs = re_read
            .body
            .expect("stored body")
            .attributes
            .expect("stored attributes");
        assert_eq!(attrs["hygiene"][0]["pattern"], "dragon");

        let stage0 = Stage0Assembly {
            personality: "A hero rises.".to_string(),
            experience: "10 years.".to_string(),
            user_prompt: "Write chapter 3.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.expect("world_kb must render");
        assert!(
            kb_text.contains("wyrm"),
            "patched carrier must drive the transform"
        );
    }

    #[tokio::test]
    async fn activation_default_on_and_all_requires_all_keys_with_secondary() {
        // note: `and_all` with `secondary_keys` — the entry needs ALL primary
        // AND ALL secondary keys to match (handbook truth table §2.1).
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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        assert!(ctx.world_kb.is_some(), "all 3 keys matched → entry present");
        assert!(ctx.world_kb.unwrap().contains("Royal Guard"));
    }

    #[tokio::test]
    async fn activation_default_on_not_any_excludes_when_secondary_matches() {
        // note: `not_any` with `secondary_keys` — the entry is excluded when a
        // secondary key matches (primary-any plus no secondary fires; spec
        // §2.1).
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
        // note: default-on (no explicit flag).
        let request = MomentRequest::new(stage0).with_world("wld_1");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        assert!(
            ctx.world_kb.is_none(),
            "Orc Warlord excluded by not_any (secondary 'army' matched) → no entries remain"
        );
    }

    #[tokio::test]
    async fn activation_empty_world_yields_none() {
        // note: default-on with no KB entries leaves `world_kb` None.
        let stores = TestStores::new();
        let request = MomentRequest::new(minimal_stage0()).with_world("wld_ghost");

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        assert!(ctx.world_kb.is_none());
    }

    #[tokio::test]
    async fn activation_timeline_participates_in_scan() {
        // note: the extended scan (V1.149 P0 T2) matches activation keys against
        // timeline outline-beat text (the event title) only — not stage0, not
        // the entry's own self-match text — so this entry fires via timeline
        // text alone.
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
        // note: a second entry whose key appears nowhere must be filtered.
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

        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
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

    /// note: seeds Harbor (fires on "king"), Dawn Dock (key "dragon" — no
    /// match) and Harbor Guild (key "elf" — no match); edges are
    /// Harbor→Dawn Dock→Guild.
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
        // note: Harbor fires on "king", so BFS pulls Dawn Dock (1 hop) and
        // Harbor Guild (2 hops) without keyword-tagging them (spec §5 product
        // story).
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land."))
            .with_world("wld_1")
            .with_hop_edges(hop_fixture_edges());
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

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
        // note: hop trace rows carry their hop fields.
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
        // note: no preloaded edges falls back to activation-only behavior (P0)
        // — neighbors stay filtered out.
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land.")).with_world("wld_1");
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        let kb_text = ctx.world_kb.expect("world_kb must be present");
        assert!(kb_text.contains("Harbor"), "primary hit present");
        assert!(!kb_text.contains("Dawn Dock"), "no edges ⇒ no hop pull");
        assert!(!kb_text.contains("Harbor Guild"), "no edges ⇒ no hop pull");
    }

    #[tokio::test]
    async fn activation_hops_off_switch_returns_all_entries_no_hop() {
        // invariant: the off-switch (V1.146 flag-off semantics) returns every
        // entry unchanged even with hop edges preloaded — nothing hops.
        let stores = TestStores::new();
        seed_hop_fixture(&stores).await;

        let request = MomentRequest::new(stage0_with("A king rules the land."))
            .with_world("wld_1")
            .with_activation_enabled(false)
            .with_hop_edges(hop_fixture_edges());
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

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
        // note: neutral-only golden under hops — no activation modules means no
        // seeds, so no hop pull, so `world_kb` is byte-identical with and
        // without edges (the neutral-only ship guarantee, spec §1, holds under
        // hops).
        //
        // why one store: `InMemoryKbStore` iterates a per-instance-seeded
        // `HashMap`, so two separately-seeded stores can return identical
        // entries in different orders (flaky test, fixed T3); reads are pure,
        // so sharing one store keeps the input order identical and the
        // byte-comparison meaningful.
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
        let ctx_plain = assemble_moment(&request_plain, &stores.narrative, &stores.kb).await;

        let request_edges = MomentRequest::new(stage0_with("Any text."))
            .with_world("wld_1")
            .with_hop_edges(hop_fixture_edges());
        let ctx_edges = assemble_moment(&request_edges, &stores.narrative, &stores.kb).await;

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
        let stage0 = stage0_with(&"P".repeat(200)); // note: section ≈ 219 chars.
        let request = MomentRequest::new(stage0.clone())
            .with_max_tokens(50) // note: max_chars 200 < personality section.
            .with_hop_max_tokens(1000);
        assert_eq!(
            hop_budget_tokens(&request, &stage0.assemble(), None, None),
            Some(0)
        );
    }

    #[test]
    fn hop_budget_caller_cap_bounds_the_remainder() {
        // note: the caller-provided hop cap is an upper bound on the computed
        // cross-domain remainder.
        let stage0 = stage0_with("small");
        let request = MomentRequest::new(stage0.clone())
            .with_max_tokens(1000) // note: remainder ≈ 994 tokens.
            .with_hop_max_tokens(7);
        assert_eq!(
            hop_budget_tokens(&request, &stage0.assemble(), None, None),
            Some(7)
        );
    }

    #[test]
    fn hop_budget_no_max_tokens_passes_through_caller_cap() {
        // note: without `max_tokens` there is no cross-domain remainder to
        // compute — the caller cap passes through unchanged (`None` stays
        // `None`: depth + cycle only, spec Q1).
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
        // note: formula (spec Q1) — `(max_tokens*4 − personality_section −
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
        let request = MomentRequest::new(stage0).with_max_tokens(100); // note: 400 chars.
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
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;

        // invariant: the personality section survives the tight budget untouched
        // — its content and heading are preserved, because the truncation
        // reconstruction keeps the section verbatim and only the delimiter
        // tokens live in the truncated remainder.
        assert!(ctx.stage0_context.contains("A king rules the land."));
        assert!(ctx.stage0_context.contains("## Personality"));

        // note: budget-gated pull — Dock (fits the remainder) is accepted via
        // hop, Guild (exceeds it) is not.
        let trace = ctx.activation_trace.expect("trace present");
        assert!(trace.iter().any(|t| t.entry_id == "kb_dock" && t.accepted));
        assert!(trace
            .iter()
            .any(|t| t.entry_id == "kb_guild" && !t.accepted));
        assert!(!trace.iter().any(|t| t.entry_id == "kb_guild" && t.accepted));
    }

    #[test]
    fn mind_input_applies_fixed_bounds_deterministically() {
        let long_soul = "x".repeat(CHARACTER_MIND_MAX_SOUL_CHARS + 500);
        let wide_soul = "é".repeat(CHARACTER_MIND_MAX_SOUL_CHARS + 3);
        let astral_soul = "🜂".repeat(CHARACTER_MIND_MAX_SOUL_CHARS + 3);
        let entries: Vec<String> = (0..(CHARACTER_MIND_MAX_MEMORY_ENTRIES + 7))
            .map(|i| {
                format!(
                    "entry {i} {}",
                    "y".repeat(CHARACTER_MIND_MAX_ENTRY_CHARS + 100)
                )
            })
            .collect();

        let mind = CharacterMindInput::new(Some(long_soul), entries);
        assert_eq!(
            mind.soul.as_ref().unwrap().chars().count(),
            CHARACTER_MIND_MAX_SOUL_CHARS,
            "truncation keeps the scalar bound (ellipsis reserved inside the cap)"
        );
        assert!(mind.soul.as_ref().unwrap().ends_with('…'));
        assert_eq!(
            mind.memory.len(),
            CHARACTER_MIND_MAX_MEMORY_ENTRIES,
            "entry cap applies"
        );
        let last = mind.memory.last().unwrap();
        assert_eq!(
            last.chars().count(),
            CHARACTER_MIND_MAX_ENTRY_CHARS,
            "each memory line is capped at the scalar bound"
        );
        assert!(last.ends_with('…'));

        // invariant: truncating é-heavy text hits the same scalar cap
        // (two-byte chars) and never splits a code point.
        let mind = CharacterMindInput::new(Some(wide_soul), vec![]);
        let wide = mind.soul.as_ref().unwrap();
        assert_eq!(wide.chars().count(), CHARACTER_MIND_MAX_SOUL_CHARS);
        assert!(wide.ends_with('…'));
        assert!(wide.is_char_boundary(wide.len()));

        // invariant: four-byte (astral) chars hit the same scalar cap and stay
        // valid UTF-8.
        let mind = CharacterMindInput::new(Some(astral_soul), vec![]);
        let astral = mind.soul.as_ref().unwrap();
        assert_eq!(astral.chars().count(), CHARACTER_MIND_MAX_SOUL_CHARS);
        assert!(astral.ends_with('…'));
        assert!(astral.is_char_boundary(astral.len()));

        // note: empty or `None` keeps `is_empty` true (legacy empty headings).
        let empty = CharacterMindInput::default();
        assert!(empty.is_empty());
        let filled = CharacterMindInput::new(Some("soul".to_string()), vec!["m1".to_string()]);
        assert!(!filled.is_empty());
    }

    #[test]
    fn render_mind_slots_bounded_and_ordered_with_empty_tom() {
        let mind = CharacterMindInput::new(
            Some("# Ava\nSOUL body".to_string()),
            vec!["- m1".to_string(), "- m2".to_string()],
        );
        let rendered = render_character_mind_headings(&mind);
        assert!(rendered.starts_with("## Character SOUL\n\n"));
        assert!(rendered.contains("# Ava\nSOUL body\n"));
        assert!(rendered.contains("## Character Memory\n\n- m1\n- m2\n"));
        assert!(rendered.contains("## Character ToM — L1\n\n## Character ToM — L2\n"));
        // note: order is SOUL, then Memory, then ToM, and no ToM content is
        // rendered.
        let soul_pos = rendered.find("## Character SOUL").unwrap();
        let mem_pos = rendered.find("## Character Memory").unwrap();
        let tom1 = rendered.find("## Character ToM — L1").unwrap();
        assert!(soul_pos < mem_pos && mem_pos < tom1);
        assert!(!rendered.contains("ToM — L1\n\nsome L1"));
        assert!(!rendered.contains("ToM — L2\n\nsome L2"));
    }

    #[test]
    fn render_mind_slots_fills_tom_l1_before_l2() {
        let mind = CharacterMindInput::new(None, vec![])
            .with_tom(vec!["- L1 line".to_string()], vec!["- L2 line".to_string()]);
        let rendered = render_character_mind_headings(&mind);
        assert!(rendered.contains(
            "## Character ToM — L1

- L1 line
"
        ));
        assert!(rendered.contains(
            "## Character ToM — L2

- L2 line
"
        ));
        let l1 = rendered.find("## Character ToM — L1").unwrap();
        let l2 = rendered.find("## Character ToM — L2").unwrap();
        assert!(l1 < l2);
    }

    #[tokio::test]
    async fn character_mind_slots_fill_only_when_projected() {
        let stores = TestStores::new();
        let allowed = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            "wld_1",
            nexus_contracts::BlockType::Character,
            "AdmittedAda",
        );
        // note: empty mind keeps the legacy P2 empty headings, with no
        // SOUL/Memory bodies.
        let request = MomentRequest::new(Stage0Assembly {
            user_prompt: "Act.".to_string(),
            personality: "Creator SOUL must not leak.".to_string(),
            ..Stage0Assembly::default()
        })
        .with_world("wld_1")
        .with_actor(MomentActorContext::character(
            CharacterViewInput::from_entries(vec![allowed.clone()]),
        ));
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let empty_full = ctx.to_full_context();
        assert!(empty_full.contains("## Character SOUL\n\n## Character Memory\n\n"));

        // note: a projected mind emits the SOUL and Memory bodies; ToM stays
        // empty.
        let projected = CharacterMindInput::new(
            Some("# Ava\nSOUL".to_string()),
            vec![
                "- SHAREDMEMORYMARKER".to_string(),
                "- LOCALMEMORYMARKER".to_string(),
            ],
        );
        let request = MomentRequest::new(Stage0Assembly {
            user_prompt: "Act.".to_string(),
            personality: "Creator SOUL must not leak.".to_string(),
            ..Stage0Assembly::default()
        })
        .with_world("wld_1")
        .with_actor(MomentActorContext::character_with_mind(
            CharacterViewInput::from_entries(vec![allowed]),
            projected,
        ));
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let full = ctx.to_full_context();
        assert!(full.contains("# Ava\nSOUL\n"));
        assert!(full.contains("- SHAREDMEMORYMARKER\n- LOCALMEMORYMARKER\n"));
        assert!(full.contains("## Character ToM — L1\n\n## Character ToM — L2\n"));
    }

    /// note: the container matrix over one store — a Character-global shared
    /// World row, this Character's own Character-owned row and binding-local
    /// row are each visible because the admitted snapshot carries them; another
    /// holder's World-owned `owner-private` row is admitted by nobody here and
    /// is never read out of the store.
    #[tokio::test]
    async fn v1191_holder_context_matrix_shared_character_binding_and_private() {
        use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;

        let shared_world = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "SharedHarbor");
        let own_character = KnowledgeEntryRecord::for_character(
            "chr_ada",
            BlockType::Character,
            "CharacterOwnFact",
        );
        let binding_local =
            KnowledgeEntryRecord::for_binding("bnd_ada", BlockType::Scene, "BindingLocalDock");
        let unadmitted_character = KnowledgeEntryRecord::for_character(
            "chr_bea",
            BlockType::Character,
            "OtherCharacterRow",
        );
        let mut other_holder_private =
            KnowledgeEntryRecord::new("wld_1", BlockType::Character, "OtherHolderPrivateRow");
        other_holder_private.holder_entry_id = Some("hld_other".to_string());
        other_holder_private.disclosure =
            Some(nexus_knowledge::world_kb::DISCLOSURE_OWNER_PRIVATE.to_string());

        for row in [
            shared_world.clone(),
            own_character.clone(),
            binding_local.clone(),
            unadmitted_character,
            other_holder_private,
        ] {
            stores.kb.insert_knowledge_entry(row).await.unwrap();
        }

        // note: the admitted Character ActorView snapshot — World shared rows
        // plus this Character's own container and its binding's container rows.
        let request = MomentRequest::new(minimal_stage0())
            .with_world("wld_1")
            .with_actor(MomentActorContext::character(
                CharacterViewInput::from_entries(vec![shared_world, own_character, binding_local]),
            ));
        let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
        let kb_text = ctx.world_kb.expect("admitted snapshot must render");

        assert!(
            kb_text.contains("SharedHarbor"),
            "Character-global shared row stays visible: {kb_text}"
        );
        assert!(
            kb_text.contains("CharacterOwnFact"),
            "own container row: {kb_text}"
        );
        assert!(
            kb_text.contains("BindingLocalDock"),
            "binding-local row: {kb_text}"
        );
        assert!(
            !kb_text.contains("OtherCharacterRow"),
            "another Character's container is never read from the store: {kb_text}"
        );
        assert!(
            !kb_text.contains("OtherHolderPrivateRow"),
            "another holder's private row is never read from the store: {kb_text}"
        );
    }

    /// note: fail-closed — an Actor with an empty admitted snapshot emits no
    /// World-KB section even though the store it was handed holds rows;
    /// "missing/incomplete snapshot" never widens into a World query.
    #[tokio::test]
    async fn v1191_holder_context_empty_snapshot_fails_closed() {
        use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

        let stores = TestStores::new();
        seed_world_and_timeline(&stores).await;
        for name in ["StoreRowOne", "StoreRowTwo"] {
            stores
                .kb
                .insert_knowledge_entry(KnowledgeEntryRecord::new(
                    "wld_1",
                    BlockType::Character,
                    name,
                ))
                .await
                .unwrap();
        }

        for actor in [
            MomentActorContext::character(CharacterViewInput::from_entries(Vec::new())),
            MomentActorContext::creator_with_view(CharacterViewInput::from_entries(Vec::new())),
        ] {
            let request = MomentRequest::new(minimal_stage0())
                .with_world("wld_1")
                .with_actor(actor);
            let ctx = assemble_moment(&request, &stores.narrative, &stores.kb).await;
            assert!(
                ctx.world_kb.is_none(),
                "an empty snapshot must not fall back to the store's rows"
            );
            let full = ctx.to_full_context();
            assert!(!full.contains("StoreRowOne") && !full.contains("StoreRowTwo"));
        }
    }
}
