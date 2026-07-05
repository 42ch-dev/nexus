---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.60-df46-local-parity"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (admission gates, creator isolation, atomicity / lost-update, timeline immutability, input validation, error handling, spec compliance)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-22-v1.60-df46-local-parity (Track A — DF-46 Local Capability Parity, 5 orchestration capabilities)
- Review range / Diff basis: 7cec348d..4d322c7c (Wave 1)
- Working branch (verified): iteration/v1.60
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 27 (plan + compass + specs + 5 capability handler impls + tests + registry + daemon handlers)
  - `.mstar/plans/2026-06-22-v1.60-df46-local-parity.md`
  - `.mstar/iterations/v1.60-df46-local-parity-and-script-depth-delivery-compass-v1.md`
  - `.mstar/knowledge/specs/world-delta-propose-apply.md` (NEW Draft)
  - `.mstar/knowledge/specs/acp-capability-set.md`
  - `.mstar/knowledge/specs/capability-registry.md`
  - `crates/nexus-orchestration/src/capability/builtins/world.rs` (961 LOC: WorldStateQuery + WorldDeltaPropose + WorldDeltaApply)
  - `crates/nexus-orchestration/src/capability/builtins/timeline.rs`
  - `crates/nexus-orchestration/src/capability/builtins/fork.rs`
  - `crates/nexus-orchestration/tests/capability_registry.rs`
  - Supporting: daemon capability_registry, works handlers, etc.
- Commit range: 7cec348d..4d322c7c
- Tools run:
  - `git diff 7cec348d..4d322c7c --stat`
  - Full targeted reads of the three new builtin handler files + shared `ensure_world_owned`
  - `grep` for admission gates (`ensure_world_owned`, `Forbidden`, `creator_id`), cross-creator tests, `unwrap`/`expect` in production paths
  - `cargo test -p nexus-orchestration --test capability_registry` (baseline)
  - Manual audit of tx + lost-update guard, collision checks, fork-point validation, parameterized SQL

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001 — Duplicated ownership gate (`ensure_world_owned`) creates drift risk between daemon host_tool layer and orchestration handlers.**
  The function is a one-line `SELECT world_id FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?` duplicated from `daemon-runtime/src/api/handlers/works.rs:825` (`ensure_world_accessible_for_creator`). Comment in world.rs explicitly states "orchestration crate cannot depend on nexus-daemon-runtime". All five new capabilities (query/propose/apply/append/fork) call it before any read or write. Cross-creator tests (`world_state_query_rejects_cross_creator`, `world_delta_*_rejects_cross_creator`, timeline/fork equivalents) assert `CapabilityError::Forbidden`. However, any future schema or policy change must be replicated in two places. In local single-creator this is low immediate exploitability, but it is a correctness + future security maintenance hazard.
  - Location: `crates/nexus-orchestration/src/capability/builtins/world.rs:42-63` (and re-exported/used in timeline.rs:18, fork.rs:27); daemon host_tool equivalent.
  - Fix recommendation: Extract a small shared crate (or move to `nexus-local-db` as a thin verified helper) that both daemon and orchestration can depend on. At minimum add a compile-time or test-time cross-check (e.g., a test that greps both implementations for the same predicate).

- **W-002 — `timeline.event.append` performs a post-DAO UPDATE to rename an explicitly supplied `event_id`; failure after `append_event` succeeds leaves an event under the auto-allocated id.**
  Flow: (1) explicit collision check (good), (2) `append_event` allocates `evt_*` and inserts, (3) if `parsed.event_id` differs, a second `UPDATE ... SET timeline_event_id = ?` is issued. If the UPDATE fails (constraint, I/O, etc.) the event now exists under the runtime-generated id while the caller expected the explicit one. No transaction wraps the rename with the append. The collision guard prevents duplicates, but the rename path can produce "wrong id" state.
  - Location: `crates/nexus-orchestration/src/capability/builtins/timeline.rs:141-157` (the `if let Some(ref explicit_id)` rename block after `append_event`).
  - Fix recommendation: Either (a) perform the append + rename inside a single tx, or (b) reject explicit `event_id` at the handler level for V1.60 (let the DAO always allocate) and document that explicit ids are not supported until a later hardening. Add a test that forces the rename path and then simulates failure (or at least asserts the final id matches the requested one on success).

- **W-003 — `world.delta.apply` claims "under workspace lock" in spec and plan, but the implementation only uses a per-world sqlx transaction; no named advisory lock or cross-process serialization is visible.**
  `world-delta-propose-apply.md` §5 and the plan T4 describe atomic write "under workspace lock". The code does:
  - `pool.begin()`
  - re-verify ownership inside tx (good TOCTOU)
  - per-change `old_value == live` lost-update guard (good)
  - apply all writes, `tx.commit()`
  This is atomic within one process/connection for a single world, which is sufficient for local-first single-writer. However, it does not match the wording "workspace lock acquired" in the plan/spec. If a future multi-process or daemon-worker scenario appears, this will be a real race surface.
  - Location: `world.rs:467-685` (the `WorldDeltaApply::run` tx block); spec `world-delta-propose-apply.md:164-178`.
  - Fix recommendation: Either update the spec to say "single sqlx transaction with ownership + lost-update guards (local single-writer atomicity)" or implement an actual advisory lock (e.g., via the existing workspace OCC machinery) and document the lock acquisition site. At minimum add a code comment citing the local-first assumption.

### 🟢 Suggestion
- **S-001 — `world.delta.apply::run` is a 250+ line function with a large match on entity/field inside the tx.**
  The match arms for `kb_key_block` (update + create) and `world_metadata` title are each ~30-40 LOC and contain their own lost-update + write logic. Adding a third entity type later will increase the surface. Consider small private helpers (`apply_kb_update`, `apply_kb_create`, `apply_world_title`) that each take `&mut tx` and the change, keeping the main loop as a dispatcher.
  - Location: `world.rs:491-679`.
  - Impact: maintainability / future audit surface (not a current bug).

- **S-002 — Plan states "≥15 test vectors (5 IDs × 3)" but the new test surface in the reviewed files is primarily the cross-creator + invalid-input happy-path set.**
  We observed explicit cross-creator `Forbidden` tests for query/propose/apply and timeline/fork equivalents, plus input-invalid cases. The plan's "≥1 success + ≥1 failure + ≥1 admission gate per ID" is directionally satisfied by the pattern, but a consolidated count or a single test module asserting the full matrix was not immediately visible in the diff surface. This is a documentation / traceability suggestion rather than a correctness gap.
  - Location: plan T7; `world.rs` tests, `timeline.rs` tests, `fork.rs` tests.
  - Fix: Add a short table or comment in the test modules (or in `capability_registry.rs`) enumerating the 15 vectors and their status.

- **S-003 — `ensure_world_owned` returns a generic `Forbidden("world not found or not owned by creator")` that does not distinguish "missing" vs "owned by someone else".**
  For audit / security forensics it can be useful to know whether the world simply does not exist or exists but belongs to another creator. Current behavior is safe (fail-closed), but the error message conflates the two.
  - Minor; only relevant if richer error taxonomy is added later.

### Notes (positive evidence)
- All five capabilities require `creator_id` in the declared JSON schema and call `ensure_world_owned` before any DB read or write.
- `world.delta.apply` correctly implements the runtime-side decision (spec §3), runs the entire package in one tx, re-checks ownership inside the tx, and applies a per-change lost-update guard using `old_value`.
- Timeline append: explicit collision rejection for supplied `event_id`; always inserts as `provisional`; no path mutates `canon` rows.
- Fork create: validates the fork-point event exists on the parent branch before allocating; materializes via a `fork_created` marker (lazy fork semantics); PD-01 boundary is documented in the module doc comment.
- No dynamic SQL construction; all queries use `sqlx::query_scalar` / `sqlx::query` with `.bind()`.
- No `unwrap`/`expect` on production paths in the three new handler files (tests legitimately use them).
- Input schemas are strict (`additionalProperties: false`).
- Spec `world-delta-propose-apply.md` is present, declares the atomic contract, policy gates, and the agent-vs-runtime split.

## Source Trace
- **W-001 (duplicated gate)**: manual diff + grep on `ensure_world_owned` + comment at world.rs:34-35; cross-reference to daemon host_tool_handlers.
- **W-002 (timeline rename)**: `timeline.rs:123-157` (append + conditional rename after DAO call).
- **W-003 (lock wording)**: spec §5 "Atomicity contract", plan T4, code `world.rs:467` (`begin`) vs absence of any `workspace_lock` or advisory primitive in the reviewed diff.
- Cross-creator tests: `world.rs: world_state_query_rejects_cross_creator`, `world_delta_propose_rejects_cross_creator`, `world_delta_apply_rejects_cross_creator`; timeline and fork equivalents.
- Positive atomicity evidence: `world.rs:472-486` (TOCTOU inside tx), `513-525` and `636-648` (old_value vs live), `683-685` (commit only after all per-change decisions).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

(The three Warnings are medium-severity maintainability / future-correctness items in a pre-1.0 local-first codebase. No Critical security holes, no missing admission gates, no SQL injection, no path traversal, and the core atomicity + immutability invariants are implemented and tested. The duplication and rename risks should be tracked as residuals or addressed in a small follow-up, but they do not block the current wave.)
