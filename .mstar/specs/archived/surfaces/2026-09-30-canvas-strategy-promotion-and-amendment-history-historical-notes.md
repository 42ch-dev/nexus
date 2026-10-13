# Canvas Strategy Surface — Promotion and Amendment History Historical Notes

**Provenance**: frozen from [`specs/active/surfaces/canvas-strategy-surface.md`](../../active/surfaces/canvas-strategy-surface.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-30 — V1.201 (2026-09-30) is the newest amendment in the removed text; the V1.200 (2026-09-29) amendment superseded the V1.156/V1.159 carrier-and-layer text frozen here, and the V1.65 whole-document editor was retired at V1.75. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (84 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~5

```markdown
| **Status** | **Shipped β (V1.74)** — Strategy read + visualization + live overlay + Idea-steer (V1.70), write-boundary operation DTOs + node-granular Strategy edits + conflict policy (V1.71), Outline+Timeline canvas β (Work → Volume → Chapter → Scene/Beat graph projection + timeline lane + foreshadow edges + 3 structured patch routes `outline.patch_structure` / `outline.patch_chapter` / `timeline.patch_event` + outlineRevision + structured conflict error + UI retry/merge + non-spatial alternate views) (V1.72), World KB canvas β (World KB graph + candidates projections, 2 structured patch routes `kb.patch_entity` / `kb.promote_candidate`, per-row OCC on `kb_key_blocks.revision` / `kb_extract_jobs.version`, 409/422 structured errors, and 4 Daemon API routes) (V1.73), and typed World KB relationship editing (schema-backed relationship DTOs, `world_kb.patch_relationship`, `kb_relationships.revision` OCC, directed/symmetric projections, non-spatial relationship table) (V1.74) are shipped. **Post-V1.74 overlay amendments (additive):** V1.122 Timeline peer surface + V1.123 three-layer/Work Timeline (Draft), V1.156 3×2 matrix completion, V1.159 era taxonomy, V1.162 fork authoring chrome, V1.163 event-level cross-surface binding — see the promotion blockquote chain below; shipped β text preserved; `wire_contracts_changed: false`. |
```

### at pre-freeze line ~8

```markdown
| **Coordinates with** | [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) (strategy = graph-of-graphs), [web-ui.md](../../active/surfaces/web-ui.md) (§15 V1.67 stage + V1.68 canvas roadmap), [daemon-api-surface-conventions.md](../../active/runtime/daemon-api-surface-conventions.md), [chapter-content-local-api.md](../../active/runtime/chapter-content-local-api.md), [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) (historical host record) |
```

### at pre-freeze line ~12

```markdown
> **Promoted to Draft (2026-06-27 V1.69 P0).** The V1.67 Exploration was promoted to Draft by `@architect` for interface contracts, structured write boundary, and canvas-token contract. Product/UX thesis from the original `@product-manager` contribution remains in §4. This Draft intentionally stops short of schema/codegen or React Flow implementation authority.

```

### at pre-freeze line ~16

```markdown
> **Promoted to Shipped α (V1.70).** V1.70 shipped the first Strategy Canvas slice: read-only Strategy graph projection, canvas visualization, live execution overlay, and Idea-steer affordance. Implementation provenance: parent `079f687f`; feature commits `81cb4256`, `f82bcdd3`, `10edf22f`, `dad35736` on `feature/v1.70-canvas-strategy-read`, merged into `iteration/v1.70`. This promotion is scoped to the α read/overlay/steer slice only; structured write-boundary DTOs, node-granular editing, outline+timeline canvas, and World KB canvas remain Draft for V1.71+.

> **Promoted to Shipped β (V1.71).** V1.71 ships the Strategy write-boundary slice: schema-backed patch DTOs, 3 node-granular Strategy patch routes, YAML `revision:` graphRevision conflict detection, daemon validation, atomic persistence, and UI retry/merge conflict handling. This promotion is scoped to the Strategy surface only; outline+timeline and World KB canvas surfaces remain Draft for V1.72+.

> **Promoted to Shipped β (V1.72).** V1.72 ships the Outline+Timeline β slice: schema-backed patch DTOs (`OutlinePatchStructureRequest` / `OutlinePatchChapterRequest` / `TimelinePatchEventRequest` + `OutlinePatchResponse` + `OutlineConflictError` + `OutlineValidationError`), 3 Daemon API patch routes (structure / chapter / timeline-event), `outline_revision:` markdown frontmatter graphRevision conflict detection, daemon validation (ID existence, structural integrity, status lifecycle, timeline reference resolution, revision precondition), atomic outline markdown persistence (temp + rename + fsync + dir fsync), and UI retry/merge conflict handling with outline-flavored copy + non-spatial alternate views (chapter list + timeline event list). `@42ch/nexus-contracts` 0.7.0 → 0.8.0 (additive outline DTOs). DESIGN.md gains 8 outline/timeline canvas-write tokens (`canvas-outline-volume-fill` + 4 chapter-card statuses + `canvas-outline-timeline-event-pin` + `canvas-outline-foreshadow-edge` + `canvas-outline-timeline-marker` + `canvas-outline-conflict-marker`). This promotion is scoped to the Outline+Timeline surface only; World KB canvas surface remains Draft for V1.73+. Canvas-pivot (retiring V1.65 outline whole-document editor) remains V1.73+ backlog.

> **Promoted to Shipped β (V1.73).** V1.73 ships the World KB canvas β slice: schema/codegen-backed World KB DTOs, 2 structured patch routes (`POST /v1/daemon/worlds/{world_id}/kb/patch-entity`, `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate`), per-row OCC via `expected_version` against `kb_key_blocks.revision` / `kb_extract_jobs.version`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and 4 Daemon API routes including the read projections (`GET /v1/daemon/worlds/{world_id}/kb/graph`, `GET /v1/daemon/worlds/{world_id}/kb/candidates`). This promotion is scoped to the World KB entities + candidates surface; typed World KB relationship editing remains V1.74+.

> **Promoted to Shipped β (V1.74).** V1.74 ships the typed World KB relationships β slice: schema-backed relationship DTOs, a single structured patch route (`POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` with action `add | update | remove`), per-row OCC via `expected_version` against `kb_relationships.revision`, structured 409 `WorldKbConflictError` + 422 `WorldKbValidationError`, and `GET /v1/daemon/worlds/{world_id}/kb/graph` populated with typed `relationships[]`. This promotion is scoped to first-class relationship editing for the World KB surface; relationship confidence weighting/filtering and automatic relationship extraction remain future work.

> **Promoted to Draft (V1.122) — Timeline peer-surface amendment.** V1.122 elevates **Timeline** from a lane inside the Outline+Timeline surface to a fourth peer `CanvasSurfaceKind = "timeline"` that is the **World-building hero** surface and the **default World-entry** surface. This is an **additive Draft overlay**: it introduces the Timeline peer surface, its World-building projection contract, its write-boundary reuse plan, the Timeline-as-default-World-entry IA rule, and it repositions the existing `work-outline-timeline` surface narratively as **"Outline (Timeline-companion)"** (the `CanvasSurfaceKind` string `"work-outline-timeline"` is unchanged; only the narrative label is clarified). Shipped β normative text for the Strategy, Outline+Timeline, and World KB surfaces is **not rewritten or removed**. The architect-locked data composition, adapter contract, write boundary, and conflict policy live in §3.3.2; the authoritative iteration-scoped contract is `timeline-canvas-architecture.md`. `wire_contracts_changed: false` (additive `CanvasSurfaceKind` enum value + reuse of 12 shipped DTOs/routes; no new schemas, no codegen, no daemon Rust change).

> **Promoted to Draft (V1.123) — Three-Layer Timeline + Work Timeline peer amendment.** V1.123 deepens Timeline into **three zoom layers** — **Brief** (world-global era markers), **Narrative** (event-level, V1.122 reframed), **Moment** (scene/beat precision) — with **domain-differentiated layer composition** (World Timeline: Brief + Narrative; Work Timeline: Narrative + Moment). This is an **additive Draft overlay**: §3.3.3 introduces (a) the Brief↔Narrative layer switcher on the V1.122 `timeline` surface; (b) a new `CanvasSurfaceKind = "work-timeline"` peer surface for Work Timeline with a Narrative↔Moment layer switcher; (c) the architect-locked carrier contract (Brief-on-KnowledgeEntry via new wire `BlockType = "era"`; Moment-on-Outline via V1.108 Scene/Beat UI projection from V1.72 `WorkOutline`); (d) cross-layer navigation rules; (e) per-layer empty-state honesty rules; (f) layer-state persistence via URL query `?layer=...`. Shipped β normative text for the Strategy, Outline+Timeline, World KB, and V1.122 Timeline surfaces is **not rewritten or removed**. The authoritative iteration-scoped contract (Brief/Moment carrier trade-off matrices, adapter TypeScript signatures, conflict policy per layer, 8-point wire-contracts gate) is `three-layer-architecture.md`. `wire_contracts_changed: true` (single additive enum value `"era"` in `BlockType` — `schemas/common/common.schema.json`; no new daemon route, no new DTO, no new conflict DTO; codegen regen + minor `@42ch/nexus-contracts` version bump).

```

### at pre-freeze line ~40

```markdown
> **V1.122 Draft overlay:** a fourth peer surface — **Timeline (World-building hero)** — is added alongside these three shipped surfaces (§3.3.2). Timeline is the default surface for **World entry**; the three surfaces above remain peers, and the Work outline + timeline surface is renamed narratively to **"Outline (Timeline-companion)"** (its `CanvasSurfaceKind` string `"work-outline-timeline"` is unchanged). The shipped β description of the original three surfaces in this section is not rewritten.
```

### at pre-freeze line ~44

```markdown
### 1.1 V1.71 β shipped slice
```

### at pre-freeze line ~46

```markdown
V1.70 promoted the **Strategy Canvas α** read/overlay/steer subset; V1.71 promotes the **Strategy Canvas β** write-boundary subset from design input to shipped product behavior:
```

### at pre-freeze line ~56

```markdown
### 1.2 V1.72 β shipped slice
```

### at pre-freeze line ~58

```markdown
V1.71 promoted the **Strategy Canvas β** write-boundary subset; V1.72 promotes the **Outline+Timeline Canvas β** write-boundary subset:
```

### at pre-freeze line ~60

```markdown
- **Read + visualization**: Work → Volume → Chapter → Scene/Beat graph projection. Volume lanes render as sub-flows with `parentId`+`extent:parent` children. Chapter cards display `wc`/`slug`/`status` from the outline markdown frontmatter. TipTap fragment preview is **read-only on the canvas** (the V1.65 whole-document outline editor was retired in V1.75; outline prose is now authored in the Chapter node inspector via `outline.patch_chapter` `set.content` — see §3.5).
```

### at pre-freeze line ~63

```markdown
- **Conflict policy**: each patch carries `base_revision`; the daemon compares it with the outline markdown `outline_revision:` frontmatter key and returns structured conflict errors. The UI keeps the draft patch, refetches canonical state, and offers **Use current**, **Reapply my edit**, and **Review side-by-side** (side-by-side enabled only when draft and canonical changes touch non-overlapping fields, disabled for same-field/path or timeline-event content). **Body ownership invariant** (compass §6.4): outline markdown body remains V1.65 editor-owned and is never overwritten by canvas writes — the canvas re-reads body under `RuntimeLockGuard` and preserves it across patch commits.
```

### at pre-freeze line ~67

```markdown
The shipped β slice still does **not** promote node-granular World KB graph edits or canvas-pivot (retiring V1.65 outline whole-document editor). World KB canvas surface remains Draft for V1.73+; canvas-pivot remains V1.73+ backlog.
```

### at pre-freeze line ~69

```markdown
### 1.3 V1.73 β shipped slice
```

### at pre-freeze line ~71

```markdown
V1.72 promoted the **Outline+Timeline Canvas β** write-boundary subset; V1.73 promotes the **World KB Canvas β** entities + candidates subset:
```

### at pre-freeze line ~78

```markdown
The shipped β slice still does **not** promote typed World KB relationship CRUD. Relationship edges render from source-anchor provenance in V1.73; the durable V1.74 target is a first-class World KB relationships surface.
```

### at pre-freeze line ~80

```markdown
### 1.4 V1.74 β shipped slice
```

### at pre-freeze line ~82

```markdown
V1.73 promoted the **World KB Canvas β** entities + candidates subset; V1.74 completes the World KB surface with typed relationships:
```

### at pre-freeze line ~84

```markdown
- **Typed relationship edges**: `WorldKbGraphResponse.relationships[]` now contains `WorldKbRelationshipProjection` items instead of the V1.73 empty reserved array. Each item projects one stored `kb_relationships` row, with `projection_direction` distinguishing the stored direction from a derived symmetric reverse projection.
```

### at pre-freeze line ~97

```markdown
- The shipped V1.65 whole-document outline rich-text editor (TipTap over `outline_path`) is itself a **canvas-pivot candidate** (V1.68+ target, recorded here; **not** changed in V1.67 — no regression to shipped surface).
- The Daemon API write boundary for canvas surfaces is **structured/node-granular**, not whole-file PUT. (V1.68 design; this Exploration records the principle.)
```

### at pre-freeze line ~113

```markdown
`apps/web/package.json` confirms **React Flow is not yet installed** and **TipTap is already present** (`@tiptap/react`, `@tiptap/starter-kit`, `tiptap-markdown`). Therefore V1.68 would add `@xyflow/react` as a new dependency and keep TipTap only for rich content **inside a node**, not for whole-document editing.
```

### at pre-freeze line ~117

```markdown
- **Browser tab (Vite SPA)** — React Flow is a DOM/React library, compatible with the current React 19 + Vite stack (V1.157 upgrade) in `apps/web/package.json`. There is no SSR path in this repo; nevertheless React Flow should be imported only in browser-rendered routes/components because it depends on DOM sizing/interaction.
- **Desktop shell** — the Electron host (since v1.192; the V1.66–V1.191 native shell is historical) loads the same `apps/web/dist`. The historical WKWebView caveat no longer applies — Electron embeds Chromium — but desktop gesture/keyboard smoke tests remain an implement-time obligation for whichever shell is current. (V1.68 implement decision)
```

### at pre-freeze line ~134

```markdown
**Data source.** The static canvas is fed by the Strategy definition (preset YAML bundle: `states`, `inner_graphs`, prompt/template references). The live overlay is fed by session state from the daemon (`orchestration-engine.md` §3.3, §4.2; `web-ui.md` §5 `NexusClient` boundary). V1.67 promotes preset get/update/delete methods on the TS client, but this Exploration does **not** assert that the Daemon API already exposes the exact graph document shape or session detail needed by the canvas. V1.68 should add or promote read endpoints such as "get Strategy graph projection" and "get session graph overlay" if the existing preset/session detail endpoints are too YAML/raw or too summary-only. (V1.68 implement decision)
```

### at pre-freeze line ~308

```markdown
> **V1.156 amendment (shipped) — 3×2 matrix completion.** V1.156 closed the two deferred matrix cells: **World Timeline gains the Moment layer** (`DF-V1123-WORLD-MOMENT` — read/projection of bound Works' scene-precision; Moments remain Work-owned) and **Work Timeline gains the Brief layer** (`DF-V1123-WORK-BRIEF` — projection of the bound World's Brief; Brief remains World spine). V1.156 shipped: both surfaces render all three layers (Brief + Narrative + Moment). This is an **additive amendment** to the V1.123 overlay: it does not rewrite the V1.123 carrier locks or conflict policy; it extends each surface's layer set from two to three. Both amendments are **frontend-only** (`wire_contracts_changed: false` — see the "wire_contracts_changed: false verification (V1.156)" subsection below). Product semantics locked in `product-locks.md` PD-2 / PD-3. **Promoted to Normative (V1.158)** — the V1.123 overlay shipped its two-layer composition in V1.123 but was not promoted at V1.123 P-last (spec-hygiene debt); this amendment completed the matrix and was the promotion point for the whole §3.3.3 overlay.
```

### at pre-freeze line ~312

```markdown
> **V1.163 amendment (architect-locked, Phase 1 review chain) — event-level cross-surface binding carrier (DF-V1123-CROSS-SURFACE-BINDING).** V1.163 upgrades the V1.123 P3 surface-level cross-surface CTAs ("View on World Timeline" / "View in Work Timeline") to **event-level read-only deep-links**. The carrier is an additive optional field `world_event_id?: string` on `WorkOutline.timeline_events[]` items (schema `schemas/daemon-api/canvas/outline/work-outline.schema.json`, `additionalProperties: false` preserved). The **referent is locked to the World KB entity `key_block_id`** — a `block_type=event` KnowledgeEntry entity from `WorldKbGraphResponse.entities[]`, which is exactly the id the World Timeline Narrative layer projects (React Flow node id = `entity:<key_block_id>`; `TimelineNodeData extends WorldKbEntityProjection`, `nodeIdOf(keyBlockId)`). It is **NOT** `narrative_timeline_events.id` (`evt_…`) — those `TimelineEventInfo` rows are compute-result / fork-marker log events merged as synthetic `compute:<event_id>` nodes (V1.147 P2 T3), not authored when-axis content; using them would require an explicit join with no projection benefit and would target non-selectable synthetic nodes. Work→World forward reads `world_event_id` on the selected Work event → `/worlds/:worldId/timeline?layer=narrative&event=:worldEventId` (`worldEventId` = `key_block_id`). World→Work reverse scans the capped set of realizing Works' outlines (`timeline_events[].world_event_id === <selected World event key_block_id>`) → `/works/:workId/timeline?layer=narrative&event=:workEventId` (`workEventId` = `WorkOutline.timeline_events[].event_id`, Work Timeline React Flow node id `wt-event:<event_id>`). **Focus mechanism = React Flow node selection** (destination orchestrator reads `?event=` via the existing `useSearchParams()` layer-param plumbing and selects the matching node after projection; unknown id → Narrative layer loads, no fabricated node — honest empty focus). Reverse fan-out cap = N=20 most-recent Works (reuses the V1.123 P3 `useWorks({ limit: 20 })` + per-Work detail fan-out; event-level scan adds an outline read for the capped candidate set). This is an **additive amendment**: it adds the `world_event_id` field + event-level focus behavior; it changes no layer-set, write boundary, or conflict policy. The V1.123 surface-level fallback (Work↔World bound but no event-level match → `?layer=narrative` without `event`) is preserved (PD-5 three-state matrix). Read-only in V1.163 — no author write path (deferred to post-dogfood; **superseded by the V1.200 Outline binding-authoring amendment below**). Authoritative iteration-scoped elaboration: `cross-surface-event-binding-product-locks.md`.
```

### at pre-freeze line ~314

```markdown
> **V1.200 amendment (shipped) — persisted Work-owned scene/beat carrier (DR-26 / `DF-V1123-MOMENT-WIRE`).** V1.200 replaces the deferred/fixtures-only Moment carrier with persisted Work-owned `WorkOutline.scenes[]` / `beats[]` (`{scene_id, chapter_id, title, status}` / `{beat_id, scene_id, title, status}`), read through the existing `GET /v1/daemon/works/{work_id}/outline` boundary and authored through four additive `outline.patch_structure` operations (`add_scene` / `remove_scene` / `add_beat` / `remove_beat` — see §3.5). This section's Layer composition and both surfaces' **Carrier contract** rows below are amended accordingly: the Work Timeline Moment layer projects its own `WorkOutline` carrier; the World Timeline Moment layer projects the composed bound-Work carrier (each entry tagged with its owning Work — chapter numbers are Work-local, so two Works that each own a chapter 1 stay distinct). Ownership is unchanged: **Outline remains the single authoring home; both Timeline Moment layers stay read-only projections; no World KB entity or storage change is introduced; scenes/beats are never fabricated** — an empty canonical array is real emptiness. Default layers are unchanged (World: Brief-if-`era`-data-else-Narrative; Work: Narrative). Coordinate [entity-scope-model.md](../../active/architecture/entity-scope-model.md) §1.4. **V1.200 is a wire change** (`wire_contracts_changed: true` — schema + codegen + Rust core + napi + service translation), which is why the V1.156 "frontend-only" verification paragraphs below are now explicitly version-scoped to V1.156.
```

### at pre-freeze line ~316

```markdown
> **V1.200 amendment (shipped) — Outline World-event binding authoring (DR-26 / `DF-V1123-CROSS-SURFACE-BINDING` write half; plan `002-cross-surface-binding-write.md`).** The V1.163 read carrier gains its first-party write path. `timeline.patch_event` gains two **additive** operations — `bind_world_event` / `unbind_world_event` — on the **existing** `POST /v1/daemon/works/{work_id}/timeline/patch` route (`TimelinePatchEventRequest` / `OutlinePatchResponse`, new optional `world_event_id` member; `additionalProperties: false` preserved, one `outline_revision` bump per accepted mutation). Authoring lives on the **Outline** surface (`apps/web` outline Timeline event inspector: each row shows the projected binding and offers bind/unbind); the projected Work/World Timeline inspectors stay read-only. **This supersedes AC-V1163-7's no-author-write scope**; everything else the V1.163 amendment locked is preserved — the KB `key_block_id` referent (`block_type=event`, never `narrative_timeline_events.id`), PD-5's three-state navigation, the N=20 reverse fan-out, and read-only navigation behavior (both CTAs still navigate and never patch).
```

### at pre-freeze line ~318

```markdown
> Binding semantics (each enforced in core, not the UI): `bind_world_event` requires `event_id` **and** `world_event_id`; `unbind_world_event` requires only `event_id` and clears the stored value **without re-reading the old referent**, so a referent deleted from the World KB stays clearable, and an already-unbound valid event is explicitly accepted with exactly one revision bump. **Both** operations require a Work with a bound World (no bound World → typed 422 `outline_validation_failed` refusal, zero mutation). **V1.201 amendment (shipped, residual `_default`/`002`/R2 remedy): `bind_world_event` reuses the published-chapter refusal for an event with an attached chapter, and its refusal message names `unbind_world_event` as the remedy; `unbind_world_event` is that sanctioned remedy and proceeds on a published chapter** (same clear-without-reread semantics, one revision bump). For a bind, the referent is validated against the Work's **stored** bound World (ownership guard first → 403 on a foreign/unowned World) and must be a `block_type=event` entity of it (missing/non-event referent → typed 422); an unknown `event_id` keeps the existing 404. A stale `base_revision` keeps the retained 409 `OutlineConflict` envelope with zero mutation — **no new conflict DTO, no new route, no new persistence** (the value is the outline frontmatter event record). The Outline inspector renders the binding control **disabled, not hidden**, when the Work has no bound World, with the core refusal as the authority. The `work-outline.schema.json` `world_event_id` read shape is unchanged; only its description and the `timeline-patch-event-request.schema.json` contract were regenerated.
```

### at pre-freeze line ~320

```markdown
V1.123 deepens Timeline into **three zoom layers** — **Brief** (world-global era markers), **Narrative** (event-level, V1.122 reframed — the Timeline layer, distinct from prose-craft narrative writing), **Moment** (scene/beat precision — the Timeline layer, distinct from Moment Context Assembly). V1.123 shipped two layers per surface (World Timeline: Brief + Narrative; Work Timeline: Narrative + Moment); **V1.156 completed the 3×2 matrix** so both surfaces render all three layers. Cross-layer navigation is **within one Timeline surface** (Brief↔Narrative↔Moment) — not cross-surface (World Timeline ↔ Work Timeline cross-surface jump uses explicit jump affordances per §3.3.3 cross-layer rules, not the layer switcher).
```

### at pre-freeze line ~346

```markdown
> **3×2 matrix (V1.156 — complete).** Both surfaces render Brief + Narrative + Moment. The two V1.123 deferred cells are closed: World×Moment (`DF-V1123-WORLD-MOMENT`) and Work×Brief (`DF-V1123-WORK-BRIEF`). The V1.123 author mental model ("World Timeline = Brief + Narrative; Work Timeline = Narrative + Moment") is now superseded: after V1.156, **both surfaces show the full three-layer stack**, with the **ownership invariant** preserved — Brief is authored on the World spine (`block_type=era` KnowledgeEntry); Moment is authored on the Work spine (Outline Scene/Beat); each surface's projection of the layer it does not own is **read-only** (World Timeline Moment = read projection of bound Works' scenes; Work Timeline Brief = read projection of bound World's eras).
```

### at pre-freeze line ~374

```markdown
| **Layer set** | Brief + Narrative + **Moment** (V1.156). V1.123 shipped Brief + Narrative; V1.156 closes the World×Moment cell (`DF-V1123-WORLD-MOMENT`). Moment on the World Timeline is a **read-only projection** of scene-precision from Works bound to that World (PD-3) — not a World authoring surface. |
```

### at pre-freeze line ~392

```markdown
| **Layer set** | **Brief** + Narrative + Moment (V1.156). V1.123 shipped Narrative + Moment; V1.156 closes the Work×Brief cell (`DF-V1123-WORK-BRIEF`). Brief on the Work Timeline is a **read-only projection** of the bound World's Brief (PD-2) — Work does not gain an independently authored Brief. |
| **Default layer** | **Narrative** (architect UX-risk override — see iteration architecture §7.3; preserved in V1.156). Brief and Moment are each one click away. |
```

### at pre-freeze line ~440

```markdown
| **Invalid layer** | V1.156: all three layer values (`brief` / `narrative` / `moment`) are valid on **both** surfaces. The V1.123 restriction ("Moment ignored on World Timeline; Brief ignored on Work Timeline") is **lifted** — both surfaces now render all three layers. Unknown layer values (`?layer=foo`) fall back to the surface default. |
```

### at pre-freeze line ~442

```markdown
| **Test** | AC-V1123-23 layer-state-persistence test (P4); V1.156 extends it to assert all three layers persist on both surfaces |
```

### at pre-freeze line ~444

```markdown
#### `wire_contracts_changed: false` verification (V1.156)
```

### at pre-freeze line ~446

```markdown
> **Version-scoped: this verification covers V1.156 only.** V1.156 P1 + P2 added no wire change **at that time**. **V1.200 supersedes the Moment-carrier half of this paragraph**: the Moment carrier is now the persisted `WorkOutline.scenes[]` / `beats[]` (V1.200 amendment above), a `wire_contracts_changed: true` change. The Brief-side statement (Work Timeline Brief reusing the era carrier, frontend-only) still holds, and the adapter-layer union extension remains frontend-local.
```

### at pre-freeze line ~448

```markdown
V1.156 P1 + P2 are **frontend-only**. Both layer additions extend existing adapter projection functions and layer-switcher segmented controls from two layers to three. No schema change, no daemon route change, no codegen diff, no `@42ch/nexus-contracts` version bump, no new DTO, no new conflict DTO. Concretely:
```

### at pre-freeze line ~450

```markdown
- **World Timeline Moment (P1) — superseded by V1.200 (carrier):** extends `TimelineLayer` from `'brief' | 'narrative'` to `'brief' | 'narrative' | 'moment'` in `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx`. The Moment projection reuses the same `OutlineSceneNodeData` / `OutlineBeatNodeData` carrier as Work Timeline Moment, composed client-side from bound Works' `GET /v1/daemon/works/{work_id}/outline` (V1.72 shipped). The V1.147 `GET /v1/daemon/worlds/{world_id}/timeline/events` route is **NOT** the read path (it reads Narrative-layer `narrative_timeline_events`, not scene/beat data). No daemon Rust change **in V1.156**. *V1.200 changes the carrier those outlines expose (`scenes[]` / `beats[]`) and therefore is a wire change.*
- **Work Timeline Brief (P2):** extends `WorkTimelineLayer` from `'narrative' | 'moment'` to `'brief' | 'narrative' | 'moment'` in `apps/web/src/components/canvas/work-timeline-canvas/work-timeline-canvas-adapter.tsx`. The Brief projection reuses the same `WorldKbGraphResponse.entities[block_type=era]` carrier as World Timeline Brief (V1.123), composed client-side from the bound World's `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73 shipped) via `Work.world_id` (already wired in the V1.123 P3 orchestrator). No daemon Rust change.
```

### at pre-freeze line ~464

```markdown
| **Brief-layer shape (V1.159)** | **Vertical time bands.** The Brief layer renders eras as stacked vertical bands (geological-strata metaphor) rather than the V1.123 flat horizontal era-sweep node list. Nesting depth is shown by **indented band offset** (each nesting level shifts the band right by one indent step). The `era_type` is shown by **per-type band coloring**. This supersedes the V1.123 "horizontal era sweep" Brief feel (layer-feel §2.2) as the Brief-layer rendering model; the V1.123 era *node card* visual language (`TimelineBriefEraChrome`, gold-bronze `--color-canvas-layer-brief-accent`) is the fallback for unknown/absent `era_type`. |
| **Carrier (read)** | Unchanged from V1.123: `WorldKbGraphResponse` from `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73). V1.159 consumes TWO arrays from the same response: `entities[block_type=era]` (era markers, now including optional `body.attributes.era_type`) AND `relationships` (to build the nesting tree). No new endpoint, no new field on the wire DTO (`era_type` rides freeform `body.attributes`; the relationship projection already carries `relation_type` + `custom_label`). |
```

### at pre-freeze line ~469

```markdown
| **Type coloring** | Per-`era_type` color via **DESIGN.md tokens** (not inline hex). A `--color-era-type-*` token family (or a hue ramp off the existing `--color-canvas-layer-brief-accent` spine) distinguishes the recommended types (`kingdom` / `age` / `epoch` / `period` / `sub-age`). **Unknown or absent `era_type` falls back to the default Brief accent** (`--color-canvas-layer-brief-accent`, gold-bronze) — legacy V1.123 flat-era data renders compatibly. Token values + dark-theme parity (`DESIGN.md` + `DESIGN.dark.md`) land in P1 implementation with architect sign-off. |
| **Read-only rendering** | The Brief-layer time band is a **read-only** rendering of the era tree. Selection may highlight a band and its descendants; cross-layer drill (Brief → Narrative) filters Narrative events within the selected era's `start_hint`/`end_hint` (V1.123 cross-layer rule preserved). No inline drag-to-reparent in the Brief layer (Non-Goal). |
```

### at pre-freeze line ~474

```markdown
| **Empty-state honesty** | Extends the V1.123/V1.156 Brief empty-state: no era entities → honest empty-state (unchanged). Eras present but **no nesting edges and/or no `era_type`** → compatible flat rendering (all roots at depth 0, default color) — the V1.159 rendering MUST NOT fabricate nesting from `canonical_name`/`start_hint` ordering; nesting comes **only** from `parent_era` relationship edges. |
```

### at pre-freeze line ~478

```markdown
#### `wire_contracts_changed: false` verification (V1.159)
```

### at pre-freeze line ~480

```markdown
V1.159 is **frontend-only** with **no schema/codegen change**, verified point by point:
```

### at pre-freeze line ~482

```markdown
- **`era_type`:** rides `KnowledgeEntry.body.attributes` (already freeform JSON since V1.123) — additive field, no `schemas/` change, no codegen diff, no `@42ch/nexus-contracts` version bump.
```

### at pre-freeze line ~484

```markdown
- **Read path:** `WorldKbGraphResponse.relationships` already ships in the V1.73 graph response; V1.159 merely *consumes* it in the Brief projection (previously the Brief layer only read `entities`). No new endpoint, no new field.
```

### at pre-freeze line ~574

```markdown
The canvas must run in both the service-served browser SPA and the desktop shell that embeds the same `apps/web/dist` (Electron since v1.192; the V1.70-era requirement named the retired macOS native shell and WKWebView — historical). Smoke tests must cover drag, pan/zoom, wheel/pinch gestures, keyboard focus movement, clipboard shortcuts, and inspector focus return in Chromium-like browsers and the desktop shell. Any desktop-only filesystem action still routes through host capabilities and structured service operations; the canvas webview never reads or writes raw local files directly.
```

### at pre-freeze line ~603

```markdown
This supersedes the V1.65 whole-file outline PUT model for canvas surfaces: V1.65 could save a whole outline document because the UI was a document editor (`web-ui.md` §13.1, §13.5). The canvas model must instead address and validate individual nodes/edges. V1.71 promotes the 3 Strategy operations to schema-backed Daemon API contracts and daemon-owned persistence; V1.72 and V1.73 extend the same structured patch boundary to Outline+Timeline and World KB entities/candidates. Future canvas operations, including typed World KB relationship editing, must reuse this boundary rather than reintroducing raw file writes.
```

### at pre-freeze line ~605

```markdown
> **V1.75 Canvas-Pivot shipped.** V1.75 ships the outline canvas as the sole outline authoring surface. The V1.65 chapter-page whole-document TipTap outline editor is retired after parity-close: chapter outline prose is edited inside the selected Chapter node inspector, not as a whole-document editor. `outline.patch_chapter` now accepts `set.content` for the chapter's outline prose notes; the daemon persists that prose to the chapter's `outline_path` markdown file while preserving the work-level `outline_revision` CAS in `Outlines/outline.md`. This keeps the structured write boundary intact: canvas patches mutate outline metadata/prose only and never touch AI-owned body files under `body_path` / `Stories/**`. The V1.65 `PUT /chapters/{n}/outline` write route and `PutChapterOutlineRequest` DTO are removed in `@42ch/nexus-contracts` 0.11.0; the `GET /chapters/{n}/outline` read route remains as the inspector content preview.
```

### at pre-freeze line ~607

```markdown
> **V1.200 scene/beat authoring shipped (DR-26).** The four scene/beat operations are members of the existing `outline.patch_structure` operation enum on the existing `POST /v1/daemon/works/{work_id}/outline/patch` route (`OutlinePatchStructureRequest` / `OutlinePatchResponse`) — **no new route, no new DTO, no new body-file writer**. `add_scene` requires an existing `chapter_id`, `add_beat` an existing `scene_id` of that Work; both require a non-blank `title`; an omitted `status` defaults to `drafted`. `remove_scene` cascades its beats in a single `outline_revision` bump; `remove_beat` removes only that beat; every operation resolves the owning chapter before mutating and reuses the published-chapter refusal (`ensure_chapter_not_published`); unknown ids reuse the existing not-found refusal with zero mutation; a stale `base_revision` returns the existing 409 `OutlineConflict` envelope. Server-minted `scn_<32-hex>` / `bet_<32-hex>` ids are observed through the canonical refetch — the response stays `OutlinePatchResponse` with empty side effects (no machine id in validation messages, no invented response field). The UI (Outline canvas) routes all four through the existing structure-patch mutation and conflict-capture pattern, so a 409 opens the existing Outline conflict modal. Both Timeline Moment layers remain read-only projections of the result.
```

### at pre-freeze line ~609

```markdown
> **V1.200 World-event binding authoring shipped (DR-26 / `DF-V1123-CROSS-SURFACE-BINDING` write half).** `bind_world_event` / `unbind_world_event` are members of the existing `timeline.patch_event` operation enum on the existing `POST /v1/daemon/works/{work_id}/timeline/patch` route (`TimelinePatchEventRequest` / `OutlinePatchResponse`, additive optional `world_event_id`) — **no new route, no new DTO, no new persistence** (the value is the outline frontmatter event record). Core validates the referent against the Work's **stored** bound World before writing (the `require_world_owner` guard first → 403 on a foreign/unowned World, then a `block_type=event` membership read of that World's KB), and refuses with the existing 422 outline-validation channel when there is no bound World, when the referent is missing, or when it is not an event. `bind_world_event` requires `event_id` + `world_event_id`; `unbind_world_event` requires only `event_id` and clears without re-reading the old referent (a deleted referent stays clearable), accepting an already-unbound valid event with exactly one `outline_revision` bump. Both reuse the shared published-chapter refusal for an event with an attached chapter; unknown ids keep the existing not-found refusal; a stale `base_revision` keeps the retained 409 `OutlineConflict` envelope with zero mutation — no new conflict DTO. The UI authors the binding on the **Outline** surface (outline Timeline event inspector, reusing the existing patch/conflict plumbing), with the control **disabled, not hidden**, when the Work has no bound World. The §3.3.3 V1.163 amendment's AC-V1163-7 no-author-write scope is superseded for this operation pair.
```

### at pre-freeze line ~627

```markdown
V1.69 freezes the minimal credible token names that V1.70 canvas implementation will need. Repo-root [`DESIGN.md`](../../../../DESIGN.md) + [`DESIGN.dark.md`](../../../../DESIGN.dark.md) stub these as commented LEVEL placeholders (formerly under `apps/web/` pre-V1.98); V1.70 assigns concrete values when implementing the canvas.
```

### at pre-freeze line ~641

```markdown
| `canvas-strategy-accent` | Strategy/preset-specific accent for state-machine nodes, inner-graph groups, and Strategy nav affordances; expected to derive from the purple family unless V1.70 changes the palette deliberately. |
```

### at pre-freeze line ~669

```markdown
Open V1.70 design points include the exact trigger verbs (for example, "Run Strategy", "Resume from Node", "Regenerate Branch", "Apply Idea to Node"), whether triggers enqueue schedule runs or call a direct orchestration advance endpoint, and how rollback/preview is shown before generated prose is committed. (V1.70 implement decision)
```

### at pre-freeze line ~671

```markdown
### 3.8 Relationship to V1.67 Daemon API convergence
```

### at pre-freeze line ~673

```markdown
The canvas is a heavy Daemon API consumer: every graph node binds to list/detail data, every inspector needs typed update operations, and every execution overlay depends on consistent session/status responses. Therefore V1.67 P0 is not incidental hygiene; it is the foundation for V1.68 canvas work:
```

### at pre-freeze line ~679

```markdown
The Canvas Shell must keep the `web-ui.md` §5 transport invariant: React components depend on `NexusClient`, not `fetch`, a host `invoke` bridge (e.g. the retired native `invoke`), or raw filesystem access.
```

### at pre-freeze line ~695

```markdown
- Keep runtime/file/CLI identifiers as **preset** in V1.67 and until an explicit breaking-change plan authorizes a rename. This includes preset YAML, existing Daemon API routes, generated DTO names, and CLI command names.
```

### at pre-freeze line ~730

```markdown
| **Work entry** (`/works/:workId`) | `/works/:workId` → **Outline** (V1.118 canvas-first work shell — **unchanged this iteration**) | **Outline (Timeline-companion)** | (Work-scoped peers per V1.118) |
```

### at pre-freeze line ~752

```markdown
## 5. Non-goals (V1.70 α)
```

### at pre-freeze line ~754

```markdown
- No promotion of canvas **writes** in V1.70 α. The shipped slice is read + visualization + live overlay + Idea-steer only.
- No schema/codegen/DTO lock for write operations in V1.70 α. Operation names and TypeScript-like interfaces above remain illustrative paper contracts until V1.71.
- No promotion of the outline+timeline canvas or World KB canvas in V1.70 α; both remain V1.71+ Draft surfaces.
- No removal/regression of the shipped V1.65 outline editor; canvas-pivot and node-granular outline edits are V1.71+ Draft scope.
- No CLI/spec rename of `preset` → `strategy` (breaking; deferred). V1.70 α adopts the terminology in UI/spec wording only.
```

### at pre-freeze line ~762

```markdown
- **V1.70 α — shipped**: Strategy Canvas read projection + visualization + live overlay + Idea-steer. This is the shipped successor slice to the retired body-editor roadmap, not the full three-surface canvas program.
```
