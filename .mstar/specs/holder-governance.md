# Actor Holder Governance

> **Status:** Shipped (v1.191 P1). Implemented and exercised on the v1.191 plan branch (`feat/v1.191-p1-spoke-0-13-adoption`); landed on `main` by the v1.191 iteration PR. Current SPOKE pins are the pinned upstream lockstep release recorded in the workspace manifests (D17).
> **Document class:** Draft overlay, sole technical authority for Actor holders and their disclosure policy.
> **Related:** [actor-product-model.md](actor-product-model.md), [spoke-adapter-architecture.md](spoke-adapter-architecture.md), [local-db-schema.md](local-db-schema.md), [rust-core-service-boundary.md](rust-core-service-boundary.md).

## 1. Independent identity axes

- `KnowledgeEntryRecord.owner: KnowledgeOwnerRef` remains the closed narrative container: World, Character, or ActorWorldBinding. It is not a holder. Its existing owner columns and owner-scoped uniqueness remain authoritative.
- Add native `holder_entry_id: Option<String>` and `disclosure: Option<String>` to that record. The adapter maps them exactly to SPOKE `KnowledgeEntry.owner` and `.disclosure`. Neither belongs in `extensions.nexus`.
- One service-managed holder KE exists per Creator and Character identity. World, binding, User, peer, session and WorldSheet are not holders. A Character uses the same holder in all bindings; this does not broaden its container selection.
- `SessionViewpoint` remains World/binding/branch/event execution context. SPOKE `Scope.viewpoint` is the separately resolved holder entry id. No API may deserialize a holder id into admitted Actor authority.
- Existing `mind_states.holder_entry_id` and l5 mental/belief references remain their existing narrative KE carrier model. Do not repoint them at Actor holders or create a second mind store. TimelineEvent gains no `owner` field.

## 2. Registry, identity stability and lifecycle

### 2.1 Persistence

Create `knowledge_holders` in each workspace database, owned by `nexus-local-db`:

| Column / constraint | Contract |
|---|---|
| `holder_entry_id TEXT PRIMARY KEY` | Reserved `hld_` namespace; never accepted as an ordinary narrative KE id |
| `creator_id TEXT NULL REFERENCES creators(creator_id) ON DELETE RESTRICT` | Exactly one of Creator/Character subject columns is non-null |
| `character_id TEXT NULL REFERENCES characters(character_id) ON DELETE RESTRICT` | Same exclusive-subject CHECK; unique partial index for each subject column |
| `created_at TEXT NOT NULL` | Creation fact; identity/name/status are not copied into a second authority |

The holder is a first-class KE projection from this registry plus its subject, not a fake World lore row. The sole adapter projection `holder_record_to_spoke` emits `entry_type="nexus_actor_holder"`, the subject's current nonempty name, `body={}`, `extensions={}`, no governance, and status `confirmed` for active or `deprecated` for archived. No invented World id, fourth `KnowledgeOwnerRef` variant, narrative search result, or WorldSheet relation is created. The registry owns holder identity; Actor tables own lifecycle/name. It has no public mutable KE endpoint.

Use the existing local-db `blake3` dependency to derive `hld_` plus the full lowercase 256-bit digest of the UTF-8 bytes `nexus-holder-v1\0creator\0<creator_id>` or `nexus-holder-v1\0character\0<character_id>`. This makes the same global Creator's workspace materializations resolve to the same id without a cross-database transaction; workspace `creators` rows are projections of global identity, not new Creators. Check a pre-existing id resolves to the same subject; collision/mismatch is a hard integrity error, never reassignment. No random re-mint on missing-row reads and no prefix-only proof of ownership. Imported identical strings do not establish a local identity (§6).

New local-db `holders.rs` owns transaction-taking `ensure_creator_holder_in_tx`, `ensure_character_holder_in_tx`, and read-only `resolve_holder`. Existing Creator materialization (`creators::ensure_creator_row` and daemon `handlers/creators.rs` helper) converges on the one transactional implementation. Global registration need not atomically write every workspace; before a workspace identity becomes usable its local subject and registry row must commit together. Character creation adds its registry row in the existing create transaction. Backfill includes archived subjects.

### 2.2 Lifecycle and retained reads

Rename changes only the projected label, never holder identity or KE ownership. Archive retains the holder and ordinary owned data. Retained Creator management reads may resolve archived holders; no new execution, governance targeting, or source extraction is admitted for archived subjects. Restore reuses the same holder and existing Actor lifecycle-epoch transition.

Public holder create/delete/merge/transfer/list-all operations do not exist. Actor deletion is unreferenced-only: any governance reference, binding, retained owned row or existing Actor reference guard refuses deletion. If deletion is otherwise allowed, remove holder and subject atomically, never cascade governed rows or transfer ownership. Missing/corrupt registry on a normal read returns `holder_state_invalid`, not provisioning or a broader fallback. Ordinary lore KE removal never removes holders; add governance-reference checks to the existing exact-reference removal guard.

## 3. Native governance and atomic authoring

Add indexed nullable `holder_entry_id` and `disclosure` columns to `kb_key_blocks`. Empty strings are invalid. `owner-private` requires a nonempty holder; shared is absence of disclosure, not the string `shared`. A local private write must resolve an existing authorized registry row within the same transaction. Keep imported unresolved ids outside this table (§6), allowing an FK from native `holder_entry_id` to `knowledge_holders` with `ON DELETE RESTRICT`. Index governance alongside the existing owner/page keys; do not replace the container indexes.

Author-facing `audience` is a closed object: `{kind:"shared"}`, `{kind:"author-only"}`, or `{kind:"character-private",character_id:string}`. On create, omission means shared. On patch, omission preserves both governance columns; shared explicitly clears both. Author-only resolves the admitted controlling Creator holder; Character-private resolves an owned active Character. For a World row the Character must have an active binding to that owned World; Character/binding rows can target only their owning Character. The author cannot supply `holder_entry_id` or a management flag on these commands. Raw SPOKE import and Connect schemas remain wire-compatible but resolve/reject under their own admission rules.

Patch `audience` with `expected_revision` alongside existing name/summary edits. Acquire the existing-style nonblocking activity/transition fences before opening `BEGIN IMMEDIATE`; inside the transaction recheck stored owner/lifecycle → CAS → resolve intended audience → validate WorldSheet references → update material fields and bump KE revision exactly once. No-op leaves revision unchanged; unrelated content/module edits preserve governance. Upsert/promote/compute callbacks must retain stored governance unless passed the explicit admitted governance mutation; ordinary `KbStore::update` is not a transfer mechanism. Content body/unknown module preservation and reference guards remain unchanged.

An entry linked as a WorldSheet must remain a live World-owned `character` KE in the same World with **no disclosure**. Apply this both when linking and when changing governance/status of an already-linked entry, in the same transaction. Private or unknown-disclosure sheets reject `invalid_world_sheet`; do not silently unlink them.

## 4. Trusted reads, filtering and invalidation

### 4.1 Core admission and lower-layer selectors

In `nexus-core::actor_knowledge`, introduce non-Serde, private-field `AdmittedKnowledgeContext`, constructed only after `Principal`, stored owner and `AdmittedActorContext` checks. It holds the existing Actor activity lease where applicable and a resolved read selection. Two server-chosen policies:

- `CreatorManagement`: owned World/Character/binding containers, including known private rows for any authorized owned Character. Retained reads are allowed. This is a distinct management query, never encoded as `Scope.viewpoint=None`.
- `ActorView`: exact admitted Creator or Character holder plus authorized containers. Character selection is World + that Character + this binding. Connect always uses this policy, including grants for a Creator; it never inherits omniscient management review.

Lower-layer `KnowledgeReadScope` in `nexus-knowledge::world_kb` carries typed container selectors, resolved holder and policy, with no deserialization/default and no implication that a client-provided value is authority. Core creates it from admitted context; adapter/local-db validate its stored subject/container tuple when resolving rows. They do not depend on `nexus-core` and cannot receive a core `Principal`. Public handlers never accept this internal type. Existing `NexusAdapter::new(pool)` cannot retain unrestricted KE reads: require a request-bound scope for KE/query/relationship/compute ports; host metadata/tools need no knowledge scope. Shared module caches remain host-owned and reused rather than recompiling for each scoped adapter.

### 4.2 Selection before observation

For ActorView, a row is visible iff it is in authorized containers AND (`disclosure IS NULL` OR (`disclosure='owner-private'` AND `holder_entry_id=resolved_holder`)). Unknown disclosure is excluded. The corresponding wire helper is a secondary invariant, not a substitute for local authorization: SPOKE's optional-viewpoint scope behavior does not grant local access.

Apply SQL predicates before keyset cursor, LIMIT, count, ranking/snippets, join expansion and truncation checks. Search filters eligible row ids before scoring/returning snippets. A hidden read-by-id is indistinguishable from missing. Relationship endpoints, findings and compute reads cannot reveal hidden row ids or payloads; filter/reject the containing operation before it produces output. Do not cap all rows then filter in Rust. CreatorManagement uses explicit owned-container checks and known-governance admission, not the ActorView wire helper.

MCA, inspect, lore emission and model context consume the same complete filtered snapshot; missing/incomplete snapshot fails closed, never falls back to unfiltered World KB. Author management data is not an extraction/model source simply because the author can review it. Holder metadata resolves only through an authorized subject or already-visible governed entry, never a global directory.

### 4.3 Session and context safety

Reuse `nexus-core::actor_fence`'s nonblocking in-process plus OS-lock mechanism and the server-owned stream drain. Its current implementation is per-Character; this contract adds a typed World knowledge fence in that same module, not a second daemon lock service. ActorView operations hold World shared then Character shared leases through their actual effects. World-governance edits take World exclusive; Character-global/binding-governance edits take Character exclusive. Multi-scope acquisition is World ids then Character ids, lexicographically within each kind, with immediate busy refusal and release on failure. This blocks privacy changes during active streams, including Connect, rather than pretending a stale prompt can be recalled.

Add `knowledge_revision INTEGER NOT NULL DEFAULT 0` to `narrative_worlds` and `characters`. Material governance changes increment the owning World or Character knowledge revision once in the same KE transaction; a binding-owned change increments its Character revision. Add this revision pair and read-policy kind to `ActorSessionKey` and the admitted context snapshot. Re-read under the leases before prompt/session reuse: mismatch retires the old session through existing tombstone machinery and admits a fresh context. This invalidates Character-global changes across all bindings and protects cross-process reuse, without overloading the existing lifecycle epoch. No-op and cosmetic name changes do not bump these knowledge revisions. Revalidate holder/owner/lifecycle at commit. Management snapshots never become ActorView.

## 5. Cutover migration and rollback

The change is one offline schema cutover, not dual-write compatibility. Raise the current DB schema version 23 to 24 (or the next unallocated version at execution), add one ordered migration and use existing strict migration/checksum admission and writer-protocol guards.

1. Quiesce CLI/core/daemon/Connect writers and in-flight Actor activities; acquire existing exclusive database/writer admission. Back up the complete database before DDL. No old process remains attached during migration.
2. Preflight stored World controlling Creator references, all Creator/Character identities (including archived), legacy bool validity, malformed extension documents, WorldSheet legality, and any native-governance collision. Report entry/subject ids to the local operator; fail before changing data. Do not resolve ownership from whichever Creator happens to be active in the shell.
3. In one migration transaction create/backfill registry, add governance columns/FK/indexes and writer-guard triggers. Shared rows remain null/null. Every `creator_only=true` World row receives its stored World Creator holder and `owner-private`. Existing equivalent native governance may be accepted only on exact equality; contradictory owner/disclosure or unresolvable Creator aborts, never picks a winner.
4. Rebuild the table as necessary to remove legacy bool/CHECK/index dependencies while preserving row ids, revisions, statuses, bodies, unknown non-legacy extensions, all indexes/FKs/triggers. Remove `extensions.nexus.creator_only`; it is not unknown passthrough. Validate referential integrity and visibility-equivalence counts inside the transaction, then advance schema metadata and commit.
5. All new readers/writers require the completed version before serving. Old binaries reject the unknown migration/version through strict admission; no `ignore_missing` or read-only bypass. Rollback is matching old binary plus complete pre-migration database backup, never an in-place lossy reverse migration.

Delete runtime `creator_only` fields/accessors/predicates/SQL/projections/CLI flag and update affected tests. Only historical migrations and the new offline migration may interpret the bool. Explicitly reject its presence in old JSON, raw extension input, import and CLI usage with the existing invalid-input error family and stable reason `legacy_creator_only_unsupported`; `false` is not silently ignored. Schema `additionalProperties` settings alone are insufficient where raw objects/extensions are accepted.

## 6. Import/export and external identity

Preserve wire owner/disclosure verbatim in pack parsing and lossless serialization. Neither unknown vocabulary nor foreign ids are rewritten to shared or to the importing Creator. Add `knowledge_import_quarantine` for unresolved/unknown governance: import batch/id, local controlling Creator and intended container, immutable original KE JSON, and source provenance. It is outside ordinary KB stores, search/MCA/compute/export and Connect. No foreign string, including one equal to `hld_<local digest>`, is trusted merely by equality.

Extend existing `creator world kb pack import` with explicit per-import `--holder-map <foreign-id>=<permitted-actor-selector>` mappings; without a mapping foreign-governed rows are quarantined. Validate mappings through authoring admission. Unknown disclosure remains quarantined even with mapped owner. Extend the import response with quarantine ids/reasons/original governance; add `--review-import <batch-id>` as a read-only mode of that same import command (mutually exclusive with file/ST input and mappings), authorized to the stored controlling Creator. The matching existing pack-import API accepts that review selector as a separate request arm and returns bounded original quarantined atoms, never a model-facing KnowledgeView. Re-import with a valid explicit mapping adopts each atom and removes its matching quarantine row atomically, preserving existing per-atom conflict-policy behavior, not inventing whole-pack atomicity. This is local import review, not federation or automatic ownership claim.

Normal exports use their admitted read policy and include owner/disclosure exactly. Holder references may travel without embedding holder records, as allowed by SPOKE. Author management export may include owned known-private material by explicit author intent; Character/Connect export stays filtered. Legacy `creator_only` packs reject with the stable error rather than masquerading as owner-private without a mapping.

## 7. Public surfaces and caller inventory

Schemas under `schemas/` remain executable wire SSOT. The following are intended source changes, not claims of generated output already present:

| Surface | Required cutover |
|---|---|
| `schemas/daemon-api/actor-knowledge/{add-knowledge-entry-request,update-knowledge-entry-request,knowledge-view-item,knowledge-entry-detail,view-request,view-response,list-character-knowledge-response}.schema.json` | Add audience input, native governance projection; remove legacy bool. Existing narrative `owner` stays `KnowledgeOwnerRef`; projection fields are `holder_entry_id` and `disclosure`, not overloaded `owner`. `audience` patch supports omission; no nullable-required accidental default |
| `schemas/daemon-api/characters/character-detail.schema.json`, `creators/creator-detail.schema.json` and their referenced identity schemas | Read-only `holder_entry_id` where identity is already authorized; no new holder CRUD route |
| `schemas/daemon-api/canvas/world-kb/{world-kb-entity-patch,world-kb-entity-projection,world-kb-patch-entity-request}.schema.json` | Governance patch and read projection through existing World maintenance, same revision as content |
| `schemas/daemon-api/kb/{pack-import-request,pack-import-response,pack-export-request,pack-export-response}.schema.json` | Explicit holder mappings, bounded quarantine/review arm and lossless governed pack output; keep existing per-atom conflict policies |
| `nexus-core/src/actor_knowledge.rs`, `world_kb.rs`, `actors.rs`, `actor_sessions.rs`, `actor_fence.rs` | Admission, authoring, native projection, invalidation and named context builder; stored identity remains authority |
| `nexus-local-db/src/{creators,character,actor_world_binding,actor_knowledge_store,kb_store,writer_protocol,version}.rs`, migrations; new `holders.rs` | Atomic provisioning/CAS, native SQL, WorldSheet/referential guards, pre-limit views, schema gate |
| `nexus-knowledge/src/world_kb/{knowledge_entry,store,extract_finalize}.rs` | Governance record/in-memory parity, immutable ordinary writes, prepared-candidate persistence split |
| `nexus-spoke-adapter/src/conversion/knowledge_entry.rs`, `extensions.rs`, `adapter/{mod,knowledge_entry_port,scope_query_port,relation_port,finding_port,computable_port}.rs` | Sole field conversion, no legacy extension, request scopes on every load/mutate port and endpoint expansion |
| `nexus-daemon-runtime/src/api/handlers/{actor_knowledge,characters,creators,world_kb,world_kb_guards,world_kb_pack}.rs` | Existing routes delegate to the core policy; reject raw old keys before deserialization can discard them |
| `nexus-core-node/src/{actors,domain}.rs`; `apps/nexus-service/src/{actors,world-kb}.ts` | Update generated response-family projections and instance-bound core calls; no parallel TS DTO/authorization |
| `apps/nexus42/src/commands/creator/{character,kb}.rs`, `creator/world/kb/{legacy_impl,pack}.rs` | Existing add/list/view/show/edit/remove, `--audience shared\|author-only\|character-private` plus `--audience-character <id>`; edit requires expected revision. Keep canonical `creator world kb` and its existing deprecated alias, add no new command family |
| `nexus-core/src/world_pack.rs::{run_world_pack_import,import_pack,export_pack}`, retained daemon `pack_import.rs` translation | Core owns mapping/quarantine/conflict policy and scoped export; CLI direct export must converge on this admitted path rather than loading a raw store |
| `nexus-mca` Moment assembly, `nexus-orchestration/src/moment.rs`, lore/inspect builders, knowledge view search/list/detail, pack import/export | Only admitted complete snapshots; preserve policies through every builder rather than filtering only at HTTP edge |
| `apps/nexus42/src/connect/{allowlist,invoke}.rs`, core Connect accept/responder, daemon Connect composition | §9 matrix and authenticated stored grants |

The unrelated User/work file knowledge API in `nexus-core/src/knowledge.rs` does not become a narrative holder API. The historical `kb_key_blocks` table name remains. Before changing exported symbols, enumerate direct references with LSP; bounded search for the removed bool and constructors supplements, not replaces, symbol-aware migration.

## 8. Production extraction boundary

Nexus keeps source I/O, the job queue, PromptExecutor, relationship extraction and persistence. The adapter alone calls `spoke_operations::adapter::orchestrate_extract`; `ExtractionPort` stays outside BaselinePorts/FullPorts.

Introduce adapter `extract_candidates(request, resolved_input, extractor)` as the narrow wrapper. Its adapter-owned `ResolvedExtractionPort` implements `load_extraction_input` over an already admitted, bounded native source bundle. The orchestration layer resolves file/work/chapter sources with its existing resolver and supplies the real run/session/cancellation identity; the adapter does not import orchestration or core. A native callback returns prepared KE records plus method/coverage; the adapter converts to SPOKE `ExtractionResult`, validates through the upstream orchestrator, and returns the same candidate ids plus run metadata. No model-generated holder assignment is accepted: trusted job policy supplies governance before conversion.

Split `extract_finalize.rs` into prepare (validate body, allocate id once, attach source/governance) and persist-prepared (no new id/default governance). `kb_extract_work.rs` uses this path before its existing atomic candidate/job completion transaction. `llm_extract.rs`/the `kb-extract` preset invoke the same wrapper, not a second raw LLM path; keep current local result shape and its relationship sidecar because SPOKE's success arm contains KEs only. Relationships are validated/resolved against exactly these candidate ids and persisted with the job result. Quality-loop review-time extraction must pass its real admitted session instead of `""`.

Cancellation before/after callback, invalid protocol result, extractor failure, incomplete terminal or stale admission yields zero candidate/relationship writes and no successful job terminal. Recheck cancellation and scope in the final transaction. Hold existing activity leases through the server-owned stream drain. Source load failure invokes no extractor. Never invent a default session, drop relationships, persist a second reconstructed candidate, or claim remote extraction support from a local wrapper.

## 9. Connect admission and served operations

Extend local `PeerScope`/allowlist rows with an operator-stored Actor grant (controlling Creator id, ActorRef, World/binding viewpoint). Absence grants no KE operations. Resolve it against current stored ownership/lifecycle on every invoke; the authenticated session peer must equal the granted peer and requested world/op/module must remain inside existing allowlist ∩ capability-token scope. Raw `owner`, scope selectors and viewpoint can only narrow/match the grant, never choose another actor. Keep the existing unsupported-op gate before payload parsing and existing timeout/result-size bounds.

| Served family | Admission / policy | Manifest |
|---|---|---|
| `upsert` | Granted ActorView, authorized live container; preserve stored governance on update. New private rows only for grant's own resolved holder; no management transfer API | `ke-ownership` only when these guards and every read port are wired |
| `promote` | Same grant; read candidate through filtered policy; preserve governance and existing CAS/status gates | Same |
| `relate` | Both endpoints visible under same grant; no hidden endpoint id/status leak | Same |
| `check`, `assemble`, `compute` | Scope viewpoint must match resolved holder; all input KE/relationships/findings and emitted results filtered/revalidated. Compute cannot assign foreign holders | Same |
| `tools.nexus.list_observed_peers`, `tools.nexus.list_modules` | Existing exact tool allowlist and peer/module policy; no holder directory | Do not infer KE capability from tools |
| `extract` | Not served remotely; reject unsupported | Never advertise `ke-extraction` |

The current CLI Connect host can retain six KE ops plus the two exact tools after enforcement is complete. A core/daemon responder composed with no KE ports remains tools-only and declares neither new family; do not advertise planned support. Existing baseline families and `libp2p =0.56.0` remain unchanged; external libp2p residuals are not resolved by this feature.

## 10. Required implementation evidence

Evidence must cover two Creators, two Characters, two Worlds, multiple bindings; shared/Creator-private/Character-private/unknown/malformed rows; retained archived reads; rename/restore id stability; old-bool migration equivalence; conflicting migration rollback; spoofed holder/viewpoint and foreign-import id collision; hidden rows before page boundaries/search scoring; governance CAS versus active session/context reuse; WorldSheet reverse-reference edits; every Connect served family; and both real local extraction callers with cancellation/no-write and relationship preservation. Use existing focused crate tests and CLI/daemon smoke paths, not a broad local suite or a helper-only claim.

**Collected evidence (v1.191 P1).** Every row above is discharged by the plan's task gates: `git diff --check` clean over the declared cutover paths; no runtime legacy predicate or unscoped public KE reader remains; the acceptance matrix, per-row gate citations, promoted docs and residual owners are recorded in `.mstar/sdd/2026-09-18-v1.191-p1-spoke-0-13-adoption/task-15-report.md` (§AC4–AC10 mapping and §§10-reconciliation). Residual items that are *not* discharged by this contract are listed there with owners; nothing in this document claims a behaviour that was not exercised.
