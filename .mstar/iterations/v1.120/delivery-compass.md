---
iteration_id: V1.120
start_date: 2026-07-17
status: completed
end_date: 2026-07-17
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.120
plans:
  - 2026-07-17-v1.120-strategies-repair
  - 2026-07-17-v1.120-shell-form-polish
  - 2026-07-17-v1.120-orchestration-ia-icon
---

# V1.120 Delivery Compass — Control Room dogfood polish

> **Phase 1:** product-manager §5.1 **done** · architect §5.2 **done** · writing-specialist §5.3 **done**.
> **PM lock (§5.4):** `status: locked` (2026-07-17). Prepare gates pass on all three plans (specify / clarify / plan). Spec freeze locked.

### Grill-me decisions (locked)

User confirmed direction **A**, then instructed **follow all PM recommendations** for remaining branches.

1. **Direction:** V1.120 = only the 7 dogfood feedback items (Control Room + Desktop Dock icon). No new feature tracks.
2. **Branch policy:** `main` → `iteration/v1.120` → `main`.
3. **Plan slicing:** three Must plans — P0 Strategies repair; P1 Shell/form polish; P2 Orchestration IA + sessions count + Dock icon.
4. **Capabilities page:** remove from Orchestration nav; soft-remove route (`/capabilities` redirects); do not build capability UI.
5. **System presets:** hide `_system.*` ids from the System presets list (authors never need them).
6. **Validate:** per-row action on each strategy row — not a global header button.
7. **Delete:** user-generated presets only (confirm dialog).
8. **Strategy detail:** fix `getPreset` / `locate_preset` load failure that surfaces canvas `ErrorState` with `common.error.title` (“无法加载此视图” / “Could not load this view”); add **Back** to list on not-found and load-error states.
9. **Dirty Save:** Save (and similar persist CTAs) enabled only when form is dirty — general UI rule for this iteration’s touched surfaces.
10. **Footer refresh:** after Agent Save, `DaemonStatusBar` agent badge refreshes immediately (not only on 10s poll).
11. **Dock icon:** restore transparent compose / squircle-compatible asset so Dock is not a sharp white square.
12. **`wire_contracts_changed: false`** — preset delete/validate APIs already exist.
13. **Sessions “running” (F3):** Sessions is an **active-work monitor**, not history. Only non-terminal sessions (`running`, `paused`, `waiting_for_input`) may appear. When the author has not started orchestration and the daemon is idle, the list is **empty** — any phantom `running` row is a defect.
14. **Capabilities soft-remove (F4):** Remove from Orchestration nav; `/capabilities` redirects to `/sessions`. **Do not** delete daemon capability APIs or registry endpoints.
15. **Dirty Save (iteration-wide):** On every **persist CTA** on surfaces touched by V1.120, the control stays disabled until local state ≠ last-saved baseline. P1 minimum: Settings → Agent **Save**; extend to any other save/persist control modified in P0–P2.

## Feedback traceability (F1–F7)

| ID | Pain (dogfood) | Plan | Primary AC |
| --- | --- | --- | --- |
| F1 | Agent Save always enabled; footer agent badge stale after save | P1 | AC-P1-1, AC-P1-2 |
| F2 | Select duplicate chevron; Advanced re-run wizard invisible; dark disabled primary ugly | P1 | AC-P1-3, AC-P1-4, AC-P1-5 |
| F3 | Sessions shows 3 “running” with idle runtime | P2 | AC-P2-1 |
| F4 | Capabilities nav is internal noise | P2 | AC-P2-2, AC-P2-3 |
| F5 | Strategy detail crash; no Back; `_system.*` clutter; global Validate; no Delete | P0 | AC-P0-1…5 |
| F6 | Restart control lacks daemon-restart hint | P1 | AC-P1-6 |
| F7 | macOS Dock = sharp white square | P2 | AC-P2-4 |

## Product story

After V1.119 unblocked Setup, author dogfood hit a cluster of Control Room polish bugs: Strategies detail is unusable, shell chrome feels broken (dirty Save, missing tooltips, Select artifact, weak danger/disabled styles), Sessions shows phantom running work, Capabilities is internal noise, and the macOS Dock icon still reads as a white square.

The coherent bet:

> **Authors can trust Strategies, shell chrome, and Orchestration navigation again — without seeing internal mechanism pages or a broken Dock tile.**

| Who | Pain today | What they get when V1.120 is Done |
| --- | --- | --- |
| **Authors (Strategies)** | Listed preset → canvas `ErrorState` (`common.error.title`); no Back; Validate is global; `_system.*` clutter; cannot delete own presets | Openable detail (`getPreset` 200); Back on not-found/load-error; per-row Validate; filtered system list; delete user presets |
| **Authors (Shell)** | Agent Save leaves footer stale; Save always lit; Select glitch; Advanced wizard CTA nearly invisible; dark disabled buttons ugly; Restart unclear | Dirty-gated Save; live footer badge; fixed Select/danger/disabled; Restart tooltip |
| **Authors (Orchestration)** | Sessions shows 3 running with idle runtime; Capabilities nav is useless | Empty Sessions when idle; only live active work listed; Capabilities gone from nav + redirect |
| **Authors (Desktop)** | Dock = white sharp square | Logo on system squircle (transparent asset) |

## Scope

Locked spec surfaces for this iteration:

- Strategies list/detail repair + Validate/Delete UX + `_system.*` filter (F5)
- Shell form dirty-gate + Agent Save → footer refresh + Select/danger/disabled + status-bar tooltips (F1, F2, F6)
- Sessions active-work honesty; remove Capabilities from Orchestration IA; macOS Dock icon fix (F3, F4, F7)

## Scope slices (non-overlapping)

| Slice | Plan | Surface boundary | Ships alone? |
| --- | --- | --- | --- |
| **P0** | strategies-repair | `/strategies`, `/strategies/:presetId`, preset actions | Yes — unblocks Strategies dogfood |
| **P1** | shell-form-polish | Settings Agent, shared Button/Select, DaemonStatusBar, Settings Advanced | Yes — independent of Strategies |
| **P2** | orchestration-ia-icon | Sessions list query/filter, sidebar nav, desktop icons compose | Yes — independent |

**Overlap guard:** P0 must not restyle global Button tokens (P1). P1 must not change Strategies routes. P2 must not touch preset CRUD.

## Plans

| plan_id | Name | Status | Tier | Notes |
|---------|------|--------|------|-------|
| 2026-07-17-v1.120-strategies-repair | Strategies repair | Done | Must / P0 | F5 — QC Approve w/ residuals (7), QA Pass; merged `a6102357` |
| 2026-07-17-v1.120-shell-form-polish | Shell & form polish | Done | Must / P1 | F1 F2 F6 — QC Approve w/ residuals (9), QA Pass; merged `5ee1a20b` |
| 2026-07-17-v1.120-orchestration-ia-icon | Orchestration IA + sessions + Dock | Done | Must / P2 | F3 F4 F7 — QC Approve w/ residuals (8), QA Pass; merged `a1b0e5ce` |

### Plan dependencies (implement order)

| Plan | Depends on | Rationale |
| --- | --- | --- |
| P0 Strategies repair | — | Highest severity (page unusable) |
| P1 Shell & form polish | — | May parallel after P0 starts; no code overlap |
| P2 Orchestration IA + icon | — | May parallel; sessions + nav + icons |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Phase 1 lock) | 2026-07-17 | done |
| Dev complete | 2026-07-17 | done |
| QC complete | 2026-07-17 | done |
| Iteration close | 2026-07-17 | done |

## Acceptance Criteria

- Every Feedback item F1–F7 has a mapped AC in the owning plan (see traceability table) and passes automated and/or documented manual smoke.
- **Strategies (P0):** Open any listed (non-`_system.*`) preset with `getPreset` 200 (no canvas `ErrorState` from `locate_preset` failure); not-found and residual load-error detail show **Back** to `/strategies`; Validate is per-row only (no header Validate); user presets deletable with confirm; `_system.*` absent from System presets list.
- **Shell (P1):** Agent **Save** disabled when clean, enabled when dirty; after successful save, `DaemonStatusBar` agent badge updates immediately (no 10s poll wait); Select has single chevron; Advanced re-run wizard uses danger variant with dark contrast; disabled primary readable in dark; Restart has tooltip + `aria-label` naming daemon restart.
- **Dirty Save:** Every persist CTA on V1.120-touched surfaces follows dirty-gate (see grill #15); P1 smoke must include Agent Save clean→dirty→save→clean cycle.
- **Orchestration (P2):** With idle daemon and no author-started orchestration, Sessions list is empty (zero `running` rows); nav has no Capabilities; `/capabilities` redirects to `/sessions`; capability daemon APIs remain callable (no API removal).
- **Desktop (P2):** Dock smoke shows logo on system squircle, not opaque white square (`apps/desktop/src-tauri/icons/README.md`).

## Non-Goals

- New capability admission UI or capability-detail product surface
- Strategy canvas authoring redesign (beyond making detail load + navigate)
- New wire schemas / `@42ch/nexus-contracts` bump (reuse existing preset APIs)
- Creation / canvas / memory feature work
- Signing/notarization or multi-OS Dock polish beyond macOS squircle asset fix

## Roadmap Position

- **Current iteration（V1.120）**：**delivered** (2026-07-17) — post-V1.119 Control Room dogfood polish cluster closed: Strategies repair (P0), shell/form polish (P1), Orchestration IA + sessions honesty + Dock icon (P2). PR `iteration/v1.120` → `main`.
- **Next iteration**：Resume product depth deferred during Setup/polish — trigger: V1.120 PR merged + author dogfood green; owner: product-manager. Registered residual candidates for early triage: R-V1120P1QC1-F001 (wizard-path save invalidation helper), R-V1120P1QC2-S003 (DESIGN.disabled token cluster), R-V1120P0QC2-S001 (preset_id validation guard).
- **最终目标**：Local-first Control Room chrome is trustworthy enough that authors spend time writing, not fighting shell bugs.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.120` |
| `target_branch` | `main` |

## Architecture decisions (Phase 1 §5.2)

| ID | Decision | Rationale |
| --- | --- | --- |
| **AD-P0-1** | Strategy detail “无法加载此视图” is **`StrategyCanvas` load failure** (`getPreset` / `locate_preset`), not React Router `errorElement`. Fix **`locate_preset`** for qualified `_system.*` ids (strip prefix → `_system/<dir>/preset.yaml`); add **Back** on `strategy-page` empty/error and canvas load error. T1 repro matrix: user, embedded, non-filtered system preset. Lazy `@xyflow/react` chunk stays; add canvas-local boundary only if T1 shows render throw. | `StrategyCanvas` `ErrorState` defaults title to `common.error.title`; `locate_preset` joins full `_system.maintenance` as dirname (bug). No `errorElement` on `/strategies/:presetId`. |
| **AD-P0-2** | Sessions phantom rows = **daemon auto-started `_system.*` sessions** at boot (`boot.rs` WS-D), not client filter bug. **Primary fix:** `list_sessions` handler excludes `preset_id.starts_with("_system.")`. **Secondary:** client defensive filter in `useSessions` + test. **Optional** startup reconcile for orphaned SQLite non-terminal rows without runners (only if dogfood repro shows author-session ghosts after AD-P2-1). | API already uses `list_active` (non-terminal only); idle dogfood “3 running” matches system-preset session count. `wire_contracts_changed: false`. |
| **AD-P1-1** | Footer refresh via **`queryClient.invalidateQueries`** on `queryKeys` for agent profile + daemon scan/status consumed by `DaemonStatusBar` after Agent Save — not poll interval change. Dirty gate: local `useDirtyForm` or equivalent in `settings-agent-section.tsx` only (P1 scope). | Matches existing TanStack Query boundary; avoids global poll churn. |
| **AD-P2-1** | Capabilities soft-remove: **`Navigate`** in `App.tsx` for `/capabilities` → `/sessions`; drop nav item in `sidebar.tsx` + mobile `MOBILE_NAV_KEYS`. **Do not** remove `CapabilitiesPage` module or daemon routes. | Iteration-scoped IA; APIs preserved per grill #14. |
| **AD-P2-2** | Dock icon: verify `compose-app-icon.mjs` alpha-0 outside mark; regenerate `icns` from LFS `source-1024.png`; Dock smoke is acceptance gate. | V1.117 claimed transparent canvas — regression is asset pipeline, not runtime. |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `locate_preset` fix incomplete for edge-case system dir names | Low | Med | P0 T1 includes `_system.maintenance` get + open; add handler unit test |
| Author-session ghosts persist after `_system.*` filter | Low | Med | AD-P0-2 secondary reconcile; document repro in P2 T1 |
| Strategy canvas render throw on malformed YAML (uncaught) | Low | Med | T1 matrix; canvas-local error boundary only if repro confirms |
| Dock icon still white after compose fix (stale generated icns / LFS) | Med | Med | Re-run compose+generate; Dock smoke mandatory; check LFS pull |
| Hotfix `2026-07-16-hotfix-setup-continue-404` still InReview on main | Low | Low | V1.120 cuts from `main`; hotfix lands independently via its PR |

## Iteration package

| Path | Purpose |
|------|---------|
| `guides/` | Exploration notes |
| `specs/` | Iteration-scoped specs (primary_spec targets) |
| `README.md` | Package index |

## Quality Gate Summary

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-17-v1.120-strategies-repair | Approve with residuals (0C/0W) | mandatory → Pass (6/6 ACs) | 7 (3 low, 4 nit) | `locate_preset` SSOT fix + Back UX + list actions; merged `a6102357` |
| 2026-07-17-v1.120-shell-form-polish | Approve with residuals (0C/1W) | mandatory → Pass with residuals (7/7 ACs) | 9 (2 low, 7 nit) | Dirty-gate + footer refresh + chevron root-fix + a11y; merged `5ee1a20b` |
| 2026-07-17-v1.120-orchestration-ia-icon | Approve with residuals (0C/0W) | mandatory → Pass with residuals (5/5 ACs) | 8 (2 low, 6 nit) | Sessions honesty + Capabilities soft-remove + Dock evidence; merged `a1b0e5ce` |

## Compound Round Summary

- 结晶文档数：2（`conventions/system-preset-qualified-id-resolution.md`、`architecture-patterns/tailwind-content-scan-for-package-primitives.md`，均已登记 knowledge/README.md）
- 新增 CONCEPTS.md 条目：1（`### System Preset (_system.*)`）
- Package 盘点（§3.2 强制）：`specs/` 3 篇（strategies-repair / shell-form-polish / orchestration-ia-icon）→ **Keep snapshot**（迭代级 primary specs，继续被 plans 引用；非跨迭代知识）；`guides/` 空；无 Promote。
- 跳过结晶候选：Dock icon 资产陈旧问题（根因为机器本地缓存，README+residual 已覆盖）；F-001 save→invalidate 约定（尚未实现，residual R-V1120P1QC1-F001 跟踪）。
- 触发 compound-refresh：flag — tailwind content-scan doc 与既有 `tailwind-theme-key-routing-for-sizing-tokens.md` 中度重叠（同属 "Tailwind silent non-emission"），后续 refresh 可合并为单篇。

## Iteration Retrospective (minimal)

- 做得好的： grill-me 锁定的范围边界（overlap guard）全程零跨 plan 污染；T1 TDD 一次命中根因；Dock F7 通过证据审核而非盲改完成（避免了对健康 pipeline 的误修）；所有 plan QC 0 Critical。
- 可改进的： QC 发现的 cross-surface 契约不对称（wizard save 不失效缓存）属于 Prepare 期可预见的调用点普查遗漏 —— 下轮 Prepare 对「契约类改动」增加 caller-sweep 检查项。
- 下迭代建议： 优先消化 R-V1120P1QC1-F001（persistAgentProfile helper）与 R-V1120P1QC2-S003（DESIGN.disabled token）；eval clippy `--all-targets` baseline 是否纳入 CI hygiene plan。
