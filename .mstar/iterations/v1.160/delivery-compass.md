---
iteration_id: V1.160
start_date: 2026-08-11
status: completed
end_date: 2026-08-12
iteration_base_branch: main
target_branch: main
plans:
  - 2026-08-11-v1.160-p1-world-kb-entity-creation
  - 2026-08-11-v1.160-p2-work-brief-inheritance-and-tracker-hygiene
---

# V1.160 Delivery Compass

## Scope

本迭代收口 **R-V1159P1-001**（World KB 实体创建后端缺口）——V1.159 发现 `patch_entity` 是 edit-only（pre-reads entity → 500 if not found），导致 "新建 era" 创建入口被 dormant-deferred。同时收口 **R-V1159P1-002**（Work Timeline Brief 层未继承 V1.159 时间带渲染）+ tracker 卫生（归档滞留的 4 个 shipped 行）。

**核心技术发现（PM 代码探索）**：spoke adapter `orchestrate_upsert` **已具备 create 路径**——当 entry absent 时走 `put_create`（`knowledge_entry_port.rs:200`，INSERT + revision=1）。gap 仅在 daemon handler `patch_entity`（`world_kb.rs:273`）：它在 line 301 pre-reads entity 并将 `NotFound` 映射为 `500 DATABASE_ERROR`，从不到达 orchestrator 的 create 路径。dormant 前端已锁定 create 约定：minted `entity_id`（`kb_<uuid>`）+ `expected_version: 0`（era-create-dialog.tsx:109-111, 213）。

**Carrier 决策（architect 待确认）**：`patch_entity` 支持 **create-on-absent**——NotFound 时不再 500，而是 branch 到 create path（`expected_version == 0` → 构建新 `KnowledgeEntry` → `orchestrate_upsert` 自动走 `put_create`）。复用全部现有 machinery（orchestrator create path、`put_create`、`insert_key_block_with_extensions_in_tx`、unique constraint race guard、OCC）。**不新增 route、不新增 wire contract、不新增 orchestrator。**

### Product decisions locked (PM pass 1, code-evidence-driven)

| # | Decision | Lock |
|---|----------|------|
| PD-1 | **收口方向 = entity creation backend + World Brief era create UI 激活** | 解决 R-V1159P1-001。dormant era-create-dialog（V1.159 P1 T3）+ 「新建 era」chrome 解锁；**仅 World Timeline Brief**。 |
| PD-2 | **Carrier = `patch_entity` create-on-absent** | 不新增 route。NotFound + `expected_version == 0` → create via `orchestrate_upsert` → `put_create`。前端已锁 mint id + `expected_version: 0`。成功响应 **HTTP 200**（非 201）。architect pass 2：spec §5.1.2 normative **已落地**（VC-1 锁定；VC-2 确认 deleted=Found，terminal guard 先于 create）。 |
| PD-3 | **Work Brief 时间带继承 = 纯前端只读切片** | R-V1159P1-002：Work Brief 复用 V1.159 `renderBriefTimeBands`；**不**在 Work 面增加 create。`wire_contracts_changed: false`。 |
| PD-4 | **Tracker 卫生 = 归档 4 个滞留 shipped 行 + ERA-TAXONOMY 在 P1 后标 complete** | 4 行移入 shipped archive；`DF-V1123-ERA-TAXONOMY` **仅当** R-V1159P1-001 关闭后 partial→shipped。 |
| PD-5 | **规模 = 2 plans（M）** | P1：handler create-on-absent + World era UI 激活（logic+visual，QA mandatory）；P2：Work Brief 渲染 + tracker（visual+docs，pm-acceptance）。P2 中 ERA-TAXONOMY 行依赖 P1。 |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-08-11-v1.160-p1-world-kb-entity-creation | World KB entity creation (patch_entity create-on-absent) + era create UI activation | Todo | logic+visual; closes R-V1159P1-001; QA mandatory; enables ERA-TAXONOMY complete |
| 2026-08-11-v1.160-p2-work-brief-inheritance-and-tracker-hygiene | Work Brief time-band inheritance + tracker hygiene | Todo | visual+docs; closes R-V1159P1-002; pm-acceptance; ERA-TAXONOMY row after P1 |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain locked) | 2026-08-11 | pending |
| Dev complete (2 plans InReview) | TBD | pending |
| QC complete (tri-review clean) | TBD | pending |
| Iteration close + PR | TBD | pending |

## Acceptance Criteria

迭代级 Done 定义（须可测）。行为 AC 以 daemon/handler 与前端可观察结果为准；spec 落盘由 architect pass 2 收口。

1. **Entity creation backend (happy path)** — `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` 在 path-world 下 entity **absent** 且 `expected_version == 0` 时走 create path（handler → `orchestrate_upsert` → `put_create` → INSERT + `revision=1`），返回 **HTTP 200** + 既有 `WorldKbPatchEntityResponse` shape（`entity` 投影 + `version=1`）。**不**引入 201、不新增 response 字段。
2. **Entity creation guardrails** — (a) entity **exists** + `expected_version == 0` → **409** `WorldKbConflict`（create-on-existing / stale create intent；与现有 OCC 一致）；(b) entity **absent** + `expected_version > 0` → **409**（update-on-absent = client staleness，**不**静默 create）；(c) create 缺 `patch.title` 或 `patch.block_type` → **422** `WorldKbValidation`；(d) `entity_id` 已存在且 `status=deleted`（或其它 terminal）→ **走 Found 路径**，保持既有 terminal/editability 拒绝（**422**），**不**把 deleted 行当成 create-on-absent；(e) 新 `entity_id` + 与已 deleted 行相同的 `canonical_name` 是否可插入，遵循既有 `kb_key_blocks_active_unique`（active-only）——本轮不新开“复活/purge”产品语义。
3. **Cross-author / scope safety** — create path **保持** authz 顺序：`require_world_owner`（path `world_id`）**先于**任何 entity read/branch。foreign **world**（非 owner）→ **403** Forbidden（不因 entity 是否存在而区分）；owner 世界内 entity 属于其它 world → **404** NotFound（既有 cross-world scope 语义，不泄漏外世界细节）。create 不削弱 V1.73 greploop issue-3 修复。
4. **Era create UI 激活（World Timeline Brief only）** — `timeline-canvas.tsx`：`showCreateEra={true}`；`era-create-dialog.test.tsx`：`describe.skip` → `describe`（既有 6 cases un-skip）；Brief empty-state / chrome 暴露「新建 era」。作者可完成：mint `kb_<32-hex>` → `patch-entity` create（`expected_version: 0`, `block_type: era`, title + optional `era_type`/`world_summary`）→ 可选 `patch-relationship`（`custom` + `custom_label: parent_era`）→ refetch graph → 新 era 出现在时间带（含嵌套缩进若有 parent）。**Work Timeline 不**增加 create 入口。
5. **Work Brief 时间带继承** — Work Timeline Brief 层（`work-timeline-canvas-adapter`）复用 V1.159 `renderBriefTimeBands` / `buildEraTree`（与 World Brief 同源组件），将扁平 era markers 升级为类型化嵌套时间带；仍为 **bound-World Brief 的只读投影**（V1.156 Work-Brief PD-2 不变）。无 bound World / 无 era → 诚实空态（既有文案契约，不因本轮退化）。spec `canvas-strategy-surface.md` §3.3.3 现有正文（"Work Timeline Brief inherits the V1.159 rendering" + read-only）**normatively 足够**（VC-3 确认：无需新增句；R-V1159P1-002 为实现缺口，P2 T1 闭环）。
6. **Tracker 卫生** — 将 4 个已 ship 仍滞留 open 的行移入 `shipped-features-tracker.md` §1：`FEAT-WASM-COMPUTE`、`DF-V1122-HARNESS-RENAME`、`DF-V1123-WORLD-MOMENT`、`DF-V1123-WORK-BRIEF`。`DF-V1123-ERA-TAXONOMY` 仅在 **P1 关闭 R-V1159P1-001 后** 从 partial → **shipped/complete**；`deferred-features-cross-version-tracker.md` quick-status + changelog 同步；§2.6 等交叉引用无悬空。
7. **Spec 落盘** — architect pass 2 **已落地**：`entity-scope-model.md` §5.1.2 增补 **normative** `patch_entity` create-on-absent 写边界语义（VC-1 = normative，锁定）。VC-2 确认：`get_knowledge_entry` 无 status filter → deleted/merged 行为 **Found**，terminal guard（`status=="deleted"` → 422）在 Found 臂先于 create 分支触发，terminal 行**永不**进 create-on-absent。VC-3 确认：`canvas-strategy-surface.md` §3.3.3 现有正文（"Work Timeline Brief inherits the V1.159 rendering" + read-only）**normatively 足够**，**无需新增句**；R-V1159P1-002 为实现缺口（P2 T1 闭环），非 spec 缺口。本 pass **已改** `{SPECS_DIR}/entity-scope-model.md`（§5.1.2 + header）；`canvas-strategy-surface.md` 无需改动。
8. **Wire contracts** — **`wire_contracts_changed: false`（FIRM）**：route 仍为 `POST .../kb/patch-entity`；`WorldKbPatchEntityRequest` / `WorldKbPatchEntityResponse` 字段不变；create 约定 = 客户端 mint `entity_id` + `expected_version: 0` + absent row（orchestrator `validate_create_path` / `put_create`）。无新 migration、无新 orchestrator API、无 DTO 版本 bump。
9. **质量门** — 2 plan 均默认 **Findings cleanup: zero-residual** + SDD plan QC tri clean。**P1**（daemon 写路径 + UI 激活）→ **QA gate: mandatory**；**P2**（前端只读渲染 + tracker docs）→ **pm-acceptance**（除非实现中引入非预期行为变更）。R-V1159P1-001 / R-V1159P1-002 在对应 plan Done 后关闭。

## Non-Goals

- **新增 entity creation route**（如 `POST .../kb/create-entity` 或第二 write surface）— 明确排除：carrier = 现有 `patch-entity` 的 create-on-absent。
- **`patch_entity` 重命名 / upsert 品牌化**（route 改名、对外宣传为 upsert API）— 明确排除：wire 名保持 `patch-entity`；create-on-absent 仅在 spec/产品文档中说明。
- **HTTP 201 Created 或新 error code 族** — 明确排除：create 成功保持与 update 相同的 **200 + response shape**；冲突/校验继续用既有 409 / 422 类型。
- **Deleted / merged 实体“复活”或 purge-and-recreate 产品流** — 明确排除：terminal 行保持既有不可编辑语义；不设计 resurrect UX；id/name 边角遵循存储层既有 unique/terminal 规则，本轮不做产品化清理工具。
- **Work Timeline / Work Brief 上的 era create 或任何 Brief 写入口** — 明确排除：Brief 仍是 **World spine**；Work Brief = 只读投影（V1.156）。本轮只激活 **World Timeline Brief**「新建 era」。
- **Brief 层内联拖拽改父子 / 重排时间带** — 明确排除（继承 V1.159 non-goal）：嵌套仍走 `patch-relationship`；时间带为渲染 + create 入口，非 spatial editor。
- **非 era block_type 的 create UI** — 明确排除：handler create-on-absent 是通用机制（任意合法 `block_type` 可经 API create），但 UI/dogfood 只交付 era create。Character/event/等创建入口不做。
- **DF-V1122-FORK-UI**（Fork 创作 UI）— 仍 deferred。
- **DF-V1123-MULTI-TIMELINE** / **DF-V1123-GLOBAL-TIMELINE-MERGE** — 仍 deferred。
- **Dependabot / libp2p 安全清扫** — blocked by upstream；单独迭代。
- **累积 nit 残余簇**（DF-V1122-V1121-RES / DF-V1127-NIT-CLOSEOUT）— 不并入。

## Roadmap Position

- **Current iteration（V1.160）**：收口 V1.159 最高优先级可行动 residual **R-V1159P1-001**（World KB entity create 后端缺口 + World Brief era create UI 从 dormant → production），使 **DF-V1123-ERA-TAXONOMY** 从 partial → complete（读 + 写闭环：类型化嵌套时间带 **与**「新建 era」）。并行收口 **R-V1159P1-002**（Work Brief 继承同一时间带渲染，双表面 Brief 呈现一致）+ tracker 卫生（4 条已 ship 滞留行归档）。技术切片刻意最小：handler create-on-absent + 前端解禁 + 渲染复用；**无**新 route/wire。
- **Previous（V1.159）**：World Brief 类型化嵌套时间带（读路径 + dormant create UI/tests）；显式 defer create 后端与 Work Brief 时间带继承 → 本迭代 residuals。
- **Next iteration（候选，非承诺）**：Fork 创作 UI（**DF-V1122-FORK-UI**，spine 数据已就绪）/ multi-timeline（**DF-V1123-MULTI-TIMELINE**）/ 安全维护清扫（dependabot，待 libp2p 0.57）/ 其它 block_type 的 create UI（若 dogfood 证明 API create 后仍缺入口）。**Owner**：product-manager。**Trigger**：V1.160 ship 后的 dogfood（尤其 era create 冲突率、Work Brief 噪声）、或作者明确要替代历史编辑/安全债阈值。
- **最终目标**：三支柱（Harness·Canvas·Computable）完整 surfacing；Canvas Timeline-first World building 在 Brief 层提供完整 era **创建 + 类型化嵌套导航**（World 写、Work 只读投影一致），World/Work 双表面三层尺度可用且命名自洽。

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | main |
| `spec_integration_branch` | iteration/v1.160 |
| `target_branch` | main |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `expected_version: 0` 在 Found vs NotFound 路径语义混淆（0 既是 create 约定，也曾是“新行/未 bump”心智） | Med | Med | Handler **显式分支**：NotFound → 仅 `expected_version==0` 允许 create；Found → 既有 OCC（0 对 revision≥1 为 409）。两条路径不交叉。Schema 上首写 revision=1（V1.73），不存在“活行 revision=0”的正常态。 |
| Create 缺 title/block_type（update 可从 pre-read 继承，create 不能） | Med | High | Create branch **强制** `patch.title` + `patch.block_type`；复用 `validate_canonical_name` + `validate_body(..., Novel)`；契约测试 422。 |
| Deleted/terminal `entity_id` 被误做成 create-on-absent 或“复活” | Low | High | Found + terminal → 保持 422，不进 create；AC-2/d 与非目标写明；architect 在 spec 中写清 terminal ≠ absent。 |
| Authz 顺序回退导致跨 world 存在性泄漏 | Low | High | 回归保留 `require_world_owner` 优先；P1 测试覆盖 foreign world **403**（无论 entity 是否存在）。 |
| dormant dialog un-skip / Brief empty-state 断言与 `showCreateEra` 不同步导致 CI 红 | Low | Med | 同 PR 翻转 gate + 相关 tests（`era-create-dialog`、`brief-empty-state`、write-boundary）；按 V1.159 canvas 测试惯例修。 |
| `patch-entity` 名承载 create 使 API 消费者困惑 | Low | Low | Spec §5 **normative** create-on-absent + `expected_version: 0` 约定（VC-1 推荐）；route 不改名以保 wire 稳定。 |
| P2 tracker 在 P1 未完成时提前把 ERA-TAXONOMY 标 shipped | Med | Low | P2 T2 **依赖** P1 Done（或同 iteration 合并前最后执行）才翻转 ERA-TAXONOMY；其余 4 行归档可并行。 |
| Work Brief 渲染上下文与 World Brief 不一致（空态/无 bound world） | Low | Med | 复用同一 `renderBriefTimeBands`；保留 Work Brief 诚实空态与 read-only；P2 单测锁“时间带而非扁平 marker”。 |

## Iteration package

> Sibling paths under `{ITERATION_DIR}/v1.160/` — not in `{SPECS_DIR}/` or `{KNOWLEDGE_DIR}/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Purpose |
|------|---------|
| `specs/product-locks.md` | PM pass-1 locked product decisions (PD-1..PD-5); input to architect/writing chain |
| `guides/` | Exploration, process notes (TBD) |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-08-11-v1.160-p1-world-kb-entity-creation | Approve (fix wave 1, revalidated) | mandatory — PASS_WITH_NOTES | R-V1159P1-001 (closed), R-V1159P1-002 (closed by P2) | `sdd/.../review/qc-consolidated.md` |
| 2026-08-11-v1.160-p2-work-brief-inheritance-and-tracker-hygiene | Approve (fix wave 1, revalidated) | pm-acceptance — PASS | none | `sdd/.../review/qc-consolidated.md` |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

- 结晶文档数：0 new (lightweight residual-close iteration; no novel architectural pattern warranting knowledge doc — patch_entity create-on-absent is documented normatively in entity-scope-model.md §5.1.2)
- 新增 CONCEPTS.md 条目：0
- 触发 compound-refresh：否
- R-V1159P1-001 closure confirms the architect's V1.159 finding: the orchestrator already supported create; the gap was handler-only. This is a useful pattern (orchestrator capability vs handler surface gap) but too specific to warrant a knowledge doc.

## Iteration Retrospective (minimal)

- 做得好的：PM code exploration discovered the orchestrator already supported create (put_create) — avoiding unnecessary new route/DTO work. Fix-wave discipline caught real input-validation gaps (whitespace title, malformed entity_id, partial-failure UX) before merge. Tracker hygiene cleared 5 stale rows.
- 可改进的：Tracker hygiene introduced a duplicate-row bug (ERA-TAXONOMY left in §2.3) — QC tri caught it but PM should self-check closing rules when batch-editing rows. web-ui.md/orchestration-engine.md stale pointers were pre-existing (V1.156-era) and only surfaced because the rows finally moved — spec hygiene should track row-moves to pointer-update.
- 下迭代建议：DF-V1122-FORK-UI（Fork 创作 UI，spine 数据已就绪）或转向 multi-timeline / dependabot 安全清扫。
