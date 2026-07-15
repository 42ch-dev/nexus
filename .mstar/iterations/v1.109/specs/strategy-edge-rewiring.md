# Strategy Spatial Edge Rewiring — Primary Spec (V1.109 P1)

**Status:** Draft — product-complete (§5.1 product-manager)  
**Tier:** Must (P1)  
**Plan:** `2026-07-11-v1.109-strategy-edge-rewiring`  
**Compass:** `../v1.109/delivery-compass.md`  
**Normative master:** `.mstar/specs/canvas-strategy-surface.md` (§1.1 Strategy β writes, §3.2 DAG mapping, §3.5 write boundary + conflict policy, §4.4 pointer alternatives, §4.5 user stories)

## Product outcome

Authors can **draw and reconnect Strategy transitions on the graph** — not only edit them in the inspector. Strategy reaches the same “edit on graph” parity that Outline (V1.108 C1 foreshadow connect) and World KB (relationship connect) already enjoy, while reusing the shipped `strategy.patch_transition` wire op (V1.71).

**User-visible win:** Drag from a state handle to another state creates a draft transition; the edge inspector collects condition/label before commit; stale revision still opens the conflict modal; existing edges can be reconnected by drag; keyboard-only authors get a dialog path (choose source → target → edge kind).

## Problem

V1.71 shipped Strategy **write-boundary** ops including `strategy.patch_transition`. The Strategy canvas renders the graph and can edit edges via inspector, but **spatial `onConnect` is missing** — authors cannot draw transitions the way they rewire World KB relationships or expect from a DAG editor. V1.108 compass Roadmap Position named Strategy spatial edge rewiring as next-iteration work.

## Product story

**Who:** Authors steering Works via the Strategy (preset) canvas — the workflow graph that drives creative execution.

**Why draw transitions on the graph:**
- A Strategy **is** a state machine; authors reason in space (“this branch goes to wait”, “rewire fail to retry”).
- Inspector-only edge creation forces mental translation between spatial layout and form fields — error-prone for multi-branch graphs.
- Outline and World KB already teach “connect on canvas”; Strategy lagging breaks cross-canvas muscle memory.
- Conflict-safe commit still matters: draft → inspect → `patch_transition` with OCC, never silent last-write-wins.

**Narrative:** Add React Flow handles + `onConnect` draft edges, reuse edge inspector + conflict modal, support reconnection, and ship §4.4 keyboard/dialog alternative so pointer is not the only path.

## Goals

1. Spatial `onConnect` creates a **draft** transition edge (not silent auto-commit).
2. Draft edge opens edge inspector with condition/label (and related fields) before commit.
3. Commit sends existing `strategy.patch_transition`; 409 opens conflict modal with **Use current** / **Reapply my edit** / **Review side-by-side**.
4. Existing edges can be **reconnected** by drag (product semantics: old transition replaced by new target).
5. Keyboard path: dialog **choose source → choose target → choose edge kind** → same commit path.

## Non-goals

- Strategy `preset` → `strategy` breaking rename (identifiers stay preset).
- Full shared command palette; Idea artifact persistence redesign.
- Graph layout engine (dagre/elk) for Strategy.
- Inner-graph group `onConnect` deep polish beyond outer state handles (may residual if blocked).
- Outline Scene/Beat (P0) or viewport scale work (P2).
- **Breaking** wire changes — the additive `op` field is backward-compatible (optional, default `"update"`); no new top-level wire op, no rename of `strategy.patch_transition`, no removal of existing fields.
- Platform / cloud.

## Studio-first note

P1 is App Strategy behavior first (live graph + wire). Prefer existing `canvas-strategy-accent`, `canvas-edge`, `canvas-edge-hover`, `canvas-port` tokens — no new tokens unless architect approves. Studio fixtures optional if Strategy surface fixtures already exist.

## Wire

**Architect-finalized (`wire_contracts_changed: true` — additive, backward-compatible):** The shipped `strategy.patch_transition` DTO (V1.71) is **rewire-only**. Spatial create of a brand-new transition requires an additive `op` field.

**Architect finding (§5.2 Q3):** Read of `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs` `patch_transition_inner` + `apply_transition_patch` (lines 660–878):

| Product bar | DTO + daemon support? | Evidence |
|-------------|----------------------|----------|
| **FB-SE-003 reconnect existing edge** | **YES — fully covered** | `source_state_id` + `old_target` + `new_target` + `condition` matches existing `next` / `rules` / `default` / `go` / `nogo` and replaces atomically. Single reconnect payload; no delete+create needed. |
| **FB-SE-000/001/002 create brand-new transition** | **NO — daemon rejects** | Source state with no `next` field → `strategy_transition_missing` (400). Source state with `next` but no matching `old_target` branch → `strategy_transition_not_found` (400). The shipped DTO is rewire-only. |

**Escape hatch (locks P1 wire change):** Additive `op: "create" | "update"` field on `StrategyPatchTransitionRequest` (default `"update"` for backward compatibility — shipped V1.71 callers are unaffected). When `op: "create"`:
- Source state with no `next` → daemon sets `next: <new_target>` (linear) or `next: { rules: [{ to: new_target, when: condition }] }` (branch, when `condition` present).
- Source state with `next` as a map → daemon appends a new rule `{ to: new_target, when: condition }`.

This is an **additive, backward-compatible** wire change → `@42ch/nexus-contracts` **minor bump only**. No breaking change to shipped V1.71 callers; the field is optional and defaults to `"update"`. Schema + daemon handler + codegen output updated in the same commit.

| User action | Structured op | Notes |
|-------------|---------------|-------|
| Commit new transition from draft (spatial or keyboard) | `strategy.patch_transition` with **`op: "create"`** | New `op` field; `old_target` becomes optional when `op: "create"` (no existing transition to match) |
| Reconnect edge | `strategy.patch_transition` with **`op: "update"`** (default) + `old_target` + `new_target` | Single reconnect payload; one logical transition after (§5.2 Q4) |
| Conflict | 409 structured conflict | UI: keep draft, refetch, modal actions per §3.5 |

**Reconnect shape (§5.2 Q4 — locked):** Single reconnect payload. The DTO already supports reconnect atomically. Delete-old + create-new is NOT needed for reconnect.

---

## User stories (steering-loop style)

- **Draw a branch on the map** — *As an author*, I drag from a Strategy state handle to another state and complete condition/label in the inspector, so I rewire the workflow where I see it — then I **Run / Resume** Nexus on the revised graph.
- **Fix a wrong edge without forms-first** — *As an author*, I reconnect an existing transition by dragging its end to a new target, so fixing a mistaken branch is spatial, not a multi-field delete/create ritual.
- **Stay safe under concurrent edits** — *As an author*, if my transition save is stale, the conflict modal offers **Use current**, **Reapply my edit**, or **Review side-by-side**, so I stay in command without silent overwrite.
- **Create edges without a pointer** — *As an author* (keyboard-first), I open the edge-creation dialog, choose source → target → edge kind, and commit the same transition, so graph editing meets §4.4 pointer alternatives.

---

## Voice & Content (locked)

Follow DESIGN.md §Voice & Content: **Title Case** for titles/nav/buttons; **sentence case** for helpers; **Verb + Noun** for actions. Conflict modal actions reuse Strategy/outline-flavored pattern from master §3.5 / §4.4.

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Draft edge | Default edge label (until set) | **Draft transition** |
| Edge inspector | Commit action (new edge) | **Create Transition** |
| Edge inspector | Commit action (edit existing) | **Save Transition** (if already present — keep consistency; do not invent third verb) |
| Edge inspector | Cancel draft | **Cancel** |
| Edge inspector | Condition field label | **Condition** |
| Edge inspector | Label field label | **Label** |
| Edge reconnection | In-progress helper (optional toast/status) | *Reconnect the transition to a new target.* |
| Edge reconnection | Confirm if required | **Reconnect Edge** |
| Keyboard dialog | Title | **Create Transition** |
| Keyboard dialog | Step 1 label | **Choose source** |
| Keyboard dialog | Step 2 label | **Choose target** |
| Keyboard dialog | Step 3 label | **Choose edge kind** |
| Keyboard dialog | Primary commit | **Create Transition** |
| Keyboard dialog | Cancel | **Cancel** |
| Conflict modal | Actions | **Use current** · **Reapply my edit** · **Review side-by-side** · **Cancel** (master lock) |
| Toolbar / shortcut help | Open keyboard create (if surfaced) | **Create Transition…** |

**Forbidden in author-facing UI:** `onConnect`, `patch_transition`, `base_revision`, `source_state_id`, “DTO”, “handle id”.

---

## FB-SE-000 — Spatial onConnect Draft Edge

**Problem:** Strategy nodes lack connect handles / draft-on-connect behavior.

**User-visible outcome:** Dragging from a source handle to a valid target creates a **draft** transition edge on the canvas (not yet persisted).

### onConnect draft behavior (product)

1. **Handles visible** on connectable Strategy state nodes (source + target) with `canvas-port` styling.
2. **Valid connect** (source → different target state) inserts a draft edge with temporary id and label **Draft transition**.
3. **Invalid connect** (self-loop, non-connectable type) does not create a draft; no silent error toast required if RF blocks — if allowed by RF, show non-blocking validation in inspector.
4. **Draft is local** until commit — leaving the page with dirty draft follows existing canvas dirty-guard patterns if any; minimum: cancel clears draft.
5. **Focus:** after draft create, open edge inspector and move focus to inspector heading / first field (§4.4 focus management).

### Acceptance

- [ ] Connectable Strategy nodes render source and target handles.
- [ ] Completing a valid drag-connect creates a draft edge in canvas state with label **Draft transition** (or empty label showing that fallback).
- [ ] Draft edge is visually distinct as uncommitted (token: `canvas-edge-hover` or draft style — implement chooses; must not look fully committed).
- [ ] Draft creation does **not** call the daemon until author commits in inspector (or keyboard dialog).
- [ ] Focus moves to edge inspector after draft create.

**SSOT:** `strategy-nodes.tsx`, `use-strategy-canvas.ts`, `state-machine.tsx` / strategy canvas orchestrator

---

## FB-SE-001 — Draft Edge Opens Edge Inspector Fields

**Problem:** Draft without condition/label collection would commit incomplete transitions.

**User-visible outcome:** Creating a draft edge opens the edge inspector with condition/label fields before commit.

### Edge inspector fields (product — draft + existing)

| Field | Required for commit? | Notes |
|-------|----------------------|-------|
| Source (read-only display) | — | Show source state label |
| Target (read-only display) | — | Show target state label |
| **Condition** | Per domain rules (may be empty for default `next`) | Label **Condition** |
| **Label** | Optional | Label **Label** |
| Edge kind (if multi-kind) | When branches/default/converge apply | Prefer human labels over enum raw values |

### Acceptance

- [ ] Draft edge selection opens edge inspector (existing component extended for draft mode).
- [ ] Inspector shows **Condition** and **Label** fields (and edge kind if product-relevant for that transition type).
- [ ] Primary action for new draft is **Create Transition**.
- [ ] **Cancel** discards the draft edge without daemon call.
- [ ] Editing an already-persisted edge still works (no regression of existing inspector path).

**SSOT:** `strategy-canvas/inspectors/edge-inspector.tsx`

---

## FB-SE-002 — Commit via `patch_transition` + Conflict Modal

**Problem:** Spatial edits must honor structured write boundary and OCC.

**User-visible outcome:** Commit sends `strategy.patch_transition`; on stale revision, conflict modal appears with locked recovery actions.

### Acceptance

- [ ] **Create Transition** / save path calls `strategy.patch_transition` through `NexusClient` (no raw file / fetch).
- [ ] Successful commit replaces draft with canonical edge from refetch (or optimistically updates then reconciles).
- [ ] 409 conflict: keep draft, refetch canonical, open conflict modal with **Use current**, **Reapply my edit**, **Review side-by-side** (side-by-side only when non-overlapping fields — master rule).
- [ ] Modal a11y: focus trap, ARIA live announcement, return focus on close (§4.4.7).
- [ ] No silent last-write-wins.

**SSOT:** edge inspector commit path; `conflict-modal` reuse for Strategy 409

---

## FB-SE-003 — Edge Reconnection by Drag

**Problem:** Fixing a wrong target currently requires inspector delete/create mental load.

**User-visible outcome:** Authors reconnect an existing edge by dragging an end to a new valid target; result is one transition to the new target.

### Reconnection UX (product)

1. **Gesture:** Drag from existing edge endpoint (or RF reconnect affordance) onto a new target handle.
2. **Semantics:** Old transition is removed and new transition created (or single reconnect op) — author ends with **one** edge for that logical rewiring, not duplicates.
3. **Confirm:** Prefer direct commit with same OCC as create; if implement needs an explicit confirm step, use **Reconnect Edge** as the confirm CTA (do not invent multi-step wizard).
4. **Invalid target:** Revert edge to previous target; no partial daemon state.
5. **Keyboard:** Reconnect may be inspector-only for V1.109 if drag reconnect is pointer-primary; keyboard users can delete + **Create Transition** dialog. Optional residual if full keyboard reconnect required later.

### Acceptance

- [ ] Dragging an existing edge end to a new valid target updates the transition (delete-old + create-new or equivalent).
- [ ] Successful reconnect does not leave both old and new edges for the same logical connection.
- [ ] Failed/invalid reconnect restores previous edge visually.
- [ ] OCC/conflict policy applies to reconnect commits the same as create.

**SSOT:** `use-strategy-canvas.ts` reconnection handlers

---

## FB-SE-004 — Keyboard Dialog Path (§4.4)

**Problem:** Edge creation must not be pointer-only (`canvas-strategy-surface.md` §4.4.5).

**User-visible outcome:** Authors open a dialog, complete **Choose source → Choose target → Choose edge kind**, then commit with the same op as spatial create.

### Keyboard dialog flow (product)

| Step | UI | Behavior |
|------|-----|----------|
| Open | Shortcut or **Create Transition…** control | Focus moves into dialog title **Create Transition** |
| 1 | **Choose source** | List/combobox of connectable source states |
| 2 | **Choose target** | List of valid targets (exclude invalid self if required) |
| 3 | **Choose edge kind** | Kind + optional condition/label fields (may merge condition on same step if kinds are few) |
| Commit | **Create Transition** | Same draft→commit or direct commit path as spatial |
| Cancel | **Cancel** | Close, no daemon write |

### Acceptance

- [ ] Keyboard-reachable control opens **Create Transition** dialog.
- [ ] Steps use locked labels: **Choose source**, **Choose target**, **Choose edge kind**.
- [ ] Completing the dialog creates a transition via `strategy.patch_transition` (same as spatial commit).
- [ ] Focus returns to a sensible control on close (trigger or selected node).
- [ ] Dialog works without pointer (keyboard only).

**Deferral rule:** If RF handle keyboard API or dialog scope balloons, FB-SE-004 may residual **only** with tracking — spatial FB-SE-000..003 remain Must. Product preference: ship dialog in-iteration.

**SSOT:** `strategy-canvas/edge-create-dialog.tsx`, canvas layout shortcut wiring

---

## Definition of Done (product)

- FB-SE-000..003 accepted in App Strategy canvas; FB-SE-004 accepted or residual with explicit tracking.
- Conflict modal path verified for transition commit (both create and reconnect).
- `wire_contracts_changed: true` (additive `op` field — §5.2 Q3 evidence); backward-compatible minor bump; schema + daemon + codegen in one commit.
- Cross-canvas parity claim: Strategy edges can be created/rewired on-graph like World KB relationships (gesture class, not identical UX chrome).

## Roadmap / deferred (tracked)

| Deferred item | Trigger | Owner |
|---------------|---------|-------|
| Inner-graph group deep connect polish | Outer-state onConnect Done + author demand | next iteration |
| Full keyboard edge reconnect | FB-SE-004 Done + a11y audit | residual / next |
| Spatial-origin metadata on wire (beyond `op`) | Only if a second escape hatch fires | `@architect` |

## Effort (agent-oriented)

Medium plan (4 SDD tasks): handles+draft → commit+conflict → reconnect → keyboard dialog.
