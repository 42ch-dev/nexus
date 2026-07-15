---
iteration_id: V1.110
start_date: 2026-07-11
end_date: 2026-07-12
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-11-v1.110-daemon-startup-latency
  - 2026-07-11-v1.110-agent-scan-path-reliability
  - 2026-07-11-v1.110-agent-picker-ux-polish
---

# V1.110 Delivery Compass

## Scope

Fix the four app/agent/daemon reliability + UX issues reported from real-world
dogfooding of V1.109 (PR #139, merge `e9d46820`). All four block the core
"launch app → pick an installed agent → start writing" flow:

- **FB-D1** — "Daemon starting" phase is noticeably slow on app launch;
  suspected sidecar detection overhead.
- **FB-D2** — `AgentPicker` renders the full registry list unsorted; common
  agents are buried. Requested: common-first ordering with an exact priority,
  then a **More** button that loads the rest sorted by registry order (last-updated desc deferred — see residual `R-V110P2-001`).
- **FB-D3** — After opening the app, **all** agents show "Not installed" even
  though several agent CLIs are installed on the machine.
- **FB-D4** — Agent custom-path `Input` and `Verify Agent` `Button` sit in a
  flex-wrap row at different heights; fuse them into one coherent component.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-11-v1.110-daemon-startup-latency | P0 — Daemon startup latency (FB-D1) | Done | three-valued port-probe gate; QC tri Approve + QA Pass; 3 nit residuals |
| 2026-07-11-v1.110-agent-scan-path-reliability | P1 — Agent scan PATH reliability (FB-D3) | Done | nvm/volta/fnm/pnpm/yarn dir resolution; QC tri Approve + QA Pass; 3 nit residuals |
| 2026-07-11-v1.110-agent-picker-ux-polish | P2 — AgentPicker UX polish (FB-D2 + FB-D4) | Done | common-first + More + fused verify; QC tri Approve + QA Pass; 4 low/nit residuals |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain) | 2026-07-11 | pending |
| P0 dev complete | 2026-07-11 | pending |
| P1 dev complete | 2026-07-11 | pending |
| P2 dev complete | 2026-07-11 | pending |
| QC complete | 2026-07-11 | pending |
| Iteration close | 2026-07-11 | pending |

## Acceptance Criteria

- **FB-D1** — Measurable reduction in "Daemon starting" time on cold start;
  the redundant pre-spawn probe no longer gates spawn when the port is known
  free, and the spawn→ready wait exposes progress without blocking UI.
  Acceptance: a timing test or evidence demonstrating the cold-start path does
  not pay an unnecessary probe round-trip.
- **FB-D3** — On a machine with agents installed via npm-global (nvm), cargo,
  homebrew, or pnpm, the agent scan reports them as `installed` without
  requiring the custom-launch escape hatch. Acceptance: `login_equivalent_bin_dirs()`
  resolves version-manager global bin dirs; a unit test covers the nvm node-bin
  glob case.
- **FB-D2** — `AgentPicker` shows common agents first in the exact priority
  order (Codex CLI, Claude Code, Cursor CLI, OpenCode, Hermes, Kimi Code, Qoder,
  GitHub Copilot CLI, Pi, Kiro CLI); remaining agents hidden behind a **More**
  button that, once clicked, reveals the rest sorted by registry order.
- **FB-D4** — Custom-path field and Verify control render as a single fused
  component with aligned height and a coherent affordance.
- All three plans `Done` (or non-blocking residuals documented).

## Non-Goals

- Fifth canvas domain surface; continuing the V1.108–V1.109 canvas trajectory
  (deferred — separate iteration).
- Replacing the `which`-based probe with a daemon-side shell-out login shell
  (considered; the login-shell approach is higher-risk and out of scope — P1
  expands the static dir list instead).
- ACP registry format changes (read-only consumer).
- Agent **launch/execution** reliability (detection only).
- Auto-update / signing / notarization of the desktop shell.
- Settings-shell IA rework beyond the AgentPicker component.
- Full keyboard navigation inside the "More" expansion (basic keyboard reach
  is in scope; deep a11y audit is not).

## Roadmap Position

- **Current iteration (V1.110):** **Delivered.** Reliability + UX polish pass on the app-launch → agent-detection → agent-pick core flow. Daemon startup latency fixed (three-valued port-probe gate); agent scan PATH reliability fixed (nvm/volta/fnm/pnpm/yarn); AgentPicker polished (common-first + More + fused verify). All 3 Must plans Done, QC tri Approve, QA Pass.
- **Next iteration:** Resume the canvas trajectory (shared command palette, sidebar canvas IA — both deferred from V1.109 `## Non-Goals`). Trigger: V1.110 ships green and the user re-opens the canvas direction. Owner: `@project-manager`.
- **Long-term Done:** A local-first authoring tool whose first-launch flow is fast, correctly detects the user's existing agents, and presents them in an ergonomically ordered picker — so the author reaches the canvas in seconds.

## Delivery Branch Policy

> Mirror of frontmatter; kept in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.110` |
| `target_branch` | `main` |

Branch resolve evidence (autonomous): `status.json` root `metadata.iteration_base_branch = main`,
`metadata.target_branch = main`, `metadata.latest_ship.iteration = V1.109` (PR #139 merged to main).
No silent `main`/`master` default — these fields are documented delivery policy from V1.109 close.

## Locked direction rationale (autonomous)

| Candidate cluster | Evidence | Rank |
|-------------------|----------|------|
| **FB-D1 + FB-D3 (reliability)** | User reports broken core flow; `sidecar.rs` `HEALTH_PROBE_TIMEOUT=2s` cold-start probe (`apps/desktop/src-tauri/src/sidecar.rs:31,557`); `login_equivalent_bin_dirs()` omits nvm/volta/fnm/pnpm/yarn (`crates/nexus-daemon-runtime/src/path_enrichment.rs`); residual `R-V1101P0-003` deferred "Document Class B PATH enrichment locus + finite common-dir" | **1** (locked) |
| **FB-D2 + FB-D4 (UX polish)** | User reports picker ergonomics; `AgentPicker` renders `agents.map(...)` unsorted with no More affordance (`apps/web/src/components/setup/agent-picker.tsx:177-197`); `CustomLaunchField` Input default-size + Button `size="small"` height mismatch (`:422-449`) | **2** (locked) |
| Canvas trajectory continuation | V1.109 `## Roadmap Position` next: command palette / sidebar IA | deferred (next iteration) |

**Scale budget:** L → 3 business plans (within 3–4 cap). Harness process
(Review chain / QC / QA / compound / close / PR) excluded from count per
autonomous-direction-lock § Scale budget.

### Must / Stretch integrity

| Tier | Plans | May defer? | Iteration incomplete if missing? |
|------|-------|------------|----------------------------------|
| **Must** | **P0** daemon latency; **P1** scan reliability; **P2** picker UX | No | **Yes** (any missing) |
| **Stretch** | — | — | — |

## Product Story

**Who:** Authors launching the Nexus desktop app for the first time (or after
an update) to start or resume writing.

**Problem:** Today the first-launch flow has three friction points stacked
back-to-back: (1) "Daemon starting…" lingers longer than feels right; (2) when
the picker finally renders, the list is long and the common agents are buried
mid-list; (3) worst of all, agents the author *knows* they installed all show
"Not installed", forcing them into the custom-launch escape hatch — and that
escape hatch's Input and Verify button are visually misaligned.

**Narrative:** Make the cold-start path not pay for a probe it does not need,
close the PATH gaps that make npm-global / cargo / homebrew agents invisible,
and turn the picker from a flat dump into a common-first, progressively
disclosed, visually coherent surface.

**Iteration complete when:** All three Must plans Done (or non-blocking
residuals documented); FB-D1 timing evidence accepted; FB-D3 detection test
accepted; FB-D2 ordering + More button accepted; FB-D4 fused component accepted.

### User-visible outcomes by feedback ID

**P0 — Daemon startup latency (FB-D1)** — primary spec: `v1.110/specs/daemon-startup-latency.md`

| ID | What the author sees |
|----|----------------------|
| FB-D1-000 | "Daemon starting" no longer pays a redundant probe round-trip on cold start; spawn begins as soon as the port is known free. |

**P1 — Agent scan PATH reliability (FB-D3)** — primary spec: `v1.110/specs/agent-scan-path-reliability.md`

| ID | What the author sees |
|----|----------------------|
| FB-D3-000 | Agents installed via npm-global (nvm), cargo, homebrew, or pnpm show as "Installed" without the custom-launch escape hatch. |

**P2 — AgentPicker UX polish (FB-D2 + FB-D4)** — primary spec: `v1.110/specs/agent-picker-ux-polish.md`

| ID | What the author sees |
|----|----------------------|
| FB-D2-000 | Common agents (Codex CLI, Claude Code, Cursor CLI, OpenCode, Hermes, Kimi Code, Qoder, GitHub Copilot CLI, Pi, Kiro CLI) appear first, in that order. |
| FB-D2-001 | A **More** button after the common list reveals the remaining agents, sorted by registry order. |
| FB-D4-000 | The custom-path field and Verify control render as one fused component with aligned height. |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| P1 nvm glob resolution is fragile across nvm versions | Medium | High | Resolve the active node version dir deterministically; fall back to scanning `~/.nvm/versions/node/*/bin`; unit-test both paths. |
| P0 probe-removal regresses the "attach to external daemon" path | Low | Medium | Preserve the attach probe for the manual `start_daemon` path; only the cold-start auto path may skip it when the port is known free. |
| P2 ordering list drifts from registry renames | Low | Low | Match by `registry_agent_id` first, then `name`; unknown common ids degrade gracefully to registry order. |

## Iteration workspace

| Path | Purpose |
|------|---------|
| `v1.110/specs/daemon-startup-latency.md` | P0 spec draft (architect refines) |
| `v1.110/specs/agent-scan-path-reliability.md` | P1 spec draft (architect refines) |
| `v1.110/specs/agent-picker-ux-polish.md` | P2 spec draft (architect refines) |
| `v1.110/guides/` | Exploration / process notes |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-11-v1.110-daemon-startup-latency | Approve with residuals (seat 2 degraded PM-authored) | mandatory — Pass | R-V110P0QC1-S001/S002/S003 (3 nit) | `{PLAN_DIR}/2026-07-11-v1.110-daemon-startup-latency.md#review-gate-summary` |
| 2026-07-11-v1.110-agent-scan-path-reliability | Approve with residuals (seat 2 degraded PM-authored) | mandatory — Pass | R-V110P1QC1-S001/S002/S005 (3 nit) | `{PLAN_DIR}/2026-07-11-v1.110-agent-scan-path-reliability.md#review-gate-summary` |
| 2026-07-11-v1.110-agent-picker-ux-polish | Approve with residuals (full tri, all 3 seats) | mandatory — Pass | R-V110P2-001 + R-V110P2QC2-W001 + 2 nit (4 low/nit) | `{PLAN_DIR}/2026-07-11-v1.110-agent-picker-ux-polish.md#review-gate-summary` |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

- 结晶文档数：2 新增 (`gui-process-path-enrichment.md`, `acp-registry-id-matching.md`) + 1 更新 (`daemon-ready-gate-pattern.md` V1.110 refinement)
- 新增 CONCEPTS.md 条目：0（无项目特有新领域词；已有 nvm/PATH/registry 概念为通用术语）
- 触发 compound-refresh：否（无过期知识标识）
- **Workspace 盘点** (`v1.110/specs/`)：3 篇 spec 草案 → **Keep snapshot**（迭代级 spec，已被 plans 消费；不值得提升为 `{SPECS_DIR}/` 冻结规格——这些是 bugfix/polish spec，不是长期行为契约）。跳过提升理由：自检 ≤2 Yes（Q1/Q2 部分 Yes，但 Q3/Q6 No——这些 spec 描述一次性修复，非可复用模式）。

## Iteration Retrospective (minimal)

- 做得好的：autonomous direction lock 基于代码优先调研（live registry fetch 发现 name 不匹配）；3 plan 模块边界干净无重叠；QC tri 全 Approve；compound 提炼了 2 个高价值模式（PATH enrichment 闭合 R-V1101P0-003、registry id matching）。
- 可改进的：`qc-specialist-2` 子代理在本 session 前 4 次派发返回空（P0×3 + P1×1）导致 2 个 degraded tri-review；换模型后恢复。建议：QC 派发失败时及早换模型而非重试同配置。T4 首次尝试在单测中 spawn 真实 daemon 导致挂起——unit-test boundary 已沉淀到 knowledge。
- 下迭代建议：恢复 canvas 轨迹（command palette / sidebar IA）；考虑 R-V110P2-001（ACP registry updated_at 字段）是否值得向上游提案。
