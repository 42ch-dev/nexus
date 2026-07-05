# Creator Memory SOUL Lifecycle — Specification v1

**Status**: Draft (V1.82 amendment — §7.2.4 per-(creator, world) narrative lifecycle added)
**Document class**: Draft overlay
**Created**: 2026-07-01
**Scope**: Creator memory review lifecycle, SOUL fragments, and on-demand Creator-SOUL + per-World narrative cache semantics.
**Coordinates with**:

- [creator-workflow.md](creator-workflow.md) §5.6 — SOUL visualization contract over memory fragments
- [web-ui.md](web-ui.md) §26–§27 — Creator SOUL Maturation (V1.81) + SOUL Completion per-World narrative (V1.82)
- [`schemas/local-api/memory/`](../../../schemas/local-api/memory/) — wire contract schemas
- [on-demand-synthesis-read-path-invariant.md](../architecture-patterns/on-demand-synthesis-read-path-invariant.md) — read-path LLM gating (applies per world)
- [fingerprint-cached-live-aggregate.md](../architecture-patterns/fingerprint-cached-live-aggregate.md) — read-path cost (cache key per (creator, world))

**Iteration compass**: [v1.82-soul-completion-delivery-compass-v1.md](../../iterations/v1.82-soul-completion-delivery-compass-v1.md) (§7.2.4); supersets [v1.81-creator-soul-maturation-delivery-compass-v1.md](../../iterations/v1.81-creator-soul-maturation-delivery-compass-v1.md)
**Plans**: [2026-07-01-v1.81-creator-soul-narrative-and-world-foundation.md](../../plans/2026-07-01-v1.81-creator-soul-narrative-and-world-foundation.md) (V1.81 P0) · [2026-07-02-v1.82-per-world-narrative-backend.md](../../plans/2026-07-02-v1.82-per-world-narrative-backend.md) (V1.82 P0)

## 1. Purpose

This spec records the durable lifecycle rules for Creator memory fragments that
feed the SOUL reflection surface. It complements `creator-workflow.md` §5.6 and
the Local API memory schemas under `schemas/local-api/memory/`.

## 7. Fragment and narrative lifecycle

### 7.2 Memory fragments, world projection, and on-demand narrative

#### 7.2.1 Fragment promotion semantics

`memory_pending_review.world_id` is the source of world context during review.
When a pending review is promoted into a `memory_fragments` row, the promotion
seam must preserve that context into nullable `memory_fragments.world_id`.

Nullable `world_id` semantics are normative:

| Query / state | Meaning |
| --- | --- |
| `world_id` omitted from fragment-list query | **Creator SOUL (whole)** — all accumulated fragments for the creator, world-agnostic. |
| `memory_fragments.world_id = <world>` | **Per-World SOUL projection (subset)** — a fragment subset filtered by the world they emerged from. A drill-down view, not a separate identity. |
| `memory_fragments.world_id IS NULL` | **Creator-core-only** — a fragment with no originating world context. |

The Creator SOUL is the whole, not the `NULL` subset. Consumers must not treat
`NULL` as "all worlds"; `NULL` means core-only / no world provenance.

#### 7.2.2 Projection behavior

The Local API may expose an optional `world_id` filter for read-side projection.
Omitting the filter returns the whole Creator SOUL. Supplying a `world_id` returns
only fragments whose nullable `world_id` equals that value. V1.81 does not expose
a public query value for `world_id IS NULL`; core-only filtering can be added in
a future contract if a user-facing need appears.

#### 7.2.3 On-demand Creator-SOUL narrative lifecycle

The Creator-SOUL narrative is an on-demand, world-agnostic synthesis over the
whole Creator SOUL. It is not a per-world narrative and it is not written into
`SOUL.md`.

Lifecycle:

1. **Generate** — the author triggers reflection through the Local API. If the
   Creator has at least 10 fragments and at least 20 distinct fragment keywords,
   the daemon builds a capped synthesis signal (top keywords, recent summaries,
   temporal buckets) and invokes the narrative synthesizer. Raw session digests
   are never passed to the narrative prompt.
2. **Persist** — the generated narrative is cached in SQLite table
   `memory_soul_narratives` with `creator_id`, `narrative`, `generated_at`,
   `fragment_count_at_generation`, `max_fragment_created_at_at_generation`,
   `created_at`, and `updated_at`.
3. **Stale** — each read compares current fragment stats with the persisted
   snapshot. The narrative is stale when the current fragment count differs from
   `fragment_count_at_generation` or the current max fragment `created_at`
   differs from `max_fragment_created_at_at_generation`. Any new fragment is
   enough to mark stale; stale is a prompt to re-reflect, not a forced job.
4. **Re-generate** — when the author explicitly re-reflects, the daemon repeats
   generation against the current whole Creator SOUL and overwrites the cache
   row with new snapshot metadata.

If the insufficient-data gate fails, the API returns an insufficient-data state
with current counts and thresholds instead of invoking the LLM or caching a thin
narrative.

#### 7.2.4 per-(creator, world) Narrative lifecycle (V1.82 amendment)

V1.82 extends the on-demand narrative to per-World: each world's fragment subset
gets its **own** narrative — the Creator SOUL's inclination *within that world*
— distinct from the world-agnostic Creator-level narrative above. Both
coexist; the cache key and lifecycle are scoped per (creator, world).

**Composite cache key.** `memory_soul_narratives` PRIMARY KEY changes from
`creator_id` to composite `(creator_id, world_id)`:

| Cache row | `world_id` value | Meaning |
| --- | --- | --- |
| Creator-level narrative | `NULL` | The V1.81 world-agnostic narrative over ALL the creator's fragments. Behavior unchanged. |
| per-World narrative | `<world_id>` | The narrative over that world's fragment subset only. |

`NULL world_id` on `memory_soul_narratives` = Creator-level narrative (the whole,
all fragments, world-agnostic) — **not** the `NULL`-world core-only subset from
§7.2.1. (The §7.2.1 `memory_fragments.world_id IS NULL` semantics govern fragment
provenance — "Creator-core-only, no originating world"; the §7.2.4
`memory_soul_narratives.world_id IS NULL` semantics govern narrative scope —
"Creator-level, the whole." They are different columns on different tables and
must not be conflated.)

**SQLite NULL-PK mitigation.** A partial UNIQUE index
`idx_memory_soul_narratives_creator_only ON memory_soul_narratives (creator_id)
WHERE world_id IS NULL` guarantees exactly one Creator-level row per creator
(SQLite does not enforce uniqueness on NULL composite-PK values).

**per-World lifecycle** (mirrors §7.2.3, scoped per (creator, world)):

1. **Generate** — the author selects a world and triggers reflection with
   `world_id` present. The endpoint verifies the creator owns the world
   (`narrative_worlds.owner_creator_id` via the `is_world_owned` helper) before
   any work. If that world's subset meets the per-world data gate (≥10 fragments
   + ≥20 distinct keywords within the subset), the daemon builds a capped
   synthesis signal over **only that world's fragments** and invokes the
   narrative synthesizer.
2. **Persist** — the generated narrative is cached at the `(creator_id, world_id)`
   row with the same snapshot + fingerprint-cache columns, scoped to the subset.
3. **Stale** — each read compares the world-subset's current stats with the
   persisted snapshot. The narrative is stale when the world's fragment count or
   max `created_at` differs from the generation snapshot. Stale is per world:
   new fragments in a *different* world do not stale this world's narrative.
4. **Re-generate** — explicit re-reflect for the selected world repeats
   generation against the current subset and overwrites the `(creator, world)`
   cache row.

**Read-path invariant (mandatory).** The per-World read path
(`force_regenerate=false` + a `world_id`) returns the state enum
(`ungenerated` / `current` / `stale` / `insufficient_data`) **without invoking
the LLM** — synthesis is gated behind `force_regenerate=true` only (the
`on-demand-synthesis-read-path-invariant` lesson). A per-World narrative may be
`insufficient_data` or `ungenerated` even when the Creator-level narrative is `current`;
the states are independent per scope.

**per-World stats + fingerprint cache.** per-World `fragment_count`,
`max_created_at`, and the distinct-keyword count derive from
`memory_fragments.world_id` filtered by `WHERE creator_id = ? AND world_id = ?`
(the Creator-level narrative uses `WHERE creator_id = ?` with no world filter — it is the
whole, not the NULL subset). The fingerprint-cache pattern
(`fingerprint-cached-live-aggregate`) applies per (creator, world): the cache
row is the `(creator_id, world_id)` row, keyed by the per-scope fingerprint
`"{count}:{max_created_at}"`.
