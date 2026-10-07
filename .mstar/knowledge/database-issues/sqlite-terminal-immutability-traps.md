---
module: nexus-local-db (writer protocol, receipts)
date: 2026-10-07
problem_type: database_issue
category: database-issues
severity: medium
plan_id: 2026-10-07-v1.207-p3-operation-receipts
symptoms:
  - "An admitted writer could rewrite a settled (terminal) receipt row through INSERT OR REPLACE with an explicit rowid, or UPDATE OR REPLACE SET rowid, bypassing the BEFORE UPDATE/DELETE guards"
  - "A batch error before receipt settlement was treated as 'no effect committed' although the sequential upsert had already committed earlier entries"
  - "Outbox row pruning could erase the row evidence used to observe that an effect had committed"
root_cause: "SQLite REPLACE deletes the uniqueness-conflict victim WITHOUT firing its DELETE trigger unless recursive_triggers is ON; rowid is a second uniqueness key reachable from SQL; and row-retention is not durable evidence of a commit."
resolution_type: code_fix
tags:
  - sqlite
  - without-rowid
  - replace
  - triggers
  - immutability
  - writer-protocol
  - autoincrement
  - receipts
related_components:
  - nexus-local-db
  - nexus-core
  - apps/nexus42
last_updated: 2026-10-07
---

# SQLite terminal-immutability + effect-observation traps (v1.207 receipts)

## Problem

The v1.207 `operation_receipts` table must keep **terminal rows immutable**
on every admitted path (a rewritten receipt would be served as the replay
answer), and the settlement path must know whether an effect had **actually
committed** when a receipt write fails. Three separate SQLite mechanics broke
both assumptions; all three were found by L3/QC review of the implementation.

## Symptoms

- Terminal rows were writable through `INSERT OR REPLACE` with a **fresh
  primary key but the victim's explicit rowid**, and through `UPDATE OR
  REPLACE SET rowid = <terminal rowid>` from a running row — both bypassed the
  BEFORE UPDATE/BEFORE DELETE fences (with `recursive_triggers` OFF, the
  REPLACE victim's DELETE trigger never fires).
- A rejected multi-entry upsert (entry A committed, entry B refused) surfaced
  as a plain `Err`; the receipt error arm read that as "no effect" although a
  durable mutation had landed.
- A successful `COUNT` over retained `core_changes` rows returned zero after
  another writer pruned the rows — erasing the commit evidence the observer
  used.

## What Didn't Work

- **Guarding only `operation_id`** in the BEFORE INSERT/UPDATE triggers: the
  rowid alias is a second uniqueness key, so the fence was bypassable without
  touching `operation_id`.
- **Enabling `recursive_triggers` per connection**: the pragma is
  connection-scoped and changes nested-trigger behaviour for *every* guarded
  table (the outbox families insert into `core_changes`, whose own guard
  would newly fire); it could not be made mandatorily unbypassable under the
  cooperative writer contract.
- **Treating row retention as commit evidence**: retention is a bounded
  resource; any admitted writer may prune rows at any time.

## Solution

1. **Make the rowid alias unreachable**: `operation_receipts` is
   `WITHOUT ROWID` (identity = `operation_id`), removing the bypass class
   instead of guarding a second key.
2. **Cover the REPLACE path at the schema boundary**:
   - the BEFORE INSERT guard also refuses an insert over an existing
     **terminal** id (BEFORE INSERT fires *before* REPLACE conflict
     resolution, so the victim is never deleted);
   - a key-immutability trigger refuses any receipt-key change on UPDATE
     (closes settlement hijack and `UPDATE OR REPLACE` victim deletion).
3. **Translate the guard refusal in `begin_operation`**: match the
   terminal-refusal error on its insert, re-read, and resolve to `Existing`
   (same fingerprint) or typed `operation_id_conflict` (different) — never a
   raw trigger error.
4. **Observe commits from the monotonic counter, not rows**:
   `core_changes.sequence` is `INTEGER PRIMARY KEY AUTOINCREMENT`;
   `sqlite_sequence.seq` is the never-lowered high-water of every insert ever
   made. Watermark before the effect, compare after; an unreadable watermark
   or counter resolves **conservatively to effect-may-have-committed**
   (typed not-retryable), never to "no effect". A pristine outbox
   (`core_changes` absent) means counter 0 — accurately no-effect.

```sql
-- WITHOUT ROWID kills the alias; BEFORE INSERT refuses over-terminal;
-- a key-immutability trigger refuses UPDATE ... SET operation_id.
CREATE TABLE operation_receipts (...) WITHOUT ROWID;
CREATE TRIGGER guard_operation_receipts_insert BEFORE INSERT ON operation_receipts ...
CREATE TRIGGER immutable_receipt_key_operation_receipts_update BEFORE UPDATE OF operation_id ...
```

## Why This Works

- REPLACE's victim deletion is the whole attack; removing rowid removes the
  second key, and the BEFORE INSERT guard stops the remaining variant before
  the conflict resolution runs.
- The AUTOINCREMENT high-water is a property of the *sequence allocator*, not
  of retained rows — pruning cannot lower it, so the observation is
  retention-proof.
- Resolving unverifiable observations toward "may have committed" keeps the
  failure direction safe: the cost of a false positive is a not-retryable
  answer for an operation that possibly didn't commit, never a double-apply.

## Prevention

- For any table whose rows must be immutable after a terminal state:
  consider `WITHOUT ROWID`; cover INSERT / UPDATE / DELETE **and REPLACE**
  variants in the guard design; and add explicit REPLACE-path regressions
  (explicit-rowid insert, `UPDATE OR REPLACE SET rowid`).
- Never infer "no effect" from `Result::Err` over an engine whose write paths
  commit per entry; carry observed commit facts (counter/watermark) instead.
- Prefer monotonic counters over row evidence for any observation that must
  survive retention.

Regression pins (v1.207 P3): `replacement_insert_cannot_overwrite_a_terminal_receipt`,
`update_replace_cannot_hijack_a_terminal_receipt`,
`replaying_a_terminal_receipt_never_reinserts_it`,
`begin_resolves_a_racing_settled_insert_instead_of_erroring`,
`pruned_outbox_rows_cannot_erase_a_committed_effect`.

Source: `.mstar/sdd/2026-10-07-v1.207-p3-operation-receipts/` (task-3 review
F3/F4/F5 + C001 history); spec §B.1–§B.3.
