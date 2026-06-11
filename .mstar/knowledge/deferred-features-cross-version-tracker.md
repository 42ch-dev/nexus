# Deferred Features — Cross-Version Tracker v1

**Quick status**: **V1.42 Active** (prepare 2026-06-11 on `iteration/v1.42` — P0 runtime_lock + P1 DF-62 multi-volume + P2 DF-56 + P3 DF-47 + P-last UX) · **V1.41 Shipped** (2026-06-11 PR #53) · **V1.40 Shipped** · Platform **paused** · DF-60/61 **archived** · Tech debt SSOT: [`status.json`](../status.json)

**Status**: Active (V1.42 prepare — compass locked; implement Todo)  
**Purpose**: Single source of truth for **open** and **backlog** features/tech-debt deferred from delivery compasses. Closed/shipped history lives in [shipped-features-tracker.md](../archived/shipped-features-tracker.md).  
**Scope**: `nexus` OSS repository only. Platform features referenced only when they block nexus-side work.  
**Predecessor**: Consolidated from delivery compasses (v1.2–v1.21) and the v1.2 reclassification matrix.  
**Created**: 2026-04-21  
**Last updated**: 2026-06-11 (V1.42 prepare: DF-60/61 → shipped archive; DF-62/56/47 → V1.42 Active; `iteration/v1.42` opened)

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
| DF-40 | Session resume stub in daemon lifecycle | V1.21 audit | **Converged via DF-68 (V1.39 P0 Shipped)** | S | V1.21→V1.39 | `daemon-runtime/lifecycle/actions.rs`. V1.39 P0 conditional boot auto-resume for checkpointed auto-chain drivers supersedes blanket pause-only recovery for those schedules. DF-68 implemented: `find_resumable_works` + boot logging in `boot.rs`. |
| DF-41 | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-42 | Full Local API redesign for World/User KB | V1.24 (KCA-003) | Any future | L | V1.24 | `/v1/local/kb/*` full scoping redesign. |
| DF-43 | SQLite persistence / crate-model alignment | V1.24 audit | Any future | M | V1.26–28 partial | Production owner = `nexus-local-db`; see decision note below. |
| DF-44 | Reference body externalization — refreshable scan pipeline | V1.26 | Any future | M | V1.26 | Static registration shipped; auto-refresh Open. |
| DF-46 | Full `nexus.*` logical capability implementation (acp-capability-set parity) | V1.34 audit | Post-V1.34 | L | V1.34 | V1.34 ships minimal host tools only; see [agent-nexus-tool-bridge.md](specs/agent-nexus-tool-bridge.md). |
| DF-47 | Host tool + `worker/agent_tool_request` unified registry | V1.34 audit | **V1.42 P3 Active** | M | V1.34→V1.35→V1.36→V1.42 | V1.34 P4 shipped adapter. **V1.42 P3** minimal slice: **one** real schedule E2E tool upcall. Full DF-46 parity remains Post-V1.42. Plan: [2026-06-11-v1.42-agent-tool-production-wiring.md](../plans/2026-06-11-v1.42-agent-tool-production-wiring.md). |
| DF-48 | Agent tool bridge via `nexus42` CLI subprocess | V1.34 | Post-V1.34 | M | V1.34 | Rejected; daemon HostToolExecutor is SSOT. |
| DF-49 | Standalone MCP server for Nexus capabilities | V1.34 | Backlog | L | V1.34 | Separate from ACP agent path. |
| DF-50 | skills-export publishable L1 capability matrix | V1.34 | Post-V1.34 | M | V1.34 | Full matrix; minimal mapping in P3. |
| DF-51 | `creator.inject_prompt` wire/schema alignment | V1.33 compass §6 | V1.34+ | S | V1.33→V1.34 | **Closed in V1.34 residual-convergence** (commits a044f94 + 71c10cc). Schema now declares `prompt_file` + `vars` with `anyOf`. Closure recorded in [`.mstar/archived/residuals/v1.32-post-qc-tech-debt.json`](../archived/residuals/v1.32-post-qc-tech-debt.json) (R-P2-01). |
| DF-52 | Top-level `nexus42 preset` command group | V1.33 | Any future | S | V1.33 | Use `creator run` + `system preset`. |
| DF-53 | FL-E `--auto-chain` default stage sequencing | V1.34 | **V1.39 P0 Shipped** | S | V1.34→V1.35→V1.36→V1.37→V1.38→V1.39 | V1.35 P4 partial **shipped**: `--chain-novel-writing` defaults true (intake → produce). V1.38 shipped multi-chapter foundation **without** auto-reenqueue. **V1.39 P0** implements full `intake → research → produce → review → persist` auto-chain (default true), chapter outer loop, side-input lane, boot recovery, `--no-auto-chain` opt-out, `creator run resume` command. Core: `nexus-orchestration::auto_chain` module with `evaluate_next_step` + 15 unit tests + 14 integration tests. Plan: [2026-06-09-v1.39-fl-e-auto-chain-engine.md](../plans/2026-06-09-v1.39-fl-e-auto-chain-engine.md). **Tri-review + targeted re-review all Approve; final consolidated gate Approve. PR #50 merged ad9725d8.** |
| DF-54 | Work `stage` / `stage_status` persistence gap | V1.34 | V1.34+ | S | V1.34 | **Closed in V1.34 P1** (commits 655d71c + R-FL-E-01..08 on `feature/v1.34-fl-e-run-intents-and-stages`). Stage columns added + DDL migration + 5 hermetic e2e tests + active schedule uniqueness. |
| DF-55 | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | V1.34: local/read-only or `policy_blocked` (PD-05). |
| DF-56 | Conditional routing / branching engine | V1.33 | **V1.42 P2 Shipped** | L | V1.33→V1.34→V1.42 | **V1.42 P2 shipped**: `llm_judge` GO/NOGO → two `next` edges (commits `5467eaa2` T1, `e81412e6` T2, `c8b1cb5c` T3, `3153a7bd` T4). Plan: [2026-06-11-v1.42-conditional-routing.md](../plans/2026-06-11-v1.42-conditional-routing.md). Spec: [preset-conditional-routing.md](specs/preset-conditional-routing.md). **Post-V1.42 full roadmap**: see §3.6.3. |
| DF-57 | **Closed in V1.36 P2** | — | — | — | See [shipped-features-tracker.md §1 Closed items](../archived/shipped-features-tracker.md) |
| DF-58 | **Closed in V1.36 P1** | — | — | — | See [shipped-features-tracker.md §1 Closed items](../archived/shipped-features-tracker.md) |
| DF-59 | Platform publish integration for novel正文 | V1.36 prepare | **Backlog** | L | V1.36 | Explicit OUT of V1.36 short-term scope; user may publish manually. See compass §1.2 non-goals |
| DF-62 | Multi-chapter / serial writing + **multi-volume PK** | V1.36 distill | **V1.42 P1 Shipped** | M | V1.36→V1.38→V1.39→V1.41→V1.42 | Single-volume multi-chapter **shipped V1.38–V1.39**. **V1.42 P1 shipped**: PK `(work_id, volume, chapter)` migration with `volume=1` backfill, `seed_chapters_multi_volume_tx`, volume-aware auto-chain (`evaluate_after_persist_volume_aware`), status API `next_chapter_volume`, and multi-volume volume-outline.md scaffold. Plan: [2026-06-11-v1.42-multi-volume.md](../plans/2026-06-11-v1.42-multi-volume.md). Commits: `9fefdfbc` (T1+T2), `398d0ba2` (T3), `b63543e1` (T4), `0bbf1581` (T5), `1a6fd97c` (T6). |
| DF-63 | **World KB cross-Work unification** (was: Worldbuilding 7 sub-categories schema) | V1.36 distill → V1.36 spec §3.5 refactor | **V1.40 Closed (Shipped P0+P1+P2+P3 all 5 slices W1–W5)** | L | V1.36→V1.37→V1.40 | **Shipped V1.40 P0–P3** (PR #52, MERGEABLE). V1.37 P2 roadmap shipped as spec; V1.40 implements all 5 slices via [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md) + [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md). **P0 W1+W4** ([world-create-and-validation plan](../plans/2026-06-10-v1.40-world-create-and-validation.md)) — mandatory world binding per user clarification 2026-06-10; `creator world create/show/list`; `create_world_tx` atomic scaffold; ownership FK + 422 enforcement. **P1 W2** ([world-kb-taxonomy plan](../plans/2026-06-10-v1.40-world-kb-taxonomy.md)) — BlockType + novel_category + canonical_name grammar; SqliteKbStore wired with ValidationMode; structured ValidationError. **P2 W3** ([world-context-prompt-block plan](../plans/2026-06-10-v1.40-world-context-prompt-block.md)) — `{{ world_kb_block }}` template var via `build_chapter_kb_block`; legacy V1.39 worldless Works skip. **P3 W5** ([world-kb-extract-binding plan](../plans/2026-06-10-v1.40-world-kb-extract-binding.md)) — `kb_extract_jobs` artifact locator; `finalize_extract` helper; `kb.extract_work` V1.40 schema; `novel-review-master sync_world_kb`. DF-63 row archived 2026-06-11; full delivery snapshot in [shipped-features-tracker.md](../archived/shipped-features-tracker.md). 24 V1.40-tagged open residuals carry-forward to V1.41 (see [`status.json`](../status.json) `residual_findings`). |
| DF-64 | Findings lifecycle (review → brainstorm → write coordination; 3-role) | V1.36 distill | **V1.39 P1+P2 Shipped** | L | V1.36→V1.37→V1.39 | V1.37 P3 roadmap; **V1.39** implemented via [novel-quality-loop.md](specs/novel-quality-loop.md) + [2026-06-09-v1.39-findings-and-review-routing.md](../plans/2026-06-09-v1.39-findings-and-review-routing.md) + [2026-06-09-v1.39-novel-review-presets.md](../plans/2026-06-09-v1.39-novel-review-presets.md). `findings` table + DAO + API + review-verdict hook + routing enum + 7 hermetic API tests; `novel-brainstorm` + `novel-review-master` presets with 8 hermetic tests. PR #50 merged ad9725d8. |
| DF-65 | Three-layer rules architecture | V1.36 distill | **V1.40 P0.5 Shipped** | M | V1.36→V1.37→V1.39→V1.40 | Plan: [2026-06-09-v1.39-rules-and-logs.md](../plans/2026-06-09-v1.39-rules-and-logs.md). V1.39 shipped Layer 1 at `embedded-presets/rules/writing-craft.md` (interim). **V1.40 P0.5** migrated Layer 1 to `embedded-rules/writing-craft.md` per spec §5.5.4; doc path corrected in this tracker and `world-kb-runtime-architecture.md`. |
| DF-66 | Per-chapter log subdirectories at `Works/<work_ref>/Logs/` | V1.36 distill | **V1.39 P3 Shipped** | S | V1.36→V1.37→V1.39 | Same plan as DF-65. Implemented: `Logs/{brainstorm,write,review,publish}/` subdirs scaffolded; novel-writing writes to `Logs/write/`; sync exclusion documented. PR #50 merged ad9725d8. |
| DF-67 | Master-decision timeout (96h finding escalation) | V1.36 distill | **V1.39 P4 Shipped** | S | V1.36→V1.37→V1.39 | Plan: [2026-06-09-v1.39-master-decision-timeout.md](../plans/2026-06-09-v1.39-master-decision-timeout.md). Implemented: 24h-interval daemon task (env-var override); `find_resumable_works` stale-finding DAO; CLI status banner `⏰ N findings stale (>96h)`; per-Work `auto_review_master_on_timeout` opt-in (default false); RVM-prefixed review-master schedule helper; 7 hermetic tests. PR #50 merged ad9725d8. |

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
| BL-10 | Novel writing author quickstart (`docs/novel-writing-quickstart.md`) | V1.42 prepare | Backlog | M | V1.42 compass §1.2 OUT — user-facing doc deferred until product path stable. |

### 3.5 Open tech-debt residuals (SSOT pointer)

**V1.42 P0 defer carry-forward** (from V1.41 P-last hygiene): R-V140P0-S3, R-V140P1-S6, R-V140P2-S2, R-V140P3-S1/S2/S3, R-V140P4-INFRA — see [2026-06-11-v1.42-runtime-lock-and-hygiene.md](../plans/2026-06-11-v1.42-runtime-lock-and-hygiene.md) §2.


**Machine state**: [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary` (`status.json.updated_at` **2026-06-11**; `tech_debt_summary.updated_at` **2026-06-11**; V1.40 closeout, Profile B compaction applied). Do **not** mirror full rows here — JSON wins on conflict. This section is a human pointer only; use `status.json` for current totals, severity buckets, and target buckets.

| Bucket | Open count | `residual_findings` key |
|--------|------------|-------------------------|
| V1.30 post-QC | 9 | `v1.30-post-qc-tech-debt` (V1.35 P0 closed 2: TD-V130-02, TD-V130-06) |
| V1.31 post-QC | 5 | `v1.31-post-qc-tech-debt` (V1.35 P0 closed 3: TD-V131-01, TD-V131-03, TD-V131-04) |
| V1.33 work model (P1) | 3 | `2026-06-04-v1.33-work-model-and-creator-run` (R-V133P1-03/08/09; partial / defer; not P0) |
| V1.33 llm_judge (P3) | 2 | `2026-06-04-v1.33-llm-judge-runtime-fix` (V1.35 P0 closed 2 critical: R-V133P3-01, R-V133P3-02; remaining 2 medium: R-V133P3-03/04) |
| V1.33 memory review (P4) | 3 | `2026-06-04-v1.33-memory-review-closed-loop` (V1.35 P0 closed 4 critical: R-V133P4-01, R-V133P4-02, R-V133P4-03, R-V133P4-07; remaining 3 medium: R-V133P4-04, R-V133P4-05, R-V133P4-06) |
| V1.34 FL-E stages (P1) | 5 | `2026-06-04-v1.34-fl-e-run-intents-and-stages` (R-FL-E-DDL/DEAD/LIST/FNAME/ENDP, all low; not P0) |
| V1.34 agent tool (P4) | 1 | `2026-06-04-v1.34-agent-tool-implementation` (DF-47 production caller wiring → conditional V1.36+) |
| V1.36 novel init (P1) | 2 | `2026-06-07-v1.36-novel-project-init-preset` (`R-V136P1-01`, `R-V136P1-02`; V1.37 P0 targets both) |
| V1.36 novel layout (P2) | 3 | `2026-06-07-v1.36-novel-artifact-layout-and-templates` (low; path helper / fixture / P3 handoff items) |
| V1.36 novel pipeline (P3) | 2 | `2026-06-07-v1.36-novel-chapter-drafting-pipeline` (`R-V136P3-01`, `R-V136P3-02`; V1.37 P0 targets runtime gates) |
| **Total** | See JSON | See `metadata.tech_debt_summary.total_open`; do not recompute in Markdown |

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
| **Layout root** | `{作品目录}/` (per-work, 7 subdirs) | In-scope: `Works/<work_ref>/` + 4 subdirs (Stories, Outlines, Logs, README.md); **per-Work Worldbuilding/ REMOVED** (content lives in World KB) | DF-57 / DF-63 |
| **Chapter file naming** | `第{N}章.md` (Chinese) | In-scope: `ch<nn>-<slug>.md` (English; international OSS) | (impl detail) |
| **Chapter frontmatter** | `title/chapter/volume/status/word_count/tags/created/updated` | In-scope: `title/chapter/volume (optional)/status/word_count/world_refs (optional)`; P3 T9 forward-compat | (impl detail) |
| **Chapter state machine** | ⬜→✏️→📝→✅→🚀 (with `published`) | In-scope: `not_started`/`outlined`/`draft`/`finalized`/`published` (`published` reserved) | (impl detail) |
| **Chapter state SSOT** | `作品状态.md` chapter table (file) | **V1.36 refactor**: `work_chapters` table in `state.db` (DB SSOT) + frontmatter mirror; `work-status.md` file **removed**; reconciliation via `creator run reconcile-chapters` | (impl detail) |
| **Outlines/ tree** | 分卷总纲/ + 单章细纲/ + 事件索引 + 逻辑异常 + 伏笔索引 | In-scope: chapter outline (required) + volume outline (optional) + foreshadowing.md (empty stub) + event-index.md (empty stub) | (impl detail) |
| **Worldbuilding (cross-Work)** | Per-work `世界设定/` (7 sub-types with item templates: foundation/background/character/location/society/rules/economy) | **V1.36 refactor**: World KB (per [entity-scope-model.md](specs/entity-scope-model.md) §5.4) is the cross-Work truth; `world_id` is the binding; `novel-project-init` grill-me; `creator run start --world-id` CLI; `world_refs: [string]` advisory frontmatter. Full KB item schema + `kb-extract` extraction path is V1.37+ | DF-63 |
| **Logs/** | 4 sub-types (写/迭代/构思/发布) with status machines | In-scope: `Logs/` optional root only; structure OUT (single-role) | DF-66 |
| **Completion detection** | `currentChapter==totalPlanned` + all chapters `published` | In-scope: `current_chapter>=total` + all `finalized` + `intake==complete` (no publish) | (impl detail) |
| **完本后同步** | 5-step ceremony (frontmatter/table/Redis×2/selection pool) | V1.36: 2-step reduced; **V1.41 P0**: completion-lock + pool row update | DF-60 |
| **Auto new-book switch** | 8-step + 2h switch lock + 中断恢复 | OUT globally; V1.41: `works use` / promote default only (no mutex) | DF-60 |
| **Quality loop** | review cron + 五问质量检验 + findings lifecycle + 96h 升级 | In-scope: `llm_judge` exit_when on `finalize` (V1.36 quality gate); full review cron + findings OUT | DF-64 / DF-67 |
| **两轮写作** | 初稿→终稿 (各带日志) | In-scope: outline→draft→finalize; 两轮合一 (no separate terminal/refine) | (impl detail) |
| **State storage** | Redis (novel:active / novel:{名}:state / novel:review-iteration) | In-scope: local SQLite (state.db); Redis OUT (OSS local-only) | (PD-05) |
| **Platform publish** | InStreet literary API + workId UUID + chapter post API | OUT (V1.36 compass §1.2) | DF-59 |
| **Selection pool / 灵感池** | Obsidian 选题库 + 灵感池 | **V1.41 P1 Shipped** — DB SSOT + `{workspace}/Pool/Ideas/` | DF-61 |
| **Three-layer rules** | writing-craft-rules.md / novel_rules.md / novel_rules_history.md | OUT (V1.36 ships 五问 inline in finalize prompt; per-work rules file deferred) | DF-65 |
| **Multi-volume auto-chronology** | per-volume outline + chapter range tracking | OUT (V1.36 single-chapter; `volume: integer` frontmatter is forward-compat) | DF-62 |
| **Three-cron staggering** | brainstorm 03/09/15/21 / write 04/10/16/22 / review :00/:30 | OUT (V1.36 single-role; multi-role staggering is V1.37+) | (with DF-64) |
| **Switch lock + 2h timeout** | file-based lock at `{小说目录}/.switch-lock.json` | **V1.41 P0**: `.completion-lock.json` (no 2h cron; no global switch) | DF-60 |
| **Master-decision timeout (96h)** | finding escalation; surfaced via activity-report cron | OUT (V1.36 no review cycle) | DF-67 |

##### V1.36 implementation of in-scope items (PM approved 2026-06-07)

| novels-system feature | V1.36 implementation | Spec/plan ref |
|---|---|---|
| Chapter finalize quality gate (五问) | `exit_when: kind: llm_judge` on `finalize` state in `novel-writing` preset; template `finalize-exit.md` | novel-workflow-profile §5.1; plan P3 T7 |
| 6-column chapter table | **Migrated to `work_chapters` DB table** (chapter table removed from work-status.md file) | novel-workflow-profile §4.1.1/§4.1.2/§4.1.3; plan P2 T10/T12/T13 |
| `volume: integer` frontmatter | Forward-compat field; V1.36 leaves blank | novel-workflow-profile §4.3; plan P2 T9 |
| `world_refs: [string]` frontmatter | Advisory; for World-bound Works (§3.5) | novel-workflow-profile §4.3 |
| `Outlines/foreshadowing.md` | Empty stub with F### table header | novel-workflow-profile §3.1/§3.2; plan P2 T7 |
| `Outlines/event-index.md` | Empty stub with E### table header | novel-workflow-profile §3.1/§3.2; plan P2 T8 |
| Foreshadowing required in outline | §4.2 promotes foreshadowing from optional to required | novel-workflow-profile §4.2; plan P3 T9 |
| **World KB cross-Work binding** | `world_id` FK on `works`; `novel-project-init` grill-me (existing/new/worldless); `--world-id` CLI; `creator run status` shows `world: <name> (<world_id>)`; `novel-writing` injects World KB context block for World-bound Works | novel-workflow-profile §3.5/§5.2/§8; plan P1 T1 (grill-me)/P2 T11 |
| **`work-status.md` file** | **REMOVED**; chapter state lives in `work_chapters` table; reconciliation via `creator run reconcile-chapters` | novel-workflow-profile §3.1/§4.1; plan P2 T10/T12/T13 |
| **Per-Work `Worldbuilding/` subtree** | **REMOVED**; world content lives in World KB (per [entity-scope-model.md](specs/entity-scope-model.md) §5.4); worldless Works put setting notes in `README.md` | novel-workflow-profile §3.5/§3.1; plan P1/P2 |

##### V1.41 distill overlay (grill-me 2026-06-10 — supersedes OUT rows above for DF-60/61)

| novels-system pattern | V1.41 OSS mapping | Plan |
| --- | --- | --- |
| Redis `novel:active` | `novel_pool_entries.status = active` — **CLI default only**; concurrent multi-Work OK | P1 pool + P0 `works use` |
| 8-step switch / 2h lock | **OUT** — `.completion-lock.json` per completed Work; no global mutex | P0 lifecycle |
| `选题库` / `灵感池` | DB SSOT; `{workspace}/Pool/Ideas/*.md` for inspiration files | P1 |
| CLI | `creator works` (list/status/use/pool); `creator run` single-Work actions; `--from-work` | P0 + P1 |
| Same-Work concurrent mutate | `works.runtime_lock_holder` (DB SSOT) | P0 |

Normative: [v1.41-multi-work-author-desk-delivery-compass-v1.md](../iterations/v1.41-multi-work-author-desk-delivery-compass-v1.md) §0.1.

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
   - DF-63 → extend `entity-scope-model.md` World KB taxonomy and `novel-workflow-profile.md` World integration sections (do **not** reintroduce a per-Work Worldbuilding spec or subtree)
   - DF-64/67 → `novel-workflow-profile.md` §5.5 quality-loop roadmap extension first; a future large implementation may split into `novel-quality-loop.md` if the section outgrows the profile
   - DF-65 → `novel-workflow-profile.md` §5.5.4 rules architecture first; a future large implementation may split into `novel-rules-architecture.md`
   - DF-66 → `novel-workflow-profile.md` §5.5.5 `Logs/` section extension
4. **Register the new spec + plan in `status.json`** per mstar-plan-artifacts lifecycle.
5. **Update the deferred tracker** to record the new spec/plan closure (per §4 change control).

#### 3.6.3 DF-56 post-V1.42 P2 roadmap (grill-me 2026-06-11)

**Shipped in V1.42 P2 (minimal slice)** — commits on `feature/v1.42-conditional-routing`:

| Task | Commit | Description |
|------|--------|-------------|
| T1 | `5467eaa2` | Spec promoted from Exploration to Draft V1.42 |
| T2 | `e81412e6` | `GoNogoNext` struct + `NextTarget::GoNogo` variant; loader validation; `add_conditional_edge` wiring; reachability via both branches |
| T3 | `c8b1cb5c` | `StateCompositeTask::judge_next_action` — GoNogo returns `Continue` for both GO and NOGO; Linear/None preserves existing behavior |
| T4 | `3153a7bd` | 12 hermetic tests (6 loader + 6 executor); reachability validator traverses GoNogo edges |

Runtime behavior: `_judge_result` in `graph_flow::Context` drives the conditional edge. `true` → `go` target; `false` or absent → `nogo` target (safe fallback).

**Deferred Post-V1.42** (remain open under DF-56 until a future compass reopens):

- Arbitrary stage-level conditional `next` (non-judge nodes)
- Expression / rule-based routing beyond GO/NOGO
- `registry.refresh` conditional edges (depends on DF-29)
- `workspace.open` / `workspace.commit` branch inputs (depends on DF-31)
- Multi-branch graphs with >2 outgoing edges and merge points

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

- **V1.42** (Active — prepare 2026-06-11): [v1.42-multi-volume-serial-writing-delivery-compass-v1.md](../iterations/v1.42-multi-volume-serial-writing-delivery-compass-v1.md) — P0 runtime_lock + P1 DF-62 + P2 DF-56 + P3 DF-47 + P-last UX on `iteration/v1.42`.
- **V1.41** (Shipped 2026-06-11): [v1.41-multi-work-author-desk-delivery-compass-v1.md](../iterations/v1.41-multi-work-author-desk-delivery-compass-v1.md) — PR #53; DF-60/61 archived.
- **V1.40** (Shipped 2026-06-11 via PR #52 merged): [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md) — DF-63 closed; `iteration/v1.40` retired.
- **V1.39** (Shipped 2026-06-09): [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) — **DF-53 full auto-chain + DF-68 daemon continuation + DF-64/65/66/67 quality loop**; P0..P5 on `iteration/v1.39`; PR #50 merged ad9725d8.
- **V1.38** (Shipped 2026-06-09): [v1.38-multi-chapter-serial-writing-delivery-compass-v1.md](../iterations/v1.38-multi-chapter-serial-writing-delivery-compass-v1.md) — DF-62 first slice shipped (PR #49).
- **V1.37** (Shipped 2026-06-08): [v1.37-novel-writing-foundation-delivery-compass-v1.md](../iterations/v1.37-novel-writing-foundation-delivery-compass-v1.md) — **Novel Writing UX foundation-first**: P0 shipped init `preset.input` plumbing, runtime `gates:` evaluation, scaffold atomicity, and first-run remediation; P1/P2/P3 roadmap multi-chapter DF-62, World KB DF-63, and quality-loop DF-64/65/66/67.
- **V1.36** (Shipped 2026-06-07): [v1.36-novel-writing-ux-delivery-compass-v1.md](../iterations/v1.36-novel-writing-ux-delivery-compass-v1.md) — **novel-writing正文产出 UX** (5 implement plans P0–P4 + prepare P-1 all Done; PM-validate path used for P1–P4 under time pressure; DF-57/58 closed; DF-53 partial again on top of V1.35 P4; DF-47 stays conditional not P0; DF-59 backlog); single-chapter MVP (outline_chapter → draft_chapter → finalize with llm_judge 五问) + completion stop + `Works/<work_ref>/` layout + `work_chapters` DB SSOT + `creator run reconcile-chapters` + `--force-gates` gate override
- **V1.35** (Shipped 2026-06-07): [v1.35-cli-ia-and-product-polish-delivery-compass-v1.md](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md) — CLI IA (5 groups; sync→platform), creator hub polish, critical residual P0 (6 criticals + R-CURSOR-PR42-03 + 5 backlog), FL-E UX polish (chain default true); 5 implement plans P0/P2/P3/P4/P5 + prepare P-1 + P1 docs all Done; DF-47 later reclassified as conditional, not V1.36 P0
- **V1.34** (Shipped 2026-06-05): [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) — FL-E + Agent tools; DF-47 carried forward to V1.35 and later reclassified as conditional

**Recent shipped compasses** (detail in archive §2)

- V1.34: [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) — FL-E + Agent tools (8 `nexus.*` tool bridge); **Shipped 2026-06-05** (5 plans P0–P5 all Done); DF-47 later reclassified as conditional rather than novel UX P0
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

*Last updated: 2026-06-11 (V1.42 Active prepare). Status: **V1.42 Active** on `iteration/v1.42`; V1.41 Shipped (PR #53). DF-62/56/47 Active; DF-60/61 archived.*
