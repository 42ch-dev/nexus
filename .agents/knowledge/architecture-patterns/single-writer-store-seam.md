---
module: nexus-local-db / nexus-core / nexus-orchestration (World KB persistence)
date: 2026-11-11
last_updated: 2026-11-11
problem_type: architecture
category: architecture-patterns
severity: medium
plan_id: v233-knowledge-ir-p1
applies_when:
  - converging production code that writes a table directly onto one store seam
  - a handler already owns a transaction but the store trait method opens its own
  - proving that an insert stays inside a caller's transaction when a later step refuses
  - adding a source guard that keeps ad-hoc SQL out of other crates
related_components:
  - crates/nexus-local-db/src/kb_store.rs
  - crates/nexus-local-db/src/actor_knowledge_store.rs
  - crates/nexus-core/src/execution/compute.rs
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

A table had writers in four crates: a compute settlement insert, two orchestration capability updates, and a dynamic actor patch/delete. They worked, but every new caller had to rediscover the predicates (revision CAS, terminal-status exclusion, ownership conjunct). The existing `KbStore` trait could not host all of them: its methods open their own transaction, so a handler that already holds one would lose atomicity.

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
- **Error taxonomy drift.** Raw-SQL call sites often surfaced coded input errors; the converged path may collapse them into an internal error. Name the mapping explicitly or capture the delta as a residual.
- **`.sqlx` hashes the SQL text.** Re-indenting a moved compile-checked statement forces `cargo sqlx prepare`; keep the literal as-is or regenerate in the same change.
- **A per-line source scan shares its blind spot with the grep it replaces.** Line-wrapped or lowercased SQL escapes both; document the limit at the guard.

## Evidence

`.mstar/sdd/v233-knowledge-ir-p1/inventory.md`, `review/qc1.md`, `review/qc2.md`, `crates/nexus-local-db/tests/kb_key_blocks_writer_guard.rs`.
