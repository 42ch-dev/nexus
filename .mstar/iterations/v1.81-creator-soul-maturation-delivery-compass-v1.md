---
iteration_id: V1.81
start_date: 2026-07-01
status: completed
end_date: 2026-07-02
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-01-v1.81-prepare-spec-and-contracts
  - 2026-07-01-v1.81-creator-soul-narrative-and-world-foundation
  - 2026-07-01-v1.81-soul-surface-deepening
  - 2026-07-01-v1.81-closure
---

# V1.81 — Creator SOUL Maturation — Delivery Compass v1

**Status**: locked (§1.6 Review & Edit chain complete — product-manager → architect → writing-specialist sequential edits landed; PM lock applied 2026-07-01).
**Author**: `@project-manager` (compass + grill-me locked scope).

## 0. Context — returning to the reflection axis after V1.80 stabilization

V1.80 (PR #107, 2026-07-01) was the first "stabilize before extending"
reliability investment iteration after four consecutive feature iterations
(V1.76–V1.79). It closed REL-01 (`R-V178P0-QC3-003`) and four low V1.79-QC
frontend hygiene residuals, leaving a clean foundation: 2 open residuals
(both low, both V1.80-P0 accepted documentation-only).

V1.81 returns to the feature cadence and deepens the **reflection axis** that
V1.79 opened. V1.79 Track B shipped the SOUL personality visualization (keyword
clusters + temporal drift) as a pure read-side surface over `memory_fragments`.
The retrospective flagged four SOUL backlog candidates (BL-09/10/11/12). V1.81
picks **BL-12 (SOUL viz refinements) + BL-10 (independent growth-curve view)**,
combined under a single product theme — **Creator SOUL Maturation** — with the
**LLM Creator-SOUL personality narrative** as the headline feature.

### Product model (grill-me locked)

A core product distinction shapes this iteration (user-locked):

- **Creator SOUL** is the creator's core creative identity — **world-agnostic**.
  It is the *whole*: all accumulated `memory_fragments`. The LLM personality
  narrative synthesizes this whole into a reflective "who you are becoming"
  statement.
- **Per-World SOUL projection** is the Creator SOUL's *inclination within a
  specific world* — a **subset** of fragments filtered by the world they
  emerged from. It is a drill-down view, not a separate identity.

This maps cleanly onto the data model: a fragment carries a nullable
`world_id` recording which world it emerged from. Creator SOUL = all fragments
(whole); World projection = `WHERE world_id = X` (subset); Creator-core-only =
`WHERE world_id IS NULL`. The LLM narrative operates on the whole and is
world-agnostic by definition; the world projection only filters the existing
keyword/drift/growth viz (no per-world narrative this iteration).

### Data-model finding (grill-me verified against code)

`memory_pending_review` already carries a nullable `world_id` (pending captures
record their world context). `memory_fragments` does **not** — the world
context is dropped at promotion time. The fix is additive and clean: the
promotion seam (`create_fragment_from_review(input)` where `PendingReviewInput`
already exposes `world_id: Option<&str>`, and the `SessionDigestSummarizer`
trait already passes `world_id: Option<&str>`) has the world context in hand;
it is simply not threaded into the fragment row today. See §10.

## 1. grill-me locked decisions

| Decision | Resolution |
|---|---|
| Main direction | **Creator SOUL Maturation** — combine BL-12 (SOUL viz refinements) + BL-10 (growth-curve) under one theme, with the LLM personality narrative as headline. (BL-09 standalone dashboard + BL-11 deeper reading remain backlog.) |
| ④ LLM narrative scope | **Headline feature, in-scope.** Synthesize a world-agnostic Creator-SOUL narrative from accumulated fragments via an agent (mirroring the `SessionDigestSummarizer` trait seam). |
| ④ invocation model | **On-demand generation + stale invalidation.** Author triggers "Reflect on my SOUL"; the narrative is persisted (cached); when fragments have arrived since the last generation it is marked stale and the UI prompts to re-reflect. Consistent with V1.80 bounded/explicit-trigger discipline — no background LLM jobs. |
| ④ scope level | **Creator-level only** (world-agnostic). Per-World narratives are deferred; the world projection only filters existing read-side viz. |
| ① per-World projection | **Data foundation + UI selector, both this iteration.** `memory_fragments.world_id` migration + threading at the promotion seam + additive DTO + DAO filter + a world-selector control that drills the keyword/drift/growth viz into a world subset. Default view is Creator SOUL (whole). |
| ② growth-curve (BL-10) | **Ship.** New cumulative-fragment-growth viz component, separate from the temporal-drift timeline (V1.79 folded only a count into the timeline). Respects the world projection. |
| ③ auto-refresh | **Ship.** Poll interval on the SOUL query + post-review invalidation so the viz refreshes without manual reload. |
| Wire contracts | **Additive.** `memory-fragment-info` gains optional `world_id`; new soul-narrative request/response schemas (on-demand reflect + stale flag). `@42ch/nexus-contracts` 0.15.0 → 0.16.0. |
| Plan structure | P-1 (Prepare: specs + wire contracts + DESIGN.md) → P0 (backend) ‖ P1 (frontend) parallel worktree → P-last (closure). 4 plans. Mirrors V1.79 cadence. |
| Branch policy | Integration `iteration/v1.81` from `main` (`83000ca3`); per-plan topic `feature/v1.81-<slug>`; P0‖P1 parallel → worktree isolation required (per `.mstar/AGENTS.md` two-tier branch model). PR target `main`. |

## 2. Scope

This iteration locks one headline spec point and three companion spec points, all
deepening the already-shipped SOUL surface (read-side consumption + one new
on-demand synthesis path). Each spec point delivers a discrete, verifiable
author-facing outcome — not just a technical capability:

- **SP-1 (Headline / ④ — Creator-SOUL Narrative)**: The author can read a
  reflective narrative synthesis of their accumulated creative identity — "who
  you are becoming" — generated on-demand by an LLM over the whole Creator SOUL
  (all fragments, world-agnostic). The narrative is persisted (cached) and shown
  on a new narrative card in the SOUL surface. When new fragments arrive after
  the last generation, the card is marked stale with a prompt to re-reflect.
  Under a minimum-fragment threshold, the card shows a graceful "not enough SOUL
  yet" empty state rather than a thin/generic narrative. New synthesis path
  mirroring the `SessionDigestSummarizer` trait seam + a daemon endpoint +
  persistence + the frontend narrative card.
- **SP-2 (① — per-World Projection)**: Add nullable `world_id` to
  `memory_fragments` (threaded at the promotion seam), expose it on the
  `memory-fragment-info` wire DTO, add a DAO world filter, and ship a UI
  world-selector that drills the keyword/drift/growth viz into a world subset
  (Creator SOUL = whole is the default). The selector shows an honest empty state
  when the selected world has no fragments. The product model is user-visible:
  "All worlds" = the whole Creator SOUL; a specific world = a subset projection.
- **SP-3 (② — Growth-curve, BL-10)**: A new cumulative-fragment-growth viz
  component, independent of the temporal-drift timeline, so the author can see
  their SOUL accumulating over time at a glance. Respects the world projection.
  Degrades gracefully for new creators (count + explanatory empty state).
- **SP-4 (③ — Auto-refresh)**: SOUL query poll interval + post-review
  invalidation so the viz refreshes without a manual reload after a review
  session.

### 2.1 User stories (author persona)

Each story maps to a verifiable acceptance criterion in §6. The product model
(narrative = whole Creator SOUL, world-agnostic; projection = subset) is
user-visible in the UI, not just a backend concept.

**SP-1 — Creator-SOUL Narrative (headline)**

- *(A) Discovery*: As an author with accumulated fragments, I can trigger
  "Reflect on my SOUL" and read a coherent narrative synthesis of my creative
  identity — themes, shifts, and preoccupations drawn from my fragments — so I
  can see my creative self as a whole, not just a scatter of keywords. → Maps
  to AC "Narrative generation + quality threshold."
- *(B) Stale awareness*: As an author returning after new review sessions, the
  narrative card shows a stale banner ("new fragments since you last reflected
  — re-reflect") with a clear action to re-trigger synthesis, so the narrative
  stays a living reflection rather than a frozen snapshot. → Maps to AC
  "Stale invalidation."
- *(C) Graceful empty state*: As a new author with fewer than the minimum
  fragment threshold, I see a forward-looking empty state explaining what the
  narrative will show once I have written and reviewed more, rather than a thin
  or generic LLM output. → Maps to AC "Insufficient-data gate."

**SP-2/3/4 — Projection, growth, refresh**

- *(D) Whole vs subset*: As an author who writes across multiple worlds, I can
  drill from my Creator SOUL (the whole, "All worlds" default) into a specific
  world's projection to see how my themes manifest within that world's fragment
  subset, then return to the whole — and the UI makes clear a world projection
  is a subset, not a separate identity. → Maps to AC "World projection + empty
  states."
- *(E) Accumulation at a glance*: As an author, I can see my SOUL growing over
  time as a cumulative curve, so the accumulation itself is visible separately
  from how the theme *mix* drifts in the temporal view. A new creator sees a
  forward-looking empty state rather than a broken chart. → Maps to AC
  "Growth-curve + density branching."
- *(F) No manual reload*: As an author finishing a review session, the SOUL
  surface refreshes on its own so I see the new fragments without reloading. →
  Maps to AC "Auto-refresh."

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-01-v1.81-prepare-spec-and-contracts` | P-1 — Prepare (specs + wire contracts + codegen + DESIGN.md) | Done | 4 schemas landed (2 edit + 2 new) + codegen + 0.15.0→0.16.0 + drift clean + DESIGN.md 2 tokens + §1.6 specs verified. Merge `095c6933`. P0/P1 consume frozen generated types. |
| `2026-07-01-v1.81-creator-soul-narrative-and-world-foundation` | P0 — Backend: Creator-SOUL Narrative + per-World foundation | Done | QC 3/3 Approve (after 3 fix-wave rounds: world_id propagation test, bounded→fingerprint-cached sound distinct-keyword count, UTF-8 char-truncation). Wire additive `memory-fragment-info`+`world_id`; new soul-narrative schemas; `@42ch/nexus-contracts` 0.16.0. 4 low deferred suggestions → V1.82+. |
| `2026-07-01-v1.81-soul-surface-deepening` | P1 — Frontend: SOUL Surface Deepening (narrative card + world selector + growth-curve + auto-refresh) | Done | QC 3/3 Approve. Narrative card (5 states) + world-selector + growth-curve + auto-refresh, consuming P-1 frozen TS contracts. No wire change. World-title resolution deferred (no worlds endpoint). |
| `2026-07-01-v1.81-closure` | P-last — Closure (compound + 2 V1.80 low doc residuals + compaction + PR + trackers/STRATEGY) | Done | Closed 2 V1.80-P0 doc residuals (mutex-map lifecycle + best-effort delete threat-model docs); Profile B compaction (4 plans archived, plans-done.json 370); tracker/STRATEGY/README updates folded into Phase 3 §3.3. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

**Parallelism**: P0 and P1 are independent by default once P-1 locks the wire
contracts. They must remain file-disjoint:

- **P0** touches `crates/nexus-local-db/` (migration + `memory_fragment.rs` +
  `.sqlx/`), the 3 `create_fragment` call sites
  (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs`,
  `crates/nexus-orchestration/src/capability/builtins/creator.rs`),
  `crates/nexus-creator-memory/` (narrative synthesis seam),
  `schemas/local-api/memory/` (fragment-info + new soul-narrative schemas),
  generated contract output, and `packages/nexus-contracts/package.json`.
- **P1** touches `apps/web/src/components/soul/*`, `apps/web/src/components/memory/*`,
  `apps/web/src/pages/memory-page.tsx`, and `apps/web/src/api/queries.ts`
  (new narrative query/mutation + world-filter query param).

The shared client surface (`apps/web/src/api/queries.ts`, `apps/web/src/lib/nexus/types.ts`)
is the single collision risk and is integration-sequenced: P0 lands the backend +
contracts; P1 consumes the frozen contract types (typed against P-1 output) and
may mock the endpoint until P0 merges, then integrates on `iteration/v1.81`. P0
must not touch `apps/web/`; P1 must not touch `crates/`, `schemas/`, or generated
output. P-last runs sequentially after P0 + P1 merge.

## 4. Branch policy

Per `.mstar/AGENTS.md` two-tier branch model:

| Tier | Branch | Purpose |
|---|---|---|
| Integration | `iteration/v1.81` (from `main` @ `83000ca3`) | Single line where all plan work lands before QC/QA; QC/QA `Working branch`. |
| Final landing | `main` | `iteration/v1.81` merges here via PR after iteration sign-off. |
| P-1 topic | `feature/v1.81-prepare` | Spec/DESIGN.md/contract amendments only. |
| P0 topic | `feature/v1.81-soul-backend` | Backend commits only (crates/* + schemas + generated output). |
| P1 topic | `feature/v1.81-soul-frontend` | Frontend commits only (apps/web soul/memory). |
| P-last topic | `feature/v1.81-closure` | Closure commits only. |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers).
P-1 → P0/P1: P0 and P1 may dispatch after P-1 locks contracts; P0 and P1 run in
separate worktrees and merge into `iteration/v1.81`. P0 must not touch `apps/web/`;
P1 must not touch `crates/`, `schemas/`, or generated contract output. Shared web
client files, if unavoidable, are additive-only and resolved on `iteration/v1.81`
after topic-branch merge, not by broad refactors in either track. P-last sequential
after P0 + P1 merge.

## 5. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass locked (§1.6 Review & Edit chain complete) | 2026-07-01 | pending |
| P-1 Prepare complete (specify/clarify/plan both tracks; contracts frozen) | 2026-07-01 | pending |
| P0 + P1 dev complete (parallel) | 2026-07-02 | pending |
| QC tri-review complete (both tracks) | 2026-07-02 | pending |
| QA Pass | 2026-07-02 | pending |
| Iteration close (P-last + PR) | 2026-07-02 | pending |

## 6. Acceptance Criteria

Each criterion maps to a user story in §2.1 (story labels in parentheses).

### Product verification (author-facing — what a user experiences)

- **AC-1 — Narrative generation + quality threshold (stories A, C)**:
  An author with accumulated fragments triggers "Reflect on my SOUL" and
  receives a coherent narrative synthesis of their creative identity — not a
  keyword dump, but a reflective statement of themes, shifts, and
  preoccupations drawn from their fragments.

  **Narrative quality threshold**: A narrative is "good enough" (not thin) when
  it satisfies all three of:
  1. **Specificity** — references at least two distinct theme keywords from
     the creator's fragment keyword clusters (not generic praise like "creative
     writer").
  2. **Temporality** — references at least one shift or development over time
     ("you began with X, then moved toward Y") drawn from the temporal drift
     signal.
  3. **Actionable tone** — ends with a forward-looking reflection or question
     ("your next work might explore…"), not a summary sign-off.

  The architect defines the prompt contract to enforce this structure; the
  product gate is that a QA session with ≥3 diverse real creator profiles
  (not test fixtures) produces narratives the author recognizes as coherent
  and non-generic. The insufficient-data gate (§2.1 story C) ensures creators
  below the minimum fragment threshold never receive a thin narrative at all.

- **AC-2 — Stale invalidation (story B)**:
  The narrative is persisted; returning to the surface shows the cached
  narrative without regenerating. When new fragments have arrived since the
  last generation, the surface clearly marks the narrative stale and offers
  to re-reflect.

- **AC-3 — Insufficient-data gate (story C)**:
  A creator with fewer than the minimum fragment threshold (architect-defined;
  recommendation: N < 10 fragments or total keyword count < 20) sees the
  graceful "not enough SOUL yet" empty state, not a thin or generic narrative.
  The empty state is encouraging and explains what will populate it.

- **AC-4 — World projection + empty states (story D)**:
  The SOUL surface defaults to Creator SOUL ("All worlds"). A world selector
  lets the author drill into a world projection; the keyword clusters, temporal
  drift, and growth-curve all re-scope to that world's fragment subset.
  Returning to "All worlds" restores the whole. The projection is honest about
  being a subset:
  - A world with no fragments shows "No fragments in this world yet — your
    fragments from other worlds still shape your Creator SOUL" (not a broken
    chart).
  - A world the creator has never written in is omitted from the selector list
    (no empty dead-end options).

- **AC-5 — Growth-curve + density branching (story E)**:
  A cumulative-fragment-growth curve is visible, distinct from the
  temporal-drift timeline. It communicates "my SOUL is growing" at a glance
  and respects the world projection. New creators see a forward-looking empty
  state (not a broken chart), matching the V1.79 density-branching pattern
  (`empty`/`low-data`/`rich`).

- **AC-6 — Auto-refresh (story F)**:
  After a review session, the SOUL viz refreshes without a manual reload
  (poll + post-review invalidation).

### Technical verification

- Wire additive: `memory-fragment-info` exposes optional `world_id`; new
  soul-narrative request/response schemas land; round-trip regression test
  green; `@42ch/nexus-contracts` 0.15.0 → 0.16.0; `pnpm run codegen` +
  `cargo build` + `validate-schemas` clean.
- `memory_fragments.world_id` migration is additive (nullable column); existing
  rows default to NULL (Creator-core-only); `cargo sqlx prepare --workspace`
  committed.
- The promotion seam threads `world_id` from `PendingReviewInput` into the
  fragment at all `create_fragment` sites; a regression test asserts a
  world-scoped pending review produces a fragment carrying that `world_id`.
- The narrative synthesis reuses the established summarizer-trait seam (no
  second agent-invocation pattern): `SoulNarrativeSynthesizer` lives in
  `nexus-creator-memory`, while the daemon adapter calls the existing
  production ACP prompt path through `CapabilityRegistry`/`acp.prompt` (the
  grep-confirmed `SessionDigestSummarizer` production impl is passthrough-only).
  On-demand generation only (no background LLM job — consistent with V1.80);
  stale-invalidation keys off fragment count / max `created_at` snapshots.
- QC tri-review 3/3 Approve (both tracks); QA Pass; `cargo clippy --all --
  -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` +
  `validate-schemas` clean; web typecheck/test green.
- `status.json` coherent (Profile B: only non-Done plans in `plans[]`; P-last
  compaction done).

## 7. Non-Goals

- **Per-World LLM narratives** — ④ operates on the Creator whole only; a
  per-world narrative synthesis is deferred (the world projection only filters
  read-side viz).
- **Narrative editing / curation by the author** — the narrative is a
  read-only synthesis; the author cannot edit, rewrite, or curate the
  narrative text. The "stale → re-reflect" cycle is the only author influence
  mechanism (invalidate and regenerate). This preserves the narrative as an
  honest reflection, not a co-authored document. Editing/curation is a
  candidate for future reflection-axis deepening if authors request it.
- **Narrative export / share** — the narrative is local-only and displayed
  in-app; there is no copy-to-clipboard, export, or share path in V1.81.
  Future product decisions (copy for prompt context, export as artifact, share
  to platform) are not precluded but are explicitly out of scope.
- **Async background-job infrastructure** for narrative generation — rejected
  (consistent with V1.80); on-demand only.
- **BL-09 standalone maturation dashboard** — remains backlog.
- **BL-11 deeper manuscript reading** (annotations/highlights) — remains backlog.
- **Realtime websocket / push** for the SOUL surface — poll interval +
  invalidation only (local-only threat model).
- Restructuring the existing keyword/drift viz beyond adding a world filter —
  surgical, not a rewrite.
- Desktop signing rollout — blocked on Apple Developer ID cert (non-agent-driven).
- Cloud sync / platform unpause — remains paused (PD-05).

## 8. Roadmap Position

- **Current iteration (V1.81, delivered 2026-07-02)**: Returned to the feature
  cadence after V1.80 stabilization; deepened the V1.79 reflection axis.
  Headline = Creator-SOUL Narrative (LLM on-demand synthesis + stale-invalidation,
  world-agnostic). Companions = per-World SOUL projection (`memory_fragments.world_id`
  migration + promotion-seam threading + additive DTO + DAO filter + UI
  world-selector), independent growth-curve (BL-10), auto-refresh. Shipped the
  user-locked "Creator SOUL (whole) vs World projection (subset)" model. Additive
  wire (`memory-fragment-info` + `world_id`; new soul-narrative request/response
  schemas) → `@42ch/nexus-contracts` 0.15.0 → 0.16.0. Plan structure P-1
  (contract landing incl. codegen moved from P0) → P0 backend ‖ P1 frontend
  (worktree) → P-last. QC tri-review 3/3 Approve after a 3-round fix-wave
  (world_id propagation test; bounded→fingerprint-cached sound distinct-keyword
  count resolving the read-path-cost vs sound-gate tension; UTF-8 char-truncation).
  QA Pass (full gate green; 379 web tests; migrations additive). Compound
  captured the fingerprint-cached-live-aggregate pattern. 4 low QC suggestions
  deferred V1.82+; 2 V1.80 doc residuals closed.
- **Next iteration (V1.82)**: Candidate evaluation at next `/iteration-start`.
  Backlog after V1.81: per-World LLM narratives (if V1.81 Creator-level narrative
  proves valuable), BL-09 standalone maturation dashboard, BL-11 deeper
  manuscript reading (annotations/highlights). Owner: `@project-manager`.
  Trigger: user initiates next `/iteration-start`.
- **Final goal**: Nexus as a local-first, AI-agent-driven creative writing tool
  where the author commands three closed loops (steer writing, triage quality,
  curate memory) **and** can reflect on the accumulated craft and evolving
  creative self — on a reliable, well-tested foundation.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Narrative synthesis quality is thin / generic (LLM produces a vague statement) | Med | High | P-1 locks the synthesis prompt contract + input shape (fragment keywords/summaries/temporal signal) before implement; the trait seam lets the prompt be iterated without re-architecting; an explicit "insufficient data" threshold gates generation so thin SOULs get an empty state, not a generic narrative. |
| Narrative agent invocation has no clean seam (daemon lacks ACP access for synthesis) | Low | High | Retired as an open design risk by P-1: grep confirms `SessionDigestSummarizer` is passthrough-only in production, and the real ACP prompt seam is `ProductionWorkerProvider` → `CapabilityRegistry` → `acp.prompt`. P0 adds sibling `SoulNarrativeSynthesizer` in `nexus-creator-memory` and a daemon adapter that calls `acp.prompt`; endpoint code depends on the trait and maps missing worker/capability through `NexusApiError`. |
| `world_id` threading misses a `create_fragment` site (fragment loses world context) | Med | Med | Enumerate all `create_fragment` call sites in P0 (grep-verified: daemon memory handler + orchestration creator capability builtins); add a regression test per site asserting world_id propagation; QC1 (architecture) audits the site coverage. |
| P0 backend and P1 frontend both need the shared web client surface → merge conflict | Med | Low | Strict file boundary (P0 = crates/schemas/generated only; P1 = apps/web only); P1 consumes P-1's frozen contract types and may mock the endpoint until P0 merges; resolve generated-import churn on `iteration/v1.81` after topic merge. |
| Narrative persistence choice (SOUL file vs new table) delays P0 | Low | Med | Decision locked in P-1: SQLite cache table `memory_soul_narratives` with `creator_id`, `narrative`, `generated_at`, `fragment_count_at_generation`, `max_fragment_created_at_at_generation`, `created_at`, `updated_at`. SOUL.md remains the human/agent identity file; narrative cache does not write to it. |
| Growth-curve viz weak when fragment data is sparse (new creators) | Med | Low | Reuse the V1.79 density-state branching (`empty`/`low-data`/`rich`); growth-curve degrades to a count + "not enough data yet" rather than a broken chart. |
| Narrative generation latency / cost on a large fragment set | Med | Med | On-demand only (author opts in); the synthesis input is the aggregated keyword/summary signal, not every fragment raw body; a cap on input size is defined in P-1; stale-invalidation avoids redundant regeneration. |

## 10. Wire contracts note

`wire_contracts_changed: TRUE`.

Two additive changes:

1. **`schemas/local-api/memory/memory-fragment-info.schema.json`** — add optional
   `world_id` (`["string", "null"]`; consumers treat absent/`null` as
   Creator-core-only). `required` stays `fragment_id` + `summary`. P0 owns the
   schema edit + codegen + wiring the DAO/handler to populate `world_id` +
   extending `memory_dto_roundtrip.rs`.
2. **New soul-narrative schemas** under `schemas/local-api/memory/`:
   - `soul-narrative-request.schema.json` — on-demand reflect request with
      required `creator_id` and optional `force_regenerate`.
   - `soul-narrative-response.schema.json` — required `creator_id`, `state`,
      `stale`, current counts, and threshold fields; optional `narrative`,
      `generated_at`, `fragment_count_at_generation`, and
      `max_fragment_created_at_at_generation` when a cached/generated narrative
      exists.
   - `list-memory-fragments-query.schema.json` — add optional `world_id` query
     param for the projection filter.

P-1 edits the schemas only; P0 runs `pnpm run codegen`, commits generated Rust +
TypeScript output, bumps `packages/nexus-contracts/package.json` 0.15.0 → 0.16.0,
wires the handler(s), and extends the round-trip regression test. P1 changes no
schema.

## Compound Round Summary

- Compound docs created: **1** — [`architecture-patterns/fingerprint-cached-live-aggregate.md`](../knowledge/architecture-patterns/fingerprint-cached-live-aggregate.md) (Knowledge track; surfaced from the 3-round fix-wave — a polled endpoint returning a live expensive aggregate must use a fingerprint cache, not a cap (under-counts) or an every-read exact scan (over-pays); the bounded-approximation-breaks-gate anti-pattern).
- New CONCEPTS.md entries: **0** (the fingerprint-cache is an architecture pattern, not a domain noun).
- Triggered compound-refresh: **no** — related to but distinct from the V1.80 `bounded-drain-completion-contract.md` (write-side drain semantics vs read-side aggregate cost); flagged in the doc for a future consolidation review, no stale doc to merge now.

## Iteration Retrospective (minimal)

- **做得好的**:
  - grill-me 锁定了 "Creator SOUL 整体 vs World 投影子集" 的产品模型 —— 数据模型缺口（`memory_fragments` 无 `world_id`）在派发前就被识别并设计为干净的 additive 透传，避免了实现期返工。
  - 把 codegen 从 P0 前移到 P-1，使 P0‖P1 真并行（各自消费冻结的生成类型），零合并冲突。
  - QC 三审在第 2/3 轮精准抓到了"有界近似破坏 gate 语义"（W-QC3-003）与"sound 计数回归 read 成本"（W-QC3-001）的两难 —— 迫使出 fingerprint-cache 这个正确解，并结晶为可复用知识。
- **可改进的**:
  - P0 初版在 read 路径上重复了 V1.80 已结晶的"bounded drain"教训的反面 —— 一次性 sound 扫描与每 poll 成本的权衡应在 plan 阶段就显式写出"fingerprint cache"而非依赖 3 轮 QC 发现。
  - fix-wave 出现了 1 次"agent 卡在读上下文阶段未实现"的空跑 —— 重派才落地；长 fix Assignment 应更强地指向"立即实现"。
- **下迭代建议**:
  - V1.82 候选评估：per-World LLM 叙事（若 V1.81 Creator 级叙事证明有价值）、BL-09 独立 maturation 仪表盘、BL-11 深度阅读（标注/highlight）、worlds-list/world-detail endpoint（让 world-selector 能渲染 world 标题 + 启用 Work-backed 无 fragment 的 subset-empty 路径）。owner `@project-manager`。
  - 收 4 个 low QC suggestion（validate-draft 步骤、typed error mapping、narrative 长度 cap、world-selector tracker 注）—— 可作为下迭代伴随轻轨。
