# ADR: Cross-Provider Conformance and Acceptance Matrix

Status: proposed — nothing here authorizes execution, and no scenario has been run in this repository.
Date: 2026-10-28
Scope: The `C-*` scenario ID space for the `spoke-memory` family: two conformance classes (single-provider semantics, cross-provider transfer), fixture semantics, the scenario matrix, nexus acceptance traceability to the `NXM-*` stages, and the execution/evidence policy.
Source: migrated from the retired tracked specs corpus

## Problem

**Landing reconciliation (2026-11-01).** Written against SPOKE 0.14.1, where the family was proposed; upstream 0.15.0 (2026-10-30) landed it and 0.15.1 (2026-10-31) is docs/CI only, published on crates.io and npm. That makes the matrix executable against real upstream participants rather than a proposed shape, with one caveat: the reference policy engine (`spoke-fixture-memory-engine`) ships only as an unpublished path-dependency fixture, so it can serve as the algorithm-variable reference but never as a shipped conformance participant. Nexus has since moved its lockstep pins to 0.15.1 but has run none of these scenarios.

## Proposal

### 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — nothing here authorizes execution, and no scenario has been run in this repository. |
| **Document class** | Exploration |
| **Scope** | The `C-*` scenario ID space for the `spoke-memory` family: two conformance classes (single-provider semantics, cross-provider transfer), fixture semantics, the scenario matrix, nexus acceptance traceability to the `NXM-*` stages, and the execution/evidence policy. |
| **Coordinates with** | [spoke-memory-protocol.md](../../implemented/architecture/2026-10-28-spoke-memory-protocol.md) (`MEM-R001`–`MEM-R027` are owned by the published upstream protocol at the 0.16.0 lockstep — referenced, never redefined here), [spoke-memory-adoption-plan.md](2026-10-28-spoke-memory-adoption-plan.md) §7 (rollout stages `NXM-*` named in the Stage column) |

**Landing reconciliation (2026-11-01).** Written against SPOKE 0.14.1, where the family was proposed; upstream 0.15.0 (2026-10-30) landed it and 0.15.1 (2026-10-31) is docs/CI only, published on crates.io and npm. That makes the matrix executable against real upstream participants rather than a proposed shape, with one caveat: the reference policy engine (`spoke-fixture-memory-engine`) ships only as an unpublished path-dependency fixture, so it can serve as the algorithm-variable reference but never as a shipped conformance participant. Nexus has since moved its lockstep pins to 0.15.1 but has run none of these scenarios.

#### Original framing (research provenance)

> This document owns the `C-*` scenario ID space (assigned by SpokeMemorySpec
> naming contract v1, extended through contract v2.0, 2026-10-28). Protocol
> symbols, operation semantics, and the normative invariants
> (`MEM-R001`–`MEM-R027`) are owned by
> [spoke-memory-protocol.md](../../implemented/architecture/2026-10-28-spoke-memory-protocol.md) and the upstream 0.16.0 publication, and are referenced,
> never redefined. Nexus rollout stages (`NXM-*`) referenced in the Stage
> column are defined in [spoke-memory-adoption-plan.md](2026-10-28-spoke-memory-adoption-plan.md) §7.

### 1. Purpose and scope

> Shorthand used by the tables below: `01` =
> [spoke-memory-protocol.md](../../implemented/architecture/2026-10-28-spoke-memory-protocol.md); `03` =
> [spoke-memory-adoption-plan.md](2026-10-28-spoke-memory-adoption-plan.md).

One unified acceptance matrix for the Memory capability family
(`spoke-memory`, `spoke-memory-reflection`, `spoke-memory-transfer`), executed
against **two real provider implementations**:

- **Local**: the Nexus default local provider — `LocalMemoryProvider:
  MemoryPort` with provider-private storage over real SQLite tables and
  bearer markdown files in an isolated temporary nexus home, with
  **Nexus-owned** algorithm policy (classification, recall scoring/budget,
  staleness, transfer transformation).
- **Remote**: an independent, real, **durable** service provider — a second
  provider with its own persistent store (restart-surviving) and its **own**
  policy implementation, served through `RemoteMemoryService` over a
  loopback SPOKE Connect channel. An engine-internal mock is NOT accepted
  as the second provider.

Policy is provider-owned in both cases and the class-`FREE` rows below
expect it to differ. The reference engine (`spoke-fixture-memory-engine`,
`fixtures/memory-engine/rust`) is path-dependency-only and unpublished, so it
is at most an **offline conformance reference** for the algorithm-variable
rows — never a linked dependency of either participant.

No mocked echo endpoint is acceptable evidence: the remote fixture is a real
provider enforcing the full `MemoryPort` contract over its own real storage
behind the real Connect seam. No scenario requires a paid/live provider, an
installed plugin, a production deployment, or real browser/device evidence;
those belong to a separately authorized workflow and are explicitly out of
scope (§6).

### 2. Two conformance classes

Every scenario is tagged with exactly one class:

- **HARD (protocol invariant)** — the observable result MUST be identical on
  every conforming provider: authorization, CAS and field legality, hard
  budgets, fencing, idempotency/receipts, lifecycle, error envelopes. These
  rows trace to 01's `MEM-R###` invariants; rows pinning an operation
  contract rather than a numbered invariant cite the owning 01 section.
- **FREE (algorithmic freedom)** — ranking/scoring choices, summarization,
  classification disposition, and reflection *content* may differ across
  providers; the row pins only the contractual envelope (bounds,
  determinism per provider, authorization, truncation reporting).

**Async model (frozen, 01):** only `memory.reflections.generate`,
`memory.exports.create`, `memory.imports.stage`, and
`memory.imports.activate` may return a `pending` receipt; every other
mutating operation commits synchronously and returns a terminal
`MemoryReceipt` whose `result` is **content-free** (IDs, revisions, counts,
outcome enums, digests, cost facts only). Long-running work resolves
exclusively via `memory.receipts.get`; no webhooks or streams.

### 3. Fixture semantics

- **Isolation**: each scenario runs in a fresh temporary nexus home
  (SQLite + bearer files) and, for `remote`/`both` rows, a fresh loopback
  Connect session to the independent durable provider's own store. No
  shared state between scenarios; no network beyond loopback.
- **Principal/grants**: fixtures mint a principal and, where a second
  client is required, a scoped `MemoryGrant` through `memory.grants.grant`
  — never by editing storage directly.
- **Crash/timeout injection** uses fixture-controlled fault points inside
  the provider adapter (not process kill of shared infra), so both
  providers exercise the same scenario.
- **Receipts** are read back only through `memory.receipts.get`; fixtures
  never inspect provider-private tables to assert protocol-visible
  outcomes (exceptions: the explicitly local-physical family L rows).
- **Runner invocation**: scenarios are standard libtest named tests in the
  proposed `nexus-memory-conformance` crate (03 NXM-11), filtered by name,
  e.g. `PROPOSED: cargo test -p nexus-memory-conformance c_020`. No custom
  `-- --scenario` harness (libtest does not support it); a custom
  `harness = false` runner is allowed only if explicitly costed in NXM-11.
  No scenario requires running the whole suite locally (`--all`-style full
  runs belong to CI).
- Where a **currently runnable** command exists it is named as-is;
  everything else is marked `PROPOSED`.

### 4. Conformance matrix

Columns: ID | Class | Invariant (01 MEM-R### / owning section) | Action |
Preconditions | Expected observable result | Forbidden side effects |
Applies | Owner | Stage.

#### A. Capability admission and discovery

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-001 | HARD | 01 §Operations (`memory.describe`) | `memory.describe` | host advertises `spoke-memory` | Response carries `memory_protocol_version = 1`, non-empty `engine_policy_version`, advertised sub-capabilities, and `limits` incl. `dedup_retention_seconds` / `receipt_retention_seconds` / `export_chunk_bytes` | None (read-only; no receipt, no state change) | both | qa-engineer | NXM-11 |
| C-002 | HARD | MEM-R019 | any memory op, e.g. `memory.capture` | host does NOT advertise `spoke-memory` | Known op on missing capability ⇒ `capability_missing`; unknown op string ⇒ `op_unsupported` | No silent local fallback; no storage write; no receipt | both | qa-engineer | NXM-11 |
| C-003 | HARD | MEM-R019 + 01 §Operations | `memory.reflections.generate` | `spoke-memory` on, `spoke-memory-reflection` off | `capability_missing`, `retry_class: terminal` | No reflection job created; no receipt | both | qa-engineer | NXM-11 |
| C-004 | HARD | MEM-R019 + 01 §Operations | `memory.exports.create` | `spoke-memory-transfer` off | `capability_missing`, `retry_class: terminal` | No export object created | both | qa-engineer | NXM-11 |
| C-005 | HARD | 01 §Operations (describe/list boundary) | inspect `memory.describe` payload; paginate `memory.spaces.list` | ≥1 space | `describe` carries NO `spaces` array; `spaces.list` is the only catalogue and paginates with bounded pages | No unpaginated second catalogue | both | qa-engineer | NXM-11 |

#### B. Space and subject lifecycle fencing

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-010 | HARD | MEM-R001 | `memory.spaces.list` twice | provider bound | Identical stable space id set, each entry carrying `pending_receipts` and `authority_generation` | None | both | qa-engineer | NXM-11 |
| C-011 | HARD | MEM-R023 (allowlist) | freeze; then attempt: ordinary write, read-only op, `spaces.unfreeze`, `grants.revoke`, `exports.create`, `imports.stage`; then unfreeze and write | space active | Ordinary mutations ⇒ `SPACE_FROZEN`; allowlisted ops (01 §8.2) admitted; after unfreeze writes succeed | No allowlist widening; no ghost write appearing later | both | qa-engineer | NXM-13 |
| C-012 | HARD | MEM-R017 + MEM-R018 | write racing `memory.spaces.freeze` | inflight write at admission boundary | Admission decides: admitted op completes (receipt `committed`); non-admitted ⇒ `SPACE_FROZEN`; receipt records the outcome | No partial row; no ambiguous silence | both | qa-engineer | NXM-13 |
| C-013 | HARD | MEM-R017 + MEM-R024 | client A: `subjects.fence{target:{type:"subject"}, fence}`; client B (granted): write | two clients on one provider | B's write fails `SUBJECT_FENCED`; other subjects unaffected | Fence MUST NOT be a per-process lock illusion: B is a separate client | both | qa-engineer | NXM-13 |
| C-014 | HARD | MEM-R017 + 03 D-13 | Nexus archived Character: fence (not terminal `archive`); read; unfence; restart; repeat | subject with retained records | Writes refused while fenced; retained reads preserved; unfence restores writability; fence state survives provider restart; terminal protocol `archive` NOT used for the Nexus restore path | No irreversible transition on the Nexus archive/restore route | both | qa-engineer | NXM-13 |
| C-015 | HARD | MEM-R023 (drain barrier) | freeze with an admitted long-running op in flight; attempt `exports.create` | pending receipt exists | Freeze result/`spaces.list` report `pending_receipts > 0`; export/cutover blocked (or `require_quiescent` refused) until barrier `pending_receipts == 0` | No export while admitted writes can still commit unnoticed | both | qa-engineer | NXM-13 |
| C-016 | HARD | MEM-R024 | `subjects.fence{target:{type:"context","context_id":b1}}`; write into b1; write into b2/shared | subject with two binding contexts | b1 writes typed-refused; b2 and shared writes succeed; `Subject.fenced_contexts` lists b1; enforced for a second independent client | No subject-wide collateral fencing | both | qa-engineer | NXM-13 |
| C-017 | HARD | MEM-R007 (fence revisions) | concurrent fence/unfence with stale fence revision | fenced subject | Stale fence op ⇒ `REVISION_CONFLICT`; latest fence state wins deterministically; stale in-flight write at fence completion gets typed refusal | No fence state lost-update | both | qa-engineer | NXM-13 |

#### C. Capture, idempotency, receipts

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-020 | HARD | MEM-R021 + MEM-R004 + MEM-R022 | `memory.capture` with `idempotency_key` + immutable `capture_scope` | admitted subject | Receipt `committed`, content-free (IDs/digests/counts); candidate visible via `candidates.list` | Candidate MUST NOT appear in `memory.recall`; receipt carries no body content | both | qa-engineer | NXM-11 |
| C-021 | HARD | MEM-R021 | identical retry (same `(space_id, requester_principal_id, op, idempotency_key)`, same JCS-canonical payload) | first committed | Replay returns the original receipt; exactly one candidate | No duplicate; no re-execution | both | qa-engineer | NXM-2/NXM-11 |
| C-022 | HARD | MEM-R021 | same idempotency_key, different payload | first committed | `IDEMPOTENCY_CONFLICT`, terminal | No overwrite | both | qa-engineer | NXM-2/NXM-11 |
| C-023 | HARD | MEM-R010 | response-loss after commit; retry | fault fixture armed | Retry returns original receipt via replay; one candidate | No effect re-run | both | qa-engineer | NXM-2/NXM-11 |
| C-024 | HARD | MEM-R010 | mid-commit crash; retry | fault fixture armed | `OUTCOME_UNCERTAIN` (`retry_class: uncertain`); `receipts.get` resolves terminal; subsequent retry replays | No blind re-apply | both | qa-engineer | NXM-2/NXM-11 |
| C-025 | HARD | MEM-R003 | foreign/ungranted principal captures | grant absent | `NOT_AUTHORIZED` (or `NOT_FOUND`-class, indistinguishable per 01) | Zero bytes persisted | both | qa-engineer | NXM-11 |
| C-026 | HARD | MEM-R017 + MEM-R024 | capture to fenced subject / fenced context | C-013/C-016 fence active | `SUBJECT_FENCED`; no candidate | No partial candidate row | both | qa-engineer | NXM-13 |
| C-027 | HARD | MEM-R022 + MEM-R014 | capture→review→delete record; replay old receipts | committed history | Receipt summaries still resolve but disclose IDs/revisions/counts only — no deleted content | Immutable receipts MUST NOT leak erased bodies | both | qa-engineer | NXM-2/NXM-11 |
| C-028 | HARD | MEM-R021 (retention) | replay a key after receipt eviction, inside `dedup_retention_seconds`; then beyond the dedup window | clock-fixture control | Inside window ⇒ `RECEIPT_EXPIRED` (terminal, `details.last_known_status`); beyond window ⇒ key treated as new | No silent re-run of an evicted committed key | both | qa-engineer | NXM-2 |
| C-029 | HARD | MEM-R021 (namespace) | reconnect the Connect session between attempt and retry | remote provider | Retry after reconnect resolves to the same logical identity and receipt | Peer-session-derived identity MUST NOT change dedup outcome | remote | qa-engineer | NXM-10 |

#### D. Candidate lifecycle

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-030 | HARD | MEM-R006 + MEM-R015 | `candidates.list` over N>page rows | N candidates seeded | Bounded pages, cursor bound to principal+space+subject+selection+snapshot; union equals the authorized set | No unbounded page; no cross-subject row | both | qa-engineer | NXM-11 |
| C-031 | HARD | MEM-R009 | `candidates.review{promote}`; then `candidates.get` | one candidate | Candidate consumed AND record created atomically at the provider boundary; `candidates.get` returns the consumed shell (content-scrubbed, `content_digest` retained) | No half state; no raw content in the consumed shell | both | qa-engineer | NXM-8/NXM-11 |
| C-032 | HARD | MEM-R009 + MEM-R010 | `candidates.dismiss`; receipt replay | one candidate | Candidate consumed, no record; replay returns original outcome | No resurrection | both | qa-engineer | NXM-11 |
| C-033 | HARD | MEM-R009 | two concurrent `candidates.review` on one candidate | two clients | Exactly one commits; the second MUTATE gets `CANDIDATE_CONSUMED`; exactly one effect | No double-promote | both | qa-engineer | NXM-11 |
| C-034 | HARD | 01 §Operations (`capture_scope` inheritance) + MEM-R003 | review-promote a binding-scoped candidate; then attempt shared visibility | candidate captured with `capture_scope{context_id}` | Review decision inherits `capture_scope` (no scope field in the decision); record stays binding-placed; shared visibility only via `records.promote` with `records_write` on BOTH source and target | No silent cross-context promotion; no advisory-scope escalation | both | qa-engineer | NXM-8/NXM-11 |
| C-035 | HARD | MEM-R019 | review action `merge` (unimplemented in reference source) | one candidate | `op_unsupported` / `INVALID_INPUT`, terminal | Candidate left unconsumed; no fake success | both | qa-engineer | NXM-8 |
| C-036 | HARD | MEM-R021 (replay ordering) | replay a committed capture AFTER later revisions/fence changes | committed receipt exists | Replay returns the stored receipt; stale CAS/fence checks are NOT re-run against current state | No double effect from replay | both | qa-engineer | NXM-2 |
| C-116 | HARD | 01 §Operations (`capture_scope`) + 03 §6 | binding-local candidate review-promote on a Character subject | Character with binding context | Record keeps the binding `capture_scope`; shared visibility only via explicit dual-authorized `records.promote` (protocol-level; placed in family D as a candidate-lifecycle rule) | No silent cross-context/world promotion | both | qa-engineer | NXM-8 |

#### E. Retained records

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-040 | HARD | MEM-R007 | create/get/update/delete round trip | admitted subject | All ops succeed; stale-revision update ⇒ `REVISION_CONFLICT`, zero write; genuine no-op update semantics per 01 (no spurious revision bump) | No write on failed CAS | both | qa-engineer | NXM-11 |
| C-041 | HARD | MEM-R007 | update; read revision token | one record | Revision changes exactly once per committed update; cross-provider comparisons treat revisions as opaque | No revision reuse | both | qa-engineer | NXM-11 |
| C-042 | HARD | MEM-R013 | after C-071: add NEW record / delete record / edit / profile update / scope-promote into the basis | reflection generated | `reflections.state` reports staleness for every basis-affecting change class, not only edits to pre-existing basis ids | No stale reflection served as fresh | both | qa-engineer | NXM-11 |
| C-043 | HARD | MEM-R015 | `records.list` over N>page rows | seeded records | Bounded pages, stable order, terminal cursor | No unbounded response | both | qa-engineer | NXM-11 |
| C-044 | HARD | MEM-R003 + MEM-R017 | `records.promote` without grant / missing target write authority / in frozen space | record in context scope | Without authority ⇒ `NOT_AUTHORIZED`; frozen ⇒ `SPACE_FROZEN`; granted+active ⇒ promotion with full provenance preserved | No promotion side effect on refusal | both | qa-engineer | NXM-13 |
| C-045 | HARD | MEM-R015 | create/update above advertised bound | bound from `describe.limits` | `INVALID_INPUT`, terminal, bound reported | No partial/truncated silent write | both | qa-engineer | NXM-11 |
| C-046 | HARD | MEM-R011 | all read-only ops (get/list/recall/profile.get/reflections.state/list/receipts.get) | any state | Succeed; storage and receipt log byte-identical before/after | Read ops MUST NOT mutate or trigger generation | both | qa-engineer | NXM-11 |
| C-047 | HARD | MEM-R026 | `records.get` / `candidates.get` with a revoked grant; by-id read after scope change | grant revoked between list and get | By-id route rechecks CURRENT authorization ⇒ `NOT_AUTHORIZED`/`NOT_FOUND`-class | No cached-authorization read | both | qa-engineer | NXM-11 |

#### F. Recall and read selection

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-050 | HARD | MEM-R006 | recall `subject_all` with unrestricted grant (`context_ids` omitted) | records in shared + 2 contexts | All of the subject's retained records; no other subject's | No candidate rows | both | qa-engineer | NXM-7/NXM-11 |
| C-051 | HARD | MEM-R006 | recall `shared_only` with `shared_access` grant | as C-050 | Only shared-placement records | Context-placed records excluded | both | qa-engineer | NXM-7/NXM-11 |
| C-052 | HARD | MEM-R006 | recall `selected_contexts{context_ids:[c1], include_shared:true}` with grant covering c1 + `shared_access` | as C-050 | Union of c1 and shared | c2 excluded | both | qa-engineer | NXM-7/NXM-11 |
| C-053 | HARD | MEM-R006 | recall `selected_contexts{context_ids:[c1], include_shared:false}` | as C-050 | Exactly c1 | Shared excluded; no null/empty-array overload accepted as a mode | both | qa-engineer | NXM-7/NXM-11 |
| C-054 | HARD | MEM-R004 | recall in all three modes | unconsumed candidates exist | Candidates never appear | — | both | qa-engineer | NXM-11 |
| C-055 | HARD | MEM-R015 (hard budget) | recall with `max_bytes` below corpus; force oversized first item | large corpus | Hard UTF-8 byte budget over serialized content respected; a single oversized first item is returned WHOLE with `overflow_record_id` flagged; truncation reported | Budget MUST NOT be silently exceeded; oversized item MUST NOT be silently dropped/skipped | both | qa-engineer | NXM-11 |
| C-056 | HARD | MEM-R026 | continue a list cursor after a grant/state change | open cursor | `CURSOR_INVALID` (terminal); re-issue with fresh snapshot | No hidden-row leak across the invalidation boundary | both | qa-engineer | NXM-11 |
| C-057 | HARD | 01 §7.22 (request shape) | recall with `selection` omitted, then with `selection: null` | any authorized grant | `INVALID_INPUT` in both cases — `selection` is a required `ReadSelection`, so its absence is a format error | MUST NOT be answered `NOT_AUTHORIZED`: format validation is not an authorization outcome | both | qa-engineer | NXM-11 |
| C-057A | HARD | MEM-R006 (grant scoping) | context-limited grant (no `shared_access`) calls recall with well-formed selections outside its coverage: `subject_all`, then `shared_only` | scoped grant | Both refused (`NOT_AUTHORIZED`) — a well-formed selection never widens to shared/subject-all | No scope amplification by selection choice | both | qa-engineer | NXM-11 |
| C-058 | FREE | algorithmic freedom envelope | same recall inputs twice per provider; compare across providers | seeded corpus | Per provider: identical inputs → identical selection; across providers: ranking/order MAY differ, invariants (authorization, budget, exclusion rules) hold | No cross-provider identical-ranking requirement | both | qa-engineer | NXM-11 |
| C-059 | HARD | 01 §Operations (`ProvenanceFilter`) | recall with `selection` + `filter{source_context_ids:[w1]}` | records with mixed provenance | Provenance filter ANDed with placement selection: only records whose provenance includes w1 within the selected placement | Filter MUST NOT act as placement (no visibility widening) | both | qa-engineer | NXM-7/NXM-11 |

#### G. Profile documents

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-060 | HARD | 01 §ProfileDocument (trust) | `profile.get{kind:"personality"}` | author-maintained profile exists | Content byte-preserved (local: matches SOUL.md body) | No captured/untrusted text merged in | both | qa-engineer | NXM-4/NXM-11 |
| C-061 | HARD | MEM-R007 | `profile.update` stale then fresh revision | profile exists | Stale ⇒ `REVISION_CONFLICT`, no write; fresh ⇒ committed | No lost update | both | qa-engineer | NXM-4/NXM-11 |
| C-062 | HARD | MEM-R005 | replacement admission: required authored content profile missing / optional absent / capability absent | three fixtures | Required missing ⇒ `PROFILE_INCOMPLETE`; optional absent ⇒ honest-empty success; capability absent ⇒ `capability_missing` at selection — three DISTINCT outcomes | No collapsed single-code handling; no empty success for required content | both | qa-engineer | NXM-4/NXM-11 |
| C-063 | HARD | 01 §ProfileDocument (trust separation) | capture text; read profile | candidate captured | Profile unchanged | Captured text MUST NOT auto-enter any profile | both | qa-engineer | NXM-11 |
| C-064 | HARD | 01 §ProfileDocument (content kinds) | provider selection with zero profile documents of kinds `transfer`/`governance`/`reflection` | fresh provider | Selection succeeds or fails on CAPABILITY grounds only; no such fake documents are required | No capability concepts as ProfileDocument kinds | both | qa-engineer | NXM-4 |

#### H. Reflections

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-070 | HARD | MEM-R012 | `reflections.generate` without/with reflect grant, with explicit `selection`+`filter` | `spoke-memory-reflection` on | Without grant ⇒ `NOT_AUTHORIZED`; with grant ⇒ receipt `committed` OR `pending` — §7.0.3 lets the four long-running ops commit synchronously or complete asynchronously | No implicit generation from read ops | both | qa-engineer | NXM-11 |
| C-071 | HARD | MEM-R012 + MEM-R010 | poll `receipts.get` to terminal; `reflections.list` | C-070 granted call | Receipt terminal `committed` (immediately when the provider committed synchronously, else after `pending`); reflection listed with pinned basis (selection+filter+source-set digest+revisions+`engine_policy_version`) | No webhook/stream dependence | both | qa-engineer | NXM-11 |
| C-072 | FREE | 01 §Reflections (envelope) | inspect generated reflection | C-071 done | Envelope valid (ids, pinned basis, timestamps); content MAY differ across providers | No identical-text requirement | both | qa-engineer | NXM-11 |
| C-073 | HARD | MEM-R011 | `reflections.state`, `reflections.list` | any state | Succeed; `state` reports `data_sufficiency`; no mutation, no receipts | MUST NOT trigger generation | both | qa-engineer | NXM-11 |
| C-075 | HARD | MEM-R006 + MEM-R012 | context-restricted reflect grant calls state/list/generate on subject-wide selection | grant limited to c1 | Refused (`NOT_AUTHORIZED`) — a context-limited grant cannot read/generate all-subject derived text | No derived-text scope leak | both | qa-engineer | NXM-11 |
| C-076 | HARD | 01 §Reflections (PUBLISH-PINNED) | generate while basis sources change mid-run | racing fixture | Exactly one completion rule observed: published WITH the pinned basis (`state_digest`); reads report staleness once basis moved — never a false-current result | No silently mixed-basis output | both | qa-engineer | NXM-11 |
| C-077 | HARD | MEM-R013 + MEM-R014 | delete a basis record; then list/get/export reflections | C-071 done | Reflections whose basis references tombstoned records are EXCLUDED from list/export | Deleted content MUST NOT leak through derived text | both | qa-engineer | NXM-11 |
| C-078 | HARD | MEM-R016 (failure honesty) | force generation failure after cost incurred; retry | fault fixture | Failed/`OUTCOME_UNCERTAIN` receipt carries content-free cost facts; recovery never double-charges; `INTERNAL_ERROR` is retryable ONLY when no effect committed | No duplicate paid generation; no input text in cost facts | both | qa-engineer | NXM-2/NXM-11 |
| C-079 | HARD | 01 §Reflections (`data_sufficiency`) | generate with insufficient basis per provider policy | tiny corpus | `INSUFFICIENT_DATA` (terminal) or explicit `data_sufficiency:"insufficient"` state per provider policy | No protocol-wide hard-coded Nexus threshold | both | qa-engineer | NXM-11 |

#### I. Export, import, recovery

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-080 | HARD | MEM-R025 + MEM-R014 | `exports.create` → poll → `exports.get(export_id, chunk_ordinal)` over all ordinals | `spoke-memory-transfer` on | Chunks respect `export_chunk_bytes`; per-chunk `chunk_digest` + whole `bundle_digest` verify; bundle includes candidate shells + content-free `receipt_summaries` + subjects/records/profiles/reflections/grants slices; deleted content excluded from default export (translation maps are staging products asserted in C-081's `staged_report`, not bundle content) | No credentials in any chunk; no silent truncation; no single-giant-bundle | both | qa-engineer | NXM-12 |
| C-081 | HARD | MEM-R025 (staging) | `imports.stage` per chunk in ordinal order (ordinal 0 carries `conflict_policy` + `adopt_grants`); recall before activation | bundle from C-080 | Staged data invisible; final chunk yields `staged_report` incl. `candidate_id_map` | No activation without explicit op; out-of-order chunk ⇒ typed refusal | both | qa-engineer | NXM-12 |
| C-082 | HARD | MEM-R025 (activation) | `imports.activate` (may be `pending`; poll) | staged import | Records/candidates/profiles live; imported records carry `import_ref`; imported reflections enter STALE pending regeneration | No partial activation visible; no claimed-valid imported reflections | both | qa-engineer | NXM-12 |
| C-083 | HARD | 01 §Transfer (`imports.discard`) | `imports.discard` | staged import | Staged data gone; target byte-identical to pre-stage | No residue | both | qa-engineer | NXM-12 |
| C-084 | HARD | MEM-R027 (rollback) | case (a): discard/abandon frozen target before any target write, controlled binding rollback with NEW `authority_generation`; case (b): after target writes — freeze+drain target (`pending_receipts==0`), reverse transfer via `exports.create(require_quiescent:true)` + stage/activate target→source, verify, reopen source | two fixtures | (a) source reopened, target abandoned cleanly; (b) source reopened only after verified reconciliation; until then source stays frozen and status is explicitly blocked/uncertain | NEVER delete target-written history; NEVER dual authority; NO compensating-delete-and-revert | both | qa-engineer | NXM-12 |
| C-085 | HARD | 01 §7.32–7.33 (`IMPORT_CONFLICT`) | `imports.activate` after the staged import was discarded or expired | discarded/expired staged import | `IMPORT_CONFLICT`, terminal; nothing half-applied | No activation of a discarded/expired staging area | both | qa-engineer | NXM-12 |
| C-086 | HARD | 01 §Errors (`EXPORT_EXPIRED`) | `exports.get` after expiry | expired export | `EXPORT_EXPIRED`, terminal | No expired chunk served | both | qa-engineer | NXM-12 |
| C-087 | HARD | MEM-R027 + MEM-R010 | crash between source freeze and target activation; resume | mid-transfer fixture | Source remains frozen (clients observe `SPACE_FROZEN`); receipts replayable; resume completes or rolls back explicitly | No dual authority window | both | qa-engineer | NXM-12 |
| C-088 | HARD | MEM-R025 (grant adoption) | import bundle containing grant summaries WITHOUT `adopt_grants`; then with explicit `adopt_grants` entries | staged import | Without adoption: imported grants inactive; with `{grant_id, principal_id}` entries: exactly those adopted for the named target principals | Grants NEVER auto-activate; no blind authorization | both | qa-engineer | NXM-12 |
| C-089 | HARD | MEM-R027 (authority pinning) | after cutover: write via old provider; write via client with stale `authority_generation` | completed activation | Old provider stays frozen and rejects writes; stale client fails typed; nothing auto-unfreezes | No stale-authority write path | both | qa-engineer | NXM-12 |

#### J. Grants and concurrency

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-090 | HARD | MEM-R003 + MEM-R020 | scoped grant; granted client exercises by-id reads, candidates, receipts, profiles, reflections, exports, counts — not only recall | holder + second client | Every surface enforces the grant scope; out-of-scope ⇒ `NOT_AUTHORIZED` | Grant semantics MUST NOT derive from KnowledgeEntry ownership or `Scope.viewpoint` | both | qa-engineer | NXM-11 |
| C-091 | HARD | MEM-R018 | `grants.revoke`; subsequent op; inflight admitted op during revoke | C-090 grant | Subsequent ops ⇒ `GRANT_REVOKED` at admission; already-admitted op completes, receipt-recorded | No retroactive cancellation of admitted ops; no silent continuation | both | qa-engineer | NXM-13 |
| C-092 | HARD | MEM-R007 | two clients concurrently update one record | both granted | One commits; other gets `REVISION_CONFLICT`; final state equals exactly one write | No lost update, no merged write | both | qa-engineer | NXM-11 |
| C-093 | HARD | MEM-R017 + MEM-R018 | revoke/fence concurrent with write | racing fixture | Admission decides: admitted completes, non-admitted typed refusal; receipt reflects outcome | No ambiguous half-state | both | qa-engineer | NXM-13 |
| C-094 | HARD | MEM-R002 | same subject id string on two providers; import between them | two providers | No automatic identity merge; only the explicit staged id maps link records | Same-ID MUST NOT auto-link identities | both | qa-engineer | NXM-12 |
| C-095 | HARD | MEM-R020 (non-amplification) | scoped admin attempts to grant wider scope (context superset / `shared_access` upgrade / longer `expires_at`) | scoped admin grant | Refused (`NOT_AUTHORIZED`); issuance must be ⊆ granter's effective coverage incl. expiry | No privilege self-amplification | both | qa-engineer | NXM-11 |
| C-096 | HARD | 01 §Grants (bootstrap) | fresh provider: enumerate how first admin/space exists | new provider fixture | Spaces + initial admins exist via provider-operator provisioning (local: the operator CoreService open); discoverable via describe/spaces.list | No uncallable grant/bootstrap cycle; no protocol v1 space-creation op assumed | both | qa-engineer | NXM-9 |

#### K. Error envelope and retry classes

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-100 | HARD | MEM-R016 | force one error of each class | fault fixtures | Every error carries `details.retry_class`; `terminal` safe-no-retry; `uncertain` requires `receipts.get` first | No class-less error | both | qa-engineer | NXM-2/NXM-11 |
| C-101 | HARD | MEM-R019 | unknown op string; known op on capability-less host | fixture hosts | Unknown ⇒ `op_unsupported`; known-op-missing-capability ⇒ `capability_missing` | No silent fallback to another provider/local | both | qa-engineer | NXM-11 |
| C-102 | HARD | MEM-R003 | foreign vs missing subject/record ids | foreign-id fixture | Same `NOT_FOUND`-class shape for both | No existence oracle | both | qa-engineer | NXM-11 |
| C-103 | HARD | MEM-R016 (INTERNAL_ERROR honesty) | force internal failure before vs after effect commit | fault fixtures | No-effect failure may be `retryable`; post-effect failure is NEVER claimed no-effect (uncertain/failed receipt path instead) | No blanket retryable INTERNAL_ERROR | both | qa-engineer | NXM-2 |
| C-104 | HARD | MEM-R003 + MEM-R022 | revoked principal calls `receipts.get` | revoked grant | Refused (`NOT_AUTHORIZED`/`GRANT_REVOKED`) — receipt reads recheck current authorization | No receipt disclosure to revoked principals | both | qa-engineer | NXM-11 |

#### L. Nexus local-provider specifics (local only)

| ID | Class | Invariant (01) | Action | Preconditions | Expected result | Forbidden side effects | Applies | Owner | Stage |
|---|---|---|---|---|---|---|---|---|---|
| C-110 | HARD | 03 §5.1 (writer guard) | direct SQL write outside the writer protocol on the local provider DB | local provider open | Rejected `WRITER_FENCED` (existing trigger behavior byte-preserved) | Provider internals MUST NOT bypass the guard | local | qa-engineer | NXM-1 |
| C-111 | HARD | 03 §6 (entry convergence) | grep audit of `apps/nexus42/src/commands/` | NXM-6 landed | No `memory_io` / `soul_io` imports outside the adapter seam; Character memory/ToM CLI family routed through the converged service path | No CLI file bypass | local | architect | NXM-6 |
| C-112 | HARD | 03 §5.1 (layout preservation) | run capture→review→promote; inspect home dir | local provider | `SOUL.md` and `memory/long-term/*.md` still the authoritative author-visible artifacts; nine-value kind taxonomy and frontmatter (slug, `source_session_ids`, user/pushed sections) preserved per 03 §4.6 | No hidden second store; no kind/frontmatter loss | local | qa-engineer | NXM-5 |
| C-113 | HARD | 03 §5.2 (journal recovery) | inject crash before journal commit / after journal commit pre-publish / post-publish | kill fixtures | Pre-commit: no visible effect; post-commit: recovery publishes or quarantines per journal; recovering objects EXCLUDED from recall/list/export until resolved | No half-committed content readable; no file+SQL atomicity claim | local | qa-engineer | NXM-3 |
| C-114 | HARD | 03 §3.2 (bearer validation) | malformed/`..`/non-`ctr_` creator ids through adapter | local provider | Same validation refusals as current `bearer.rs` | No path escape | local | qa-engineer | NXM-5 |
| C-115 | HARD | 03 D-4 (run-capture boundary) | Host `remember:true` prompt | any session | `invalid_input` (non-Character) / `not_supported` (Character) — unchanged | This program MUST NOT re-enable auto capture | local | qa-engineer | NXM-5 |
| C-117 | HARD | 03 §5.1 (control metadata) | run NXM-1 migration twice; inspect registration | fresh + migrated home | Migration idempotent; registered in `core_writer_registration`; `DB_SCHEMA_VERSION` bumped; old-binary compatibility statement verified or backup-restore documented | No unregistered schema drift | local | qa-engineer | NXM-1 |
| C-118 | HARD | 03 NXM-16 (Creator capture parity) | Creator explicit capture→review→promote via typed core/native/service/CLI path | NXM-16 landed | Creator path behaves identically-in-contract to Character capture family; auto `remember` still refused (C-115) | No Host auto-capture introduced | local | qa-engineer | NXM-16 |
| C-119 | HARD | 03 §5.3 (snapshot consistency) | export the local provider mid-write-load with `require_quiescent` | write load fixture | Export runs only under drain barrier; snapshot is SQL+file consistent (digests verify after restore to staging) | No torn SQL+file snapshot | local | qa-engineer | NXM-12 |

### 6. Execution and evidence policy

- Runner: `PROPOSED` crate `nexus-memory-conformance` (03 NXM-11) with
  scenarios as standard named tests, filtered by name:
  `cargo test -p nexus-memory-conformance c_020` (single scenario) or a
  family prefix (`c_08`). No custom `-- --scenario` flag (libtest does not
  support it); a `harness = false` runner only if explicitly costed in
  NXM-11. Full-matrix runs belong to CI, not local gates. Precedent: the
  existing `nexus-provider-conformance` neutral CI runner (credential-free).
- Existing runnable regression anchors reused by this program (verified to
  exist today): `cargo test -p nexus-local-db pending_review`;
  `cargo test -p nexus-core memory_pipeline` (includes
  `passthrough_summarizer_includes_untrusted_header`);
  `cargo test -p nexus-creator-memory` (review classifier and bearer tests).
- Local provider rows run against real SQLite + real files in a temp home —
  no in-memory fake store. Remote rows run against the independent durable
  loopback provider — no echo mock, no engine-internal test double.
- Paid/live providers, installed plugins, production deployments, and real
  browser/device evidence are **not** required and **not** claimed; they
  require a separate authorized workflow outside this package.
- A scenario passes only when its expected result is observed through
  protocol-visible surfaces (operations, receipts, errors) — never through
  provider-private storage inspection, except family L rows which are
  explicitly local-physical.

### 7. Current-vs-proposed statement

Every "Expected result" above is a **proposed contract** traced to 01's
invariants (MEM-R001–R027); current Nexus behavior differs where 03 §3.4
records gaps (unwired Creator production capture, refused run capture, CLI
bypass, ambiguous scope). Nothing in this matrix asserts that current code
already passes.

## Alternatives considered

Not stated as a separate comparison in the migrated proposal; where the body names a rejected direction, that record stays in the proposal.

## Acceptance criteria

| User/program goal | 03 task(s) | Scenario IDs |
|---|---|---|
| Default local provider, zero network, content layout preserved | NXM-1..5 | C-001..C-005, C-010..C-017, C-020..C-059, C-110..C-114, C-117 |
| All entry points converge; no CLI/MCA bypass | NXM-5..8 | C-111, plus family A–F rows on the converged path |
| Creator/Character explicit-capture parity (auto capture still refused) | NXM-16 | C-118, C-115 |
| Placement vs provenance disambiguation (binding placement; world filter; immutable `capture_scope`) | NXM-5, NXM-7, NXM-8 | C-034, C-050..C-054, C-059, C-116 |
| Credentials protected; secrets never persisted | NXM-9 | C-080 (chunk hygiene), NXM-9 stage AC |
| Remote provider via Connect, no cloud-sync proxy | NXM-10 | all `both` rows against the durable loopback provider; C-029 |
| Receipts/idempotency incl. uncertain replay, retention, content-free summaries | NXM-2 | C-021..C-024, C-027, C-028, C-036, C-100, C-103, C-104 |
| Lifecycle fencing (subject + context) across clients; reversible archive | NXM-13 | C-011..C-017, C-026, C-044, C-091, C-093 |
| Transfer completeness + rollback without data loss or split authority | NXM-12 | C-080..C-089, C-094, C-119 |
| Profile trust, content kinds, and admission triage | NXM-4 | C-060..C-064 |
| Reflections: scoped, pinned, invalidated, failure-honest | NXM-11 | C-042, C-070..C-079 |
| Grant model self-contained, non-amplifying, operator-bootstrapped | NXM-9, NXM-11 | C-057A, C-075, C-090, C-095, C-096 |
| Honest capability surface | NXM-8 | C-002..C-005, C-035, C-101 |
| Bounded reads, hard budgets, cursor integrity | NXM-11 | C-030, C-043, C-045, C-055, C-056 |
| Auto run capture stays refused (separate product decision) | D-4 boundary | C-115 |

## Risks

Not stated separately; the proposal body carries its non-goals and open questions where present.
