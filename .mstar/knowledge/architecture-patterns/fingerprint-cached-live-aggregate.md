---
module: local-api, daemon-runtime, local-db
date: 2026-07-02
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-07-01-v1.81-creator-soul-narrative-and-world-foundation
tags: [read-path-cost, live-aggregate, fingerprint-cache, polling, bounded-endpoint, sound-count, local-api, reliability, nexus-contracts]
applies_when: a polled local endpoint's response contract requires a live "current" aggregate that is expensive to compute exactly, under a small-write / large-read ratio
---

# Fingerprint-cached live aggregate — decouple read-path cost from sound exact counts

## Context

Nexus ships local-only endpoints that a client **polls** (e.g. the web UI polls
`POST /v1/local/memory/soul/reflect` every ~30s). The response contract requires
a live "current" aggregate — for the Creator-SOUL narrative, `current_fragment_count`
and `current_distinct_keyword_count` are **required** fields, and the distinct-keyword
count drives an `insufficient_data` gate (`<10 fragments OR <20 distinct keywords`).

The distinct-keyword count is expensive: keywords are stored as a JSON array per
`memory_fragments` row, so an exact distinct count across a creator's fragments
means scanning + decoding every row. Computing it on every read/poll is
O(total fragments + total keyword JSON bytes) per poll — unacceptable for a 30s poll.

This is the **read-side cost trap** — distinct from the V1.80 drain-completion
contract (write-side: did the queue advance?) and from cursor pagination
(read-side listing). It is its own pattern with its own two-sided trap.

## Guidance

### The two-sided trap (both halves are easy to get wrong, in opposite directions)

> **A bounded approximation that breaks a gate's semantics is worse than no bound — and a sound exact count on every read is unaffordable. The resolution is a fingerprint cache, not a cap.**

**Side A — "bound it with a cap" (W-QC3-003 under-count).** Replacing `fetch_all()`
with `SELECT … LIMIT N` + Rust decode bounds the read cost, BUT it under-counts:
a creator whose N newest rows don't contain all distinct keywords reports
`distinct_keyword_count < 20` → the gate returns a **false `insufficient_data`**
for a creator who actually has enough data. A cap is performance-bounded but
**not semantically sound** for proving a `< threshold` gate. Seeing `≥ threshold`
in the cap is sufficient; seeing `< threshold` in the cap proves nothing about
the full set.

**Side B — "compute it exactly every time" (W-QC3-001 unbounded read).** Making
the count sound by streaming all rows to EOF (no LIMIT) gives an exact count
BUT reintroduces the O(total fragments + keyword JSON) cost on **every read/poll**,
even when returning a cached narrative. For a polled endpoint this is the
original reliability regression.

Both halves were hit in V1.81 across three fix-wave rounds; neither a cap nor
an unbounded exact count is correct.

### The pattern — fingerprint cache

Decouple the read-path cost from the aggregate computation with a **fingerprint
cache** keyed on cheap SQL aggregates:

1. **Fingerprint = `"{COUNT(*)}:{MAX(created_at)}"`** (or any cheap monotonic
   signature of the row set). Both are O(1) SQL aggregates — no row
   materialization, no JSON decode.
2. **Persist the cache** alongside the aggregate's consumer (here, a
   `distinct_keyword_count_cache` + `stats_fingerprint` column on
   `memory_soul_narratives`).
3. **Read path**: compute the cheap fingerprint; if it **matches** the cached
   fingerprint → return the cached aggregate (zero expensive work). If it
   **differs** → recompute the aggregate **soundly** (early-exit-at-threshold
   streaming, or full EOF scan — sound either way) + update the cache +
   fingerprint.
4. **Stale detection** uses the same cheap aggregates (`fragment_count`,
   `max_created_at` vs the generation snapshot) — never the expensive aggregate.

### Why it works (and when it doesn't)

- **Cost shifts from read-frequency to write-frequency.** Reads are frequent
  (polling); writes are infrequent (review batches create fragments). The
  expensive recompute runs only when fragments actually change, not every poll.
  For an append-mostly local model this is proportional.
- **Soundness is preserved.** When the count IS computed (fingerprint mismatch),
  it is exact (or early-exit-at-threshold, which is sound for a `≥ threshold?`
  gate). The cache never holds an under-count.
- **Fingerprint limitation**: `"{COUNT}:{MAX}"` detects any append (count rises,
  max rises) and any tail delete (count falls). It does **not** detect a
  delete-then-reinsert that preserves both count and max. For an append-mostly
  model with no production update path, this is acceptable; document it. If the
  model gains in-place mutation, use a stronger fingerprint (row count + sum of
  hashes, or a version column bumped on every write).

### The gate-threshold insight (early-exit sound count)

When the aggregate feeds a `≥ threshold?` gate and the threshold is small,
**early-exit streaming** is sound AND cheap: stream rows, accumulate a `HashSet`,
stop as soon as the set reaches the threshold (gate passes authoritatively).
Only the `< threshold` case (small set anyway) scans to EOF. This bounds the
sound recompute by `min(rows-to-find-threshold-distinct, total-rows)`.

## Why this matters

Polled endpoints returning live expensive aggregates are a recurring shape in
local-first tools (status dashboards, maturation indicators, any "current N"
field). Without the fingerprint-cache pattern, every such endpoint either
over-pays on every poll or ships an unsound gate. The pattern makes the
read-path O(cheap aggregates) in the steady state while keeping the aggregate
exact.

## When to apply

- A polled/refreshed endpoint whose response carries a live "current" aggregate.
- The aggregate is expensive to compute exactly (decode/scan/join).
- Small-write / large-read ratio (writes are infrequent events; reads are
  periodic polls).
- A gate consumes the aggregate (`≥ threshold?`) — the early-exit sound count
  applies.

Do NOT apply when the aggregate is already cheap (a SQL `COUNT(*)` with no
decode) — just compute it. The pattern earns its complexity only when the
sound exact aggregate is genuinely expensive.

## Examples

- **V1.81 `POST /soul/reflect`** — `current_distinct_keyword_count` (distinct
  keywords across JSON-array `memory_fragments.keywords`) drives the
  `insufficient_data` gate + the response field. Fingerprint cache on
  `memory_soul_narratives`; cached read = O(COUNT + MAX) only. (3 fix-wave
  rounds: LIMIT-200 under-count → unbounded exact regression → fingerprint
  cache.)
- **Hypothetical**: a "current open-findings count by severity" field on a
  polled dashboard where findings carry a decoded tag array — same shape.

## Relationship / consolidation flag

Related to but distinct from
[`bounded-drain-completion-contract.md`](bounded-drain-completion-contract.md)
(V1.80): both concern bounded local endpoints under the local-only threat
model, but V1.80 is **write-side drain-completion semantics** (`has_more`
reflects advancement) while this doc is **read-side live-aggregate cost**.
Flagged for a future consolidation review if a unifying "bounded local
endpoint contract" doc is warranted; for now they address orthogonal traps.
