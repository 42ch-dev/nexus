# Deferred Features — Cross-Version Tracker v1

**Quick status**: **V1.57 Shipped (2026-06-22)** — DF-46 Full Parity & DF-47 Unification (T-A single axis); 7 plans all Done (P-1 + P0 + P1 + P2 + P3 + P-mid + P-last); integration branch `iteration/v1.57` ready for PR to `main`; Profile B compaction complete; 3 V1.57 carry-forwards closed (R-V156P1-M001/M002/P3-S003); 3 new V1.57+ residuals filed (R-V157P0-L001/L002 + R-V157P1-W001); DF-46 reduced (not closed). V1.56 Shipped 2026-06-22 via PR #78 @ 8a2fb20e; V1.55 Shipped 2026-06-22 (PR #77); V1.54 Shipped 2026-06-21 (PR #76). Platform **paused**. Tech debt SSOT: [`status.json`](../status.json)

**Status**: V1.57 Shipped (closeout 2026-06-22) — DF-46 Full Parity & DF-47 Unification. **T-A single axis** (Mode 1 spec governance: `agent-nexus-tool-bridge.md` → Master + acp §4 roster rewrite; Option A adapter topology: 1 `capability::Registry` + 3 caller adapter; CLI `nexus42 host-call` debug-only; 4 implement plans + P-mid + P-last in 3-wave dispatch). DF-46 reduced: 41 roster rows (18 `shipped` host tools + 18 `catalog-only` orchestration + 3 `scaffold-equivalent` profile sets + 2 `OUT` publish.*); not "Closed" — 2 publish.* IDs still in catalog as OUT. V1.56 Shipped 2026-06-22 via PR #78 at commit 8a2fb20e on main; integration branch `iteration/v1.56` retired per `mstar-branch-worktree`. V1.55 Shipped 2026-06-22 via PR #77 at commit 9d2893c2.

**V1.57 carry-forward index (closed in V1.57)**: `R-V156P1-M001` → V1.57 P2 (`2026-06-22-v1.57-v156-carry-forwards-and-compliance`) — schema rename `agent_count` → `capability_count` with backward-compat serde aliases — **Closed** (commit 28005f11); `R-V156P1-M002` → V1.57 P1 (`2026-06-22-v1.57-daemon-refactor-and-caller-adapters`) — global `RwLock<CdnConfig>` → constructor-injected (god-file refactor cross-cut) — **Closed** (commit 64c0b245); `R-V156P3-S003` → V1.57 P0+P1 — registry_output_to_context field drops (P0 re-introduced 4 fields; P1 caller audit) — **Closed** (commits 7e9932ce + fc0e250a). 3 new V1.57+ residuals filed post-Wave 1: `R-V157P0-L001` (low; AC wording), `R-V157P0-L002` (medium; per-ID test vectors), `R-V157P1-W001` (medium; host-call smoke `#[ignore]`).

**DF-46 reduction note (V1.57)**: DF-46 "Full `nexus.*` logical capability implementation" was the longest-standing open feature (V1.34). V1.57 reduces it to "Roster-documented: 41 rows = 18 `shipped` + 18 `catalog-only` + 3 `scaffold-equivalent` + 2 `OUT` (publish.* per DF-59)". 2 publish.* IDs remain in catalog as `OUT` (DF-59 platform publish — platform/V2.0+ dependency). Roster is the SSOT for capability coverage; future `nexus.*` additions must register in both `capability::Registry` and the acp §4 roster (cross-validation enforced by `catalog_registry_invariant_all_ids_present` test).

**V1.55 carry-forward index (closed in V1.55)**: DF-43 → V1.55 P0 (`2026-06-22-v1.55-df43-sqlite-alignment`) — **Closed**; DF-31 → V1.55 P1 (`2026-06-22-v1.55-df31-workspace-interface`) — **Skeleton shipped**; game-bible Depth 3.5 → V1.55 P2 (`2026-06-22-v1.55-game-bible-depth-35`) — **Shipped**; Script scaffold → V1.55 P3 (`2026-06-22-v1.55-script-scaffold`) — **Shipped**; `R-V154P1-S002` → V1.55 P2 — **Resolved**; `R-V154P1-W001` → V1.55 P3 — **Resolved**.
**Purpose**: Single source of truth for **open** and **backlog** features/tech-debt deferred from delivery compasses. Closed/shipped history lives in [shipped-features-tracker.md](../archived/shipped-features-tracker.md).
**Scope**: `nexus` OSS repository only. Platform features referenced only when they block nexus-side work.
**Predecessor**: Consolidated from delivery compasses (v1.2–v1.21) and the v1.2 reclassification matrix.
**Created**: 2026-04-21
**Last updated**: 2026-06-22 (V1.57 Active: compass + 7 plan stubs authored, integration branch `iteration/v1.57` created from main, 3 V1.56 medium/low residuals absorbed into V1.57 plan slots per V1.57 carry-forward index; V1.56 ship baseline)

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
| DF-40 | Session resume stub in daemon lifecycle | V1.21 audit | **Converged via DF-68 (V1.39 P0 Shipped)** | S | V1.21→V1.39 | `daemon-runtime/lifecycle/actions.rs`. V1.39 P0 conditional boot auto-resume for checkpointed auto-chain drivers supersedes blanket pause-only recovery for those schedules. DF-68 implemented: `find_resumable_works` + boot logging in `boot.rs`. |
| DF-41 | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-43 | SQLite persistence / crate-model alignment | V1.24 audit | **Closed V1.55 P0** | M | V1.26–28 partial→V1.55 P0 | **Closed**: adapter `From<ReferenceSourceRow> for nexus_knowledge::ReferenceSource` added in `nexus-local-db`; `nexus-knowledge` crate docs locked to model/adapter-seam only; spec `local-db-schema.md` §4.1.1 ownership boundary text added; round-trip + duplicate-truth prevention tests in `nexus-local-db/src/reference_source.rs`. Plan: [2026-06-22-v1.55-df43-sqlite-alignment.md](../plans/2026-06-22-v1.55-df43-sqlite-alignment.md). |
| DF-44 | Reference body externalization — refreshable scan pipeline | V1.26 | Any future | M | V1.26 | Static registration shipped; auto-refresh Open. |
| DF-46 | Full `nexus.*` logical capability implementation (acp-capability-set parity) | V1.34 audit | **Reduced — V1.57** | L | V1.34→V1.53→V1.57 | V1.34 ships minimal host tools only. V1.53 P0/P1 extended (8→13 host tools). **V1.57 reduces**: 41-row roster in `acp-capability-set.md` §4 (18 `shipped` host tools + 18 `catalog-only` orchestration + 3 `scaffold-equivalent` profile sets + 2 `OUT` publish.*). 2 publish.* IDs remain in catalog as `OUT` (DF-59 platform publish — platform/V2.0+ dependency). Future additions must register in both `capability::Registry` and the acp §4 roster (cross-validation enforced). Roster is the SSOT for capability coverage. See [v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md](../iterations/v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md). |
| DF-47 | Host tool + `worker/agent_tool_request` unified registry | V1.34 audit | **V1.42 P3 Narrowed** | M | V1.34→V1.35→V1.36→V1.42 | V1.34 P4 shipped adapter. **V1.42 P3 shipped**: `DaemonToolDispatchAdapter` + `HostToolCallTask` + one tool (`nexus.orchestration.schedule_status`) proven E2E with 5 hermetic tests. Production caller wiring complete for minimal slice. Full DF-46 parity remains Post-V1.42. Plan: [2026-06-11-v1.42-agent-tool-production-wiring.md](../plans/2026-06-11-v1.42-agent-tool-production-wiring.md). |
| DF-48 | Agent tool bridge via `nexus42` CLI subprocess | V1.34 | Post-V1.34 | M | V1.34 | Rejected; daemon HostToolExecutor is SSOT. |
| DF-49 | Standalone MCP server for Nexus capabilities | V1.34 | Backlog | L | V1.34 | Separate from ACP agent path. |
| DF-51 | `creator.inject_prompt` wire/schema alignment | V1.33 compass §6 | V1.34+ | S | V1.33→V1.34 | **Closed in V1.34 residual-convergence** (commits a044f94 + 71c10cc). Schema now declares `prompt_file` + `vars` with `anyOf`. Closure recorded in [`.mstar/archived/residuals/v1.32-post-qc-tech-debt.json`](../archived/residuals/v1.32-post-qc-tech-debt.json) (R-P2-01). |
| DF-52 | Top-level `nexus42 preset` command group | V1.33 | **V1.45 Shipped** (P-last) | S | V1.33 | **Resolution path:** `creator run <preset_id>` generic entry (BL-12). Archived to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1 (V1.45 snapshot). |
| DF-53 | FL-E `--auto-chain` default stage sequencing | V1.34 | **V1.39 P0 Shipped** | S | V1.34→V1.35→V1.36→V1.37→V1.38→V1.39 | V1.35 P4 partial **shipped**: `--chain-novel-writing` defaults true (intake → produce). V1.38 shipped multi-chapter foundation **without** auto-reenqueue. **V1.39 P0** implements full `intake → research → produce → review → persist` auto-chain (default true), chapter outer loop, side-input lane, boot recovery, `--no-auto-chain` opt-out, `creator run resume` command. Core: `nexus-orchestration::auto_chain` module with `evaluate_next_step` + 15 unit tests + 14 integration tests. Plan: [2026-06-09-v1.39-fl-e-auto-chain-engine.md](../plans/2026-06-09-v1.39-fl-e-auto-chain-engine.md). **Tri-review + targeted re-review all Approve; final consolidated gate Approve. PR #50 merged ad9725d8.** |
| DF-54 | Work `stage` / `stage_status` persistence gap | V1.34 | V1.34+ | S | V1.34 | **Closed in V1.34 P1** (commits 655d71c + R-FL-E-01..08 on `feature/v1.34-fl-e-run-intents-and-stages`). Stage columns added + DDL migration + 5 hermetic e2e tests + active schedule uniqueness. |
| DF-55 | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | V1.34: local/read-only or `policy_blocked` (PD-05). |
| DF-57 | **Closed in V1.36 P2** | — | — | — | See [shipped-features-tracker.md §1 Closed items](../archived/shipped-features-tracker.md) |
| DF-58 | **Closed in V1.36 P1** | — | — | — | See [shipped-features-tracker.md §1 Closed items](../archived/shipped-features-tracker.md) |
| DF-59 | Platform publish integration for novel正文 | V1.36 prepare | **Backlog** | L | V1.36 | Explicit OUT of V1.36 short-term scope; user may publish manually. See compass §1.2 non-goals |
| DF-62 | Multi-chapter / serial writing + **multi-volume PK** | V1.36 distill | **V1.42 P1 Shipped** | M | V1.36→V1.38→V1.39→V1.41→V1.42 | Single-volume multi-chapter **shipped V1.38–V1.39**. **V1.42 P1 shipped**: PK `(work_id, volume, chapter)` migration with `volume=1` backfill, `seed_chapters_multi_volume_tx`, volume-aware auto-chain (`evaluate_after_persist_volume_aware`), status API `next_chapter_volume`, and multi-volume volume-outline.md scaffold. Plan: [2026-06-11-v1.42-multi-volume.md](../plans/2026-06-11-v1.42-multi-volume.md). Commits: `9fefdfbc` (T1+T2), `398d0ba2` (T3), `b63543e1` (T4), `0bbf1581` (T5), `1a6fd97c` (T6). |
| DF-63 | **World KB cross-Work unification** (was: Worldbuilding 7 sub-categories schema) | V1.36 distill → V1.36 spec §3.5 refactor | **V1.40 Closed (Shipped P0+P1+P2+P3 all 5 slices W1–W5)** | L | V1.36→V1.37→V1.40 | **Shipped V1.40 P0–P3** (PR #52, MERGEABLE). V1.37 P2 roadmap shipped as spec; V1.40 implements all 5 slices via [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md) + [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md). **P0 W1+W4** ([world-create-and-validation plan](../plans/2026-06-10-v1.40-world-create-and-validation.md)) — mandatory world binding per user clarification 2026-06-10; `creator world create/show/list`; `create_world_tx` atomic scaffold; ownership FK + 422 enforcement. **P1 W2** ([world-kb-taxonomy plan](../plans/2026-06-10-v1.40-world-kb-taxonomy.md)) — BlockType + novel_category + canonical_name grammar; SqliteKbStore wired with ValidationMode; structured ValidationError. **P2 W3** ([world-context-prompt-block plan](../plans/2026-06-10-v1.40-world-context-prompt-block.md)) — `{{ world_kb_block }}` template var via `build_chapter_kb_block`; legacy V1.39 worldless Works skip. **P3 W5** ([world-kb-extract-binding plan](../plans/2026-06-10-v1.40-world-kb-extract-binding.md)) — `kb_extract_jobs` artifact locator; `finalize_extract` helper; `kb.extract_work` V1.40 schema; `novel-review-master sync_world_kb`. DF-63 row archived 2026-06-11; full delivery snapshot in [shipped-features-tracker.md](../archived/shipped-features-tracker.md). 24 V1.40-tagged open residuals carry-forward to V1.41 (see [`status.json`](../status.json) `residual_findings`). |
| DF-64 | Findings lifecycle (review → brainstorm → write coordination; 3-role) | V1.36 distill | **V1.39 P1+P2 Shipped** | L | V1.36→V1.37→V1.39 | V1.37 P3 roadmap; **V1.39** implemented via [novel-writing/quality-loop.md](specs/novel-writing/quality-loop.md) + [2026-06-09-v1.39-findings-and-review-routing.md](../plans/2026-06-09-v1.39-findings-and-review-routing.md) + [2026-06-09-v1.39-novel-review-presets.md](../plans/2026-06-09-v1.39-novel-review-presets.md). `findings` table + DAO + API + review-verdict hook + routing enum + 7 hermetic API tests; `novel-brainstorm` + `novel-review-master` presets with 8 hermetic tests. PR #50 merged ad9725d8. |
| DF-65 | Three-layer rules architecture | V1.36 distill | **V1.40 P0.5 Shipped** | M | V1.36→V1.37→V1.39→V1.40 | Plan: [2026-06-09-v1.39-rules-and-logs.md](../plans/2026-06-09-v1.39-rules-and-logs.md). V1.39 shipped Layer 1 at `embedded-presets/rules/writing-craft.md` (interim). **V1.40 P0.5** migrated Layer 1 to `embedded-rules/writing-craft.md` per spec §5.5.4; doc path corrected in this tracker and `world-kb-runtime-architecture.md`. |
| DF-66 | Per-chapter log subdirectories at `Works/<work_ref>/Logs/` | V1.36 distill | **V1.39 P3 Shipped** | S | V1.36→V1.37→V1.39 | Same plan as DF-65. Implemented: `Logs/{brainstorm,write,review,publish}/` subdirs scaffolded; novel-writing writes to `Logs/write/`; sync exclusion documented. PR #50 merged ad9725d8. |
| DF-67 | Master-decision timeout (96h finding escalation) | V1.36 distill | **V1.39 P4 Shipped** | S | V1.36→V1.37→V1.39 | Plan: [2026-06-09-v1.39-master-decision-timeout.md](../plans/2026-06-09-v1.39-master-decision-timeout.md). Implemented: 24h-interval daemon task (env-var override); `find_resumable_works` stale-finding DAO; CLI status banner `⏰ N findings stale (>96h)`; per-Work `auto_review_master_on_timeout` opt-in (default false); RVM-prefixed review-master schedule helper; 7 hermetic tests. PR #50 merged ad9725d8. |
| ~~BL-10~~ | Novel writing author quickstart (`docs/novel-writing-quickstart.md`) | V1.41 prepare | **V1.43 Shipped** | M | V1.41→V1.43 | **Shipped V1.43** on `iteration/v1.43` (merge `340423e5`, 2026-06-12). Plan: [2026-06-12-v1.43-novel-writing-quickstart.md](../plans/2026-06-12-v1.43-novel-writing-quickstart.md). Spec: [novel-writing/author-experience.md](specs/novel-writing/author-experience.md). P-last residuals: R-V143P0-001 closed (spec amendment); R-V143P0-002 deferred to V1.44+ (review-master surface). Archived to [shipped-features-tracker.md](../archived/shipped-features-tracker.md). |
| ~~BL-12~~ | `creator run` hardcoded subcommands vs preset-generic entry | V1.44 ship | **V1.45 Shipped** (P0+P1+P2) | L | V1.45 | V1.44 shipped `audit-chapter` / `review-master` as new enum variants — anti-pattern. V1.45: generic `creator run <preset_id>` + delete bespoke subcommands. Plan: [2026-06-13-v1.45-creator-run-generic-runner.md](../plans/2026-06-13-v1.45-creator-run-generic-runner.md). Spec: [creator-run-preset-entry.md](specs/creator-run-preset-entry.md) (Shipped V1.45). Archived to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1 (V1.45 snapshot). |
| ~~BL-13~~ | `STAGE_PRESET_ALLOWLIST` references `memory-review` without embedded preset | V1.34 | **V1.45 Shipped** (P1 T4) | S | V1.34 | Allowlist drift in `validation.rs`; no `embedded-presets/memory-review/`. P1: removed allowlist entry (implement decision). Archived to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1 (V1.45 snapshot). |
| PF-ESSAY | `essay` Work profile | V1.52 lock | V1.52 | M | V1.52 lock + spec authoring | First non-novel Feature line: [essay-profile.md](specs/essay-profile.md). |
| PF-GAME-BIBLE | `game-bible` Work profile | V1.52 lock | **V1.55 P2 (Depth 3.5 shipped; Master spec)** | L | V1.52 Exploration → V1.54 Scaffold → V1.55 Depth 3.5 | V1.54 shipped scaffold + Draft spec; V1.55 P2 shipped `design-writing` + design 五问 + section completion detection + KB extraction (profile-aware via `candidate_from_llm_json_for_profile`). R-V154P1-S002 observability closed. Spec promoted to Master at V1.55 P-last. Spec: [game-bible-profile.md](specs/game-bible-profile.md). |
| PF-SCRIPT | `script` Work profile | V1.52 lock | **V1.55 P3 (scaffold shipped; Draft spec)** | L | V1.52 Exploration → V1.55 scaffold | V1.55 P3 shipped `script-profile.md` Draft + V1.54-style scaffold (Scripts/ + Beats/ + Characters/ + Logs/); additive `dialogue`/`beat`/`act` BlockType + `script_category` validation. R-V154P1-W001 ScaffoldTransaction applied to BOTH game-bible and script scaffolds. Spec: [script-profile.md](specs/script-profile.md). |

#### DF-43 decision note — Reference sources persistence

**Status:** **Closed in V1.55 P0** (2026-06-22).

1. **`nexus-local-db`** owns production `reference_sources` in `state.db` — confirmed sole persistence owner.
2. **`nexus-knowledge::ReferenceSource`** remains as domain model / adapter seam. `From<ReferenceSourceRow>` adapter lives in `nexus-local-db`.
3. `nexus-knowledge` crate docs and AGENTS.md explicitly state no second SQLite/file-backed truth source.

See plan: [2026-06-22-v1.55-df43-sqlite-alignment.md](../plans/2026-06-22-v1.55-df43-sqlite-alignment.md).

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

**V1.55 ship residual retargeting**: All 2 V1.54 carry-forwards closed in V1.55. `R-V154P1-S002` resolved in P2 (profile-gate observability); `R-V154P1-W001` resolved in P3 (ScaffoldTransaction). `R-V155P2-F002` (V1.55-internal; design-writing preset no durable section_status auto-transition) **absorbed into V1.56 P-last fix-wave** per compass Q7 → **Closed in V1.56 P-last** (no carry-forward to V1.57).

**V1.56 ship residual retargeting (V1.57 carry-forwards)**: 3 V1.56 medium/low residuals absorbed into V1.57 plan slots per V1.57 carry-forward index above. Lifecycle stays `deferred` until the absorbing plan ships and resolves the row. 32 V1.57+ residuals remain in backlog (out of V1.57 scope: R-V156P0-M001 sha2 dep + R-V156P0-M002 path canonicalize stay V1.58+ workspace concerns per compass §1.2).

**Machine state**: [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary` (`status.json.updated_at` **2026-06-22T22:30:00Z**; iteration state: V1.57 Active; `integration_branch=iteration/v1.57` (created from main @ 329c5ff2); `integration_branch_retired=false`; `pre_implement_gate=pending`; `latest_active_iteration=V1.57`; `latest_ship.iteration=V1.56` (`merge_commit=8a2fb20e`, `pr=78`); 3 V1.57 carry-forwards registered (R-V156P1-M001→P2, R-V156P1-M002→P1, R-V156P3-S003→P1); 32 V1.57+ backlog residuals). Do **not** mirror full rows here — JSON wins on conflict. Closed/historical rows: `.mstar/archived/residuals/<plan-id>.json`.

| Bucket | Open count | `residual_findings` key |
|--------|------------|-------------------------|
| V1.55 carry-forward (V1.54 ship) | **0** | 2 low; both resolved in V1.55 P2+P3 (see V1.55 carry-forward index above) |
| V1.55 internal (V1.55 ship) | **0** | 1 low: `R-V155P2-F002` → closed in V1.56 P-last fix-wave (no V1.57 carry-forward) |
| V1.56 carry-forward (V1.56 ship) | **3** | 1 medium + 1 medium + 1 low; absorbed into V1.57 plan slots per V1.57 carry-forward index above |
| V1.56 internal (V1.56 ship, V1.57+ backlog) | **32** | 17 medium + 15 low; in V1.57+ backlog (out of V1.57 scope per compass §1.2) |
| **Total deferred at V1.57 active** | **35** | See `metadata.tech_debt_summary` (3 V1.57 carry-forwards + 32 V1.57+ backlog) |

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
   - DF-60/61 → new spec `novel-writing/multi-work-lifecycle.md` (or extension to novel-workflow-profile)
   - DF-62 → new spec section in novel-workflow-profile §3.1 + chapter frontmatter becomes required
   - DF-63 → extend `entity-scope-model.md` World KB taxonomy and `novel-writing/workflow-profile.md` World integration sections (do **not** reintroduce a per-Work Worldbuilding spec or subtree)
   - DF-64/67 → `novel-writing/workflow-profile.md` §5.5 quality-loop roadmap extension first; a future large implementation may split into `novel-writing/quality-loop.md` if the section outgrows the profile
   - DF-65 → `novel-writing/workflow-profile.md` §5.5.4 rules architecture first; a future large implementation may split into `novel-rules-architecture.md`
   - DF-66 → `novel-writing/workflow-profile.md` §5.5.5 `Logs/` section extension
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

**Latest active iteration**

- **None** (V1.57 Shipped 2026-06-22; integration branch `iteration/v1.57` ready for PR to `main`; V1.58+ will be the next active iteration).

**Latest shipped iteration**

- **V1.57** (Shipped 2026-06-22; PR-ready on `iteration/v1.57`): [v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md](../iterations/v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md) — DF-46 Full Parity & DF-47 Unification (T-A single axis): P0 Spec Governance (bridge→Master + acp §4 roster rewrite 41 rows + capability::Registry consolidation + R-V156P3-S003 field drops); P1 Daemon Refactor (host_tool_executor.rs 4298→349 lines + 3 caller entry points + `nexus42 host-call` CLI + CdnConfig constructor-injection closing R-V156P1-M002 + 4 spec amendments); P2 V1.56 Carry-Forwards (R-V156P1-M001 schema rename with backward-compat serde aliases + 5 reproducer tests); P3 Worker IPC (dynamic allowlist 1→18 IDs + 54-case cross-caller E2E in `cross_caller_e2e.rs`); P-mid meta tracking; P-last hygiene (bridge Master promote + capability-registry.md fold-in + Profile B compaction + tracker V1.57 snapshot + DF-46 reduced + tech-debt rollup + report-only QA). 7 plans all Done; Profile B compaction applied; integration branch `iteration/v1.57` ready for PR. 3 V1.57 carry-forwards CLOSED (R-V156P1-M001/M002/P3-S003); 3 new V1.57+ residuals filed (R-V157P0-L001/L002 + R-V157P1-W001). DF-46 reduced: 41-row roster in `acp-capability-set.md` §4 (18 `shipped` + 18 `catalog-only` + 3 `scaffold-equivalent` + 2 `OUT`).

- **V1.56** (Shipped 2026-06-21 via PR #78 @ `8a2fb20e`): [v1.56-workspace-and-routing-seam-closure-delivery-compass-v1.md](../iterations/v1.56-workspace-and-routing-seam-closure-delivery-compass-v1.md) — Workspace & Routing Seam Closure: P0 DF-31 full + DF-42 full Local API redesign (`/v1/local/*` + workspace OCC + persistent sessions + changes[] payload); P1 DF-29 `nexus.registry.refresh` capability (synthetic default + optional `--cdn-url`); P2 DF-56 independent slice (conditional `next` + expression routing + multi-branch + merge points); P3 DF-56 dependent slice (registry conditional edges + workspace branch inputs); P-mid meta tracking; P-last R-V155P2-F002 fix-wave (game-bible design-writing section_status auto-transition) + Profile B. 7 plans all Done; merged to `main` at `8a2fb20e` via PR [#78](https://github.com/42ch-dev/nexus/pull/78) on 2026-06-21. `iteration/v1.56` retired. 35 open V1.57+ residuals registered (18 medium + 17 low); 3 absorbed into V1.57 plan slots.

- **V1.55** (Shipped 2026-06-21 via PR #77 @ `9d2893c2`): [v1.55-non-novel-profile-completion-and-infrastructure-refactor-delivery-compass-v1.md](../iterations/v1.55-non-novel-profile-completion-and-infrastructure-refactor-delivery-compass-v1.md) — Non-novel profile completion & infrastructure refactor: P0 DF-43 SQLite persistence alignment; P1 DF-31 workspace interface (skeleton); P2 game-bible Depth 3.5 (design-writing + 五问 + section completion + KB extraction); P3 script scaffold (Scripts/ + Beats/ + Characters/ + Logs/); P-mid meta tracking; P-last spec hygiene + Profile B. 7 plans all Done; merged to `main` at `9d2893c2` via PR #77 on 2026-06-21. `iteration/v1.55` retired.

- **V1.54** (Shipped 2026-06-21 via PR #76): [v1.54-df46-completion-and-game-bible-foundation-delivery-compass-v1.md](../iterations/v1.54-df46-completion-and-game-bible-foundation-delivery-compass-v1.md) — DF-46 Completion & Game-Bible Foundation: P0 DF-46 full-spectrum write tools (6 tools) + LazyLock registry cache + 13 V1.53 residuals all converged; P1 game-bible scaffold (Depth 2: spec + 7 BlockType variants + bootstrap + 12 Design templates); P-last capability-registry.md Draft → Master + Profile B + shipped snapshot. 4 plans all Done; merged to `main` at `2fd183f0` via PR [#76](https://github.com/42ch-dev/nexus/pull/76) on 2026-06-21. `iteration/v1.54` retired. 2 open residuals (R-V154P1-W001 scaffold atomicity, R-V154P1-S002 profile-gate observability) deferred to V1.55+.

- **V1.53** (Shipped 2026-06-20): [v1.53-capability-surface-completion-and-skills-cli-cleanup-delivery-compass-v1.md](../iterations/v1.53-capability-surface-completion-and-skills-cli-cleanup-delivery-compass-v1.md) — Capability Surface Completion & Skills CLI Cleanup: P0 CapabilityRegistry SSOT (3 sub-phase cutover, 8 → 13 host tools), P1 DF-46 read slice (5 new read-heavy `nexus.*` tools + cross-creator isolation), P-c skills-export CLI cleanup (DF-50 Cancelled), P-last spec hygiene + dual Profile B (V1.53 + V1.52 retro). 5 plans all Done; merged to `main` at `e6c214840e457faaa23298a532b4b0de90905807` via PR [#74](https://github.com/42ch-dev/nexus/pull/74). `iteration/v1.53` retired. 13 open residuals (4 medium + 9 low) deferred to V1.54+. `capability-registry.md` kept as Draft overlay (Master promotion deferred to V1.54+).

**Recent shipped compasses** (detail in archive §2)

- **V1.52** (Shipped 2026-06-19): [v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md](../iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md) — Author Completion & Multi-Branch Preset Orchestration: T-A outline 五问 + auto-promote + CLI consolidation + Work→KeyBlock provenance + essay profile; T-B N-way GO/NOGO + branch merge semantics. 7 plans all Done; merged to `main` at `d6aadd2fb5f287056dbd41b701eea8d5e6114dcc` via PR [#73](https://github.com/42ch-dev/nexus/pull/73); `iteration/v1.52` retired. (Profile B retroactively completed by V1.53 P-last.)

- V1.44 (Shipped 2026-06-13): [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) — DF-69 + review-master CLI + multi-volume + author-desk; PR #57 merged `76a9eb79`.
- V1.43 (Shipped 2026-06-12): [v1.43-novel-author-experience-delivery-compass-v1.md](../iterations/v1.43-novel-author-experience-delivery-compass-v1.md) — BL-10 author quickstart + CLI copy P1 + author visibility P2 + P-last hygiene; `iteration/v1.43` retired.
- V1.42 (Shipped 2026-06-12): [v1.42-multi-volume-serial-writing-delivery-compass-v1.md](../iterations/v1.42-multi-volume-serial-writing-delivery-compass-v1.md) — P0 runtime_lock + P1 DF-62 + P2 DF-56 + P3 DF-47 + P-last UX.
- V1.41 (Shipped 2026-06-11): [v1.41-multi-work-author-desk-delivery-compass-v1.md](../iterations/v1.41-multi-work-author-desk-delivery-compass-v1.md) — PR #53; DF-60/61 archived.
- V1.40 (Shipped 2026-06-11 via PR #52 merged): [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md) — DF-63 closed; `iteration/v1.40` retired.
- V1.39 (Shipped 2026-06-09): [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) — **DF-53 full auto-chain + DF-68 daemon continuation + DF-64/65/66/67 quality loop**; P0..P5 on `iteration/v1.39`; PR #50 merged ad9725d8.
- V1.38 (Shipped 2026-06-09): [v1.38-multi-chapter-serial-writing-delivery-compass-v1.md](../iterations/v1.38-multi-chapter-serial-writing-delivery-compass-v1.md) — DF-62 first slice shipped (PR #49).
- V1.37 (Shipped 2026-06-08): [v1.37-novel-writing-foundation-delivery-compass-v1.md](../iterations/v1.37-novel-writing-foundation-delivery-compass-v1.md) — **Novel Writing UX foundation-first**: P0 shipped init `preset.input` plumbing, runtime `gates:` evaluation, scaffold atomicity, and first-run remediation; P1/P2/P3 roadmap multi-chapter DF-62, World KB DF-63, and quality-loop DF-64/65/66/67.
- V1.36 (Shipped 2026-06-07): [v1.36-novel-writing-ux-delivery-compass-v1.md](../iterations/v1.36-novel-writing-ux-delivery-compass-v1.md) — **novel-writing正文产出 UX** (5 implement plans P0–P4 + prepare P-1 all Done; PM-validate path used for P1–P4 under time pressure; DF-57/58 closed; DF-53 partial again on top of V1.35 P4; DF-47 stays conditional not P0; DF-59 backlog); single-chapter MVP (outline_chapter → draft_chapter → finalize with llm_judge 五问) + completion stop + `Works/<work_ref>/` layout + `work_chapters` DB SSOT + `creator run reconcile-chapters` + `--force-gates` gate override
- V1.35 (Shipped 2026-06-07): [v1.35-cli-ia-and-product-polish-delivery-compass-v1.md](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md) — CLI IA (5 groups; sync→platform), creator hub polish, critical residual P0 (6 criticals + R-CURSOR-PR42-03 + 5 backlog), FL-E UX polish (chain default true); 5 implement plans P0/P2/P3/P4/P5 + prepare P-1 + P1 docs all Done; DF-47 later reclassified as conditional, not V1.36 P0
- V1.34 (Shipped 2026-06-05): [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) — FL-E + Agent tools; DF-47 carried forward to V1.35 and later reclassified as conditional
- V1.33 (Shipped 2026-06-04): [v1.33-work-experience-loop-delivery-compass-v1.md](../iterations/v1.33-work-experience-loop-delivery-compass-v1.md) — narrative Work loop, Creative Brief Intake, `creator run`, `llm_judge` fix, memory review closed loop; 5 plans P1–P5 all Done
- V1.32: [v1.32-preset-quality-gate-delivery-compass-v1.md](../iterations/v1.32-preset-quality-gate-delivery-compass-v1.md)
- V1.31: [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)
- V1.30: [v1.30-residual-convergence-delivery-compass-v1.md](../iterations/v1.30-residual-convergence-delivery-compass-v1.md)

**Knowledge & specs**

- Shipped history archive: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)
- Done plans index: [archived/plans-done.json](../archived/plans-done.json) (string list of plan_ids; full JSON in [archived/plans/](archived/plans/))
- CLI IA (V1.45): [specs/creator-run-preset-entry.md](specs/creator-run-preset-entry.md) (**Shipped Master V1.45**); three-plane IA superseded the V1.35 [specs/cli-command-ia.md](specs/cli-command-ia.md) overlay
- Orchestration engine: [specs/orchestration-engine.md](specs/orchestration-engine.md)
- Creator schedule & core context: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)
- Iteration index: [iterations/README.md](../iterations/README.md)
- Machine state: [status.json](../status.json) (SSOT for residuals, iteration complete flags, ship records)

External (via `.mstar/local-paths.json`): `{v1-spec}/architecture/v1.md`, `{platform-designs}/roadmap.md`

---

*Last updated: 2026-06-22 (V1.57 closeout: 7 plans all Done; Profile B compaction applied; 3 V1.57 carry-forwards CLOSED; 3 new V1.57+ residuals filed; DF-46 reduced to 41-row roster; bridge→Master promotion finalized; capability-registry.md folded-in; shipped-features-tracker V1.57 snapshot; tech-debt rollup; pre-implement gate: was go; report-only QA pending; integration branch `iteration/v1.57` ready for PR to `main`). Status: **V1.57 Shipped**. PR-ready.*
