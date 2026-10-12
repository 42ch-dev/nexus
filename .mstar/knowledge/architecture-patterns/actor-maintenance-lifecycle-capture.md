---
module: actor-runtime
date: 2026-09-07
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-06-v1.185-p3-run-memory-capture
applies_when:
  - adding-mutable-actor-resources
  - adding-lifecycle-transitions
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
  - nexus-agent-host
  - nexus-creator-memory
---

# Actor maintenance lifecycle and run capture

> **Current scope.** The durable mechanisms in §1–§3 and §5–§6 (per-resource revision CAS, lifecycle epoch, per-Character activity/transition fence, the operation phase machine, and the receipt-keyed dedup rows) are live. The **run-to-memory capture writer** described in §4 and §7 is **historical (v1.185)** and is not reachable from the current Host: Character `remember:true` is refused before any effect, so no drain/digest writer runs. That historical writer and its example symbols are retained only as the design record.

## Context

The v1.185 developer loop added material mutation and reversible freeze to the shipped Actor vertical: identity edit, archive/restore, WorldSheet maintenance, bounded KE content maintenance, and an opted-in run-to-memory capture. Three well-known concurrency mechanisms are individually insufficient here: SQLite row CAS does not cover Host sessions or file effects, a provider session can outlive a DB transaction, and a server-owned stream ends on the Host's schedule, not the request's. The pattern combines **per-resource revision CAS**, an **internal lifecycle epoch**, and a **per-Character activity fence**; the run-to-memory capture (a v1.185 **historical** drain-authoritative capture with a receipt-keyed immutable dedup, see §4/§7) is currently disabled.

Non-normative product semantics: [Actor Product Model §11](../../specs/active/architecture/actor-product-model.md). The current Host completion limit for capture is stated at §6.1 there: the Host has a reservation but no complete run-capture writer, so Character `remember:true` is refused. The sibling composition doc covers the shipped v1.184 admission/bearer boundaries: [actor-bearer-boundary-composition.md](actor-bearer-boundary-composition.md).

Rejected option (kept as explicit non-goal): archive cancels all sessions, persists conversation/job history, and reconstructs capture from broadcast or SSE. Cancellation cannot atomically roll back provider or file effects, broadcasts are lossy, and persistent history is out of scope. Busy is refused, never forced.

## Guidance

### 1. Per-resource revision CAS with named conflict codes

Each mutable resource keeps its own nonnegative `revision` CAS token, initially `0`:

- Row revisions are independent: Character, binding, and KE each carry their own token. Timestamps are never concurrency tokens. A successful material edit increments exactly once; an identical-value patch and a same-state lifecycle request are CAS-checked no-ops that preserve revision and `updated_at`.
- `expected_revision` is validated to `0..=9223372036854775806` before the transaction (`check_expected_revision`, `crates/nexus-local-db/src/character.rs:84`), then re-checked in-transaction against the stored row. Model the CAS as `UPDATE … WHERE character_id = ? AND owner_creator_id = ? AND revision = ?` (or the equivalent row predicate), not as a read-then-conditional-write only.
- Conflict is a **named actor conflict** (`ActorContractConflict::CharacterRevisionConflict` / `BindingRevisionConflict` / `KnowledgeRevisionConflict`, `crates/nexus-local-db/src/error.rs`), never a generic `VersionMismatch` that the HTTP mapper could turn into `500 internal`. `ConflictCoded` keeps it a stable `409`.
- Map SQLite constraint failures onto stable codes by constraint name (`map_actor_constraint`, `crates/nexus-local-db/src/character.rs:863`): the partial unique index `idx_characters_owner_active_display_name` yields `duplicate_character_display_name`, the active KE unique indexes yield `duplicate_actor_knowledge`. A write path that skips this mapping surfaces a `500` exactly where the restore path surfaces `409`.
- Use **checked-macro static SQL** (`sqlx::query!` / `query_scalar!` with `as "revision!"`-style type assertions) in production code; dynamic `sqlx::query*` belongs in test fixtures only. `BEGIN IMMEDIATE` for every write transaction: it acquires the write reservation up front, so in-transaction revalidation does not upgrade mid-transaction.

### 2. Internal lifecycle epoch as a session fence

`lifecycle_epoch` is a separate durable integer, initially `0`, incremented exactly once per **material** archive or restore. It is never client-supplied, never an admission claim, and never bumped by metadata/WorldSheet edits or same-state lifecycle requests.

- The epoch becomes part of `ActorSessionKey`, so a post-restore admission can never reuse a pre-transition session; stored sessions from an old epoch must never resume the pre-archive conversation. `retire_character_sessions` (`crates/nexus-core/src/actor_sessions.rs:1085`) removes old-epoch reuse keys and moves their ids into owner-retaining tombstones (owner + Actor kind retained, never legacy/Creator fallback). A retired session id fails `409 actor_session_stale` before any Host effect (`actor_session_stale`, `crates/nexus-core/src/actor_sessions.rs:87`).
- **Re-read the epoch under the exclusive fence** (see §3). A session key or transition decision built from a pre-fence read is stale once another transition committed between the read and the fence acquisition. `CoreService::acquire_actor_activity` and `CoreService::acquire_character_transition` (`crates/nexus-core/src/actors.rs:297,340`) both re-read ownership and `lifecycle_epoch` after acquiring their lease for exactly this reason.
- Compare the indexed session epoch against the fence epoch **after** activity admission and before Host effect: a mismatch is `actor_session_stale`. This closes the archive-races-the-prompt window.
- The generated wire records expose `revision` and `status` but never the epoch; it stays internal.

### 3. Per-Character activity fence: busy refusal, guard held across effects

The fence table `character_fences: ActorFenceTable` (`crates/nexus-core/src/service.rs:115`) provides both admission modes; the fence mechanics live in `crates/nexus-core/src/actor_fence.rs` (`try_acquire_activity`:223, `try_acquire_transition`:234). Two layers per Character: a same-process `RwLock`, plus a per-Character OS shared/exclusive file lock, so a direct CLI transition fences against a live service.

- **Activity admission** (writes, runs, and historical capture): an owned shared lease via `CoreService::acquire_actor_activity` (`actors.rs:297`). Owner-check first (no fence state leaks existence), then acquire the read lock and **re-read status and epoch under the fence** — the pair is exact for the lease's lifetime.
- **Exclusive transition** (archive/restore): `CoreService::acquire_character_transition` (`actors.rs:340`) takes the exclusive lease; while any activity lease is outstanding it returns `409 character_busy` (`busy_error`, `actor_fence.rs:342`) — zero mutation, no forced cancellation, no provider wait. Acquisition is `try_`, never an awaited write that a provider can pin.

The lease must be **held through every DB, file, provider, and terminal effect** of the admitted activity. Real gaps: mutation handlers that admit then drop the lease and continue, returning `character_busy` only for the next activity, and binding add/remove handlers left entirely unfenced.

Ordering for a lifecycle transition. `CoreService::transition_character` (`actors.rs:436-456`) is the one-call convenience that **only** acquires the exclusive lease and commits the transition — its doc comment explicitly excludes Host session retirement. A host that must retire old-epoch sessions composes the same two steps and then continues:

1. `acquire_character_transition` exclusive lease (busy refusal + epoch/ownership re-read, before DB),
2. `commit_character_transition` (`actors.rs:375`) — the revision-checked `BEGIN IMMEDIATE` commit (revision and epoch each increment once on the material UPDATE).

Steps 3–4 are the **caller/Host integration requirement**, not behaviour of `transition_character` itself:

3. while the lease is still held, `retire_character_sessions` **only when** the committed epoch differs from the lease's pre-transition epoch (a same-state CAS no-op must not discard a currently reusable session),
4. drop the exclusive lease, then one Host shutdown attempt per retired id outside registry locks (failure is logged and cannot undo the commit).

Leases with no live holder are swept on admission; no timer. Cancel/shutdown are safety/observation actions and must **not** take the activity lease.

### 4. Drain-authoritative capture (historical — v1.185)

> **Historical.** This section records the v1.185 run-capture design. The writer it described (`drain_and_finalize_character_operation`, `try_persist_capture`, `DrainAccumulator`, `build_capture_digest` in the former `crates/nexus-core/src/execution/lifecycle.rs`) does not exist at HEAD. Capture is currently refused: the Host has a reservation but no complete run-capture writer, so Character `remember:true` is refused before any effect (`not_supported` in core, `crates/nexus-core/src/host.rs:1066-1075`; HTTP `501 route_not_migrated`, `apps/nexus-service/src/provider.ts:456-462`); non-Character `remember:true` stays `422 invalid_input`.

The only capture authority was the **server-owned drain of the stream returned by `HostFacade::exec`** — not CLI SSE, not a broadcast subscriber, not model-written metadata or a caller-supplied digest. The admission snapshot (owner Creator, Character, binding, World, epoch, server-minted session and operation ids) was transferred into the drain along with the activity lease.

- The drain was filtered to the exact stored session + operation; only `MessageDelta` text accumulated into the candidate digest (never thought/tool/plan events) and only `OpFinished(end_turn)` counted as a successful terminal. `MaxTokens` / `MaxTurnRequests` / `Refusal` were `Incomplete` — no candidate.
- The lease epoch was re-checked at persist time against the snapshot epoch; a mismatch (or a gone binding) failed capture with `CaptureScopeChanged` (now `ActorContractConflict::RunCaptureScopeChanged`, `crates/nexus-local-db/src/error.rs`) **without** rewriting the successful run. Run success and capture failure were distinct outcomes.
- `remember:true` required a stored admitted Character session with an exact binding; a foreign actor/session still fails validation before Host execution. No caller-supplied actor, binding, digest, or operation id claimed capture authority.

### 5. Atomic cancel latch, bounded outcome memory

Per-operation phase machine: `Running → CancelRequested → Finalizing → Terminal` (`OperationPhase`, `crates/nexus-core/src/actor_sessions.rs:116-118`).

- `request_operation_cancel` (`actor_sessions.rs:833`) latches `CancelRequested` before awaiting the Host cancel; a cancel after `Finalizing`/`Terminal` is `actor_operation_finished` (`interrupted`) — cancellation cannot erase capture after finalization.
- `begin_operation_finalizing` (`actor_sessions.rs:950`) transitions to `Finalizing` and reads the cancel intent in one registry-lock step, so no window observes stale cancel state. The settle then computes run status from `cancel_requested` + the accumulated terminal.
- Bounds: `MAX_NONTERMINAL_OPERATIONS` (128) is checked at registration (`actor_sessions.rs:222,721`) — the next admit is refused as `CoreError::Busy`; terminal outcomes are a FIFO bounded by `MAX_TERMINAL_OPERATIONS` (`actor_sessions.rs:223`). None of this is per-character unlimited memory.

### 6. Receipt-keyed immutable dedup, FK-backed provenance

`capture_character_run` (`crates/nexus-local-db/src/character_pending_review.rs:212`) is a single `BEGIN IMMEDIATE` transaction that inserts the **receipt first**, then the pending row; `capture_character_run_in_tx` (:231) is the in-transaction body and `receipt_matches_input` (:102) decides replay equality. Provenance is validated by `require_valid_provenance_tx` (`crates/nexus-local-db/src/actor_world_binding.rs`).

- Receipt table DDL intent: `operation_id TEXT PRIMARY KEY`; `character_id` is a real `REFERENCES characters(character_id) ON DELETE RESTRICT`; `binding_id`/`pending_id`/`captured_at` are historical provenance **intentionally not FKs** to consumable rows; no output text column. Pending rows carry a non-null `source_operation_id` FK to the receipt.
- Idempotent replay: an existing exact-matching receipt is returned without reinsert; an existing receipt that does not match the replay input (same operation id, different scope/digest/epoch) is `run_capture_provenance_conflict` (`ActorContractConflict::RunCaptureProvenanceConflict`) — an operation's capture is immutable.
- Inside the transaction: active owned Character, exact `lifecycle_epoch` equality, and binding provenance are revalidated before insert — caller-supplied scope is never the authority.
- The receipt row **survives pending-row consumption** (review/dismiss/promote), so re-observing terminal events can never duplicate a candidate; and two operations sharing one Host session produce two distinct candidates (distinct operation ids → distinct receipts/pending rows).

> **Reachability note.** These rows and their writer still exist for the memory owner, but no current Host path produces them: Character `remember:true` is refused before any effect (§4). Reintroducing capture requires a separately accepted complete integration of the memory owner, not a second pipeline (Actor Product Model §6.1).

### 7. Bounded digest accumulation (historical — v1.185)

> **Historical.** Part of the retired §4 capture writer.

The candidate digest is `Prompt:\n<raw prompt>\n\nResponse:\n<accumulated message text>`, capped at `MAX_CAPTURE_DIGEST_BYTES = 65_536`. Accumulation used **checked arithmetic** (`checked_add` on static overhead + prompt + buffered text + incoming delta); any overflow or over-cap flips `digest_too_large` before a delta is copied and yields `CaptureTooLarge` — never an oversized candidate. An empty (`trim()`-empty) response is `CaptureEmptyOutput`; it is never a candidate.

## Why this matters

Each mechanism covers what the others cannot. Row CAS protects the SQLite truth; the epoch protects Host session identity across a freeze; the fence makes archive/restore refuse instead of racing provider or file effects; the receipt makes capture idempotent and immutable. Omitting any of them produces a concrete failure class: a `500` where a `409` is expected (unmapped constraint), a stale pre-archive session resuming after restore (unfenced epoch key), an archive that "succeeds" mid-activity (lease dropped early), a duplicate candidate after review (no receipt), or an oversized/overflowed digest (unchecked accumulation — historical).

## When to apply

- Adding a mutable resource or a lifecycle transition to an Actor/Actor-adjacent surface (use §1–§3).
- Any code path whose effects cannot be rolled back by the database transaction (Host sessions, files, provider streams) but still needs per-entity serialization.
- Reconsider the fence boundaries whenever a new side-effecting handler is added — every mutation route needs its route-level busy-refusal regression.

## Examples

- Revision CAS + epoch + active gate ordering — `crates/nexus-local-db/src/character.rs` (`update_character`:634, `transition_character`:700, `map_actor_constraint`:863, `check_expected_revision`:84), mirror in `crates/nexus-local-db/src/actor_world_binding.rs`.
- Fence leases, epoch keys, phase machine — `crates/nexus-core/src/actor_fence.rs` (`ActorFenceTable`, `try_acquire_activity`:223, `try_acquire_transition`:234) driven by `crates/nexus-core/src/actors.rs` (`acquire_actor_activity`:297, `acquire_character_transition`:340, `commit_character_transition`:375); session retirement, cancel latch and finalization in `crates/nexus-core/src/actor_sessions.rs` (`retire_character_sessions`:1085, `request_operation_cancel`:833, `begin_operation_finalizing`:950).
- Transition ordering under the lease — `CoreService::transition_character` (`crates/nexus-core/src/actors.rs:446`) composing acquire + commit.
- Historical (v1.185, retired): drain/digest/persist writer — the former `crates/nexus-core/src/execution/lifecycle.rs` (`drain_and_finalize_character_operation`, `try_persist_capture`, `DrainAccumulator`, `build_capture_digest`); stale-session rejection for the prompt path — the former `prepare_prompt` in `crates/nexus-core/src/host.rs`. The current stale-session refusal is `actor_session_stale` (`crates/nexus-core/src/host.rs:1082-1125`).
- Receipt dedup, provenance — `crates/nexus-local-db/src/character_pending_review.rs` (`capture_character_run`:212, `capture_character_run_in_tx`:231, `receipt_matches_input`:102), migration `crates/nexus-local-db/migrations/20260906000003_character_run_capture.sql`.
- Published slice — the v1.185 iteration package is a local process artifact (not tracked at HEAD); the durable authority is [§11](../../specs/active/architecture/actor-product-model.md) plus the sources above.
