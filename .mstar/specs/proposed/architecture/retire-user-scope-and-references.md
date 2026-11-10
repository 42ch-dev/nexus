---
module: nexus architecture / knowledge / runtime
date: 2026-11-10
status: proposed
problem_type: design
category: architecture
severity: info
tags:
  - retirement
  - user-scope
  - knowledge-store
  - reference-sources
  - creator-kb
  - moment-assembly
  - archive
---

# Retire UserScope, Reference Sources, and the creator kb File Index — Archive Scatter Surfaces

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | **Proposed** — not implement authority. Nothing here deletes production code, drops tables, or schedules crates. |
| **Document class** | Exploration |
| **Scope** | Detailed design to **retire and archive** three leftover scatter surfaces after the modular / ports-adapters / IR cut: (a) **User KnowledgeStore / UserScope** (`knowledge_entries`, `creator knowledge`, MCA `## User Knowledge` slot, Host / inspector user-knowledge wiring, demo-seed writes); (b) **reference sources** (SQLite `reference_sources` metadata + `references/units/<id>/body.md`); (c) **`creator kb` work-scope file index** (`kb/index.json` + `kb/entries/<id>.md`, the `nexus42 creator kb` CLI group, and HTTP `/v1/daemon/kb/entries`). |
| **Coordinates with** | [entity-scope-model.md](../../active/architecture/entity-scope-model.md) (User scope, two-tier knowledge, §5.3 work-scope index, §6.2–§6.3 transitions), [local-db-schema.md](../../active/runtime/local-db-schema.md), [reference-store-layout.md](../../active/runtime/reference-store-layout.md), [reference-knowledge.md](../../active/runtime/reference-knowledge.md), [cli-spec.md](../../active/cli/cli-spec.md), [world-kb-runtime-architecture.md](../../active/architecture/world-kb-runtime-architecture.md), [spoke-adapter-architecture.md](../../active/architecture/spoke-adapter-architecture.md), Spoke memory Explorations under `proposed/architecture/spoke-memory-*.md` |
| **Relationship to Knowledge IR** | A stacked sibling Exploration (`proposed/architecture/knowledge-ir.md`, opened on top of this branch) is the **implementation design** for Knowledge after these surfaces are cleared. **This document** is the base: it removes the leftovers so Knowledge IR can stay on Spoke `KnowledgeEntry` + World KB adapters only. The old `creator kb` file index MUST NOT be kept as a compatibility or staging layer inside that IR. The two MUST remain separate documents. Merge order is **this PR, then Knowledge IR**. |
| **Path choice** | Lands under `proposed/architecture/` because the change is a knowledge-family / entity-scope boundary cleanup that spans `nexus-knowledge`, MCA, CLI, and runtime storage. `runtime/` would be too narrow (UserScope is not just a table). There is no `knowledge/` class axis. |

The key words MUST, MUST NOT, SHOULD, and MAY are RFC 2119 terms for a conforming implementation of this proposal only (if adopted).

**Not implement authority.** Per [`.mstar/specs/AGENTS.md`](../../AGENTS.md), Exploration under `proposed/` never outranks active Masters (entity-scope, local-db schema, reference layout, CLI). Adoption requires a later implementation PR (and, at promotion, a Draft/Master plus link-migration sweep). **Merging this file into `proposed/` does not authorize deleting code.**

### 0.1 Why retire rather than adapt

After Workflow IR ([PR #483](https://github.com/42ch-dev/nexus/pull/483)), Work Artifact IR ([PR #485](https://github.com/42ch-dev/nexus/pull/485)), and the Spoke `MemoryPort` cutover (N1–N3 / N4-R on `main` via #465–#484), most durable knowledge-adjacent I/O has a clear owner. Three scatter surfaces remain:

1. **UserScope / User KnowledgeStore** — specified as a logged-in user's global, cross-Creator, tagged free-text index and as Moment's independent fourth domain. The product identity (`User` / `Pairing`) never landed. Writes are hard-coded to `user_default`; the production Host assembles with `with_user(creator_id)`. There is no HTTP/UI surface, no promotion into World KB, and no write path for `reference_uri`. The capability is an unfinished placeholder, not a shipped product.
2. **Reference sources** — a more complete local registry (CLI register/list/show, HTTP `GET /v1/daemon/references`, refresh capability + schedule, `body.md` on disk). It is still a leftover hybrid (workspace SQLite metadata + Creator-rooted files) that is not Spoke `KnowledgeEntry`, not World KB, and not Work-tree output. After the IR cut it does not have a home in the remaining knowledge family.
3. **`creator kb` work-scope file index** — a filesystem-core local index under the active workspace (`kb/index.json` + `kb/entries/<id>.md`), plus the `nexus42 creator kb` command group and HTTP `/v1/daemon/kb/entries`. [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §5.3–§5.4 already separates this from User knowledge and World KB. It is a leftover staging face, not the Knowledge IR. Keeping it would mean maintaining a second consumption / adapter path for a producer the current IR design does not need.

**Owner direction:** leftover surfaces MAY be retired and archived now, and added back later as **independent** proposals. They MUST NOT be force-fitted into `spoke-memory` or Spoke `KnowledgeEntry`.

**Owner ruling (2026-11-10):** the product has not formally launched and has no production consumers. Do not over-protect unused leftover faces. The latest IR design is the authority. Retaining the filesystem-core `creator kb` index only adds complexity: with no production producer, inventing a consumption or compatibility adapter is pointless. This is withdrawal of a placeholder / scatter surface, not a merge into memory or `KnowledgeEntry`. If workspace staging inputs are wanted later, that MUST be a new independent Exploration — the old index MUST NOT be kept as a compatibility layer inside Knowledge IR.

### 0.2 What this retirement is and is not

**This is:** a planned withdrawal of unfinished or leftover placeholders, plus an archive/disposal rule for any residual rows and files, so later Knowledge IR work is not built on a confused multi-store picture.

**This is not:**

- a merge of User knowledge, reference bodies, or `creator kb` markdown into Creator / Character memory (`spoke-memory` / `MemoryPort`);
- a projection of those leftovers into Spoke `KnowledgeEntry` / World KB;
- a cancellation of the *product idea* of "a human user's global notes," "source reference materials," or "workspace staging inputs" — those MAY return as future standalone Explorations (a real User identity, a deliberate new source-material port, or a new staging design). Re-introduction MUST NOT revive the withdrawn file index as a shim;
- a retirement of World KB, Actor knowledge, the World KB extract pipeline (`kb_extract_jobs`, `kb.extract_work`, `creator world kb` adopt/reject), or research-run artifact directories.

### 0.3 Recommended sequence — proposals then implementation

This section is **maintainer guidance**. It does not schedule work by itself.

#### Proposal merge order

| Order | Artifact | Why this order |
| --- | --- | --- |
| **1** | **This PR** (retire + archive design) | Clears the leftover surfaces and records data/consumer rules before Knowledge IR assumes a World-KB-only family. |
| **2** | Knowledge IR implementation Exploration (stacked PR; base = this branch) | Depends on (1)'s exclusions. Filling adapters and import seams *before* retirement is accepted risks dragging UserScope / references / the old `creator kb` index back into the IR. |
| **3** | Resolve §7 open questions that this file and Knowledge IR share | Archive vs wipe defaults; Moment slot deletion vs hard-disable; whether `knowledge_entries` / `reference_sources` are dropped or left dormant; where extract/rescan operator entrances land after the `creator kb` group is retired. |
| **4** | Implementation PR(s), then later Draft/Master promotion | Only a later, separately authorized PR deletes code. Promotion of any replacement Knowledge Master is also later. |

**Anti-pattern:** deleting UserScope, reference, or `creator kb` code inside this Exploration, or landing Knowledge IR adapters that still read `knowledge_entries` / `reference_sources` / `kb/index.json` "just in case."

#### Implementation land order (after this Exploration is merged **and** an implementation PR is authorized)

```text
This Exploration merged (direction; still not implement authority)
        │
        ▼
Knowledge IR Exploration merged (stacked design)
        │
        ▼
Implementation PR for this retirement (authorized separately)
  P0  inventory + archive export of residual rows/files
      (knowledge_entries, reference bodies, kb/index.json + kb/entries)
  P1  stop writes (CLI, demo seed, register, refresh schedule,
      creator kb add/remove, POST /v1/daemon/kb/entries)
  P2  remove or hard-disable MCA User Knowledge slot + Host/inspector wiring
  P3  retire HTTP GET references + /v1/daemon/kb/entries and generated
      clients; retire the creator kb command group after rebinding
      extract/rescan operator entrances onto the World KB surface
  P4  drop or seal tables; remove body trees and workspace kb/ indexes;
      amend active Masters in the same change as any schema/API drop
        │
        ▼
Only then implement Knowledge IR adapters against the cleaned family
```

---

## 1. Current inventory (evidence)

Evidence is from production sources under `crates/` and `apps/` on `origin/main` at `5f27b8bfa` (post #484). Representative symbols are named so a later implementer can re-verify; line numbers are orientation, not a frozen contract.

### 1.1 UserScope / User KnowledgeStore

Specified intent (still in active Masters):

- [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §1.1–§1.2 places a **User knowledge index** beside Creator under `User`. §5.2 calls `nexus-knowledge` a two-tier crate (World-scoped narrative KB + User-scoped global knowledge). §5.4 distinguishes **User knowledge** from **World KB** and from the CLI work-scope index. §6.2 allows MCA to pull User knowledge into a Moment without moving ownership; §6.3 requires an explicit promotion into World KB that keeps source provenance.
- [cli-spec.md](../../active/cli/cli-spec.md) lists `nexus42 creator knowledge ...` as the User-knowledge entrance and describes `platform context assemble-moment` as a **four-domain** Moment (narrative / World KB / User knowledge).
- [local-db-schema.md](../../active/runtime/local-db-schema.md) owns `knowledge_entries` as the User-scoped table for that Moment domain.

What actually shipped:

| Surface | Location | What it does today |
| --- | --- | --- |
| Trait + in-memory store | `nexus-knowledge::{KnowledgeStore, UserKnowledgeEntry, InMemoryKnowledgeStore}` | User-scoped tagged free text (`content`, `tags`, optional `reference_uri`). |
| SQLite adapter | `nexus-local-db::SqliteKnowledgeStore` | Full text + tags in workspace `state.db` table `knowledge_entries` (migration `20260526_knowledge_entries.sql`). Every SQL clause is `WHERE user_id = ?`. |
| Core write/read | `CoreService::{add,list,search}_user_knowledge` in `crates/nexus-core/src/knowledge.rs` | Writes and lists under the hard-coded `DEFAULT_USER_ID = "user_default"` ("until the platform `usr_*` mapping exists"). No HTTP wrapper. |
| CLI | `nexus42 creator knowledge add\|list\|search` (`apps/nexus42/src/commands/creator/knowledge.rs`) | Direct-core seam only. No `--user-id` (the typed seam has none). |
| MCA fourth domain | `nexus-moment-context-assembly` | Heading `## User Knowledge`; field `MomentContext.user_knowledge`; `fetch_user_knowledge` via `KnowledgeStore`; budget applied last after world_state / timeline / world_kb. Activation runs between World KB fetch and this slot. |
| Production Host | `crates/nexus-core/src/host.rs` | For a Creator actor, `MomentRequest::with_user(creator_id)` — **not** `user_default`. So Host assembly typically cannot see rows the CLI just wrote. |
| Inspector / CLI assemble | `apps/nexus42/src/commands/platform/context.rs` | `--user-id` defaults to `user_default`. Same mismatch if a caller passes a creator id. |
| Demo seed | `apps/nexus42/src/commands/creator/mod.rs` demo world seed | On `--force`, `DELETE FROM knowledge_entries WHERE user_id = 'user_default'`, then inserts a demo `UserKnowledgeEntry` under `user_default`. |

**Gaps (why this is a placeholder, not a product):**

1. No real User / Pairing identity in the local product. The store keys a `user_id` that the platform never issues.
2. Write path (`user_default`) and production Host read path (`creator_id`) disagree.
3. No HTTP or UI for User knowledge.
4. §6.3 promotion (User knowledge → World KB with provenance) has no implementation.
5. `reference_uri` exists on the row type but has no CLI/Core write entrance.
6. `spoke-memory` explicitly MUST NOT grow `KnowledgeEntry` fields or fake memory with knowledge rows (see the adopted memory Explorations). Memory N4-T (export/import/transfer) is not started and MUST NOT be used as a dump for this table.

The MCA slot, CLI, Host assembly, and demo seed still call this path, so it MUST NOT be deleted casually — consumers have to be rewired in the implementation PR.

### 1.2 Reference sources

Specified intent:

- [reference-store-layout.md](../../active/runtime/reference-store-layout.md) — two-layer model: `reference_sources` in workspace `state.db` is the registry; canonical body is `~/.nexus42/creators/<creator_id>/references/units/<reference_source_id>/body.md`. Path helpers live in `nexus-home-layout`.
- [reference-knowledge.md](../../active/runtime/reference-knowledge.md) — refresh policy, scheduler, and capability `nexus.reference.refresh`.
- [local-db-schema.md](../../active/runtime/local-db-schema.md) §4.1.1 — registry fields, including deprecated inline `content` (NULL for new rows).

What actually shipped (more complete than UserScope, still a leftover family):

| Surface | Location | What it does today |
| --- | --- | --- |
| Registry DAO | `crates/nexus-local-db/src/reference_source.rs` | CRUD + scan/hash fields on `reference_sources`. |
| Domain + Core | `nexus-knowledge` reference types; `CoreService::{register,list,get}_reference` | Register writes the row and `body.md`; list/get are read. |
| CLI | `nexus42 creator reference register\|list\|show` | Direct-core seam. There is deliberately **no** `reference refresh` leaf (retired V1.58 entrance). |
| HTTP | `GET /v1/daemon/references`, `GET /v1/daemon/references/{id}` | Read-only daemon-api + generated contracts. The web `browser-client` exposes `listReferences` / get, but no in-tree web page currently calls them. |
| Refresh | `nexus-orchestration` builtin `reference_refresh` + `crates/nexus-core/src/execution/schedules/refresh.rs` | May update `body.md` and `content_hash` for `refreshable` sources. |
| Schemas | `schemas/core/references/*` | Wire envelopes for the GET routes. |

References are **source material**, not narrative KnowledgeEntries. Nothing in this inventory promotes a reference body into World KB automatically. That is one reason they must not be silently folded into Knowledge IR.

### 1.3 In scope — `creator kb` work-scope file index

Specified intent (still in active Masters):

- [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §5.3–§5.4 already separates the CLI work-scope file index from User knowledge and World KB.
- [cli-spec.md](../../active/cli/cli-spec.md) says `nexus42 creator kb` MUST NOT create World KnowledgeEntries or User knowledge rows; extracted candidates enter World KB only through the existing extract/promote pipeline.

What actually shipped:

| Surface | Location | What it does today |
| --- | --- | --- |
| Workspace file index | `kb/index.json` + `kb/entries/<id>.md` under the active workspace (`nexus-home-layout::{creator_kb_dir, creator_kb_entries_dir}`) | Local markdown + JSON index. This is the filesystem-core face being retired. |
| Core file-index API | `CoreService::{list,add,get,delete}_kb_entry` in `crates/nexus-core/src/knowledge.rs` | Reads and writes the workspace index. Distinct from World KB `KbStore`. |
| CLI command group | `nexus42 creator kb` (`apps/nexus42/src/commands/creator/kb.rs`, `kb/rescan.rs`) | `list\|search\|show\|add\|remove` against the file index; `queue-extract\|extract-status` against `kb_extract_jobs`; `rescan` / `rescan --work` over chapter prose in `Works/<work_ref>/Stories/`. |
| HTTP | `GET\|POST /v1/daemon/kb/entries`, `GET\|DELETE /v1/daemon/kb/entries/{id}` | Work-scope file-index daemon-api (`apps/nexus-service/src/knowledge.ts`, `schemas/daemon-api/kb/*-kb-entry*.schema.json`). This is **not** World KB `/kb/` or `/kb/key-blocks/`, and **not** World pack `/v1/daemon/worlds/:world_id/kb/pack/*`. |
| Generated clients | web `browser-client` `listKbEntries` / add / get / delete; `packages/nexus-native` same wrappers | No in-tree web page currently calls them. |
| Operator extract entrances hanging off this group | `queue-extract` (historically keyed by a file-index `work_entry_id`, with `--chapter N` sugar that locates a Work chapter body); `extract-status`; `rescan` | These enqueue or refresh World KB extract work. `rescan` does **not** use the file index as SSOT — it reads chapter prose and confirmed `KnowledgeEntryRecord` rows. The **command spellings** retire with this group; the **World KB extract pipeline** stays and MUST be rebound (§2.4). |
| Active Masters | entity-scope §5.3–§5.4, cli-spec work-scope rows, [world-kb-runtime-architecture.md](../../active/architecture/world-kb-runtime-architecture.md) extract-status/rescan mentions, [workflow-profile.md](../../active/novel-writing/workflow-profile.md) persist stage, [work-experience-model.md](../../active/creator/work-experience-model.md) | Still describe `creator kb` as a live work-scope index and as the Work → World KB extract entrance. |

**Why this is a leftover, not a product face to keep:** the product has not formally launched; there is no production consumer of the file index. After the IR cut, narrative knowledge already has a home (Spoke `KnowledgeEntry` → `KnowledgeEntryRecord` → `KbStore`). Keeping a parallel filesystem index only adds a second authoring and consumption path. Extract/rescan *functions* belong to World KB and will be rebound; they are not a reason to keep the old index as a compatibility layer.

The CLI, HTTP routes, Core file-index methods, workflow persist wording, and extract/rescan spellings still call this path, so it MUST NOT be deleted casually — consumers have to be rewired in the implementation PR.

### 1.4 Do not confuse research-run artifact directories

Some orchestration / research presets write under a workspace path such as `.nexus42/references/<run_id>/`. That is a **run artifact directory** (research preset `report.md` / `artifacts/`, `auto_chain` `research_artifacts_dir`), not the `reference_sources` registry, not `references/units/<id>/body.md`, and not the `creator kb` file index. This retirement MUST NOT touch research-run output paths, quality-loop logs, or Work-tree files under `Works/`.

---

## 2. Target retirement design

### 2.1 Normative target rules

1. After the authorized implementation PR, **User KnowledgeStore / UserScope is gone from the runtime**: no new writes, no MCA fourth domain, no Host/inspector `KnowledgeStore` assembly, no demo-seed rows, no `creator knowledge` command.
2. After the same or a tightly following authorized PR, **the reference-source stack is gone from the runtime**: no register/list/show CLI, no HTTP GET references, no refresh capability admission or schedule, no new `body.md` units.
3. After the same or a tightly following authorized PR, **the `creator kb` work-scope file index is gone from the runtime**: no `kb/index.json` / `kb/entries/<id>.md` writes, no `nexus42 creator kb` command group, no HTTP `/v1/daemon/kb/entries`.
4. Residual data is **archived or explicitly discarded** (§3). Silent leftover tables or file trees that new code keeps reading are not an archive.
5. Demand MAY be re-proposed later as **independent** Explorations (`User-scope personal knowledge` once a real User / Pairing identity exists; `reference / source-material port` if the product wants a source registry again; a new workspace staging design if authors still need pre-extract inputs). Re-introduction MUST NOT revive the withdrawn tables or the old `kb/index.json` tree as a compatibility shim inside Knowledge IR or `spoke-memory`.
6. World KB, Actor knowledge, `spoke-memory`, Workflow IR, Work Artifact IR, and the World KB extract pipeline (`kb_extract_jobs`, `kb.extract_work`, `creator world kb` adopt/reject, World pack import/export) stay. Research-run `.nexus42/references/<run_id>/` stays.

### 2.2 UserScope — consumers to rewire

The implementation PR MUST, in one accountable change set (or a short sequenced stack with no window that still writes to a half-removed store):

| Consumer | Target change |
| --- | --- |
| MCA `## User Knowledge` slot, `user_knowledge` field, `fetch_user_knowledge`, budget tail, golden fixtures | **Delete** the slot from the assembled Moment (preferred) or hard-disable it so the heading and field can never populate. Do not leave an empty heading. Update goldens and dogfood packets in the same change. |
| `MomentRequest::with_user` / `--user-id` / `--knowledge-limit` | Remove or make them rejected flags (no silent no-op). |
| Production Host `with_user(creator_id)` + `SqliteKnowledgeStore` injection | Stop injecting a User knowledge store; assemble World / Actor / memory only. |
| Inspector / `platform context assemble-moment` | Drop User-knowledge reads; command help and cli-spec "four-domain" language become three narrative domains + Stage-0 (memory remains a separate family). |
| `nexus42 creator knowledge` | Retire the command group. |
| `CoreService::{add,list,search}_user_knowledge` and `DEFAULT_USER_ID` | Remove from the core public surface. |
| `KnowledgeStore` / `UserKnowledgeEntry` / `SqliteKnowledgeStore` / `InMemoryKnowledgeStore` | Remove once callers are gone. Do not keep a "maybe later" trait in the hot crate graph. |
| Demo seed knowledge step and the `DELETE FROM knowledge_entries WHERE user_id = 'user_default'` cleanup | Remove. Demo worlds still MAY seed a World KB `KnowledgeEntryRecord` (that is World KB, not UserScope). |
| Active Masters: entity-scope §1.1 tree "User knowledge index", §5.2 User-tier, §5.4 User-knowledge shorthand, §6.2–§6.3, cli-spec User-knowledge rows, local-db-schema `knowledge_entries` | Amend in the implementation / promotion change (link-migration sweep). Replace User-tier claims with a short "retired; future independent proposal" pointer, not with a memory or KnowledgeEntry rewrite. |

### 2.3 Reference sources — consumers to rewire

| Consumer | Target change |
| --- | --- |
| `nexus42 creator reference register\|list\|show` | Retire the command group. |
| `CoreService::{register,list,get}_reference` | Remove from the core public surface. |
| HTTP `GET /v1/daemon/references` and `GET /v1/daemon/references/{id}` | Retire routes, schemas, generated contracts, and the unused web client wrappers. Follow daemon-api / schema-drift rules so retirement is a deliberate contract break, not a silent 404. |
| `nexus.reference.refresh` capability + execution refresh schedule | Unadmit / retire. In-flight refresh jobs MUST be cancelled or failed closed before the table or body tree is removed. |
| DAO `reference_source.rs`, home-layout helpers *used only for this tree* | Remove once callers are gone. Shared `nexus-home-layout` helpers for other Creator paths stay. |
| Active Masters: reference-store-layout, reference-knowledge, local-db-schema §4.1.1, capability IDs | Archive or amend in the implementation / promotion change. The current Masters remain authority until that change. |

### 2.4 `creator kb` — consumers to rewire

| Consumer | Target change |
| --- | --- |
| `nexus42 creator kb list\|search\|show\|add\|remove` | Retire with the command group. |
| `nexus42 creator kb queue-extract\|extract-status` and `creator kb rescan` / `rescan --work` | Retire these **spellings** with the group. **Rebind** the operator entrances onto the remaining World KB surface (default: the `creator world kb` family, or a dedicated extract leaf in that family — see §7). Do not delete `kb_extract_jobs`, the `kb.extract_work` capability, or `creator world kb` adopt/reject. |
| `CoreService::{list,add,get,delete}_kb_entry` and the `kb/index.json` read/write helpers | Remove from the core public surface once callers are gone. `queue_kb_extract` / list/get extract-job methods stay or move with the rebound operator entrance; they MUST NOT keep a hard dependency on the file index. |
| HTTP `GET\|POST /v1/daemon/kb/entries` and `GET\|DELETE /v1/daemon/kb/entries/{id}` | Retire routes, the work-scope entry schemas, generated contracts, and the unused web / native client wrappers. Follow daemon-api / schema-drift rules so retirement is a deliberate contract break, not a silent 404. Do **not** retire World KB `/kb/` / `/kb/key-blocks/` or World pack `/v1/daemon/worlds/:world_id/kb/pack/*`. |
| Home-layout helpers `creator_kb_dir` / `creator_kb_entries_dir` | Remove once callers are gone. Shared workspace helpers stay. |
| Workflow persist stage and novel-writing / work-experience wording that maps Work → World KB to `creator kb queue-extract` | Point at the rebound World KB extract entrance. |
| Active Masters: entity-scope §5.3–§5.4 work-scope index, cli-spec `creator kb` rows, world-kb-runtime extract-status/rescan mentions, workflow-profile, work-experience-model, creator-workflow | Amend in the implementation / promotion change. Replace "live work-scope file index" claims with a short "retired; future staging would be a new Exploration" pointer, not with a silent import adapter inside Knowledge IR. |

### 2.5 How the demand comes back later (not in this implementation)

**User-scope personal knowledge (future Exploration, not this file's body):** a real User / Pairing identity; a dedicated User-scope port; a Moment injection rule that does not overload Creator memory or World KB; explicit promotion into World KB with provenance. Until that identity exists, do not re-key `user_default` as "the current Creator."

**Reference / source-material (future Exploration, not this file's body):** if the product still wants a registry of external files/URLs/PDFs, design it as a source-material adapter that can *feed* World KB extract, not as a third knowledge SSOT, and not by reviving `creator kb` staging.

**Workspace staging inputs (future Exploration, not this file's body):** if authors later need a pre-extract scratch area, design it against the Knowledge IR write port (candidates still go through the existing extract/promote gate). MUST NOT keep or revive `kb/index.json` + `kb/entries/<id>.md` as a compatibility layer inside Knowledge IR.

---

## 3. Data handling

Pre-1.0 local persistence already MAY be wiped rather than migrated ([reference-store-layout.md](../../active/runtime/reference-store-layout.md) §7). This Exploration still prefers an **archive-then-delete** default so residual author text is not silently dropped.

### 3.1 `knowledge_entries` / `user_default` rows

1. **Inventory.** Count rows by `user_id` (expect `user_default`; do not assume it is the only key — tests and manual DBs may have others).
2. **Archive (default).** Export remaining rows to a one-shot, non-runtime archive (for example a dated JSON/NDJSON dump under a maintainer-chosen local archive directory, *not* under `Works/`, *not* under `kb/`, *not* re-imported into World KB or memory). Record the export path and row count in the implementation PR description.
3. **Discard (explicit opt-in).** If the inventory is empty, or the maintainer records that the rows are disposable demo data, the export MAY be skipped. Demo-seed text is disposable by default; any non-demo content MUST be archived or explicitly waived.
4. **Remove.** `DROP TABLE knowledge_entries` (or a migration that leaves a sealed, unused table — see §7). After drop, no runtime path may recreate the table except a future, separately accepted proposal.

### 3.2 Reference registry and `body.md`

1. **Inventory.** List `reference_sources` rows and resolve `content_path` / home-layout body paths. Note orphan files under `references/units/` with no row, and rows whose `body.md` is missing.
2. **Archive (default).** Copy registry rows plus available `body.md` (and, if present, legacy inline `content`) into the same class of dated archive as §3.1. Do not copy them into World KB, the retiring `creator kb` tree, or `Works/`.
3. **In-flight refresh.** Cancel or fail-closed any `nexus.reference.refresh` / scheduled job before deleting bodies.
4. **Remove.** Delete archived unit directories; then drop or seal `reference_sources`. Shared Creator home directories that still have other purposes stay.

### 3.3 `creator kb` file index (`kb/index.json` + `kb/entries/<id>.md`)

1. **Inventory.** Scan active workspaces for `kb/index.json` and files under `kb/entries/`. Note orphans (markdown with no index row, or index rows whose file is missing).
2. **Archive (default).** Copy the index and available entry markdown into the same class of dated archive as §3.1. Do **not** copy them into World KB, `knowledge_entries`, `spoke-memory`, or `Works/`.
3. **Discard (explicit opt-in).** If the inventory is empty, or the maintainer records that the files are disposable fixtures / demo data, the export MAY be skipped. Any non-demo author content MUST be archived or explicitly waived.
4. **Remove.** After archive or waiver, delete the workspace `kb/` index trees. No runtime path may recreate `kb/index.json` except a future, separately accepted staging proposal.

### 3.4 What MUST NOT happen to the data

- MUST NOT upsert retired rows or `creator kb` markdown into `kb_key_blocks` / Spoke `KnowledgeEntry` as a "migration."
- MUST NOT insert them into `spoke-memory` stores, including any future N4-T transfer bundle.
- MUST NOT rewrite User knowledge or reference bodies as `creator kb` markdown (the index itself is being withdrawn).
- MUST NOT leave production code quietly reading the archive.

---

## 4. Boundaries

| Family | Relationship after retirement |
| --- | --- |
| **World KB** (`KnowledgeEntryRecord` → `KbStore` / `SqliteKbStore`, `kb_key_blocks.body_json`) | Unchanged. This is the knowledge family Knowledge IR will formalize. Extract/rescan operator *entrances* MAY move onto this family's CLI/HTTP, but the store and promotion gate stay. |
| **Actor knowledge** | Same World KB store, different owner. Unchanged. |
| **World KB extract pipeline** (`kb_extract_jobs`, `kb.extract_work`, `creator world kb` adopt/reject, World pack import/export) | Stays. Only the `creator kb` spellings and the file-index input are withdrawn. |
| **`spoke-memory` / `MemoryPort`** | Unchanged. Creator / Character memory is a different Spoke family. Retirement MUST NOT add User notes or file-index markdown to memory records. |
| **Workflow IR (#483)** | Unchanged. Strategy graphs only. Persist-stage wording that named `creator kb queue-extract` is rebound, not folded into Workflow IR. |
| **Work Artifact IR (#485)** | Unchanged. Work-tree artifacts only. Missing-KB *log files* under `Works/.../Logs/kb/` stay Work artifacts, not a revival of the file index. |
| **`creator kb` file index** | Retired (§1.3, §2.4). |
| **Research-run `.nexus42/references/<run_id>/`** | Stays (§1.4). |

---

## 5. Safety invariants

1. This Exploration MUST NOT delete production code, drop tables, or rewrite active Masters by itself.
2. An implementation PR MUST be separately authorized, MUST land consumer rewires before or in the same change as store / file-tree deletion, and MUST include the archive/waiver record from §3.
3. Moment assembly after retirement MUST still produce a valid context without a User Knowledge heading.
4. World KB extract, adopt/reject, holder/governance, and memory ports MUST keep passing their existing tests. Extract/rescan operator entrances MUST be rebound before the `creator kb` group disappears, so the pipeline is not left without a caller.
5. Schema/API retirement (HTTP references, HTTP `/v1/daemon/kb/entries`, generated contracts, CLI command-surface tests) MUST be explicit; no silent contract drift.
6. Simulate / dry-run paths MUST NOT be "fixed" by pointing them at these stores or at `kb/index.json`.
7. Knowledge IR MUST NOT keep the old file index as an import/staging compatibility adapter.

---

## 6. Acceptance (for a later implementation PR, not this file)

- [ ] `creator knowledge`, `creator reference`, and `creator kb` are gone from the shipped CLI surface contract.
- [ ] Assembled Moments have no `## User Knowledge` section; Host no longer injects `SqliteKnowledgeStore` / `with_user(creator_id)`.
- [ ] Demo seed no longer writes or deletes `knowledge_entries`.
- [ ] `GET /v1/daemon/references` routes and `nexus.reference.refresh` are retired; no new `references/units/<id>/body.md` is created.
- [ ] `/v1/daemon/kb/entries` routes are retired; no new `kb/index.json` or `kb/entries/<id>.md` is created.
- [ ] World KB extract operator entrances formerly spelled `creator kb queue-extract` / `extract-status` / `rescan` are rebound; `kb_extract_jobs`, `kb.extract_work`, and `creator world kb` adopt/reject still behave.
- [ ] Residual `user_default` / reference bodies / `creator kb` files are archived or explicitly waived; tables are dropped or sealed; workspace `kb/` index trees are removed.
- [ ] Active Masters listed in §2.2–§2.4 are amended with a link-migration sweep, or are scheduled in the immediate follow-up promotion PR — no dual-authority window that still describes a live User-knowledge fourth domain or a live `creator kb` file index.
- [ ] World KB, memory, Work trees, and research-run artifact dirs still behave.

---

## 7. Open questions

1. **Delete vs hard-disable the MCA slot.** Preferred: delete (cleaner Moment contract). Hard-disable is only a staging tactic if goldens must move in two PRs.
2. **`DROP TABLE` vs sealed unused table.** Prefer drop for `knowledge_entries` and `reference_sources` once archives exist. A sealed table is acceptable only as a short compatibility bridge inside the implementation stack, not as a permanent ghost.
3. **HTTP references and `/v1/daemon/kb/entries` contract break.** Retire in the same daemon-api version bump as other accepted breaks, or leave a documented 410/sunset for one release? Default: retire in the implementation PR with an explicit changelog (pre-1.0).
4. **Archive location and retention.** Maintainer-local dated dump vs in-repo `archived/` fixtures. Default: local dump, not committed author content.
5. **Shared `nexus-knowledge` crate shape.** After the User tier is removed, keep the crate World-KB-only (preferred) or split? Decide in the implementation PR; Knowledge IR assumes World-KB-only.
6. **Where extract/rescan operator entrances land.** After `creator kb` is retired, default is to move `queue-extract` / `extract-status` / `rescan` onto the `creator world kb` family (or a dedicated extract leaf in that family). Do not leave the pipeline without a first-party operator entrance, and do not keep `creator kb` as a hidden alias.

---

## 8. Non-goals

- Implementing the retirement in this PR (docs only).
- Designing the future User-scope port, the future source-material port, or a new workspace staging face in detail (names and pointers only).
- Retiring World KB, Actor knowledge, extract/promote jobs, holder/governance, `creator world kb`, World pack import/export, or `spoke-memory`.
- Changing Workflow IR (#483) or Work Artifact IR (#485).
- Using memory N4-T (export/import/transfer) as a migration vehicle.
- Touching research-run artifact directories, Work-tree logs, or cloud User/Pairing transport (`nexus-cloud-domain`) beyond acknowledging that a real User identity does not exist locally today.
- Inventing a parallel "Knowledge IR" document inside this file, or keeping the old file index as that IR's import adapter.

---

## 9. References

- [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §1, §5.2–§5.4, §6.2–§6.3
- [local-db-schema.md](../../active/runtime/local-db-schema.md) (`knowledge_entries`, `reference_sources`)
- [reference-store-layout.md](../../active/runtime/reference-store-layout.md), [reference-knowledge.md](../../active/runtime/reference-knowledge.md)
- [cli-spec.md](../../active/cli/cli-spec.md) (`creator knowledge`, `creator reference`, `creator kb`, assemble-moment)
- [world-kb-runtime-architecture.md](../../active/architecture/world-kb-runtime-architecture.md)
- Spoke memory Explorations: `proposed/architecture/spoke-memory-protocol.md`, `spoke-memory-adoption-plan.md` (N4-T / N5 still open; no KnowledgeEntry-in-memory)
- Sibling strategy / work Explorations: [PR #483](https://github.com/42ch-dev/nexus/pull/483), [PR #485](https://github.com/42ch-dev/nexus/pull/485)
- Stacked follow-up: `proposed/architecture/knowledge-ir.md` (lands in the child PR; not present on this branch alone)
- Implementation evidence: `crates/nexus-local-db/src/knowledge_store.rs`, `crates/nexus-core/src/knowledge.rs` (`DEFAULT_USER_ID`, work-scope file index, `queue_kb_extract`), `crates/nexus-core/src/host.rs` (`with_user(creator_id)`), `crates/nexus-moment-context-assembly/src/moment.rs`, `apps/nexus42/src/commands/creator/knowledge.rs`, `apps/nexus42/src/commands/creator/reference.rs`, `apps/nexus42/src/commands/creator/kb.rs`, `apps/nexus42/src/commands/creator/mod.rs` (demo seed), `apps/nexus-service/src/knowledge.ts`, `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`
