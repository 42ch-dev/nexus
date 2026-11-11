---
module: nexus-local-db / nexus-core / nexus-orchestration (World KB persistence)
date: 2026-11-11
last_updated: 2026-11-12
problem_type: architecture
category: architecture-patterns
severity: medium
plan_id: v233-knowledge-ir-p1
applies_when:
  - converging production code that writes a table directly onto one store seam
  - a handler already owns a transaction but the store trait method opens its own
  - proving that an insert stays inside a caller's transaction when a later step refuses
  - adding a source guard that keeps ad-hoc SQL out of other crates
  - reaching a crate-private write port from a crate that must not depend on its owner
  - a write path resolves its root from cwd while the port resolves it from the workspace
related_components:
  - crates/nexus-local-db/src/kb_store.rs
  - crates/nexus-local-db/src/actor_knowledge_store.rs
  - crates/nexus-core/src/execution/compute.rs
  - crates/nexus-core/src/execution/work_artifact.rs
  - crates/nexus-core/src/execution/executor.rs
  - crates/nexus-orchestration/src/capability/builtins/work_commit.rs
  - crates/nexus-orchestration/src/capability/builtins/world.rs
tags:
  - single-writer
  - transaction-seam
  - rollback-proof
  - source-guard
  - sqlx
---

# Single-writer store seam for a shared table

## Problem

A table had four writers across three crates — a compute settlement insert in `nexus-core`, two orchestration capability updates in `nexus-orchestration`, and a dynamic actor patch/delete in `nexus-local-db`. They worked, but every new caller had to rediscover the predicates (revision CAS, terminal-status exclusion, ownership conjunct). The existing `KbStore` trait could not host all of them: its methods open their own transaction, so a handler that already holds one would lose atomicity.

## Pattern

1. **One file owns the SQL.** Move every write statement into the store module as a primitive that takes `&mut Transaction<'_, Sqlite>` and returns rows affected. Keep the predicate byte-for-byte; a rewritten predicate is a behavior change, not a refactor.
2. **Keep policy at the call site.** Zero-rows → conflict translation, constraint mapping, preimage reads, referent guards and rollback arms stay in the calling module. The store primitive must not know domain policy.
3. **Prefer caller-transaction primitives over the trait when the caller already holds `tx`.** `insert_key_block_in_tx(tx, …)` is the seam; `SqliteKbStore::insert_knowledge_entry` is for entry points that own the transaction.
4. **Write the mutation test first, then the code.** A source guard (walk `crates/` + `apps/`, skip `tests/` and `migrations/`, cut each file at its last exact `#[cfg(test)]`, match SQL tokens with a delimiter so `foo_new` never matches) keeps the seam single and fails loud with `file:line`.

## Verification

- **Deterministic post-insert failure beats a race.** Racing two accepts with the same primary key lets the loser fail before it ever inserts. Use a test-only `AFTER INSERT` trigger that flips the later CAS target inside the transaction, then assert the inserted row is absent and the pre-existing id set is unchanged.
- **Mutation probe both ways.** Swap the seam for the self-committing trait call and confirm the assertion fails on exactly the distinguishing line; loosen each moved predicate and confirm its own test flips. Record the probe output; it is the evidence that the test is not tautological.

## Pitfalls

- **Column drift.** Replacing raw SQL with a store helper silently changes defaults: `updated_at` became `NULL`, and the helper populated `extensions_nexus_json` where the raw insert wrote `NULL`. Either pin the old value or assert the delta explicitly in the recorded-delta test.
- **Governance drift.** A store primitive that binds a record also binds its `holder_entry_id` / `disclosure` / `revision`, so a lane that must stay World-shared has to normalise those to `NULL` explicitly — the wire proposal it decoded may carry an owner the lane never accepted. Assert the stored `NULL/NULL`, not the schema default.
- **Error taxonomy drift.** Raw-SQL call sites often surfaced coded input errors; the converged path may collapse them into an internal error. Name the mapping explicitly or capture the delta as a residual.
- **`.sqlx` hashes the SQL text.** Re-indenting a moved compile-checked statement forces `cargo sqlx prepare`; keep the literal as-is or regenerate in the same change.
- **A per-line source scan shares its blind spot with the grep it replaces.** Line-wrapped or lowercased SQL escapes both; document the limit at the guard.

## Evidence

- `crates/nexus-local-db/tests/kb_key_blocks_writer_guard.rs` — the source guard.
- `crates/nexus-local-db/tests/actor_knowledge_maintenance.rs` — the `t4_primitive_*` cases cover the caller-transaction patch/delete primitives.
- `crates/nexus-core/tests/capability_compute.rs::aborted_accept_rolls_back_key_block_inserts` — the deterministic post-insert failure proving the settlement insert stays in the accept transaction.

## Byte-write port (files, not rows)

The same seam argument applies when the durable artifact is a file tree instead of a table row. Route the byte write through one named port, not through a helper call at the call site:

1. **Classify once at the boundary.** `Works/<work_ref>/<child>` is an artifact; `Works`, `Works/<ref>` without a child, and any `..` in the suffix are not writable; everything else keeps the old writer. Classification runs on the raw path after the workspace guard, so the seam does not silently widen to the whole workspace.
2. **Own `mkdir` + temp-write + fsync + rename inside the adapter.** Callers pass a relative child; the adapter constructs the absolute target, so classification and construction cannot disagree.
3. **Take the per-object lock the domain already uses.** `WorkLock` with a distinct holder label (`tool:fs_write`) is the concurrency authority; the tool hot path was previously outside it.
4. **Release on every path.** With no `Drop` guard, the shape is `let result = { .. }.await; lock.release().await; result` — a skipped release pins the object until the TTL.

### Verification

- **A canonical-path comparison is not containment.** Comparing a canonicalized destination against the allowed prefix accepts a lower symlink that resolves to the prefix's ancestor. Require the probed path to be lexically under the allowed subtree, and allow the workspace root / `Works` ancestor only when the probe itself is that real, non-symlink ancestor (the not-yet-created first-write case). Regress both alias destinations, not just an unrelated external directory.
- **Read-then-update lock acquisition is not exclusive.** `SELECT holder; if NULL then UPDATE` admits two winners. Bind one conditional `UPDATE` to the observed `(holder, acquired_at)` pair with NULL-safe `IS`, and elect on affected rows; keep stale eligibility as a separate pre-check. Test free-lock, distinct-holder and same-holder stale takeover — the same-holder case is the one a holder-only predicate still loses.

### Pitfalls

- **A holder-only CAS leaves the same-holder takeover open.** Stable holders (`daemon:schedule:<id>`) keep the predicate true after the first winner refreshes the timestamp.
- **A gated module can hide an unrun test.** When the crate declares an empty default feature set, the recorded selector must carry `--features <module>`; otherwise the evidence command compiles a different crate than the one claimed.
- **A grouped inventory is not a per-hit inventory.** When a gate requires every site classified, emit one row per `file:line` from the recorded scan output; crate totals and "…" shorthand are not auditable.

### Evidence

- `crates/nexus-core/src/execution/work_artifact.rs` — classification, port, adapter, boundary re-check.
- `crates/nexus-local-db/src/runtime_lock.rs` — conditional acquisition bound to `(holder, acquired_at)`.
- `crates/nexus-core/src/execution/capabilities.rs` — the `fs/write_text_file` branch; the `NonWork` branch is unchanged.

## Reaching a core-private port from another crate

When the port lives in a crate the writers cannot depend on (`nexus-orchestration` must not link
`nexus-core`), do not widen the port into a public API. Extend the trait both sides already share
(`WorkspaceExecutor`) with one method whose input/output structs live in `nexus-contracts`, give it
a default body that refuses, and implement it in core beside the production adapter. Builtins
receive the executor through the existing runtime-deps injection; a CLI caller gets one narrow
public accessor built from the same internal deps, never the whole deps struct.

- **A default-refusing trait method keeps every other impl untouched** — no 34-item churn, and an
  unwired capability fails closed instead of silently falling back to a local writer.
- **One construction site** (per dispatch, from the tool context) means the hot path and the
  builtin path cannot disagree about the adapter.
- **The registry arm is part of the seam.** A migrated builtin registered without the executor is
  an always-refusing capability; add a registry-level invocation test, not only a manual-injection
  test. A validation-only registry must be documented as such.

### One resolved root, or the seam is split

A port that reconstructs `Works/<ref>/` from a caller-supplied root, while the caller derives its
existence checks, `mkdir`, idempotency snapshot and rollback paths from the process cwd, addresses
two trees whenever those differ. The result is silent: re-init overwrites user content, and a
failed write cannot roll back the bytes that landed.

- Resolve the effective absolute root **once** at the start of the operation (from the executor's
  own workspace root, not cwd) and use that same value for reads, directories, returned paths,
  commit input and compensation.
- Validate a caller-supplied root against the executor's canonical root **before** any claim,
  `mkdir` or metadata write; refuse an external or nested root with zero mutation.
- If the port reconstructs a literal child (`Works/`), either carry the resolved works directory
  through the input or refuse roots whose last component is not that child, so a custom override
  cannot reintroduce the split.

### Recoverable ordering claims

When a write needs a pre-existing row identity (a `works.work_ref` normally patched after the
files), claiming it before the first byte is a real ordering change:

- publish only the identity the port needs, keep final profile/metadata at successful completion,
  and restore the prior value on every failed path;
- make the restore cancellation-safe — a guard that keeps its recovery state through the restore
  await (and retries from `Drop`) closes the window where an aborted future leaves the claim;
- predicate the restore on the claimed value so a concurrent writer's newer value is not clobbered;
- a partial re-init must use the **stored** effective ref for the port input, not the requested one,
  and the test must run against the real port, not a double that skips the lookup.

### Verification (cross-crate port)

- Production-shape test: default relative `works_root`, process cwd ≠ canonical root, first
  scaffold with `work_ref = NULL`; assert bytes land only in the canonical tree, a later refusal
  restores the canonical tree, and re-init preserves an existing user file.
- Refusal tests: unrelated absolute root, nested `Works`, and `..`-relative root are rejected with
  no directory, no commit and an unchanged row.
- Cancellation test: hold the port's DB connection so the restore cannot run, abort the future
  mid-await, then assert the prior ref returns from the guard's own task.

