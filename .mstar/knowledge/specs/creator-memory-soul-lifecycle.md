# Creator Memory SOUL Lifecycle — Specification v1

**Status**: Draft (V1.81 amendment)
**Document class**: Draft overlay
**Created**: 2026-07-01
**Scope**: Creator memory review lifecycle, SOUL fragments, and on-demand Creator-SOUL narrative cache semantics.
**Coordinates with**:

- [creator-workflow.md](creator-workflow.md) §5.6 — SOUL visualization contract over memory fragments
- [web-ui.md](web-ui.md) §26 — Creator SOUL Maturation product contract
- [`schemas/local-api/memory/`](../../../schemas/local-api/memory/) — wire contract schemas

**Iteration compass**: [v1.81-creator-soul-maturation-delivery-compass-v1.md](../../iterations/v1.81-creator-soul-maturation-delivery-compass-v1.md)
**Plan**: [2026-07-01-v1.81-creator-soul-narrative-and-world-foundation.md](../../plans/2026-07-01-v1.81-creator-soul-narrative-and-world-foundation.md) (P0)

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
