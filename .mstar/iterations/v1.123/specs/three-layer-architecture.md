# V1.123 — Three-Layer Timeline Architecture (iteration-scoped)

> **Status:** LOCKED — architect seat 2 (Phase 1 Review & Edit chain). Implements the locked direction: Brief · Narrative · Moment as Timeline's three zoom layers with World/Work domain split.
>
> **Compass reference:** [`../delivery-compass.md`](../delivery-compass.md) § Three-layer model → Data carrier strategy + Architect seat 2 LOCKED decisions + Risk Register.
>
> **Depends on:** V1.122 Timeline Canvas architecture ([`../../v1.122/specs/timeline-canvas-architecture.md`](../../v1.122/specs/timeline-canvas-architecture.md)) + V1.114 `CanvasSurfaceAdapter` recipe ([`canvas-strategy-surface.md` §3.3.1](../../../specs/canvas-strategy-surface.md)) + V1.73/V1.74 World KB DTOs + V1.72 Outline DTOs + V1.108 outline Scene/Beat UI projection.
>
> **Implements in:** plans P1 (`world-timeline-brief-narrative`) + P2 (`work-timeline-narrative-moment`) + P3 (`timeline-first-ia-deepening`) + P4 (`three-layer-zoom-experience`).
>
> **Peer product spec:** [`three-layer-product-spec.md`](./three-layer-product-spec.md) (product-manager seat 1).
>
> **Peer feel contract:** [`layer-feel-differentiation.md`](./layer-feel-differentiation.md) (P4 handoff).
>
> **Promotes to (at iteration-close):** Prospective (decided at iteration-close via `mstar-compound`) — likely `knowledge/architecture-patterns/three-layer-timeline-projection.md` + extensions to `canvas-surface-extraction-pattern.md` + `world-vs-work-canvas-scope.md`.

## 1. Purpose

LOCK the V1.123 three-layer Timeline architecture so P1–P4 implementers (and writing-specialist seat 3, then PM lock) have one normative reference for:

- Brief data carrier choice (one of Brief-on-KeyBlock / Brief-on-World / Brief-on-TimelineEvent) — §2.
- Moment data carrier choice (one of Moment-on-Outline / Moment-on-TimelineEvent) — §3.
- `wire_contracts_changed` verdict (`true` or `false`) + enumerated schema/DTO/route changes — §4.
- Daemon route plan (promote `GET /v1/daemon/worlds/{world_id}/timeline`? new Work Timeline route? compose?) — §5.
- Conflict policy per layer (reuse World KB / Outline / new Timeline-specific DTOs) — §6.
- Work Timeline peer surface adapter TypeScript contract (conforms to V1.114 `CanvasSurfaceAdapter`) — §7.
- Data composition per layer (projection mapping parallel to V1.122 §2.2) — §8.
- Explicit non-composition — what V1.123 does NOT compose — §9.

All verdicts cite codebase evidence. Codebase facts were verified by architect seat 2 on 2026-07-18 against the `iteration/v1.123` branch state.

> **Terminology note:** Throughout this document, "Narrative" refers to the Timeline layer (event-level), distinct from prose-craft narrative writing. "Moment" refers to the Timeline layer (scene/beat-precise), distinct from the existing Moment Context Assembly (session context packing) concept. See CONCEPTS.md § Narrative / § Moment for the full disambiguation.

## 2. Brief data carrier (LOCKED)

### 2.1 Candidates evaluated

| Candidate | Pros | Cons | Verdict |
|-----------|------|------|---------|
| **Brief-on-KeyBlock (`block_type=era`)** | Reuses V1.73 `WorldKbGraphResponse` as the sole graph source (filter `entities[block_type=era]`); reuses V1.73 `kb.patch_entity` write path verbatim; reuses `WorldKbConflictError` + `WorldKbValidationError` conflict DTOs; era semantics fit `KeyBlock.body.attributes` (`era_id`, `start_hint`, `end_hint`, `world_summary`) — same freeform-object pattern as `novel_category` / `game_bible_category` / `script_category`; World-scoped naturally (KeyBlocks are World-scoped per `entity-scope-model.md` §2); follows the established BlockType extension precedent (V1.54 added 7 values for game-bible; V1.55 added 3 for script — `schemas/common/common.schema.json` `BlockType` enum) | Requires adding `"era"` to the `BlockType` wire enum (single additive schema change in `schemas/common/common.schema.json`); `wire_contracts_changed: true` follows from this single change; manual `enum_conversions.rs` companion update required per `crates/nexus-contracts/AGENTS.md` | **LOCKED** ✅ |
| **Brief-on-World (new `WorldBrief` DTO + new route)** | Clean separation of Brief from KeyBlock taxonomy; could carry rich structured fields (`eras[]`, `summary`, `epoch_taxonomy`) | Highest wire + daemon churn: new schema (`schemas/daemon-api/...`), new daemon route (`GET /v1/daemon/worlds/{world_id}/brief` or extend `GET /v1/daemon/narrative/worlds/{world_id}`), new write boundary (Brief has no existing structured operation), new conflict DTO family (no existing conflict policy applies), daemon Rust changes in `crates/nexus-daemon-runtime/src/api/handlers/narrative.rs` or new handler module; breaks the "KeyBlock is the unit of World KB" invariant in `entity-scope-model.md` §2 | Rejected — wire + daemon churn disproportional to V1.123 MVP value |
| **Brief-on-TimelineEvent (`event_type=brief_marker`)** | Aligns with the existing World-scoped `TimelineEvent` domain table (`schemas/domain/timeline-event.schema.json`); `TimelineEventType` enum is already extensible (5 values today) | Requires **two** additive schema changes (extend `TimelineEventType` enum + extend the `TimelineEvent` row with `realizes_era`/`affected_key_block_ids` reuse) AND promoting `GET /v1/daemon/worlds/{world_id}/timeline` from `DF-V1122-DEEPER-WB` (daemon Rust route + handler) — because `TimelineEvent` rows are currently reachable only via internal `NarrativeGateway::get_timeline()` (`crates/nexus-narrative/src/gateway.rs`) and the `nexus.timeline.recent.get` host-tool capability (`crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs::execute_timeline_recent_get`), not via an HTTP route. Brief is conceptually a "world shape summary", not a discrete narrative event with causality (`caused_by_event_ids` / `affected_key_block_ids`) — semantic mismatch with the TimelineEvent row shape. Bigger blast radius than KeyBlock for the same product value | Rejected — semantic mismatch + double schema change + daemon route promotion required |

### 2.2 LOCKED carrier

**Brief-on-KeyBlock via new wire `BlockType = "era"`.**

### 2.3 Rationale (architect seat 2)

1. **Lowest wire-contract churn for the Brief layer.** A single additive enum value (`era`) in one schema (`schemas/common/common.schema.json`) plus its codegen + manual `enum_conversions.rs` companion. No new schema file, no new route, no new write boundary, no new conflict DTO. The change set is enumerated in §4.
2. **Reuses the proven V1.122 Timeline projection pattern.** The V1.122 `TimelineCanvasAdapter` (`apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx::projectTimelineGraph`) already splits `entities[]` by `block_type === 'event'` vs other kinds. Brief adds a third partition: `block_type === 'era'` → `TimelineEraNode` on the Brief when-axis. The adapter contract extension is purely additive (one new `layoutHint: 'brief'` value, one new node type).
3. **`era` semantics fit `KeyBlockBody.attributes`.** The V1.40 novel / V1.54 game-bible / V1.55 script precedent establishes that profile-specific categorization rides in `body.attributes.<profile>_category`, with `block_type` as the wire enum. `era` follows the same pattern: it is a cross-profile world-shape block type; its era-specific data (`era_id`, `start_hint`, `end_hint`, `world_summary`) lives in `body.attributes`. The existing `validate_body(block_type, body, ValidationMode)` API in `crates/nexus-kb/src/validation.rs` does not require changes — `era` is not subject to `novel_category` / `game_bible_category` / `script_category` enforcement, mirroring how `event` (the V1.122 Timeline event block type) is handled today.
4. **World-scoped naturally.** KeyBlocks are World-scoped per `entity-scope-model.md` §2 ("`World` ... Owns creative-work narrative state ... KeyBlocks, SourceAnchors"). Brief is a World-global layer per the locked direction — KeyBlock is the natural carrier.
5. **Write boundary reuses V1.73 verbatim.** `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73 shipped) already accepts `block_type` in its patch schema (`schemas/daemon-api/canvas/world-kb/world-kb-entity-patch.schema.json` lines 12–13: `"block_type": { "$ref": ".../BlockType" }`). Brief-era writes flow through this same route with `block_type: "era"` in the patch. No new daemon Rust handler is required; the existing `parse_block_type` (`crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:230`) parses any valid `BlockType` enum value, including the new `era` once added.
6. **Conflict policy reuses V1.73 verbatim.** Brief-era writes reuse `WorldKbConflictError` (HTTP 409, stale `expected_version` against `kb_key_blocks.revision`) + `WorldKbValidationError` (HTTP 422, domain-rule failure) from V1.73. See §6.

### 2.4 Codebase evidence

- `schemas/common/common.schema.json` — current `BlockType` enum (verified: 18 shipped values including `event` + 7 game-bible + 3 script). Adding `"era"` follows the same pattern as V1.54/V1.55.
- `schemas/daemon-api/canvas/world-kb/world-kb-entity-projection.schema.json` line 13 — `block_type: { $ref: .../BlockType }`. The projection already references the enum; no schema change in the projection file itself.
- `schemas/daemon-api/canvas/world-kb/world-kb-entity-patch.schema.json` lines 6, 12 — `kb.patch_entity` patch carries `block_type`; re-classification is already supported.
- `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:230` — `parse_block_type(s)` parses any valid `BlockType`; `era` will be accepted without daemon Rust change.
- `crates/nexus-contracts/src/enum_conversions.rs:96-121` — manual `BlockType::as_str()` companion; `Era` arm must be added in the same commit as the schema change per `crates/nexus-contracts/AGENTS.md`.
- `entity-scope-model.md` §5.1.1 — documents the `BlockType` SSOT; an architect Draft overlay (see this iteration's `entity-scope-model.md` Draft (V1.123) addition) extends the narrative taxonomy to include `era` cross-profile semantics.

## 3. Moment data carrier (LOCKED)

### 3.1 Candidates evaluated

| Candidate | Pros | Cons | Verdict |
|-----------|------|------|---------|
| **Moment-on-Outline (frontend-only projection of V1.108 Scene/Beat UI data)** | Reuses the existing V1.72 `GET /v1/daemon/works/{work_id}/outline` route (`crates/nexus-daemon-runtime/src/api/handlers/outline.rs::get_work_outline`) — zero new daemon route; reuses V1.72 `OutlineConflictError` + `OutlineValidationError` conflict DTOs for any future Work-scope writes; aligns with the locked direction that Moment is **Work-scoped**; V1.108 `OutlineSceneNodeData` / `OutlineBeatNodeData` (`apps/web/src/components/canvas/outline-canvas/rf-projection.ts:92+, V1.109 C2`) already provides the UI contract; when the WorkOutline wire later extends to expose scenes/beats (V1.124+), the Moment projection upgrades from fixture/empty-state to real wire data without breaking the adapter contract | The V1.72 `WorkOutline` wire (`schemas/daemon-api/canvas/outline/work-outline.schema.json`) does **not** expose scenes/beats today — confirmed by reading the schema (only `volumes[]`, `timeline_events[]`, `foreshadows[]`, `chapter_titles{}`). V1.108 Scene/Beat is **fixture-driven UI exploration** per the `rf-projection.ts` docstring ("The outline wire model has no scene data today"). So Moment-on-Outline means Moment layer shows honest empty-state in nearly all real Works in V1.123. This is acceptable per the product spec §4.5 (honest empty-state) and §4.3 (fall back to Narrative-default — see §7 below) | **LOCKED** ✅ |
| **Moment-on-TimelineEvent (`event_type=moment`)** | Aligns with the existing `TimelineEvent` domain aggregate; `TimelineEventType` enum is already extensible (`story_advance` / `state_update` / `fork_marker` / `official_progression` / `publish_marker` — `schemas/common/common.schema.json:159-163`); would carry `realizes_scene_id` / `realizes_beat_id` anchors in `affected_key_block_ids` or a new column | Requires (a) extending `TimelineEventType` enum (schema change), AND (b) adding a Work-scoped TimelineEvent route — either promote `GET /v1/daemon/worlds/{world_id}/timeline` (World-scoped — Moment is Work-scoped per locked direction, so this route is the wrong scope) OR add a new `GET /v1/daemon/works/{work_id}/timeline` (new daemon route + handler + persistence layer for Work-scoped events). The existing World-scoped `TimelineEvent` row has `world_id` not `work_id` (`schemas/domain/timeline-event.schema.json:17`); making it Work-scoped requires either a parallel Work-scoped table or a Work-binding column. Bigger wire + daemon + storage churn than V1.123 MVP can absorb | Rejected — wire + daemon + storage churn disproportional to V1.123 MVP value; deferred to V1.124+ as `DF-V1123-MOMENT-WIRE` (see §9) |

### 3.2 LOCKED carrier

**Moment-on-Outline (frontend-only projection; backend stays V1.72 `WorkOutline`).**

### 3.3 Rationale (architect seat 2)

1. **Zero wire-contract churn for the Moment layer.** No schema change, no daemon route change, no codegen diff, no version bump attributable to Moment. The V1.72 `WorkOutline` is the existing Work-scoped read model; `GET /v1/daemon/works/{work_id}/outline` is the existing route. The frontend `WorkTimelineCanvasAdapter` (§7) projects the same DTO in two layers (Narrative from `timeline_events[]`; Moment from outline Scene/Beat when present, else empty-state).
2. **Moment is Work-scoped (locked direction); the Work outline IS the Work-scoped source of truth.** The V1.122 architect's §2.4 decision ("Work outline timeline events are NOT composed onto the World Timeline surface") was grounded in this scope split. V1.123 honors that split by reading Work-scoped data from the Work-scoped route, not by inventing a parallel Work-scoped TimelineEvent table.
3. **Honest empty-state is product-locked.** The product spec §4.3 + §4.5 explicitly accepts "Moment empty → fallback Narrative with explanation" as a valid MVP state. The V1.122 precedent (§7 Honest empty-state) set the discipline: "do not fabricate chronology"; V1.123 extends it: "do not fabricate scenes".
4. **Forward-compatible with V1.124+ wire upgrade.** When the WorkOutline wire schema later extends to expose `scenes[]` / `beats[]` (V1.124+ tracked as `DF-V1123-MOMENT-WIRE`), the `WorkTimelineCanvasAdapter.projectGraphForLayer(graph, 'moment')` projection upgrades in-place — the same DTO read returns richer data; the adapter contract (§7) does not break. The V1.108 `OutlineSceneNodeData` / `OutlineBeatNodeData` UI types survive the wire upgrade because they are UI-only types that read from the projection, not from the wire directly.
5. **Read-only Moment layer in V1.123.** Edits to scenes/beats route through the V1.108 Outline adapter (V1.72 `outline.patch_chapter` / `outline.patch_structure`); the Work Timeline's Moment layer offers "Edit in Outline" affordance, not direct writes. This means V1.123 ships zero new write boundary for Moment — only an empty-state CTA pointing to Outline.

### 3.4 Codebase evidence

- `schemas/daemon-api/canvas/outline/work-outline.schema.json` — V1.72 wire shape: `volumes[]`, `timeline_events[]` (`event_id`/`title`/`description`/`realizes_chapter_id`), `foreshadows[]`, `chapter_titles{}`. No scenes/beats in the wire today.
- `apps/web/src/components/canvas/outline-canvas/rf-projection.ts:84-100+` — V1.108/V1.109 `OutlineSceneNodeData` docstring: "The data is **fixture-driven** — the outline wire model has no scene data today (architect-locked §5.2 Q1)."
- `crates/nexus-daemon-runtime/src/api/handlers/outline.rs:448-472` — `GET /v1/daemon/works/{work_id}/outline` (`get_work_outline`) returns `WorkOutline` from outline frontmatter. No daemon change required for V1.123 Moment layer.
- `crates/nexus-daemon-runtime/src/api/mod.rs:331-344` — Work outline routes already registered: `GET .../outline`, `POST .../outline/patch`, `POST .../timeline/patch`.

## 4. Wire-contracts-changed verdict (LOCKED)

### 4.1 Verdict

**`wire_contracts_changed: true`** — minimal, additive, single-file schema change attributable to the Brief carrier (`era` enum value). No daemon Rust route changes. No new DTOs. No conflict DTO additions.

### 4.2 Enumerated changes

**Schema changes (additive — exactly one file in `schemas/`):**

| File | Change | Type |
|------|--------|------|
| `schemas/common/common.schema.json` | `BlockType` enum gains `"era"` value (additive; follows V1.54 / V1.55 precedent for profile / cross-profile block-type extensions). Description updated to note `era` is a cross-profile world-shape marker. | Additive enum value |

**Generated code regen (automatic via `pnpm run codegen`):**

| File | Change |
|------|--------|
| `crates/nexus-contracts/src/generated/common.rs` | `BlockType` enum gains `Era` variant (codegen output) |
| `crates/nexus-contracts/src/generated/mod.rs` | Re-export of `BlockType::Era` (automatic) |
| `packages/nexus-contracts/src/generated/common.ts` | `BlockType` union type gains `'era'` |
| `packages/nexus-contracts/src/generated/index.ts` | Re-export (automatic) |

**Manual companion updates (per `crates/nexus-contracts/AGENTS.md`):**

| File | Change |
|------|--------|
| `crates/nexus-contracts/src/enum_conversions.rs` | Add `Self::Era => "era"` arm to `BlockType::as_str()` match (line ~96–121). Update any `FromStr for BlockType` if present. Run `cargo test -p nexus-contracts` to verify drift detection still passes. |
| `crates/nexus-contracts/tests/schema_drift_detection.rs` | Add an `entry!` for `schemas/common/common.schema.json` if not already registered (likely already registered — `BlockType` is a referenced enum). Run `./tooling/check-wire-drift.sh`. |

**Package version bump:**

| File | Change |
|------|--------|
| `packages/nexus-contracts/package.json` | Minor version bump (additive enum value; semver-minor). Exact bump from current version is implementer's call (e.g., `0.x.y → 0.(x+1).y`). |

**Daemon Rust changes:** **NONE.**

The existing handlers accept any valid `BlockType` value:
- `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:230` — `parse_block_type(s)` parses the new `era` value automatically once `BlockType::Era` exists.
- `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs::patch_entity` — accepts `block_type` in the patch body; validates via `validate_body(block_type, body, ValidationMode)`. `era` is not subject to Novel / GameBible / Script category enforcement; advisory warnings only (mirroring how `event` is handled today).

**Narrative crate changes:** **NONE.**

**New daemon HTTP routes:** **NONE.** See §5.

**New DTOs:** **NONE.** Brief reuses `WorldKbGraphResponse` / `WorldKbEntityProjection` / `WorldKbPatchEntityRequest` / `WorldKbPatchEntityResponse` / `WorldKbEntityPatch` / `WorldKbConflictError` / `WorldKbValidationError`. Moment reuses `WorkOutline`. Work Timeline adapter adds frontend-only types (`WorkTimelineGraph`, `WorkTimelineNodeData`, `WorkTimelineEdgeData`) that are **not** wire DTOs.

**New conflict DTOs:** **NONE.** See §6.

### 4.3 P1 + P2 verification contract

The V1.122 eight-point verification gate (`timeline-canvas-architecture.md` §9.2) is updated for V1.123's `wire_contracts_changed: true` verdict. P1 + P2 implementers verify:

| # | Command | Expected (V1.123) |
|---|---------|--------------------|
| 1 | `git diff --stat schemas/` against `iteration/v1.123` base | **exactly one file changed:** `schemas/common/common.schema.json` (BlockType enum + `era` value) |
| 2 | `git diff --stat schemas/common/common.schema.json` | single additive enum value `"era"` in `BlockType` |
| 3 | `git diff --stat crates/nexus-contracts/src/enum_conversions.rs` | `Era` arm added to `BlockType::as_str()` |
| 4 | `pnpm run codegen` on integration branch → `git status` under `**/generated/` | regen of `BlockType` in Rust + TS only; no other generated file touched |
| 5 | `jq '.version' packages/nexus-contracts/package.json` | minor version bump from pre-iteration |
| 6 | `git diff --stat crates/nexus-daemon-runtime/src/api/` | **empty** (no new routes / handlers) |
| 7 | `rg -n '"era"' schemas/` | exactly one hit in `schemas/common/common.schema.json` BlockType enum |
| 8 | `./tooling/check-wire-drift.sh` | passes (schema drift detection green) |

If any sub-step fails, the implementer MUST STOP and escalate to architect before marking the plan Done.

## 5. Daemon route plan (LOCKED)

### 5.1 Verdict

**Compose from existing routes. DO NOT promote `GET /v1/daemon/worlds/{world_id}/timeline`. DO NOT add a new Work Timeline route.**

### 5.2 Rationale (architect seat 2)

1. **Both Brief and Narrative layers on World Timeline compose from `GET /v1/daemon/worlds/{world_id}/kb/graph`** (V1.73 shipped). The frontend adapter filters `entities[]` by `block_type`:
   - Brief layer → `entities.filter(e => e.block_type === 'era')` → `TimelineEraNode` on the Brief when-axis.
   - Narrative layer → `entities.filter(e => e.block_type === 'event')` → `TimelineEventNode` on the Narrative when-axis (V1.122 unchanged).
   - Context clusters → `entities.filter(e => !['event', 'era'].includes(e.block_type))` → `TimelineKeyBlockNode` off-axis (V1.122 unchanged).
2. **Both Narrative and Moment layers on Work Timeline compose from `GET /v1/daemon/works/{work_id}/outline`** (V1.72 shipped). The frontend adapter partitions `WorkOutline`:
   - Narrative layer → `outline.timeline_events[]` → `WorkTimelineEventNode` on the Work Timeline Narrative when-axis.
   - Moment layer → outline Scene/Beat data when the WorkOutline wire exposes them (V1.124+); until then, fixture-only or honest empty-state.
3. **Optional Work Timeline sidecar for bound World events**: the frontend MAY additionally call `GET /v1/daemon/worlds/{world_id}/kb/graph` for the World bound to the Work (`Work.world_id`) to render bound World events on the Work Timeline Narrative layer at Work resolution. This is a **client-side composition**, not a new route. The adapter does not introduce a server-side join.
4. **`DF-V1122-DEEPER-WB` stays open.** Promoting `GET /v1/daemon/worlds/{world_id}/timeline` (the World-scoped `TimelineEvent` HTTP route) is **not required** for V1.123 PMF because Brief rides KeyBlock and Narrative rides KeyBlock (`block_type=event`). The route promotion remains deferred to V1.124+ — see §5.4.

### 5.3 Codebase evidence

- `crates/nexus-daemon-runtime/src/api/mod.rs` — confirms the only timeline-related routes registered are `GET /v1/daemon/works/{work_id}/outline` + `POST /v1/daemon/works/{work_id}/timeline/patch` (Work-scoped) + `GET /v1/daemon/worlds/{world_id}/kb/graph` (World KB). No `GET /v1/daemon/worlds/{world_id}/timeline` route exists.
- `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:886-915` — `nexus.timeline.recent.get` is a host-tool capability (ACP/orchestration), not an HTTP route. The World-scoped `TimelineEvent` row remains reachable only via internal `NarrativeGateway::get_timeline()` (`crates/nexus-narrative/src/gateway.rs`) and this host-tool.
- `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx:300-373` — `projectTimelineGraph` already partitions `WorldKbGraphResponse.entities[]` by `block_type`; adding a third partition for `era` is purely additive.

### 5.4 `DF-V1122-DEEPER-WB` status update

`DF-V1122-DEEPER-WB` (Deeper World-building on Timeline — promote `GET /v1/daemon/worlds/{world_id}/timeline`) status: **STAYS OPEN** with the note: "V1.123 P1 composes Brief + Narrative from existing `kb/graph` route; route promotion deferred to V1.124+ because V1.123 carrier choice (Brief-on-KeyBlock) does not require the World-scoped `TimelineEvent` HTTP route." The architect's update to the tracker row is recorded in §(tracker update) of this iteration.

## 6. Conflict policy (LOCKED)

### 6.1 Verdict

**Reuse existing conflict DTOs per layer domain. NO new Timeline-specific conflict DTO. NO new write boundary for Moment (read-only in V1.123).**

### 6.2 Per-layer conflict policy

| Layer | Write boundary | Conflict DTOs | Reuse source |
|-------|----------------|---------------|--------------|
| **Brief** (World Timeline, World scope) | `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73) with `block_type: "era"` in patch | `WorldKbConflictError` (HTTP 409, stale `expected_version` against `kb_key_blocks.revision`) + `WorldKbValidationError` (HTTP 422, domain-rule failure) | V1.73 verbatim |
| **Narrative** (World Timeline, World scope — V1.122 unchanged) | `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (`kb.patch_entity`, V1.73) with `block_type: "event"` in patch | `WorldKbConflictError` + `WorldKbValidationError` | V1.73 verbatim (V1.122 §5 preserved) |
| **Narrative** (Work Timeline, Work scope — new in V1.123 P2) | `POST /v1/daemon/works/{work_id}/timeline/patch` (`timeline.patch_event`, V1.72) — add_event / remove_event / attach_event_to_chapter / link_foreshadow / unlink_foreshadow | `OutlineConflictError` (HTTP 409, stale `outline_revision`) + `OutlineValidationError` (HTTP 422, domain-rule failure) | V1.72 verbatim |
| **Moment** (Work Timeline, Work scope — new in V1.123 P2) | **NONE in V1.123** — Moment is read-only. Edits route through Outline (`POST .../outline/patch` — `outline.patch_chapter` / `outline.patch_structure`, V1.72) via "Edit in Outline" affordance | (n/a — no direct writes; Outline adapter owns the write when the user navigates there) | V1.72 verbatim when the user lands on Outline |

### 6.3 Rationale (architect seat 2)

1. **No new conflict DTO family.** All four conflict DTOs (`WorldKbConflictError`, `WorldKbValidationError`, `OutlineConflictError`, `OutlineValidationError`) are shipped V1.72/V1.73 wire types. V1.123 adds zero new conflict DTOs to `schemas/`.
2. **World scope reuses World KB DTOs.** Brief + Narrative on World Timeline both edit World-scoped KeyBlocks through `kb.patch_entity`. The conflict path is identical to V1.122 (read canonical `WorldKbGraphResponse`, offer Use current / Reapply my edit / Review side-by-side) — see V1.122 `timeline-canvas-architecture.md` §5 for the canonical flow.
3. **Work scope reuses Outline DTOs.** Work Timeline Narrative writes route through `timeline.patch_event` (V1.72), which already returns `OutlineConflictError` + `OutlineValidationError` via the `ErrorResponse` envelope. The adapter parses these via the same `extractConflict` pattern as V1.122 (`apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx:716-753`), specialized for Outline error codes (`outline_conflict`, `outline_validation_failed`).
4. **Moment read-only eliminates the write-boundary design surface for V1.123.** The Work Timeline Moment layer offers "Edit in Outline" — when clicked, the user navigates to the Outline surface (`/works/:workId`), where the V1.108 Outline adapter owns the write. This keeps V1.123's Work Timeline MVP focused on projection + display + cross-layer navigation, not on a parallel scene/beat write boundary that would require its own conflict DTO family.

### 6.4 `adaptConflict` adapter method

The V1.122 `TimelineCanvasAdapter.adaptConflict(error)` returns `null` and the orchestrator renders `WorldKbEntityConflictModal` from the structured `TimelineConflictInfo` (`timeline-canvas-adapter.tsx:601-608`). V1.123 preserves this pattern for the World Timeline Brief + Narrative layers (extend the same `extractTimelineConflict` parse to recognize Brief-era patches verbatim — no code change beyond accepting `block_type: "era"` in the patch).

The V1.123 `WorkTimelineCanvasAdapter.adaptConflict(error)` follows the same orchestrator-owned modal pattern: returns `null`; the orchestrator renders an Outline-flavored conflict modal (`OutlineConflictModal` — V1.72 reused component, or a thin Work-Timeline-flavored wrapper) from a structured `WorkTimelineConflictInfo`. The error codes parsed are `outline_conflict` + `outline_validation_failed` (V1.72).

## 7. Work Timeline peer surface adapter contract (LOCKED)

### 7.1 TypeScript signature

```ts
// apps/web/src/components/canvas/work-timeline-canvas/work-timeline-canvas-adapter.tsx

import type { MutableRefObject } from 'react';
import type { Edge, Node } from '@xyflow/react';

import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import type {
  WorkOutline,
  ChapterStatus,
} from '@42ch/nexus-contracts';

// ─── Public types (architect-locked §7) ─────────────────────────────────────

/**
 * Single graph source — no wrapper, no join (mirrors V1.122 §3.1).
 *
 * The Work Timeline Narrative + Moment layers both project from the V1.72
 * `WorkOutline` wire DTO. The orchestrator MAY additionally fetch the bound
 * World's `WorldKbGraphResponse` for the Work Timeline Narrative layer's
 * "bound World events" cross-surface binding (P3), but the adapter's
 * `projectGraphForLayer` accepts `WorkOutline` as its sole graph payload.
 */
type WorkTimelineGraph = WorkOutline;

/**
 * Layer supported on the Work Timeline surface.
 *
 * - 'narrative' — Work-scoped events from `outline.timeline_events[]`.
 * - 'moment'    — Scene/Beat precision from outline Scene/Beat data
 *                 (wire-driven when WorkOutline exposes scenes/beats in
 *                 V1.124+; fixture-only or empty-state until then).
 */
type WorkTimelineLayer = 'narrative' | 'moment';

/**
 * Node data payload for the Work Timeline surface.
 *
 * `WorkOutline` is a flat wire DTO; the adapter owns the partition into
 * per-layer node data. The `[key: string]: unknown` index signature satisfies
 * React Flow's `Node<TNodeData extends Record<string, unknown>>` constraint.
 */
interface WorkTimelineNodeData {
  [key: string]: unknown;
  workId: string;
  /** Discriminator for node rendering: 'event' for Narrative layer; 'scene' / 'beat' for Moment layer. */
  nodeKind: 'event' | 'scene' | 'beat';
  /** Stable node id source (event_id for Narrative; sceneId/beatId for Moment). */
  nodeId: string;
  /** Human-readable label (event title / scene title / beat title). */
  label: string;
  /** Optional description (event description / scene summary / beat summary). */
  description?: string;
  /** Chapter this node realizes (event realizes_chapter_id; scene/beat chapter_id). */
  realizesChapterId?: number;
  /** Manuscript anchor badge data (Moment layer — chapter/scene link). */
  manuscriptAnchor?: {
    chapterId: number;
    sceneId?: string;
    beatId?: string;
  };
}

/**
 * Edge data payload for the Work Timeline surface.
 *
 * Reuses the V1.72 outline edge semantics without inventing a new edge DTO
 * family. `realizes_event` / `foreshadows` are Work-outline projection labels
 * (same as V1.108); the Work Timeline surface introduces NO new edge types.
 */
type WorkTimelineEdgeData = {
  /** Edge relation: 'realizes_event' (chapter → event) or 'foreshadows' (event → event). */
  relation: 'realizes_event' | 'foreshadows';
  /** Stable edge id source (event_id pair). */
  sourceEventId?: string;
  targetEventId?: string;
};

// ─── Adapter context ────────────────────────────────────────────────────────

/**
 * Mutable context supplied by the orchestrator so the adapter can render
 * inspectors / wire "Edit in Outline" navigation without closing over stale
 * values. Read at render time from the ref; the adapter object itself stays
 * stable across renders (V1.114 §3.3.1 "stable factory that reads from a
 * mutable `React.RefObject` context").
 *
 * The Work Timeline adapter is read-only in V1.123. Writes route through the
 * Outline surface (`outline.patch_chapter` / `outline.patch_structure`) via
 * the `onEditInOutline` callback. The orchestrator owns the navigation.
 */
interface WorkTimelineCanvasAdapterContext {
  workId: string;
  /** Optional bound World id (for P3 cross-surface navigation to World Timeline). */
  worldId?: string;
  /**
   * "Edit in Outline" hand-off — fired when the user clicks the affordance on
   * a Moment scene/beat or a Narrative event. The orchestrator navigates to
   * `/works/:workId` (Outline surface) with state hinting which node to focus.
   * The adapter performs NO writes (architect-locked §6 — Moment read-only in
   * V1.123; Narrative writes route through Outline).
   */
  onEditInOutline?: (node: Node<WorkTimelineNodeData>) => void;
  /**
   * P3 cross-surface navigation hand-off — fired when the user clicks "View on
   * World Timeline" on a Narrative event with a bound World event. The
   * orchestrator navigates to `/worlds/:worldId/timeline?layer=narrative&event=:eventId`.
   * Undefined when no World is bound or when P3 is not yet shipped.
   */
  onViewOnWorldTimeline?: (node: Node<WorkTimelineNodeData>) => void;
}

type WorkTimelineCanvasAdapter = CanvasSurfaceAdapter<
  WorkTimelineGraph,
  WorkTimelineNodeData,
  WorkTimelineEdgeData
>;

// ─── Layer extension (V1.123 P2 — additive over V1.114 CanvasSurfaceAdapter) ─

/**
 * The Work Timeline adapter extends the base V1.114 CanvasSurfaceAdapter with
 * layer-aware projection. The base `projectGraph(graph)` MUST delegate to the
 * active layer so `useCanvasSurface()` (V1.114) composes unchanged.
 *
 * `defaultLayer: 'narrative'` — architect UX-risk note (§7.3 below): the
 * V1.72 `WorkOutline` wire has no Scene/Beat data today, so Moment-default
 * would surface persistent empty-state in nearly all real Works. Narrative-
 * default with Moment one click away is the safer V1.123 position. The
 * product spec §4.3 authorized this fallback with documentation in P2.
 */
interface WorkTimelineLayerAdapter extends WorkTimelineCanvasAdapter {
  surfaceKind: 'work-timeline';
  /** Default layer on Work Timeline entry. Architect-locked: 'narrative' (§7.3). */
  defaultLayer: 'narrative';
  /** Project the graph for a specific layer. */
  projectGraphForLayer(
    graph: WorkTimelineGraph,
    layer: WorkTimelineLayer,
  ): { nodes: Node<WorkTimelineNodeData>[]; edges: Edge<WorkTimelineEdgeData>[] };
  /** Inherited from V1.114 CanvasSurfaceAdapter — MUST delegate to active layer. */
  projectGraph(graph: WorkTimelineGraph): { nodes: Node<WorkTimelineNodeData>[]; edges: Edge<WorkTimelineEdgeData>[] };
}

// ─── Stable factory ─────────────────────────────────────────────────────────

/**
 * Build a stable Work Timeline canvas adapter that reads mutable values from
 * the supplied context ref (V1.114 §3.3.1 "stable factory that reads from a
 * mutable `React.RefObject` context").
 *
 * Mirrors `createTimelineCanvasAdapter` in V1.122 timeline-canvas-adapter.tsx.
 * The returned object MUST stay referentially stable across renders —
 * `useCanvasSurface` memoises on `adapter`.
 */
declare function createWorkTimelineCanvasAdapter(
  ctxRef: MutableRefObject<WorkTimelineCanvasAdapterContext>,
): WorkTimelineLayerAdapter;
```

### 7.2 Conformance rules

- The adapter **MUST** conform to `CanvasSurfaceAdapter<WorkTimelineGraph, WorkTimelineNodeData, WorkTimelineEdgeData>` from `specs/canvas-strategy-surface.md` §3.3.1.
- `WorkTimelineCanvasAdapter` (via `WorkTimelineLayerAdapter`) is the **only** adapter permitted to register `surfaceKind: "work-timeline"` in `CanvasShell` / `canvas-nav`.
- The adapter object **MUST** stay stable across renders (V1.114 §3.3.1 "stable factory that reads from a mutable `React.RefObject` context").
- The adapter **MUST NOT** introduce new edge types beyond the V1.72 outline edge semantics (`realizes_event`, `foreshadows`). `WorkTimelineEdgeData` is a frontend-only projection type; it does NOT introduce a wire edge DTO.
- `layoutOptions.direction` defaults: `'LR'` for Narrative layer (parallel to V1.122 World Timeline); `'TB'` for Moment layer (per `layer-feel-differentiation.md` §2.4 vertical scene-stack preference). The adapter MAY switch `layoutOptions` based on the active layer.
- `summarizeGraph(graph)` MUST include the honest empty-state disclaimer when the active layer has no projectable data (e.g., "Work Timeline Moment: no scene/beat data yet — switch to Narrative or add scenes in Outline.").

### 7.3 Default layer: Narrative (architect UX-risk override)

**LOCKED default: `defaultLayer: 'narrative'` on Work Timeline entry, with Moment one click away.**

This **overrides** the product spec §4.3 preference ("Moment when Scene/Beat data exists; else Narrative"). Rationale (UX risk):

1. **The V1.72 `WorkOutline` wire has no Scene/Beat data today.** Confirmed by reading `schemas/daemon-api/canvas/outline/work-outline.schema.json` — the wire exposes `volumes[]`, `timeline_events[]`, `foreshadows[]`, `chapter_titles{}`, but no `scenes[]` or `beats[]`. V1.108 Scene/Beat is fixture-driven UI exploration only.
2. **Moment-default would surface persistent empty-state in nearly all real Works.** Authors opening Work Timeline would see "No scene/beat data yet" on every Work in V1.123. This erodes trust in the new surface and contradicts the "Moment is Work-hero" promise.
3. **Narrative-default gives immediate value.** `WorkOutline.timeline_events[]` ships real data in V1.72; the Narrative layer is populated for any Work with outline events. Defaulting to Narrative lets authors see Work-scoped events immediately, with the Moment layer one click away via the layer switcher.
4. **Product spec §4.3 explicitly authorized this fallback.** *"If implementer evidence shows Moment-default confuses empty Works, fall back to Narrative-default with Moment one click away — document in P2 plan; do not flip Work **entry** away from Outline."* The implementer evidence is the wire gap in §3.4 above; this LOCK documents the override per the product authorization.
5. **Work entry still defaults to Outline.** This override does NOT flip `/works/:workId` away from Outline (V1.118 preserved). It only sets the default **layer within Work Timeline** when the user navigates there.

The product spec's "Moment is Work-hero" promise is preserved: the Moment layer **exists**, is **reachable**, is **one click away** from Narrative, and has its own feel differentiation (P4). When the WorkOutline wire extends to expose scenes/beats (V1.124+ tracked as `DF-V1123-MOMENT-WIRE`), the default may flip to Moment at that time without breaking the adapter contract.

### 7.4 Codebase evidence

- `apps/web/src/components/canvas/canvas-surface-adapter.ts:6-15` — current `CanvasSurfaceKind` enum: `'strategy' | 'outline' | 'world-kb-entities' | 'world-kb-relationships' | 'timeline'`. Adding `'work-timeline'` is additive.
- `apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx:582-622` — V1.122 `createTimelineCanvasAdapter(ctxRef)` factory pattern; the Work Timeline adapter mirrors this recipe.
- `apps/web/src/components/canvas/outline-canvas/rf-projection.ts:92-100+` — V1.108 `OutlineSceneNodeData` UI type; survives the V1.124+ wire upgrade because it reads from projection, not wire directly.

## 8. Data composition per layer (LOCKED)

Parallel to V1.122 `timeline-canvas-architecture.md` §2.2.

| Layer | Data source | Endpoint | Projection mapping |
|-------|-------------|----------|---------------------|
| **Brief** (World Timeline — hero) | `WorldKbGraphResponse.entities[block_type=era]` (new `era` block type — §2) | `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73 shipped, unchanged) | Filter `entities.filter(e => e.block_type === 'era')` → `TimelineEraNode` on the Brief when-axis; era markers carry `body.attributes.{era_id, start_hint, end_hint, world_summary}`. Optional world-shape summary header above the axis from the same data. |
| **Narrative** (World Timeline — peer, V1.122 preserved) | `WorldKbGraphResponse.entities[block_type=event]` (V1.122 unchanged) | `GET /v1/daemon/worlds/{world_id}/kb/graph` (V1.73 shipped, unchanged) | V1.122 §2.2 preserved verbatim — see `timeline-canvas-architecture.md` §2.2 row 1. |
| **Narrative** (Work Timeline — peer, new in P2) | `WorkOutline.timeline_events[]` (V1.72 shipped) + optional bound World events from `WorldKbGraphResponse` (client-side composition) | `GET /v1/daemon/works/{work_id}/outline` (V1.72 shipped, unchanged) + optional `GET /v1/daemon/worlds/{world_id}/kb/graph` for bound World events | Project `timeline_events[]` → `WorkTimelineEventNode` on the Work Timeline Narrative when-axis; sort by `realizes_chapter_id` then `event_id`. Bound World events (when the Work has a `world_id`) render as muted badges on the same axis (client-side composition — no new server join). |
| **Moment** (Work Timeline — hero-on-demand, new in P2) | Outline Scene/Beat data from `WorkOutline` when the wire exposes them (V1.124+); until then, fixture-only or honest empty-state | `GET /v1/daemon/works/{work_id}/outline` (V1.72 shipped — Moment reads from the same route; future wire extension is additive) | Project Scene/Beat data → `WorkTimelineSceneNode` / `WorkTimelineBeatNode` in a vertical scene-stack (TB layout direction per `layer-feel-differentiation.md` §2.4); manuscript-anchor badges when anchor data exists. Honest empty-state when no Scene/Beat data — CTA "Edit in Outline" routes the user to `/works/:workId` (Outline surface). |

### 8.1 Cross-layer binding (informational — P3 owns the surface jump)

- **World Timeline Brief era → World Timeline Narrative events**: a Brief era may declare a time span (`body.attributes.start_hint`, `body.attributes.end_hint`); when the user clicks an era, the Narrative layer MAY filter to events whose `body.attributes.occurred_at` falls within that span. This is a within-surface layer drill, not cross-surface navigation.
- **Work Timeline Narrative event → World Timeline Narrative event** (P3): when a Work Timeline Narrative event is bound to a World event (via the bound World's `WorldKbGraphResponse`), the "View on World Timeline" affordance navigates to `/worlds/:worldId/timeline?layer=narrative&event=:eventId`. P3 owns this cross-surface jump.
- **Work Timeline Moment scene/beat → Outline chapter** (within-Work): "Edit in Outline" navigates to `/works/:workId` with state hinting which scene/beat to focus. This is a within-Work surface jump; the Outline adapter owns the actual write.

## 9. Explicit non-composition (architect decision)

Parallel to V1.122 `timeline-canvas-architecture.md` §2.4.

V1.123 does NOT compose:

- **World Moment layer** (Moment-on-World). Moment is Work-scoped per the locked direction. World Timeline shows Brief + Narrative only. Tracked as `DF-V1123-WORLD-MOMENT` (V1.124+ if ever promoted). Rationale: World history is read at era/event granularity; scene-precision within World history is the Outline's job today.
- **Work Brief layer** (Brief-on-Work). Brief is World-global per the locked direction. Work Timeline shows Narrative + Moment only. Tracked as `DF-V1123-WORK-BRIEF` (V1.124+ if ever promoted). Rationale: Work-level world-shape context is the Work outline's job today; adding a Brief projection per Work would duplicate the World Brief.
- **Cross-World Timeline merge** (read-write merged view). The P3 global Timeline view is **read-only overview** — recent Timeline activity across Worlds; no merged editing. Tracked as `DF-V1123-GLOBAL-TIMELINE-MERGE` (V1.125+). Rationale: cross-World narrative merge needs a separate merge semantics that V1.123 cannot absorb.
- **World-scoped `TimelineEvent` HTTP route promotion** (`GET /v1/daemon/worlds/{world_id}/timeline`). V1.123 composes Brief + Narrative from `kb/graph` instead. Tracked under `DF-V1122-DEEPER-WB` (stays open with V1.124+ target). Rationale: see §5.
- **Work-scoped timeline-event route** (`GET /v1/daemon/works/{work_id}/timeline`). V1.123 composes Work Timeline Narrative + Moment from the existing `outline` route instead. No tracker row needed (this is a non-decision — the existing outline route suffices). Rationale: see §5.
- **Moment layer writes** (scene/beat edits from the Work Timeline surface). V1.123 ships Moment as **read-only**. Edits route through Outline (`outline.patch_chapter` / `outline.patch_structure`). Future writes are V1.124+ (`DF-V1123-MOMENT-WIRE` covers wire + write boundary). Rationale: see §6.2.
- **World Timeline ↔ Work Timeline cross-surface merged rendering**. Each Timeline surface renders its own layers; cross-surface is **navigation** (P3 owns the jump), not merged rendering. Rationale: merged rendering would require a composite graph source that V1.123 does not introduce.
- **Rich era taxonomy** (kingdoms, ages, sub-ages beyond era markers). V1.123 Brief MVP uses `block_type=era` markers as the only Brief granularity. Tracked as `DF-V1123-ERA-TAXONOMY` (V1.124+).
- **Fork marker nodes on either Timeline surface.** V1.122 §8 forbade `TimelineForkMarkerNode` on the World Timeline; V1.123 extends the same prohibition to the Work Timeline. Fork data continues to render as optional header-badge chrome from the `WorldState` sidecar (`GET /v1/daemon/narrative/worlds/{world_id}`) on the World Timeline; the Work Timeline has no Fork chrome in V1.123. Fork create/merge UI remains `DF-V1122-FORK-UI`.

## 10. References

- V1.122 `iterations/v1.122/specs/timeline-canvas-architecture.md` — Narrative layer baseline (§2 data composition, §3 adapter contract, §4 write boundary, §5 conflict policy, §9 wire-contracts false verification).
- V1.114 `specs/canvas-strategy-surface.md` §3.3.1 — `CanvasSurfaceAdapter` recipe (adapter interface, `useCanvasSurface()` composition, `useAutoLayout()` integration, "add a new canvas surface" recipe steps).
- V1.122 `specs/canvas-strategy-surface.md` §3.3.2 — Timeline peer surface amendment (Draft text V1.122 — preserved; V1.123 Draft overlay adds Work Timeline peer + layer switchers — see `canvas-strategy-surface.md` Draft (V1.123) overlay).
- V1.73 / V1.74 — `WorldKbGraphResponse`, `WorldKbEntityProjection`, `WorldKbRelationshipProjection`, `WorldKbPatchEntityRequest` / `Response`, `WorldKbEntityPatch`, `WorldKbConflictError`, `WorldKbValidationError`, `WorldKbRelationshipKind`, `WorldKbSourceAnchorProjection`.
- V1.72 — `WorkOutline`, `OutlineConflictError`, `OutlineValidationError`, `outline.patch_structure`, `outline.patch_chapter`, `timeline.patch_event`.
- V1.108 / V1.109 C2 — `OutlineSceneNodeData`, `OutlineBeatNodeData` (`apps/web/src/components/canvas/outline-canvas/rf-projection.ts:92+`).
- V1.26 — `WorldState` narrative read model for Fork-badge sidecar chrome.
- `entity-scope-model.md` §1.1 — `World > Timeline > Event > Moment` scope hierarchy (unchanged in V1.123).
- `entity-scope-model.md` §5.1.1 — `BlockType` wire enum SSOT; V1.123 Draft overlay extends the narrative taxonomy to include `era` cross-profile semantics.
- `crates/nexus-contracts/src/enum_conversions.rs` — manual companion for `BlockType::as_str()`; `Era` arm added in the same commit as the schema change.
- `knowledge/conventions/wire-contracts-frozen-verification.md` — verification discipline; V1.123 verdict is `true` (single additive enum value), so the "frozen" path does not apply — see §4.3 for the updated verification gate.
- `knowledge/architecture-patterns/canvas-surface-extraction-pattern.md` (V1.122 compound) — surface extraction recipe.
- `knowledge/architecture-patterns/world-vs-work-canvas-scope.md` (V1.122 compound) — World vs Work scope invariant; V1.123 honors it (Brief is World-scoped KeyBlock; Moment is Work-scoped Outline projection).
