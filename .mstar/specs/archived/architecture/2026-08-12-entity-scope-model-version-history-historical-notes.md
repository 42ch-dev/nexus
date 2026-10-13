# Entity Scope Model — Version-Delivery History (Historical Notes)

**Provenance**: frozen from [`specs/active/architecture/entity-scope-model.md`](../../active/architecture/entity-scope-model.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-08-12 — the removed document-position log records the last version-delivery ("Last updated 2026-08-12 — V1.162 Review & Edit chain"); every removed tag cites the version that delivered a rule whose current statement remains in the live spec.
**Freeze date**: 2026-10-13

Nothing below is current authority. The live spec states every retained rule in present tense without version tags.

---

## 1. Document-position version log (removed from §0 Status / Last updated)

| **Status** | Normative — entity scope hierarchy, uniqueness, crate ownership. **V1.40 Shipped**: §5.1.1 narrative taxonomy (`BlockType` + `novel_category` + `canonical_name` grammar). **V1.50 Shipped**: §5.5 World KB promotion state machine. **V1.51 Shipped**: §5.5.6 LLM pathway subsection. **V1.54 Shipped**: §5.1.1 game-bible taxonomy. **V1.55 P3**: §5.1.1 script taxonomy. **V1.62 Shipped**: §5.5.9 computable-flag semantics + structured validation mode (closes `R-V161P1-LOW-001`). **V1.74 Shipped β**: §5.6 World KB relationship semantics. **V1.139 architect §5.2**: §2 (crate table: `nexus-kb`→`nexus-knowledge`, KeyBlock→KnowledgeEntry in architecture-bearing prose). **V1.155 P2**: §5.5+ prose refs modernized to KnowledgeEntry; remaining `KeyBlock` mentions are structural wire/DB identifiers (`kb_key_blocks` table, `ComputeInput.key_blocks` field, `KeyBlockStatus`/`KeyBlockId` types, `BlockType` enum values) and historical implementation records (`crates/nexus-kb/…` paths as-shipped). Full V1.139 SPOKE alignment tracked in [`spoke-adapter-architecture.md`](../../active/architecture/spoke-adapter-architecture.md). **V1.156**: §1.4 three-layer Draft overlay 3×2 matrix completion amendment (World×Moment + Work×Brief closed; frontend-only, `wire_contracts_changed: false` — no scope-ownership/transition change). **V1.159**: §5.1.1 era taxonomy amendment — `era_type` freeform field + era-era nesting via §5.6 `custom`/`custom_label: "parent_era"` (additive; `wire_contracts_changed: false`). **V1.160 Review & Edit chain (architect pass 2)**: §5.1.2 `patch_entity` create-on-absent write boundary (normative; closes create half of `R-V1159P1-001`; `wire_contracts_changed: false`). |
| **Last updated** | 2026-08-12 — V1.162 Review & Edit chain (architect pass 2): §6.6 fork-creation write boundary + lineage projection contract (PD-01 local-vs-platform reconciliation; carrier approach B locked). |

Original opening sentence:

> This file is normative for V1.23 crate wiring and naming alignment. When this file overlaps older wording in prior specs, keep the locked decisions here and update the older wording during the relevant follow-up task.

## 2. §1.4 Three-Layer Timeline — delivery blockquotes (removed)

> **V1.156 amendment (shipped)**: the 3×2 matrix is completed (World×Moment + Work×Brief closed — see §1.4.4). The amendment is frontend-only (`wire_contracts_changed: false`); it adds no scope-ownership, uniqueness, or transition rule. The V1.123 carrier locks (§1.4.1) are unchanged. Product semantics: `product-locks.md` PD-2 / PD-3.
> **V1.200 carrier amendment (shipped) — persisted Work-owned scene/beat carrier.** V1.200 replaces the deferred/fixtures-only Moment carrier described in §1.4.1, §1.4.3 and §1.4.4 with persisted Work-owned `WorkOutline.scenes[]` / `beats[]` (`{scene_id, chapter_id, title, status}` / `{beat_id, scene_id, title, status}`), authored through four additive `outline.patch_structure` operations (`add_scene` / `remove_scene` / `add_beat` / `remove_beat`) and read through the existing outline read route. Carrier/write-boundary contract: [`canvas-strategy-surface.md`](../../active/surfaces/canvas-strategy-surface.md) §3.3.3 (V1.200 amendment) + §3.5 (V1.200 scene/beat authoring). This is a wire-carrier extension, **not a new scope**: Outline remains the authoring home, both Timeline Moment projections stay read-only, default layers remain unchanged, and Moment Context Assembly is unaffected. The V1.156 frontend-only statement remains historical (V1.200 itself is a wire change).

## 3. §1.4.x version annotations (stripped; rules retained)

- §1.4 heading: "(V1.123 — Normative)" → "(Normative)".
- §1.4.1 carrier table: "(new wire enum value — see §5.1.1 narrative taxonomy extension + iteration architecture §2)", "(V1.122 preserved)", "(V1.72 preserved)", "(… — V1.200, replacing the V1.108 fixture-only deferred carrier)", "See iteration architecture §3." — tags dropped; `three-layer-architecture.md` is an iteration-package file that never landed in tracked specs (dead reference).
- §1.4.2: "(V1.73)" write-boundary/conflict-policy tags; "(architect seat 2 LOCK — see iteration architecture §2)".
- §1.4.3: "V1.123 **adds** a second, parallel meaning" (delivery framing); "(**V1.200** — …)"; "the V1.108 `OutlineSceneNodeData`… (architect seat 2 LOCK — see iteration architecture §3)"; "The previous V1.108 fixture-only deferred carrier is retired."; "(which since V1.200 carries …)"; "(V1.123)" disambiguation tag.
- §1.4.4 heading: "(LOCKED — V1.156 completed the 3×2 matrix)" → "(LOCKED)"; diagram version tags (`V1.123 + V1.156`, `— V1.123`, `(V1.122 baseline, reframed)`, `(V1.156; carrier V1.200)`, `(V1.156)`, `V1.73`, `(V1.72 preserved)`, `(persisted V1.200 carrier… architect UX-risk override documented in iteration architecture §7.3, authorized by product spec §4.3)`); trailing paragraph version framing ("The V1.156 matrix completion does not change spine ownership… `wire_contracts_changed: false` for **V1.156** … version-scoped: **V1.200 is a wire change** …"). `product-locks.md`, `three-layer-product-spec.md`, `layer-feel-differentiation.md` are dead iteration-package references.
- §1.4.5: "unchanged in V1.123"; "The V1.123 changes are confined to…"; "(… V1.123 overlay…)"; "The V1.156 matrix completion adds… (version-scoped to V1.156)"; "The **V1.200** scene/beat carrier …" tail.
- §1.4.6 cross-reference list: iteration-package bullets dropped (dead refs); canvas-strategy-surface.md bullet retargeted without version tags.

## 4. §5.5.8 V1.56 historical snapshot (removed)

Branch inputs do **not** redefine entity ownership. The following API wiring is a **V1.56 historical snapshot**; the former daemon handlers are not current implementation anchors:

- Registry refresh input: `nexus-orchestration::tasks::inject_registry_refresh_context()` → reads `nexus.registry.refresh` capability output via existing capability registry machinery.
- Workspace input: `nexus-orchestration::tasks::inject_workspace_context()` → reads active workspace session via its injected `workspace_state_provider` (`crates/nexus-orchestration/src/tasks/mod.rs`); the V1.56 wiring read the `nexus-daemon-runtime` workspace handlers, deleted with the crate in v1.193 P2.

This amendment exists to clarify that DF-56 conditional routing branch inputs are **derived from**, not **owners of**, the entity scope. The entity ownership hierarchy (Creator > World > Timeline > Event > Moment) is unchanged; routing inputs are projections.

Original closing sentence: "This amendment exists to clarify that DF-56 conditional routing branch inputs are **derived from**, not **owners of**, the entity scope." (heading tag "(V1.56 P3 amendment)" removed; "(CdnError variant stringified per V1.56 P1 fix-wave)" → "(CdnError variant stringified)"; "(V1.56 P0 OCC + persistent session)" → "(OCC + persistent session)")

## 5. §5.5.9 V1.61 → V1.62 evolution narrative (rewritten present-tense)

**Status**: Normative — V1.62 Shipped (closes `R-V161P1-LOW-001`).

V1.61 added `computable: Option<bool>` and `state: Option<serde_json::Value>` to
`KnowledgeEntryBody` (carried inside spoke `KnowledgeEntry` — see spoke `knowledge-entry.schema.json`). V1.62 formalizes
the semantics of these fields and establishes the structured validation mode that
was deferred from V1.61 P1.
when building the invocation snapshot. The V1.61 P1 historical implementation


Per-module attribute and state shapes are **not** declared in product-level
schemas. V1.61 placed placeholder schemas under
`schemas/compute/compute-entity-attributes.schema.json` and
`schemas/compute/compute-entity-state.schema.json` — these are **deleted**
in V1.62 P0. Their content (per-`BlockType` shape declarations) is replaced
by per-module JSON Schema fragments declared in each compute module's
`manifest.json` `schemas` block (V1.62 P1; see
[compute-module-abi.md](../../active/compute/compute-module-abi.md) §7.3).


##### 5.5.9.4 Relationship to deleted entity-* schemas

V1.61's `schemas/compute/compute-entity-attributes.schema.json` and
`schemas/compute/compute-entity-state.schema.json` are **deleted** in
V1.62 P0. Their content was per-`BlockType` placeholder shape declarations
(e.g., `character` attributes: `max_hp`, `base_atk`, `base_def`). V1.62
replaces this with:

- **Per-module shape declarations** in each module's `manifest.json`
  `schemas` block (V1.62 P1).
- **No product-level enumeration** of all possible BlockType attributes

Retained rules (still in the live spec, reworded): the `computable`/`state` field semantics; the structured-validation-mode steps; "Specs and code MUST NOT reference the deleted paths `schemas/compute/compute-entity-attributes.schema.json` / `compute-entity-state.schema.json`; the canonical replacement is `manifest.json` `schemas` (compute-module-abi.md §7.3)."

## 6. §5.1.1 history paragraphs (removed)

**V1.40 P1 history:** validation helpers shipped in the former `nexus-kb` crate for wire `BlockType` + optional `novel_category` / per-category `body.attributes` minimums, without a `schemas/` enum change in V1.40. Their current owner is `nexus_knowledge::world_kb::validation` (`crates/nexus-knowledge/src/world_kb/validation.rs`); later wire additions are recorded below.

**V1.123 implementation history:** no new `ValidationMode` was added for `era` — `era` is cross-profile and not subject to `novel_category` / `game_bible_category` / `script_category` enforcement (mirroring `event`, the V1.122 Timeline event block type). Advisory warnings only. `canonical_name` validation is identical across all modes. The wire enum change was additive; the then-current daemon Rust handlers accepting `BlockType` (`world_kb::patch_entity` and `world_kb::get_graph`) needed no code change because they parsed valid values through `parse_block_type`. **Current validation anchor:** `crates/nexus-knowledge/src/world_kb/validation.rs` (`is_cross_profile_block_type` recognizes `Event` and `Era`).

Stripped tags in §5.1.1: "(V1.40 grill-me locked — **Shipped V1.40 P1**)"; "The V1.61 compass initially named"; "the V1.54 game-bible precedent"; "The V1.37 novel"; "**V1.40 P1 validation (current owner):**"; "(V1.40 P1)" grammar/body-shape tags; "(body layer — V1.54 P1)"; "(V1.54 P1, snake_case on wire)"; "**V1.54 P1 validation (current owner):**"; "(body layer — V1.55 P3)"; "(cross-profile, V1.123 — Normative)"; "(V1.123)" body-shape tag; "(V1.55 P3, snake_case on wire)"; "**V1.55 P3 validation (current owner):**"; "The eight-value V1.40 baseline was extended by…" → "The baseline set was extended by…".

## 7. §5.1.1 era-taxonomy delivery framing (V1.159 amendment narration → present tense)

Original: "**Era taxonomy type semantics (V1.159 — Normative amendment to §5.1.1).** The V1.123 `era` block_type carried flat, untyped era markers. V1.159 deepens the Brief-era carrier with a **typed nesting model** (DF-V1123-ERA-TAXONOMY): … This is an **additive amendment** — it does not change the V1.123 `era` wire `BlockType`, … or the V1.123 implementation note (no `ValidationMode` for `era`)."

Also stripped: "**`era_type` field (V1.159 — cross-profile, freeform, no validation mode).**"; "This mirrors the V1.123 decision"; "V1.159 deliberately does NOT introduce… (mirrors the V1.123 "no `brief_category`" rationale…)"; "(legacy V1.123 flat-era data)"; "V1.156-era flat era data"; "**Era nesting carrier (V1.159 — reuses §5.6 …)**"; "(V1.159)" body-shape tag; "`kb.relate` (V1.144 spoke `RelationPort`)" / "`kb.patch_relationship` (V1.74)".

## 8. §5.1.2 V1.160 status/intro (rewritten)

> **Status**: Normative (V1.160 Review & Edit chain, architect pass 2) — closes the entity-create half of `R-V1159P1-001`. Documents the carrier that the World Timeline Brief 「新建 era」 affordance ([`canvas-strategy-surface.md`](../../active/surfaces/canvas-strategy-surface.md) §3.3.3 "Create entry") drives. `wire_contracts_changed: false`: the route, request DTO, response DTO, and success status are unchanged from V1.73; create-on-absent is an internal handler branch, not a new wire contract.

`POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73) is the sole author-facing World KB entity write surface. V1.160 makes it a true **create-or-update** handler by branching on the store read result instead of failing on absent entities (pre-V1.160 the handler mapped `NotFound` → 500). No new route, no DTO rename, no `upsert` re-branding — the wire name stays `patch-entity`.

Also stripped: "**Update** — unchanged V1.73 path"; "(V1.159 P1 T3)"; "the same V1.40 P1 validation"; "(must not regress — V1.73 greploop issue-3)"; "§3.3.3 "Create entry" (V1.159)".

## 9. §5.5 V1.50 delivery status (removed)

> **Status**: Normative (V1.50) — V1.50 T-B P1 shipped on 2026-06-18. Migration `202606180002_kb_extract_jobs.sql` landed; review-time extraction hook verified end-to-end; promotion row promoted Draft → Normative at V1.50 P-last.

Also stripped: "### 5.5 … (V1.50 normative)" → "(normative)"; "#### 5.5.3 Promotion gate (normative V1.50)" → "(normative)"; "(V1.50 T-B P1 extension)"; "(see V1.40 P1 validation module)" → "(see §5.1.1 validation module)".

## 10. §5.5.6 V1.50-heuristic → V1.51-LLM delivery narrative (rewritten)

#### 5.5.6 LLM extraction pathway (V1.51 T-A P0 — Normative)

V1.51 T-A P0 closes `R-V150KBED-01`. The V1.50 heuristic defaulted every
review-time candidate to `block_type_guess='character'` (capitalized noun
phrase), forcing authors to correct the type on adopt for every non-character
entity. V1.51 replaces the heuristic with the `nexus.llm.extract` capability
(see [llm-extract.md](../../active/orchestration/llm-extract.md)) at the review-time extraction hook.

Table headers "V1.50 heuristic" / "V1.51 LLM pathway" → "Heuristic fallback" / "LLM pathway (default)". §5.5.7: "(V1.52 T-A P2 — Normative)" → "(Normative)"; "(V1.50 heuristic or V1.51 LLM)" → "(heuristic or LLM)"; "V1.51 T-A P1 cross-chapter reconciliation" → "cross-chapter reconciliation".

## 11. §5.6 V1.74 delivery status (rewritten)

Original: "> **Status**: Normative — V1.74 Shipped β. This section defines first-class inter-entity relationships in the World KB graph. The V1.50–V1.73 model defined KeyBlocks, SourceAnchors, and the promotion state machine; V1.74 adds typed edges between confirmed/non-deleted World KB entities."

Also stripped: "V1.74 core values are:" → "Core values are:"; "display-only in V1.74" → "display-only"; "#### 5.6.7 … (V1.76 γ)" tag.

## 12. §6.6 delivery sentences (removed)

- §6.6.1: "`nexus.fork.create` capability (V1.60 P0, DF-46); the daemon HTTP route and Canvas UI that expose it to authors are added in V1.162." → "(DF-46); the daemon HTTP route and Canvas UI expose it to authors."
- "**Lazy-branch model (V1.26+, preserved).**" → "**Lazy-branch model.**"
- "(deferred — `BL-01` fork-merge; V1.162 compass Non-Goals)" → "(deferred — `BL-01` fork-merge)".
- §6.6.2: "(Tracker §2.1 PD-01 wording is reconciled to match by V1.162 P1 T3 — doc-aligns-to-shipped-reality, not new policy.)" — dropped.
- §6.6.3: "(V1.162 — normative)" → "(normative)"; "(architect-locked, V1.162)" → "(architect-locked)".

## 13. §2.2 / §4 version tags (stripped)

- §2.2 heading "(v1.190 normative)" → "(normative)"; "The v1.190 P0 core cutover makes the §2.1 workspace boundary enforceable" → "The §2.1 workspace boundary is enforceable".
- §4 `nexus-knowledge` row: "merged in V1.139 (former `nexus-kb`)" → "merged from the former `nexus-kb`"; §5.2 same edit.
- §4 `nexus-daemon-runtime` row: "(historical; retired v1.193)" → "(retired)".

## 14. §7 Wiring implications for V1.23 (whole section removed — historical snapshot of retired crate topology)

## 7. Wiring implications for V1.23

> **Historical V1.23 wiring snapshot.** The former `nexus-kb` and separate User-only `nexus-knowledge` responsibilities below describe that version, not current crate ownership. Current World/User knowledge ownership is in §4 and §5.2.

1. `nexus-moment-context-assembly` target wiring should aggregate local `nexus-creator-memory`, `nexus-narrative`, `nexus-kb`, and `nexus-knowledge` inputs for Stage-0 moment context.
2. `nexus-narrative` is the core entry point for creative-work narrative state and should be the natural owner for World/Timeline/Event queries consumed by context assembly and CLI/daemon local APIs.
3. `nexus-knowledge` must be described and wired as User-scoped global knowledge, not Creator-scoped knowledge.
4. `nexus-cloud-sync` must depend on and use `nexus-cloud-domain` for User/Pairing invariants.
5. `nexus-kb` should become the key module for World-scoped KB graph insertion and query, with `nexus-narrative` owning the surrounding narrative aggregates.
