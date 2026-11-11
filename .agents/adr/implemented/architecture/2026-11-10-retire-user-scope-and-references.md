# ADR: Retire UserScope, Reference Sources, and the creator kb File Index — Archive Scatter Surfaces

Status: implemented — all three scatter surfaces are gone from the CLI parser, the daemon-api contract, the runtime stores and the workspace, leaving World KB as the only knowledge family
Date: 2026-11-10
Scope: Retirement and archive of three surfaces left over from the modular / ports-adapters / IR cut: (a) **User KnowledgeStore / UserScope** (`knowledge_entries`, `creator knowledge`, MCA `## User Knowledge` slot, Host / inspector user-knowledge wiring, demo-seed writes); (b) **reference sources** (SQLite `reference_sources` metadata + `references/units/<id>/body.md`); (c) **`creator kb` work-scope file index** (`kb/index.json` + `kb/entries/<id>.md`, the `nexus42 creator kb` CLI group, and HTTP `/v1/daemon/kb/entries`).
Source: migrated from the retired tracked specs corpus

## Problem

UserScope/User KnowledgeStore, reference sources, and the `creator kb` work-scope file index were three parallel knowledge surfaces that duplicated World-KB ownership and kept user-tier tables, reference bodies, and a JSON file index alive after the ports/IR cut. Leaving them in place invited a fourth store and compatibility shims inside Knowledge IR; they had to be retired before the IR could assume a World-KB-only family.

After Workflow IR ([PR #483](https://github.com/42ch-dev/nexus/pull/483)), Work Artifact IR ([PR #485](https://github.com/42ch-dev/nexus/pull/485)), and the Spoke `MemoryPort` cutover (N1–N3 / N4-R on `main` via #465–#484), most durable knowledge-adjacent I/O had a clear owner. Three scatter surfaces remained:

1. **UserScope / User KnowledgeStore** — specified as a logged-in user's global, cross-Creator, tagged free-text index and as Moment's independent fourth domain. The product identity (`User` / `Pairing`) never landed. Writes were hard-coded to `user_default`; the production Host assembled with `with_user(creator_id)`. There was no HTTP/UI surface, no promotion into World KB, and no write path for `reference_uri`. The capability was an unfinished placeholder, not a shipped product.
2. **Reference sources** — a more complete local registry (CLI register/list/show, HTTP `GET /v1/daemon/references`, refresh capability + schedule, `body.md` on disk). It was still a leftover hybrid (workspace SQLite metadata + Creator-rooted files) that was not Spoke `KnowledgeEntry`, not World KB, and not Work-tree output. After the IR cut it had no home in the remaining knowledge family.
3. **`creator kb` work-scope file index** — a filesystem-core local index under the active workspace (`kb/index.json` + `kb/entries/<id>.md`), plus the `nexus42 creator kb` command group and HTTP `/v1/daemon/kb/entries`. [entity-scope-model.md](../../../../docs/specs/architecture/entity-scope-model.md) §5.3–§5.4 already separated Work knowledge from User knowledge and World KB. It was a leftover staging face, not the Knowledge IR; keeping it meant maintaining a second consumption / adapter path for a producer the IR design did not need.

Every surface had live consumers — MCA, Host assembly, the CLI groups, the HTTP routes, the demo seed — so the retirement was a rewiring job, not a struct deletion.

## Decision

All three surfaces are retired. Nothing in the runtime writes, reads, serves or schedules them: the parser refuses the `creator knowledge`, `creator reference` and `creator kb` command groups; the `GET /v1/daemon/references[/{id}]` and `/v1/daemon/kb/entries` daemon routes are absent from the contract rather than stubbed; `nexus-moment-context-assembly` assembles Moments with no `## User Knowledge` heading; and the production Host injects no User knowledge store. Migration `20261111000001_retire_user_and_reference_stores.sql` drops `knowledge_entries` and `reference_sources` rather than sealing them.

World KB is the only knowledge family left. Its store (`KnowledgeEntryRecord` → `KbStore` / `SqliteKbStore`), the World KB extract pipeline (`kb_extract_jobs`, `kb.extract_work`), the `creator world kb` adopt/reject gate, holder/governance and World pack import/export are untouched. Research-run artifact directories keep writing where they always wrote. Anything the retirement removed returns only as a new independent proposal — never as a compatibility shim inside Knowledge IR or `spoke-memory`.

### What was retired

#### UserScope / User KnowledgeStore

The Masters at the time placed a **User knowledge index** beside Creator under `User` ([entity-scope-model.md](../../../../docs/specs/architecture/entity-scope-model.md) §1.1–§1.2, §5.2, §5.4, §6.2–§6.3), named `nexus42 creator knowledge ...` as its CLI entrance and described `platform context assemble-moment` as a **four-domain** Moment ([cli-spec.md](../../../../docs/specs/cli/cli-spec.md)), and owned `knowledge_entries` as the User-scoped table ([local-db-schema.md](../../../../docs/specs/runtime/local-db-schema.md) §4.1.3). What actually shipped:

| Surface | Location | What it did |
| --- | --- | --- |
| Trait + in-memory store | `nexus-knowledge::{KnowledgeStore, UserKnowledgeEntry, InMemoryKnowledgeStore}` | User-scoped tagged free text (`content`, `tags`, optional `reference_uri`). |
| SQLite adapter | `nexus-local-db::SqliteKnowledgeStore` | Full text + tags in workspace `state.db` table `knowledge_entries` (migration `20260526_knowledge_entries.sql`). Every SQL clause was `WHERE user_id = ?`. |
| Core write/read | `CoreService::{add,list,search}_user_knowledge` in `crates/nexus-core/src/knowledge.rs` | Wrote and listed under the hard-coded `DEFAULT_USER_ID = "user_default"` ("until the platform `usr_*` mapping exists"). No HTTP wrapper. |
| CLI | `nexus42 creator knowledge add\|list\|search` (`apps/nexus42/src/commands/creator/knowledge.rs`) | Direct-core seam only. No `--user-id`; the typed seam had none. |
| MCA fourth domain | `nexus-moment-context-assembly` | Heading `## User Knowledge`; field `MomentContext.user_knowledge`; `fetch_user_knowledge` via `KnowledgeStore`; budget applied last after world_state / timeline / world_kb. Activation ran between World KB fetch and this slot. |
| Production Host | `crates/nexus-core/src/host.rs` | For a Creator actor, `MomentRequest::with_user(creator_id)` — **not** `user_default`, so Host assembly typically could not see rows the CLI had just written. |
| Inspector / CLI assemble | `apps/nexus42/src/commands/platform/context.rs` | `--user-id` defaulted to `user_default`. Same mismatch if a caller passed a creator id. |
| HTTP inspector / Moment assembly | `crates/nexus-core/src/context.rs` (`CoreService::inspect_moment`), reached from `crates/nexus-core-node/src/actors.rs` → `apps/nexus-service/src/context.ts` (`POST /v1/daemon/inspector/moment`) | Assembled the Moment through `.with_user(creator_id)` with a `SqliteKnowledgeStore::new(pool)`. This was the HTTP entry path for Moment assembly, but the response body (`MomentInspectResponse`) carries only `modules` / `slot_map` / `budget` / `moment_directive` / `hygiene` — never the User Knowledge content itself. |
| Demo seed | `apps/nexus42/src/commands/creator/mod.rs` demo world seed | On `--force`, `DELETE FROM knowledge_entries WHERE user_id = 'user_default'`, then inserted a demo `UserKnowledgeEntry` under `user_default`. |

Why it was a placeholder rather than a product:

1. No real User / Pairing identity in the local product. The store keyed a `user_id` that the platform never issued.
2. The write path (`user_default`) and the production Host read path (`creator_id`) disagreed.
3. No HTTP or UI **write/management** surface for User knowledge. The daemon inspector's Moment assembly (`POST /v1/daemon/inspector/moment`) did read the store, but the User Knowledge content never entered the response body.
4. The specified promotion path (entity-scope §6.3, User knowledge → World KB with provenance) had no implementation.
5. `reference_uri` existed on the row type but had no CLI/Core write entrance.
6. `spoke-memory` explicitly MUST NOT grow `KnowledgeEntry` fields or fake memory with knowledge rows; memory N4-T (export/import/transfer) is not started and is not a dump for this table.

#### Reference sources

Masters at the time: [reference-store-layout.md](../../../../docs/specs/runtime/reference-store-layout.md) described a two-layer model in which `reference_sources` in workspace `state.db` was the registry and the canonical body was `~/.nexus42/creators/<creator_id>/references/units/<reference_source_id>/body.md`, with path helpers in `nexus-home-layout`; [reference-knowledge.md](../../../../docs/specs/runtime/reference-knowledge.md) owned the refresh policy, scheduler and the `nexus.reference.refresh` capability; [local-db-schema.md](../../../../docs/specs/runtime/local-db-schema.md) §4.1.1 owned the registry fields, including the deprecated inline `content` (NULL for new rows). What actually shipped — more complete than UserScope, still a leftover family:

| Surface | Location | What it did |
| --- | --- | --- |
| Registry DAO | `crates/nexus-local-db/src/reference_source.rs` | CRUD + scan/hash fields on `reference_sources`. |
| Domain + Core | `nexus-knowledge` reference types; `CoreService::{register,list,get}_reference` | Register wrote the row and `body.md`; list/get read. |
| CLI | `nexus42 creator reference register\|list\|show` | Direct-core seam. Deliberately **no** `reference refresh` leaf (retired V1.58 entrance). |
| HTTP | `GET /v1/daemon/references`, `GET /v1/daemon/references/{id}` | Read-only daemon-api + generated contracts. The web `browser-client` exposed `listReferences` / get, but no in-tree web page called them. |
| Refresh | `nexus-orchestration` builtin `reference_refresh` + `crates/nexus-core/src/execution/schedules/refresh.rs` | Could update `body.md` and `content_hash` for `refreshable` sources. |
| Schemas | `schemas/core/references/*` | Wire envelopes for the GET routes. |

References were **source material**, not narrative KnowledgeEntries. Nothing in the inventory promoted a reference body into World KB automatically, which is one reason they were not folded into Knowledge IR.

#### creator kb work-scope file index

Masters at the time: [entity-scope-model.md](../../../../docs/specs/architecture/entity-scope-model.md) §5.3–§5.4 already separated the CLI work-scope file index from User knowledge and World KB, and [cli-spec.md](../../../../docs/specs/cli/cli-spec.md) said `nexus42 creator kb` MUST NOT create World KnowledgeEntries or User knowledge rows — extracted candidates entered World KB only through the existing extract/promote pipeline. What actually shipped:

| Surface | Location | What it did |
| --- | --- | --- |
| Workspace file index | `kb/index.json` + `kb/entries/<id>.md` under the active workspace (`nexus-home-layout::{creator_kb_dir, creator_kb_entries_dir}`) | Local markdown + JSON index. This was the filesystem-core face that was retired. |
| Core file-index API | `CoreService::{list,add,get,delete}_kb_entry` in `crates/nexus-core/src/knowledge.rs` | Read and wrote the workspace index. Distinct from World KB `KbStore`. |
| CLI command group | `nexus42 creator kb` (`apps/nexus42/src/commands/creator/kb.rs`, `kb/rescan.rs`) | `list\|search\|show\|add\|remove` against the file index; `queue-extract\|extract-status` against `kb_extract_jobs`; `rescan` / `rescan --work` over chapter prose in `Works/<work_ref>/Stories/`. |
| HTTP | `GET\|POST /v1/daemon/kb/entries`, `GET\|DELETE /v1/daemon/kb/entries/{id}` | Work-scope file-index daemon-api (`apps/nexus-service/src/knowledge.ts`, `schemas/daemon-api/kb/*-kb-entry*.schema.json`). Not World KB `/kb/` or `/kb/key-blocks/`, and not World pack `/v1/daemon/worlds/:world_id/kb/pack/*`. |
| Generated clients | web `browser-client` `listKbEntries` / add / get / delete; `packages/nexus-native` same wrappers | No in-tree web page called them. |
| Operator extract entrances hanging off this group | `queue-extract` (historically keyed by a file-index `work_entry_id`, with `--chapter N` sugar that located a Work chapter body); `extract-status`; `rescan` | These enqueued or refreshed World KB extract work. `rescan` did **not** use the file index as SSOT — it read chapter prose and confirmed `KnowledgeEntryRecord` rows. The **command spellings** retired with this group; the **World KB extract pipeline** stayed and was rebound. |
| Masters at the time | entity-scope §5.3–§5.4, cli-spec work-scope rows, [world-kb-runtime-architecture.md](../../../../docs/specs/architecture/world-kb-runtime-architecture.md) extract-status/rescan mentions, [spoke-adapter-architecture.md](../../../../docs/specs/architecture/spoke-adapter-architecture.md) §6.1 route table, [workflow-profile.md](../../../../docs/specs/novel-writing/workflow-profile.md) persist stage, [work-experience-model.md](../../../../docs/specs/creator/work-experience-model.md), [creator-workflow.md](../../../../docs/specs/creator/creator-workflow.md) Work-vs-work-scope row | Described `creator kb` as a live work-scope index and as the Work → World KB extract entrance. |

The file index was a leftover rather than a product face: no production consumer existed, and narrative knowledge already had a home (Spoke `KnowledgeEntry` → `KnowledgeEntryRecord` → `KbStore`). Keeping a parallel filesystem index added a second authoring and consumption path. Extract and rescan belonged to World KB and were rebound; they were not a reason to keep the old index as a compatibility layer.

#### Research-run artifact directories

Some orchestration / research presets write under a workspace path such as `.nexus42/references/<run_id>/`. That is a **run artifact directory** (research preset `report.md` / `artifacts/`, `auto_chain` `research_artifacts_dir`), not the `reference_sources` registry, not `references/units/<id>/body.md`, and not the `creator kb` file index. The retirement did not touch research-run output paths, quality-loop logs, or Work-tree files under `Works/`.

### Data handling

Pre-1.0 local persistence MAY be wiped rather than migrated ([overview.md — Pre-1.0 persistence stance](../../../../docs/specs/architecture/overview.md#pre-10-persistence-stance)). This retirement nevertheless used an **archive-then-delete** default so residual author text was not silently dropped, and each export recorded its path and row/file count in the implementing change.

#### knowledge_entries / user_default rows

1. **Inventory.** Counted rows by `user_id` (expecting `user_default`, but not assuming it was the only key — tests and manual DBs may have others).
2. **Archive (default).** Remaining rows were exported to a one-shot, non-runtime archive (for example a dated JSON/NDJSON dump under a maintainer-chosen local archive directory, not under `Works/`, not under `kb/`, and never re-imported into World KB or memory).
3. **Discard (explicit opt-in).** If the inventory was empty, or the maintainer recorded that the rows were disposable demo data, the export was skipped. Demo-seed text is disposable by default; any non-demo content had to be archived or explicitly waived.
4. **Remove.** After archive or waiver, `DROP TABLE knowledge_entries`. No runtime path recreates the table except a future, separately accepted proposal. A sealed unused table is not an acceptable long-term or default path.

#### Reference registry and body.md

1. **Inventory.** `reference_sources` rows were listed and `content_path` / home-layout body paths resolved; orphan files under `references/units/` with no row, and rows whose `body.md` was missing, were noted.
2. **Archive (default).** Registry rows plus available `body.md` (and, where present, legacy inline `content`) were copied into the same class of dated archive. They were not copied into World KB, the retired `creator kb` tree, or `Works/`.
3. **In-flight refresh.** Any `nexus.reference.refresh` / scheduled job was cancelled or failed closed before bodies were deleted.
4. **Remove.** Archived unit directories were deleted, then `DROP TABLE reference_sources`. Shared Creator home directories that still have other purposes stayed.

#### creator kb file index

1. **Inventory.** Active workspaces were scanned for `kb/index.json` and files under `kb/entries/`; orphans (markdown with no index row, or index rows whose file was missing) were noted.
2. **Archive (default).** The index and available entry markdown were copied into the same class of dated archive. They were not copied into World KB, `knowledge_entries`, `spoke-memory`, or `Works/`.
3. **Discard (explicit opt-in).** If the inventory was empty, or the maintainer recorded that the files were disposable fixtures / demo data, the export was skipped. Any non-demo author content had to be archived or explicitly waived.
4. **Remove.** After archive or waiver, the workspace `kb/` index trees were deleted. No runtime path recreates `kb/index.json` except a future, separately accepted staging proposal.

#### What did not happen to the data

- Retired rows and `creator kb` markdown were not upserted into `kb_key_blocks` or Spoke `KnowledgeEntry` as a "migration".
- Nothing was inserted into `spoke-memory` stores, including any future N4-T transfer bundle.
- User knowledge and reference bodies were not rewritten as `creator kb` markdown; the index itself was withdrawn.
- No production code reads the archive.

### Boundaries

| Family | Relationship after retirement |
| --- | --- |
| **World KB** (`KnowledgeEntryRecord` → `KbStore` / `SqliteKbStore`, `kb_key_blocks.body_json`) | Unchanged. This is the knowledge family Knowledge IR formalizes. Extract/rescan operator entrances may move onto this family's CLI, but the store and promotion gate stay. |
| **Actor knowledge** | Same World KB store, different owner. Unchanged. |
| **World KB extract pipeline** (`kb_extract_jobs`, `kb.extract_work`, `creator world kb` adopt/reject, World pack import/export) | Stays. Only the `creator kb` spellings and the file-index input were withdrawn. |
| **`spoke-memory` / `MemoryPort`** | Unchanged. Creator / Character memory is a different Spoke family. No User notes or file-index markdown entered memory records. |
| **Workflow IR (#483)** | Unchanged. Strategy graphs only. Persist-stage wording that named `creator kb queue-extract` was rebound, not folded into Workflow IR. |
| **Work Artifact IR (#485)** | Unchanged. Work-tree artifacts only. Missing-KB *log files* under `Works/.../Logs/kb/` stay Work artifacts, not a revival of the file index. |
| **`creator kb` file index** | Retired. |
| **Research-run `.nexus42/references/<run_id>/`** | Stays. |

### Owner direction and rulings

**Owner direction:** leftover surfaces MAY be retired and archived now, and added back later as **independent** proposals. They MUST NOT be force-fitted into `spoke-memory` or Spoke `KnowledgeEntry`.

**Owner ruling (2026-11-10):** the product has not formally launched and has no production consumers. Do not over-protect unused leftover faces. The latest IR design is the authority. Retaining the filesystem-core `creator kb` index only adds complexity: with no production producer, inventing a consumption or compatibility adapter is pointless. This is withdrawal of a placeholder / scatter surface, not a merge into memory or `KnowledgeEntry`. If workspace staging inputs are wanted later, that MUST be a new independent Exploration — the old index MUST NOT be kept as a compatibility layer inside Knowledge IR.

**Owner rulings (2026-11-10, open questions settled — now normative for a later implementation PR):**

1. **MCA User Knowledge slot: delete.** The slot MUST be deleted, not hard-disabled. Do not leave an empty heading. Golden fixtures and dogfood packets MUST be updated in the same implementation change.
2. **Tables: DROP.** After archive (or an explicit waiver), the implementation PR MUST `DROP TABLE knowledge_entries` and `DROP TABLE reference_sources`. A sealed unused table is **not** an acceptable long-term or default path, and MUST NOT be left as a permanent ghost.
3. **HTTP contract: simplify and modularize, then add later if needed.** User-level knowledge and the retiring file-kb / references surfaces are removed from the daemon-api contract in the implementation PR, as part of a pre-1.0 API simplification / modular refactor, with an explicit changelog. MUST NOT leave a 410/sunset compatibility layer. If demand returns later, add a **new** contract — do not revive the old routes. User knowledge has no HTTP surface today; the retiring HTTP faces this rule covers are `GET /v1/daemon/references` (and the by-id route) and `/v1/daemon/kb/entries`.
4. **Non-Spoke, file-based `creator kb`: full retirement.** Already in this proposal's scope; this is now a settled ruling, not an open question.

## Alternatives considered

Four directions were genuinely weighed. Each was available at the point of decision and each lost on a stated reason, not on hindsight.

1. **Keep the `creator kb` file index as an import / staging adapter inside Knowledge IR.** Rejected. The index is a second authoring and consumption path for narrative knowledge that already has a home (Spoke `KnowledgeEntry` → `KnowledgeEntryRecord` → `KbStore`). An adapter would keep `kb/index.json` readers alive inside the IR for a producer that does not exist, so it would add permanent surface to protect zero data. Workspace staging, if authors want it later, is designed against the IR write port and still passes the extract/promote gate — that is a new, independent record, not this index wearing a new name.

2. **Merge the leftover data into `spoke-memory` or Spoke `KnowledgeEntry`.** Rejected. `spoke-memory` needs candidates, grants, fencing and receipts that knowledge rows cannot satisfy, and its protocol MUST NOT grow `KnowledgeEntry` fields or simulate memory with knowledge rows; memory N4-T is not a migration vehicle. Spoke `KnowledgeEntry` is a narrative KB envelope with `canonical_name` and ownership-disclosure semantics that User notes, reference bodies and file-index markdown do not fit. Merging would also convert a withdrawal into a permanent projection decision made for the wrong reason, and would put retired rows into the store the IR is meant to keep clean.

3. **Seal the tables instead of `DROP`.** Rejected. A sealed unused table is a permanent ghost: it stays in every workspace `state.db`, it keeps a reader one commit away, and it makes "recreate it later" look free. The retirement removes placeholders with no production consumers, and pre-1.0 local persistence MAY be wiped rather than migrated — so the archive step, not the table, is what preserves residual text. Dropping is also what makes "no runtime path reads it" checkable at all; a seal is only a promise.

4. **Keep a 410/sunset compatibility layer for the retired HTTP routes.** Rejected. `GET /v1/daemon/references[/{id}]` and `/v1/daemon/kb/entries` had no in-tree web page calling them, so a deprecation window would protect no client while freezing the wrong contract shape past 1.0. A silent 404 was refused for the same reason a silent success would be: the removal is recorded as an explicit pre-1.0 contract simplification with a changelog, and a returning capability gets a new contract rather than a revived route.

## Consequences

### Accepted behavior

- `creator knowledge`, `creator reference`, and `creator kb` are gone from the shipped CLI surface contract; the parser refuses the groups rather than resolving them to a no-op.
- Assembled Moments carry no `## User Knowledge` section — the slot was deleted, not hard-disabled — and goldens and dogfood packets were updated in the same change; the Host no longer injects `SqliteKnowledgeStore` / `with_user(creator_id)`.
- The demo seed neither writes nor deletes `knowledge_entries`. Demo worlds may still seed a World KB `KnowledgeEntryRecord`, which is World KB, not UserScope.
- `GET /v1/daemon/references` routes and `nexus.reference.refresh` are removed from the contract with a changelog and no 410/sunset layer; no new `references/units/<id>/body.md` is created.
- `/v1/daemon/kb/entries` routes are removed from the contract with a changelog and no 410/sunset layer; no new `kb/index.json` or `kb/entries/<id>.md` is created.
- World KB extract operator entrances formerly spelled `creator kb queue-extract` / `extract-status` / `rescan` are rebound onto the `creator world kb` family; `kb_extract_jobs`, `kb.extract_work`, and `creator world kb` adopt/reject still behave.
- Residual `user_default` rows, reference bodies and `creator kb` files are archived or explicitly waived; `knowledge_entries` and `reference_sources` are DROPped, not sealed; workspace `kb/` index trees are removed.
- The Masters named in the retired-surface sections are amended with a link-migration sweep — no dual-authority window still describes a live User-knowledge fourth domain or a live `creator kb` file index.
- World KB, memory, Work trees, and research-run artifact directories still behave. Moment assembly produces a valid context without a User Knowledge heading, and the World KB extract pipeline is never left without a caller.

### Costs and how they are contained

- **A renamed operator surface costs muscle memory.** `queue-extract` / `extract-status` / `rescan` moved onto `creator world kb` rather than disappearing, so the pipeline keeps a first-party entrance with no hidden alias.
- **Removal is not reversible for the data.** A DROP destroys residual rows and deleting a workspace `kb/` tree destroys markdown that was never in Git. The archive step runs before removal, and an empty or demo-only inventory may be waived explicitly — the waiver is recorded rather than assumed.
- **No deprecation window means a hard cut for any unlisted client.** The routes had no in-tree caller, and the change ships with a changelog entry naming the removed identities, so the break is discoverable rather than silent.
- **Extract/rescan had to land before the group disappeared.** The rebinding shipped in the same change set, which is why `kb_extract_jobs`, `kb.extract_work` and `creator world kb` adopt/reject are untouched.
- **`nexus-knowledge` lost its User tier and stayed whole.** The crate is World-KB-only, which is the shape Knowledge IR assumes; splitting it further was not attempted, so no second crate boundary has to be argued for now.

### Residual questions and their recorded mitigations

1. **Archive location and retention** — resolved as a maintainer-local dated dump, not committed author content. The implementing change records the path and counts.
2. **Shared `nexus-knowledge` crate shape** — resolved: the crate is World-KB-only after the User tier is removed, and it was not split. Knowledge IR assumes that single family.
3. **Where extract/rescan operator entrances land** — resolved: `queue-extract` / `extract-status` / `rescan` were rebound onto the `creator world kb` family (or a dedicated extract leaf there). The pipeline keeps a first-party entrance, and `creator kb` was not kept as a hidden alias.

### Future demand returns as a new record

**User-scope personal knowledge:** a real User / Pairing identity; a dedicated User-scope port; a Moment injection rule that does not overload Creator memory or World KB; explicit promotion into World KB with provenance. Until that identity exists, `user_default` is not re-keyed as "the current Creator."

**Reference / source-material:** if the product still wants a registry of external files/URLs/PDFs, it is a source-material adapter that can *feed* World KB extract — not a third knowledge SSOT, and not a revival of `creator kb` staging.

**Workspace staging inputs:** if authors later need a pre-extract scratch area, it is designed against the Knowledge IR write port, and candidates still go through the existing extract/promote gate. The retired `kb/index.json` + `kb/entries/<id>.md` tree is not a compatibility layer.

## References

- [entity-scope-model.md](../../../../docs/specs/architecture/entity-scope-model.md) §1, §5.2–§5.4, §6.2–§6.3
- [local-db-schema.md](../../../../docs/specs/runtime/local-db-schema.md) (`knowledge_entries`, `reference_sources`)
- [reference-store-layout.md](../../../../docs/specs/runtime/reference-store-layout.md), [reference-knowledge.md](../../../../docs/specs/runtime/reference-knowledge.md)
- [cli-spec.md](../../../../docs/specs/cli/cli-spec.md) (`creator knowledge`, `creator reference`, `creator kb`, assemble-moment)
- [world-kb-runtime-architecture.md](../../../../docs/specs/architecture/world-kb-runtime-architecture.md), [spoke-adapter-architecture.md](../../../../docs/specs/architecture/spoke-adapter-architecture.md) (§6.1 route stability), [creator-workflow.md](../../../../docs/specs/creator/creator-workflow.md)
- Spoke memory records: `implemented/architecture/2026-10-28-spoke-memory-protocol.md`, `implemented/architecture/2026-10-28-spoke-memory-adoption-plan.md` (N4-T open; N5 closed 2026-11-11; no KnowledgeEntry-in-memory)
- Sibling strategy / work IRs: [PR #483](https://github.com/42ch-dev/nexus/pull/483), [PR #485](https://github.com/42ch-dev/nexus/pull/485)
- Companion record: `2026-11-10-knowledge-ir.md` — the Knowledge IR design built on this retirement
- Implementation evidence: `crates/nexus-local-db/migrations/20261111000001_retire_user_and_reference_stores.sql` (the drop), `crates/nexus-local-db/src/version.rs` (V1.231 schema bump)
