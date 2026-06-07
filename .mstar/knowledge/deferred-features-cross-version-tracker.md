# Deferred Features — Cross-Version Tracker v1

**Quick status**: **V1.36 Active** · Latest shipped: **V1.35** · Latest active compass: [v1.36-pending-delivery-compass.md](../iterations/v1.36-pending-delivery-compass.md) · FL-E **Shipped in V1.34** · Platform **paused** · V1.35 focus: **CLI IA + critical residual convergence + DF-53 partial shipment** → Done · V1.36 focus: **novel-writing正文产出 UX** (`work_profile: novel`, `Works/<work_ref>/` layout, init preset, chapter pipeline, completion stop) · DF-47 **conditional** (not V1.36 P0) · V1.36 PM distill: **novels-system V1.36 baseline** (DF-60..DF-67 registered, full capability matrix in §3.6.1) · Open FL-D deferrals: **DF-29, DF-31, DF-56** · Tech debt SSOT: [`status.json`](../status.json) (`total_open`: 28, `critical`: 0)

**Status**: Active  
**Purpose**: Single source of truth for **open** and **backlog** features/tech-debt deferred from delivery compasses. Closed/shipped history lives in [shipped-features-tracker.md](../archived/shipped-features-tracker.md).  
**Scope**: `nexus` OSS repository only. Platform features referenced only when they block nexus-side work.  
**Predecessor**: Consolidated from delivery compasses (v1.2–v1.21) and the v1.2 reclassification matrix.  
**Created**: 2026-04-21  
**Last updated**: 2026-06-07 (V1.36 novels-system distill: DF-60..DF-67 registered; §3.6 Reference system distills established; novel-workflow-profile §3.1/§4.1/§4.2/§4.3/§5 expanded with 4 gap-fills from distill; P0/P2/P3 plans updated with T8-T11/T7-T10/T7-T9 tasks)

---

## 1) How to use this file

- **Product decisions (not deferrals)**: See §3.1 Program planning decisions (PD-*).
- **Future product lines (cross-version themes)**: See §3.2 Future product lines (FL-*).
- **Planning a new version**: Scan §3.3 Open features for items targeting that version or "Any future".
- **Closing an item**: Remove its row from §3.3; append to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) with completion version, plan-id, and note.
- **Deferring again**: Update the `Target` column; keep the row in §3.3. Add a note in `Deferral history`.
- **Shipped / cancelled history**: [shipped-features-tracker.md](../archived/shipped-features-tracker.md) (§1 closed items, §2 per-version snapshots).
- **Tech-debt residuals**: [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary` (§3.5 pointer only).
- **Source of truth**: This file is the **tracker**; the **compass** of the active version is the **scope authority**. If this file and the active compass conflict, the compass wins.

---

## 2) Lifecycle status definitions

| Status | Meaning |
|--------|---------|
| **Open** | Item has not been implemented. May have a target version assigned, or be in backlog. |
| **Shipped** | Implemented and merged in the indicated version (record in archive). |
| **Cancelled** | Explicitly removed from scope (no longer planned). |
| **Superseded** | Replaced by a different approach; original item no longer relevant. |

---

## 3) Open items

### 3.1 Program planning decisions

Recorded product rulings for iteration planning. **Not** implementation tasks — the active delivery compass is scope authority. Closed PD-02..12 → [shipped archive §2](../archived/shipped-features-tracker.md).

| ID | Decision | Notes |
|----|----------|-------|
| PD-01 | **World fork is platform-only** | Community/social feature; **no** local `nexus42` CLI or daemon fork. See DF-45 (Cancelled) in archive. |
| PD-05 | Cloud sync is **not** a short-term iteration focus | CLI `sync push/pull` unchanged; orchestration `sync.pull`/`sync.push` stubs remain Open |
| PD-08 | Preset orchestration + Agentic Design Patterns | See FL-D; research: https://github.com/evoiz/Agentic-Design-Patterns |

### 3.2 Future product lines (planning backlog)

Cross-version themes. Suggested targets are non-binding until locked in a compass. Shipped FL-A/B → archive §2 V1.29.

| ID | Product line | Suggested target | Notes |
|----|--------------|------------------|-------|
| FL-D | **Preset orchestration** (Agentic Design Patterns) | Post-V1.34 | V1.31–32 shipped capabilities + quality gate; **still open**: DF-29, DF-31, **conditional routing** (DF-56; OUT of V1.34) |
| FL-E | **Generic creator workflow** (intake → research → draft → review → persist) | **V1.34** | **Shipped in V1.34** — [creator-workflow.md](specs/creator-workflow.md) (Status: Shipped V1.34) + compass [v1.34](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md). FL-E closed in V1.34 with 5 plan P0–P5; minimal preset chain (research → produce → review → persist) + Work `stage`/`stage_status` + linear gates + active schedule uniqueness (P1) + preset chain wiring (P2). `--auto-chain` default still DF-53. |

### 3.3 Open features (deferred from compass "Out" or audit)

| ID | Feature | First deferred | Target | Effort | Deferral history | Notes |
|----|---------|---------------|--------|--------|-----------------|-------|
| DF-12 | Dual outbox consolidation (full merge) | V1.2 | Any future | L | V1.2 | Knowledge: `dual-outbox-architecture.md` (archived). Single-writer follow-up. |
| DF-13 | Entitlements API consumption | V1.3 | V2.0+ | M | V1.3 | Platform API dependency. |
| DF-16 | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2→V1.3 | ADR-011/012/013. Platform dependency. |
| DF-29 | `registry.refresh` (synthetic output) | V1.21 audit | Any future | M | V1.21 | **Out of V1.31/V1.32** — needs network/CDN. `builtins/registry.rs`. |
| DF-31 | `workspace.open` / `workspace.commit` stubs | V1.21 audit | Any future | M | V1.21 | **Out of V1.31/V1.32** — `nexus-home-layout` wiring deferred. |
| DF-40 | Session resume stub in daemon lifecycle | V1.21 audit | Any future | S | V1.21 | `daemon-runtime/lifecycle/actions.rs`. |
| DF-41 | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-42 | Full Local API redesign for World/User KB | V1.24 (KCA-003) | Any future | L | V1.24 | `/v1/local/kb/*` full scoping redesign. |
| DF-43 | SQLite persistence / crate-model alignment | V1.24 audit | Any future | M | V1.26–28 partial | Production owner = `nexus-local-db`; see decision note below. |
| DF-44 | Reference body externalization — refreshable scan pipeline | V1.26 | Any future | M | V1.26 | Static registration shipped; auto-refresh Open. |
| DF-46 | Full `nexus.*` logical capability implementation (acp-capability-set parity) | V1.34 audit | Post-V1.34 | L | V1.34 | V1.34 ships minimal host tools only; see [agent-nexus-tool-bridge.md](specs/agent-nexus-tool-bridge.md). |
| DF-47 | Host tool + `worker/agent_tool_request` unified registry | V1.34 audit | **Conditional (V1.36+)** | M | V1.34→V1.35→V1.36 | V1.34 P4 shipped adapter; V1.35 P0 deferred carry-forward: **production caller wiring OPEN**. V1.36 compass revises: **not P0** unless novel-writing UX is blocked (e.g. agent must patch Work files during drafting). Default: backlog after P3 |
| DF-48 | Agent tool bridge via `nexus42` CLI subprocess | V1.34 | Post-V1.34 | M | V1.34 | Rejected; daemon HostToolExecutor is SSOT. |
| DF-49 | Standalone MCP server for Nexus capabilities | V1.34 | Backlog | L | V1.34 | Separate from ACP agent path. |
| DF-50 | skills-export publishable L1 capability matrix | V1.34 | Post-V1.34 | M | V1.34 | Full matrix; minimal mapping in P3. |
| DF-51 | `creator.inject_prompt` wire/schema alignment | V1.33 compass §6 | V1.34+ | S | V1.33→V1.34 | **Closed in V1.34 residual-convergence** (commits a044f94 + 71c10cc). Schema now declares `prompt_file` + `vars` with `anyOf`. Closure recorded in [`.mstar/archived/residuals/v1.32-post-qc-tech-debt.json`](../archived/residuals/v1.32-post-qc-tech-debt.json) (R-P2-01). |
| DF-52 | Top-level `nexus42 preset` command group | V1.33 | Any future | S | V1.33 | Use `creator run` + `system preset`. |
| DF-53 | FL-E `--auto-chain` default stage sequencing | V1.34 | V1.36+ (partial shipped V1.35 P4) | S | V1.34→V1.35 | V1.35 P4 partial **shipped**: `--chain-novel-writing` defaults true (intake → produce); clap opt-out syntax `--chain-novel-writing=false` works. Full multi-stage auto-chain remains a V1.36+ exploration; DF-53 stays open until full chain landed |
| DF-54 | Work `stage` / `stage_status` persistence gap | V1.34 | V1.34+ | S | V1.34 | **Closed in V1.34 P1** (commits 655d71c + R-FL-E-01..08 on `feature/v1.34-fl-e-run-intents-and-stages`). Stage columns added + DDL migration + 5 hermetic e2e tests + active schedule uniqueness. |
| DF-55 | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | V1.34: local/read-only or `policy_blocked` (PD-05). |
| DF-56 | Conditional routing / branching engine | V1.33 | Post-V1.34 | L | V1.33→V1.34 | OUT of V1.34/V1.35; see [preset-conditional-routing.md](specs/preset-conditional-routing.md). |
| DF-57 | `Works/<work_ref>/` artifact layout + sync scan migration | V1.36 prepare | **V1.36 P2** | M | V1.36 | Pre-1.0: no legacy `Stories/<story_ref>/` shims. Spec: [novel-workflow-profile.md](specs/novel-workflow-profile.md) §3; plan `2026-06-07-v1.36-novel-artifact-layout-and-templates` |
| DF-58 | Interactive novel project init preset (`novel-project-init`) | V1.36 prepare | **V1.36 P1** | M | V1.36 | Separate grill-me preset; not embedded in `novel-writing` auto-chain. Plan `2026-06-07-v1.36-novel-project-init-preset` |
| DF-59 | Platform publish integration for novel正文 | V1.36 prepare | **Backlog** | L | V1.36 | Explicit OUT of V1.36 short-term scope; user may publish manually. See compass §1.2 non-goals |
| DF-60 | Multi-novel auto-switch ceremony (8-step new-book switch + 2h switch lock) | V1.36 distill | **Backlog (V1.37+)** | L | V1.36 | novels-system pattern: `novel-write` triggers `{小说目录}/.switch-lock.json` on completion; 8 steps to register new InStreet workId, init new `作品目录`, swap `novel:active`, release lock; 2h timeout enforced by all three crons. **OSS deferred** — V1.36 explicit non-goal (compass §6.3). When re-opened: must use local DB lock + `creator run start --continue-from <old_work_id>` to invoke, not Redis |
| DF-61 | Selection pool / multi-novel idea tracking (`选题库.md` / 灵感池) | V1.36 distill | **Backlog (V1.37+)** | M | V1.36 | novels-system has `小说/选题库.md` (status: 当前在写 / 排队开坑 / 已完结) + `小说/灵感池/`. OSS Work is **single-Work** (work-experience-model §3); multi-novel idea tracking is intentionally OUT. When re-opened: should be a new `work_profile: novel-pool` or a separate CLI command, not Work |
| DF-62 | Multi-volume auto-chronology (per-volume outline + chapter range tracking) | V1.36 distill | **V1.37+** (planned when multi-chapter ships) | M | V1.36 | novels-system has `大纲/分卷总纲/` per volume (each .md with 卷名/范围/总字数/章节概要表/伏笔管理/卷末三要素). V1.36 §3.1 has `Outlines/volume-outline.md` (optional) + chapter frontmatter `volume: integer` (V1.36 leaves blank; V1.37+ uses). When multi-chapter ships, plan a `2026-XX-v1.3X-multi-volume-chronology` plan |
| DF-63 | Worldbuilding 7 sub-categories schema (foundation/background/character/location/society/rules/economy) | V1.36 distill | **Backlog (V1.37+)** | M | V1.36 | novels-system has 7 `世界设定/{基础,故事背景,人物,位置,社会与组织,规则与系统,经济与政治}/` subdirs, each item is a .md with frontmatter `type: 基础设定 | name | tags | source_chapters | updated`. V1.36 spec §3.1 has `Worldbuilding/` (optional) with `character.md` stub only. When re-opened: spec must add type taxonomy; sync module must NOT scan these |
| DF-64 | Findings lifecycle (review → brainstorm → write coordination; 3-role) | V1.36 distill | **Backlog (V1.37+)** | L | V1.36 | novels-system has `Redis novel:{作品名}:state.findings[]` with full lifecycle (pending → resolved/archived), severity enum (critical/major/minor/informational), `targetExecutor: brainstorm | write | none`, 96h `needsMasterDecision` timeout. V1.36 single-role (`novel-writing` produce) has no separate review cron; llm_judge NOGO is a lighter-weight gate. When re-opened: 3-role split requires `novel-brainstorm` / `novel-write` / `novel-review` as separate presets; findings backplane = local DB table, not Redis |
| DF-65 | Three-layer rules architecture (`writing-craft-rules.md` / `novel_rules.md` / `novel_rules_history.md`) | V1.36 distill | **Backlog (V1.37+)** | M | V1.36 | novels-system rule split: cross-work通用 → `writing-craft-rules.md` (shared); per-work specific → `novel_rules.md`; full experience原文 (append-only) → `novel_rules_history.md`. V1.36 has `work-status.md` + `ch<nn>-outline.md` only. V1.36 P3 ships a **minimal** 五问 inline in finalize prompt (no per-work rules file). When re-opened: spec `creator-rules-architecture.md` (new Feature line); write-back protocol for `novel-review` cron to update `novel_rules.md` |
| DF-66 | Per-chapter log subdirectories (write/review/brainstorm/publish at `Works/<work_ref>/Logs/`) | V1.36 distill | **Backlog (V1.37+)** | S | V1.36 | novels-system has `日志/{迭代日志,写作日志,发布记录,构思日志}/` per-work; each log has file naming, status machine (fresh→digested→archived), and write-discipline rules per log type. V1.36 §3.1 has `Logs/` (optional) with no structure. When re-opened: add log-template assets + write-discipline spec; sync module MUST NOT scan |
| DF-67 | Master-decision timeout (96h finding escalation) | V1.36 distill | **Backlog (V1.37+)** | S | V1.36 | novels-system: findings with `targetExecutor: "master"` past 96h → mark `needsMasterDecision: true`; surfaced via `activity-report` cron to user. V1.36 has no review cycle and no `master` actor (deferral goes to V1.36 user via status banner only). When re-opened: local DB scheduled task; user-visible surface is `creator run status` banner |

#### DF-43 decision note — Reference sources persistence

**Status:** Production persistence owner decided (V1.25 Theme C); crate-model alignment **remains open**.

1. **`nexus-local-db`** owns production `reference_sources` in `state.db`.
2. **`nexus-knowledge::ReferenceSource`** remains in-memory crate model until a follow-up adapter plan.

See [2026-05-23-v1.26-reference-store-layout](../plans/2026-05-23-v1.26-reference-store-layout.md). Re-evaluate when `nexus-knowledge` proposes a SQLite/file-backed adapter with migration plan.

### 3.4 Backlog (no committed target version)

| ID | Feature | First deferred | Target | Effort | Notes |
|----|---------|---------------|--------|--------|-------|
| DF-03 | Preset third-party registry / signing / publish | V1.4 | Backlog | XL | Potentially independent project. |
| BL-01 | World Merge complete execution / rollback | V1.2 | Backlog | XL | Spec: `platform/world-merge-execution-backlog-v1.md`. |
| BL-02 | Local Shadow Read / staged change full chain | V1.2 | Backlog | L | Requires product spec. |
| BL-03 | Advanced declarative Context Assembly API / DSL | V1.2 | Backlog | XL | Spec: `platform/context-assembly-advanced-dsl-backlog-v1.md`. |
| BL-04 | Long-running task checkpoint (product-level) | V1.2 | Backlog | M | |
| BL-05 | Commonware / multi-workspace advanced narrative | V1.2 | Backlog | XL | |
| BL-06 | Independent search microservice | V1.2 | Backlog | L | |
| BL-07 | Explore ranking / cold-start + Publish compliance matrix | V1.2 | Backlog | M | ADR-011 elevated. |
| BL-08 | Social / marketing features | V1.3 | V2.0+ | XL | ADR-011/012/013. |

### 3.5 Open tech-debt residuals (SSOT pointer)

**Machine state**: [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary` (`status.json.updated_at` **2026-06-07**; `tech_debt_summary.updated_at` **2026-06-07**). Do **not** mirror full rows here — JSON wins on conflict. Total open: **28** (0 critical, 0 high, 8 medium, 15 low, 5 nit).

| Bucket | Open count | `residual_findings` key |
|--------|------------|-------------------------|
| V1.30 post-QC | 9 | `v1.30-post-qc-tech-debt` (V1.35 P0 closed 2: TD-V130-02, TD-V130-06) |
| V1.31 post-QC | 5 | `v1.31-post-qc-tech-debt` (V1.35 P0 closed 3: TD-V131-01, TD-V131-03, TD-V131-04) |
| V1.33 work model (P1) | 3 | `2026-06-04-v1.33-work-model-and-creator-run` (R-V133P1-03/08/09; partial / defer; not P0) |
| V1.33 llm_judge (P3) | 2 | `2026-06-04-v1.33-llm-judge-runtime-fix` (V1.35 P0 closed 2 critical: R-V133P3-01, R-V133P3-02; remaining 2 medium: R-V133P3-03/04) |
| V1.33 memory review (P4) | 3 | `2026-06-04-v1.33-memory-review-closed-loop` (V1.35 P0 closed 4 critical: R-V133P4-01, R-V133P4-02, R-V133P4-03, R-V133P4-07; remaining 3 medium: R-V133P4-04, R-V133P4-05, R-V133P4-06) |
| V1.34 FL-E stages (P1) | 5 | `2026-06-04-v1.34-fl-e-run-intents-and-stages` (R-FL-E-DDL/DEAD/LIST/FNAME/ENDP, all low; not P0) |
| V1.34 agent tool (P4) | 1 | `2026-06-04-v1.34-agent-tool-implementation` (DF-47 production caller wiring → conditional V1.36+) |
| **Total** | **28** | See `metadata.tech_debt_summary.total_open` (after V1.35 P0) |

**Closed / historical residuals**

- V1.30 convergence (R5–R20 fixed): [`archived/residuals/v1.30-residual-convergence.json`](../archived/residuals/v1.30-residual-convergence.json)
- V1.13 forward delivery (R-V113-005 waived, R-V113-007 resolved): [`archived/residuals/2026-05-06-v1.13-oss-forward-delivery.json`](../archived/residuals/2026-05-06-v1.13-oss-forward-delivery.json)
- V1.33 P1 (4 closed via fix waves): [`archived/residuals/2026-06-04-v1.33-work-model-and-creator-run.json`](../archived/residuals/2026-06-04-v1.33-work-model-and-creator-run.json)
- V1.32 (R-P2-01/02 closed via V1.34 P0): [`archived/residuals/v1.32-post-qc-tech-debt.json`](../archived/residuals/v1.32-post-qc-tech-debt.json)
- **V1.34 PR #42 cursor automation** (2 medium resolved in 3b24aaf: R-CURSOR-PR42-01 permission policy bypass; R-CURSOR-PR42-02 FL-E force default): [`archived/residuals/2026-06-04-v1.34-pr-42-cursor-automation.json`](../archived/residuals/2026-06-04-v1.34-pr-42-cursor-automation.json)
- **V1.35 P0** (11 closed: 6 V1.33 criticals + 1 V1.34 medium R-CURSOR-PR42-03 + 4 V1.30/31 backlog — see [`.mstar/archived/residuals/2026-06-04-v1.33-llm-judge-runtime-fix.json`](../archived/residuals/2026-06-04-v1.33-llm-judge-runtime-fix.json), [`.mstar/archived/residuals/2026-06-04-v1.33-memory-review-closed-loop.json`](../archived/residuals/2026-06-04-v1.33-memory-review-closed-loop.json), [`.mstar/archived/residuals/2026-06-04-v1.34-cursor-pr42-stage-status.json`](../archived/residuals/2026-06-04-v1.34-cursor-pr42-stage-status.json), [`.mstar/archived/residuals/v1.30-post-qc-tech-debt.json`](../archived/residuals/v1.30-post-qc-tech-debt.json), [`.mstar/archived/residuals/v1.31-post-qc-tech-debt.json`](../archived/residuals/v1.31-post-qc-tech-debt.json))
- Cross-cutting accept items (e.g. DEBT-RAND-073): `status.json` → `metadata.tech_debt_summary.cross_cutting`

---

### 3.6 Reference system distills (V1.36 baseline)

**Purpose**: capture research snapshots of production-grade reference systems that informed V1.36 spec/plan decisions. Future iterations (V1.37+) may extend these distills or use them as a research starting point when re-opening the deferred items above.

#### 3.6.1 Novels-system V1.36 baseline (2026-06-07)

**Source**: internal reference at `~/workspace/organizations/42ch/internal-sharing/novels-system/` (Obsidian + Redis + InStreet literary API; multi-novel, multi-role, multi-chapter serial production system).

**Distilled by**: `@project-manager` (PM), 2026-06-07 V1.36 prepare wave (after V1.35 shipped, before P0 dispatch). Audit evidence: [v1.36-pending-delivery-compass.md §0.1 grill decisions](../iterations/v1.36-pending-delivery-compass.md) + novels-system files: `shared-rules/novel-system-rules.md` (790 lines), `cron-prompts/{novel-brainstorm,novel-write,novel-review,novel-publish}.md`, `schemas/{novel-active,novel-state,novel-review-iteration}.schema.json`, `templates/novel/*.md` (20 templates).

**V1.36 north star** (from compass §0): *Complete the novel-writing正文产出 journey on generic Work — from project scaffold through one polished chapter — without platform publish and without legacy layout shims.*

##### Capability matrix (novels-system × V1.36 disposition)

| Capability area | novels-system | V1.36 disposition | Tracker row |
|---|---|---|---|
| **Layout root** | `{作品目录}/` (per-work, 7 subdirs) | In-scope: `Works/<work_ref>/` + 4 subdirs (Stories, Outlines, Worldbuilding, Logs) | DF-57 |
| **Chapter file naming** | `第{N}章.md` (Chinese) | In-scope: `ch<nn>-<slug>.md` (English; international OSS) | (impl detail) |
| **Chapter frontmatter** | `title/chapter/volume/status/word_count/tags/created/updated` | In-scope: `title/chapter/volume (optional)/status/word_count`; P3 T10 forward-compat | (impl detail) |
| **Chapter state machine** | ⬜→✏️→📝→✅→🚀 (with `published`) | In-scope: `not_started`/`outlined`/`draft`/`finalized`/`published` (`published` reserved) | (impl detail) |
| **Work status doc** | frontmatter + basic info + character table + chapter table(6 cols) + 最近更新(5) + 外部链接 + 规则引用 | In-scope: frontmatter + chapter table(6 cols); logs/external links OUT (single-user, no publish) | DF-57 |
| **Outlines/ tree** | 分卷总纲/ + 单章细纲/ + 事件索引 + 逻辑异常 + 伏笔索引 | In-scope: chapter outline (required) + volume outline (optional) + foreshadowing.md (empty stub) + event-index.md (empty stub) | (impl detail) |
| **Worldbuilding** | 7 sub-types with item templates (foundation/background/character/location/society/rules/economy) | In-scope: `Worldbuilding/character.md` optional stub; full taxonomy OUT | DF-63 |
| **Logs/** | 4 sub-types (写/迭代/构思/发布) with status machines | In-scope: `Logs/` optional root only; structure OUT (single-role) | DF-66 |
| **Completion detection** | `currentChapter==totalPlanned` + all chapters `published` | In-scope: `current_chapter>=total` + all `finalized` + `intake==complete` (no publish) | (impl detail) |
| **完本后同步** | 5-step ceremony (frontmatter/table/Redis×2/selection pool) | In-scope: 2-step reduced (Work.status + work-status.md); ceremony OUT | DF-60 |
| **Auto new-book switch** | 8-step + 2h switch lock + 中断恢复 | OUT (V1.36 compass §6.3 explicit non-goal) | DF-60 |
| **Quality loop** | review cron + 五问质量检验 + findings lifecycle + 96h 升级 | In-scope: `llm_judge` exit_when on `finalize` (V1.36 quality gate); full review cron + findings OUT | DF-64 / DF-67 |
| **两轮写作** | 初稿→终稿 (各带日志) | In-scope: outline→draft→finalize; 两轮合一 (no separate terminal/refine) | (impl detail) |
| **State storage** | Redis (novel:active / novel:{名}:state / novel:review-iteration) | In-scope: local SQLite (state.db); Redis OUT (OSS local-only) | (PD-05) |
| **Platform publish** | InStreet literary API + workId UUID + chapter post API | OUT (V1.36 compass §1.2) | DF-59 |
| **Selection pool / 灵感池** | Obsidian 选题库 + 灵感池 | OUT (multi-novel is OSS non-goal) | DF-61 |
| **Three-layer rules** | writing-craft-rules.md / novel_rules.md / novel_rules_history.md | OUT (V1.36 ships 五问 inline in finalize prompt; per-work rules file deferred) | DF-65 |
| **Multi-volume auto-chronology** | per-volume outline + chapter range tracking | OUT (V1.36 single-chapter; `volume: integer` frontmatter is forward-compat) | DF-62 |
| **Three-cron staggering** | brainstorm 03/09/15/21 / write 04/10/16/22 / review :00/:30 | OUT (V1.36 single-role; multi-role staggering is V1.37+) | (with DF-64) |
| **Switch lock + 2h timeout** | file-based lock at `{小说目录}/.switch-lock.json` | OUT (V1.36 has no auto-switch) | DF-60 |
| **Master-decision timeout (96h)** | finding escalation; surfaced via activity-report cron | OUT (V1.36 no review cycle) | DF-67 |

##### V1.36 implementation of in-scope items (PM approved 2026-06-07)

| novels-system feature | V1.36 implementation | Spec/plan ref |
|---|---|---|
| Chapter finalize quality gate (五问) | `exit_when: kind: llm_judge` on `finalize` state in `novel-writing` preset; template `finalize-exit.md` | novel-workflow-profile §5.1; plan P3 T7 |
| 6-column chapter table | work-status.md chapter table: 卷/范围/预计/实际/完成度/状态 | novel-workflow-profile §4.1; plan P2 T9 |
| `volume: integer` frontmatter | Forward-compat field; V1.36 leaves blank | novel-workflow-profile §4.3; plan P2 T10 |
| `Outlines/foreshadowing.md` | Empty stub with F### table header | novel-workflow-profile §3.1/§3.2; plan P2 T7 |
| `Outlines/event-index.md` | Empty stub with E### table header | novel-workflow-profile §3.1/§3.2; plan P2 T8 |
| Foreshadowing required in outline | §4.2 promotes foreshadowing from optional to required | novel-workflow-profile §4.2; plan P3 T9 |
| `Worldbuilding/character.md` | Optional stub for main characters | novel-workflow-profile §3.1 |

##### Re-open instructions for V1.37+ implementers

When V1.37+ picks up multi-chapter or multi-novel work:

1. **Read** the novels-system source files listed above (`shared-rules/novel-system-rules.md` is the SSOT; cross-reference `cron-prompts/` for behavior contracts).
2. **For each V1.36+ deferred item (DF-60..DF-67)**, design a plan that maps the reference pattern to OSS constraints:
   - Replace Redis → local DB table
   - Replace Obsidian file tree → `Works/<work_ref>/` subdir
   - Replace InStreet literary API → CLI-only or platform `nexus-cloud-sync` boundary
   - Replace cron schedule → orchestration `Schedule` with `fl_e_stage` wire key (per [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md))
3. **Update the spec the spec covers**:
   - DF-60/61 → new spec `novel-multi-work-lifecycle.md` (or extension to novel-workflow-profile)
   - DF-62 → new spec section in novel-workflow-profile §3.1 + chapter frontmatter becomes required
   - DF-63 → new spec `novel-worldbuilding-schema.md` (Companion)
   - DF-64/67 → new spec `novel-quality-loop.md` (Feature line)
   - DF-65 → new spec `novel-rules-architecture.md` (Feature line)
   - DF-66 → spec extension to `Logs/` section
4. **Register the new spec + plan in `status.json`** per mstar-plan-artifacts lifecycle.
5. **Update the deferred tracker** to record the new spec/plan closure (per §4 change control).

#### 3.6.2 Future distills

Future iterations may add new distills here. Each distill should be a single subsection with:

- Source (path / URL / repo)
- Distilled by + date
- Iteration that consumed it
- Capability matrix (source system × OSS disposition)
- Implementation table (in-scope items × spec/plan ref)
- Re-open instructions

This convention is established by the V1.36 novels-system distill above. Extend, do not replace.

---

## 4) Change control

- **Shipped rows**: Move from §3.3 to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **Effort estimates**: XS/S/M/L/XL agent-session scale; guidance only. See `effort-estimation.md`.
- **Residual detail**: `status.json` wins over this file for machine-state residuals.

---

## 5) Related index

**Latest shipped iteration**

- V1.35 delivery compass: [v1.35-cli-ia-and-product-polish-delivery-compass-v1.md](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md) (Shipped 2026-06-07; PR [#43](https://github.com/42ch-dev/nexus/pull/43) awaiting merge to main)

**Latest active iteration**

- **V1.36** (Active 2026-06-07): [v1.36-pending-delivery-compass.md](../iterations/v1.36-pending-delivery-compass.md) — **novel-writing正文产出 UX** (P0 spec lock → P1 init preset → P2 layout/templates → P3 chapter pipeline → P4 completion/hygiene); DF-47 conditional; DF-57/58 in-scope; DF-59 publish OUT; opportunistic V1.30/31 residual sweep
- **V1.35** (Shipped 2026-06-07): [v1.35-cli-ia-and-product-polish-delivery-compass-v1.md](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md) — CLI IA (5 groups; sync→platform), creator hub polish, critical residual P0 (6 criticals + R-CURSOR-PR42-03 + 5 backlog), FL-E UX polish (chain default true); 5 implement plans P0/P2/P3/P4/P5 + prepare P-1 + P1 docs all Done; DF-47 → V1.36 P0
- **V1.34** (Shipped 2026-06-05): [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) — FL-E + Agent tools; DF-47 carried forward to V1.35 P0 (now V1.36 P0)

**Recent shipped compasses** (detail in archive §2)

- V1.34: [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) — FL-E + Agent tools (8 `nexus.*` tool bridge); **Shipped 2026-06-05** (5 plans P0–P5 all Done); DF-47 → V1.35 P0 → V1.36 P0
- V1.33: [v1.33-work-experience-loop-delivery-compass-v1.md](../iterations/v1.33-work-experience-loop-delivery-compass-v1.md) — narrative Work loop, Creative Brief Intake, `creator run`, `llm_judge` fix, memory review closed loop; **Shipped 2026-06-04** (5 plans P1–P5 all Done)
- V1.32: [v1.32-preset-quality-gate-delivery-compass-v1.md](../iterations/v1.32-preset-quality-gate-delivery-compass-v1.md)
- V1.31: [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)
- V1.30: [v1.30-residual-convergence-delivery-compass-v1.md](../iterations/v1.30-residual-convergence-delivery-compass-v1.md)

**Knowledge & specs**

- Shipped history archive: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)
- Done plans index: [archived/plans-done.json](../archived/plans-done.json)
- CLI IA (V1.35): [specs/cli-command-ia.md](specs/cli-command-ia.md), [specs/creator-centric-entry-model.md](specs/creator-centric-entry-model.md), [specs/preset-conditional-routing.md](specs/preset-conditional-routing.md); audit evidence in [v1.35 compass Appendix A](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md#appendix-a-cli-usability-audit-v135)
- Orchestration engine: [specs/orchestration-engine.md](specs/orchestration-engine.md)
- Creator schedule & core context: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)
- Iteration index: [iterations/README.md](../iterations/README.md)
- Machine state: [status.json](../status.json)

External (via `.mstar/local-paths.json`): `{v1-spec}/architecture/v1.md`, `{platform-designs}/roadmap.md`

---

*Last updated: 2026-06-07. Status: V1.36 Active; V1.35 Shipped 2026-06-07; 28 open residuals (0 critical); tracker quick status aligned to status.json. 8 novel-system deferred items (DF-60..DF-67) + 1 distill section (§3.6.1) registered for V1.37+ re-open.*
