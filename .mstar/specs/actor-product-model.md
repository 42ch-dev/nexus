# Actor Product Model

> **Status:** Draft overlay (2026-09-04 user product lock; **honesty amended 2026-09-06**). **v1.184 shipped** the Character bearer, ActorWorldBinding, three KE owner scopes, Character KnowledgeView, one-host Character execution, Character SOUL/Memory, and ToM L1/L2 (PR [#240](https://github.com/42ch-dev/nexus/pull/240)). **v1.185 shipped** (PR [#241](https://github.com/42ch-dev/nexus/pull/241)): identity edit, reversible archive/restore, post-create WorldSheet maintenance, KE content/detail/edit/delete, and run-connected `--remember`. Product vocabulary herein remains authoritative. This file is **not** a second wire/API SSOT for local implementation snapshots.
> **Document class:** Draft overlay (product-model SSOT; user-locked product semantics, authoritative for planning)
> **Scope:** `ActorRef` (`Creator | Character`) with per-kind bearers; Creator operational ownership; Character SOUL/Memory/ToM/image; ActorWorldBinding (1..n per active Character, atomic initial binding); WorldSheet distinction; KnowledgeEntry canonical ownership and read views (Creator omniscient read, Character KnowledgeView); one-Agent-Host execution with session isolation; Viewpoint; current-vs-planned migration contract; shipped v1.185 developer maintenance (§11); non-goals.
> **Coordinates with:** [entity-scope-model.md](entity-scope-model.md) (shipped KE taxonomy + scope hierarchy), [agent-host.md](agent-host.md) and [acp-client-tech-spec.md](acp-client-tech-spec.md) (the one host plane; ACP sessions), [creator-workflow.md](creator-workflow.md) and [creator-memory-soul-lifecycle.md](creator-memory-soul-lifecycle.md) (shipped Creator SOUL/Memory bearer), [spoke-adapter-architecture.md](spoke-adapter-architecture.md) (V1.164–V1.166 l5 MindState/belief/observation carriers; moment context assembly), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [world-membership.schema.json](../../schemas/domain/world-membership.schema.json) (shipped Creator↔World aggregate), repo-root [STRATEGY.md](../../STRATEGY.md) + [CONCEPTS.md](../../CONCEPTS.md).

## 0. Document position

This file is the **durable trackable spec-path authority (SSOT) for the Actor model**. `STRATEGY.md` and `CONCEPTS.md` link here; they do not restate the model. Execution tracking (row ids, triggers, owner projects) lives in the local Morning Star harness register, which is intentionally not tracked in this repo — this spec stands alone.

**Supersession.** This lock supersedes the same-day (2026-09-04) Viewpoint-as-identity direction, which encoded Viewpoint as identity and Character as an existing World-scoped `KnowledgeEntry(block_type=character)`. That direction was rejected: a Character is a durable first-class identity, not lore. Viewpoint is demoted to subordinate execution context (§7).

**Honesty invariant.** Product vocabulary in this file is accepted direction. **v1.184 shipped** stages 1–6 of §10 (identity, binding, view, execution, memory, ToM). Cite generated schemas and Daemon API for wire facts; do not treat this overlay as a dump of local handlers. **v1.185** behavior in §11 (identity edit, reversible freeze, WorldSheet maintenance, KE content maintenance, run-connected `--remember`) is **shipped** (PR #241). The pre-v1.185 absence of edit/archive/content/`--remember` verbs was a maintenance gap, not proof that no Actor storage exists.

## 1. Product thesis

**Actor** is the cross-cutting **narrative identity** primitive — *who can think and act* in a story.

- Outward line: **Nexus Actors are who can think and act — Creators conduct the story; Characters live it.**
- Actor is **not a fourth pillar**: it cuts across Harness (executes an Actor), Canvas (surfaces one), and Computable (worlds react to one).
- **No consumption-end changes**: still no first-party player (PD-09), no second runtime, no companion app.

## 2. ActorRef and per-kind bearers

`ActorRef` is a **closed v1 sum**: `Creator | Character`. It is a **product-identity model**, not a unified actors-table storage commitment — no unified actors table is introduced, and existing Creator storage remains as-is.

Each Actor kind has its **own bearer** — the storage/identity aggregate that carries that kind's state. The Actor is the narrative-identity reading of its bearer: the bearer is storage; the Actor is who thinks and acts.

| Actor kind | Bearer | Status |
| --- | --- | --- |
| `Creator` | The shipped `creator_id` identity aggregate — author profile, preferences, SOUL + Memory stores | Shipped; unchanged (not re-keyed) |
| `Character` | A durable, Creator-owned Character record carrying its own SOUL, Memory, ToM, and image/persona assets — **not** a World KB row | **Shipped v1.184** (create/list/detail, memory, ToM, run). Identity edit and reversible archive/restore are **shipped v1.185** |

### 2.1 Identity axes — keep these separate

| Axis | What it is | Relation to Actor identity |
| --- | --- | --- |
| **User Entrance** | User-layer usage identity (`developer` \| `content-creator`, V1.170 P1) selecting SPA layout trees | UI routing state; creates, swaps, or hides no agent or narrative identity |
| **Creator profile** | Shipped operational identity aggregate (`creator_id`): admission boundary, ownership, SOUL/Memory bearer | The *Creator Actor kind's bearer* — the profile is storage; Actor is the narrative-identity reading of it |
| **Orchestration role** | Workflow-function routing inside presets (`GraphNode.agent` / `WorkerAgentConfig.role`) | Names which worker executes a step; never names *who* in the story |
| **ACP session** (`HostSessionId`) | Transport conversation identity between Nexus and an agent | A session is a pipe: isolated per Actor/World view, but carries no identity semantics |
| **V1.164 l5 carriers** | `MindState.holder_entry_id` names the carrier KnowledgeEntry; `modules.belief[*].holder` names the epistemic subject; `TimelineEvent.modules.observation` names event observation metadata | ToM data axes, not identity joins. Character mapping is additive on belief `holder`; the carrier KE and observation placement remain unchanged |
| **`actor_kind=character`** | `ActorRef` discriminant value marking a first-class Character identity row | This one *is* Actor vocabulary — the identity axis; it does not denote a KB lore row |
| **`block_type=character`** | Shipped World KB lore taxonomy value on KnowledgeEntry | Lore *about* a person inside a World (a WorldSheet, §4.3) — a separate axis from `actor_kind=character`: taxonomy vs identity |

## 3. Creator — conducts the story (operational ownership)

The Creator is the first Actor kind: the god/orchestrator/narrative driver who **conducts** the story, and the **operational owner / admission identity** for everything Nexus executes.

- **Ownership.** The Creator owns Worlds via the shipped `WorldMembership` aggregate — Creator↔World only ([world-membership.schema.json](../../schemas/domain/world-membership.schema.json)). Characters are **Creator-owned** durable identities. ActorWorldBindings join owned entities (§4.2).
- **Bearer.** The Creator carries SOUL + Memory through the shipped Creator memory pipeline (see [creator-workflow.md](creator-workflow.md), [creator-memory-soul-lifecycle.md](creator-memory-soul-lifecycle.md)).
- **Omniscient read** over owned knowledge — see §5.2.
- **Execution gate.** Character execution requires the requesting Creator to own **both** the Character and the World, plus an active binding; a missing/invalid binding or incomplete view **fails closed** — never falls back to the Creator/god context or a default ACP session.
- **Stability.** Existing `creator_id` storage and FKs are not re-keyed, and Creator execution stays byte-stable when no Character is bound.

## 4. Character — lives the story

The second Actor kind: a durable, **Creator-owned** narrative identity that **lives inside** the story. A Character is **not** a World-scoped `KnowledgeEntry(block_type=character)` — those remain WorldSheets (§4.3).

### 4.1 Bearer: SOUL, Memory, ToM, image

- **SOUL + Memory** on a **distinct bearer** from Creator memory: Character writes never enter Creator memory rows. The pipeline semantics of the shipped Creator memory system are reused — this is not a second mind stack.
- **ToM** built **additively** on the shipped V1.164–V1.166 l5 carriers: L1 (self) and L2 (other). Character mapping extends the epistemic-subject `modules.belief[*].holder`; `MindState.holder_entry_id` remains the carrier KnowledgeEntry id. ToM L3 and any new ToM engine replacing the l5 carriers are out (§9).
- **Image/persona assets** attached to the Character record.

### 4.2 ActorWorldBinding — 1..n Worlds, atomic initial binding

A Character associates with Worlds **only** through explicit **ActorWorldBinding** records.

- Cardinality is exactly **one-or-more**: an active Character has **1..n** bindings, each to exactly one World.
- **Character creation establishes an initial ActorWorldBinding atomically.** An active Character never has zero active bindings. **Removing the last active binding fails** (no mutation). Transitioning a Character out of active state is the **v1.185 reversible freeze** (§11) — never an implicit effect of last-binding removal, and never an active orphan. That freeze is **shipped v1.185**.
- A binding carries the **binding-local isolated** Character KE scope (§5.1) and may link optional WorldSheets (§4.3).
- **Naming:** `WorldMembership` is reserved for the shipped Creator↔World aggregate and MUST NOT name Character↔World — ActorWorldBinding is the only Character↔World term.

### 4.3 WorldSheet distinction

A **WorldSheet** is a World-scoped `KnowledgeEntry(block_type=character)` read under the Actor model: **character lore owned by a World** — World-local truth.

- A binding may link a WorldSheet as that World's sheet *about* a Character; unbound sheets remain pure lore.
- Existing character KEs stay WorldSheets; **no silent migration**.
- A WorldSheet is data *about* a person in a World; a Character (`actor_kind=character`) is *who can think and act* — separate axes (§2.1).

## 5. Knowledge ownership and views

### 5.1 One KnowledgeEntry, exactly one canonical owner

KnowledgeEntry stays **one primitive**, and each entry has **exactly one canonical owner** scope:

| Owner scope | Semantics | Status |
| --- | --- | --- |
| **World-owned** | World-local truth | Shipped — pre-Actor default; every World KB entry is World-owned |
| **Character-owned** | Lives in the Character knowledge space; **explicitly shared** by owner scope — visible in every active binding of that Character **without copying** (no separate mount join) | **Shipped v1.184** (owner-scoped add without content body, Character list, composed view). Content/detail/edit/delete are **shipped v1.185** |
| **ActorWorldBinding-owned** (binding-local) | Belongs to one binding; **isolated** from the Character's other Worlds — private to that World life | **Shipped v1.184** as an owner scope; content maintenance is **shipped v1.185** |

Cross-World sharing is always explicit — it never implicitly copies all World facts or memories.

### 5.2 Creator omniscient read

The Creator's **capability** over its ownership boundary is omniscient: it covers every KnowledgeEntry whose canonical owner is a **World it owns**, a **Character it owns**, or an **ActorWorldBinding joining those owned entities** — across all three owner scopes — **including creator-only facts**. Creator-only is an explicit boolean marker on KnowledgeEntry (v1: valid on World-owned KE only). Entries marked creator-only never enter any Character KnowledgeView. A product surface may require an explicit World selection for bounded pagination (v1.184 does); that selector narrows one request, not the underlying ownership capability. This marker is not a generalized visibility taxonomy.

### 5.3 Character KnowledgeView

A Character **reads only its authorized composed KnowledgeView**, per (Character, active binding):

```text
KnowledgeView(Character, Binding) =
    authorized World-owned KE          (v1: all World-owned KE of binding.world_id except creator-only)
  + Character-owned KE of this Character
  + ActorWorldBinding-owned KE of this binding
```

- Never the whole World directly, never another binding's local KE, never creator-only facts.
- A missing/invalid binding or an incomplete view **fails closed** (§3 execution gate).

## 6. Execution — one Agent Host, session isolation

- **One Agent Host / runtime / provider plane serves both Actor kinds** (see [agent-host.md](agent-host.md), [acp-client-tech-spec.md](acp-client-tech-spec.md)). No second runtime or process plane.
- A Character session executes **under the owning Creator's admission boundary**, with an **isolated ACP conversation history** per Actor/World view.
- The ACP session (`HostSessionId`) is a pipe: isolation boundary, not identity (§2.1).
- Actor identity in Moment context is optional and additive; the Creator-only execution path is byte-stable (§8).

## 7. Viewpoint — subordinate execution context

**Viewpoint** is subordinate **execution context** paired with an `ActorRef` — logically `{world_id, optional binding_id/branch_id/event_id}` — describing *from where* that Actor acts or reads within a session. Character execution requires the binding id; Creator execution omits it. Viewpoint does not repeat an actor id, is **not** identity, is **not** an Actor kind, and is **not** the name of any Character↔World association. The earlier Viewpoint-as-identity direction is superseded (§0).

## 8. Current vs shipped (migration contract)

| Area | Shipped today (current) | Shipped v1.185 (§11) | Migration rule |
| --- | --- | --- | --- |
| Creator identity & storage | `creator_id` aggregate; SOUL/Memory stores; `WorldMembership` (Creator↔World) | Unchanged; the aggregate is the Creator kind's bearer | Existing tables and `creator_id` FKs are **not re-keyed**; no unified actors table |
| Creator execution | Runs with no Character concept | Unchanged when no Character is bound | Byte-stable Creator-only path |
| Character identity | Durable Character bearer: create/list/detail; SOUL/Memory/ToM; image/persona metadata on create (v1.184) | Identity edit + reversible archive/restore (shipped v1.185, §11) | Additive public verbs on the existing bearer; same `character_id`; **shipped v1.185** |
| Character↔World association | ActorWorldBinding add/list/remove; atomic initial binding; last-active-binding `409`; optional WorldSheet at create/add (v1.184) | Post-create WorldSheet set/change/clear (v1.185) | `WorldMembership` stays Creator↔World only; relink does not reparent |
| World character lore | `KnowledgeEntry(block_type=character)` rows, World-owned WorldSheets, optionally linked from bindings | Unchanged lore axis | Existing rows stay WorldSheets; binding links are explicit; **no silent migration** |
| KE ownership | Three owner scopes on one primitive: World \| Character \| ActorWorldBinding (v1.184). Add currently has no content body | Content/detail/edit/explicit delete with immutable owners (v1.185) | Existing World-owned rows remain World-owned; no owner transfer |
| ToM carriers | Character ToM L1+L2 on V1.164–V1.166 l5 carriers (v1.184) | Unchanged carriers; no L3 | Character mapping is additive on belief-row `holder`; `MindState.holder_entry_id` remains the carrier KE id; no new ToM engine |
| Execution gate | Creator owns both ends + active binding; fail closed (v1.184). Process-lifetime Actor sessions | Archived-readable vs write/run split; no cross-restart resume | Never fall back to Creator/god context or a default ACP session |
| Run → memory | Separate Character pending-review capture/review/promote (v1.184) | Explicit `--remember` on successful run only (v1.185) | No default capture; no auto-promotion |

## 9. Non-goals (v1)

- A unified actors table, or re-keying `creator_id` FKs.
- A second runtime or process plane, a companion Character app, an NPC swarm.
- Per-Character copied World KB; implicit cross-World sharing of World facts or memories.
- `WorldMembership` naming or reuse for Character↔World.
- An active Character stored with zero active bindings; last-active-binding removal that mutates state or implicitly archives/deletes the Character; any fallback to the Creator/god context or a default ACP session when a binding or view is missing/invalid.
- ToM L3, or replacing the V1.164–V1.166 l5 carriers with a new ToM engine.
- A first-party player (PD-09 unchanged) or any new consumption end.
- Treating the **pre-v1.185** absence of maintenance verbs (edit, archive, KE content, `--remember`) as proof that the v1.184 public Actor API does not exist.

## 10. Staged roadmap (durable direction)

Dependency-ordered stages; each stage lands only on top of a working product. Execution tracking (row ids, triggers, owner projects) lives in the local Morning Star harness register and is not tracked in-repo.

1. **Actor/Character contract + durable Character bearer** — **shipped v1.184.**
2. **ActorWorldBinding + Character KnowledgeView** — **shipped v1.184.**
3. **Execute Actor on the existing Agent Host** — **shipped v1.184.**
4. **Multi-World binding proof** — **shipped v1.184.**
5. **Character SOUL/Memory bearer lifecycle** — **shipped v1.184.**
6. **Character ToM L1+L2** on the V1.164–V1.166 l5 carriers — **shipped v1.184.**
7. **Canvas persona surface** — image, SOUL summary, bindings; after stage 1 plus observed need. (Dep: 1.) **Not scheduled in v1.185.**
8. **Visibility/interoperability dialect evaluation** — trigger-gated and spec-only. **Not scheduled in v1.185.**
9. **Developer maintenance loop (shipped v1.185)** — identity edit; reversible freeze; WorldSheet maintenance; KE content maintenance; explicit `--remember`. See §11.

## 11. Developer maintenance contract (shipped v1.185)

**Delivery status, not planning authority.** The user selected a four-plan developer API/CLI loop, reversible freeze and explicit `--remember`; no UI. The following architecture decisions resolve that scope against the shipped bearer, SQLite store and HostFacade; they shipped in v1.185 (PR #241). JSON Schemas remain the executable wire SSOT; names below designate the shipped v1.185 schemas and endpoints, not placeholder proposals.

### 11.1 Common authority, concurrency and errors

- The active Creator is resolved from daemon configuration. No new request accepts `owner_creator_id`. A foreign or absent stored Character/binding/KE/session/operation is `404 not_found`, without exposing another owner's revision or referents.
- Character `revision` and binding `revision` are independent nonnegative integer CAS tokens, initially `0`. New writes require `expected_revision` in `0..=9223372036854775806`; a successful material edit increments once. Timestamps are not concurrency tokens. KE uses its existing `revision`, normalizing SQL NULL to `0`.
- Character `lifecycle_epoch` is a separate, internal durable integer, initially `0`, incremented once on each material archive or restore. It is neither a client-supplied admission claim nor the metadata revision. Metadata edits and WorldSheet edits do not increment it.
- Patch schemas are closed (`additionalProperties:false`). Omission keeps a field; explicit null clears only nullable fields. Decode the original member presence before mapping a generated optional value: generated `Option<T>` alone must not collapse missing and null. Do not introduce a second handwritten wire DTO. An empty patch is HTTP 422 `invalid_input`; a supplied identical value is a CAS-checked no-op (no revision/timestamp change).
- For owned targets: active-write gate precedes CAS; CAS precedes dependent-resource validation and uniqueness. Same-state lifecycle requests first check CAS, then return the unchanged detail. Invalid wire structure is HTTP 422 `invalid_input`; database faults remain `500 internal` through the canonical `NexusApiError` envelope.
- Stable `409` codes: `character_inactive`, `character_busy`, `character_revision_conflict`, `binding_revision_conflict`, `knowledge_revision_conflict`, `character_restore_requires_active_binding`, `actor_session_stale`, `knowledge_entry_in_use`, `knowledge_reference_state_invalid`, `knowledge_entry_not_mutable`, `duplicate_actor_knowledge`. Reuse existing `duplicate_character_display_name`, `last_active_actor_world_binding`, `duplicate_active_actor_world_binding`, `invalid_world_sheet`, `world_has_actor_bindings`, `binding_has_owned_knowledge`, `binding_has_local_memory`, `world_inactive`, and `actor_session_busy`. Codes, not message text, are the branching contract. Existing `ConflictCoded` suffices; do not add a new error envelope or promise absent `details` fields.

### 11.2 Character identity and reversible freeze

**Routes** (all under `/v1/daemon`; Axum registrations use `:character_id`):

| Method/path | Request schema in `schemas/daemon-api/characters/` | Success |
|---|---|---|
| `PATCH /characters/{character_id}` | `update-character-request.schema.json` | `200` existing `character-detail.schema.json` |
| `POST /characters/{character_id}/archive` | `character-lifecycle-request.schema.json` | `200` CharacterDetail |
| `POST /characters/{character_id}/restore` | same lifecycle request | `200` CharacterDetail |
| `GET /characters`, `GET /characters/{character_id}` | existing list query / no body | existing list/detail, now with `revision` |

Lifecycle request is exactly `{expected_revision}`. Update request adds optional `display_name`, `image_uri`, `persona`. Mutable fields:

| Field | Value / null / absence |
|---|---|
| `display_name` | Existing trimmed, nonempty, at most 120 Unicode scalars; existing active-owner NOCASE uniqueness. Null rejects; absence keeps. |
| `image_uri` | String at most 2048 UTF-8 bytes (empty string remains a string, as on create); null stores SQL NULL; absence keeps. No fetch or URI interpretation. |
| `persona` | JSON object, at most 16384 UTF-8 bytes when compactly serialized; replace whole object, not recursive merge. Null clears to `{}`; absence keeps. |

Id, owner, created timestamp and bindings are immutable here. `status` remains the shipped `active|archived`; clients cannot PATCH it. Lists retain existing owner-wide behavior: both statuses by default, no new filter required, bounded existing pagination unchanged.

Archive changes only Character status/revision/updated_at/epoch. Binding rows retain their actual `active|inactive` status; no synthetic archived binding state, no unbinding, data deletion, file rewrite or ToM rewrite. World deletion continues counting **every** retained binding, independent of Character status.

Restore checks, inside its write transaction, `EXISTS(actor_world_bindings b JOIN narrative_worlds w ON w.world_id=b.world_id WHERE b.character_id=? AND b.status='active' AND w.owner_creator_id=? AND w.status='active')`. At least one match suffices; other inactive/invalid Worlds and their bindings are retained, not repaired. Each later run still validates its selected World/binding. Zero matches gives `character_restore_requires_active_binding`. The active display-name unique index is rechecked: a name claimed while this Character was archived gives `duplicate_character_display_name`, leaving it archived. Resolve by explicitly renaming the active conflicting Character; restore never silently renames either identity.

**Read versus mutation classification**:

| Surface | Archived owning-Creator behavior |
|---|---|
| Character list/detail; binding list/detail | Read retained records; do not filter the Character away |
| Character-owned KE list/detail; exact-binding KE detail; Character KnowledgeView | Read authorized data with the same owner tuple and creator-only exclusion |
| Creator World-selected KnowledgeView | Include owned archived Character/binding scopes; do not silently drop them with `c.status='active'` |
| Pending list/count, fragment list, ToM list, SOUL reflect with `force_regenerate=false`, canonical SOUL/Memory file reads | Read only; never synthesize, refresh caches, bootstrap files or advance queues |
| Identity edit; binding add/update/remove; KE add/update/delete; pending capture/dismiss; review; fragment promotion; SOUL reflect with `force_regenerate=true`; ToM record | `409 character_inactive`, zero DB/file/provider mutation |
| Actor session creation/reuse; Prompt; model/mode changes | No execution admission while archived; no fallback to Creator or legacy session |
| Host cancel/shutdown and terminal/outcome observation | Safety/observation operations, not Character authored-data writes; owner-scoped, allowed to drain/inspect an already admitted operation |

Retained-data reads verify current ownership and the stored binding tuple, **not** Character, binding or owned-World liveness. A World that is still owned but paused/archived does not erase owned retained history. Missing/foreign World still fails closed for World-selected/binding-selected reads. Active execution and mutation keep live Character + active selected binding + owned active World checks where a World/binding is involved. Character-shared memory/KE writes require only the active owned Character. ToM list does not require an L2 subject to remain active; it displays retained authored belief data. ToM record retains active viewer/World/binding and active co-bound L2-subject admission.

### 11.3 Atomic mutation and process-lifetime session boundary

**Selected ordering: busy refusal, not forced cancellation or waiting for a provider.** Archive/restore return `409 character_busy` with zero mutation if a Character side-effecting activity is in flight. The caller may wait or explicitly cancel the operation and then retry the lifecycle request with a freshly read revision. This resolves archive-during-run without a post-freeze write or an indefinite archive request.

The daemon keeps a reclaimable per-Character `tokio::sync::RwLock<()>` fence in its existing Actor-session workspace state:

1. Owner-check before allocating a fence. Every mutation, memory pipeline and session-create/Prompt takes an owned **read/activity** guard, then rechecks stored ownership/status/epoch. Hold it through all DB/file effects; a Prompt transfers it into the existing server-owned stream-drain task through terminal processing and optional capture. An idle retained Host session holds no activity guard.
2. Archive/restore owner-check, `try_write_owned` (busy if unavailable), then `BEGIN IMMEDIATE`, reload owner/revision/status, validate the transition, update row and epoch, commit. Never hold SQLite locks across provider shutdown or session creation. Lifecycle is not allowed from an unguarded daemon helper.
3. After a **material** committed transition, while the exclusive fence is still held, remove old-epoch session reuse keys and retire their ids in `ActorSessionRegistry`; old ids stay recognizably Actor-mode, never legacy. Compare the returned epoch with the guard's pre-transition epoch: a same-state CAS no-op retires/shuts down nothing. Physical Host shutdown is resource cleanup, not the safety boundary. Attempt shutdown once per retired session outside DB/registry locks; a cleanup failure is logged and cannot undo or misreport a committed archive. No retry worker or persistent conversation store.
4. Session keys and admitted contexts include stored lifecycle epoch. Prompt compares indexed and current epoch after activity admission and before MCA/Host; mismatch/retired id is `actor_session_stale`. Registry creation holds activity admission through index insertion, so an archive cannot race an in-flight create into a reusable old session. Restore starts a new epoch and therefore a fresh conversation; no pre-archive transcript is re-fed.
   Character-session list/get/events/cancel/shutdown also authorize the current Creator against the stored session owner; list filtering occurs before pagination. Retired tombstones retain owner identity, not only a set of ids. A foreign id is 404 before Host access; an owning Creator cannot execute through a retired id. Existing Creator-mode and never-indexed legacy behavior is otherwise unchanged.
5. All SQL mutations revalidate active ownership/provenance in their own write transaction. In particular, queue dismiss/claim/drop and KE insert must not rely only on an earlier route check. Filesystem review/promotion/reflect paths retain the activity guard across asynchronous work and file commit. Cancel/disconnect cannot drop the guard while a spawned task still writes.
6. Acquire order: Character activity fence → exact session-key lock (if needed) → short SQLite transaction; registry map mutexes are never held across await. Reclaim unused Character fences with the same live-Arc discipline as existing session-key locks, not timer tasks.

The fence composes with the existing single-daemon workspace ownership; it is not a cross-process lease or protection from direct unsupported SQL edits. SQLite transactions still serialize supported storage writers. Both guards and database predicates are required: guards cover Host/files, predicates cover stale stored authorization.

### 11.4 Binding and WorldSheet maintenance

`GET /characters/{character_id}/bindings/{binding_id}` returns `200` `character-binding-detail.schema.json` (`{binding: ActorWorldBinding}`).

`PATCH` on that path accepts `update-character-binding-request.schema.json`: required `expected_revision`, optional nullable `world_sheet_entry_id`, at least one mutable member. Success is the same detail schema. A sheet string must be a `kb_` id, at most 128 bytes; null unlinks; omission is not a clear. No-op and stale semantics follow §11.1.

Only `world_sheet_entry_id` is mutable. Binding id, Character id, World id, status and created timestamp cannot be patched. Add/remove remain explicit membership operations. Stored binding remains active during Character freeze; effective write authority derives from Character state.

Within `BEGIN IMMEDIATE`, require the owned active Character, exact active binding and owned active World, compare binding revision, then validate any new link against `kb_key_blocks`: `owner_kind='world'`, matching `world_id`, `block_type='character'`, live status not in `deleted|merged|deprecated`. `creator_only` sheets reject with `invalid_world_sheet` so their identity link cannot leak private lore through Character reads. Wrong type/World/owner, missing and non-live all use the same `invalid_world_sheet` conflict. Apply this validation to existing create/add as well as update. Clearing never edits or deletes the old sheet.

Binding DELETE retains its existing bodyless API and fixed precedence: owned active Character → exact active binding/owned active World → last-active-binding count → any binding-owned KE row (including non-live rows) → binding-local pending/fragments/SOUL-narrative rows → delete exactly the binding. All checks and deletion share `BEGIN IMMEDIATE`. A WorldSheet link is not binding-owned data: removing a non-last otherwise-empty binding removes only the link, not the World KE.

Two competing removals cannot both remove the final active binding. Capture versus removal is ordered by SQLite: capture-first creates local memory and removal conflicts; removal-first makes capture fail before any candidate/receipt insert. Linking/editing a WorldSheet never authorizes KE ownership transfer. Retained bindings still enforce `world_has_actor_bindings` on World deletion.

### 11.5 Canonical Actor KE content maintenance

**Narrow authored content:** use the existing `KnowledgeEntryBody.summary` in `kb_key_blocks.body_json`, not `knowledge_entries.content`, a new table, an Actor content column, or a second block copy. This is the text already consumed by Moment context assembly. The public field is `summary`; no synonymous `content` field.

| Surface | Contract |
|---|---|
| Existing `POST /actor-knowledge/entries` | Existing add request gains optional `summary:string`. For Character/binding owners it writes `body.summary` atomically with the row; omission preserves current body-less create. `summary` on World-owned create rejects HTTP 422 `invalid_input` (World maintenance expansion is out of scope). Existing response stays `{item}`, with revised KnowledgeViewItem below. |
| `GET /characters/{character_id}/knowledge/{entry_id}` | `200` `actor-knowledge/knowledge-entry-detail.schema.json`: `{item: KnowledgeViewItem, summary: string|null}` |
| `PATCH` on that detail path | `actor-knowledge/update-knowledge-entry-request.schema.json`: required `expected_revision`; optional non-null `canonical_name`, nullable `summary`; at least one mutable member; response is detail |
| `DELETE` on that detail path | `actor-knowledge/delete-knowledge-entry-query.schema.json`: required `expected_revision` query parameter; success `204` with empty body |

`KnowledgeViewItem` gains required integer `revision` (SQL NULL→0). Lists/views remain bounded metadata pages rather than returning all bodies. Detail is the explicitly selected authored summary, not an arbitrary modules editor or an unrestricted KE getter. It accepts a Character-owned row or a binding-owned row whose stored binding joins that path Character and an owned World; another Character, World-owned row or foreign binding is indistinguishable from missing. No client-provided owner selector can override the stored owner.

`canonical_name` uses existing `validate_canonical_name` and owner-scoped uniqueness. `summary` is plain UTF-8, at most 65536 bytes; empty string is an explicit empty summary, null removes the summary member, absence keeps it. Do not trim or silently truncate. Preserve all other raw `body_json` members (including unknown members), modules, status, block type, anchors, timestamps other than updated_at, ownership and extensions; malformed stored body refuses `409 knowledge_entry_not_mutable` rather than overwriting it. Canonical-name-only editing need not parse/rewrite the body. New empty-body create remains supported.

Updates/deletes require a live row (not `deleted|merged|deprecated`) and active owning Character; detail can inspect a retained non-live row. Freeze gate then CAS precede payload semantic validation. A successful material update bumps the shared KE revision once, so concurrent ToM carrier CAS and summary edit cannot erase each other. This surface accepts no `body`, `modules`, owner fields, `creator_only`, `block_type`, `status`, source-anchor or provenance patch.

**Delete is physical, explicit, unreferenced-only, never cascading.** In one write transaction, authorize stored owner, check live state and revision, then run existence checks before deleting the KE:

| Protected referent | Stored relation to check |
|---|---|
| WorldSheet links | `actor_world_bindings.world_sheet_entry_id`, all statuses; defensive even though valid sheets are World-owned |
| Authored source anchors | `kb_source_anchors.key_block_id`; do not rely on its CASCADE FK |
| Relationship endpoints | `kb_relationships.source_entity_id` or `target_entity_id`, any status; do not cascade |
| Derivative mind records | `mind_states.holder_entry_id`; do not cascade |
| Authoritative mind carrier data | Target KE has nonempty `modules.mental` or `modules.belief`; do not delete authored mind merely because no derivative exists |
| Structured l5 references | Exact target-id string values in KE `modules_json` and timeline `modules_json` (including holder / mental belief references / observation); use SQLite JSON traversal, not substring search or free-text inference |
| Timeline participants | Exact target-id element in `narrative_timeline_events.affected_key_block_ids_json` |
| Findings / compute state | `world_findings.target_entry_id`, `compute_sessions.entry_id` |

Any referent gives `knowledge_entry_in_use`, no counts/foreign ids disclosed. Invalid JSON in a reference-bearing document prevents proving absence: `knowledge_reference_state_invalid`, zero mutation. SQL `EXISTS`/JSON traversal, not unbounded Rust materialization; unknown modules are preserved and conservatively protected when they contain an exact id. Free-text summary/proposition mentions are not inferred references; the id must be an entire JSON string value, not text containing it. Do not scan unrelated text files or invent a generic reference index. FK constraints remain a final defense; live-scope ToM CAS + derivative insertion remains atomic. Existing World-owned relation/participant writers do not gain Character-owner support.

### 11.6 Successful run → pending candidate

`schemas/daemon-api/agent-host/execute-operation-request.schema.json` adds optional boolean `remember` **only to Prompt**; absent/false keeps current execution and performs no memory capture. `remember:true` requires a stored admitted Character session with an exact binding; Creator/legacy sessions reject HTTP 422 `invalid_input` before Host execution. No actor, binding, digest or operation id supplied in the Prompt request can claim capture authority.

The authority is the **existing server-owned drain of the stream returned by `HostFacade::exec`**, not CLI SSE, a broadcast subscriber, model-written metadata or a fabricated digest. Snapshot active Creator id, Character id, binding id, World/viewpoint, lifecycle epoch, server-minted Host operation id and session id during admission. Transfer the activity guard and that immutable snapshot into the drain.

- Only a matching `(session_id, op_id)` `OpFinished` with `reason=end_turn`, with no prior matching failure/cancel/stream fault, is successful for capture. `max_tokens`, `max_turn_requests`, and `refusal` are incomplete/refused outcomes, not successful memory evidence. `OpFailed`, explicit cancellation, session stop before terminal and EOF without terminal produce no candidate.
- Candidate text is exactly `Prompt:\n{raw user prompt}\n\nResponse:\n{concatenated matching MessageDelta.text}`. It never includes MCA/system context, prior turns, ThoughtDelta, tool output, plan/status events or credentials read from configuration. User-authored text remains user content; no claim of redacting secrets the user deliberately put in it.
- Bound the **entire** digest at 65536 UTF-8 bytes. Track length with checked arithmetic and stop buffering once exceeded; keep draining/streaming output. Oversize or empty/whitespace-only response means capture failure (`capture_too_large` / `capture_empty_output`), not a truncated or fabricated candidate and not provider failure. `task_kind='unknown'`; no classifier/provider call at capture time.
- In `BEGIN IMMEDIATE`, revalidate active Character, exact epoch, active same binding and owned active World; insert one pending row and one immutable capture receipt atomically. The receipt key is the server `operation_id`; columns are operation/session/Character/binding/epoch/pending ids and captured_at, **no transcript copy**. Receipt survives review/dismissal so observing completion again cannot recreate consumed memory. Conflicting provenance on an existing key fails, never counts as success. Receipts are historical deduplication facts, not active binding-owned rows; they do not permanently prevent otherwise-valid binding removal.
- Pending rows gain nullable `source_operation_id`: manual captures remain null; run captures carry the real Host operation id. Keep the real session id (do not invent a per-operation fake session). Replace the current `(character_id,session_id)` unique index with a partial index for manual rows (`source_operation_id IS NULL`) plus a unique non-null operation index. Thus two successful operations in one reusable session can each be captured. Reserve run-generated pending ids (`run_{operation UUID without hyphens}`); manual capture cannot submit that prefix or source_operation_id.
- A receipt replay can report its original capture, but no external replay/capture-retry API is added. No provider retry, automatic promotion, ToM inference, Creator write or generalized job system.

**Observation:** POST continues returning immediate `OperationResponse` (`status=started`). Add optional `capture` to that schema: absent on legacy/Creator operations, `disabled|pending` initial status on Character prompts. `GET /agent-host/operations/{operation_id}` returns generated `character-operation-result.schema.json` for owner-authorized process-lifetime Character operations only:

`{operation_id, session_id, run_status: running|succeeded|incomplete|failed|cancelled, finish_reason: null|end_turn|max_tokens|max_turn_requests|refusal, capture: {status: disabled|pending|captured|skipped|failed, pending_id: string|null, code: string|null}}`.

The nested capture object is `character-run-capture-outcome.schema.json`; required nullable members avoid ambiguous absence. `captured` means the enqueue transaction committed, not that the row still awaits review. `skipped` codes are `run_incomplete|run_failed|run_cancelled`; failure codes are `capture_too_large|capture_empty_output|capture_scope_changed|capture_store_failed`. Opted-out capture is always disabled, even when execution fails. A successful run with failed capture stays `run_status=succeeded`; output SSE is not rewritten.

Keep observation records in existing process-lifetime Actor workspace state: at most 128 nonterminal Character operations (reject before Host with `409 actor_operation_capacity`), retain the most recent 1024 terminal records FIFO, never evict a running record. Exceeding retention or restarting the daemon yields `404 not_found`, not replay. Store no output transcript in this map. This is bounded status observation, not persistent conversation history. Register before exec and finalize on every drain exit; an exec admission error removes the reservation.

SSE disconnect is **not cancellation**: the server continues draining/capturing and retains its activity guard. Explicit cancel uses short internal `Running → CancelRequested` versus `Running → Finalizing → Terminal` transitions. Cancellation first latches no-capture **before** awaiting Host.cancel; the resulting `cancelled` classification records accepted local cancel intent, not rollback of provider effects. Completion first moves Finalizing; subsequent cancel is `409 actor_operation_finished` and cannot erase capture. Do not hold that record mutex across Host/provider await. The first matching terminal finalizes exactly once; release the activity guard after capture/outcome finalization, not after arbitrary extra producer events. EOF/fault before terminal finalizes failure (or cancelled when cancel intent already won). Daemon termination before the capture transaction commits produces no receipt/candidate; a committed enqueue survives even if the client never saw it. No crash-replay promise.

The CLI correlates every event by returned session and operation ids (not the first terminal event on a reused session), displays received output and observes the generated outcome concurrently with SSE, every 100 ms after POST returns. It must not wait for a possibly dropped SSE terminal before starting observation. The first matching SSE terminal, terminal outcome, or transport loss starts a 30-second grace period to obtain the other terminal observation; this is not a 30-second provider-run limit. Success requires both matching stream terminal and terminal outcome. Missing stream terminal means output observation is incomplete; missing/expired outcome means capture outcome unavailable. Preserve received output and any known outcome, exit nonzero, and never infer provider failure or fabricate captured data. A successful run with failed capture is visibly distinct and nonzero **after preserving run output**; do not advise rerunning it. Existing pending/review/promotion commands consume the queue; only explicit fragment promotion clears binding provenance on the same fragment id.

### 11.7 Persistence and cutover rules

- New immutable 14-digit migrations only: Character `revision` + `lifecycle_epoch`; binding `revision`; run-capture receipts + nullable pending provenance + replacement unique indexes. Do not rebuild or rewrite shipped migrations, re-key Creator rows or copy KE content.
- Backfill counters with `NOT NULL DEFAULT 0`; historical pending rows retain null source operation, content and real session ids. Preserve existing name/binding/owner/FK constraints. Summary editing requires no new content column.
- Generated schema changes and both language outputs land together; update registered schema-drift fixtures and every changed struct-literal caller. No compatibility alias, duplicate DTO or mixed old/new active-lifecycle gate.
- Rollback before release is restoring a pre-upgrade development database snapshot plus matching binary, not dropping authored data or running old code against new semantics. Forward correction uses another migration. Conversation history is intentionally not restored across a lifecycle epoch.
