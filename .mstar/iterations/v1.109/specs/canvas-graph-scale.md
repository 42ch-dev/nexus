# Canvas Graph Scale + Viewport Reliability — Primary Spec (V1.109 P2)

**Status:** Draft — product-complete (§5.1 product-manager)  
**Tier:** Must (P2)  
**Plan:** `2026-07-11-v1.109-canvas-graph-scale`  
**Compass:** `../v1.109-delivery-compass.md`  
**Normative master:** `.mstar/specs/canvas-strategy-surface.md` (§3.1 performance notes, §3.3 shared shell, §4.4 a11y)  
**Upstream residuals:** V1.108 post-ship QC findings (10 open rows targeting post-V1.108)

## Product outcome

Authors working on **real Works** (many nodes, frequent graph↔list toggles) keep their place on the canvas and do not pay unnecessary selection thrash. Contributors gain **real React Flow integration tests** so spatial regressions are caught before ship. The ten non-blocking V1.108 residuals are either fixed surgically or re-tracked with clear disposition.

**User-visible win:** Toggle **Show list view** → **Show graph** and pan/zoom is where you left it; dragging nodes does not feel “sticky” from over-firing selection sync; quality of life fixes from V1.108 QC land without a new product surface.

## Problem

V1.108 shipped Outline spatial C1 and UI shell SSOT with **Approve + residuals**. Three findings directly hurt scale/reliability of the expanded graphs V1.109 is about to deepen (Scene/Beat, Strategy edges):

| Residual | Author impact |
|----------|----------------|
| R-V1108P0QC3-S002 | Graph↔list toggle unmounts shell → **viewport loss** (re-orient every toggle) |
| R-V1108P0QC3-W001 | Selection sync fires on every node interaction → **latent perf trap** as graphs grow |
| R-V1108P0QC3-S003 | Orchestrator tests mock `CanvasShell` → **no real RF coverage** for spatial bugs |

Seven additional V1.108 residuals (UI shell / AgentPicker / extract hooks / alt sort) remain open; this plan sweeps them with a **cap** so scope does not become a junk drawer.

## Product story

**Who:** Authors on multi-chapter Works; contributors protecting canvas quality in CI.

**Why viewport + tests matter now:**
- C2 adds nested Scene/Beat nodes — more pan/zoom navigation, more selection.
- Strategy edge rewiring adds drag gestures — selection/position noise must not thrash inspectors.
- Without viewport preservation, graph↔list alt toggle (a11y requirement) punishes authors who check the list then return to the map.
- Mocked CanvasShell tests green-washed spatial regressions in V1.108; Scene/Beat and edge work raise the cost of that gap.

**Narrative:** Invest in shell reliability and test harness **before** graphs get deeper — not as a polish afterthought.

## Goals

1. Preserve pan/zoom viewport across graph↔list toggle (and re-mount of graph content).
2. Selection sync updates only when **selected entity ids** change — not on pure position drag.
3. Real React Flow integration test harness covers outline (and reusable pattern for other surfaces).
4. V1.108 residual sweep: targeted fixes for fixable rows; defer only with rationale + updated tracking.

## Non-goals

- Graph layout engines (dagre/elk).
- Full virtualization of thousand-node Worlds (master §3.1 still advises progressive expansion — not this plan’s engine).
- New canvas domain features (Scene/Beat product in P0; Strategy edges in P1).
- Shared command palette; Sidebar canvas IA.
- Wire/schema changes — **`wire_contracts_changed: false`** hard.
- Platform / cloud.

## Studio-first note

UI/test-only plan. No new DESIGN tokens. No new author-facing surfaces unless residual copy fixes require string tweaks (see Voice & Content).

## Wire

**Hard lock:** `wire_contracts_changed: false`. No Daemon API or schema changes.

---

## User stories

- **Keep my place when I check the list** — *As an author*, I switch to **Show list view** and back to **Show graph**, and the canvas pan/zoom is restored, so scanning structure in a list does not force me to re-find my chapter cluster.
- **Drag without inspector thrash** — *As an author*, I drag nodes to tidy layout and the inspector does not flicker or re-bind unless I change selection, so spatial cleanup stays calm.
- **Trust automated spatial coverage** — *As a contributor*, integration tests exercise real React Flow selection/interaction paths, so Scene/Beat and edge work do not reintroduce silent spatial regressions.

---

## Voice & Content (locked)

No new primary CTAs. Preserve V1.108 locks:

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Outline/Strategy/World KB toolbar | Graph → list | **Show list view** |
| Outline/Strategy/World KB toolbar | List → graph | **Show graph** |

**If FB-GS-003 touches Verify error helper copy** (R-V1108P1QC2-S001): keep V1.108 failure helper as default user-facing string unless implement can distinguish transport vs no-match **without** jargon:

| Case | Copy (exact) |
|------|--------------|
| Transport / unreachable | *Could not reach this agent. Check the command and try again.* (existing) |
| Reachable but no match (if distinguishable) | *No matching agent for this command. Check the command and try again.* |
| Success | *Agent responded successfully.* (existing) |

Do **not** surface HTTP codes, “scan endpoint”, or stack traces in author UI.

---

## FB-GS-000 — Viewport Preserved Across Graph↔List Toggle

**Problem:** Toggle unmounts `CanvasShell` / RF tree and drops pan/zoom (R-V1108P0QC3-S002).

**User-visible outcome:** After graph → list → graph, viewport (pan x/y + zoom) matches pre-toggle state for that surface session.

### Viewport behavior (product)

1. **Scope:** At minimum Outline canvas (where residual was found). Prefer shell-level hook reusable by Strategy/World KB if low cost — product success = Outline Must; other surfaces Nice-if.
2. **What is preserved:** `{ x, y, zoom }` of the RF viewport — not selection, not draft ops.
3. **When saved:** On viewport change (pan/zoom) while graph is mounted; also immediately before toggle to list if needed to avoid race.
4. **When restored:** On re-mount of graph view if a cached viewport exists for this surface+work (or surface session).
5. **When reset is OK:** Hard navigation away from the Work/route may clear cache; full page reload may clear — product does not require cross-session persistence to disk.
6. **A11y:** Restoration must not steal focus from the toggle control incorrectly; focus management for alt toggle remains keyboard-reachable.

### Acceptance

- [ ] Outline: pan and zoom, toggle **Show list view**, then **Show graph** — viewport matches prior pan/zoom within normal float tolerance.
- [ ] Works with empty and non-empty graphs.
- [ ] Does not break keyboard focus path for the alt toggle.
- [ ] Residual R-V1108P0QC3-S002 closed when accepted.

**SSOT:** `use-canvas-viewport.ts` (new), `canvas-shell.tsx` integration

---

## FB-GS-001 — Selection Sync Optimized

**Problem:** Selection-sync effect fires on every node interaction including position changes (R-V1108P0QC3-W001).

**User-visible outcome:** Dragging/repositioning nodes does not re-fire selection→inspector sync; changing which node is selected still updates the inspector immediately.

### Selection optimization scope (product)

| Event | Must update inspector selection? |
|-------|----------------------------------|
| Click/select different node | **Yes** |
| Multi-select change (if supported) | **Yes** when selected id set changes |
| Pure position drag (`position` change only) | **No** |
| Viewport pan/zoom | **No** |
| Data refetch same selection ids | **No** thrash (stable ids) |

### Acceptance

- [ ] Position-only node drag does not trigger selection sync side effects (inspector rebind / equivalent).
- [ ] Changing selected node id still drives inspector.
- [ ] No regression: click-to-select on Outline (and any surface touched) still works.
- [ ] Residual R-V1108P0QC3-W001 closed when accepted.

**SSOT:** selection effects in outline orchestrator / `use-outline-data` (and shared patterns if extracted)

---

## FB-GS-002 — Real React Flow Integration Test Coverage

**Problem:** Orchestrator tests fully mock `CanvasShell` — no real RF integration (R-V1108P0QC3-S003).

**User-visible outcome:** None directly; contributor-visible: CI exercises real RF mount paths for critical interactions.

### What RF integration tests cover (product minimum)

1. **Harness:** Test wrapper renders real `<ReactFlow>` / provider (not a no-op CanvasShell mock).
2. **Outline smoke:** Graph mounts with fixture nodes; selection can drive inspector binding (or selection state assertion).
3. **Interaction class:** At least one real user-event path (click node or keyboard focus) without mocking away RF internals for that path.
4. **Non-goals for this FB:** Full E2E in Playwright/Tauri; pixel snapshots; entire Strategy/World KB suite (reuse harness later).
5. **jsdom limits:** If ResizeObserver missing, harness polyfills or documents required test setup — “blocked on jsdom” is not acceptance without a working harness approach.

### Acceptance

- [ ] Shared RF integration harness exists under canvas test utilities.
- [ ] At least one Outline orchestrator/integration test uses real RF (not fully mocked CanvasShell).
- [ ] Test is stable in CI (documented polyfills if needed).
- [ ] Residual R-V1108P0QC3-S003 closed when accepted.

**SSOT:** `canvas/__tests__/rf-integration-harness.tsx`, outline canvas tests

---

## FB-GS-003 — V1.108 Residual Sweep (10 Findings)

**Problem:** Ten post-V1.108 residuals remain open with `target: post-V1.108`.

**User-visible outcome:** Targeted quality fixes land; deferred items keep honest tracking (not silent drop).

### Disposition table (product + plan agreement)

| ID | Title (short) | Disposition | Rationale |
|----|---------------|-------------|-----------|
| **R-V1108P0QC3-S002** | Viewport loss on graph↔list | **Fix** (FB-GS-000) | Blocks usable deep graphs |
| **R-V1108P0QC3-W001** | Selection overfire | **Fix** (FB-GS-001) | Scale risk with C2/P1 gestures |
| **R-V1108P0QC3-S003** | No real RF integration tests | **Fix** (FB-GS-002) | Safety net for V1.109 spatial work |
| **R-V1108P0QC1-S001** | Extract `useOutlineCanvasGraph()` | **Fixed in P0** (§5.2 Q5 architect-locked — C2 complexity justifies the extract now; P2 no longer owns this row) | P0 Task 1 lands the extract; P2 FB-GS-001 selection-memo fix targets the extracted hook |
| **R-V1108P0QC1-M001** | Alt view no sort controls | **Defer** | UX enhancement; inspectors cover edit; track residual with target post-V1.109 |
| **R-V1108P1QC1-S001** | CREATOR_NAV / ORCHESTRATOR_NAV duplicated | **Fix** | Small consolidate; fixture drift risk |
| **R-V1108P1QC1-S002** | Studio node chrome should import P0 extract | **Defer** | Depends on P0 extract availability; track until extract ships |
| **R-V1108P1QC2-S001** | Verify error helper transport vs no-match | **Fix** | Small honesty improvement; Voice table above |
| **R-V1108P1QC3-W001** | AgentPicker grid re-render on verifyStatus | **Fix** | Small memo; low severity but cheap |
| **R-V1108P1QC3-W002** | useVerifyAgent lacks unmount cancellation | **Fix** | AbortController / cancel on unmount |

**Scope cap:** 7 fix-oriented items max in the “always fix” set above (viewport, selection, RF tests, nav consolidate, verify error, AgentPicker memo, verify abort). The `useOutlineCanvasGraph` extract (R-V1108P0QC1-S001) is **owned by P0** per §5.2 Q5, so it is no longer in P2's sweep. Anything larger discovered mid-impl → **new residual**, not silent scope creep.

### Acceptance

- [ ] FB-GS-000..002 residuals closed in `status.json` when those FBs pass.
- [ ] Each **Fix** row either closed with evidence or converted to a documented blocker residual.
- [ ] Each **Defer** row updated with new target (e.g. post-V1.109) and non-empty note.
- [ ] No wire contract changes.

**SSOT:** implement files per residual; `.mstar/status.json` residual lifecycle updates

---

## Definition of Done (product)

- FB-GS-000..002 accepted with test evidence.
- FB-GS-003 disposition applied in status residuals.
- Outline (and any shell consumer) remains a11y-safe for alt toggle.
- `wire_contracts_changed: false`.

## Roadmap / deferred (tracked)

| Deferred item | Residual | Trigger |
|---------------|----------|---------|
| Alt list sort controls | R-V1108P0QC1-M001 | Author demand / a11y audit |
| Studio import of P0 node chrome | R-V1108P1QC1-S002 | After presentational extract exists |
| Cross-session viewport persistence | — | Only if authors request restore after reload |
| Full multi-surface viewport matrix | — | After Outline hook proven |

## Effort (agent-oriented)

Medium plan (4 SDD tasks): viewport hook → selection memo → RF harness → residual sweep. Mostly UI/test; low product ambiguity.
