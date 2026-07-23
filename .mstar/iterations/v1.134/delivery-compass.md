---
iteration_id: V1.134
start_date: 2026-07-23
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.134
scale: L
direction_lock_mode: interactive
plans:
  - 2026-07-23-v1.134-p0-desktop-startup-500
  - 2026-07-23-v1.134-p1-app-icon-full-bleed
  - 2026-07-23-v1.134-p2-agent-picker-vi-retune
  - 2026-07-23-v1.134-p3-creator-hub-dual-pane-ia
---

# V1.134 Delivery Compass — Dogfood usability hardening (desktop 500 + app icon + AgentPicker VI + Creator Hub IA)

## Product story

**One sentence:** Stop re-fixing four named dogfood defects so the author can open Nexus, trust the desktop shell, pick an agent without visual regression, and create Worlds/Works the way the mental model already expects.

This is **usability hardening**, not feature expansion. Every plan is **Must** (bug-fix / IA correction). There is **no Stretch** budget in V1.134 — if a task grows beyond the named defect, cut scope rather than smuggle features.

### Success snapshot (author-observable)

| Surface | Before (still wrong) | After (this iteration) |
|---------|----------------------|------------------------|
| Desktop launch | Startup network log shows a non-blocking `500` | Clean startup log — no `/v1/daemon/*` or `/v1/local/*` 500s |
| Dock icon | Flat un-rounded square (macOS squircle mask defeated) | Normal macOS app icon with system squircle rounding |
| Agent pick (Setup + Settings) | Status dots gone; Light shell over-cyans | Dots restored; Light = cyan accent-only; Dark = liberal |
| Creator Hub | Modal create + controller stub; lists not dual-pane workspace | Stable dual-pane: left workspace (inline create), right cards, linked 世界/作品 tabs |

## Scope

**Locked direction (interactive):** Fix four recurring dogfood-usability problems the author has reported across multiple prior iterations (V1.126→V1.132) and that are **still wrong**. Each is a concrete, user-observable defect — not net-new feature scope.

1. **P0 — Desktop startup 500 (app still launches).** A startup fetch the web app makes on mount returns `500 Internal Server Error`; the app still launches because the failing probe is non-blocking. User explicitly asked to locate the 500 source ("需要定位500的报错是什么") and eliminate it. V1.132 P0 (orch-load-404) + V1.133 (brace-param route sweep) closed the 404 class; a 500 on a startup route remains. **Root cause not yet pinned** — RCA (reproduce in desktop/web dev, capture the failing request + stack) is the first task.

2. **P1 — Desktop app icon never looked right (5 iterations).** Author's direct observation (corrected root cause): the compiled icon shows as a **flat full square with no macOS rounded corners / icon treatment**. Reading `compose-app-icon.mjs` confirmed the cause: the script rasterizes `logo-primary-square.svg` at **768×768** and composites it onto a **fully transparent** 1024×1024 canvas with a **128px transparent margin** per side (added to "avoid the squircle clipping plate corners into a halo"). That fear was misguided and **backfired**: macOS Big Sur+ applies the automatic squircle mask **only to opaque icons that fill the canvas**; the transparent alpha border **defeats the mask**, so the icon renders as a flat un-rounded square. The squircle clipping the plate corners **is** the intended rounded look. Fix: rasterize the plate as an **opaque full-bleed 1024×1024** square (no resize-to-768, no transparent composite) so macOS masks it. Also correct the Studio brand doc + icons README that describe the inset/transparent margin as intended. **Caveat:** the PM cannot view the icon this session, so P1 Task 1 is an actual RCA (rebuild opaque, author confirms macOS now rounds it after `killall Dock`), not a pre-baked fix.

3. **P2 — AgentPicker got uglier; status dots removed; cyan overused in Light shell.** `apps/web/src/components/setup/agent-picker.tsx` line 15 explicitly documents *"no competing tint wash or right-side status dot"* — the agent-selection status dots + top-right status dot were stripped during the V1.108–V1.119 polish passes. User requirement: restore the dots, de-emphasize cyan in Light shell (accent-only; cyan liberal only in Dark shell), and retune the AgentPicker VI per DESIGN.md. **Surfaces:** same presentational component is mounted in **Setup wizard** and **Settings → Agent** — both must look correct after the retune. **Re-investigate VI references** (the user notes prior reference designs were given and the result keeps regressing) — recover from git history, do not invent a new look.

4. **P3 — Creator Hub / creation page IA (biggest / critical path).** Current `creator-hub-page.tsx` = right `CreatorEntityListsPanel` (Worlds/Works lists) + left Sidebar Create-only panel (V1.132 P3) + a **controller stub that replaces the whole hub** on selection; create actions are **dialog/modals**. Author mental model (locked):
   - **Left pane = workspace 功能区** — directly usable (inline create / select), **not** modal-driven.
   - **Right pane = card list** for the active entity kind; empty state steers create to the left.
   - **World / Work (世界 / 作品) tabs on BOTH panes, linked** — switching one switches the other; one shared tab SSOT.
   - **Dual-pane is stable chrome** — selecting a card must **not** abandon the dual-pane for a full-page controller stub (today's `selectedEntity` branch). Selection outcome (navigate to canvas vs in-pane focus) is architect-owned in the IA contract; product rule is "list layout stays."
   - **Canvas routes stay orthogonal** — `/works/:workId/*` and `/worlds/:worldId/*` are unchanged product surfaces; hub IA does not rewrite canvas.

   Product intent brief (product-side only; architect expands contract): [`.mstar/iterations/v1.134/specs/p3-creator-hub-product-brief.md`](specs/p3-creator-hub-product-brief.md).

### Priority of pain (user-value rank — not execution order)

1. **P3 — Creator Hub IA** (critical path, largest). Primary creation surface; modal create + controller stub fight the author's mental model.
2. **P2 — AgentPicker VI** (visible regression every dogfood on Setup + Settings).
3. **P1 — App icon** (5 iterations of rework; corrected root cause = transparent alpha defeats macOS mask).
4. **P0 — Desktop 500** (non-blocking but must not ship; locate + fix).

### Sequencing (user-confirmed)

**P0 first** (backend, isolated), then the **P1–P3 frontend track**. Within the frontend track, **P3 is the critical path**; P1 and P2 are parallel-frontend-safe (disjoint files) and may run concurrently with P3 via separate worktrees under the harness isolation rules.

**Priority note:** Pain rank puts P3 first in value; execution still runs P0 first so backend RCA does not block the frontend wave.

## Visual acceptance caveat (this iteration)

The PM orchestrator **cannot view the running app or images this session** (model-level: no image input). Per the repo **studio-first** policy, visual acceptance for P1 (icon), P2 (AgentPicker), and P3 (creation page) lands in `apps/design-studio` fixtures (light + dark, all variants) **first**, which the **author eyeballs**; only after visual acceptance there is the surface wired into `apps/web`. Agent claims of "looks good" without author Studio (and for P1, Dock) sign-off are **not** acceptance. P0 has no visual surface.

| Plan | Visual gate (author) | Not a gate |
|------|----------------------|------------|
| P0 | — | — |
| P1 | Studio brand fixture **and** Dock tile after `killall Dock` | Script change alone / theory |
| P2 | Studio AgentPicker fixture (light+dark × statuses × dots) | Component unit tests alone |
| P3 | Studio dual-pane fixture (tabs × empty/populated × themes) | App wiring before Studio accept |

## Architecture Locks

**Lock date:** 2026-07-23 — **confirmed by architect (Seat 2)**.
**Branch path:** `main` -> `iteration/v1.134` -> `main`.

### P0 — Startup 500 RCA + route fix (architect-confirmed)

A daemon route in `crates/nexus-daemon-runtime/` returns `INTERNAL_SERVER_ERROR` on a request the web app issues during desktop mount. Candidates: an orchestration/agent-host scan or list endpoint, a workspace/works/worlds overview fetch, or a route the V1.133 brace sweep left returning 500 for an empty/uninitialized state. **RCA first** (do not assume the route): reproduce via `pnpm dev:desktop` (dist-load) or web dev + daemon, capture the failing request (URL + status + response body) and the server-side stack, then fix at the handler. **`wire_contracts_changed: false`** — a 500 is a server bug, not a contract change — unless RCA reveals a missing/shape-mismatched DTO (escalate to architect). Risk register verified: RCA-not-reproducible risk is defensible (capture from logs if dev won't reproduce; add defensive handler + regression test). `primary_spec` (recommended for PM → status.json): compass AC-0…AC-3; `spec_refs`: `.mstar/specs/desktop-shell.md` (daemon startup surface).

### P1 — App icon opaque full-bleed (let macOS mask the squircle) (architect-confirmed)

- The compose script `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs` rasterizes `logo-primary-square.svg` at 768×768 onto a **transparent** 1024×1024 canvas (128px alpha margin per side). The transparent alpha border **defeats macOS Big Sur+ automatic squircle masking** → the icon renders as a flat un-rounded square (the author's observed symptom). Fix: rasterize the plate as an **opaque full-bleed 1024×1024** square (no inset, no transparent composite). macOS then masks it to the squircle; the squircle clipping the plate corners is the intended rounded look.
- Regenerate `source/source-1024.png` + `source/app-icon-preview-256.png`; the generated desktop formats (`icon.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png`) are produced by `icons:generate` and are **gitignored/generated** (only `source/` + README tracked).
- Correct the inset/transparent-margin description in `apps/desktop/src-tauri/icons/README.md` and the Studio brand page copy (state the opaque-full-bleed rule + *why*: transparency defeats the macOS mask — the 5-iteration bug).
- **RCA-first (PM cannot view the icon):** Task 1 rebuilds the opaque asset and the author confirms (after `killall Dock` to drop stale LaunchServices tile) that macOS now applies rounding. If it still does not round, the RCA branch pins the next candidate (tauri `.icns` generation / stale bundle). Studio brand fixture shows the composition for sign-off; the Dock smoke (R-VI-003 lineage) remains the runtime acceptance surface.
- **`wire_contracts_changed: false`** (icon composition only; no wire types involved). Risk register verified: stale LaunchServices tile is a real risk; `killall Dock` + app-bundle removal escalation is the correct mitigation chain. `primary_spec` (recommended): compass AC-4…AC-7; `spec_refs`: `.mstar/specs/desktop-shell.md` (icon/brand), `DESIGN.md` (brand tokens).

### P2 — AgentPicker VI retune (architect-confirmed)

- `agent-picker.tsx` is presentational (Setup + Settings hosts). Restore the agent-selection status dot + the top-right status dot (revert the V1.108–V1.119 removal documented at component line 15). Audit cyan (`--color-accent-cyan` family) usage against DESIGN.md: **Light shell = accent-only** (highlights, active selection); **Dark shell = liberal**. Do not delete the status dot again.
- Studio fixture for AgentPicker: light + dark, all `AgentPickerStatus` variants (loading/ready/empty/error), dots visible. Visual acceptance in Studio before app wiring claims.
- **`wire_contracts_changed: false`** (presentational retune only; no wire types). Risk register verified: file overlap with P1/P3 is Low (P2 touches `agent-picker.tsx` + its test; P3 touches `creator-hub-page.tsx` + new pane components; disjoint). Cyan discipline rule is DESIGN.md-backed (brand-cyan-* tokens). `primary_spec` (recommended): compass AC-8…AC-11; `spec_refs`: `DESIGN.md` + `DESIGN.dark.md`, `.mstar/specs/web-ui.md` (Setup/Settings surface).

### P3 — Creator Hub dual-pane IA (architect-confirmed; IA contract authored)

- Replace the current hub model (Create-only left + right lists + **controller stub on selection** + modal create) with a **stable dual-pane**: **left = workspace 功能区 (inline create/select, no modal on hub)**; **right = card list with empty state**; **shared World/Work tab state linked across both panes** (`useHubTab` SSOT).
- **IA contract authored by architect:** [`.mstar/iterations/v1.134/specs/p3-creator-hub-dual-pane-ia.md`](specs/p3-creator-hub-dual-pane-ia.md) — component ownership (new `HubWorkspacePane`, `HubCardListPane`, `HubTabBar`), tab-link semantics (single shared tab bar above both panes, not mirrored), selection behavior (card click → navigate to canvas route; no controller stub; `useCreatorEntitySelection` not used on hub), inline create scope (both World + Work inline on hub; dialogs preserved for non-hub call sites), empty-state i18n (en + zh-CN), canvas-route orthogonality confirmed, sliced deliverable with explicit cut line.
- **Empty-state copy (i18n en + zh-CN from day one):** zh-CN intent "暂无世界，从左边创建" / "暂无作品，从左边创建"; en: "No Worlds yet — create one from the left" / "No Works yet — create one from the left".
- **Evolves (does not reopen) shell parent IA:** footer / 工作区 parent / 创作|编排 mode switch stay. P3 changes **hub content** only.
- Studio-first: dual-pane layout fixture in `apps/design-studio` (World-tab + Work-tab, empty + populated, light + dark) **before** `apps/web` wiring.
- **`wire_contracts_changed: false`** — apps/web IA change; no new daemon routes/DTOs. Existing `NexusClient` methods reused. Risk register verified: canvas-route orthogonality risk mitigated by hub/canvas being independent route components; over-scope risk mitigated by explicit cut line in IA contract §8; file-overlap risk is Low (P3 touches `creator-hub-page.tsx` + new pane components; P1 touches icon scripts; P2 touches `agent-picker.tsx` — disjoint). `primary_spec` (recommended for PM → status.json): `.mstar/iterations/v1.134/specs/p3-creator-hub-dual-pane-ia.md`; `spec_refs`: `.mstar/specs/web-ui.md` (Control Room contract), `.mstar/specs/desktop-shell.md` (shell IA), `DESIGN.md` + `DESIGN.dark.md` (tokens).

## Plans

| Wave | plan_id | Name | Priority | Status | blocked_by |
|------|---------|------|----------|--------|------------|
| 1 | `2026-07-23-v1.134-p0-desktop-startup-500` | Desktop startup 500 RCA + fix | **Must** | Todo | - |
| 2 | `2026-07-23-v1.134-p1-app-icon-full-bleed` | App icon opaque full-bleed | **Must** | Todo | - |
| 2 | `2026-07-23-v1.134-p2-agent-picker-vi-retune` | AgentPicker VI retune | **Must** | Todo | - |
| 2 | `2026-07-23-v1.134-p3-creator-hub-dual-pane-ia` | Creator Hub dual-pane IA | **Must** (critical path) | Todo | - |

**Scale budget:** L → **4** business plans. Wave 1 = P0 (backend, isolated); Wave 2 = P1 ∥ P2 ∥ P3 (frontend track; P3 critical path, P1/P2 disjoint from P3 and from each other → parallel-frontend-safe per worktree isolation). **No Stretch plans.**

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-23 | pending |
| Wave 1 — P0 RCA + fix | 2026-07-23 | pending |
| Wave 2 — P1–P3 frontend track | 2026-07-23 | pending |
| Iteration close | 2026-07-24 | pending |

## Acceptance Criteria

### P0 — Desktop startup 500 (user-observable + verification)

- **AC-0 (user-observable).** On a clean desktop launch (`pnpm dev:desktop` or a fresh bundle), the startup network log contains **no** `500 Internal Server Error` response from any `/v1/daemon/*` (or `/v1/local/*`) route. Non-blocking probes succeed or fail with an expected domain status (e.g. legitimate empty 200), never a 500.
- **AC-1 (root cause documented).** The failing route + the server-side root cause are recorded in the plan's Review Gate Summary and/or iteration guide, so the 500 class does not recur undocumented.
- **AC-2 (regression).** An automated test reproduces the previously-failing condition and asserts the non-500 outcome (or a unit test at the handler covering the previously-unhandled branch).
- **AC-3 (crate health).** `cargo test -p <affected crate>` + `cargo clippy -p <affected crate> -- -D warnings` pass.

### P1 — App icon opaque full-bleed (user-observable)

- **AC-4 (composition + docs).** `compose-app-icon.mjs` produces an **opaque full-bleed** 1024×1024 plate (no transparent alpha border / inset). Rationale: macOS Big Sur+ masks opaque full-canvas icons to the squircle; the previous transparent margin defeated the mask. `apps/desktop/src-tauri/icons/README.md` + Studio brand page copy state the opaque-full-bleed rule (and *why* — transparency defeats the mask), removing the inset/transparent description. The `app-icon-preview-256.png` reflects the opaque full-bleed composition.
- **AC-5 (RCA verified by author).** After rebuild + `killall Dock` (stale-tile invalidation), the author confirms the Dock tile now shows **macOS squircle rounding** (not a flat square). If rounding still does not appear, the RCA pinned the real cause (tauri `.icns` / stale bundle) instead of declaring done on theory.
- **AC-6 (studio fixture).** A Studio brand fixture shows the opaque full-bleed icon composition; **author** visually signs off (PM cannot view images).
- **AC-7 (human smoke guidance).** Dock smoke steps remain current (R-VI-003 lineage); the opaque full-bleed asset is what the author will eyeball at Dock.

### P2 — AgentPicker VI retune (user-observable)

- **AC-8 (dots restored).** The agent-selection status dot and the top-right status dot render for the relevant states — visible in **Studio fixture** and in Setup + Settings mounts. The "no status dot" note at component line 15 is removed/rewritten.
- **AC-9 (cyan discipline).** Cyan usage in the AgentPicker passes a DESIGN.md audit: Light shell = accent-only; Dark shell = liberal. No cyan washes in Light where a neutral/token alternative applies.
- **AC-10 (studio fixture).** Studio AgentPicker fixture covers light + dark + all `AgentPickerStatus` variants with dots visible; **author** visually accepts in Studio before app "done" claims.
- **AC-11 (tests).** Existing `agent-picker.test.tsx` updated to reflect restored dots; `pnpm --filter web run build` + `typecheck` pass.

### P3 — Creator Hub dual-pane IA (user-observable)

- **AC-12 (dual-pane layout).** Creator Hub renders **left workspace 功能区 (inline create/select, no modal on hub)** + **right card list** as **stable chrome**. Selecting a World/Work does **not** replace the dual-pane with a full-page controller stub.
- **AC-13 (empty state).** Right pane shows i18n empty copy (per IA contract §1.5 — entity-specific en + zh-CN per World/Work tab) when the active World/Work tab has no items.
- **AC-14 (linked tabs).** A World/Work (世界/作品) tab on **both** panes stays linked — switching one switches the other consistently; selection/tab state is coherent.
- **AC-15 (studio-first).** The dual-pane layout is built/accepted in `apps/design-studio` (World-tab + Work-tab, empty + populated, light + dark) **before** app wiring; **author** accepts the fixture.
- **AC-16 (app wiring + tests).** `apps/web` hub routes consume the new layout; canvas routes remain orthogonal; `pnpm --filter web run build` + `typecheck` + relevant page tests pass.
- **AC-17 (inline create).** Hub create flows are inline in the left pane; no create-via-dialog path remains **on the hub** (dialogs may remain for non-hub call sites if still used).

## Non-Goals

Explicit and defensible for this L-scale usability-hardening iteration:

| Non-goal | Why out |
|----------|---------|
| Net-new product features (new engines, new canvas surfaces, Fork UI, compute-on-timeline, entity Chat deepening) | Direction is dogfood-usability hardening of four named defects, not feature expansion. |
| Full 62-residual burn-down | Scale L; only the four targeted defects. Remaining nits stay deferred with rationale. |
| Redesigning surfaces beyond the four named (icon, AgentPicker, Creator Hub, startup 500) | Scope discipline; other VI/IA polish is a separate iteration. |
| Replacing shell footer / 工作区 parent / 创作\|编排 mode switch | Shell parent IA already shipped; P3 is hub content IA only. |
| Promoting AgentPicker or hub chrome into `@42ch/nexus-ui` unless architect explicitly adds it | Studio-first fixture + app component is enough; package promotion is a separate decision. |
| Signing/notarization, multi-OS icon variants beyond macOS-generation correctness | P1 fixes the composition source; OS-bundle signing is a separate track. |
| Cloud/platform sync, remote-bind, TLS changes | Not in these four defects. |
| Wire-contract / schema version bumps (unless P0 RCA forces one) | Anticipated `wire_contracts_changed: false` on all four plans. |
| Stretch polish (animation, micro-copy beyond empty-state + required labels, density experiments) | No Stretch; cut rather than expand. |

## Roadmap Position

- **Current (V1.134):** dogfood-usability hardening — desktop startup 500 + app icon opaque full-bleed + AgentPicker VI retune + Creator Hub dual-pane IA.
- **Prior:** V1.133 legacy false-Done repair + residual burn-down (#171).
- **Next:** deeper Creator entity Chat; Orchestrator 功能区 beyond the hub; opportunistic DF-70/71; remaining open residuals. The recurring VI/IA surfaces should now be stable enough to stop re-litigating each iteration. Trigger: next dogfood pass. Owner: PM + frontend.
- **最终目标:** a Control Room whose primary creation surface matches the author's mental model, with a visually correct, HIG-conformant desktop shell and no spurious startup errors — so iteration velocity stops being consumed by re-fixing the same four things.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.134` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| P0 root cause not reproducible in dev (env-specific) | Med | High | RCA task must capture evidence before assuming; if not reproducible, document the failing route from logs + add defensive handler + regression test. |
| P1 opaque full-bleed still does not round (cause is tauri `.icns` / stale LaunchServices, not transparency) | Med | High | P1 Task 1 is RCA-first: rebuild opaque, `killall Dock` to drop stale tile, author confirms rounding; if still flat, RCA pins the real cause before any "done". |
| P3 IA redesign breaks selection/canvas-route orthogonality or reintroduces controller-stub full-page replace | Med | High | Product lock: dual-pane is stable chrome. Architect contract (§1.3, §1.6): selection → navigate to canvas route; hub and canvas are independent route components (`CreatorHubPage` ≠ `WorldsPage`/`WorksPage` sub-routes); no conditional mode-switch in the hub component. Pre-merge gate: `pnpm --filter web run typecheck && build && test` + manual smoke (hub → card click → canvas renders; back → dual-pane). |
| P3 over-scopes into entity Chat / canvas rewrite | Med | High | Non-Goals + product brief + IA contract §8 explicit cut line. Trim any task that is not dual-pane / inline create / linked tabs / empty state. |
| P1–P3 frontend track file overlap (shared DESIGN.md tokens, shared Studio app) | Low | Med | P1: `compose-app-icon.mjs`, `icons/README.md`; P2: `agent-picker.tsx`, `agent-picker.test.tsx`; P3: `creator-hub-page.tsx`, new `hub-*-pane.tsx`, `i18n` catalogs. **Disjoint files confirmed by architect.** Shared tokens only: DESIGN.md (read-only). Worktree isolation per `mstar-branch-worktree`. |
| PM cannot view app → visual defects slip past Studio to app wiring | Med | Med | Studio-first fixtures are the gate; **author** eyeballs Studio (and P1 Dock) before apps/web wiring claims. |

## Iteration package

> Sibling paths under `{ITERATION_DIR}/v1.134/` — not in `{SPECS_DIR}/` or `{KNOWLEDGE_DIR}/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Purpose |
|------|---------|
| `specs/p3-creator-hub-product-brief.md` | Product intent for P3 dual-pane IA (product Seat 1) |
| `specs/p3-creator-hub-dual-pane-ia.md` | **Architect IA contract (Seat 2)** — component ownership, tab-link semantics, selection behavior, inline create scope, sliced deliverable |
| `specs/` | Further iteration-scoped drafts |
| `guides/` | Exploration notes, RCA writeups (P0/P1/P2), VI reference research (P2) |

## primary_spec / spec_refs (for PM → status.json mirror)

> Architect recommends these per-plan spec links. PM should mirror into `status.json` `plans[].metadata` at Phase 2 entry.

| Plan | primary_spec | spec_refs |
|------|-------------|-----------|
| P0 | compass AC-0…AC-3 | `.mstar/specs/desktop-shell.md` (daemon startup surface) |
| P1 | compass AC-4…AC-7 | `.mstar/specs/desktop-shell.md` (icon/brand), `DESIGN.md` (brand tokens) |
| P2 | compass AC-8…AC-11 | `DESIGN.md` + `DESIGN.dark.md` (token SSOT), `.mstar/specs/web-ui.md` (Setup/Settings surface) |
| P3 | `.mstar/iterations/v1.134/specs/p3-creator-hub-dual-pane-ia.md` | `.mstar/specs/web-ui.md` (Control Room contract), `.mstar/specs/desktop-shell.md` (shell IA), `DESIGN.md` + `DESIGN.dark.md` (tokens) |

## Spec / knowledge cross-refs (do not duplicate)

| Artifact | Use |
|----------|-----|
| `.mstar/specs/web-ui.md` | Control Room product contract |
| `.mstar/specs/desktop-shell.md` | Tauri shell + icon/sidecar boundary |
| `.mstar/specs/design-studio.md` | Studio-first gallery rules |
| `DESIGN.md` + `DESIGN.dark.md` | Token SSOT (cyan, status, shell) |
| V1.132 shell/creator IA knowledge | Workspace-parent + Create-only left — P3 **evolves hub content intent** only |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| _pending — filled at close_ | | | | |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
