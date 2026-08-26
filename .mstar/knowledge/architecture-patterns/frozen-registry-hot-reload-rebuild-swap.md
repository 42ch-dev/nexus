---
module: harness-capability-registry
date: 2026-08-26
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - "Adding hot-reload / runtime mutation to a registry that was designed as boot-frozen"
  - "Extending an admission path (directory scan) to re-run against a live component"
  - "Keeping catalog consumers (HTTP route, MCP child watch) consistent with registry changes"
  - "Reviewing watchers for transient-failure wipe / baseline-absorb / leak hazards"
tags:
  - capability-registry
  - hot-reload
  - watcher
  - poll-digest
  - rebuild-swap
  - last-good
  - boot-equivalence
  - digestpoll
---

# Frozen-registry hot-reload via rebuild-and-swap

## Context

V1.172 admitted user capabilities (`~/.nexus42/capabilities/` descriptor +
manifest + wasm trio) with a **boot-time scan** into `CapabilityRegistry` —
set once on `WorkspaceState` via a `&mut` setter, **no interior mutability**
(frozen post-boot by design). V1.176 (RN-2) made capability iteration
restart-free. Two wrong first instincts, both caught before landing:

1. "Mutate the registry in place" — impossible (frozen `Arc` semantics) and
   wrong anyway: in-flight dispatch holds references.
2. "Reuse the peer lane's live table" — the peer lane (`PeerToolTable`,
   `LazyLock` + Mutex merged at read time) is a **different mechanism** for a
   different lane; reusing it would fork the spine.

## Guidance

**The pattern: rebuild-and-swap behind a shared `RwLock` holder.**

- **Rebuild, never mutate.** On a detected change, construct a **fresh**
  registry through the **same admission path as boot** (`scan` → admission
  ordering collision → file → hash → clamp → merge last-good). No parallel
  admission implementation, no in-place `Vec` mutation.
- **Holder seam.** `CapabilityRegistryHolder(Arc<RwLock<Arc<CapabilityRegistry>>>)`:
  readers `read()` and **clone the Arc** (one refcount), then use the snapshot;
  the writer swaps with `(*guard).replace(fresh)` and drops the previous
  generation **after releasing the write lock** (never stall readers on a
  destructor). In-flight dispatch naturally finishes against the pre-swap
  generation (last-good).
- **Watch mechanism: poll + digest (no new deps).** 1 s tick computes a
  content digest (sorted walk) of the scan dir; digest change → rebuild.
  Same shape as the V1.175 MCP child watch. An fs-notify crate is a new
  dependency for a problem polling already solves at author scale.
- **Three failure states, not two.** Digest polling distinguishes
  `Missing` (dir gone — honest removal, names drop), `Unreadable` (dir exists
  but read fails — **keep last-good, no rescan, no baseline disturbance**),
  and `Tree` (content). Collapsing Missing/Unreadable silently wipes the
  registry on a transient EACCES/EMFILE.
- **Per-entry failures need the same distinction.** `ScanOutcome.transient`:
  a per-entry `read_dir` error marks the scan incomplete → merge **carries
  every unmatched last-good entry** (an incomplete scan never reads as
  deletions).
- **Seed the baseline from boot.** The watcher's first poll must **compare**
  against the digest computed at boot-scan time, not establish it — else a
  write between boot scan and first poll is absorbed as baseline.
- **Derived sets must be live too.** Anything reserved/derived from the
  registry at boot (peer-lane `reserved_tool_ids`) must re-derive **at use
  time** from the holder, or hot-admitted names silently lose protection
  (V1.174 collision contract regression — caught by plan QC, not task
  review).
- **`&'static str` fields under reload: intern, don't leak.** The
  `Capability` trait mandates `&'static str`; `Box::leak` per admission is
  unbounded across reloads. A global interner (`distinct value → one leak`)
  keeps memory bounded by distinct admitted strings.
- **Bound the promise honestly.** Document the budget decomposition (watch
  interval + catalog freshness + downstream watch), include rebuild time in
  the daemon leg, and pin journey tests at ~1.5× the documented bound —
  2×+ slack passes regressions.

## Why This Matters

A frozen-by-design registry is a **sound** architecture; the hot-reload
temptation is to punch holes in it (interior mutability, second table,
mutable statics). Rebuild-and-swap keeps the freeze discipline (readers
always see a complete, boot-equivalent generation) while making iteration
restart-free — and **boot-equivalence is machine-checkable**: identical
directory content must yield an identical registry whether built at boot or
hot (`hot_rebuild_equals_boot_constructor_for_identical_dir`).

## When to Apply

- Any boot-scanned directory-backed registry gaining runtime reload
  (modules, presets, config-shaped surfaces).
- Reviewing watcher/loop code: look for the Missing/Unreadable collapse,
  baseline-establish-on-first-poll, per-entry transient handling, lock-scope
  drops, and boot-snapshot derived sets.
- Deciding poll+digest vs fs-notify: prefer the dependency-free option until
  scale proves otherwise; document the cost model where the loop runs.

## Examples

- V1.176 P1 (RN-2): `crates/nexus-orchestration/src/capability/watch.rs`
  (`DigestPoll`, `watch_loop_inner(initial_digest)`, `WatcherGuard`
  abort-on-drop), `scan.rs` (`ScanOutcome.transient`), `user_capability.rs`
  (interner), `boot.rs` (boot digest seeding, holder wiring),
  `connect/table.rs` (`live_reserved_tool_ids`).
- QC findings that shaped it: transient-wipe (I-1), boot-to-first-poll absorb
  (F-001/W-1), per-admission leak (F-002/W-2), reserved-set boot snapshot
  (W-1/F-003) — all fixed in wave `4db562da`; boot-equivalence + journey
  pins green.
- Catalog/MCP consistency rides existing consumers unchanged: `GET
  /v1/daemon/tools` reads the holder; the V1.175 MCP child watch is
  source-agnostic (digest over the tools body) — zero child change.
