---
module: spoke-memory protocol family (schemas + operations + Connect arbitration)
date: 2026-10-28
status: proposed
problem_type: design
category: architecture
severity: info
tags:
  - spoke
  - memory
  - protocol
  - capability-family
  - rfc2119
---

# SPOKE Memory Protocol — RFC-style Proposal (spoke-memory)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. The upstream protocol surface is landed and published; **no nexus code implements or serves this family**, and this document authorizes no work. |
| **Document class** | Exploration |
| **Scope** | The `spoke-memory` wire and service contract: operation names, wire fields, capability IDs, normative invariants (`MEM-R###`), data models, lifecycle, error codes, and the amendment it makes to the "Author Memory is product-local" boundary. Behavior is owned by upstream SPOKE; this document is the nexus-side reading of it plus the adoption constraints. |
| **Coordinates with** | [spoke-memory-adoption-plan.md](spoke-memory-adoption-plan.md) (nexus rollout stages `NXM-*`), [spoke-memory-conformance.md](spoke-memory-conformance.md) (`C-*` conformance matrix), [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md) (the SPOKE consumption boundary this family would join), [entity-scope-model.md](../../active/architecture/entity-scope-model.md) (scope hierarchy a memory space is provisioned against) |

**Landing reconciliation (2026-11-01).** Written as a research proposal against SPOKE 0.14.1, this family has since landed upstream: release 0.15.0 (2026-10-30) added it additively (schemas + codegen, `MemoryPort` + validators, Connect dispatch rows, `RemoteMemoryService`, and a fixtures-only reference engine), and 0.15.1 (2026-10-31) is docs/CI only; both crates.io and npm publish 0.15.1. The reference policy engine is `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`), a path-dependency-only crate that is **never published** — it is implementation, not protocol, so it is not part of the surface a conforming provider must reproduce (§1.1 still binds: the protocol surface is schemas + operations + Connect arbitration). This nexus repository moved its five lockstep pins 0.14.1 → 0.15.1 in the same round as this promotion (the re-pin also carries the upstream generated-type rename sweep in its consumers), but consumes neither side of the family: nothing below is adopted, and §18.3's "not current" statements describe the pre-landing repository snapshot, not today's upstream.

### Original framing (research provenance)

- **Status:** Proposed. Nothing in this document was implemented in the SPOKE repository at research time. Every normative statement below (RFC 2119 MUST/SHOULD/MAY) binds only a conforming implementation of this proposal. §18 records the repository state the proposal was written against and the exact boundary it amends.
- **Protocol family:** `spoke-memory` — optional, independent capability family. It does not extend `spoke-baseline`, does not add fields to `KnowledgeEntry`, and does not use `modules.memory` (which does not exist; see §18).
- **Wire protocol version:** `memory_protocol_version = 1` (integer, independent of SPOKE package versions).
- **SSOT roles:** This document is the single source of truth for all memory operation names, wire field definitions, capability IDs, and common semantic invariants. Companion documents ([spoke-memory-adoption-plan.md](spoke-memory-adoption-plan.md), [spoke-memory-conformance.md](spoke-memory-conformance.md)) cite these symbols and never redefine them.


The key words MUST, MUST NOT, SHOULD, and MAY in this document are to be interpreted as described in RFC 2119, and apply exclusively to implementations claiming conformance to this proposal.

---

## 1. Scope and non-goals

### 1.1 In scope

- A complete wire and service contract for per-subject long-term memory: spaces, subjects, capture, candidate review, retained records, recall, profiles, reflections, grants, write fencing, and versioned export/import.
- The single public replacement boundary `MemoryPort` (in-process) and `RemoteMemoryService` (SPOKE Connect responder seam) through which a product embeds or serves a memory provider.
- A separately reusable reference algorithm engine, delivered as the fixtures-only crate `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`, internal facade type `MemoryEngine`) containing deterministic, I/O-free algorithm policy. The engine receives already-authorized data as pure function inputs and returns classification, ranking, reflection-assembly, and transfer-transformation results. It holds **no storage abstraction** and enforces no authorization, CAS, idempotency, fencing, or transaction semantics — all constraint enforcement belongs to the provider behind `MemoryPort`, in the provider's own transaction layer. The engine is **not a protocol participant and not part of the published protocol surface** (schemas + operations + Connect arbitration): it is path-dependency-only test tooling and a conformance reference, and each provider — including Nexus — implements its own policy under the behavioral-conformance split (§17).
- Concurrency, revocation, freeze, delete, and provider-switching semantics that hold across multiple clients of one authoritative provider.

### 1.2 Non-goals

- No distributed exactly-once delivery, no cross-provider or cross-space atomic transactions, no CRDT, no offline multi-master replication, no federated recall, no silent local fallback. One `MemorySpace` has exactly one authoritative provider (MEM-R001).
- No transport definition beyond mapping onto the existing SPOKE Connect envelope (`schemas/connect/*`, verified §18). No HTTP routes, headers, or methods appear in this protocol.
- No mandate of identical ranking, summarization, or reflection algorithms across providers (§9, item 2). Algorithm policy is engine-versioned, not protocol-versioned.
- No space-creation or provider-bootstrap operation: spaces and initial administrator grants are provider-operator provisioned out of band (§7.0.6). No product identity model: `Subject` is a stable opaque ID with optional namespaced kind metadata; the protocol never closes the kind vocabulary to `Creator|Character` or any product enum, and equal subject IDs across systems never auto-link identities (MEM-R002).
- No changes to `BaselinePorts`, the six baseline port families, the core op table (`upsert|promote|relate|check|assemble|project|compute|extract`), or the `port.*` D4 catalogue, except the additive extension points listed in §13 and in the companion implementation plan.

---

## 2. Terminology

| Term | Definition |
|---|---|
| **MemorySpace** | One logical memory store with exactly one authoritative provider. The unit of fencing, export, and import activation. |
| **Subject** | The entity memory is about. Stable opaque `subject_id` chosen by the integrating product; optional namespaced `kind` metadata. |
| **Principal** | An authenticated caller identity, established by the transport/session or in-process admission context — never asserted by the requester through payload fields (MEM-R003). Payloads may *select* a target principal (e.g. a grantee), never impersonate the requester (§7.0.2). |
| **MemoryGrant** | A provider-enforced authorization record binding (principal, space, optional subject, capability set, optional context restriction, shared-access flag). |
| **RetainedRecord** | A reviewed or directly written durable memory record. Untrusted content; trusted only as to provenance and scope metadata. |
| **MemoryCandidate** | A provisional, not-yet-retained capture product with an immutable admitted `capture_scope`. Never visible to `memory.recall` before review (MEM-R004). |
| **ProfileDocument** | Author-maintained, trusted subject document (content kinds; core: `personality`, `preferences`). |
| **Reflection** | A derived, generated synthesis over a pinned basis (selection + provenance filter + authorized source set + revisions + policy). |
| **Context** | A placement/visibility partition within a subject (product-meaningful ID, e.g. a binding). Placement (where a record lives), provenance (where material came from), and read selection (what a view includes) are independent concepts. |
| **ProvenanceFilter** | A read/projection filter over `provenance.source_context_ids` — e.g. a Creator per-World projection. Never a placement statement. |
| **MemoryReceipt** | Durable, payload-bound, content-free operation receipt; the single async/outcome model of this protocol (§7.0.3). |
| **MemoryExportBundle / MemoryExportChunk** | Versioned, digested logical export artifact and its bounded wire chunks (§6.13). |

---

## 3. Versioning

Three independent version axes; conforming implementations MUST NOT conflate them:

1. **`memory_protocol_version`** (integer ≥ 1, this document = 1). Wire contract. Carried in `memory.describe` responses and `MemoryExportBundle`. Breaking wire changes increment it; old/new interop rules are out of scope for v1 (single-version deployments).
2. **Package versions** (SPOKE repo lockstep, currently `0.15.1` per `Cargo.toml` `[workspace.package]` and the published packages). Orthogonal to protocol versions.
3. **`engine_policy_version`** (opaque string). Reference-engine algorithm policy (candidate classification, recall scoring, reflection assembly). Surfaced by `memory.describe` and recorded on each `Reflection.generator`. Two providers with equal `memory_protocol_version` and different `engine_policy_version` are wire-compatible and algorithm-variable (§9, §17).

---

## 4. Capability and profile compatibility

### 4.1 Capability flags

| Capability ID | Gates | Required for conformance |
|---|---|---|
| `spoke-memory` | All `memory.*` ops except those gated by the two sub-flags below | Yes (memory family core) |
| `spoke-memory-reflection` | `memory.reflections.state`, `memory.reflections.list`, `memory.reflections.generate` | Optional |
| `spoke-memory-transfer` | `memory.exports.create`, `memory.exports.get`, `memory.imports.stage`, `memory.imports.activate`, `memory.imports.discard` | Optional |

Rules:

- A provider declaring `spoke-memory-reflection` or `spoke-memory-transfer` MUST also declare `spoke-memory`.
- A provider declaring `spoke-memory` without a sub-flag MUST reject the sub-flag's ops with `capability_missing` at the dispatch/admission boundary, with zero side effects (mirrors the existing fail-closed dispatch rule in `crates/spoke-connect/src/core/dispatch.rs:52-58`).
- Hosts advertise these flags in `HostCapabilityManifest.capabilities[]` (open string vocabulary, same carrier as `spoke-baseline` / `l2-computable` / `ke-extraction`; verified §18). Connect peers negotiate them in the hello exchange; the dispatch gate evaluates **negotiated** capabilities only.

### 4.2 Profile compatibility

`ProfileDocument.profile_kind` is an open string vocabulary of **authored content kinds**. Core vocabulary (documented, not schema-enforced): `personality`, `preferences`. Profile kinds are never capability concepts — provider features are expressed exclusively through the §4.1 capability flags, never through the presence or absence of profile documents.

- Each provider declares `required_profile_kinds` in `memory.describe` — a list of authored content kinds a subject must present before reflection generation is admitted. Admission fails with `PROFILE_INCOMPLETE` if any required kind is absent or empty for that subject — no empty success (MEM-R005).
- Absence of a *non-required* profile kind is honest-empty: `memory.profile.get` returns fewer documents, never an error.
- Provider **selection** failure (missing capability) and profile **admission** failure (missing authored content) are distinct outcomes (`capability_missing` vs `PROFILE_INCOMPLETE`) and MUST NOT be collapsed.
- A complete Nexus replacement profile set is product policy defined in doc 03 on top of this matrix; this protocol defines only the admission failure semantics.

---

## 5. Normative invariants

Anchors `MEM-R###` are the conformance reference points; [spoke-memory-conformance.md](spoke-memory-conformance.md) owns the `C-*` scenario IDs that exercise them.

| Anchor | Invariant |
|---|---|
| MEM-R001 | One authoritative provider per `MemorySpace`. Multi-client write access goes through that authority under CAS and grants. |
| MEM-R002 | Equal IDs across systems never auto-link identities; identity mapping is explicit product policy (e.g. import `subject_id_map`, `adopt_grants`). |
| MEM-R003 | Principal, ownership, consuming subject, and grants derive from authenticated context and provider state. Model- or payload-supplied type/owner/grant JSON is never authority. Payload principal fields only select targets for admin operations. |
| MEM-R004 | Candidates are never returned by `memory.recall` and never count in recall totals. |
| MEM-R005 | Profile admission fails closed with `PROFILE_INCOMPLETE` (missing required authored kinds); capability absence fails with `capability_missing`; optional absent data is honest-empty. No empty success. |
| MEM-R006 | Authorization (grant coverage incl. context restriction and shared access) is evaluated **before** ranking and pagination; counts, pages, and cursors reflect only authorized records. |
| MEM-R007 | Every mutable object carries `revision` (integer ≥ 1); conditional writes require `expected_revision` and fail with `REVISION_CONFLICT` on mismatch. Genuine no-ops do not increment revisions (§7.0.4). |
| MEM-R008 | Mutating ops require `idempotency_key` (sole exception: `memory.subjects.ensure`, §7.0.5). The provider durably binds (space, principal, op, key) → payload_digest → receipt. Same key + same digest replays the original receipt without re-executing; same key + different digest fails `IDEMPOTENCY_CONFLICT`. |
| MEM-R009 | Candidate consumption and outcome publication commit atomically at the provider boundary: a review/dismiss either fully consumes the candidate and writes all outcomes, or has no effect. |
| MEM-R010 | After timeout, transport failure, or uncertain outcome, the client retries with the same `idempotency_key` and/or polls `memory.receipts.get`; the provider never double-applies. |
| MEM-R011 | Read-only ops (`memory.describe`, `memory.spaces.list`, `memory.subjects.get`, `memory.grants.list`, `memory.candidates.list`, `memory.candidates.get`, `memory.records.get`, `memory.records.list`, `memory.recall`, `memory.profile.get`, `memory.reflections.state`, `memory.reflections.list`, `memory.receipts.get`, `memory.exports.get`) MUST NOT mutate persistent state and MUST NOT trigger reflection generation or any capture. |
| MEM-R012 | Reflection generation is explicit, authorized (`reflect` grant covering the pinned selection/filter), and cost-recorded; it is never a side effect of reads. |
| MEM-R013 | Any change to a reflection's pinned basis — new/deleted/edited records in scope, profile updates, scope promotions moving records into/out of scope — makes it read as `stale: true`. |
| MEM-R014 | Deletion removes record content, excludes the record from recall/export-by-default, and preserves a tombstone plus all receipts (which are content-free, MEM-R022) for audit. |
| MEM-R015 | All collections are finite and paginated; all request, stored-material, and response sizes have declared limits (`memory.describe.limits`, §7.1). |
| MEM-R016 | Errors are stable machine codes (§14) with an explicit retry class: `retryable`, `terminal`, or `uncertain`. |
| MEM-R017 | Space freeze and subject/context fence are provider-enforced at admission across all clients of the authority, not per-process locks. |
| MEM-R018 | Grant revocation takes effect at admission of subsequent ops; ops that already passed admission complete and issue receipts. Revoked principals cannot read receipts (§7.0.5). |
| MEM-R019 | No fallback: when `spoke-memory` is not negotiated/available, clients MUST NOT emulate memory semantics through `KnowledgeEntry`, `modules.*`, `Scope.viewpoint`, or `port.*` ops. |
| MEM-R020 | The memory grant model is self-contained. It does not inherit, imply, or consult `ke-ownership` / `Scope.viewpoint` disclosure semantics, and memory ops never accept a `Scope` object as authorization input. |
| MEM-R021 | Replay never re-evaluates superseded CAS/fence preconditions of an already-committed effect; receipt disclosure still requires current authorization. Dedup tombstones outlive receipt bodies (`dedup_retention_seconds`); replay against an evicted receipt fails `RECEIPT_EXPIRED`, never silently re-executes. |
| MEM-R022 | `MemoryReceipt.result` is content-free: IDs, revisions, counts, outcome enums, digests, and cost facts only. Live objects are obtained through read ops under current authorization. |
| MEM-R023 | A frozen space admits exactly the §8.2 maintenance allowlist; quiescent export and cutover require the drain barrier (`pending_receipts == 0`). |
| MEM-R024 | Context-scoped fences reject writes to the fenced context across all clients while sibling contexts remain usable. |
| MEM-R025 | Transfer preserves candidates (as content-scrubbed shells), content-free receipt/dedup summaries, and all reference maps; grants are never auto-activated on import (explicit `adopt_grants` only); imported reflections enter `stale: true`. |
| MEM-R026 | Cursors bind principal + space + subject + selection/filter + snapshot + `engine_policy_version`; state or grant change invalidates them with `CURSOR_INVALID`; every by-id read rechecks current authorization. |
| MEM-R027 | Exactly one authority exists through cutover and rollback: the old provider stays frozen and rejects writes; after the target has accepted writes, rollback requires verified reverse transfer (delta reconciliation) before the source reopens. Never dual authority; never deletion of target-written history. |

---

## 6. Data models

Conventions follow the verified schema authoring rules (`schemas/README.md`): draft-07, closed objects (`additionalProperties: false`), `schema_version` + `extensions` required on data objects, open string vocabularies documented in descriptions, absolute `$ref` URIs. Field tables below are normative; "—" in the Null/Omission column means the field is required and non-null.

### 6.1 Common primitives

| Name | Type | Semantics |
|---|---|---|
| `schema_version` | integer ≥ 1 | Per-object wire version (existing `SchemaVersion` def). |
| `*_id` | string, minLength 1 | Opaque. `subject_id` is product-chosen and stable; all other IDs are provider-assigned unless stated. |
| `revision` | integer ≥ 1 | Per-object optimistic-concurrency counter; increments on each accepted state-changing write (MEM-R007). |
| timestamps | string RFC 3339 UTC | Existing `Timestamp` def. |
| `extensions` | ExtensionMap | Existing def; adapters round-trip unknown namespaces verbatim. |
| `idempotency_key` | string, 1–128 chars | Caller-chosen; unique within the dedup namespace (§7.0.5). |
| `payload_digest` | string, 64 lowercase hex chars | SHA-256 over the RFC 8785 JCS canonicalization of the request payload object. Binding to the op comes from the dedup namespace (§7.0.5), which includes the op name. |
| `cursor` | string, opaque | Paginates one bound query (MEM-R026). |

### 6.2 `MemorySpace`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `space_id` | string | — | Provider-assigned at operator provisioning. |
| `display_name` | string | optional | Human label; absence = unnamed. |
| `authority_host_id` | string | — | `host_id` of the single authoritative provider (MEM-R001). |
| `authority_generation` | int | — | ≥ 1; increments on each cutover (import activation into the space) and on each operator re-open after rollback. Client bindings pin (authority_host_id, authority_generation) (§12.5). |
| `status` | string | — | `active` \| `frozen`. Frozen admits only the §8.2 allowlist. |
| `pending_receipts` | int | — | Computed at read time: count of receipts with status `pending`. The drain barrier (MEM-R023). |
| `created_at` | Timestamp | — | |
| `revision` | int | — | Bumped by freeze/unfreeze and cutover. |
| `extensions` | ExtensionMap | — | |

### 6.3 `Subject`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `space_id` | string | — | |
| `subject_id` | string | — | Product-chosen stable opaque ID. |
| `kind` | object | optional | Namespaced kind metadata: `{ "ns": string (ExtensionMap key pattern `^[a-z][a-z0-9_-]*$`), "id": string }`. Absence = kind unspecified. Never a closed enum; never authorization input. |
| `status` | string | — | `active` \| `fenced` \| `archived` (§8.3). `archived` is terminal decommission; products with reversible archive semantics map them to `fence`/`unfence` (§7.7). |
| `fenced_contexts` | string[] | — | Currently context-fenced placement IDs (may be empty; MEM-R024). |
| `created_at` | Timestamp | — | |
| `revision` | int | — | Bumped by any fence/unfence/archive transition. |
| `extensions` | ExtensionMap | — | |

### 6.4 `Principal`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `principal_id` | string | — | Authenticated identity as established by the admission context. |
| `display_name` | string | optional | |
| `extensions` | ExtensionMap | — | |

Principals appear **only** inside provider state (grants, receipts, audit fields) and as *target selectors* in admin operations. The requester's principal always comes from the authenticated context (MEM-R003): over Connect, the hello/capability-token peer identity is mapped to a `principal_id` by provider-local configuration; in-process, the embedding product supplies it. A payload `principal_id` (e.g. in `memory.grants.grant`) names the *target grantee*; it never reassigns the requester.

### 6.5 `MemoryGrant`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `grant_id` | string | — | Provider-assigned. |
| `space_id` | string | — | |
| `subject_id` | string | optional | Absence = space-wide grant. |
| `principal_id` | string | — | Grantee (target). |
| `capabilities` | string[] | — | minItems 1, uniqueItems. Open vocabulary; core: `capture`, `review`, `records_read`, `records_write`, `recall`, `profile_write`, `reflect`, `promote`, `transfer`, `admin`. `admin` implies grant management and all other capabilities within the grant's coverage. |
| `context_ids` | string[] | optional, minItems 1 | Context restriction over **placement** contexts. **Omitted** = unrestricted within the grant's subject/space scope (includes shared visibility and `subject_all` selection). **Present** = only the listed placement contexts, plus shared visibility iff `shared_access` is true. An empty array is invalid (minItems 1). |
| `shared_access` | bool | optional | Default `false`. Meaningful only when `context_ids` is present; MUST be omitted when `context_ids` is omitted. When true, the grant additionally covers shared-visibility data and `shared_only` / `include_shared` selections — still never `subject_all`. |
| `created_at` | Timestamp | — | |
| `expires_at` | Timestamp | optional | Absence = no expiry. Expired grants are treated as revoked. |
| `revoked_at` | Timestamp | optional | Set by `memory.grants.revoke`; terminal. |
| `revision` | int | — | |
| `extensions` | ExtensionMap | — | |

**Grant evaluation** (MEM-R006, MEM-R017). An op is admitted iff the authenticated principal holds at least one grant that is unexpired, unrevoked, matches the space, matches the subject (or is space-wide), includes the op's required capability (§7 tables), and covers the op's data access:

- Ops naming placement context IDs (capture placement, record write/read by placement, context fence): grant `context_ids` omitted, or covers every named context.
- Reads selecting shared visibility (`shared_only`, `include_shared: true`, by-id read of a shared record, profile reads, whole-subject exports): grant `context_ids` omitted, or `shared_access: true`.
- `subject_all` selection: grant `context_ids` MUST be omitted (unrestricted subject or space-wide grant). A context-limited grant never obtains `subject_all`, and never obtains shared access implicitly by naming no contexts on an op — the selection mode itself is checked against coverage.
- Reflections: the grant must cover the reflection's pinned selection + filter exactly as above (§7.25–7.27).

**Non-amplification** (§7.9): a granter may only grant a subset of its own effective coverage — capabilities, subject scope, `context_ids`, `shared_access`, and `expires_at` (a granter whose grants expire cannot issue longer-lived or non-expiring grants).

### 6.6 `RetainedRecord`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `record_id` | string | — | Provider-assigned. |
| `space_id`, `subject_id` | string | — | |
| `kind` | string | — | Open vocabulary. Core (documented, not enforced): `event`, `fact`, `note`. Product-specific kinds are product policy, never protocol vocabulary. |
| `content` | object | — | `{ "text": string, "structured": OpaqueJson (optional) }`. **Untrusted** content (§10). |
| `scope` | object | — | **Write placement**: `{ "visibility": "shared" \| "context", "context_id": string }`. `context_id` is required iff `visibility = "context"`, and MUST be omitted when `visibility = "shared"`. Changed only by `memory.records.promote`. |
| `provenance` | object | — | `{ "source_context_ids": string[] (may be empty), "anchors": SourceAnchor[] (may be empty), "captured_by": principal_id, "captured_at": Timestamp }`. `captured_by`/`captured_at` are always provider-populated; `source_context_ids`/`anchors` are recorded client claims (§10). Provenance is independent of placement. Reuses the existing `SourceAnchor` wire object (`schemas/data/source-anchor.schema.json`). |
| `import_ref` | object | optional | Present iff created by import activation: `{ "import_id": string, "source_record_id": string, "source_revision": int }`. Historical attribution; never CAS input. |
| `created_at`, `updated_at` | Timestamp | — | |
| `revision` | int | — | |
| `tombstone` | object | optional | Present only after deletion: `{ "deleted_at": Timestamp, "deleted_by": principal_id, "reason": string (optional) }`. A tombstoned record has `content` removed (empty text, no `structured`) and is excluded from recall and default export (MEM-R014). |
| `extensions` | ExtensionMap | — | |

### 6.7 `MemoryCandidate`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `candidate_id` | string | — | Provider-assigned. |
| `space_id`, `subject_id` | string | — | |
| `capture_scope` | object | — | **Immutable admitted placement** from `memory.capture` (§7.11): `{ "visibility": "shared" \| "context", "context_id": string (conditional as 6.6) }`. The authorization fence for review outcomes. |
| `proposed` | object | — | `{ "kind": string, "content": Content (as 6.6) }`. Scrubbed on consumption (below). |
| `provenance` | object | — | Same shape as 6.6; claims recorded, `captured_by` provider-set. |
| `classification` | object | — | Advisory deterministic classification: `{ "disposition": "promote" \| "fragment" \| "drop", "rationale_codes": string[] (may be empty), "confidence": number 0..1 (optional) }`. Advisory only; the reviewer decision is authoritative. |
| `status` | string | — | `pending` \| `consumed`. |
| `consumed_by` | object | optional | Present iff `consumed`: `{ "op": "memory.candidates.review" \| "memory.candidates.dismiss", "receipt_id": string, "outcome": "promoted" \| "fragmented" \| "dropped" \| "dismissed", "record_ids": string[] (may be empty) }`. |
| `content_digest` | string | optional | 64-hex SHA-256 over the JCS-canonical `proposed.content`; set when content is scrubbed at consumption, retained for dedup/audit (MEM-R025). |
| `created_at` | Timestamp | — | |
| `expires_at` | Timestamp | optional | Provider policy; expired pending candidates are excluded from lists (not consumed). |
| `revision` | int | — | |
| `extensions` | ExtensionMap | — | |

**Consumption scrub:** when a candidate is consumed, `proposed.content` is removed from the candidate shell (promoted/fragmented content now lives in the created records; dropped/dismissed content is discarded) and `content_digest` is set. The shell (IDs, capture_scope, classification, provenance, outcome) is retained for audit until `expires_at`.

### 6.8 `ProfileDocument`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `space_id`, `subject_id` | string | — | Unique per (space, subject, profile_kind). |
| `profile_kind` | string | — | Open vocabulary of authored content kinds; core `personality` \| `preferences` (§4.2). |
| `document` | object | — | `{ "format": "markdown" \| "json", "body": string }`. **Trusted** author-maintained content (§10). |
| `updated_by` | principal_id | — | Provider-populated from the authenticated context. |
| `created_at`, `updated_at` | Timestamp | — | |
| `revision` | int | — | |
| `extensions` | ExtensionMap | — | |

### 6.9 `Reflection`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `reflection_id` | string | — | Provider-assigned. |
| `space_id`, `subject_id` | string | — | |
| `selection` | ReadSelection | — | Pinned placement selection used for generation. |
| `filter` | ProvenanceFilter | optional | Pinned provenance filter (absent = none). |
| `basis` | object | — | `{ "source_set_digest": string (64 hex over the authorized source record ID+revision set), "record_count": int, "profile_revisions": {profile_kind: revision}, "state_digest": string (64 hex, §7.25) }`. Pins the exact inputs and coverage. |
| `body` | object | — | `{ "format": "markdown" \| "json", "body": string }`. Derived content; trusted only as to its pinned basis and generator. |
| `stale` | bool | — | Authoritative value is computed at read time (§7.26): true iff the current authorized basis differs from the pinned basis (MEM-R013). The stored flag is a write-time cache. |
| `generator` | object | — | `{ "engine_policy_version": string, "model": string (optional) }`. |
| `cost` | object | optional | `{ "units": number, "unit_kind": string }`. Content-free cost fact; survives failed generation without exposing input text (§14.3). |
| `created_at` | Timestamp | — | |
| `revision` | int | — | |
| `extensions` | ExtensionMap | — | |

**Privacy exclusion:** a reflection whose pinned basis references any tombstoned record is excluded from `memory.reflections.list` and from export — deleted content must not remain reachable through derived text (§11).

### 6.10 `MemoryReceipt`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `receipt_id` | string | — | Provider-assigned; returned by every mutating op. |
| `op` | string | — | The op string that produced it. |
| `space_id` | string | — | |
| `requester_principal_id` | string | — | For receipt-read authorization (§7.28). |
| `idempotency_key` | string | optional | Echo when supplied (always, except `memory.subjects.ensure`). |
| `payload_digest` | string | — | MEM-R008 binding. |
| `status` | string | — | `committed` \| `pending` \| `failed`. |
| `result` | OpaqueJson | optional | **Content-free** op summary (MEM-R022): IDs, revisions, counts, outcome enums, digests, cost facts. Present iff `committed`. Never contains record/candidate/profile/reflection bodies. |
| `error` | ErrorEnvelope | optional | Present iff `failed`; reuses the existing `schemas/common/error-envelope.schema.json`. |
| `created_at` | Timestamp | — | |
| `completed_at` | Timestamp | optional | Present iff status ≠ `pending`. |
| `extensions` | ExtensionMap | — | |

Receipt bodies are retained for `receipt_retention_seconds`; afterwards a content-free dedup tombstone (namespace key, payload_digest, receipt_id, terminal status) is retained for `dedup_retention_seconds` (MEM-R021).

### 6.11 `MemoryExportBundle` (logical artifact)

The bundle is the logical whole reassembled from chunks (§6.13). Fields:

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `schema_version` | int | — | |
| `bundle_format_version` | int | — | Starts at 1; independent of `memory_protocol_version`. |
| `memory_protocol_version` | int | — | Protocol version of the exporting provider. |
| `export_id` | string | — | |
| `space_id` | string | — | Source space. |
| `created_at` | Timestamp | — | |
| `source` | object | — | `{ "authority_host_id": string, "authority_generation": int, "engine_policy_version": string }`. |
| `subjects` | Subject[] | — | May be empty only if the exported scope has no subjects. |
| `records` | RetainedRecord[] | — | Tombstoned records excluded unless `include_tombstoned: true` was requested. |
| `candidates` | MemoryCandidate[] | — | Shells (content-scrubbed, `content_digest` present) for consumed candidates; full pending candidates. Preserves consumed-operation history (MEM-R025). |
| `profiles` | ProfileDocument[] | — | May be empty. |
| `reflections` | Reflection[] | optional | Present iff `include_reflections: true`. Excludes tombstone-referencing reflections (§6.9). |
| `receipt_summaries` | object[] | — | Content-free dedup provenance: `{ "receipt_id", "op", "requester_principal_ref": string (opaque label), "idempotency_key" (optional), "payload_digest", "status", "created_at" }`. Enables reimport/replay duplicate suppression without transferring live receipt state. |
| `grants` | object[] | — | **Redacted** grant summaries `{ "grant_id", "subject_id" (optional), "principal_ref": string (opaque correlation label — never credentials/tokens), "capabilities", "context_ids" (optional), "shared_access" (optional), "revision": int }`. Sufficient for explicit administrator adoption via `adopt_grants`; never auto-activated (MEM-R025). |
| `digest` | string | — | SHA-256 hex over JCS of the bundle object with the `digest` field removed. |

Secrets, credentials, and capability tokens MUST NOT appear anywhere in a bundle.

### 6.12 `ReadSelection`

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `mode` | string | — | `subject_all` \| `shared_only` \| `selected_contexts`. |
| `context_ids` | string[] | conditional | Required, minItems 1 iff `mode = "selected_contexts"`; MUST be omitted otherwise. |
| `include_shared` | bool | conditional | Allowed only with `mode = "selected_contexts"`; omission there means `false`. MUST be omitted for other modes. |

Semantics: `subject_all` = every record of the subject regardless of placement or provenance (requires unrestricted grant, §6.5). `shared_only` = only `scope.visibility = "shared"` (requires shared access). `selected_contexts` = records whose placement `context_id` ∈ `context_ids`, plus shared records iff `include_shared = true` (requires coverage of every listed context; shared access when `include_shared`). Whole-subject management views use `subject_all`; ordinary execution views use `shared_only` or `selected_contexts`. No mode is expressed by null/empty-array overloading.

### 6.13 `ProvenanceFilter` and `MemoryExportChunk`

`ProvenanceFilter`: `{ "source_context_ids": string[] (minItems 1) }` — matches records/candidates having at least one provenance source context in the list. A **filter**, never a placement statement: a Creator per-World projection filters provenance while placement stays untouched; a Character binding is placement. Selection (§6.12) and filter combine by AND.

`MemoryExportChunk` (bounded wire unit, MEM-R015):

| Field | Type | Null/Omission | Semantics |
|---|---|---|---|
| `export_id` | string | — | |
| `chunk_ordinal` | int ≥ 0 | — | Strict order. |
| `chunk_count` | int ≥ 1 | — | |
| `bundle_digest` | string | — | Digest of the logical bundle (6.11) this chunk belongs to. |
| `chunk_digest` | string | — | SHA-256 over JCS of this chunk object minus `chunk_digest`. |
| `header` | object | optional | Present iff `chunk_ordinal = 0`: all scalar bundle fields (schema_version, bundle_format_version, memory_protocol_version, export_id, space_id, created_at, source, digest). |
| `subjects` / `records` / `candidates` / `profiles` / `reflections` / `receipt_summaries` / `grants` | arrays | optional | Slices of the bundle arrays, enumerated in §6.11 field order. Partitioning rule: arrays are concatenated in that fixed order and split into consecutive element runs; one element is never split across chunks. Aggregating a bundle's chunks therefore rebuilds every §6.11 array — `grants` included — so `bundle_digest`/`digest` verify over the reassembled bundle. |
| `bytes` | int | — | Serialized chunk size. ≤ `export_chunk_bytes`, except a single element larger than the limit occupies its own chunk (flagged by `bytes` exceeding the limit; never split, never dropped). |

Reassembly concatenates the slices in ordinal order and field order; verification recomputes `chunk_digest` per chunk and `bundle_digest`/`digest` over the reassembled bundle.

---

## 7. Operations

### 7.0 Common operation contract

**7.0.1 Requests.** Every op request names `space_id`; subject-scoped ops also name `subject_id`. Requests are closed JSON objects. The requester principal never appears in the payload (§6.4).

**7.0.2 Responses.** Read ops return their payload directly. Mutating ops return a `MemoryReceipt` (§6.10) whose `result` is a content-free summary (MEM-R022); callers obtain live objects through read ops, which always re-check current authorization (MEM-R026). Read responses are snapshots: they are not CAS tokens and carry no freshness guarantee beyond their creation instant.

**7.0.3 Async model (single).** All mutating ops either commit synchronously (`status: committed` in the returned receipt) or — only `memory.reflections.generate`, `memory.exports.create`, `memory.imports.stage` (final chunk), `memory.imports.activate` — return `status: pending` and complete asynchronously; completion is observed exclusively via `memory.receipts.get`. No webhooks, streams, or callbacks exist.

**7.0.4 Revisions and no-ops.** A mutating op that changes observable state increments the target object's `revision`. A genuine no-op (e.g. freezing an already-frozen space with matching `expected_revision`, unfreezing an active space) succeeds without incrementing. Content updates are never no-ops.

**7.0.5 Idempotency and replay (MEM-R008/R021).** Every mutating op except `memory.subjects.ensure` REQUIRES `idempotency_key` (absent → `INVALID_INPUT`). `memory.subjects.ensure` is the sole exception: get-or-create is naturally idempotent and its outcome is recoverable via `memory.subjects.get`, giving equivalent uncertain-outcome recovery. The dedup namespace is `(space_id, requester_principal_id, op, idempotency_key)`; reconnects and retries share the authenticated principal, so logical identity is stable across connections. Replay (same namespace + same payload_digest) returns the stored receipt **without re-executing** and without re-evaluating the original CAS/fence preconditions. Replay with the same key and a different digest fails `IDEMPOTENCY_CONFLICT`. Receipt disclosure on replay or `memory.receipts.get` still requires current authorization: a principal whose grants were revoked cannot read receipts (`NOT_AUTHORIZED`). After receipt-body eviction, replay within `dedup_retention_seconds` fails `RECEIPT_EXPIRED` (terminal; `details.last_known_status` carries the tombstoned terminal status) — the effect is never silently re-executed; beyond the dedup window the key is treated as new (declared provider limit).

**7.0.6 Bootstrap.** Spaces and the initial administrator grants are created by provider-operator provisioning (host-local configuration), not by any protocol op. There is no uncallable grant cycle: the operator assigns the first `admin` grant directly in provider storage; all further grants flow through `memory.grants.grant` under the §6.5 non-amplification rule. Clients discover provisioned spaces via `memory.describe` / `memory.spaces.list`.

**7.0.7 Pagination.** List/recall ops accept `cursor` + `limit` (≤ `max_page_size`) and return `next_cursor` (absent = end). Cursors bind principal + space + subject + selection/filter + snapshot marker + `engine_policy_version`; any state or grant change invalidates them with `CURSOR_INVALID` — pages never silently re-filter mid-pagination (MEM-R026).

Limits (`memory.describe.limits`) apply to every op; exceeding a limit fails `INVALID_INPUT` with `details.limit` naming the exceeded limit. Each op below states: purpose — request fields — content-free receipt result (mutations) or payload (reads) — required grant capability — CAS/idempotency/atomicity — errors.

### 7.1 `memory.describe`

Provider self-description. **Grant:** none (authenticated principal only). Read-only (MEM-R011).
- Request: `{}` (plus optional `extensions`).
- Result: `{ "memory_protocol_version": int, "capabilities": string[] (subset of §4.1 actually served), "limits": Limits, "required_profile_kinds": string[] (authored content kinds, §4.2), "engine_policy_version": string }`.
- `Limits` (all integers > 0; application target in parentheses): `max_content_bytes` (request field and stored content), `max_candidates_per_capture` (request), `max_page_size` (response page), `export_chunk_bytes` (response chunk, §6.13), `idempotency_retention_seconds` (stored receipt bodies), `receipt_retention_seconds` (stored receipts), `dedup_retention_seconds` (stored dedup tombstones; ≥ `idempotency_retention_seconds`), `staged_import_ttl_seconds` (stored staged imports), `export_retention_seconds` (stored export chunks).
- Space discovery is exclusively `memory.spaces.list` (paginated); `describe` carries no space catalogue.
- Errors: `INTERNAL_ERROR`.

### 7.2 `memory.spaces.list`

List spaces visible to the principal; also the drain-barrier read (MEM-R023). **Grant:** any valid grant in the space, or provider-configured visibility. Read-only.
- Request: `{ "cursor": string (optional), "limit": int (optional) }`.
- Result: `{ "spaces": MemorySpace[], "next_cursor": string (optional) }`. Each entry includes the computed `pending_receipts`.
- Pagination invariant: no page may contain a space the principal cannot see.

### 7.3 `memory.spaces.freeze`

Freeze a space for maintenance/migration. **Grant:** `admin`. Idempotency key required.
- Request: `{ "space_id", "expected_revision": int, "idempotency_key": string }`.
- Result: `{ "status": "frozen", "pending_receipts": int, "revision": int }`.
- Admission effect (one transaction with the status flip, MEM-R017): subsequent non-allowlist ops fail `SPACE_FROZEN`; already-admitted ops complete (MEM-R018). Freezing an already-frozen space with matching revision is a no-op success (§7.0.4).
- Errors: `REVISION_CONFLICT`, `NOT_AUTHORIZED`, `NOT_FOUND`.

### 7.4 `memory.spaces.unfreeze`

Reverse of 7.3; always allowed on a frozen space (maintenance allowlist, §8.2). Re-opening a decommissioned source after cutover is an **operator** action that also bumps `authority_generation` (§12.5) — protocol ops never auto-unfreeze a cutover source (MEM-R027). Request/result mirror 7.3.

### 7.5 `memory.subjects.ensure`

Idempotent get-or-create of a subject. **Grant:** `capture` or `admin`. Sole `idempotency_key` exemption (§7.0.5).
- Request: `{ "space_id", "subject_id", "kind": Kind (optional) }`. `kind` is set only on creation; on an existing subject the op returns it unchanged with `created: false` (kind mismatch is not an error — kind is metadata, not authority).
- Result: `{ "subject_id", "created": bool, "status": string, "revision": int }`.
- Atomic: single conditional insert.
- Errors: `SPACE_FROZEN`, `SUBJECT_FENCED`, `NOT_AUTHORIZED`, `INVALID_INPUT` (bad kind shape).

### 7.6 `memory.subjects.get`

**Grant:** any capability on the subject. Read-only; rechecks current authorization. Request `{ "space_id", "subject_id" }`; result `{ "subject": Subject }`. Errors: `NOT_FOUND`, `NOT_AUTHORIZED`.

### 7.7 `memory.subjects.fence`

Provider-enforced subject- or context-scoped lifecycle fencing across all clients (MEM-R017, MEM-R024). **Grant:** `admin`.
- Request: `{ "space_id", "subject_id", "target": Target, "action": "fence" \| "unfence" \| "archive", "expected_revision": int, "idempotency_key": string }`, where `Target` = `{ "type": "subject" }` or `{ "type": "context", "context_id": string }`.
- Subject target: `active → fenced` (fence), `fenced → active` (unfence), `active|fenced → archived` (archive; **terminal** decommission). Products with reversible archive semantics (e.g. Nexus) MUST map reversible archive/restore to `fence`/`unfence`; protocol `archive` is never the mapping target of a reversible product flow. Fence state is durable across provider restarts.
- Context target (`fence`/`unfence` only; `archive` → `INVALID_INPUT`): adds/removes `context_id` in `fenced_contexts`. Writes placing into a fenced context fail `SUBJECT_FENCED` with `details.scope: "context"`; sibling contexts remain fully usable (MEM-R024).
- Effects: subject `fenced` rejects all writes for the subject (reads allowed). `archived` additionally excludes the subject's records from recall and default candidate listing; records and receipts are retained; export still allowed.
- Result: `{ "subject_id", "status": string, "fenced_contexts": string[], "revision": int }`.
- Stale in-flight writes: writes admitted before the fence commit complete; writes admitted after fail — never half-applied (§11).
- Errors: `INVALID_STATUS_TRANSITION` (archive→anything, unfence of active subject, unfence of non-fenced context), `INVALID_INPUT` (archive on context target), `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`, `SPACE_FROZEN`.

### 7.8 `memory.grants.list`

**Grant:** `admin` (full view; the optional `principal_id` filter is admin-only) or self-view (a principal lists exactly its own grants). Read-only.
- Request: `{ "space_id", "subject_id" (optional), "principal_id" (optional, admin-only target selector, §6.4), "cursor" (optional), "limit" (optional) }`.
- Result: `{ "grants": MemoryGrant[], "next_cursor" (optional) }`.

### 7.9 `memory.grants.grant`

**Grant:** `admin` with coverage subsuming the grant issued (non-amplification, §6.5).
- Request: `{ "space_id", "subject_id" (optional), "principal_id": string (target grantee — a selector, never requester identity), "capabilities": string[] (minItems 1), "context_ids" (optional, minItems 1), "shared_access" (optional; forbidden without context_ids), "expires_at" (optional), "idempotency_key": string }`.
- Non-amplification checks (all fail `NOT_AUTHORIZED`): granted capabilities ⊆ granter's effective capabilities; granted subject scope ⊆ granter's (space-wide granter may issue subject-scoped; subject-scoped granter only the same subject); granted `context_ids` ⊆ granter's (or granter unrestricted); `shared_access` only if the granter has it; `expires_at` not later than the granter's own earliest expiring applicable grant.
- Result: `{ "grant_id", "revision": int }`.
- Errors: `NOT_AUTHORIZED` (amplification), `INVALID_INPUT`, `SPACE_FROZEN`, `SUBJECT_FENCED`.

### 7.10 `memory.grants.revoke`

**Grant:** `admin`. Allowed during freeze (incident response is on the §8.2 allowlist). Request `{ "space_id", "grant_id", "expected_revision", "idempotency_key" }`; result `{ "grant_id", "revoked": true, "revision": int }`. Terminal; applies to subsequent admissions (MEM-R018). Errors: `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`.

### 7.11 `memory.capture`

Capture raw material for one subject; produces zero or more pending candidates. **Grant:** `capture` covering the declared `capture_scope` (§6.5: context coverage for `context` placement; shared access for `shared` placement).
- Request: `{ "space_id", "subject_id", "content": Content, "capture_scope": Scope (required, immutable placement), "source_context_ids": string[] (optional provenance claims), "anchors": SourceAnchor[] (optional provenance claims), "kind_hint": string (optional, advisory), "occurred_at": Timestamp (optional), "idempotency_key": string }`.
- `capture_scope` (placement, admission-checked, immutable on the candidate) and `source_context_ids`/`anchors` (recorded provenance claims) are distinct and independent.
- Result: `{ "candidate_ids": string[], "classified": int }`. Zero candidates is a valid committed outcome (provider policy, e.g. dedup); the receipt records the count — never an empty success without receipt.
- Atomic: admission check + candidate inserts + receipt commit in one transaction. The deterministic classifier runs inside the transaction boundary or its output is committed with the candidates; classification is advisory (§6.7).
- Capture NEVER writes retained records, profiles, or reflections, and never triggers generation (MEM-R012).
- Errors: `NOT_AUTHORIZED`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `INVALID_INPUT` (oversize content, missing key), `IDEMPOTENCY_CONFLICT`, `OUTCOME_UNCERTAIN` (§14.3).

### 7.12 `memory.candidates.list`

**Grant:** `review`. Read-only. Coverage: a context-restricted review grant sees only candidates whose `capture_scope` it covers (plus shared-placement candidates iff `shared_access`).
- Request: `{ "space_id", "subject_id", "status": "pending" \| "consumed" (optional — omitted = both), "cursor" (optional), "limit" (optional) }`.
- Result: `{ "candidates": MemoryCandidate[], "next_cursor" (optional) }`. Expired pending candidates excluded.

### 7.13 `memory.candidates.get`

**Grant:** `review` covering the candidate's `capture_scope`. Read-only; rechecks current authorization. Request `{ "space_id", "candidate_id" }`; result `{ "candidate": MemoryCandidate }`. Errors: `NOT_FOUND`, `NOT_AUTHORIZED`.

### 7.14 `memory.candidates.dismiss`

Lightweight consumption without a review decision (queue removal). **Grant:** `review` covering the candidate's `capture_scope`.
- Request: `{ "space_id", "candidate_id", "expected_revision": int, "reason": string (optional), "idempotency_key": string }`.
- Result: `{ "candidate_id", "outcome": "dismissed" }`.
- Atomic per MEM-R009, including the §6.7 consumption scrub. Dismissal is replayable per §7.0.5.
- Errors: `CANDIDATE_CONSUMED`, `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`.

### 7.15 `memory.candidates.review`

Formal review decision; the only path from candidate to retained records. **Grant:** `review` plus `records_write`, both covering the candidate's `capture_scope`.
- Request: `{ "space_id", "candidate_id", "expected_revision": int, "decision": Decision, "idempotency_key": string }`, where `Decision` is exactly one of:
  - `{ "action": "promote", "kind": string (optional — default from candidate), "content_override": Content (optional) }`
  - `{ "action": "fragment", "fragments": [ { "kind": string, "content": Content } ] (minItems 1) }`
  - `{ "action": "drop", "rationale": string (optional) }`
- **Placement rule:** created records inherit the candidate's `capture_scope` verbatim. A review decision carries no placement fields and cannot cross the admitted capture scope. Any later visibility change is a separate authorized `memory.records.promote` (§7.21).
- Result: `{ "outcome": "promoted" \| "fragmented" \| "dropped", "record_ids": string[] (empty for drop) }`.
- Atomic (MEM-R009): candidate consumption + scrub, all record inserts, receipt — one transaction. Partial success is impossible by construction.
- No merge-into-existing action exists in v1; products compose `records.update` after `promote` if needed (§16.1).
- Errors: `CANDIDATE_CONSUMED`, `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`, `INVALID_INPUT`, `SPACE_FROZEN`, `SUBJECT_FENCED`.

### 7.16 `memory.records.create`

Direct authored write, bypassing capture/review (management surfaces). **Grant:** `records_write` covering the declared placement.
- Request: `{ "space_id", "subject_id", "kind", "content": Content, "scope": Scope, "source_context_ids": string[] (optional claims), "anchors": SourceAnchor[] (optional claims), "idempotency_key": string }`. `captured_by`/`captured_at` are always provider-populated from the authenticated context — never client-supplied.
- Result: `{ "record_id", "revision": int }`. Atomic single insert. Live record via `memory.records.get`.
- Errors: `NOT_AUTHORIZED`, `INVALID_INPUT`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `IDEMPOTENCY_CONFLICT`.

### 7.17 `memory.records.get`

**Grant:** `records_read` or `recall` covering the record's placement (shared records require shared access). Read-only; rechecks current authorization (MEM-R026).
- Request: `{ "space_id", "record_id" }`. Result: `{ "record": RetainedRecord }`. Tombstoned records are returned with their tombstone (audit visibility) only to `admin`; others get `NOT_FOUND`.
- Errors: `NOT_FOUND`, `NOT_AUTHORIZED`.

### 7.18 `memory.records.update`

**Grant:** `records_write` covering the current placement.
- Request: `{ "space_id", "record_id", "expected_revision": int, "content": Content (optional), "kind" (optional), "idempotency_key": string }`. At least one of `content`/`kind` required. Placement changes are NOT an update — use `memory.records.promote`.
- Result: `{ "record_id", "revision": int }`.
- Same-transaction reflection invalidation per MEM-R013.
- Errors: `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`, `INVALID_INPUT`, `SUBJECT_FENCED`, `SPACE_FROZEN`.

### 7.19 `memory.records.delete`

**Grant:** `records_write` covering the placement.
- Request: `{ "space_id", "record_id", "expected_revision": int, "reason": string (optional), "idempotency_key": string }`.
- Result: `{ "record_id", "tombstoned": true }`.
- Semantics (MEM-R014, MEM-R022): content removed, tombstone retained, receipts unaffected (they are content-free), reflections referencing the record excluded from list/export (§6.9), remaining reflections invalidated per MEM-R013. Privacy-erasure deployments MAY additionally scrub tombstone provenance claims by product policy; the receipt trail itself is never rewritten.
- Errors: `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`, `SPACE_FROZEN`.

### 7.20 `memory.records.list`

**Grant:** `records_read` or `recall` with coverage of the selection (and filter coverage check per §6.5). Read-only.
- Request: `{ "space_id", "subject_id", "selection": ReadSelection, "filter": ProvenanceFilter (optional), "kinds": string[] (optional), "include_tombstoned": bool (optional, default false, admin only), "cursor" (optional), "limit" (optional) }`.
- Result: `{ "records": RetainedRecord[], "total_authorized": int, "next_cursor" (optional) }`. Authorization filters before pagination; `total_authorized` counts only records the principal may see (MEM-R006).

### 7.21 `memory.records.promote`

Explicit placement change of a retained record (context ↔ shared, or context → other context). **Grant:** `promote` plus `records_write` covering **both** source and target placements (target `shared` requires shared access).
- Request: `{ "space_id", "record_id", "expected_revision": int, "target_scope": Scope, "idempotency_key": string }`.
- Result: `{ "record_id", "revision": int, "scope": Scope }`.
- Promotion is always explicit; no capture, review, or recall path ever changes placement implicitly. Provenance is unchanged. Promotion moves the record into/out of reflection scopes and therefore invalidates per MEM-R013.
- Errors: `REVISION_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`, `INVALID_INPUT`, `SPACE_FROZEN`, `SUBJECT_FENCED`.

### 7.22 `memory.recall`

Authorized, ranked, budgeted read for execution-time context. **Grant:** `recall` covering the selection (§6.5). Read-only (MEM-R011).
- Request: `{ "space_id", "subject_id", "selection": ReadSelection, "filter": ProvenanceFilter (optional), "query": string (optional — ranking hint), "budget": { "max_items": int (required, ≥1, ≤ max_page_size), "max_bytes": int (optional, hard UTF-8 budget, §9) }, "cursor": string (optional) }`.
- Result: `{ "records": RetainedRecord[], "total_authorized": int, "truncated": bool, "overflow_record_id": string (optional, §9), "next_cursor": string (optional), "engine_policy_version": string }`.
- Semantics: authorization filter first (MEM-R006); then provider ranking (algorithm-variable, §9 item 2); then budget. `truncated: true` whenever budget cut the authorized set. Candidates never appear (MEM-R004); tombstoned records never appear. Ordering MUST be stable for identical provider state, request, and `engine_policy_version`; it MAY differ across providers or policy versions.
- Errors: `NOT_AUTHORIZED`, `INVALID_INPUT`, `NOT_FOUND`, `CURSOR_INVALID`.

### 7.23 `memory.profile.get`

**Grant:** any capability on the subject with shared access or unrestricted coverage (profiles are subject-level trusted documents, §6.5). Read-only.
- Request: `{ "space_id", "subject_id", "profile_kinds": string[] (optional — omitted = all present kinds) }`.
- Result: `{ "profiles": ProfileDocument[] }`. Absent kinds are honest-empty, never an error (§4.2).

### 7.24 `memory.profile.update`

**Grant:** `profile_write`.
- Request: `{ "space_id", "subject_id", "profile_kind", "expected_revision": int \| null (null = create-only), "document": Document, "idempotency_key": string }`.
- Result: `{ "profile_kind", "revision": int }`.
- Same-transaction reflection invalidation per MEM-R013.
- Errors: `REVISION_CONFLICT`, `NOT_AUTHORIZED`, `INVALID_INPUT`, `SPACE_FROZEN`, `SUBJECT_FENCED`.

### 7.25 `memory.reflections.state`

Capability `spoke-memory-reflection`. **Grant:** `reflect` or `admin` covering the selection/filter. Read-only.
- Request: `{ "space_id", "subject_id", "selection": ReadSelection, "filter": ProvenanceFilter (optional) }`.
- Result: `{ "state_digest": string (64 hex), "basis": { "record_count": int, "profile_kinds": string[] }, "stale": bool, "last_reflection_id": string (optional), "missing_required_profile_kinds": string[], "data_sufficiency": "sufficient" \| "insufficient" }`.
- `state_digest` commits to: selection, filter, the authorized source record ID+revision set, profile kinds+revisions, and `engine_policy_version`. It is the CAS token for generation. `data_sufficiency` surfaces engine/provider policy (minimum-material heuristics); the protocol defines no numeric thresholds.

### 7.26 `memory.reflections.list`

Capability `spoke-memory-reflection`. **Grant:** `reflect` or `admin` covering each returned reflection's pinned selection/filter (context-restricted grants never read all-subject derived text). Read-only.
- Request: `{ "space_id", "subject_id", "selection": ReadSelection (optional — restricts listing), "filter": ProvenanceFilter (optional), "cursor" (optional), "limit" (optional) }`.
- Result: `{ "reflections": Reflection[], "next_cursor" (optional) }` newest-first. `stale` values are computed at read time (§6.9). Reflections referencing tombstoned records are excluded (§6.9).

### 7.27 `memory.reflections.generate`

Capability `spoke-memory-reflection`. **Grant:** `reflect` covering the selection/filter. Explicit, cost-bearing generation (MEM-R012).
- Request: `{ "space_id", "subject_id", "selection": ReadSelection, "filter": ProvenanceFilter (optional), "expected_state_digest": string, "idempotency_key": string }`.
- Admission failures (fail-closed): `missing_required_profile_kinds` non-empty → `PROFILE_INCOMPLETE` with `details.missing` (MEM-R005); `data_sufficiency: "insufficient"` → `INSUFFICIENT_DATA` (provider/engine policy gate, §7.25); `expected_state_digest` ≠ current digest → `REVISION_CONFLICT`.
- Result: pending `MemoryReceipt`; on completion `result = { "reflection_id": string, "cost": Cost (optional) }` (content-free).
- **Concurrency rule (publish-pinned):** generation completes against the pinned basis from admission. If the basis moved meanwhile, the reflection is still published with its pinned basis and immediately reads `stale: true` (MEM-R013) — never a false-current result, never a silently dropped paid generation.
- Errors: `PROFILE_INCOMPLETE`, `INSUFFICIENT_DATA`, `REVISION_CONFLICT`, `NOT_AUTHORIZED`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `OUTCOME_UNCERTAIN`.

### 7.28 `memory.receipts.get`

Read-only (MEM-R011). **Grant:** current authorization as the receipt's `requester_principal_id`, or `admin` in the space (MEM-R018/R021).
- Request: `{ "space_id", "receipt_id" }`. Result: `{ "receipt": MemoryReceipt }`.
- Sole uncertainty-resolution channel (MEM-R010, §14.3) and polling mechanism for pending receipts.
- Errors: `NOT_FOUND` (unknown), `RECEIPT_EXPIRED` (evicted body; `details.last_known_status` from the dedup tombstone), `NOT_AUTHORIZED`.

### 7.29 `memory.exports.create`

Capability `spoke-memory-transfer`. **Grant:** `transfer` with coverage of the requested export scope: whole-space export requires space-wide coverage; subject export requires coverage of the optional `selection` (shared access where applicable).
- Request: `{ "space_id", "subject_id" (optional — omitted = whole space), "selection": ReadSelection (optional, subject exports only), "include_reflections": bool (optional, default false), "include_tombstoned": bool (optional, default false, admin only), "require_quiescent": bool (optional, default false), "idempotency_key": string }`.
- Quiescence: with `require_quiescent: true`, the op fails `INVALID_INPUT` (`details.reason: "space_not_drained"`) unless the space is frozen AND `pending_receipts == 0` (drain barrier, MEM-R023) — the provider's snapshot-consistency guarantee for migration-grade exports. With `false`, the snapshot is provider-consistent under the single-authority property but may race ongoing writes; for backups only.
- Result: pending receipt; completion `result = { "export_id": string, "chunk_count": int, "total_bytes": int, "bundle_digest": string, "counts": { "subjects": int, "records": int, "candidates": int, "profiles": int, "reflections": int, "receipt_summaries": int, "grants": int } }` (all content-free).
- Errors: `NOT_AUTHORIZED`, `NOT_FOUND`, `INVALID_INPUT`, `IDEMPOTENCY_CONFLICT`. Export is on the freeze allowlist — `SPACE_FROZEN` never applies to it.

### 7.30 `memory.exports.get`

Capability `spoke-memory-transfer`. **Grant:** `transfer` (same coverage as creation). Read-only.
- Request: `{ "space_id", "export_id", "chunk_ordinal": int ≥ 0 }`. Result: `{ "chunk": MemoryExportChunk }` (§6.13).
- Callers fetch chunks 0..chunk_count-1 and verify `chunk_digest` per chunk and `bundle_digest` on reassembly.
- Errors: `NOT_FOUND`, `EXPORT_EXPIRED` (past `export_retention_seconds`), `INVALID_INPUT` (ordinal out of range), `NOT_AUTHORIZED`.

### 7.31 `memory.imports.stage`

Capability `spoke-memory-transfer`. **Grant:** `transfer` + `admin` on the target space.
- Request (per chunk, strict ordinal order): `{ "space_id", "chunk": MemoryExportChunk, "conflict_policy": "fail" \| "rename_subject" \| "merge_existing" (required iff chunk_ordinal = 0, forbidden otherwise), "adopt_grants": [ { "grant_id": string, "principal_id": string (target-space principal) } ] (optional, chunk_ordinal = 0 only), "idempotency_key": string }`. The idempotency namespace makes per-chunk replay safe (§7.0.5).
- Chunk accumulation is durable (survives provider crash). Out-of-order chunks fail `INVALID_INPUT`. Verification runs when the final chunk arrives (all failures reject `IMPORT_CONFLICT` with `details.errors[]`, nothing staged): per-chunk `chunk_digest`, reassembled `bundle_digest`/`digest`, `bundle_format_version` support, `memory_protocol_version` compatibility (equal major = 1), schema validation of every element, secret-shape scan of `grants` summaries.
- Subject collision: same `subject_id` already exists in target →
  - `fail` rejects the import (nothing staged);
  - `rename_subject` assigns a fresh `subject_id` and records the mapping (MEM-R002: no auto-link);
  - `merge_existing` updates the existing target subject in place: `kind`, `status`, `fenced_contexts`, and `extensions` take the staged values, `created_at` and `schema_version` stay the target's, `revision` increments, and `subject_id_map[source] = source` (identity). Records, candidates, and reflections are never deleted or overwritten by this rule — each transferred element still gets a fresh target revision and its own map entry.
- `archived` is terminal (§8.3), so a collision involving an `archived` subject on either side always resolves as `rename_subject`, never as a merge.
- Grant adoption: only grants listed in `adopt_grants` are created on the target, bound to the named target principal, and only if the staging principal's own coverage subsumes them (non-amplification, §6.5). Unlisted grants are dropped. No grant is ever auto-activated (MEM-R025).
- Reference translation (computed at staging, applied at activation): `subject_id_map`, `record_id_map`, `candidate_id_map`; candidate `consumed_by.record_ids`, reflection basis record IDs, and receipt-summary references are remapped through them; imported reflections are marked `stale: true` unconditionally (revalidation required, MEM-R025); provenance and `import_ref` are adopted verbatim as historical attribution (§6.6); target revisions are fresh (source revisions preserved only in maps).
- Result: pending receipt per chunk (`result = { "import_id": string, "chunks_received": int }`); final chunk completion adds `"staged_report": { "counts": Counts (subjects, records, candidates, profiles, reflections, receipt_summaries, grants), "subject_id_map": {source: target}, "record_id_map": {source: target}, "candidate_id_map": {source: target}, "grants_adopted": int, "grants_dropped": int, "verification": { "bundle_digest_ok": bool, "errors": string[] } }`.
- Staged data is invisible to all non-transfer ops. Staged imports expire after `staged_import_ttl_seconds`.

### 7.32 `memory.imports.activate`

Capability `spoke-memory-transfer`. **Grant:** `transfer` + `admin`.
- Request: `{ "space_id", "import_id", "idempotency_key": string }`.
- Result: pending receipt; completion `result = { "import_id", "activated": Counts, "authority_generation": int }`.
- Atomic per space: all staged subjects/records/candidates/profiles/reflections/receipt-summaries become visible (or recorded) with freshly assigned target revisions in one transaction, or none do. Activation increments the space `authority_generation` (§12.5). Replay per §7.0.5. Expired or discarded imports fail `IMPORT_CONFLICT`.
- Recommended practice (non-normative): activate into a frozen, drained target space, verify, then unfreeze — the sequence of §12.

### 7.33 `memory.imports.discard`

Capability `spoke-memory-transfer`. **Grant:** `transfer` + `admin`.
- Request: `{ "space_id", "import_id", "idempotency_key": string }`. Result: `{ "import_id", "discarded": true }`.
- Explicit pre-activation rollback: removes all staged data. Valid only before activation; after activation it fails `IMPORT_CONFLICT` — post-activation rollback is §12.4, never compensating-only deletion.
- Errors: `IMPORT_CONFLICT`, `NOT_FOUND`, `NOT_AUTHORIZED`.

---

## 8. Lifecycle and state transitions

### 8.1 Candidate FSM

`pending` → `consumed` via exactly one of `memory.candidates.review` (outcomes `promoted`/`fragmented`/`dropped`) or `memory.candidates.dismiss` (outcome `dismissed`). `consumed` is terminal; further mutation fails `CANDIDATE_CONSUMED`. Consumption scrubs content and sets `content_digest` (§6.7). Expiry removes a `pending` candidate from lists without consuming it.

### 8.2 Space FSM and freeze maintenance allowlist

`active ⇄ frozen` via `memory.spaces.freeze` / `memory.spaces.unfreeze`. While `frozen`, the space admits **exactly**: all read-only ops (MEM-R011 list), `memory.spaces.unfreeze`, `memory.grants.revoke`, and the transfer ops `memory.exports.create`, `memory.exports.get`, `memory.imports.stage`, `memory.imports.activate`, `memory.imports.discard`. All other mutations fail `SPACE_FROZEN`. Already-admitted ops complete (MEM-R018); the drain barrier is `pending_receipts == 0` observable via `memory.spaces.list` (MEM-R023). Freeze is durable across provider restart.

### 8.3 Subject and context fence FSM

Subject target: `active ⇄ fenced`, `{active,fenced} → archived` (terminal decommission; reversible product archive maps to `fence`, §7.7). Context target: membership in `fenced_contexts` toggles per context (MEM-R024). All transitions bump the subject `revision` and are provider-enforced at admission across all clients.

### 8.4 Import FSM

`staging (chunks 0..n)` → `staged` → `activated` (terminal, visible) or `staged`/`staging` → `discarded` (terminal, removed) or → `expired` (TTL, removed). All transitions receipt-bound.

### 8.5 Receipt FSM

`pending → committed | failed`. Ops that complete synchronously commit directly as `committed`. Only `memory.reflections.generate`, `memory.exports.create`, final-chunk `memory.imports.stage`, and `memory.imports.activate` may return `pending`. Bodies retire to dedup tombstones per §6.10.

---

## 9. Recall, ranking, truncation, budget

1. **Filter (normative):** authorization + selection + provenance filter + tombstone/candidate exclusion (MEM-R004/R006).
2. **Rank (non-normative):** provider algorithm; `query` is a hint, never a filter. Identical-algorithm behavior across providers is explicitly NOT mandated.
3. **Budget (normative):** `max_items` is a hard item cap. `max_bytes` is a **hard** budget measured as the UTF-8 byte length of the serialized `content` fields of returned records. Progress rule: if the next ranked item would exceed `max_bytes` and the page is empty, the provider returns that single item whole and sets `overflow_record_id` to its ID — an oversized first item is surfaced, never silently dropped or silently skipped; if the page is non-empty, the item is deferred and `truncated: true`. `truncated` MUST be true iff the authorized filtered set exceeds the returned page under the applied budget.
4. **Pagination:** `next_cursor` per §7.0.7 (MEM-R026): bound, snapshot-consistent, invalidated by state/grant change with `CURSOR_INVALID`; no hidden-row leaks.

## 10. Trusted profiles vs untrusted captured text

- `ProfileDocument.document.body` is trusted author-maintained content; it may drive execution conditioning per product policy.
- `RetainedRecord.content` and `MemoryCandidate.proposed.content` are untrusted data: conforming providers and consumers MUST treat them as data, never as instructions, and MUST NOT let record content alter grant, scope, or principal interpretation (MEM-R003).
- Provenance `source_context_ids`/`anchors` on capture/create are recorded **claims**: stored verbatim, attributed to the provider-populated `captured_by`, and never validated as authorization. Imported provenance is adopted verbatim as historical attribution linked through `import_ref` (§6.6); adoption never re-stamps `captured_by`.
- Reflection bodies are derived data trusted only as to basis/generator metadata.

## 11. Concurrent clients, revoke, freeze, delete races

- Single authority (MEM-R001) serializes conflicting writes; CAS (MEM-R007) detects the rest. No client-side locking assumption exists.
- **Revoke vs in-flight:** admission-time evaluation (MEM-R018). Receipt disclosure rechecks current authorization (§7.0.5).
- **Freeze/fence vs write:** fence commit and admission-gate update are one transaction; a write either passes admission before the fence commit (and completes) or is rejected after — never half-applied. Applies to space freeze and context fence alike (MEM-R017/R024).
- **Delete vs recall:** recall sees either pre- or post-delete state per record; never partial content. Delete vs reflections: tombstone-referencing reflections drop out of list/export (§6.9); others read stale per MEM-R013.
- **Delete vs receipts:** receipts are content-free (MEM-R022), so erasure and immutable audit coexist.
- **Grant narrowing vs existing records:** narrowing `context_ids`/`shared_access` does not rewrite records; it filters future reads/writes and invalidates outstanding cursors (MEM-R026).
- **Generation vs source change:** publish-pinned (§7.27).
- **Archive vs export:** export includes archived subjects; recall excludes them.

## 12. Provider switching, export/import, recovery

### 12.1 Cutover sequence (normative steps, operator-timed)

1. `memory.spaces.freeze` on the source space.
2. Drain barrier: poll `memory.spaces.list` until `pending_receipts == 0` (MEM-R023).
3. `memory.exports.create` with `require_quiescent: true`; fetch all chunks via `memory.exports.get`; verify chunk and bundle digests.
4. `memory.imports.stage` on the target (recommended into a frozen, drained target space), chunk by chunk, with explicit `adopt_grants`.
5. Verify the staged report (counts, all ID maps, `verification.errors` empty).
6. `memory.imports.activate` (bumps target `authority_generation`).
7. Operator re-points every participating client binding, pinned to the new `(authority_host_id, authority_generation)`; unfreeze target.
8. Keep the source frozen until the rollback window closes; then decommission per product policy.

### 12.2 Crash recovery

Freeze/fence state, staged import chunks, receipts, and dedup tombstones are durable. After a crash: pending receipts resolve via `memory.receipts.get` or key replay (MEM-R010/R021); half-activated imports are impossible (atomic activation, §7.32); staging resumes at the first unacknowledged chunk ordinal; a staged import resumes to activation or discard.

### 12.3 Identity and reference preservation/translation

Subject IDs preserved by default (`rename_subject` on collision for a forward cutover; `merge_existing` for the reverse-transfer rollback of §12.4). Record/candidate IDs and all revisions are re-assigned by the target; source values survive in the staged report maps. Candidate consumption outcomes, reflection basis references, profile/subject/fence revisions, and provenance translate through those maps (§7.31); imported reflections enter `stale: true` (MEM-R025). Receipt bodies and live idempotency state do NOT transfer as active state — `receipt_summaries` carry content-free dedup provenance so the target can recognize reimported/replayed capture digests; clients use fresh idempotency keys against the new provider. Grants transfer only via explicit `adopt_grants` (MEM-R025).

### 12.4 Rollback

- **Before the target accepts any writes** (target still frozen): `memory.imports.discard` (or operator abandonment of the frozen target space), then controlled binding rollback — clients re-pin to the source and the operator unfreezes the source with a bumped `authority_generation`.
- **After the target has accepted writes:** freeze and drain the target; perform a verified **reverse transfer** — `memory.exports.create(require_quiescent: true)` on the target, stage+activate into the still-frozen source with `conflict_policy: merge_existing` (delta reconciliation through the same staging verification), and only then reopen the source. `merge_existing` is what makes rollback of a space whose subjects pre-date the export possible: the source's own subjects are *updated in place* (§7.31) rather than forked into renamed duplicates. No allowlist change is required — `memory.imports.stage` and `memory.imports.activate` are already admitted on a frozen space (§8.2) — and no other op becomes admissible. Until reverse reconciliation verifies, the source remains frozen and clients observe `SPACE_FROZEN` (explicit blocked state, MEM-R027). Default behavior never deletes target-written history and never opens both authorities.

### 12.5 Cutover coordination

Cutover is an operator-coordinated procedure, not a local config edit: all participating clients re-pin bindings to the new `(authority_host_id, authority_generation)`. The old provider remains frozen and rejects writes after cutover; stale clients fail with `SPACE_FROZEN` against it — nothing auto-unfreezes a decommissioned source except an explicit operator re-open, which bumps `authority_generation` so stale pins are detectable (MEM-R027).

## 13. Wire/transport mapping

- Memory ops ride the existing Connect envelope: `ConnectInvokeRequest.op` carries the exact op string (e.g. `memory.capture`), `payload` = the op's request object (same wrapping rule as the core `extract` op, verified in `schemas/connect/connect-invoke-request.schema.json`). Chunked export/import keeps every invoke payload within `export_chunk_bytes` (§6.13).
- Dispatch gate: `required_capability` gains rows mapping every `memory.*` op to `spoke-memory` (or the sub-flag per §4.1). Unknown ops keep failing closed with `op_unsupported`; unnegotiated memory ops fail `capability_missing` before any handler runs (existing rule shape, `crates/spoke-connect/src/core/dispatch.rs:52-58`).
- Serving seam: `RemoteMemoryService` (mirrors the `RemoteExtractService` precedent in `crates/spoke-connect/src/remote/serve_ports.rs`) — the serving host owns the provider; requesters need only the negotiated flag.
- Client seam: remote adapters expose `MemoryPort` over the session, mapping each method to its `memory.*` op (same `invoke_mapped` pattern as the `port.*` catalogue in `crates/spoke-connect/src/remote/remote_adapter.rs:1412-1539`).
- **Fallback prohibition (MEM-R019):** absent the negotiated capability, there is no emulation path through baseline objects — including `KnowledgeEntry`, `modules.*`, `Scope.viewpoint`, or `port.*`. `Scope.viewpoint`/`ke-ownership` semantics are never consulted for memory ops (MEM-R020).
- In-process embedding uses `MemoryPort` directly with a product-supplied admission context producing the authenticated `Principal`.

## 14. Errors and unsupported handling

### 14.1 Code vocabulary (wire strings)

Reused existing ops-layer codes (verified `crates/spoke-operations/src/result.rs:8-35`): `INVALID_INPUT`, `INVALID_STATUS_TRANSITION`, `REVISION_CONFLICT`, `INTERNAL_ERROR`. Reused dispatch-layer codes (snake_case, Connect boundary): `op_unsupported`, `capability_missing`. New memory codes (SCREAMING_SNAKE): `NOT_FOUND`, `NOT_AUTHORIZED`, `GRANT_REVOKED`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `CANDIDATE_CONSUMED`, `PROFILE_INCOMPLETE`, `INSUFFICIENT_DATA`, `IDEMPOTENCY_CONFLICT`, `OUTCOME_UNCERTAIN`, `EXPORT_EXPIRED`, `IMPORT_CONFLICT`, `RECEIPT_EXPIRED`, `CURSOR_INVALID`.

`GRANT_REVOKED` vs `NOT_AUTHORIZED`: `GRANT_REVOKED` when the principal presents a specific grant that exists but is revoked/expired; `NOT_AUTHORIZED` for absence of coverage (and is also the correct code when the provider declines to reveal existence).

### 14.2 Retry classes

Every error envelope carries `details.retry_class`:

| Class | Meaning | Client action |
|---|---|---|
| `retryable` | No effect committed; safe to retry (same key if mutating). | Retry with backoff. |
| `terminal` | No effect; retry unchanged will fail again. | Fix request/authorization. |
| `uncertain` | Commit state unknown (timeout/transport failure after admission). | Poll `memory.receipts.get` or replay with the same `idempotency_key` (MEM-R010). |

Mapping: `INVALID_INPUT`, `NOT_AUTHORIZED`, `GRANT_REVOKED`, `CANDIDATE_CONSUMED`, `PROFILE_INCOMPLETE`, `INSUFFICIENT_DATA`, `IDEMPOTENCY_CONFLICT`, `EXPORT_EXPIRED`, `IMPORT_CONFLICT`, `RECEIPT_EXPIRED`, `CURSOR_INVALID`, `REVISION_CONFLICT`, `INVALID_STATUS_TRANSITION`, `SPACE_FROZEN`, `SUBJECT_FENCED`, `NOT_FOUND` → `terminal`. `OUTCOME_UNCERTAIN` → `uncertain`. `op_unsupported`/`capability_missing` → `terminal`.

`INTERNAL_ERROR` is `retryable` **only when the provider asserts no effect committed** (storage/runtime failure before commit). If effects may have committed or cost was charged, the provider MUST NOT claim clean no-effect failure: it returns `OUTCOME_UNCERTAIN` (or completes the receipt as `failed` with content-free cost facts, §14.3).

### 14.3 Uncertain outcomes and paid generation

A provider that cannot determine whether a mutating op committed returns `OUTCOME_UNCERTAIN` (or the client observes a transport timeout). Resolution is always: same-key replay (returns the original receipt, MEM-R008/R021) or `memory.receipts.get`. There is no other recovery channel.

For async pending receipts (§7.0.3): the pending receipt is durable; after a provider crash the work is resumed or the receipt is completed as `failed`. Duplicate paid generation is impossible because replay returns the same pending/committed receipt (MEM-R008). A `failed` generation receipt MAY record content-free cost facts (`cost.units`/`unit_kind`) actually charged — cost facts never include input text.

## 15. Complete JSON examples

All examples are complete, valid JSON documents conforming to this proposal. They are illustrative vectors, not execution receipts from a running provider. Every `payload_digest`/`digest`/`content_digest` value below was computed as SHA-256 over the RFC 8785 JCS canonicalization (§6.1) of its matching object — not a decorative constant.

### 15.1 `RetainedRecord`

```json
{
  "schema_version": 1,
  "record_id": "rec_01J8ZQ3K4M0EXAMPLE0000001",
  "space_id": "spc_local_default",
  "subject_id": "nexus:character:mira",
  "kind": "event",
  "content": {
    "text": "Mira promised the harbor master she would return before dawn.",
    "structured": { "certainty": "stated" }
  },
  "scope": { "visibility": "context", "context_id": "bnd_mira_harbor_run_7" },
  "provenance": {
    "source_context_ids": ["bnd_mira_harbor_run_7"],
    "anchors": [
      {
        "schema_version": 1,
        "source_id": "run://nexus/local/run_9182",
        "span": { "start": 1204, "end": 1271 },
        "label": "harbor promise",
        "mime_type": "text/plain",
        "extensions": {}
      }
    ],
    "captured_by": "prn_nexus_host_local",
    "captured_at": "2026-10-28T09:15:00Z"
  },
  "created_at": "2026-10-28T09:15:01Z",
  "updated_at": "2026-10-28T09:15:01Z",
  "revision": 1,
  "extensions": {}
}
```

### 15.2 `memory.capture` request

`capture_scope` is the admitted immutable placement; `source_context_ids`/`anchors` are provenance claims.

```json
{
  "space_id": "spc_local_default",
  "subject_id": "nexus:character:mira",
  "content": {
    "text": "Mira told Bo the gate key is hidden under the third barrel."
  },
  "capture_scope": { "visibility": "context", "context_id": "bnd_mira_harbor_run_7" },
  "source_context_ids": ["bnd_mira_harbor_run_7"],
  "anchors": [
    {
      "schema_version": 1,
      "source_id": "run://nexus/local/run_9182",
      "span": { "start": 1400, "end": 1463 },
      "extensions": {}
    }
  ],
  "kind_hint": "fact",
  "occurred_at": "2026-10-28T09:20:00Z",
  "idempotency_key": "cap-9182-1400-mira"
}
```

### 15.3 `memory.capture` success receipt

`payload_digest` below is the real digest of the §15.2 request under §6.1.

```json
{
  "schema_version": 1,
  "receipt_id": "rcpt_01J8ZQ9W2N0EXAMPLE0000099",
  "op": "memory.capture",
  "space_id": "spc_local_default",
  "requester_principal_id": "prn_nexus_host_local",
  "idempotency_key": "cap-9182-1400-mira",
  "payload_digest": "034a5c6d74a6c8fa3fa25d0e41dd2b10d1a110464c5e9e3f56e7c1a22cf48696",
  "status": "committed",
  "result": { "candidate_ids": ["cand_01J8ZQ9X7P0EXAMPLE0000101"], "classified": 1 },
  "created_at": "2026-10-28T09:20:01Z",
  "completed_at": "2026-10-28T09:20:01Z",
  "extensions": {}
}
```

### 15.4 `memory.candidates.review` request (fragment)

Fragments inherit the candidate's `capture_scope` (here `bnd_mira_harbor_run_7`); no placement fields appear in a decision. Sharing any fragment afterwards is a separate authorized `memory.records.promote`.

```json
{
  "space_id": "spc_local_default",
  "candidate_id": "cand_01J8ZQ9X7P0EXAMPLE0000101",
  "expected_revision": 1,
  "decision": {
    "action": "fragment",
    "fragments": [
      {
        "kind": "fact",
        "content": { "text": "The gate key is hidden under the third barrel." }
      },
      {
        "kind": "event",
        "content": { "text": "Mira revealed the key location to Bo." }
      }
    ]
  },
  "idempotency_key": "rev-cand-01J8ZQ9X7P-1"
}
```

### 15.5 `memory.recall` request / response

```json
{
  "space_id": "spc_local_default",
  "subject_id": "nexus:character:mira",
  "selection": {
    "mode": "selected_contexts",
    "context_ids": ["bnd_mira_harbor_run_7"],
    "include_shared": true
  },
  "query": "Where is the gate key?",
  "budget": { "max_items": 8, "max_bytes": 16384 }
}
```

```json
{
  "records": [
    {
      "schema_version": 1,
      "record_id": "rec_01J8ZQ3K4M0EXAMPLE0000001",
      "space_id": "spc_local_default",
      "subject_id": "nexus:character:mira",
      "kind": "event",
      "content": {
        "text": "Mira promised the harbor master she would return before dawn.",
        "structured": { "certainty": "stated" }
      },
      "scope": { "visibility": "context", "context_id": "bnd_mira_harbor_run_7" },
      "provenance": {
        "source_context_ids": ["bnd_mira_harbor_run_7"],
        "anchors": [],
        "captured_by": "prn_nexus_host_local",
        "captured_at": "2026-10-28T09:15:00Z"
      },
      "created_at": "2026-10-28T09:15:01Z",
      "updated_at": "2026-10-28T09:15:01Z",
      "revision": 1,
      "extensions": {}
    }
  ],
  "total_authorized": 1,
  "truncated": false,
  "engine_policy_version": "spoke-memory/1.0.0-deterministic"
}
```

### 15.6 `MemoryGrant`

A context-restricted grant with explicit shared access (never `subject_all`):

```json
{
  "schema_version": 1,
  "grant_id": "grt_01J8ZR0A1Q0EXAMPLE0000201",
  "space_id": "spc_local_default",
  "subject_id": "nexus:character:mira",
  "principal_id": "prn_nexus_cli_operator",
  "capabilities": ["review", "records_write", "recall"],
  "context_ids": ["bnd_mira_harbor_run_7"],
  "shared_access": true,
  "created_at": "2026-10-28T09:10:00Z",
  "revision": 1,
  "extensions": {}
}
```

### 15.7 `memory.describe` response

```json
{
  "memory_protocol_version": 1,
  "capabilities": ["spoke-memory", "spoke-memory-reflection", "spoke-memory-transfer"],
  "limits": {
    "max_content_bytes": 65536,
    "max_candidates_per_capture": 16,
    "max_page_size": 100,
    "export_chunk_bytes": 262144,
    "idempotency_retention_seconds": 86400,
    "receipt_retention_seconds": 604800,
    "dedup_retention_seconds": 2592000,
    "staged_import_ttl_seconds": 259200,
    "export_retention_seconds": 86400
  },
  "required_profile_kinds": ["personality"],
  "engine_policy_version": "spoke-memory/1.0.0-deterministic"
}
```

### 15.8 `MemoryExportBundle` (logical, minimal subject export)

The `digest` below is the real digest of this bundle minus the `digest` field. Wire transfer happens as `MemoryExportChunk`s (§6.13); this document shows the logical whole.

```json
{
  "schema_version": 1,
  "bundle_format_version": 1,
  "memory_protocol_version": 1,
  "export_id": "exp_01J8ZR5B2R0EXAMPLE0000301",
  "space_id": "spc_local_default",
  "created_at": "2026-10-28T10:00:00Z",
  "source": {
    "authority_host_id": "host_nexus_local",
    "authority_generation": 4,
    "engine_policy_version": "spoke-memory/1.0.0-deterministic"
  },
  "subjects": [
    {
      "schema_version": 1,
      "space_id": "spc_local_default",
      "subject_id": "nexus:character:mira",
      "kind": { "ns": "nexus", "id": "character" },
      "status": "active",
      "fenced_contexts": [],
      "created_at": "2026-10-27T08:00:00Z",
      "revision": 3,
      "extensions": {}
    }
  ],
  "records": [],
  "candidates": [
    {
      "schema_version": 1,
      "candidate_id": "cand_01J8ZQ9X7P0EXAMPLE0000101",
      "space_id": "spc_local_default",
      "subject_id": "nexus:character:mira",
      "capture_scope": { "visibility": "context", "context_id": "bnd_mira_harbor_run_7" },
      "classification": { "disposition": "promote", "rationale_codes": ["single_fact"] },
      "status": "consumed",
      "consumed_by": {
        "op": "memory.candidates.review",
        "receipt_id": "rcpt_01J8ZR1D4T0EXAMPLE0000501",
        "outcome": "promoted",
        "record_ids": ["rec_01J8ZR2E5U0EXAMPLE0000601"]
      },
      "provenance": {
        "source_context_ids": ["bnd_mira_harbor_run_7"],
        "anchors": [],
        "captured_by": "prn_nexus_host_local",
        "captured_at": "2026-10-28T09:20:00Z"
      },
      "content_digest": "ff087c41e39e57602c1190289863c9dbc107dbbc8ef32b9a6e020bcf78da3bd4",
      "created_at": "2026-10-28T09:20:01Z",
      "revision": 2,
      "extensions": {}
    }
  ],
  "profiles": [
    {
      "schema_version": 1,
      "space_id": "spc_local_default",
      "subject_id": "nexus:character:mira",
      "profile_kind": "personality",
      "document": { "format": "markdown", "body": "# Mira\n\nGuarded, loyal, keeps promises." },
      "updated_by": "prn_nexus_cli_operator",
      "created_at": "2026-10-27T08:05:00Z",
      "updated_at": "2026-10-27T08:05:00Z",
      "revision": 1,
      "extensions": {}
    }
  ],
  "receipt_summaries": [
    {
      "receipt_id": "rcpt_01J8ZQ9W2N0EXAMPLE0000099",
      "op": "memory.capture",
      "requester_principal_ref": "prn_nexus_host_local",
      "idempotency_key": "cap-9182-1400-mira",
      "payload_digest": "034a5c6d74a6c8fa3fa25d0e41dd2b10d1a110464c5e9e3f56e7c1a22cf48696",
      "status": "committed",
      "created_at": "2026-10-28T09:20:01Z"
    }
  ],
  "grants": [
    {
      "grant_id": "grt_01J8ZR0A1Q0EXAMPLE0000201",
      "subject_id": "nexus:character:mira",
      "principal_ref": "prn_nexus_cli_operator",
      "capabilities": ["review", "records_write", "recall"],
      "context_ids": ["bnd_mira_harbor_run_7"],
      "shared_access": true,
      "revision": 1
    }
  ],
  "digest": "0d0e8c9f5398b0bc5c5a93be941bcfe5484b8d87b992f22d15ca5623094e0bcb",
  "extensions": {}
}
```

### 15.9 `memory.exports.create` request and pending receipt

```json
{
  "space_id": "spc_local_default",
  "subject_id": "nexus:character:mira",
  "include_reflections": false,
  "require_quiescent": true,
  "idempotency_key": "exp-2026-10-28T10:00Z"
}
```

```json
{
  "schema_version": 1,
  "receipt_id": "rcpt_01J8ZR6C3S0EXAMPLE0000401",
  "op": "memory.exports.create",
  "space_id": "spc_local_default",
  "requester_principal_id": "prn_nexus_cli_operator",
  "idempotency_key": "exp-2026-10-28T10:00Z",
  "payload_digest": "58c03d12c3e307e4483a49f144efb87a81f4c1b16724ac8fefe8b1c173e3e136",
  "status": "pending",
  "created_at": "2026-10-28T10:00:00Z",
  "extensions": {}
}
```

### 15.10 Error envelope (`uncertain` class)

```json
{
  "code": "OUTCOME_UNCERTAIN",
  "message": "Capture admission passed but commit outcome is unknown; poll memory.receipts.get or replay with the same idempotency_key.",
  "details": { "retry_class": "uncertain", "idempotency_key": "cap-9182-1400-mira" },
  "extensions": {}
}
```

## 16. Alternatives considered and ablation

### 16.1 Rejected alternatives

- **Reuse `KnowledgeEntry` + `modules.memory`:** rejected. KnowledgeEntry is a narrative KB envelope (canonical_name required, `ke-ownership` disclosure semantics); memory needs candidates, grants, fencing, and receipts that do not exist there. The product-local boundary note is amended instead (§18.2).
- **Expand `BaselinePorts`:** rejected; memory is optional and independent (mirrors `ExtractionPort` standing outside `BaselinePorts`/`FullPorts`).
- **Public storage SPI behind the engine:** rejected; the replacement boundary is `MemoryPort` only. Provider-private storage and transactions stay provider implementation detail; the reference engine is pure algorithm policy.
- **CRDT / offline multi-master / federated recall:** rejected by MEM-R001; conflicts with provider-enforced fencing and atomic review.
- **Webhooks / streaming / callback async:** rejected; one async model (durable receipt + poll, §7.0.3) covers all long-running ops.
- **Inheriting `Scope.viewpoint` authorization:** rejected (MEM-R020); it is a KB disclosure carrier, not a memory grant system.
- **Merge-into-existing review action:** omitted from v1 (current Nexus precedent declares it unimplemented; composable from promote + update). Ablation: no confirmed v1 requirement fails.
- **Unchunked single-message export:** rejected; Connect invoke payloads are bounded and memories must never silently truncate (§6.13).

### 16.2 Ablation evidence for retained components

- `MemoryGrant` (+`shared_access`) — removal would lose provider-enforced cross-client authorization with unambiguous shared/context coverage; kept for MEM-R006/R017/R018.
- `MemoryCandidate` (+`capture_scope`) — removal would lose never-recall-before-review and the admitted-placement fence for review outcomes; kept for MEM-R004/R009.
- `MemoryReceipt` (content-free) + dedup tombstones — removal would lose uncertain-outcome recovery and erasure-safe audit; kept for MEM-R008/R010/R021/R022.
- `ReadSelection` explicit modes + `ProvenanceFilter` — removal would reintroduce null/empty-array ambiguity and conflate placement with provenance; kept for §6.12/§6.13 semantics.
- `memory.subjects.fence` (context target) / `memory.imports.discard` — removal would lose cross-client per-context fencing and explicit pre-activation rollback; kept for MEM-R024 and §12.4.
- `authority_generation` — removal would make stale post-cutover bindings indistinguishable; kept for MEM-R027.
- Tombstones — removal would lose delete-vs-audit reconciliation; kept for MEM-R014.

## 17. Conformance anchors

Doc 04 owns `C-*` scenarios; each MUST cite at least one `MEM-R###` anchor from §5 or the precise §6/§7 contract it exercises (mixed citation styles are acceptable; section-level citations name the exact op or model section). Identical-outcome expectations apply to: admission/authorization results (incl. `shared_access`/`subject_all` coverage), CAS conflicts, idempotent replays and dedup-tombstone behavior, candidate consumption atomicity and placement inheritance, fencing rejections (space/subject/context), freeze allowlist and drain barrier, cursor invalidation, export digest verification, staged-report contents, grant non-amplification. Algorithm-variable expectations apply to: candidate classification dispositions, recall ranking order, reflection body text, `data_sufficiency` thresholds.

## 18. Current state vs proposed (verified evidence)

### 18.1 Verified repository facts (research snapshot — SPOKE @ 0.14.1)

- Author Memory is explicitly **product-local at this pin**: [`CONCEPTS.md` §Creator Memory](../../../../CONCEPTS.md) defines it as the author's own memory pipeline, and no `spoke-*` schema, port family, or `modules.memory` dialect carries it. (The pre-lifecycle-migration spec that also recorded this boundary was retired into the `specs/archived/` tree; the archive carries no current authority.)
- Port architecture: six baseline ports in `crates/spoke-operations/src/adapter/ports.rs` (`BaselinePorts` supertrait), optional standalone `ExtractionPort` precedent (same file, documented "NOT part of BaselinePorts / FullPorts").
- Capability gate: `crates/spoke-connect/src/core/dispatch.rs:13-24` defines `spoke-baseline`, `l2-computable`, `ke-extraction`, `ke-ownership`; `dispatch_allowed` fails closed for unknown ops (lines 52-58). The `port.*` D4 catalogue's requirement map lives in `crates/spoke-connect/src/remote/responder.rs:107-129`; client mapping in `crates/spoke-connect/src/remote/remote_adapter.rs:1412-1539`. `RemoteExtractService` (connect-owned service seam, distinct from the in-process port) is in `crates/spoke-connect/src/remote/serve_ports.rs`.
- Manifest carrier: `schemas/data/host-capability-manifest.schema.json` — `capabilities[]` open string vocabulary.
- Error shape: `schemas/common/error-envelope.schema.json` (`code`/`message`/`details`/`extensions`); existing ops-layer codes in `crates/spoke-operations/src/result.rs:8-35` (incl. `REVISION_CONFLICT`, `INTERNAL_ERROR`).
- Idempotency today: `idempotency_key` on `UpsertRequest` is documented "no server semantics in v0.1" (`schemas/ops/upsert-request.schema.json:16-19`); tool `idempotent` is advisory only (`schemas/data/tool-descriptor.schema.json:56-60`). **This proposal adds real, durable, payload-bound idempotency for the memory family only.**
- OCC precedent: TS `KnowledgeEntryPort.putKnowledgeEntry` documents `expectedBaseRevision` conditional-put semantics (`packages/spoke-operations/src/adapter/ports.ts:25-42`).
- Schema conventions: `schemas/README.md` (draft-07, closed objects, extensions required, one type per file, `<op>-request`/`<op>-response` pairing, codegen via `pnpm run codegen`).

### 18.2 Boundary amendment this proposal makes

The "Author Memory is product-local" boundary statements cited in §18.1 are amended to: *Author Memory is standardized as the optional `spoke-memory` capability family (this document); products that do not implement it keep memory product-local and MUST NOT emulate it through baseline objects (MEM-R019).* World KB and ToM/mental-state surfaces (`modules.mental`, `modules.belief`, `MindState`) remain independent and referenceable; nothing is copied into memory.

### 18.3 Proposed, not current in nexus

Everything in §§3–17. At the research pin (SPOKE 0.14.1) there was no `spoke-memory` capability, no `MemoryPort`, no memory schemas, and no memory ops in the dispatch table; upstream has since landed and published them (see *Landing reconciliation* in §0), while **this repository still consumes neither side of the family** — no `MemoryPort` wiring, no served memory ops, no memory capabilities in the host manifest. This document authorizes none of that work by itself.
