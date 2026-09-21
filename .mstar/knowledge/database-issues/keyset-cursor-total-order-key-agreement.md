---
module: crates/nexus-local-db (kb_store keyset query) + crates/nexus-core (ActorView pagination)
date: 2026-09-21
problem_type: database_issue
category: database-issues
severity: medium
plan_id: 2026-09-21-v1.194-p2-retirement-residuals
symptoms:
  - "A one-row-per-page walk of the actor holder view intermittently ends one page early — `has_more` is false while an eligible row was never emitted"
  - "The failure depends on the stored sub-millisecond fraction, so it shows up in roughly one of five concurrent runs and disappears on a re-run"
  - "A green re-run is not evidence of a fix: production inserts persist `Utc::now().to_rfc3339()` nanoseconds, so which rows disagree with the cursor varies per run"
root_cause: "The keyset key was computed twice with different rounding rules — the SQL row/cursor key used SQLite `strftime('%f')` (which ROUNDS the sub-second fraction to three digits) while the in-process merge key `stored_created_at_order_millis` used chrono `timestamp_millis` (which TRUNCATES). For every row whose stored fraction sat at or above the half-millisecond boundary the two keys differed by one millisecond, so the cursor written from the SQL key excluded that row from the following page."
resolution_type: code_fix
tags:
  - sqlite
  - strftime
  - keyset-pagination
  - cursor
  - ordering-key
  - timestamp-millis
  - row-value-comparison
  - actor-view
related_components:
  - crates/nexus-knowledge
  - crates/nexus-spoke-adapter
---

# Keyset cursor and the in-process merge key must share one truncating rule

## Problem

`list_by_owner_keyset` (`crates/nexus-local-db/src/kb_store.rs`) pages owner-scoped knowledge rows through an opaque keyset cursor. It built the ordering key in SQL as

```sql
(CAST(strftime('%s', created_at) AS INTEGER) * 1000
 + CAST(substr(strftime('%f', created_at), 4) AS INTEGER))
```

while the caller — `paginate` in `crates/nexus-core/src/actor_knowledge.rs` — re-sorted the fetched window by `stored_created_at_order_millis` (`crates/nexus-knowledge/src/world_kb/knowledge_entry.rs`), which is chrono's `timestamp_millis`.

Those are not the same function. SQLite's `%f` **rounds** the sub-second fraction to three digits (`strftime('%f','2026-01-01T10:00:00.123999999Z')` → `00.124`; a probe of `.000400000 / .000500000 / .123500001 / .999999999` puts the round-up threshold at half a millisecond, and `.999x` is clamped inside its own second). `timestamp_millis` **truncates** (`.123999999` → `123`). Production constructors store `Utc::now().to_rfc3339()` with nanosecond precision, so every row whose fraction reached the boundary carried an SQL key one millisecond above the key the merge sorted it by — and which rows those were changed from run to run.

## Symptoms

- `has_more` reports false while an eligible row remains: a `limit = 1` walk emits four pages where five rows were eligible.
- The failing row is a *different* row across runs; the failure rate tracks how many rows land at or above the rounding boundary.
- Concurrent execution raises the frequency (one failure in five runs of the caller's test cohort at four threads) without being the cause; a serialized run can pass indefinitely on lucky fractions.

## The failure chain

The cursor is produced by SQL but consumed as the Rust key, so a one-millisecond disagreement is enough to lose a row:

1. Two rows truncate into the same millisecond and one rounds up in SQL. SQL orders them `[down, up]`; the Rust merge orders them `[up, down]` whenever `key_block_id(up) < key_block_id(down)`.
2. Page 1 returns the merge's first row (`up`). The next page's cursor carries **SQL's** key for that row — `ms + 1`.
3. The row that did not round up has SQL key `ms`, so `(key, key_block_id) > (ms + 1, cursor_id)` excludes it for good. The walk then terminates with a short final page and the row is never paged.

## What Didn't Work

- **Treating it as a fixture or concurrency defect.** The CI mitigation at the time was an actor-specific `--test-threads=2` pin on the core-domain job. Serializing tests lowers the *frequency* of the symptom, not the mechanism: the key mismatch is a function of the stored bytes, and a serialized walk over rows that all fall below the boundary passes.
- **Re-running until green.** The reported failure was taken as fact rather than re-confirmed; a green re-run proves only that this run's generated fractions stayed below the boundary.
- **Trusting the doc comment.** `stored_created_at_order_millis` documented the `%f` formula as *the* matching conversion. The comment asserted agreement; the two implementations disagreed. A doc comment is not evidence of arithmetic equivalence.
- **Widening the test or adding a global mutex.** A global test mutex or a wider thread pin would have hidden a production row-skip behind a scheduling change.

## Solution

One truncating conversion, applied to both the row key and the cursor key:

1. A private SQL helper reads the digits off the **stored bytes** and truncates them, so its value equals `timestamp_millis` for every supported layout (RFC3339 `Z` and `+00:00`, SQLite `datetime('now')`, `…SS.SSS`):

```sql
fraction_millis(expr) =
  (CASE WHEN instr(expr,'.') = 0 THEN 0
   ELSE CAST(substr(substr(expr, instr(expr,'.')+1) || '000',1,3) AS INTEGER) END)
```

2. `created_key` and `cursor_key` are both built from it.
3. The cursor predicate became a row-value comparison — `AND ({created_key}, key_block_id) > ({cursor_key}, ?)` — i.e. the `>` / `=` + `>` expansion without repeating the cursor expression. Placeholder count (4 × `created_at` + 1 × `entry_id`) and the existing bind loop are unchanged; the row-value form needs SQLite ≥ 3.15, which the bundled engine satisfies.

Contract preserved: the stored `created_at` bytes are never rewritten (no production writer touched); the cursor stays the opaque `k2:<created_at>\u{1f}<key_block_id>` token; the `key_block_id` tie-break is unchanged; the visibility conjunct still filters hidden rows before cursor and `LIMIT`. Fixed in `0e9f5771f` (`crates/nexus-local-db/src/kb_store.rs`, a doc correction in `crates/nexus-knowledge/src/world_kb/knowledge_entry.rs`, and the regression below).

## Why This Works

The cursor is authored by SQL and consumed by Rust, so the two sides must agree on a **total** order over the stored representation. Deriving both keys from one truncating rule makes them equal for every supported layout by construction rather than by coincidence:

- the cursor carries exactly the key the merge used for the row it points at, so "everything strictly after this key in the same order" is well defined;
- nothing depends on how SQLite would have rounded, so the row set no longer varies with the nanosecond fraction that the writer happened to produce;
- keeping the tie-break on `key_block_id` and the fields of the cursor token means no consumer, cache or stored test fixture had to change.

The row-value form is not cosmetic: the previous `>` / `=` + `>` expansion repeated the cursor expression, so every extra use of the key also had to be mirrored in the bind loop. A single tuple comparison keeps the SQL and the bind count in step.

## Prevention

- **One rule per ordering key.** If a key is produced by SQL and compared against an in-process value, they must be derived from the same definition — ideally the process computes the key and passes it in, or SQL is provably truncating/rounding identically. Two independent implementations of "milliseconds" is a latent row-skip, not a style question.
- **Check the DB time function's fidelity before reusing it as a key.** SQLite `%f` rounds to three digits; `.999…` clamps inside its second; `strftime('%s')` drops the fraction. None of these match a language-side millisecond truncation by default.
- **Make the regression deterministic instead of flaky-shaped.** Pin the stored fractions in the fixture and assign the boundary rows **by id** so the two orderings are forced apart for any generated ids, then assert page count plus the exact eligible set. A boundary case that can pass by luck is not a regression test.
- **Prove red → green.** The regression must fail against the unfixed revision (it did: four pages, the larger-`key_block_id` boundary row missing) and pass after.
- **Prefer the row-value comparison.** It keeps the placeholder count stable when the key expression grows.
- **Retire the scare-mitigation.** Once the mechanism is repaired, the actor cohort runs at four threads on the dedicated step, while the non-actor owners keep their own `--test-threads=2`; a thread pin that exists to avoid a production defect should not outlive the defect.

## When to Apply

- Any keyset/cursor pagination over a timestamp column whose stored precision is finer than the cursor's granularity.
- Any query whose ordering key is computed in SQL and re-derived in the process — pagination, merge joins, dedupe windows, conflict resolution.
- Reviewing a "flaky pagination"/"missing row in list" report: compare the SQL key expression against the in-process key function before suspecting the fixture.
- Writing a regression for a time-dependent ordering defect: control the stored bytes, not the clock.

## Coverage notes

- The total-order conversion still exists twice (the SQL helper and `stored_created_at_order_millis`); both now truncate and both doc comments warn against `strftime('%f')`. Collapsing them into one carried key would change `list_by_owner_keyset`'s signature and its other caller, and is tracked as a maintainability follow-up rather than a correctness gap.
- A `created_at` value SQLite cannot parse still yields a NULL SQL key (NULL sorts first; with a cursor the comparison is NULL → excluded) and is reported through the existing invalid-timestamp carrier. Unchanged by this fix.
- `crates/nexus-spoke-adapter`'s `scope_query_port` is the other `list_by_owner_keyset` caller; its module selector was run with the change.

## Evidence

- `crates/nexus-local-db/src/kb_store.rs` — `fraction_millis`, `list_by_owner_keyset` (row key, cursor key, row-value predicate).
- `crates/nexus-knowledge/src/world_kb/knowledge_entry.rs` — `stored_created_at_order_millis` (truncating side) and the corrected doc comment.
- `crates/nexus-core/tests/actor_services.rs` — `set_created_at` helper and `v1191_holder_visibility_actor_view_boundary_fraction_pages_every_row`.
- Runtime proof: RED at the pre-fix revision (four pages, larger-id boundary row missing), GREEN after the fix; the four-test `v1191_holder_visibility_` cohort at `--test-threads=4` four times (16/16 executions), `kb_owner_store` 12 passed, `kb_store` lib 44 passed, timestamp-parse and scope-query selectors green.
- Sibling reads for the read-side `has_more`/pagination family: [pagination-cursor-without-total-count-labels.md](../architecture-patterns/pagination-cursor-without-total-count-labels.md) (honest "N+" labels) and [bounded-drain-completion-contract.md](../architecture-patterns/bounded-drain-completion-contract.md) (`has_more` must reflect advancement, not attempts). Both cover *what* `has_more` should mean; this note covers *how the cursor's key is computed*, which is the failure mode that makes `has_more` report false while a row remains.
