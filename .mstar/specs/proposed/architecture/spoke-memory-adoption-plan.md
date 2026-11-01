---
module: nexus spoke-memory adoption (rollout stages NXM-*)
date: 2026-10-28
status: proposed
problem_type: design
category: architecture
severity: info
tags:
  - spoke
  - memory
  - adoption
  - roadmap
  - capability-family
---

# Nexus Adoption Plan for the SPOKE Memory Protocol

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not authorization to execute. Nothing here is registered as a workflow, no stage is scheduled, and no feature is claimed shipped. Every "Current" statement is a snapshot of the repository as read on 2026-10-28. |
| **Document class** | Exploration |
| **Scope** | How nexus would adopt the `spoke-memory` family: the two-layer landing model (protocol adoption ≠ data merge), the target architecture (`MemoryPort` in-process provider + application authorization adapter), the caller-migration inventory, the staged roadmap `NXM-*` with its gates, the deferred triggers, and the adoption-program definition of done. |
| **Coordinates with** | [spoke-memory-protocol.md](spoke-memory-protocol.md) (wire symbols and `MEM-R###` invariants — referenced, never redefined here), [spoke-memory-conformance.md](spoke-memory-conformance.md) (`C-*` scenarios and the acceptance matrix this plan's stages trace to), [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md) (the consumption boundary and port-delegation rules the provider must fit) |

**Landing reconciliation (2026-11-01).** This plan was written against SPOKE 0.14.1, where the family was still proposed. Upstream 0.15.0 (2026-10-30) landed it additively and 0.15.1 (2026-10-31) is docs/CI only; crates.io and npm publish 0.15.1, so the protocol side of the two-layer model is now available to consume. The reference policy engine stays `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`, path-dependency-only, unpublished) — provider-private implementation, not a published protocol participant, so plans that treat it as an upstream dependency must vendor or reimplement rather than depend on it. This nexus repository has since moved its five lockstep pins to 0.15.1 (the re-pin also carries the upstream generated-type rename sweep in its consumers), but the adoption stages below remain unscheduled and unconsumed; upstream task IDs from the discarded implementation plan are replaced by "already landed upstream".

### Original framing (research provenance)

> This plan was **not** authorization to execute, does not register any workflow, and did not claim any feature shipped. Wire-level symbols are owned by [spoke-memory-protocol.md](spoke-memory-protocol.md) (naming contract v2.0, 2026-10-28) and are **referenced, never redefined** here. Upstream implementation task IDs cited as dependencies were owned by the upstream implementation plan, which is superseded by the landed 0.15.0 shape and is deliberately not promoted: the family now exists, so a staging plan for building it is dead weight.

## 1. Purpose and scope

Nexus adopts the SPOKE Memory capability family as: (a) the **default local
provider** of the protocol, implemented in-process over the existing Nexus
storage; and (b) the **application authorization and policy adapter** that
fronts every memory consumer in this repository (CLI, native napi surface,
local TS service, moment-context-assembly, Host/capability callers) and routes
them through one provider boundary. Remote (cloud or third-party) providers
implement the same service semantics over SPOKE Connect.

Non-goals for this document: defining wire fields (owned by
[spoke-memory-protocol.md](spoke-memory-protocol.md)), changing SPOKE source
(upstream owns its own releases), executing any task below, and re-enabling the
currently refused automatic run-capture path (§6.4, Decision D-4).

## 2. Protocol symbol reference (frozen names, cited from the protocol spec)

> Shorthand used by the `Owner` column and body prose: `01` =
> [spoke-memory-protocol.md](spoke-memory-protocol.md); `04` =
> [spoke-memory-conformance.md](spoke-memory-conformance.md); `02` = the
> upstream implementation plan, **not tracked here** — it is superseded by the
> landed 0.15.0 shape, so its task series (A1–A3, B1–B4, C1–C3, D1–D5, E1–E4,
> F1–F2) is history, not a dependency.

| Symbol | Meaning | Owner |
|---|---|---|
| `spoke-memory` | Umbrella capability flag gating ALL memory operations | 01 |
| `spoke-memory-reflection` | Sub-capability: reflection state/list/generate | 01 |
| `spoke-memory-transfer` | Sub-capability: exports/imports | 01 |
| `memory_protocol_version` | Wire integer protocol version, starts at 1 | 01 |
| `engine_policy_version` | Opaque engine-policy string surfaced by `memory.describe` | 01 |
| `MemoryPort` | Single public Rust service-boundary trait (spoke-operations) | 01 |
| `RemoteMemoryService` | Connect responder seam (mirrors `RemoteExtractService`) | 01 |
| Reference policy (fixture engine) | Implementation, not protocol: `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`, facade type `MemoryEngine`) — deterministic, I/O-free **pure algorithm policy** (candidate classification, recall scoring + budget, `state_digest`/staleness, bundle assembly/verify/ID-translation). Receives already-authorized data, returns results. **No storage abstraction exists** — all MEM-R enforcement (admission, CAS, idempotency, candidate atomicity, fencing) belongs to the provider behind `MemoryPort`. Path-dependency-only, never published; Nexus implements its own policy and treats it strictly as a conformance reference | 01/02 |
| `MemorySpace`, `Subject`, `RetainedRecord`, `MemoryCandidate`, `ProfileDocument`, `Reflection`, `MemoryGrant`, `Principal`, `MemoryReceipt`, `MemoryExportBundle`, `ReadSelection` | Wire models | 01 |
| `RetainedRecord.kind` | Open vocab; core `event`/`fact`/`note`; Nexus additions in §4.6 | 01 + this doc |
| `ReadSelection.mode` | `subject_all` \| `shared_only` \| `selected_contexts` (`context_ids[]` + `include_shared`) | 01 |
| Operation inventory (33) | `memory.describe`, `memory.spaces.list`, `memory.spaces.freeze`, `memory.spaces.unfreeze`, `memory.subjects.ensure`, `memory.subjects.get`, `memory.subjects.fence`, `memory.grants.list`, `memory.grants.grant`, `memory.grants.revoke`, `memory.capture`, `memory.candidates.list`, `memory.candidates.get`, `memory.candidates.dismiss`, `memory.candidates.review`, `memory.records.create`, `memory.records.get`, `memory.records.update`, `memory.records.delete`, `memory.records.list`, `memory.records.promote`, `memory.recall`, `memory.profile.get`, `memory.profile.update`, `memory.reflections.state`, `memory.reflections.list`, `memory.reflections.generate`, `memory.receipts.get`, `memory.exports.create`, `memory.exports.get`, `memory.imports.stage`, `memory.imports.activate`, `memory.imports.discard` | 01 |
| Error codes | `NOT_AUTHORIZED`, `GRANT_REVOKED`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `CANDIDATE_CONSUMED`, `PROFILE_INCOMPLETE`, `IDEMPOTENCY_CONFLICT`, `OUTCOME_UNCERTAIN`, `EXPORT_EXPIRED`, `IMPORT_CONFLICT`, `INSUFFICIENT_DATA`, `RECEIPT_EXPIRED`, `CURSOR_INVALID`, plus `INVALID_INPUT`, `REVISION_CONFLICT`, `NOT_FOUND` class, `INTERNAL_ERROR` (retryable ONLY when no effect committed), dispatch-level `op_unsupported`/`capability_missing`; `error.details.retry_class` = `retryable`\|`terminal`\|`uncertain` (`uncertain` ⇒ poll `memory.receipts.get`) | 01 |
| Async model | All mutating ops commit synchronously and return `MemoryReceipt` (`committed`\|`pending`\|`failed`, payload-digest-bound idempotency); ONLY `memory.reflections.generate`, `memory.exports.create`, `memory.imports.stage`, `memory.imports.activate` may return `pending` (long-running, polled via `memory.receipts.get`); every other mutating op commits synchronously; no webhooks/streams | 01 |
| `MEM-R001`–`MEM-R027` | Normative invariant anchors (01 §5; R001–R020 frozen, R021–R027 append-only from the integration-correction round); cross-referenced by number throughout 04 | 01 |
| Integration-round symbols (naming contract v2.0, landed 2026-10-28) | Fence `target` = `{"type":"subject"}` \| `{"type":"context","context_id"}`; actions `fence`\|`unfence` (both targets), `archive` (subject-only, TERMINAL); `Subject.fenced_contexts`; drain barrier = `MemorySpace.pending_receipts == 0`; freeze maintenance allowlist (01 §8.2): read-only ops + `memory.spaces.unfreeze` + `memory.grants.revoke` + `memory.exports.*` + `memory.imports.*`; grant `shared_access`; required `idempotency_key` (sole exception `memory.subjects.ensure`); dedup namespace `(space_id, requester_principal_id, op, idempotency_key)` → `payload_digest` over op + JCS-canonical payload; `dedup_retention_seconds` / `receipt_retention_seconds` in `describe.limits`; content-free `MemoryReceipt.result`; consumed-candidate shells with `content_digest`; `MemoryExportChunk` chunked transfer (`chunk_ordinal`, per-chunk `chunk_digest`, ordinal-0 `conflict_policy` + `adopt_grants`, `staged_report` incl. `candidate_id_map`, `export_chunk_bytes` limit); immutable `capture_scope` `{visibility, context_id?}` + provenance `source_context_ids`/`anchors`; `ProvenanceFilter` `{source_context_ids}` ANDed with placement `selection`; PUBLISH-PINNED concurrent-generation rule; `data_sufficiency`; `RetainedRecord.import_ref`; `MemorySpace.authority_generation` + binding pin `(authority_host_id, authority_generation)`; new terminal codes `INSUFFICIENT_DATA`, `RECEIPT_EXPIRED`, `CURSOR_INVALID`; `describe.spaces` removed (paginated `spaces.list` only); hard UTF-8 `max_bytes` with `overflow_record_id` | 01 |

## 3. Current-state evidence baseline (verified)

All paths are repo-root-relative. At this snapshot both this repository and
SPOKE consumed upstream package version `0.14.1`; SPOKE has since published
`0.15.1` carrying the memory family; this repository then moved its five
lockstep pins to `0.15.1` without consuming the family.

### 3.1 Memory entry points (as-is)

| # | Entry point | Current behavior | Evidence |
|---|---|---|---|
| E1 | CLI `apps/nexus42/src/commands/creator/memory.rs` | **Direct file I/O** through `nexus_creator_memory::memory_io` (`list_memories`, `load_memory`, `save_memory`, `delete_memory`) against the Creator bearer, bypassing CoreService | `memory.rs:14-15,199-337` |
| E2 | CLI `apps/nexus42/src/commands/creator/soul.rs` | **Direct file I/O** through `nexus_creator_memory::soul_io` (`exists`/`create`/`load`/`save`/`validate`) for Creator SOUL.md | `soul.rs:7,62-216` |
| E3 | CLI `apps/nexus42/src/commands/creator/character.rs` | The Character memory and ToM family authors through **direct-core** (`open_direct_core`): capture, pending-review list/count/dismiss, the bounded review drain, fragments, and the revision-checked promote, plus ToM record/show with carrier admission and keyset paging (`character.rs:20-27`, DTO imports `:44-46`, retained `Memory{CharacterMemoryCommand}` / `Tom{CharacterTomCommand}` enum arms `:118-127`). Only the `creator character run` and `creator character soul reflect` entrances were retired (with the daemon transport they needed) | `character.rs:20-27,44-46,118-127` |
| E4 | CoreService memory family `crates/nexus-core/src/memory.rs` (2178 lines) | Bearer-isolated pending-review capture/list/count/delete, fragment list, revision-checked promotion, bounded Creator review drain, Character ToM carrier family. Character mutations admit a per-Character `ActorActivityLease` held across DB/file/provider effects | `memory.rs:1-14,106-123,356-388` |
| E5 | Host run path `crates/nexus-core/src/host.rs` | `ExecuteOperationRequest::Prompt{remember:true}`: non-Character ⇒ `invalid_input`; Character ⇒ typed `not_supported` **before** any provider effect — automatic run capture is currently refused | `host.rs:1208-1230` |
| E6 | Execution projection `crates/nexus-core/src/soul.rs` | `load_character_mind_projection` reads Character SOUL.md + fragment rows (shared scope + selected binding scope) + promoted LTM files directly; fail-closed on read errors; deterministic merge with bounded budget | `soul.rs:628-722` |
| E7 | Host prompt assembly `crates/nexus-core/src/host.rs` | `assemble_admitted_prompt` builds `CoreCharacterMind` projection (with ToM) for Character actors; Creator actors get `creator_with_view` **without** a memory mind projection — no general Creator memory load is proven | `host.rs:1960-2016` |
| E8 | napi native surface `crates/nexus-core-node/src/actors.rs` | Exports the memory family to TS: `capture_character_pending_review`, `list/count/delete_character_pending_review`, `review_character_memory`, `list_character_memory_fragments`, `promote_character_fragment`, plus Creator-arm `list/count/delete_pending_review`, `review_memory`, `list_memory_fragments` | `actors.rs:1046-1350` |
| E9 | TS service `apps/nexus-service/src/memory.ts` | `/v1/daemon/*` HTTP memory/SOUL/ToM routes; thin translation over the native family surface with schema-owned generated DTOs | `memory.ts:1-40` |
| E10 | Moment context assembly `crates/nexus-moment-context-assembly` | `stage0.rs:18,51,189` and `cloud_stage.rs:9,87,246` consume `nexus_creator_memory::LongTermMemory`; `moment.rs:64-220` defines `CharacterMindInput` and the four fixed mind slots | cited lines |
| E11 | Review pipeline `crates/nexus-core/src/memory_pipeline.rs` + `crates/nexus-creator-memory/src/review.rs` | Deterministic rule classifier (thresholds 50/80/200 chars, task-kind rules); outcomes `PromoteToLongTerm`/`FragmentOnly`/`Drop` mutually exclusive; binding-local Character Promote coerced to fragment to block silent cross-world promotion; `MergeIntoExisting`/`TriggerSoulExperienceOnly` **declared but unimplemented** (skipped with debug log) | `review.rs:130-249`; `memory_pipeline.rs:319-418` |
| E12 | Digest capture writer `memory_pipeline.rs` | `PassthroughSummarizer`: no LLM; 256 KiB cap; prepends `# UNTRUSTED:` provenance header | `memory_pipeline.rs:645-700` |

### 3.2 Storage layout (as-is)

- **Files** (per bearer, via `nexus-home-layout`): `SOUL.md` and
  `memory/long-term/*.md`. Creator arm: `~/.nexus42/creators/<creator_id>/…`;
  Character arm: `~/.nexus42/creators/<owner_creator_id>/characters/<character_id>/…`.
  Character memory files are **Character-shared** — there is no binding
  dimension in the file layout. `MemoryBearerRef` is a **closed** two-arm enum
  (`Creator` | `Character`); creator ids must carry the `ctr_` prefix and pass
  path-safety validation.
  Evidence: `crates/nexus-creator-memory/src/bearer.rs:1-46`.
- **SQL tables** (SQLite, `crates/nexus-local-db/migrations/`): Creator and
  Character table families for pending review, fragments, soul narratives and
  stats caches (`20260701_000001_memory_fragments_world_and_soul_narratives.sql`,
  `20260702_000001_memory_soul_narratives_stats_cache.sql`,
  `20260704_000001_memory_soul_narratives_composite_world_key.sql`,
  `202609050003_character_memory.sql`); run-capture receipts and pending
  provenance (`20260906000003_character_run_capture.sql`: `character_run_captures`
  with `operation_id` PK, `pending_id` UNIQUE, `lifecycle_epoch`, partial unique
  indexes separating manual vs operation-sourced pending rows); core writer
  protocol (`20260912000001_core_writer_protocol.sql`: `core_workspace_gate`,
  `core_writer_registration`, `core_changes`, `WRITER_FENCED` triggers keyed on
  `nexus_writer_protocol()`/`nexus_writer_mode()`).
- **Scope encoding (dual-meaning, as-is)**: `MemoryPipelineCtx.scope_id` is the
  Creator arm's **world id** or the Character arm's **binding id**; `None`
  means whole-Creator vs shared-Character depending on the arm. The same
  column therefore carries two different foreign-key semantics.
  Evidence: `crates/nexus-core/src/memory_pipeline.rs:73-93`.

### 3.3 Authorization and fencing (as-is)

- `MemoryPipelineCtx` is an **authorization capability**: private fields, only
  `creator` / `character_read` / `character_write` constructors; Character
  contexts pass an ownership check (foreign or missing Character ⇒ 404
  `not_found`, indistinguishable); a read-only context cannot mutate.
  Evidence: `memory_pipeline.rs:66-123`.
- Writable Character contexts wrap an admitted `ActorActivityLease` held for
  the context lifetime (activity fence across all nested effects).
  Evidence: `memory_pipeline.rs:59-75`.
- Fragment promotion is revision-CAS: a stale `expected_revision` writes
  nothing (409 `version_mismatch`); success clears binding provenance and
  bumps the revision. Evidence: `crates/nexus-core/src/memory.rs:356-388,1219-1224`.
- Durable receipts: `crates/nexus-core/src/operation_receipts.rs:1-13` owns
  the **read side** of the durable operation-receipts store (spec
  `connect-event-delivery-and-operation-receipts.md` §B.2/§B.3); the frozen
  `uncertain` refusal (`UNCERTAIN_CODE`, `uncertain_refusal`) exists at
  `:32-57` but is currently **`#[cfg(feature = "compute")]`-gated** — the
  receipt stack is live for compute Runs, **not** yet a Memory-domain
  consumer.
- Stable operation identity: `crates/nexus-core/src/operation_id.rs:1-24`
  defines `op_<hex32>` = SHA-256(canonical_json({actor, session, action,
  args})) truncated to 128 bits; `:39-55` documents that random-per-attempt
  `HostOperationId::new()` is uniqueness, **not** replay stability.
- Service open/binding: `CoreOpenOptions` currently carries only
  `user_home` + `access`; `CoreInner` binds one creator + one workspace
  (`state.db`) per open, holds the engine admission, re-reads the on-disk
  selection on use (`verify_selected_context`), and runs a retained close
  drain. Evidence: `crates/nexus-core/src/service.rs:62-98,114+`.
- Conformance precedent: `crates/nexus-provider-conformance` is a neutral,
  credential-free CI conformance runner over normalized provider streams;
  `crates/nexus-provider-ports` defines the schema-derived `ProviderPort`
  trait precedent for a single injectable service boundary.
- Cloud boundary: the local service host must not depend on cloud-sync /
  cloud-domain crates and must not expose cloud HTTP or sync proxies.
  Evidence: `docs/ARCHITECTURE.md:37-58`.
- Credential precedent: CLI auth persists the user token at
  `~/.nexus42/auth.json` (`apps/nexus42/src/auth/user_auth.rs:8`);
  `crates/nexus-cloud-sync/src/platform_client.rs:54-95` redacts bearer/API-key
  patterns from error strings.

### 3.4 Verified gaps (current facts, not proposals)

1. **Creator production capture is not wired.** `create_pending_review`
   (`crates/nexus-local-db/src/pending_review.rs:36`) has callers only in that
   file's tests (`:220-314`); no production Creator capture writer exists.
   This is scoped investigation evidence about this repository, not a claim
   about external consumers.
2. **Automatic run capture is refused.** Host `remember:true` is rejected for
   every session (`host.rs:1208-1230`); the `character_run_captures` receipt
   table exists but no Host capture writer is connected.
3. **Two declared review actions are unimplemented.**
   `MergeIntoExisting` / `TriggerSoulExperienceOnly` are skipped at runtime
   (`memory_pipeline.rs:319-418`); they must not be advertised as supported
   default behavior.
4. **Scope is ambiguous.** `scope_id` doubles as world id vs binding id and
   `None` overloads two meanings (§3.2).
5. **CLI bypass.** E1/E2 reach memory files without CoreService, the writer
   protocol, or any grant check (§3.1).
6. **Receipt stack not extended to Memory.** The `uncertain` refusal is
   compute-gated; no memory-domain operation identity is recorded (§3.3).

## 4. Target architecture (proposed)

### 4.1 Providers

- **Default local provider — `LocalMemoryProvider` (new, in-process).**
  Implements `MemoryPort` inside `nexus-core` with **provider-private**
  storage/transaction code over the existing SQLite tables and bearer
  markdown files, enforcing all MEM-R invariants in its own transaction
  layer and running **Nexus-owned** algorithm policy (classification,
  recall scoring/budget, staleness, transfer transformation) over
  already-authorized data. Policy is nexus code, never an upstream
  dependency: the reference engine ships only as the unpublished
  path-dependency fixture `spoke-fixture-memory-engine`, usable as an
  offline conformance reference for the algorithm-variable expectations
  (protocol §9, matrix class `FREE`) and never linked in. Zero network. The
  first cutover keeps the current physical layout unchanged behind the
  adapter (§5.1).
- **Remote providers — via `RemoteMemoryService` over SPOKE Connect.**
  A remote provider is a Connect peer exposing the 33-op inventory under the
  `spoke-memory` capability. Transport reuses Connect; it MUST NOT route
  through `nexus-cloud-sync` and MUST NOT reintroduce a platform sync proxy
  (`docs/ARCHITECTURE.md:37-58` hard isolation). Protocol support does not
  imply any production cloud-service deployment.
- **One boundary.** `MemoryPort` is the single public replacement boundary.
  Provider-private storage/transaction code stays an implementation detail;
  no public second storage protocol, no local DTO clone of the wire models.

### 4.2 Configuration and provider selection

- `CoreOpenOptions` gains one optional field: `memory: Option<MemoryProviderSelection>`
  where `MemoryProviderSelection = Local (default) | Remote { space_hint,
  credential_id, connect_endpoint }`. Absent ⇒ `Local`; behavior without
  configuration is byte-identical to today.
- **Binding pin.** The selected provider is resolved once at
  `CoreService::open` and pinned in `CoreInner` for the open's lifetime —
  same discipline as the existing creator/workspace binding
  (`service.rs:62-98`). A config change never retargets a live open.
  The pin carries the **authority identity pair `(authority_host_id,
  authority_generation)`** (01 v2.0): after a cutover (§5.3) the old provider
  remains frozen and rejects writes, and a stale client holding a
  pre-cutover `authority_generation` fails with a typed refusal — it is
  never auto-unfrozen or silently re-pointed. A local configuration edit is
  never presented as a distributed cutover guarantee; all participating
  clients coordinate through the provider-enforced fence and drain barrier
  (`MemorySpace.pending_receipts`, §5.3).
- **Switching** is an explicit rebind: drain or await inflight memory
  operations (the existing retained close-drain pattern is the precedent),
  run the transfer procedure of §5.3 if data must move, then open a new
  service against the new selection. Inflight receipts remain replayable
  against the provider that accepted them (§5.4).
- Selection state lives in the workspace config file; **secrets never do**
  (§4.3). The on-disk selection is re-read on use, mirroring
  `verify_selected_context`'s staleness discipline, so a config edit between
  opens is honored and a mid-open edit is not silently applied.

### 4.3 Credentials protection

- Remote credentials are referenced by opaque `credential_id` and resolved
  from the local auth store (precedent: `~/.nexus42/auth.json`,
  `apps/nexus42/src/auth/user_auth.rs:8`). The token lives in memory for the
  duration of a Connect session and is never written into receipts, export
  bundles, `core_changes`, logs, or error messages.
- All memory-provider error rendering reuses the redaction discipline of
  `platform_client.rs:54-95` (bearer/API-key pattern redaction).
- `MemoryReceipt` carries `payload_digest`, never payload; receipts and
  `memory.receipts.get` responses are therefore safe to persist and replay
  without disclosing secrets or content.

### 4.4 Capability admission and entry-point convergence

- Admission: a memory operation is admitted only when the serving host
  advertises `spoke-memory` (plus `spoke-memory-reflection` /
  `spoke-memory-transfer` for those families). A caller on a host without
  the capability gets dispatch-level `capability_missing` / `op_unsupported`
  — never a silent local fallback.
- **Convergence rule (hard): every Nexus memory consumer calls the
  CoreService memory family, which is the only `MemoryPort` client.** The
  adapter translates the existing bearer model onto protocol models inside
  `nexus-core`; no other crate, CLI command, napi export, TS route, or MCA
  stage may touch memory tables/files directly after cutover. The migration
  inventory in §6 lists every caller; "CLI file bypass" (gap 5) is closed by
  rerouting E1/E2 through the same service calls the napi surface uses, not
  by wrapping `memory_io` in more places.
- Pre-ranking authorization (including counts and pagination) is enforced in
  the adapter before any recall ranking work, per 01's invariant set.

### 4.5 Entity / actor / holder mapping

| Protocol model | Nexus mapping | Notes |
|---|---|---|
| `MemorySpace` | One default local space per CoreService open, keyed by (creator_id, workspace) — the holder binding of §3.3. Remote spaces are discovered via `memory.spaces.list`. | One authoritative provider per space; never shared-SQLite across processes |
| `Subject` | Creator ⇒ `Subject{id: creator_id, kind: "creator"}`; Character ⇒ `Subject{id: character_id, kind: "character"}` with owner-namespace metadata. Open vocab, **not** a closed two-arm enum at the wire; the Nexus adapter keeps the closed bearer validation internally | Cross-system same-ID equality never auto-links identities |
| `Principal` | The verified Nexus `Principal` (principal's own creator id) is the **authenticated requester** on every operation. Admin operations (`memory.grants.grant`/`list`) may name a **target grantee** `principal_id` in the payload — that selects the grantee, never impersonates the requester | Model-supplied JSON is never authority; provider-populated provenance/audit fields cannot be spoofed by record-create content |
| Holder | The `CoreInner` (creator + workspace + writer admission) holding the local space authority | Pinned per open (§4.2) |
| `MemoryGrant` | Per-subject/context grants minted by the holder for additional clients (e.g. a Connect peer UI). Scope axes per 01 v2.0: `context_ids` omitted = unrestricted (including shared); `shared_access: bool` (default false) gates shared visibility when `context_ids` present; `subject_all` recall REQUIRES a grant with `context_ids` omitted, `shared_only`/`include_shared:true` require `shared_access` or unrestricted. Non-amplification: a granted admin's issuance must be ⊆ its own effective coverage (capabilities, subject, context_ids, shared_access, `expires_at`). Bootstrap: spaces + initial admins are provider-operator provisioned out-of-band (no protocol op in v1) — locally, the operator's CoreService open IS that provisioning act. Imported grants arrive inactive and are adopted only via explicit `adopt_grants` entries naming `{grant_id, principal_id}` | Grants are provider-enforced across ALL clients, not per-process locks; grant checks apply to by-id reads, candidates, receipts, profiles, reflections, exports and counts/cursors — not only recall |
| Context (placement vs provenance) | **Placement** (visibility/written scope): Character binding id ⇒ `capture_scope {visibility, context_id}` naming the binding context; shared Character / whole-Creator ⇒ shared placement. **Provenance**: Creator world id and session anchors ⇒ `source_context_ids` + `anchors` (SourceAnchor), filters on reads/reflections, NOT a placement dimension. `memory.capture` REQUIRES immutable `capture_scope`; review decisions inherit it (no scope field in the decision); shared visibility afterwards requires `memory.records.promote` with `records_write` authority on BOTH source and target | Replaces dual-meaning `scope_id` (gap 4); `None` overloads disappear — read selection uses the three explicit `ReadSelection.mode` values |
| `RetainedRecord` | Promoted LTM markdown entries and retained fragment rows | Provenance contexts travel on the record; physical placement stays provider-private (§5.1) |
| `MemoryCandidate` | Pending-review rows (`memory_pending_review`, `character_memory_pending_review`) | Never returned by ordinary recall before review; consumed/dismissed candidates are content-scrubbed shells retaining `content_digest` (sha256) only |
| `ProfileDocument` | **Content kinds only** (01 v2.0 core vocab): SOUL.md ⇒ `personality` (author-maintained, trusted); author preference data, if added, ⇒ `preferences`. Provider policy (binding, retention, review policy) and transfer manifests are **configuration/state, never ProfileDocuments** — no fake `governance`/`transfer`/`reflection` markdown is required to select a Nexus provider. Failure triage (never collapsed): missing REQUIRED authored content profile ⇒ `PROFILE_INCOMPLETE`; absent optional authored data ⇒ honest-empty success; missing provider capability ⇒ `capability_missing` at selection | A complete Nexus replacement fails admission if a required profile is missing — no empty success for required content |
| `Reflection` | Derived soul narratives / experience aggregation outputs. Scoped by `selection` (ReadSelection) **ANDed** with optional `filter` (`ProvenanceFilter {source_context_ids}`, minItems 1): Creator per-World projections are provenance-filtered; Character binding scope is visibility placement. Basis/cache key pins selection+filter + authorized-source-set digest + source revisions + `engine_policy_version`; `state_digest` commits to all. Invalidation triggers: new/deleted records, edits, profile changes, scope promotions. Concurrent generation rule: PUBLISH-PINNED (published with pinned basis; reads stale if basis moved). `reflections.state` reports `data_sufficiency`; generate MAY refuse `INSUFFICIENT_DATA` (terminal; threshold is provider/engine policy, not a protocol-wide Nexus constant). Reflections whose basis references tombstoned records are EXCLUDED from list/export | Derived state; context-restricted reflect grants cannot read all-subject derived text through a subject-only call |
| ToM | **Stays in World KB** (`KnowledgeEntryRecord` family, `nexus-knowledge`). Memory reflections may *reference* ToM carrier ids; ToM content is never copied into memory records | Boundary preserved per agreed direction |

### 4.6 Nexus record kinds (mapping, not replacement)

Current fact: `nexus_creator_memory::memory_item::MemoryKind` is a **nine-value**
enum (`memory_item.rs:31-64`): `story_summary`, `research_material`,
`review_note`, `character_note`, `world_building`, `plot_outline`,
`theme_analysis`, `personality_core` (the push-from-SOUL.md personality
track), `custom`. The protocol's open `RetainedRecord.kind` vocab (core
`event`/`fact`/`note`) MUST NOT force these into three buckets, and no data
may be dropped. Mapping rules:

- Every existing `MemoryKind` value maps to a Nexus-namespaced record kind
  carrying the same string (e.g. `nexus.story_summary`); core `event`/`fact`/
  `note` are used for protocol-level generic entries only. The mapping is a
  lossless bijection at the adapter boundary.
- **Frontmatter and file structure are preserved**: slug identity,
  `source_session_ids`, user-authored sections, and the push-personality /
  extra-section structure survive the mapping; the deterministic experience
  refresh (soul/experience aggregation) keeps its current deterministic
  behavior over the mapped kinds.
- Fragments (review `FragmentOnly` products) map to core `fact` records with
  binding/world provenance contexts; raw session digests are `event` records
  in candidate form until reviewed.

Ablation: collapsing to core-only kinds would lose the nine-value authoring
taxonomy the CLI and review pipeline expose today (confirmed requirement:
§6 caller surfaces render kind strings); the namespaced mapping costs one
translation table.

## 5. Two-layer landing: protocol adoption ≠ data merge

### 5.1 Layer separation and physical-layout preservation

Adoption changes the **service contract** first; the **content schema is
preserved**, but the control plane is honestly additive:

- Existing content tables (`*_pending_review`, `*_memory_fragments`,
  `*_soul_narratives*`, `character_run_captures`) and markdown files
  (`SOUL.md`, `memory/long-term/*.md`) remain the local provider's physical
  content store. Protocol compatibility never requires identical storage
  schema.
- **Not fully schema-unchanged**: revision columns, stable record/candidate
  identity, and idempotency/fence/journal control metadata require an
  explicit additive migration. That migration registers with the writer
  guard/outbox machinery and bumps `DB_SCHEMA_VERSION`; it is a normal
  guarded migration, not an uncontrolled side channel.
- **Rollback constraint (corrected)**: once that DDL has run, rolling back
  to an old binary is NOT a bare Git revert — it requires the matched
  binary plus a fully consistent backup restore, or an explicit
  compatibility verification showing the old binary tolerates the additive
  columns. After any target-side writes, rollback follows the data-preserving
  reverse path of §5.3 step 5, never a delete-and-revert.
- The **writer guard is preserved, not bypassed**: the local provider's
  writes execute inside `nexus-core` under the existing core writer protocol
  (`core_writer_registration` modes, `WRITER_FENCED` triggers,
  `core_workspace_gate`). The adapter never opens its own SQLite handle and
  never writes files outside the pipeline that holds writer admission.
- Files do not become transactional merely because code writes them inside a
  SQL transaction (§5.2 handles the consequence explicitly).
- Migration/merge of existing content into a *different* store is a
  separate, later, explicitly triggered decision (§8), not part of protocol
  adoption.

### 5.2 Cross-store (SQL + file) recovery

The local provider's writes span SQLite rows **and** bearer markdown files,
and a single SQL transaction CANNOT make a file write atomic with a row
write. The design therefore uses a **durable intent journal**, not a claimed
file+SQL atomic transaction:

1. **Intent first**: each mutating operation commits a durable journal
   record (inside the guarded SQL transaction) BEFORE any file effect:
   operation identity, planned file targets with content digests, and the
   SQL effects. The journal commit is the operation's point of no return.
2. **Staged file effect**: the markdown write lands as a hidden staged file
   (not visible to recall/list), followed by a **consistency publish** step
   that atomically renames it into place and marks the journal entry
   applied, then the receipt reaches its terminal state.
3. **Crash windows**: a crash before the journal commit leaves no visible
   effect (the receipt never existed; the idempotency key is free). A crash
   after the journal commit leaves a journaled intent; on reopen or replay,
   recovery reads the journal — a staged file matching the intent digest is
   published, a missing/mismatched one is regenerated from the journaled
   content or quarantined — and only then is the receipt finalized. Replay
   NEVER re-executes a committed effect (§5.4 replay ordering).
4. **Isolation of recovering objects**: any object with an unresolved
   journal entry is excluded from ordinary recall/list/get and from export
   snapshots until resolved; recovery may run at open, but open does not
   make half-committed content readable. Consistent backup/snapshot requires
   a quiesced barrier (§5.3 drain barrier) — "recovery never blocks open"
   is not a license to expose recovering state.
5. Unresolved items after the bounded recovery pass surface as explicit
   `INTERNAL_ERROR` on the affected subject with `retry_class: terminal`
   until operator action.

### 5.3 Remote lifecycle: fence / unbind / archive / transfer

Multi-system sharing is provider-enforced, never per-process locking dressed
up as distributed safety. The protocol primitives (01) are used as follows:

1. **Fence before move**: `memory.subjects.fence` and/or
   `memory.spaces.freeze` make the source provider reject new ordinary
   domain writes with `SUBJECT_FENCED` / `SPACE_FROZEN` **across all
   clients**, not just this Nexus process. A subject-wide fence cannot
   safely remove a single ActorWorldBinding: binding removal uses the
   fence request's `target: {"type":"context","context_id"}` (01 v2.0),
   leaving other contexts and shared placement writable; fenced contexts
   are observable via `Subject.fenced_contexts`; fence revisions CAS, and
   stale in-flight writes at fence completion get typed refusals. Grant
   revocation (`memory.grants.revoke` ⇒ `GRANT_REVOKED` at admission of
   subsequent ops; already-admitted ops complete, MEM-R018) is likewise
   provider-enforced.
2. **Freeze is not a total write ban**: the 01 §8.2 maintenance allowlist
   remains admitted under freeze — all read-only ops,
   `memory.spaces.unfreeze`, `memory.grants.revoke`, `memory.exports.*`,
   `memory.imports.*` — while all other mutations are denied
   `SPACE_FROZEN`. The durable **drain barrier** is
   `MemorySpace.pending_receipts == 0` (surfaced in `memory.spaces.list`
   entries and the freeze result): no export or cutover starts while
   admitted writes can still commit unnoticed.
3. **Snapshot/export**: `memory.exports.create` (long-running, polled),
   `memory.exports.get(export_id, chunk_ordinal)` retrieves
   **`MemoryExportChunk`s** in ordinal order under the `export_chunk_bytes`
   limit — per-chunk `chunk_digest`, `bundle_digest` over the whole, never
   one giant bundle, never silent truncation; an over-limit space gets an
   explicit bounded refusal, not clipped memories. The bundle includes
   **candidate shells (content-scrubbed, `content_digest` retained) and
   content-free `receipt_summaries`** preserving consumed-operation history
   and preventing reimport/replay duplicate capture, plus explicit
   translation maps for principals/grants/subject/record/candidate IDs and
   revisions — and all references (candidate consumption, reflection basis,
   profile/subject/fence revisions, provenance; imported records carry
   `import_ref {import_id, source_record_id, source_revision}`). Credentials
   are never copied; grant entries carry `principal_ref` + `revision` as
   redacted correlation summaries, **inactive until explicit
   `adopt_grants` adoption** on the target. Imported reflections enter
   **stale** pending regeneration, not claimed valid. The provider owes the
   snapshot consistency guarantee: single authority alone does not make a
   SQL+file snapshot coherent — the export runs under the drain barrier
   (`require_quiescent: true`).
4. **Stage on target**: `memory.imports.stage` is called per chunk in
   ordinal order (ordinal 0 carries `conflict_policy` + `adopt_grants`);
   the final chunk triggers verification and a `staged_report` including
   `candidate_id_map`. Staged data is invisible to recall.
   **Activation** (`memory.imports.activate`) is the single cutover commit,
   verified by counts covering records, candidates, and translated
   references plus spot-CAS on translated revisions. **Pre-activation
   rollback** is `memory.imports.discard` — explicit, not implicit GC.
5. **Post-activation rollback (corrected)**: it is NOT compensating deletion
   of records followed by reverting to the old source — that leaves
   subjects/profiles behind and loses any memory written on the target
   after cutover. Two cases (01 v2.0 vocabulary):
   - **Target has accepted no writes**: `memory.imports.discard` / abandon
     the frozen target, and reopen the source under a controlled binding
     rollback — operator unfreeze plus a NEW `authority_generation`.
   - **Target has accepted writes**: first freeze+drain the target
     (`pending_receipts == 0`), then run a verified **reverse transfer** —
     `memory.exports.create(require_quiescent: true)` + per-chunk
     `imports.stage`/`imports.activate` target→source (delta
     reconciliation) — before reopening the source. Until reconciliation
     completes, the source stays frozen (clients observe `SPACE_FROZEN`),
     exactly one authority exists, and status is explicitly
     blocked/uncertain. Default behavior NEVER deletes target-written
     history and NEVER opens both authorities at once.
6. **Cutover authority pinning**: binding config pins
   `(authority_host_id, authority_generation)`; after activation the old
   provider remains frozen and keeps rejecting writes; clients holding a
   stale `authority_generation` fail with a typed refusal — nothing
   auto-unfreezes the old authority. All participating clients coordinate
   through the provider-enforced barrier; a local config edit alone is not
   a cutover.
7. **Archive mapping (corrected)**: the protocol's terminal `archive` is
   end-of-life and irreversible, while Nexus archive/restore is
   **reversible** — Nexus maps its archived Character state to the
   reversible fence semantics (writes refused, retained reads preserved,
   unfence restores, state survives restart via the fence metadata of
   §5.1's migration), and does NOT use the terminal protocol archive for
   its restore path. Today's "archived Character retained rows stay
   readable" behavior (`memory_pipeline.rs:106-117`) maps directly.
8. **Races**: freeze/revoke/fence vs in-flight write ⇒ outcome decided at
   admission (MEM-R018: admitted ops complete; non-admitted get the typed
   refusal), receipt records the outcome; vs concurrent import activation
   ⇒ `IMPORT_CONFLICT`; vs export expiry ⇒ `EXPORT_EXPIRED`. None of these
   degrade to silent fallback or dual-write.
9. **Explicit non-claims**: no distributed exactly-once, no cross-provider
   atomic transactions, no CRDT/offline multi-master, no default dual-write,
   no federated recall. One authoritative provider per logical space.

### 5.4 Memory-domain operation identity and receipts

- **Mandatory idempotency**: every mutating memory request carries a stable
  `idempotency_key`; the sole exception is `memory.subjects.ensure`
  (naturally idempotent, outcome recoverable via `memory.subjects.get`).
  The dedup namespace is `(space_id, requester_principal_id, op,
  idempotency_key)` with `payload_digest` computed over op + JCS-canonical
  payload — peer session scope is rejected as identity input because
  reconnects and provider migration must not change logical identity.
- The existing receipt/id primitives are **reused, not inherited blindly**:
  the `op_<hex32>` derivation and the durable receipt store serve the memory
  domain through an explicitly registered new consumer; the memory dedup
  namespace above is recorded as the memory-domain operation identity. This
  requires extending the receipt store's consumer set and schema
  registration and lifting the `uncertain` refusal out of its current
  `compute`-feature gate for the memory consumer
  (`operation_receipts.rs:32-57`). This plan does **not** claim the existing
  store already serves Memory.
- **Retention**: dedup tombstones are retained `dedup_retention_seconds`
  (≥ `receipt_retention_seconds`; both advertised in `describe.limits`).
  Replay after receipt eviction but within the dedup window ⇒
  `RECEIPT_EXPIRED` (terminal, `details.last_known_status`) — never a
  silent re-run; beyond the dedup window a key is treated as new.
- **Content-free receipts**: durable `MemoryReceipt` summaries carry IDs,
  revisions, counts, and outcome only — never record/profile bodies — so
  deletion/erasure semantics do not contradict immutable receipts. Live
  result data, when returned, is separate from the immutable summary and is
  authorized at read time; a revoked principal cannot read receipts.
- **Replay ordering**: replay of a committed operation returns the stored
  receipt and NEVER re-runs stale CAS/fence checks against current state;
  replay of an uncommitted one re-enters admission normally.
- **Failure honesty**: `INTERNAL_ERROR` maps to retryable/no-effect only
  when zero effects committed. For async pending receipts (generate/export/
  import), crash recovery never double-charges a paid generation; a failed
  generation may leave a content-free cost fact without exposing input text.

## 6. Caller migration inventory

| Entry | Current (§3.1) | Target | Migration action |
|---|---|---|---|
| E1 CLI `creator/memory.rs` | Direct `memory_io` file I/O | Calls CoreService memory family (records list/get/create/update/delete) | Reroute each subcommand; delete direct `memory_io` use |
| E2 CLI `creator/soul.rs` | Direct `soul_io` file I/O | `memory.profile.get`/`memory.profile.update` (kind `personality`) | Reroute; delete direct `soul_io` use |
| E3 CLI `creator character memory` / `tom` | Direct-core bearer-lease ops (capture, list/count/dismiss, review drain, fragments, promote; ToM record/show) | Same commands re-targeted onto the converged CoreService→`MemoryPort` path; CLI surface preserved; only `run`/`soul reflect` stay retired | Reroute each subcommand onto the migrated CoreService family; keep DTOs |
| E4 CoreService `memory.rs` | Direct pool + file pipeline | Becomes the adapter core: translates to `MemoryPort` calls | Refactor internals; public napi/service DTOs preserved |
| E5 Host `remember` | Refused (`not_supported`) | **Unchanged** — see D-4 | No code change in this plan |
| E6 soul.rs projection | Direct fragment/LTM/SOUL reads | Recall via adapter (`memory.recall` with `selected_contexts` + `include_shared`, profile get) | Replace read path; keep fail-closed + deterministic merge semantics at adapter boundary |
| E7 host.rs prompt assembly | Uses E6 for Character | Unchanged call shape; source now adapter-backed | Dependency-only |
| E8 core-node napi | Exposes memory family | Same exports, backed by migrated CoreService | No napi signature change required |
| E9 nexus-service memory.ts | Thin HTTP over napi | Unchanged routes | Dependency-only; verify in AC |
| E10 MCA stage0/cloud_stage | Consume `LongTermMemory` type | Consume adapter recall DTO (contract-generated) | Type swap; no second DTO set |
| E11 review pipeline | Direct DB/file review actions | Candidate ops (`memory.candidates.*`) + `memory.records.promote` via adapter | Reroute; unimplemented actions stay unadvertised (D-3) |
| E12 PassthroughSummarizer | Digest header writer | Capture path input to `memory.capture` | Preserve UNTRUSTED provenance header as candidate metadata |

### 6.4 Automatic run capture — explicit product boundary (Decision D-4)

Current fact: Host refuses `remember:true` for all sessions (gap 2). **This
plan does not re-enable automatic run capture.** Re-enabling is a product
policy decision requiring separate authorization; its acceptance scenarios
are out of scope for 04's matrix and would be added as a matrix revision
under that separate workflow. The adoption below only guarantees that the
explicit capture path (E4 `capture_character_pending_review` ⇒
`memory.capture`) is protocol-conformant, so a future authorized capture
wiring lands on an already-conformant operation.

## 7. Staged roadmap

Stages are sequential unless "Parallel" is stated. Owner = responsible role.
Every AC names a currently runnable command **or** an explicitly proposed new
test name (marked `PROPOSED`); nothing below executes paid/live providers,
real browsers/devices, or deployments.

### Stage N0 — Upstream availability gate

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-0: Confirm upstream protocol + package landed: `MemoryPort` in spoke-operations, schemas/codegen, `RemoteMemoryService` seam — **landed upstream in 0.15.0** (2026-10-30) and published as 0.15.1 (2026-10-31): schemas/codegen/parity, `MemoryPort`+validators+TS parity, the fixtures-only engine crate, Connect dispatch/`RemoteMemoryService`/client, and conformance fixtures (there is no separate `spoke-memory` protocol crate — the surface is schemas + operations + Connect arbitration). The upstream independent-provider conformance proof parallels NXM-11 | architect | upstream 0.15.0 landed and 0.15.1 published (gate materially open) | Nexus workspace resolves the upstream package version carrying the family; `cargo metadata` shows the new dep graph. Rollback point: pin stays at 0.14.1 |

### Stage N1 — Local provider adapter (storage unchanged)

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-1: `LocalMemoryProvider: MemoryPort` inside nexus-core: provider-private storage/transaction layer over existing SQLite tables + bearer files under writer admission; **Nexus-owned** policy functions for classification/recall/staleness/transfer (algorithm-variable — the unpublished fixture engine is an optional offline reference, never a dependency). Includes the §5.1 additive control-metadata migration (revision columns, stable record/candidate identity, idempotency/fence/journal metadata) registered with the writer guard/outbox and bumping `DB_SCHEMA_VERSION` | fullstack-dev | NXM-0 | `PROPOSED: cargo test -p nexus-core local_memory_provider` — round-trips capture→review→promote→recall against a temp nexus home; writer-guard triggers still fire on out-of-protocol writes (assert `WRITER_FENCED`); migration idempotent and registered in `core_writer_registration` |
| NXM-2: Memory-domain receipts: extend receipt store consumer set + memory scope tuple (§5.4); lift `uncertain` refusal for memory consumer | fullstack-dev | NXM-1 | `PROPOSED: cargo test -p nexus-core memory_receipts` — duplicate `memory.capture` returns `IDEMPOTENCY_CONFLICT`-free replay with identical receipt; timeout path yields `OUTCOME_UNCERTAIN` then resolves via `memory.receipts.get` |
| NXM-3: SQL+file reconciliation pass (§5.2) | fullstack-dev | NXM-1 | `PROPOSED: cargo test -p nexus-core memory_reconcile` — kill-injected fixtures for both crash windows converge or quarantine deterministically |
| NXM-4: Profile admission with content-kind discipline (§4.5): required authored profile missing ⇒ `PROFILE_INCOMPLETE`; optional authored data absent ⇒ honest-empty; provider-capability mismatch ⇒ distinct provider-selection failure. SOUL.md mapped to `personality`; NO fake governance/transfer/reflection profile documents | fullstack-dev | NXM-1 | `PROPOSED: cargo test -p nexus-core memory_profile_admission` — all three outcomes distinguished |

Parallel-safe: NXM-2 ∥ NXM-3 ∥ NXM-4 after NXM-1 (disjoint modules; shared
files: none beyond NXM-1's landed interface).

### Stage N2 — Entry-point convergence (cutover)

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-5: Refactor CoreService memory family (E4) onto `MemoryPort`; preserve DTOs and error mapping | fullstack-dev | NXM-1..4 | Existing runnable regression anchors pass: `cargo test -p nexus-local-db pending_review`, `cargo test -p nexus-core memory_pipeline` (incl. `passthrough_summarizer_includes_untrusted_header`) |
| NXM-6: Reroute CLI E1/E2 through CoreService; delete direct `memory_io`/`soul_io` use from `apps/nexus42` | fullstack-dev | NXM-5 | `grep`-verifiable absence: no `memory_io`/`soul_io` imports under `apps/nexus42/src/commands/`; CLI smoke via existing command tests `PROPOSED: cargo test -p nexus42-cli creator_memory` |
| NXM-7: Reroute E6 projection + E10 MCA consumption through adapter recall; swap `LongTermMemory` for contract DTO | fullstack-dev | NXM-5 | `PROPOSED: cargo test -p nexus-moment-context-assembly memory_recall_dto`; `cargo test -p nexus-core soul` projection fixtures unchanged in content |
| NXM-8: Review pipeline reroute (E11) onto candidate/record ops | fullstack-dev | NXM-5 | Existing: `cargo test -p nexus-creator-memory` (review classifier tests); `PROPOSED: cargo test -p nexus-core memory_review_port` — binding-local promote coercion preserved |
| NXM-16: Creator explicit-capture parity: typed Creator capture capability contract across core / native (napi) / service / CLI (mirroring the existing Character capture family), closing the Creator/Character interface asymmetry (gap 1). Explicit capture only — Host auto `remember` stays refused (D-4) | fullstack-dev | NXM-5 | `PROPOSED: cargo test -p nexus-core creator_capture` — Creator capture→review→promote round trip via the typed API; CLI `creator memory` capture command smoke; regression: `remember:true` refusal unchanged (C-115) |

Rollback point for all of N2: rollback of the *service-contract* stage is a
Git-level revert of rerouted callers ONLY while the binary matches the DB
schema. Once NXM-1's additive control-metadata DDL has run
(`DB_SCHEMA_VERSION` bumped), an old-binary rollback requires the matched
binary plus a fully consistent backup restore, or an explicit compatibility
verification that the old binary tolerates the additive columns; after any
new-system writes, rollback follows §5.3 step 5's data-preserving reverse
path. No `memory.provider = legacy` flag is introduced.

Parallel-safe: NXM-6 ∥ NXM-7 ∥ NXM-8 ∥ NXM-16 after NXM-5 (disjoint crates;
NXM-16 touches the same family surface as NXM-5 but lands after it).

### Stage N3 — Remote provider path

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-9: `CoreOpenOptions.memory` selection, binding pin, credential resolution + redaction (§4.2/§4.3) | fullstack-dev | NXM-2 | `PROPOSED: cargo test -p nexus-core memory_provider_selection` — pin stability, config re-read between opens, no secret in any persisted receipt/log fixture |
| NXM-10: Connect client wiring to `RemoteMemoryService`; capability admission (`spoke-memory` + sub-capabilities); no cloud-sync dependency | fullstack-dev | NXM-9 | `PROPOSED: cargo test -p nexus-core memory_remote_loopback` — loopback Connect responder served by an independent provider with its own private temp storage and its **own policy implementation** (the unpublished fixture engine is an optional offline conformance reference, never a dependency — not an echo mock); architecture guard: `cargo tree -p nexus-runtime` shows no cloud-sync/cloud-domain edge |
| NXM-11: Conformance runner integration: the 04 matrix executes against the local provider AND an **independent real durable service provider** (own persistent store, restart-surviving, served via `RemoteMemoryService` on loopback — an engine-internal mock is NOT accepted as the second provider). Standard libtest named-test filters (`cargo test -p nexus-memory-conformance c_020`); no custom `--scenario` harness and no local `--all` requirement | qa-engineer | NXM-10, 04 matrix | All 04 scenarios marked `both` pass against both providers; results table in 04 §6 |

### Stage N4 — Lifecycle and transfer tooling

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-12: Freeze/drain-barrier/export/stage/activate/discard runbook as CLI ops over the adapter (§5.3): chunked transfer incl. candidate shells + receipt summaries + `adopt_grants`, and the two-case post-activation rollback (discard/abandon vs verified reverse transfer) | fullstack-dev | NXM-10 | `PROPOSED: cargo test -p nexus-core memory_transfer` — full local→loopback-remote transfer with verification, discard rollback, and the post-target-writes reverse-transfer path; no target-written history loss |
| NXM-13: Fence mapping: subject- and context-scoped fences provider-enforced across clients (binding removal = context target); Nexus reversible archive maps to fence/unfence, terminal protocol `archive` unused for restore; drain barrier `pending_receipts == 0` gating export/cutover | fullstack-dev | NXM-10 | `PROPOSED: cargo test -p nexus-core memory_fencing` — cross-client fence refusals (`SUBJECT_FENCED`, context-target fence), archived read success + unfence restore across restart, freeze allowlist behavior |

### Stage N5 — Removal and closure

| Task | Owner | Deps | Acceptance |
|---|---|---|---|
| NXM-14: Removal list execution: delete direct `memory_io`/`soul_io` call sites left dead after N2; remove dual-meaning `scope_id` internals in favor of typed contexts; delete obsolete comments referencing removed entrances | fullstack-dev | NXM-6..8 | `grep` audit: no production caller of `nexus_creator_memory::memory_io`/`soul_io` outside the adapter crate seam; `cargo check` clean |
| NXM-15: Final DoD audit against §9 | architect + qa-engineer | NXM-1..14, NXM-16 | DoD checklist all green; 04 matrix re-run green |

## 8. Deferred triggers

| Deferred item | Trigger condition | Tracking home |
|---|---|---|
| Automatic run capture re-enable (E5) | Explicit product authorization; new acceptance revision of 04 | Decision D-4; future plan |
| `MergeIntoExisting` / `TriggerSoulExperienceOnly` review actions | Upstream advertises them as implemented; until then 01/04 mark them unsupported | Decision D-3 |
| Physical data migration off current tables/files | Product decision to leave the legacy layout; requires its own export/import runbook (NXM-12 machinery reusable) | §5.1 |
| Multi-space local operation (beyond one default space per open) | Confirmed multi-tenant requirement | §4.5 |
| LLM-assisted review classification / digest summarization | Separate capability decision; protocol explicitly allows algorithm freedom | 04 §2 |

## 9. Final definition of done (for the adoption program, not this document)

1. Every entry point in §6 routes through the CoreService memory family →
   `MemoryPort`; zero direct memory table/file bypass remains (grep-audited).
2. Default local provider passes the full 04 conformance matrix; the loopback
   independent remote provider passes the same matrix.
3. Writer-guard triggers, revision CAS, and bearer path validation behave
   byte-identically for local operation.
4. Receipts/idempotency for memory ops verified incl. `OUTCOME_UNCERTAIN`
   replay; no peer-session-derived identity anywhere in the memory domain.
5. Transfer runbook (NXM-12) demonstrated local→remote and back with
   explicit rollback points; no dual-write or silent fallback exists.
6. D-4 boundary intact: `remember:true` refusal unchanged and covered by a
   regression test; Creator **explicit** capture parity (NXM-16) passes its
   typed contract scenarios.
7. Control-metadata migration is registered/idempotent; binary/schema
   rollback compatibility verified or backup-restore path documented.
8. Removal list (NXM-14) executed; docs updated (ARCHITECTURE.md memory
   section, CLI help).

## 10. Risks

| Risk | Mitigation |
|---|---|
| CLI reroute changes interactive UX subtly (errors now service-shaped) | Preserve message text where user-facing; AC in NXM-6 compares output fixtures |
| SQL+file crash windows widen under provider concurrency | §5.2 durable intent journal + staged-file publish + NXM-3 kill-injection fixtures; recovering objects isolated from recall until resolved |
| Old-binary rollback after additive DDL corrupts or loses control metadata | §5.1 rollback constraint: matched binary + consistent backup, or explicit compatibility verification; never bare revert |
| Post-cutover rollback loses target-written memory | §5.3 step 5 two-case rule: abandon-only before target writes; verified reverse transfer after; explicit blocked/uncertain status until reconciled |
| Receipt store extension touches shared schema | NXM-2 isolated migration; compute consumer behavior pinned by existing tests |
| Loopback remote diverges from real remote semantics | 04 forbids echo mocks; the loopback provider enforces full `MemoryPort` semantics with private durable storage and its own policy implementation (the fixture engine is at most an offline conformance reference); live-provider evidence requires separate authorized workflow |
| Scope-typing migration (gap 4) mis-maps legacy rows | Typed placement/provenance mapping table reviewed in NXM-14; provenance-preserving |
| Freeze allowlist misuse becomes a write backdoor | Allowlist is exactly 01 §8.2 (read-only + unfreeze + revoke + exports.* + imports.*); C-011/C-015 pin behavior |

## 11. Decision log

- **D-1**: Default keeps physical layout; protocol adoption ≠ data merge (§5.1). Rationale: zero-downtime rollback, writer-guard preservation.
- **D-2**: One `MemoryPort` boundary; no local DTO clone, no legacy shim layer, no second storage protocol. Rationale: agreed direction §9 of the package brief.
- **D-3**: Unimplemented review actions stay unadvertised; capability/`op_unsupported` is the honest refusal. Evidence: `memory_pipeline.rs:319-418`.
- **D-4**: Automatic run capture remains refused; re-enable is a separate authorized product decision with its own acceptance revision (§6.4). Evidence: `host.rs:1208-1230`.
- **D-5**: Memory operation identity uses memory-space binding scope, never Connect peer session scope (§5.4). Evidence: `operation_id.rs:39-55`.
- **D-6**: Remote transport is SPOKE Connect; `nexus-cloud-sync` is not involved (§4.1). Evidence: `docs/ARCHITECTURE.md:37-58`.
- **D-7**: Nexus keeps its nine-value `MemoryKind` taxonomy via a lossless namespaced kind mapping plus frontmatter/section preservation (§4.6); no forced collapse into core `event`/`fact`/`note`.
- **D-8**: ToM stays in World KB; memory references, never copies (§4.5).
- **D-9**: Durable receipts are content-free summaries; live data is re-read under current authorization; dedup tombstones outlive receipts (§5.4).
- **D-10**: Cross-store writes use a durable intent journal + staged-file publish; no file+SQL atomic-transaction claim; recovering objects are isolated from recall (§5.2).
- **D-11**: Post-activation rollback is two-case: discard/abandon before target writes; freeze+drain+verified reverse transfer after; never delete target history, never dual authority (§5.3).
- **D-12**: ProfileDocuments are content kinds only (`personality`/`preferences`); provider policy is configuration, never profile content (§4.5).
- **D-13**: Nexus reversible archive/restore maps to fence/unfence; the terminal protocol `archive` is not used for restore (§5.3).
- **D-14**: Creator explicit capture parity is added (NXM-16) while automatic Host `remember` stays refused (D-4); the two are independent decisions.

## 12. Evidence appendix

Verified source windows (read 2026-10-28): `crates/nexus-creator-memory/Cargo.toml:12-15`; `crates/nexus-creator-memory/src/bearer.rs:1-46`; `crates/nexus-creator-memory/src/review.rs:119-263`; `crates/nexus-creator-memory/src/memory_item.rs:24-73`; `crates/nexus-core/src/memory_pipeline.rs:59-123,314-428,644-708`; `crates/nexus-core/src/memory.rs:1-123,176-260,356-388,557-620,740-830,953,1219-1262`; `crates/nexus-core/src/host.rs:1199-1243,1949-2028`; `crates/nexus-core/src/soul.rs:619-733`; `crates/nexus-core/src/service.rs:54-113`; `crates/nexus-core/src/operation_receipts.rs:1-63`; `crates/nexus-core/src/operation_id.rs:1-63`; `crates/nexus-local-db/src/pending_review.rs:36,220-314`; `crates/nexus-local-db/migrations/20260906000003_character_run_capture.sql:1-40`; `crates/nexus-local-db/migrations/20260912000001_core_writer_protocol.sql:1-60`; `crates/nexus-moment-context-assembly/src/stage0.rs:18-51,189-302`; `crates/nexus-moment-context-assembly/src/cloud_stage.rs:9-87,246-274`; `crates/nexus-moment-context-assembly/src/moment.rs:24-220`; `crates/nexus-core-node/src/actors.rs:1046-1350`; `apps/nexus-service/src/memory.ts:1-40`; `apps/nexus42/src/commands/creator/memory.rs:14-337`; `apps/nexus42/src/commands/creator/soul.rs:7-216`; `apps/nexus42/src/commands/creator/character.rs:17-53,111-133`; `apps/nexus42/src/auth/user_auth.rs:8`; `crates/nexus-cloud-sync/src/platform_client.rs:37-95,308-348`; `crates/nexus-provider-ports/src/lib.rs:1-30`; `crates/nexus-provider-conformance/src/lib.rs:1-25`; `docs/ARCHITECTURE.md:29-68`.

Known factual limits: exact CLI subcommand help text after reroute (depends on
implementation). The upstream implementation task series (A1–A3, B1–B4, C1–C3,
D1–D5, E1–E4, F1–F2) that this plan originally tracked as a dependency was
confirmed final by the protocol owner on 2026-10-28 and is now historical: the
family landed as 0.15.0, so no upstream task ID gates any stage below. The
`MEM-R001`–`MEM-R027` invariant numbering and all v2.0 wire symbols
cross-referenced by [spoke-memory-conformance.md](spoke-memory-conformance.md)
are frozen (received 2026-10-28).
