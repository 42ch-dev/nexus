---
module: actor-runtime
date: 2026-09-07
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-06-v1.185-p3-run-memory-capture
applies_when:
  - adding-mutable-actor-resources
  - adding-lifecycle-transitions
  - adding-run-to-memory-capture
  - changing-session-admission
tags:
  - actor-runtime
  - revision-cas
  - lifecycle-epoch
  - activity-fence
  - character-busy
  - run-capture
  - receipt-dedup
  - sqlite
related_components:
  - nexus-local-db
  - nexus-daemon-runtime
  - nexus-agent-host
  - nexus-creator-memory
---

# Actor maintenance lifecycle and run capture

## Context

The v1.185 developer loop added material mutation and reversible freeze to the shipped Actor vertical: identity edit, archive/restore, WorldSheet maintenance, bounded KE content maintenance, and an opted-in run-to-memory capture. Three well-known concurrency mechanisms are individually insufficient here: SQLite row CAS does not cover Host sessions or file effects, a provider session can outlive a DB transaction, and a server-owned stream ends on the Host's schedule, not the request's. The pattern combines **per-resource revision CAS**, an **internal lifecycle epoch**, a **per-Character activity fence**, and a **drain-authoritative capture** with a receipt-keyed immutable dedup.

Normative product semantics: [Actor Product Model §11](../../specs/actor-product-model.md). The sibling composition doc covers the shipped v1.184 admission/bearer boundaries: [actor-bearer-boundary-composition.md](actor-bearer-boundary-composition.md).

Rejected option (kept as explicit non-goal): archive cancels all sessions, persists conversation/job history, and reconstructs capture from broadcast or SSE. Cancellation cannot atomically roll back provider or file effects, broadcasts are lossy, and persistent history is out of scope. Busy is refused, never forced.

## Guidance

### 1. Per-resource revision CAS with named conflict codes

Each mutable resource keeps its own nonnegative `revision` CAS token, initially `0`:

- Row revisions are independent: Character, binding, and KE each carry their own token. Timestamps are never concurrency tokens. A successful material edit increments exactly once; an identical-value patch and a same-state lifecycle request are CAS-checked no-ops that preserve revision and `updated_at`.
- `expected_revision` is validated to `0..=9223372036854775806` before the transaction, then re-checked in-transaction against the stored row. Model the CAS as `UPDATE … WHERE character_id = ? AND owner_creator_id = ? AND revision = ?` (or the equivalent row predicate), not as a read-then-conditional-write only.
- Conflict is a **named actor conflict** (`ActorContractConflict::CharacterRevisionConflict` / `BindingRevisionConflict` / `KnowledgeRevisionConflict`), never a generic `VersionMismatch` that the HTTP mapper could turn into `500 internal`. `ConflictCoded` keeps it a stable `409`.
- Map SQLite constraint failures onto stable codes by constraint name (`map_actor_constraint`): the partial unique index `idx_characters_owner_active_display_name` yields `duplicate_character_display_name`, the active KE unique indexes yield `duplicate_actor_knowledge`. A write path that skips this mapping surfaces a `500` exactly where the restore path surfaces `409`.
- Use **checked-macro static SQL** (`sqlx::query!` / `query_scalar!` with `as "revision!"`-style type assertions) in production code; dynamic `sqlx::query*` belongs in test fixtures only. `BEGIN IMMEDIATE` for every write transaction: it acquires the write reservation up front, so in-transaction revalidation does not upgrade mid-transaction.

### 2. Internal lifecycle epoch as a session fence

`lifecycle_epoch` is a separate durable integer, initially `0`, incremented exactly once per **material** archive or restore. It is never client-supplied, never an admission claim, and never bumped by metadata/WorldSheet edits or same-state lifecycle requests.

- The epoch becomes part of `ActorSessionKey`, so a post-restore admission can never reuse a pre-transition session; stored sessions from an old epoch must never resume the pre-archive conversation. `retire_character_sessions` removes old-epoch reuse keys and moves their ids into owner-retaining tombstones (owner + Actor kind retained, never legacy/Creator fallback). A retired session id fails `409 actor_session_stale` before any Host effect.
- **Re-read the epoch under the exclusive fence** (see §3). A session key or transition decision built from a pre-fence read is stale once another transition committed between the read and the fence acquisition. Corresponding regression: `transition_guard_rereads_epoch_under_exclusive_fence`.
- Compare the indexed session epoch against the fence epoch **after** activity admission and before MCA/Host: `stored.character_epoch != Some(guard.epoch())` → `actor_session_stale`. This closes the archive-races-the-prompt window.
- The generated wire records expose `revision` and `status` but never the epoch; it stays internal.

### 3. Per-Character activity fence: busy refusal, guard held across effects

One registry `RwLock` per Character (`character_fences: HashMap<String, Arc<RwLock<()>>>`) provides both admission modes:

- **Activity admission** (writes, runs, capture): an owned read guard. `admit_character_activity` owner-checks first (no fence state leaks existence), then acquires the read lock and **re-reads status and epoch under the fence** — the pair is exact for the guard's lifetime.
- **Exclusive transition** (archive/restore): `try_character_transition` uses `try_write_owned`; while any activity guard is outstanding it returns `409 character_busy` — zero mutation, no forced cancellation, no provider wait. Write lock acquisition must be `try_`, never an awaited write that a provider can pin.

The guard must be **held through every DB, file, provider, and terminal-capture effect** of the admitted activity, and for a Prompt through the server-owned drain until terminal finalization (the Host flips Busy→Ready before the daemon sees the terminal — a shorter hold would let archive slip into the capture window). Real gaps: mutation handlers that admit then drop the guard and continue, returning `character_busy` only for the next activity, and binding add/remove handlers left entirely unfenced.

Ordering for a lifecycle transition (`characters.rs` `execute_character_lifecycle`):

1. `try_character_transition` exclusive fence (busy refusal before DB),
2. `transition_character` `BEGIN IMMEDIATE` commit (CAS; revision and epoch each increment once on the material UPDATE),
3. while the fence is still held, `retire_character_sessions` **only when** the committed epoch differs from the guard's pre-transition epoch (a same-state CAS no-op must not discard a currently reusable session),
4. drop the exclusive fence, then one Host shutdown attempt per retired id outside registry locks (failure is logged and cannot undo the commit).

Fence entries with no live guard (`Arc::strong_count == 1`) are swept on admission; no timer. Cancel/shutdown are safety/observation actions and must **not** take the activity fence.

### 4. Drain-authoritative capture

The only capture authority is the **server-owned drain of the stream returned by `HostFacade::exec`** — not CLI SSE, not a broadcast subscriber, not model-written metadata or a caller-supplied digest. The admission snapshot (owner Creator, Character, binding, World, epoch, server-minted session and operation ids) is transferred into the drain along with the activity guard.

- `event_matches_operation` filters the drain to the exact stored session + operation; only `MessageDelta` text accumulates into the candidate digest (never thought/tool/plan events) and only `OpFinished(end_turn)` counts as a successful terminal. `MaxTokens` / `MaxTurnRequests` / `Refusal` are `Incomplete` — no candidate.
- The guard's epoch is re-checked at persist time against the snapshot epoch; a mismatch (or a gone binding) fails capture with `CaptureScopeChanged` **without** rewriting the successful run. Run success and capture failure are distinct outcomes, reported honestly; a failed capture never triggers a provider retry.
- `remember:true` requires a stored admitted Character session with an exact binding; Creator/legacy sessions reject `422 invalid_input` before Host execution. No caller-supplied actor, binding, digest, or operation id can claim capture authority.

### 5. Atomic cancel latch, bounded outcome memory

Per-operation phase machine: `Running → CancelRequested → Finalizing → Terminal`.

- `request_operation_cancel` latches `CancelRequested` before awaiting the Host cancel; a cancel after `Finalizing`/`Terminal` is `409 actor_operation_finished` — cancellation cannot erase capture after finalization.
- `begin_operation_finalizing` transitions to `Finalizing` and reads the cancel intent in one registry-lock step, so no window observes stale cancel state. The drain then computes run status/capture from `cancel_requested` + the accumulated terminal.
- Bounds: `MAX_NONTERMINAL_OPERATIONS` (128) is checked at registration — the next admit is `409 actor_operation_capacity`; terminal outcomes are a FIFO (`MAX_TERMINAL_OPERATIONS`). None of this is per-character unlimited memory.

### 6. Receipt-keyed immutable dedup, FK-backed provenance

`capture_character_run` is a single `BEGIN IMMEDIATE` transaction that inserts the **receipt first**, then the pending row:

- Receipt table DDL intent: `operation_id TEXT PRIMARY KEY`; `character_id` is a real `REFERENCES characters(character_id) ON DELETE RESTRICT`; `binding_id`/`pending_id`/`captured_at` are historical provenance **intentionally not FKs** to consumable rows; no output text column. Pending rows carry a non-null `source_operation_id` FK to the receipt.
- Idempotent replay: an existing exact-matching receipt is returned without reinsert; an existing receipt that does not match the replay input (same operation id, different scope/digest/epoch) is `run_capture_provenance_conflict` — an operation's capture is immutable.
- Inside the transaction: active owned Character, exact `lifecycle_epoch` equality, and binding provenance (`require_valid_provenance_tx`: owned binding, same Character, owned active World) are revalidated before insert — caller-supplied scope is never the authority.
- The receipt row **survives pending-row consumption** (review/dismiss/promote), so re-observing terminal events can never duplicate a candidate; and two operations sharing one Host session produce two distinct candidates (distinct operation ids → distinct receipts/pending rows).

### 7. Bounded digest accumulation

The candidate digest is `Prompt:\n<raw prompt>\n\nResponse:\n<accumulated message text>`, capped at `MAX_CAPTURE_DIGEST_BYTES = 65_536`. Accumulation uses **checked arithmetic** (`checked_add` on static overhead + prompt + buffered text + incoming delta); any overflow or over-cap flips `digest_too_large` before a delta is copied and yields `CaptureTooLarge` — never an oversized candidate. An empty (`trim()`-empty) response is `CaptureEmptyOutput`; it is never a candidate.

## Why this matters

Each mechanism covers what the others cannot. Row CAS protects the SQLite truth; the epoch protects Host session identity across a freeze; the fence makes archive/restore refuse instead of racing provider or file effects; the drain makes capture keyed to the exact server operation rather than a lossy observer; the receipt makes capture idempotent and immutable; the digest bound keeps the persisted candidate finite. Omitting any of them produces a concrete failure class: a `500` where a `409` is expected (unmapped constraint), a stale pre-archive session resuming after restore (unfenced epoch key), an archive that "succeeds" mid-capture (guard dropped early), a duplicate candidate after review (no receipt), or an oversized/overflowed digest (unchecked accumulation).

## When to apply

- Adding a mutable resource or a lifecycle transition to an Actor/Actor-adjacent surface (use §1–§3).
- Wiring a run or any provider stream to side-effectful capture (use §4–§7).
- Any code path whose effects cannot be rolled back by the database transaction (Host sessions, files, provider streams) but still needs per-entity serialization.
- Reconsider the fence boundaries whenever a new side-effecting handler is added — every mutation route needs its route-level busy-refusal regression.

## Examples

- Revision CAS + epoch + active gate ordering — `crates/nexus-local-db/src/character.rs` (`update_character`, `transition_character`, `map_actor_constraint`, `check_expected_revision`), mirror in `crates/nexus-local-db/src/actor_world_binding.rs`.
- Fence guards, epoch keys, phase machine — `crates/nexus-daemon-runtime/src/workspace/actor_sessions.rs` (`admit_character_activity`, `try_character_transition`, `retire_character_sessions`, `request_operation_cancel`, `begin_operation_finalizing`; tests `archive_try_refuses_while_activity_outstanding`, `transition_guard_rereads_epoch_under_exclusive_fence`, `retire_character_sessions_tombstones_owner_and_fresh_epoch_reuses_nothing`).
- Transition ordering under fence — `crates/nexus-daemon-runtime/src/api/handlers/characters.rs` (`execute_character_lifecycle` ordering comment).
- Drain, digest, persist — `crates/nexus-daemon-runtime/src/actor_run_capture.rs` (`drain_and_finalize_character_operation`, `try_persist_capture`, `DrainAccumulator`, `build_capture_digest`).
- Receipt dedup, provenance — `crates/nexus-local-db/src/character_pending_review.rs` (`capture_character_run`, `capture_character_run_in_tx`, `receipt_matches_input`), migration `crates/nexus-local-db/migrations/20260906000003_character_run_capture.sql`.
- Stale-session rejection and session admission order — `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs` (`prepare_prompt`, retired-session paths).
- Published slice — the v1.185 iteration package (`.mstar/iterations/v1.185/`) is a local process artifact; the durable authorities are [§11](../../specs/actor-product-model.md) plus the sources above.
