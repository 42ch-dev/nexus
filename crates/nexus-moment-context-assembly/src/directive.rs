//! Moment Directive provider boundary (V1.150 P1, DF-75 — spec
//! `fl-l-w5-prompt-control-plane.md` §3).
//!
//! MCA renders the active directive into the reserved `moment.directive`
//! slot and triggers its lifecycle; the persistence adapter lives at the
//! composition root (`nexus42`) over `nexus-local-db`. The [`DirectiveStore`]
//! trait keeps this crate free of a `nexus-local-db` dependency — the same
//! provider pattern as [`NarrativeGateway`](nexus_narrative::NarrativeGateway)
//! / [`KbStore`](nexus_knowledge::world_kb::KbStore).
//!
//! # Product-local only (AC-I3)
//!
//! The Moment Directive is **never** on the spoke wire: not a `modules.*`
//! object, not a `KnowledgeEntry`, never in `AssemblePacket` `placement[]` /
//! `activation_trace[]`, never in any pack export/import path. It is prompt
//! control that exists only because an author wrote it (spec §3.4).

/// Insert depth of the Moment Directive **within the directive region** — the
/// band between the Stage-0 block (system/personality, above) and the
/// `## World Knowledge Base` section (lore, below).
///
/// The depth cannot move the directive below lore or above system (plan
/// Scope; spec §1.2 / §3.3). Position within the region (top → bottom):
/// - `head` — nearest system/personality (directly below Stage-0, above
///   `## World State`)
/// - `mid` — between `## World State` and `## Timeline`
/// - `tail` — nearest lore (between `## Timeline` and
///   `## World Knowledge Base`) — **P0's reserved position** (guide
///   `mca-section-audit.md` Q1), and the default so P0's slot-layout tests
///   stay byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirectiveDepth {
    /// Nearest system/personality.
    Head,
    /// Between the region's middle sections.
    Mid,
    /// Nearest lore — P0's reserved position (default).
    #[default]
    Tail,
}

impl DirectiveDepth {
    /// Stable string form (also the persisted `insert_depth` column value).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Mid => "mid",
            Self::Tail => "tail",
        }
    }

    /// Parse the persisted string form; `None` for unknown values (a corrupt
    /// row must never inject — the adapter skips it).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "head" => Some(Self::Head),
            "mid" => Some(Self::Mid),
            "tail" => Some(Self::Tail),
            _ => None,
        }
    }
}

/// TTL kind of a Moment Directive (spec §3.1 / §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveTtlKind {
    /// Count-down by 1 on every injecting `assemble_moment`.
    Generations,
    /// Count-down by the number of chapter advances since the last injecting
    /// assemble (R-V1150P2-004: the delta between the previously observed
    /// `works.current_chapter` and the current one, per (directive, work) —
    /// R-V1150P2-008); treated identically to `generations` for essay /
    /// game-bible / script / worldless Works (documented fallback, spec §3.3).
    Chapters,
}

impl DirectiveTtlKind {
    /// Stable string form (also the persisted `ttl_kind` column value).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generations => "generations",
            Self::Chapters => "chapters",
        }
    }

    /// Parse the persisted string form; `None` for unknown values.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "generations" => Some(Self::Generations),
            "chapters" => Some(Self::Chapters),
            _ => None,
        }
    }
}

/// The active Moment Directive as MCA renders it (spec §3.1). This is the
/// full product payload MCA ever sees — it never reaches `modules.*`, a
/// `KnowledgeEntry`, or an `AssemblePacket` trace (AC-I3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveDirective {
    /// Stable directive id (lifecycle handle).
    pub directive_id: String,
    /// Author instruction text (non-empty after trim — validated at write).
    ///
    /// No size cap by design — consistent with the personality section; the
    /// MCA token budget governs overall context, not per-section caps
    /// (R-V1150P2-006 accepted).
    pub body: String,
    /// Placement within the directive region.
    pub insert_depth: DirectiveDepth,
    /// TTL kind (`generations` | `chapters`).
    pub ttl_kind: DirectiveTtlKind,
    /// Clear when the focused moment anchor changes between assembles.
    pub clear_on_scene_change: bool,
    /// Remaining TTL count as persisted at load — the post-injection
    /// decrement lands in the DB *after* this payload is captured, so the
    /// inspector packet shows the pre-decrement value (V1.151 P0, DF-76
    /// spec §2 H6 — surfaced from `MomentDirectiveRow.ttl_remaining`;
    /// `None` when no TTL is tracked). Active rows always have a value.
    /// `u64` matches the wire input width so counts above `u32::MAX`
    /// render instead of nulling (QC3-S-1).
    pub ttl_remaining: Option<u64>,
    /// `active` | `expired` (V1.151 P0 H6) — only active rows inject, so
    /// this is `"active"` on every payload MCA ever renders.
    pub status: String,
    /// `work` | `world` (V1.151 P0 H6 — `scope_kind` column).
    pub scope_kind: String,
    /// Work id (`scope_kind` = `work`) or world id (`scope_kind` = `world`)
    /// (V1.151 P0 H6 — `scope_id` column).
    pub scope_id: String,
}

/// Scope pair of a Moment Directive (`work` | `world` + the scoped id).
///
/// The metadata surface of the inspector packet's `moment_directive`
/// section (V1.151 P0, DF-76 spec §2 H6). Status-only: never the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MomentDirectiveScope {
    /// `work` | `world` (`scope_kind` column).
    pub kind: String,
    /// Work id / world id (`scope_id` column).
    pub id: String,
}

/// Status-only metadata of the active Moment Directive.
///
/// The source of the inspector packet's `moment_directive` section
/// (V1.151 P0, DF-76 spec §2 H6). **NEVER carries the directive body** —
/// body exclusion is by construction: the packet builder reads only this
/// metadata, never `MomentContext::moment_directive` (AC-I3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MomentDirectiveStatus {
    /// Scope kind + scoped id.
    pub scope: MomentDirectiveScope,
    /// Placement within the directive region (`head` | `mid` | `tail`).
    pub insert_depth: DirectiveDepth,
    /// TTL kind (`generations` | `chapters`).
    pub ttl_kind: DirectiveTtlKind,
    /// Remaining TTL count as persisted at load (pre-decrement; the
    /// lifecycle write happens after the packet is built).
    pub ttl_remaining: Option<u64>,
    /// Clear when the focused moment anchor changes between assembles.
    pub clear_on_scene_change: bool,
    /// `"active"` when the directive injected this assembly.
    pub status: String,
}

impl From<&ActiveDirective> for MomentDirectiveStatus {
    fn from(d: &ActiveDirective) -> Self {
        Self {
            scope: MomentDirectiveScope {
                kind: d.scope_kind.clone(),
                id: d.scope_id.clone(),
            },
            insert_depth: d.insert_depth,
            ttl_kind: d.ttl_kind,
            ttl_remaining: d.ttl_remaining,
            clear_on_scene_change: d.clear_on_scene_change,
            status: d.status.clone(),
        }
    }
}

/// Provider boundary for the directive persistence adapter.
///
/// Implemented at the composition root (`nexus42`'s `LocalDirectiveStore`)
/// over `nexus-local-db`; in-memory stubs in tests. All failures degrade to
/// "no directive" / "no lifecycle write" — a broken directive store must
/// never fail the whole assembly (consistent with `assemble_moment`'s
/// per-section degradation).
///
/// The trait uses `#[allow(async_fn_in_trait)]` consistent with
/// `nexus-narrative::NarrativeGateway` (auto-trait bounds not needed — the
/// provider is used monomorphically at the composition root).
#[allow(async_fn_in_trait)]
pub trait DirectiveStore {
    /// Resolve + load the active directive for a work-bound moment (scope
    /// resolution, spec §3.2): a Work-scoped directive wins; else the World
    /// override when the Work binds a World; else none.
    ///
    /// Returns `None` when no directive is in scope (or when the store
    /// cannot resolve one — the caller renders nothing, preserving the
    /// neutral-only byte-equivalence promise, AC-I1b).
    async fn load_active(
        &self,
        creator_id: Option<&str>,
        work_id: Option<&str>,
        world_id: Option<&str>,
    ) -> Option<ActiveDirective>;

    /// Post-injection lifecycle (spec §3.3) — TTL decrement / chapter-advance
    /// / scene-clear bookkeeping. Called only after a directive was actually
    /// injected. Best-effort: failures are logged by the adapter, never
    /// surfaced as assembly errors.
    async fn after_injection(
        &self,
        directive_id: &str,
        event_id: Option<&str>,
        work_id: Option<&str>,
    );
}

/// No-op store used by the plain [`crate::assemble_moment`] entry point.
///
/// This is what guarantees the neutral-only byte-equivalence promise
/// (AC-I1b): without a store, no directive can ever load, so nothing renders
/// and nothing decrements — the assembly is byte-identical to V1.149 / P0.
#[derive(Debug, Default)]
pub struct NoDirectiveStore;

// `unused_async_trait_impl` (new in clippy 1.98): the no-op methods perform no
// async I/O; `async` is by `DirectiveStore` trait contract — toolchain-drift debt.
#[allow(clippy::unused_async_trait_impl)]
impl DirectiveStore for NoDirectiveStore {
    async fn load_active(
        &self,
        _creator_id: Option<&str>,
        _work_id: Option<&str>,
        _world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        None
    }

    async fn after_injection(
        &self,
        _directive_id: &str,
        _event_id: Option<&str>,
        _work_id: Option<&str>,
    ) {
    }
}
