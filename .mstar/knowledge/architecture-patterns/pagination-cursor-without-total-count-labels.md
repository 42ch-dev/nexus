---
module: local-api, web-ui
date: 2026-07-01
problem_type: knowledge
category: architecture-patterns
severity: low
plan_id: 2026-07-01-v1.79-manuscript-reading-surface
tags: [pagination, cursor, has-more, count-ui, wire-contract, local-api, nexus-contracts]
applies_when: building a UI count/badge affordance over a Nexus cursor-paginated list endpoint
---

# Cursor pagination without `total` — render honest "N+" lower-bound count labels via `has_more`

## Context

Nexus Local API list endpoints use **cursor pagination** with the envelope
`PaginationInfo = { limit, next_cursor?, has_more }`. There is **no `total`
field** — by design (cursor pagination cannot cheaply yield a total without a
separate `COUNT(*)`, which the daemon does not run for list reads). This is the
canonical shape across `works`, `chapters`, `findings`, `kb_key_blocks`,
`memory_pending_review`, etc.

The contract is defined in `schemas/local-api/kb/pagination-info.schema.json`
(`additionalProperties: false`) and codegen'd into `@42ch/nexus-contracts`
`PaginationInfo`.

## Guidance

When a UI needs a **count** of items behind one of these endpoints (e.g. "N open
findings", "N key blocks", "N pending reviews"), **do not**:

- assume `pagination.total` exists (it does not — inventing it is wire-contract drift);
- fetch all pages and count client-side (over-fetch, slow at scale);
- render the page-1 `items.length` as if it were the exact total (silent undercount for large sets — a correctness defect).

**Do**: read `has_more` from the loaded page and render an honest **lower-bound
"N+"** label when `has_more === true`, e.g. `"2+ open findings"`. When
`has_more === false`, the page-1 count IS exact and renders without the `+`.

```
count = items.length
label = has_more ? `${count}+` : `${count}`
aria-label = has_more ? `${count} or more open findings` : `${count} open findings`
```

This is the accurate, contract-faithful affordance. It tells the author "at
least this many" rather than misreporting an exact number that is wrong.

## Why This Matters

- **Correctness**: a headline feature ("see your work's maturity at a glance")
  silently misreporting counts for any work with >1 page of findings/KB is a
  real defect, not a cosmetic nit. QC (qc3 performance/reliability) flagged the
  page-1 undercount as **Request Changes** blocking in V1.79 P0.
- **Contract discipline**: `apps/web/AGENTS.md` + `schemas/AGENTS.md` forbid
  parallel/invented DTO shapes. Treating `pagination.total` as existing when the
  schema says `additionalProperties: false` is drift.
- **Cost**: a `COUNT(*)` per list read would be wasteful for a local-first tool;
  the cursor design deliberately avoids it. The UI should match that decision.

## When to Apply

- Any "N items" badge, count chip, or summary stat in `apps/web` (or
  `apps/desktop`) that consumes a cursor-paginated Local API list endpoint.
- Confirmed paginated (no `total`): `listWorks`, `listChapters`, `listFindings`,
  `listPendingReviews`, `getWorldKbGraph` (entity list portion), and any future
  endpoint returning `PaginationInfo`.
- If a future endpoint DOES add a `total` (schema change), prefer the exact
  total; revisit this guidance then.

## Examples

### V1.79 P0 reading-surface maturation indicators (the case that surfaced this)

`useOpenFindingsCount` (reading-hooks.ts) initially returned page-1
`items.length`. For a work with >1 page of open findings it undercounted. The
fix exposes a `truncated: boolean` derived from the last loaded page's
`pagination.has_more`; `CountBadge` renders `"${count}+"` when truncated.

The first fix-wave brief assumed `pagination.total` existed; the implementer
verified the contract (`packages/nexus-contracts/src/generated/local-api/kb/PaginationInfo.ts`
→ `{ limit, next_cursor?, has_more }`, no `total`) and implemented the
`has_more` lower-bound label instead (qc3's option b). This avoided both the
undercount AND a wire-contract drift.

### Anti-pattern (do not)

```ts
// ❌ invents a field that does not exist in the contract
const total = pagination.total ?? items.length;   // TS error / undefined at runtime
// ❌ silent undercount for multi-page sets
const count = items.length;                        // wrong when has_more
```

### Pattern (do)

```ts
// ✅ honest lower-bound label
const count = items.length;
const truncated = pagination.has_more === true;
return { count, truncated, label: truncated ? `${count}+` : `${count}` };
```

## Source

- Surfaced: V1.79 P0 QC fix-wave (qc3 W-QC3-002; qc1 W-001 concurred).
- Contract: `schemas/local-api/kb/pagination-info.schema.json`.
- Implementer verification: `PaginationInfo` generated TS has no `total`.
- Related (distinct): `contracts-gap-on-shipped-backend.md` covers the
  schema/codegen gap pattern; this doc covers the pagination-count UX contract.
