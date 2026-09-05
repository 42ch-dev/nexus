# Actor Product Model

> **Status:** Draft (2026-09-04 user product lock) — **implementation not shipped**: no Actor storage, schema, or API exists yet, and no current source models are claimed shipped under these terms. The **product semantics herein are user-locked and authoritative for planning**: this spec locks the product model and its vocabulary, and it carries **no implement authority** until normative implementation specs land.
> **Document class:** Draft overlay (product-model SSOT; user-locked product semantics, authoritative for planning)
> **Scope:** `ActorRef` (`Creator | Character`) with per-kind bearers; Creator operational ownership; Character SOUL/Memory/ToM/image; ActorWorldBinding (1..n per active Character, atomic initial binding); WorldSheet distinction; KnowledgeEntry canonical ownership and read views (Creator omniscient read, Character KnowledgeView); one-Agent-Host execution with session isolation; Viewpoint; current-vs-planned migration contract; non-goals.
> **Coordinates with:** [entity-scope-model.md](entity-scope-model.md) (shipped KE taxonomy + scope hierarchy), [agent-host.md](agent-host.md) and [acp-client-tech-spec.md](acp-client-tech-spec.md) (the one host plane; ACP sessions), [creator-workflow.md](creator-workflow.md) and [creator-memory-soul-lifecycle.md](creator-memory-soul-lifecycle.md) (shipped Creator SOUL/Memory bearer), [spoke-adapter-architecture.md](spoke-adapter-architecture.md) (V1.164–V1.166 l5 MindState/belief/observation carriers; moment context assembly), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [world-membership.schema.json](../../schemas/domain/world-membership.schema.json) (shipped Creator↔World aggregate), repo-root [STRATEGY.md](../../STRATEGY.md) + [CONCEPTS.md](../../CONCEPTS.md).

## 0. Document position

This file is the **durable trackable spec-path authority (SSOT) for the Actor model**. `STRATEGY.md` and `CONCEPTS.md` link here; they do not restate the model. Execution tracking (row ids, triggers, owner projects) lives in the local Morning Star harness register, which is intentionally not tracked in this repo — this spec stands alone.

**Supersession.** This lock supersedes the same-day (2026-09-04) Viewpoint-as-identity direction, which encoded Viewpoint as identity and Character as an existing World-scoped `KnowledgeEntry(block_type=character)`. That direction was rejected: a Character is a durable first-class identity, not lore. Viewpoint is demoted to subordinate execution context (§7).

**Honesty invariant.** Everything herein is accepted product direction. Shipped behavior authorities remain the existing specs cited above; nothing in this file may be read as a shipped storage, schema, or API claim.

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
| `Character` | A durable, Creator-owned Character record carrying its own SOUL, Memory, ToM, and image/persona assets — **not** a World KB row | Accepted direction; not shipped |

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
- **Character creation establishes an initial ActorWorldBinding atomically.** An active Character never has zero active bindings. **Removing the last active binding fails** (no mutation). Transitioning a Character out of active state is a **separate later lifecycle contract** (explicit archive) — never an implicit effect of last-binding removal, and never an active orphan.
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
| **World-owned** | World-local truth | Shipped — the only owner scope with storage today; every World KB entry is World-owned |
| **Character-owned** | Lives in the Character knowledge space; **explicitly shared** by owner scope — visible in every active binding of that Character **without copying** (no separate mount join) | Accepted direction; not shipped |
| **ActorWorldBinding-owned** (binding-local) | Belongs to one binding; **isolated** from the Character's other Worlds — private to that World life | Accepted direction; not shipped |

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

## 8. Current vs planned (migration contract)

| Area | Shipped today (current) | Planned under this model (target) | Migration rule |
| --- | --- | --- | --- |
| Creator identity & storage | `creator_id` aggregate; SOUL/Memory stores; `WorldMembership` (Creator↔World) | Unchanged; the aggregate is the Creator kind's bearer | Existing tables and `creator_id` FKs are **not re-keyed**; no unified actors table |
| Creator execution | Runs with no Character concept | Unchanged when no Character is bound | Byte-stable Creator-only path |
| Character identity | — (no Character storage) | Durable Character bearer (SOUL/Memory/ToM/image, §4.1) | New bearer, distinct from Creator tables; additive |
| Character↔World association | — | ActorWorldBinding; 1..n per active Character; atomic initial binding at creation | New record type; `WorldMembership` stays Creator↔World only |
| World character lore | `KnowledgeEntry(block_type=character)` rows, World-owned | WorldSheets, optionally linked from bindings | Existing rows stay WorldSheets; binding links are explicit; **no silent migration** |
| KE ownership | Single owner scope: World | Exactly one canonical owner per entry: World \| Character \| ActorWorldBinding | Additive schema/codegen work; existing rows remain World-owned |
| ToM carriers | V1.164–V1.166 l5 authority on holder KE `modules.mental` / `modules.belief`, derivative `MindState` keyed by `holder_entry_id`, and event observation metadata | Character ToM L1+L2 on the same carriers | Character mapping is additive on belief-row `holder`; `MindState.holder_entry_id` remains the carrier KE id; no new ToM engine |
| Execution gate | n/a | Creator owns both ends + active binding; fail closed | New gate; never fall back to Creator/god context or a default ACP session |

## 9. Non-goals (v1)

- A unified actors table, or re-keying `creator_id` FKs.
- A second runtime or process plane, a companion Character app, an NPC swarm.
- Per-Character copied World KB; implicit cross-World sharing of World facts or memories.
- `WorldMembership` naming or reuse for Character↔World.
- An active Character stored with zero active bindings; last-active-binding removal that mutates state or implicitly archives/deletes the Character; any fallback to the Creator/god context or a default ACP session when a binding or view is missing/invalid.
- ToM L3, or replacing the V1.164–V1.166 l5 carriers with a new ToM engine.
- A first-party player (PD-09 unchanged) or any new consumption end.
- Any public Actor API before the contract and bearer stage lands (specs, vocabulary, and codegen first).

## 10. Staged roadmap (durable direction)

Dependency-ordered stages; each stage lands only on top of a working product. Execution tracking (row ids, triggers, owner projects) lives in the local harness register and is not tracked in-repo.

1. **Actor/Character contract + durable Character bearer** — atomic initial ActorWorldBinding; specs/vocabulary/codegen before any public API.
2. **ActorWorldBinding + Character KnowledgeView** — owner scopes, WorldSheet linkage, shared-vs-isolated KE, creator-only marker. (Dep: 1.)
3. **Execute Actor on the existing Agent Host** — optional actor identity in Moment context; same process plane; isolated sessions; bounded fail-closed views. (Dep: 1, 2. Harness-owned mechanics.)
4. **Multi-World binding proof** — Character-owned KE is visible across that Character's bindings without copying; binding-local KE stays isolated. (Dep: 2.)
5. **Character SOUL/Memory bearer lifecycle** — distinct bearer tables/files, reused pipeline semantics; execution projection consumes stage 3 without creating another runtime. (Bearer dep: 1; projection dep: 3.)
6. **Character ToM L1+L2** on the V1.164–V1.166 l5 carriers; Character mapping applies to belief `holder`, while MindState keeps the carrier KE id. (Dep: 2, 3, 5.)
7. **Canvas persona surface** — image, SOUL summary, bindings; after stage 1 plus observed need. (Dep: 1.)
8. **Visibility/interoperability dialect evaluation** — trigger-gated and spec-only; only if bindings + l5 carriers + a minimal creator-only marker prove insufficient for real needs (audience sets, target linkage, discovery time, fork applicability, order-2+ semantics). No engine commitment. (Dep: evidence after stage 2 dogfood.)
