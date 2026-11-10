---
module: nexus architecture / knowledge / world-kb
date: 2026-11-10
status: proposed
problem_type: design
category: architecture
severity: info
tags:
  - knowledge-ir
  - spoke
  - knowledge-entry
  - ports-adapters
  - world-kb
  - kb-store
  - simulation
---

# Knowledge IR — Spoke KnowledgeEntry as SSOT with Local Read/Write Adapters

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here schedules crates, migrates tables, or claims a shipped feature. |
| **Document class** | Exploration |
| **Scope** | Implementation design for a **Knowledge IR** after the leftover UserScope, reference-source, and `creator kb` work-scope file-index surfaces are retired. The IR **is** the existing Spoke `KnowledgeEntry` protocol (plus the already-specified `extensions.nexus` contract). Local Nexus code uses **read/write adapters** around that protocol: wire `KnowledgeEntry` → domain `KnowledgeEntryRecord` → `KbStore` / `SqliteKbStore` (`kb_key_blocks.body_json`, plus anchors, relationships, holders, extract jobs). The retired `creator kb` file index is **not** an adapter of this IR. |
| **Coordinates with** | [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md) (conversion seam, `/kb/` HTTP stability, `extensions.nexus`), [world-kb-runtime-architecture.md](../../active/architecture/world-kb-runtime-architecture.md), [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §5.1 / §5.5–§5.6, [holder-governance.md](../../active/architecture/holder-governance.md), [llm-extract.md](../../active/orchestration/llm-extract.md), [local-db-schema.md](../../active/runtime/local-db-schema.md), stacked base [retire-user-scope-and-references.md](retire-user-scope-and-references.md) ([PR #486](https://github.com/42ch-dev/nexus/pull/486)) |
| **Depends on** | [PR #486](https://github.com/42ch-dev/nexus/pull/486) (`proposal/retire-user-scope-and-references`). This branch is stacked on that one. **Merge order is #486 then this PR.** Implementation of this design SHOULD wait until #486 is at least merged as direction, and SHOULD prefer waiting until the *retirement implementation* has removed UserScope, reference sources, and the `creator kb` file index (or a written waiver records why a knowledge-adapter PR must start earlier without reintroducing those stores or the old index). |
| **Path choice** | Lands under `proposed/architecture/` next to World KB and Spoke adapter Masters. There is no `knowledge/` class axis. This is not a `creator/` or `orchestration/` spec. |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only (if adopted).

**Not implement authority.** Per [`.mstar/specs/AGENTS.md`](../../AGENTS.md), Exploration under `proposed/` never outranks active Masters (Spoke adapter, World KB runtime, entity-scope, holder-governance, local-db schema). Adoption requires a later Draft/Master promotion and link-migration sweep. **Merging this file does not authorize production code changes.**

### 0.1 Why this is an implementation design, not a new model

World KB already *is* "protocol → domain record → SQLite adapter":

- Spoke `KnowledgeEntry` is the interchange / disclosure type.
- `KnowledgeEntryRecord` is the Nexus domain aggregate (`nexus-knowledge`).
- `nexus-spoke-adapter` (`conversion/knowledge_entry.rs`: `knowledge_record_to_spoke` / reverse) is the sole conversion seam.
- `KbStore` / `SqliteKbStore` persist `body_json` (and related columns) in workspace `state.db` table `kb_key_blocks`, with source anchors, relationships, holders, and extract jobs beside it.

The remaining problem is **not** "we lack a knowledge model." It is that several leftover I/O faces made the family look like four stores. After [PR #486](https://github.com/42ch-dev/nexus/pull/486) withdraws UserScope, reference sources, and the `creator kb` work-scope file index, this document records how the **remaining** local implementation must keep walking one IR (Spoke `KnowledgeEntry`) through ports, instead of growing new ad-hoc writers or keeping the old filesystem index as a staging shim.

**Anti-pattern:** inventing a parallel "Knowledge IR" schema, document, or crate that restates `KnowledgeEntry`. The protocol is the IR. Local types are projections and adapters.

### 0.2 Core claim

**Spoke `KnowledgeEntry` is the SSOT** for narrative / World (and Actor) knowledge content and the protocol fields already defined for it. This Exploration does **not** replace that protocol. It constrains Nexus to:

1. treat `KnowledgeEntryRecord` as an in-process projection of that SSOT, not a second authority;
2. treat `SqliteKbStore` (and any future store) as a persistence adapter, not an informal SSOT of its own;
3. treat HTTP `/kb/` and `creator world kb` as wire / operator adapters on the same port;
4. stub the write port in Simulate / dry-run / extract-preview paths so they cannot insert real `kb_key_blocks` rows.

The `creator kb` work-scope file index is **out of this IR**. #486 retires it. This design MUST NOT keep it as an import / staging / compatibility adapter.

Today's Shipped Masters remain implement authority until a later promotion PR. This file is the migration destination for "how local I/O is allowed to touch knowledge," not a silent rewrite of promotion state machines, relationship semantics, or holder/governance.

### 0.3 Explicit exclusions (MUST NOT enter this IR)

| Concern | Where it lives | Why it is out |
| --- | --- | --- |
| User KnowledgeStore / UserScope (`knowledge_entries`, `creator knowledge`, MCA `## User Knowledge`) | Retired by [PR #486](https://github.com/42ch-dev/nexus/pull/486) | Unfinished placeholder. Future User-scope notes, if any, are a **separate** Exploration. MUST NOT be re-keyed as `KnowledgeEntry`. |
| Reference sources (`reference_sources` + `references/units/<id>/body.md`) | Retired by #486 | Source-material leftover. A future source-material port is a **separate** Exploration. MUST NOT be upserted into `kb_key_blocks` as a migration. |
| `creator kb` work-scope file index (`kb/index.json` + `kb/entries/<id>.md`, `nexus42 creator kb`, HTTP `/v1/daemon/kb/entries`) | Retired by #486 | Leftover filesystem-core face. MUST NOT be kept as an import / staging / compatibility adapter in this IR. A future workspace staging design, if any, is a **separate** Exploration. Retiring the code itself belongs to #486's implementation PR; this PR does not delete it. |
| Creator / Character memory | `spoke-memory` / `MemoryPort` (N1–N3 / N4-R shipped; N4-T / N5 open) | Different Spoke family. Protocol already MUST NOT grow `KnowledgeEntry` fields or simulate memory with knowledge rows. |
| Workflow IR | [PR #483](https://github.com/42ch-dev/nexus/pull/483) | Strategy orchestration SSOT only. |
| Work Artifact IR | [PR #485](https://github.com/42ch-dev/nexus/pull/485) | Work-tree artifacts only. `Works/.../Logs/kb/` rejected-candidate logs stay Work artifacts. |
| Research-run `.nexus42/references/<run_id>/` | Orchestration / research presets | Run artifact directories, not World KB. |

If a Workflow Simulate or Work dry-run path would write knowledge, it MUST use **this** port's stub (no real `kb_key_blocks` writes), just as #483 must stub Work artifact writes through #485.

### 0.4 Recommended sequence

```text
#486 merged (retirement Exploration; direction only)
        │
        ▼
This PR merged (Knowledge IR Exploration; still not implement authority)
        │
        ▼
Retirement implementation PR for #486 (authorized separately; preferred before knowledge code changes)
        │
        ▼
Draft/Master promotion of this design (optional staging: start P0 against current World KB Masters)
        │
        ▼
§5 P0  document / test the existing conversion + store as the port
        │
        ▼
§5 P1  close remaining direct writers onto KbStore
        │
        ▼
§5 P2  Simulate / extract-preview stub
        │
        ▼
§5 P3  after #486 implementation: drop leftover UserScope /
       reference / creator kb file-index references
        │
        └─ holder/governance / extract promotion remain on existing Masters
```

The previous "P3: keep `creator kb` as an import adapter" default is withdrawn; retiring that surface is #486.

**Anti-pattern:** starting Knowledge "cleanup" PRs that still read `knowledge_entries`, register references, or scan `kb/index.json` "for compatibility."

---

## 1. Current inventory (evidence)

Evidence is from production sources under `crates/` and `apps/` on `origin/main` at `5f27b8bfa` (post #484), plus the retirement design in #486. Symbols are named for re-verification; line numbers are orientation only.

### 1.1 The stack that already exists

```text
Spoke KnowledgeEntry          <- protocol SSOT (wire / disclosure)
        │  nexus-spoke-adapter
        │  knowledge_record_to_spoke / reverse
        ▼
KnowledgeEntryRecord          <- Nexus domain projection (nexus-knowledge)
        │  KbStore port
        ▼
SqliteKbStore                 <- persistence adapter
        │
        ▼
state.db  kb_key_blocks.body_json
          + source anchors, kb_relationships, holders, kb_extract_jobs
```

| Layer | Location | Role today |
| --- | --- | --- |
| Protocol type | Spoke package `KnowledgeEntry` | Interchange; HTTP `/kb/` keeps that path stability ([spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md) §6). |
| Conversion seam | `crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs` | Sole `KnowledgeEntryRecord ↔` Spoke `KnowledgeEntry` seam; `extensions.nexus` extras ride through; lifecycle methods delegate via `KnowledgeEntryRecordSpokeExt`. |
| Domain aggregate | `nexus-knowledge` `KnowledgeEntryRecord` / `KnowledgeEntryBody` | World-scoped (and Actor-owned) narrative entries; block type, canonical name, status, revision, body, anchors, modules, provenance. |
| Store port + SQLite | `KbStore` / `SqliteKbStore` (`crates/nexus-local-db/src/kb_store.rs`) | Insert/get/update with OCC (`revision`); body serialized as `body_json`; list-by-world filters terminal statuses. |
| Extract / promote | `kb_extract_jobs` + [llm-extract.md](../../active/orchestration/llm-extract.md) + entity-scope §5.5 | Review / finalize / rescan propose candidates; author adopt/reject is the gate into confirmed KnowledgeEntries. |
| Relationships | `kb_relationships` + entity-scope §5.6 | Same World graph; suggestions use `needs_review`, not a second IR. |
| Holders / governance | [holder-governance.md](../../active/architecture/holder-governance.md) + holder tables | Authorization / custody. Not a knowledge content SSOT. |
| Operator / HTTP adapters | `nexus42 creator world kb …`; daemon `/kb/` and `/kb/key-blocks/` | Already meant to go through core / adapter, not a private file format. |
| MCA World KB slot | `nexus-moment-context-assembly` | Reads World KB into Moment. After #486, there is no User Knowledge slot. |

### 1.2 Pain points (why an Exploration is still warranted)

1. **Several leftover faces** (UserScope, references, the `creator kb` file index) sat beside this stack and invited "one more store" thinking. #486 removes them; this file must lock the door and MUST NOT reintroduce the file index as a staging adapter.
2. **Extract and some CLI/demo paths** can still look like direct `insert_knowledge_entry` calls. The target is that every durable write is visibly a port commit (production or stub), including extract-apply. Operator entrances formerly spelled `creator kb queue-extract` / `rescan` are rebound by #486 onto the World KB surface; they still commit through this port after the author gate.
3. **Dry-run / Simulate** of workflows that extract or preview KB changes must not write `kb_key_blocks`. That stub is not specified today as part of a shared knowledge port (Workflow IR #483 calls out the same class of risk for Work artifacts).

### 1.3 What does *not* need reinventing

- Promotion state machine (manual / pending / confirmed / rejected).
- Relationship taxonomy and OCC.
- Holder/governance rules.
- `/kb/` path stability and `extensions.nexus` field inventory.
- Memory ports.

This Exploration MAY later amend those Masters only to say "writes go through the knowledge port" and to drop User-tier language once #486's implementation has landed. It MUST NOT restated-own their state machines.

---

## 2. Target architecture

```text
                    ┌──────────────────────────┐
                    │  Spoke KnowledgeEntry    │
                    │  (IR / protocol SSOT)    │
                    └────────────┬─────────────┘
                                 │
              nexus-spoke-adapter conversion seam
                                 │
                    ┌────────────▼─────────────┐
                    │  KnowledgeEntryRecord    │
                    │  (in-process projection) │
                    └────────────┬─────────────┘
                                 │
                         KnowledgePort
                    (load / validate / commit / stub)
                                 │
        ┌──────────────┬─────────┴──────────┐
        ▼              ▼                    ▼
  SqliteKbStore   HTTP / CLI          extract-apply
  (durable        adapters            (commit after
   adapter)       (`/kb/`,            author gate)
                  `creator world kb`)
        │
        ▼
  kb_key_blocks + anchors / relationships / holders / jobs

Simulate / dry-run / extract-preview ──► stub adapter (no durable writes)
```

### 2.1 Normative target rules

1. **No parallel IR.** A conforming implementation MUST NOT introduce a second knowledge document type, JSON schema, or crate whose fields duplicate Spoke `KnowledgeEntry`. New local fields go through the existing `extensions.nexus` contract, or they do not exist.
2. **One write port.** Durable creates/updates/deletes of KnowledgeEntries (and their relationship rows when the same transaction applies) MUST go through a documented `KnowledgePort` (today: `KbStore`, possibly a thin façade that also exposes a stub). Callers MUST NOT open ad-hoc SQL against `kb_key_blocks` outside the adapter.
3. **Projection, not authority.** If `KnowledgeEntryRecord` and a Spoke `KnowledgeEntry` disagree after a round-trip through the conversion seam, the seam is buggy; do not "fix" it by treating the SQLite row as authoritative *over* the protocol. SQLite is how the IR persists locally. Reconcile / OCC remain as today's Masters specify (`revision`, terminal-status guards).
4. **Stub in non-apply paths.** Simulate, dry-run, extract *preview*, and any Workflow IR Simulate that would touch knowledge MUST bind a stub adapter. The stub MAY record intended mutations for inspection. It MUST NOT insert or update `kb_key_blocks` (or extract-job rows that auto-apply).
5. **Do not absorb other families.** The port MUST NOT grow User-knowledge APIs, reference-registry APIs, memory APIs, Work-tree file APIs, or a `creator kb` / `kb/index.json` import API.
6. **Holder/governance is not a second kitchen.** Authz checks stay on holder-governance / world-owner / source-work gates. The knowledge port calls them; it does not reimplement them.

### 2.2 What "IR + adapters" means here (vs #483 / #485)

#483's Workflow IR is a new strategy-graph SSOT that may replace YAML presets. #485's Work Artifact IR is a new SSOT for Work trees that today live as files + `work_chapters`.

Knowledge is different: **the SSOT already exists upstream (Spoke).** "Making a Knowledge IR" means **adopting that protocol as the only content SSOT and making every local I/O path an adapter**, not minting a third representation. Shared *practice* with #483/#485: ports, stubs, no silent direct writes. Separate *documents, types, and versions*.

---

## 3. Adapters

### 3.1 Persistence adapter — `SqliteKbStore`

Remains the local durable adapter. `body_json` is the serialized body of the projected record, not a second authoring format. Related tables (anchors, relationships, holders, extract jobs) stay beside it; they are not promoted into a new IR document.

A future alternate adapter (e.g. a test fixture store, or a remote Spoke-backed store) MUST implement the same port. It MUST NOT invent a different entry shape.

### 3.2 Conversion adapter — `nexus-spoke-adapter`

The existing conversion seam stays the **only** `KnowledgeEntryRecord ↔` Spoke `KnowledgeEntry` mapping. New fields MUST not sprout a second mapper in `nexus-core` or CLI. Round-trip tests in `nexus-spoke-adapter` remain the conformance evidence, together with Spoke version checks already required by the adapter Master.

### 3.3 Wire / operator adapters

HTTP `/kb/` and `/kb/key-blocks/`, plus `nexus42 creator world kb` (list / show / edit / adopt / reject / …), are adapters over the same port. Path stability stays with the Spoke adapter Master. This Exploration does not redesign the HTTP envelopes.

### 3.4 Extract-apply adapter

LLM / heuristic extract produces *candidates* (`kb_extract_jobs`, relationship `needs_review` rows). Confirmed KnowledgeEntries appear only after the existing author gate (or the already-specified auto-adopt paths). The apply step MUST be a port commit. Preview / Simulate MUST use the stub.

### 3.5 `creator kb` is not an adapter of this IR

#486 retires the work-scope file index and the `nexus42 creator kb` command group. This Exploration MUST NOT specify an import / staging adapter over `kb/index.json` + `kb/entries/<id>.md`, and MUST NOT keep that tree as a compatibility layer.

Retiring the code itself belongs to #486's implementation PR. This PR does not delete it. Until that implementation lands, leftover files MAY still exist on disk; Knowledge IR work MUST NOT grow new readers against them.

If authors later need workspace staging inputs, that is a **separate** Exploration and must be designed against this port (candidates still go through the extract/promote gate). World KB extract operator entrances rebound by #486 (`queue-extract` / `extract-status` / `rescan` onto the `creator world kb` family, or a dedicated leaf there) remain extract-apply callers of §3.4, not a revival of the file index.

### 3.6 Moment read adapter

MCA's World KB slot remains a read adapter into Moment. After #486 it MUST NOT call `KnowledgeStore` / User knowledge. Memory injection, if any, stays on `MemoryPort`.

---

## 4. Lifecycle (local)

Knowledge mutations are **not** a strategy Generate → Execute pipeline. The local lifecycle is:

```text
Load (protocol or store) → Validate → Preview/Dry-run (optional, stub only) → Author-confirm (when required) → Commit
```

- **Load** from the persistence adapter or from a Spoke-shaped payload.
- **Validate** with existing World KB validation (`ValidationMode`, name uniqueness, terminal-status guards, relationship endpoint rules).
- **Preview** emits a planned mutation list through the stub.
- **Commit** applies through `KbStore` under OCC and the existing authz gates.

Extract jobs and relationship suggestions are *inputs* to this lifecycle, not a parallel SSOT.

---

## 5. Migration plan

### 5.1 Phase plan (steady, gated, no big-bang rewrite)

| Phase | Intent | Gate |
| --- | --- | --- |
| **P0** | Name the existing conversion + `KbStore` as the knowledge port; add/adjust tests that prove round-trip and "no second mapper." Docs only + tests if needed. No behavior change. | Adapter conformance tests green; no new schema. |
| **P1** | Audit remaining direct writers; route them through the port façade. | `rg` inventory of `kb_key_blocks` writers outside `SqliteKbStore` is empty (tests/fixtures excepted or listed). |
| **P2** | Introduce the stub adapter; bind it in Simulate / extract-preview / Workflow Simulate paths that can touch knowledge. | A documented dry-run extracts or previews **without** inserting `kb_key_blocks`. |
| **P3** | After #486 implementation: drop any leftover compile-time references to `KnowledgeStore` / `reference_sources` / the `creator kb` file index in knowledge paths; amend Masters if promotion is in the same window. | UserScope / reference / work-scope index types are gone from the knowledge crate graph; no new `kb/index.json` reader. |

P0 MAY start after this Exploration is merged, even if the #486 *implementation* PR is still in flight, but P3 depends on that retirement landing. Prefer not to ship P1–P2 features that reintroduce retired stores or the old file index. The previous "P3: keep `creator kb` as an import adapter" default is withdrawn.

### 5.2 Must keep / must not break

- Must keep: OCC / `revision`, terminal-status behavior, adopt/reject gates, relationship semantics, holder/governance, `/kb/` paths, Spoke conversion extras.
- Must not break: World KB HTTP/CLI, extract jobs already in `pending`, MCA World KB reads, Actor knowledge ownership, memory ports.
- Must not: auto-migrate archived User knowledge, reference bodies, or `creator kb` markdown into `kb_key_blocks`.

### 5.3 Acceptance

- [ ] Spec and later code review can point to one port and one conversion seam, not a new IR schema.
- [ ] Simulate / preview cannot persist knowledge rows.
- [ ] No `creator kb` / `kb/index.json` import or staging adapter on the knowledge port. Retiring that surface itself belongs to #486; this PR does not delete the code.
- [ ] No UserScope or reference-registry APIs on the knowledge port.
- [ ] No absorption of Workflow IR or Work Artifact IR types.
- [ ] Existing World KB, extract, and memory tests still pass.

---

## 6. Safety invariants

1. This Exploration MUST NOT change production code by itself.
2. Spoke `KnowledgeEntry` remains the content SSOT; local SQLite is an adapter.
3. `spoke-memory` and Knowledge stay distinct families.
4. Holder/governance and promotion/relationship Masters stay authoritative for those concerns until explicitly amended.
5. Dry-run MUST stub the write port.
6. Archived leftovers from #486 MUST NOT be rehydrated as "helpful" KnowledgeEntries.
7. Cross-IR rule: Workflow Simulate that would write knowledge uses this stub; knowledge paths that would write Work trees use the #485 stub.

---

## 7. Open questions

1. **Façade vs status quo name.** Is `KbStore` already the port (document + stub implementation only), or should a thin `KnowledgePort` wrapper be introduced so Simulate can bind without touching SQLite types? Default: keep `KbStore`, add an explicit stub implementation.
2. **When P0 may start.** After this spec merges, or only after #486's implementation PR removes UserScope / references / the `creator kb` file index? Default: P0 docs/tests MAY start after merge; P3 and any crate-graph cleanup wait for #486 implementation.
3. **Relationships and extract jobs on the same port.** Should relationship writes and extract-job apply share the KnowledgePort surface, or remain sibling ports with the same stub policy? Default: sibling ports, same stub rule.
4. **`creator kb`.** The previous default ("keep as import/staging") is withdrawn. #486 retires the file index; this PR does not implement that deletion. A future staging face, if any, is a new Exploration — not an open question of keeping the old tree.
5. **Promotion of this file.** Fold "writes go through the port" into world-kb-runtime / spoke-adapter Masters, or keep a short Feature/Master after Draft? Decide at promotion; do not fork state machines into this document.

---

## 8. Non-goals

- Inventing a new Knowledge IR schema or document format beside Spoke `KnowledgeEntry`.
- Replacing or widening `spoke-memory`, including N4-T / N5.
- Re-introducing UserScope, reference sources, or the `creator kb` file index, or migrating their archives into World KB.
- Redesigning holder/governance, the promotion state machine, or relationship taxonomy.
- Changing Workflow IR (#483) or Work Artifact IR (#485) except the cross-stub rule.
- Retiring `creator kb` itself (that retirement belongs to #486). This PR does not implement that code deletion.
- Moving knowledge bodies onto the Work-tree filesystem as a new SSOT (the Work family is #485; knowledge persistence stays the store adapter).
- Implementing adapters or stubs in this PR (docs only).
- Cloud User/Pairing identity work (that would belong to a future User-scope Exploration, not here).

---

## 9. References

- Base retirement Exploration: [retire-user-scope-and-references.md](retire-user-scope-and-references.md) — [PR #486](https://github.com/42ch-dev/nexus/pull/486)
- [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md) (conversion seam, `extensions.nexus`, `/kb/` stability)
- [world-kb-runtime-architecture.md](../../active/architecture/world-kb-runtime-architecture.md)
- [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §5.1, §5.5–§5.6
- [holder-governance.md](../../active/architecture/holder-governance.md)
- [llm-extract.md](../../active/orchestration/llm-extract.md)
- [local-db-schema.md](../../active/runtime/local-db-schema.md)
- Spoke memory Explorations under `proposed/architecture/spoke-memory-*.md`
- Sibling IR Explorations: [PR #483](https://github.com/42ch-dev/nexus/pull/483) (Workflow IR), [PR #485](https://github.com/42ch-dev/nexus/pull/485) (Work Artifact IR)
- Implementation evidence: `crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs`, `crates/nexus-local-db/src/kb_store.rs`, `crates/nexus-knowledge` `KnowledgeEntryRecord`, `crates/nexus-moment-context-assembly` World KB slot
