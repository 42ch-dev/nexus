# Shipped Features — Cross-Version Archive

**Status**: Archived (append-only)  
**Purpose**: Long-term **append-only** archive for closed deferred-feature tracker rows (shipped, cancelled, superseded) and per-version delivery snapshots. The **active** open/backlog tracker lives in [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md).  
**Scope**: `nexus` OSS repository only.  
**Location**: Top-level harness archive (`.mstar/archived/`) — not under `archived/knowledge/` (implementation knowledge supersession).  
**Split from**: [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md) §4–§5 (2026-05-30 restructure)  
**Created**: 2026-05-30  
**Last updated**: 2026-06-12

When a version ships, append new closed rows here and remove them from the active tracker open tables.

---

## 1) Closed items

### Features shipped

| ID | Feature | Shipped in | Notes |
|----|---------|------------|-------|
| ~~DF-A~~ | `context.summarize` capability (LLM-driven core_context summarisation) | V1.5 (WS-C) | V1.4 reserved `DerivationKind::LlmSummarize`; V1.5 implemented. |
| ~~DF-B~~ | Schedule cron / wall-clock triggers (`scheduled_at` column) | V1.5 (WS-D) | V1.4 reserved column; V1.5 added clock poller zero-migration. |
| ~~DF-C~~ | System-managed multi-preset scheduler (`_system/` directory) | V1.6 (WS-D / Track D) | V1.4 backlog → V1.5 deferred → V1.6 implemented. |
| ~~DF-D~~ | V1.5 residual R1 — cancel signal ignores `pause_schedule()` error | V1.6 (WS-A) | Medium severity. Fixed in `nexus42d/.../schedules.rs`. |
| ~~DF-E~~ | V1.5 residual R2 — `resume_schedule()` TOCTOU race | V1.6 (WS-A) | Medium severity. Fixed in `nexus-orchestration/.../supervisor.rs`. |
| ~~DF-F~~ | V1.5 residual R3 — `Scheduler::tick()` dead code | V1.6 (WS-A) | Low severity. Removed redundant DB query path. |
| ~~DF-G~~ | V1.5 residual R6 — Recovered sessions lack FlowRunner | V1.6 (WS-A) | Low severity. Session recovery after daemon restart fixed. |
| ~~DF-H~~ | ACP SDK DTO decoupling (Nexus-owned trait types) | V1.6 (WS-B) | Nexus-owned DTOs for `NexusAcpClient` trait. Preparation for DF-04. |
| ~~DF-I~~ | ACP permission policy CLI surface (`nexus42 permission`) | V1.6 (WS-C) | CLI command group: list/grant/deny/ask/revoke/reset. |
| ~~DF-J~~ | Full daemon lifecycle state machine (6-state FSM) | V1.4 (WS4) | `statig` HSM. Originally deferred from V1.2 matrix (TD-9-FU). |
| ~~DF-K~~ | User registration / Creator binding full story | V1.3 | V1.2 deferred to V1.3. Creator register CLI delivered. |
| ~~DF-01~~ | Multi-agent worker (single worker hosting >1 ACP agent) | V1.7 (WS-E) | Approach A (multiplex one worker). WorkerRegistry: `HashMap<CreatorId, WorkerHandle>`. |
| ~~DF-04~~ | ACP SDK migration to sacp v1.0 | V1.7 (WS-D) | Adapter-trait policy. SDK types confined to `AcpSdkAdapter`. |
| ~~DF-08~~ | Wire/local drift auto-detect tooling | V1.9 (WS-D) | Automated detection of schema classification drift. CLI command delivered. |
| ~~DF-09~~ | Template_file path validation | V1.9 (WS-B) | Filesystem preset path traversal protection. |
| ~~DF-10~~ | Starting lifecycle edge cases | V1.9 (WS-C) | HealthDegraded during Starting, Starting.exit in-flight cancel. |
| ~~DF-17~~ | Third-party preset loading (`~/.nexus42/presets/`) + CLI init templates | V1.9 (WS-A) | Path corrected from `~/.nexus/strategies/` to `~/.nexus42/presets/`. |
| ~~DF-11~~ | CoreContext Handlebars template engine binding | V1.13 | WS7 data path + template rendering integrated per V1.13 OSS-forward delivery. |
| ~~DF-14~~ | CLI + Platform e2e integration | V1.13 | Staged cross-repo gates + harness per V1.13 OSS-forward delivery. |
| ~~DF-38~~ | OrchestrationEngine instantiation in daemon | V1.25 audit hygiene | Shipped before V1.25: `crates/nexus-daemon-runtime/src/boot.rs` instantiates `GraphFlowEngine::new_with_storage(...)`, stores it as `Arc<dyn OrchestrationEngine>`, and calls `state.set_engine(...)`. The older `lifecycle/actions.rs` comment still says “Instantiate OrchestrationEngine (stub, subsystem task)”; that comment is stale evidence only, not current product state. |
| ~~DF-39~~ | Worker Manager subsystem wiring in daemon lifecycle | V1.25 audit hygiene | Shipped before V1.25: `crates/nexus-daemon-runtime/src/boot.rs` creates `WorkerManager::new()` and calls `state.set_worker_manager(...)`; `lifecycle/subsystems/worker_mgr.rs` describes the real subsystem replacing the mock stub. The older `lifecycle/actions.rs` comment still says “Start Worker Manager (stub, subsystem task)”; that comment is stale evidence only and is distinct from remaining task-level worker-handle fallback tracked by DF-37. |
| ~~FL-C~~ | Structured KB query + context assembly convergence | V1.28 | `assemble-moment` SSOT; KbQuery + cross-domain token budget; `assemble-local` removed. Plans: `2026-05-25-v1.28-context-assembly-convergence`, agent-host plans, `local-ssot-refresh`. |
| ~~DF-30~~ | `creator.read_memory` / `write_memory` / `inject_prompt` de-stub | V1.31 | Plan `2026-05-30-v1.31-creator-memory-capabilities`: real SQLite read/write via `CreatorCapabilityStore`; `inject_prompt` persisted queue in `state.db`. |
| ~~DF-32~~ | `judge.rule` expression engine | V1.31 | Plan `2026-05-30-v1.31-judge-and-summarize-capabilities`: boolean literals, field equality/inequality, and numeric comparisons over `contextData`. |
| ~~DF-33~~ | `judge.llm` worker-backed GO/NOGO judge | V1.31 | Plan `2026-05-30-v1.31-judge-and-summarize-capabilities`: executes via `WorkerHandleProvider::call_acp_prompt` with `deny_all` and parses GO/NOGO. |
| ~~DF-34~~ | `context.summarize` worker-backed summarization | V1.31 | Plan `2026-05-30-v1.31-judge-and-summarize-capabilities`: executes via `WorkerHandleProvider` and returns `{ summary, prompt_hash }`. |
| ~~DF-37~~ | Worker-handle plumbing for capability-layer LLM calls | V1.31 | Plan `2026-05-30-v1.31-judge-and-summarize-capabilities`: `Arc<dyn WorkerHandleProvider>` injected through `CapabilityRegistry::with_runtime_deps()`; fallback limited to explicit standalone/test mode. |
| ~~BL-09~~ | V1.17 Prompt + Skills Compass v1 | V1.17 | Shipped V1.17 — see archive §2 V1.17 snapshot. |
| ~~DF-51~~ | `creator.inject_prompt` wire/schema alignment | V1.34 (P0) | Commits a044f94 + 71c10cc on `feature/v1.34-residual-convergence`: input_schema declares `prompt_file` + `vars` with `anyOf`; `R-P2-01` closed. |
| ~~DF-54~~ | Work `stage` / `stage_status` persistence gap | V1.34 (P1) | Commits 655d71c + R-FL-E-01..08 on `feature/v1.34-fl-e-run-intents-and-stages`: stage columns + DDL migration + 11 e2e tests + active schedule uniqueness. |

### Tech-debt residuals shipped

| ID | Title | Shipped in | Notes |
|----|-------|------------|-------|
| ~~DTO-C1~~ | `sdk_new_session_request_from_nexus` silently drops `mcp_servers` field | V1.7 | Medium severity. Fixed in `nexus-acp-host`. |
| ~~DTO-C2~~ | Protocol version `.parse::<u16>().unwrap_or(1)` silent degradation | V1.7 | Low severity. Fixed in `nexus-acp-host`. |
| ~~DTO-W1~~ | `NexusContentBlock` missing `Eq` derive | V1.7 | Nit. Fixed in `nexus-contracts`. |
| ~~PERM-W1~~ | `policy.rs` save re-serializes TOML losing comments/format | V1.7 | Low severity. Fixed in `nexus-acp-host`. |
| ~~PERM-W2~~ | JSON permission list omits global rules when agent filter used | V1.7 | Low severity. Fixed in `nexus42`. |
| ~~PERM-W3~~ | Unvalidated TOML keys in permission commands | V1.7 | Low severity. Fixed in `nexus42`. |
| ~~R4~~ | `SystemClock` DST safety not implemented | V1.7 | Low severity. Fixed in `nexus-orchestration`. |
| ~~R7~~ | `schedule_guards` HashMap grows unbounded | V1.7 | Nit. Fixed in `nexus-orchestration`. |
| ~~TD-10~~ | Device flow OAuth — production auth deferred; stub `verify_device_code` only | V1.10 | Low severity. Replaced by real Device Flow Login (WS-A). |
| ~~DF-57~~ | `Works/<work_ref>/` artifact layout + sync scan migration | **V1.36 P2** (Shipped 2026-06-07) | Pre-1.0: no legacy `Stories/<story_ref>/` shims. Plan `2026-06-07-v1.36-novel-artifact-layout-and-templates`; `sync_module` rewritten to scan only `Works/<work_ref>/Stories/*.md`; per-chapter metadata derived from `work_chapters` table; 5 novel-writing templates (chapter-outline / chapter-body / volume-outline / foreshadowing / event-index). |
| ~~DF-58 (V1.36)~~ | Interactive novel project init preset (`novel-project-init`) | **V1.36 P1** (Shipped 2026-06-07) | Separate grill-me preset; not embedded in `novel-writing` auto-chain. Plan `2026-06-07-v1.36-novel-project-init-preset`; 10 prompts (init-intro, init-title, init-genre, init-chapters, init-work-ref, init-world + 3 branches, init-summary) + 4 templates (README, foreshadowing, event-index, volume-outline); `novel.project_scaffold` capability with atomic FS+DB transaction (ScaffoldTransaction with Drop rollback) + sanitization (`validate_work_ref` / `validate_slug` / `validate_total_chapters` 1..=100) + world_id FK existence check. |
| ~~DF-60~~ | Multi-novel lifecycle (2-step completion + completion-lock + runtime lock columns + `creator works` IA) | **V1.41 P0** (Shipped 2026-06-11) | PR [#53](https://github.com/42ch-dev/nexus/pull/53) merged to `main`; post-merge `12753eb8` lineage validation. Plan [2026-06-10-v1.41-multi-work-switch.md](../plans/2026-06-10-v1.41-multi-work-switch.md). Spec [novel-multi-work-lifecycle.md](../knowledge/specs/novel-multi-work-lifecycle.md). **Note:** production `runtime_lock_holder` acquire deferred V1.42 P0. |
| ~~DF-61~~ | Selection pool + inspiration pool (DB SSOT + `Pool/Ideas/` MD) | **V1.41 P1** (Shipped 2026-06-11) | PR #53; post-merge `156e669d` `set_pool_active` creator_id authz. Plan [2026-06-10-v1.41-selection-pool.md](../plans/2026-06-10-v1.41-selection-pool.md). Spec [novel-work-pool.md](../knowledge/specs/novel-work-pool.md). |
| ~~BL-10~~ | Novel writing author quickstart (`docs/novel-writing-quickstart.md`) | **V1.43 P0** (Shipped 2026-06-12) | Shipped on `iteration/v1.43` (merge `340423e5`, 2026-06-12). Plan [2026-06-12-v1.43-novel-writing-quickstart.md](../plans/2026-06-12-v1.43-novel-writing-quickstart.md). Spec [novel-author-experience.md](../knowledge/specs/novel-author-experience.md). QC tri-review Approve (qc1 `efc8cfda`, qc2 `84e28acf`, qc3 `16953b9a` reval #2); QA Pass with residuals (`2709506a`). New file `docs/novel-writing-quickstart.md` (280 lines; Part I §1–§6 ongoing serial + Part II A/B/C optional/advanced) + 1-line cross-link in `docs/ARCHITECTURE.md`. 2 open residuals carry-forward to P-last hygiene plan `2026-06-12-v1.43-hygiene-and-residuals`: **R-V143P0-001** (spec overlay `novel-author-experience.md` §2 row 4 references stale `creator run status`; should be `creator works status` per V1.41 cli-spec.md §6.2H) + **R-V143P0-002** (spec/CLI drift: `novel-workflow-profile.md` §5.5.3 + `novel-quality-loop.md` §6 reference future `creator run review-master <work_id>` surface, not yet implemented in current CLI; quickstart line 168 has an inline note for readers). |
| ~~DF-69~~ | **Standalone manuscript audit preset** (review report **or** KB extract on chapter正文) | **V1.44 P0** (Shipped 2026-06-13) | Dual-mode embedded preset `novel-manuscript-audit` (split into `novel-manuscript-audit-review` + `novel-manuscript-audit-extract` per R-V144P0-001 fix wave) + CLI entry `creator run audit-chapter --mode review|extract`. Review mode: structured 五問 report → `Logs/review/`. Extract mode: sync `kb.extract_work` without `kb_extract_jobs` (distinct from shipped `creator kb queue-extract --chapter`). Does NOT enter FL-E auto-chain driver. Plan [2026-06-13-v1.44-manuscript-audit-preset.md](../plans/2026-06-13-v1.44-manuscript-audit-preset.md). Spec [novel-manuscript-audit.md](../knowledge/specs/novel-manuscript-audit.md) (promoted Draft overlay → Shipped Feature line in P-last T1). Fix wave: R-V144P0-001..010 all resolved before ship (qc-specialist F-001 Critical + 11 Warning → fix commits `d6b9400e..fc9f2f6d` → targeted re-review Approve all 3 → QA Approve `5a0548c5` → Done). |

### Cancelled / Superseded

| ID | Status | Cancelled in | Reason |
|----|--------|--------------|--------|
| ~~DF-L~~ | **Cancelled** | V1.6 (accepted) | rand 0.7.3 blocked by wiremock — accepted as permanent tech debt. |
| ~~DF-M~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-07 — Capability schema registry sharing with platform. Over-designed. |
| ~~DF-N~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-02 — User-authored capabilities (shell / WASM plugin ABI). Over-designed. |
| ~~DF-O~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-05 — Full ACP permission policy engine UI (web-based). Not core product value. |
| ~~DF-P~~ | **Superseded** | 2026-04-21 (V1.7 planning) | DF-06 — Preset hot-reload. Snapshot semantics correct; real need → DF-17. |
| ~~DF-15~~ | **Cancelled** | V1.13 (governance closure) | OpenAPI export work. Nexus is not an OpenAPI-first product boundary for runtime value delivery; V1.13 resolves tracker ambiguity as governance-only closure with no implementation scope. |

### Tech-debt residuals shipped (V1.30)

| ID | Title | Shipped in | Notes |
|----|-------|------------|-------|
| ~~R5~~ | body.md written before SQL INSERT — orphaned files on DB failure | V1.30 | Write order reversed: SQL INSERT first, body.md second. |
| ~~R6~~ | list_references returns all rows unbounded — no pagination | V1.30 | LIMIT/OFFSET pagination with DEFAULT_PAGE_LIMIT; clamped 1..=1000. |
| ~~R7~~ | content_hash always NULL — integrity field unused | V1.30 | blake3 content_hash computed on registration. |
| ~~R8~~ | db_err maps all sqlx::Error to ValidationError — no Storage variant | V1.30 | `NarrativeError::Storage` variant introduced. |
| ~~R9~~ | KbQuery fetches all blocks into memory — no DB-level pagination | V1.30 | `LIST_BY_WORLD_LIMIT=500` added to `list_by_world`. |
| ~~R14~~ | SessionCapture created at agent-stop time — near-zero metrics | V1.30 | SessionCapture at session start with `session_captures` map in `MultiplexedWorkerState`. |
| ~~R15~~ | KB extract job claim not atomic across next_queued + mark_running | V1.30 | Atomic `claim_job()`: SELECT+UPDATE in single tx + `rows_affected()` check. |
| ~~R16~~ | kb.extract_work placeholder — no full extraction lifecycle | V1.30 | Full e2e: claim → extract → parse → mark_done → KeyBlock insert. |
| ~~R17~~ | Persistent child Drop cleanup is best-effort and Unix-only | V1.30 | SIGTERM→wait→SIGKILL + PID existence check (`kill -0`). |
| ~~R18~~ | KB extract job IDs use custom timestamp-derived generation | V1.30 | UUIDv4 with `xj_` prefix + `insert_with_retry`. |
| ~~R19~~ | creator command module approaching maintainability threshold | V1.30 | KB handlers extracted to `creator/kb.rs` (973 lines); `mod.rs` reduced ~30%. |
| ~~R20~~ | KB extract status list is unbounded | V1.30 | Bounded listing with `limit=100` default. |

---

## 2) Per-version summary

### Shipped in V1.7

| Category | Count | IDs |
|----------|-------|-----|
| Features | 2 | DF-01 (multi-agent worker), DF-04 (ACP SDK migration) |
| Tech-debt residuals | 8 | DTO-C1, DTO-C2, DTO-W1, PERM-W1, PERM-W2, PERM-W3, R4, R7 |
| **Total** | **10** | |

### Shipped in V1.8

| Category | Count | Notes |
|----------|-------|-------|
| Features from tracker | 0 | V1.8 was purely CLI spec alignment (`--handle`, `--name` flag, LLM fallback) |
| New residuals introduced | 4 | R-WA-001, R-WA-002, R-WC-002, R-WC-003 |
| **Total** | **4 new residuals** | No tracker items scoped into V1.8 |

### Shipped in V1.9

| Category | Count | IDs |
|----------|-------|-----|
| Features | 4 | DF-08 (drift auto-detect), DF-09 (template_file validation), DF-10 (Starting lifecycle), DF-17 (third-party presets) |
| New residuals introduced | 7 | R-M1-W02 through R-M1-W07, R-M1-W09 |
| **Total** | **11** | 4 features shipped + 7 new residuals created |

### Shipped in V1.10

| Category | Count | IDs |
|----------|-------|-----|
| Tech-debt residuals | 1 | TD-10 (Device Flow Login — real auth replaced stub) |
| New residuals introduced | 2 | R-V110-003, R-V110-004 |
| **Total** | **3** | 1 residual closed + 2 new residuals created |

### Shipped in V1.13

| Category | Count | IDs |
|----------|-------|-----|
| Features | 2 | DF-11 (Handlebars binding), DF-14 (CLI+Platform e2e) |
| Governance closure | 1 | DF-15 (Cancelled — OpenAPI export) |
| Tech-debt residuals | 0 | — |

### V1.14 delivery snapshot (registered)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.14-delivery-compass-v1.md](../iterations/v1.14-delivery-compass-v1.md)（§0 scope lock **合并于**本 compass） |
| Machine state | `status.json` `plans[]` **空**；`residual_findings` 仅 **R-V113-005** / **R-V113-007**（accepted / backlog） |
| Platform execution | **Done** — `nexus-platform` Plans **86–87**（rate-limit/JWKS + OpenAPI doc batch）；详见平台仓 `status.json` `metadata.tech_debt_summary.note` |
| Cross-repo gates | Canonical: `nexus-platform/.mstar/knowledge/v1.14-program-compass-v1.md` §5 |

### V1.15 delivery snapshot (Done)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.15-delivery-compass-v1.md](../iterations/v1.15-delivery-compass-v1.md)（§0 scope lock, D1-D7 architecture decisions） |
| Machine state | `status.json` `plans[]` **empty** (archived); `residual_findings` unchanged — **R-V113-005** / **R-V113-007**（accepted / backlog） |
| Plan | `2026-05-10-v1.15-orchestration-first-pipeline` — **Done** (archived to `archived/plans/`) |
| PR | [#23](https://github.com/42ch-dev/nexus/pull/23) merged to `main` |
| QC | Triple review: QC1 Request Changes (pre-existing auth test drift), QC2 Approve, QC3 Approve (3 warnings accepted) |
| Cross-repo gates | G1–G3 done, G4 done (tracker aligned), G5 done (QC triple complete) |
| New tracker items | None — all V1.15 work was new features, no DF-* items from tracker were in scope |
| New residuals | None formally filed — QC3 warnings (skill_sync I/O, skill_link TOCTOU, sync_module unbounded memory, embedded_skills linear search) accepted in-place |

### V1.18 delivery snapshot (Done)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.18-delivery-compass-v1.md](../iterations/v1.18-delivery-compass-v1.md)（§0 scope lock, R-001–R-010 requirements, §9 deferred D-001–D-008） |
| Machine state | `status.json` `plans[]` **empty** (archived); `residual_findings` includes V1.18 code-quality residuals + V1.19 deferred functional gaps |
| Plan | `2026-05-15-v1.18-agent-host-core` — **Done** (archived to `archived/plans/`) |
| New tracker items | 11 | DF-18 through DF-28 (deferred from V1.18 §9 + QC residuals → V1.19 hardening backlog) |
| Post-implementation audit | §9 of compass updated with R-003/R-005/R-006/R-007 audit notes, 3 new risk rows |

### V1.21 delivery snapshot (Done)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.21-local-platform-isolation-delivery-compass-v1.md](../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) |
| Machine state | `status.json` `plans[]` **empty** (archived); all gates passed (QC tri-review: 3× Approve, QA: 7/7 Pass) |
| Plan | `2026-05-20-v1.21-local-platform-isolation` — **Done** (archived to `archived/plans/`) |
| PR | [#28](https://github.com/42ch-dev/nexus/pull/28) merged to `main` |
| QC | Triple review: QC1 Approve, QC2 Approve (2 low warnings accepted), QC3 Approve (2 suggestions) |
| QA | 7/7 acceptance criteria verified |
| Scope | Renamed `nexus-sync` → `nexus-cloud-sync` with `legacy-sync` feature; split `nexus-domain` into 6 focused crates; isolated daemon from cloud deps; wired CLI to cloud-sync directly; stubbed orchestration sync capabilities |
| New tracker items | 13 | DF-29 through DF-41 (orchestration capability stubs, daemon lifecycle stubs, agent slot stub — see §3.1) |

### V1.21 delivery snapshot (Done)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.21-local-platform-isolation-delivery-compass-v1.md](../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) |
| Machine state | `status.json` `plans[]` **empty** (archived); all gates passed (QC tri-review: 3× Approve, QA: 7/7 Pass) |
| Plan | `2026-05-20-v1.21-local-platform-isolation` — **Done** (archived to `archived/plans/`) |
| PR | [#28](https://github.com/42ch-dev/nexus/pull/28) merged to `main` |
| QC | Triple review: QC1 Approve, QC2 Approve (2 low warnings accepted), QC3 Approve (2 suggestions) |
| QA | 7/7 acceptance criteria verified |
| Scope | Renamed `nexus-sync` → `nexus-cloud-sync` with `legacy-sync` feature; split `nexus-domain` into 6 focused crates; isolated daemon from cloud deps; wired CLI to cloud-sync directly; stubbed orchestration sync capabilities |
| New tracker items | 13 | DF-29 through DF-41 (orchestration capability stubs, daemon lifecycle stubs, agent slot stub — see §3.1) |

### V1.24 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.24-knowledge-crates-alignment-audit-compass-v1.md](../iterations/v1.24-knowledge-crates-alignment-audit-compass-v1.md) |
| Status | Shipped (2026-05-22) |
| Scope | Normative spec refresh; KCA-002 B2; KCA-003 C2; tracker hygiene |
| New tracker items | 2 | DF-42, DF-43 |

### V1.26 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.26-local-persistence-delivery-compass-v1.md](../iterations/v1.26-local-persistence-delivery-compass-v1.md) |
| Shipped at | 2026-05-23 |
| Plans | iteration-hygiene, reference-store-layout, narrative-kb-persistence, local-context-product |
| Platform | `metadata.platform_integration` = paused |
| Open residuals into V1.27 | R10 (InMemory knowledge), R3 (KB scope), R5–R9 (nit/low) |

### V1.27 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.27-local-authoring-delivery-compass-v1.md](../iterations/v1.27-local-authoring-delivery-compass-v1.md) |
| Shipped at | 2026-05-24 (`status.json` `latest_shipped_iteration`) |
| Scope | CLI-first local writes; `creator demo seed`; four-domain persistent `assemble-moment`; API/CLI hygiene; `acp agent use` |
| Plans | `2026-05-24-v1.27-narrative-world-writes`, `knowledge-persistence-context`, `api-cli-hygiene`, `acp-agent-use` |
| Note | Local world fork explicitly out (platform-only per PD-01 / V1.28) |

### V1.28 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.28-context-and-agent-host-delivery-compass-v1.md](../iterations/v1.28-context-and-agent-host-delivery-compass-v1.md) |
| Shipped at | 2026-05-25 (`status.json` `latest_shipped_iteration`) |
| PR | [#36](https://github.com/42ch-dev/nexus/pull/36) merged to `main` |
| Scope | `assemble-moment` SSOT (remove `assemble-local`); KbQuery + token budget; Agent Host Batch 1 (DF-18–20, 24–26); SSOT doc refresh |
| Plans | `2026-05-25-v1.28-context-assembly-convergence`, `agent-host-acp-correctness`, `agent-host-native-multiturn`, `local-ssot-refresh` |
| Tracker | FL-C shipped; Batch 1 DF items closed; `local-cloud-crate-architecture` backfill deferred to V1.29 spec plan H0 |

### V1.29 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md](../iterations/v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md) |
| Shipped at | 2026-05-26 (`status.json` `latest_shipped_iteration`) |
| Scope | FL-A (session review, SOUL Experience preset, Stage0 delimiters); FL-B (kb extract queue + preset); Agent Host Batch 2; spec/tracker hygiene |
| Plans | `2026-05-26-v1.29-*` (six plans — all Done, archived to `plans-done.json`) |
| Shipped DF items | DF-21, DF-22, DF-23, DF-27, DF-28 (Batch 2); DF-35, DF-36 (partial); FL-A, FL-B (product lines) |
| Closed residuals | R11 (Drop kill), R12 (cancel), R13 (Stage0 markdown heuristic) |
| New residuals (v1.30) | R14–R20 (7 findings: 2 medium, 5 low/nit) |
| Explicit deferrals | FL-D full de-stub; DF-42; DF-44; platform unpause |

### V1.30 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.30-residual-convergence-delivery-compass-v1.md](../iterations/v1.30-residual-convergence-delivery-compass-v1.md) |
| Shipped at | 2026-05-26 (`status.json` `latest_shipped_iteration`) |
| PR | [#38](https://github.com/42ch-dev/nexus/pull/38) merged to `main` |
| Scope | Residual convergence — close all open residuals R5–R20 from V1.26–V1.29 delivery compasses |
| Plans | `2026-05-26-v1.30-*` (four plans — all Done, archived to `plans-done.json`) |
| Closed residuals | R5–R20 (12 findings: 2 medium, 8 low, 3 nit — **all fixed**) |
| QC | Tri-review: QC1 Approve; QC2 Request Changes → 4 Critical fixes landed → consolidated Approve; QC3 Request Changes → W-001 fix landed → consolidated Approve |
| Post-QC tech debt | 11 items (TD-V130-01..11: 8 low, 3 nit) — all `accept/defer`, backlog |
| Key changes | Atomic `claim_job()` + `rows_affected()`, UUID `xj_` job IDs, bounded listing (limit=100), full e2e `kb.extract_work` lifecycle, SessionCapture at session start, SIGTERM→SIGKILL + PID existence check, `creator/kb.rs` extraction (973 lines), write-after-INSERT + blake3 content_hash + pagination, `NarrativeError::Storage`, KB LIMIT 500 |
| Verification | 687 tests pass (0 failures); clippy clean on all V1.30 crates |

### V1.31 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md) |
| Shipped at | 2026-05-30 |
| PR | Pending — P4 spec/tracker hygiene branch prepares integration close |
| Scope | FL-D partial close: creator memory capabilities, rule/LLM judge, context summarization, worker-handle provider injection, and two embedded Agentic Design Pattern presets |
| Plans | `2026-05-30-v1.31-creator-memory-capabilities`, `2026-05-30-v1.31-judge-and-summarize-capabilities`, `2026-05-30-v1.31-agentic-pattern-presets`, `2026-05-30-v1.31-spec-tracker-hygiene` |
| Shipped DF items | DF-30, DF-32, DF-33, DF-34, DF-37 |
| Embedded presets | `reflection-loop`, `memory-augmented` |
| Explicit deferrals | DF-29 `registry.refresh`, DF-31 `workspace.*`, conditional routing engine, platform HTTP unpause |

### V1.32 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.32-preset-quality-gate-delivery-compass-v1.md](../iterations/v1.32-preset-quality-gate-delivery-compass-v1.md) |
| Shipped at | 2026-06-03 |
| Scope | Preset validator quality gate (shared semantic validation facade, asset/path safety, capability compat checks), embedded preset smoke coverage, SEC-V131-01 closure, spec/tracker hygiene |
| Plans | `2026-06-03-v1.32-preset-validator-core-and-cli` (P1), `2026-06-03-v1.32-embedded-presets-usability-hardening` (P2), `2026-06-03-v1.32-orchestration-security-followup` (P3), `2026-06-03-v1.32-spec-tracker-hygiene` (P4) |
| Closed residuals | SEC-V131-01 (medium — IDOR defense-in-depth fix: judge.llm + context.summarize now read only context-injected IDs) |
| Key changes | Shared `validate_preset_semantic` + `validate_assets_in_bundle` + `validate_path_safety` facade; CLI/API validate endpoint uses same facade as loader; reachability/terminal/bundle-id/orphan inner graph checks; O(1) capability registry lookup with arg drift detection; kb-extract inner graph wiring fixed; all 6 embedded presets pass strict validation; stale `--var` CLI removed |
| Known residuals deferred | R-P2-01 (creator.inject_prompt schema gap, Medium), R-P2-02 (same root cause, Low) |
| Explicit deferrals | DF-29, DF-31, DF-42, DF-44 remain open; platform pause (PD-05) preserved; conditional routing engine deferred |

### V1.33 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.33-work-experience-loop-delivery-compass-v1.md](../iterations/v1.33-work-experience-loop-delivery-compass-v1.md) |
| Shipped at | 2026-06-04 |
| Scope | Narrative **Work** product loop, Creative Brief Intake (grill-me), `creator run` high-level entry, preset run-intent taxonomy, `llm_judge` → `judge.llm` runtime fix, memory review/fragments closed loop |
| Plans | `2026-06-04-v1.33-work-model-and-creator-run` (P1), `2026-06-04-v1.33-creative-brief-intake-preset` (P2), `2026-06-04-v1.33-llm-judge-runtime-fix` (P3), `2026-06-04-v1.33-memory-review-closed-loop` (P4), `2026-06-04-v1.33-spec-tracker-hygiene` (P5) |
| Key changes | Work domain model (title, intake_status, inspiration_log, run_intents, stage); `creator run` CLI surface; `creative-brief-intake` + `novel-writing` preset; `judge.llm` parses LLM output (NOGO/GO with first-word anchor); memory review + fragments daemon API + CLI closed loop |
| Open residuals at close | R-V133P1-03, -05, -07, -08, -09, -11, -12 (7), R-V133P3-01..04 (4), R-V133P4-01..07 (7), R-P2-01, R-P2-02 — all shipped in V1.34 P0 (R-P2-01/02 closed) or V1.34+ |
| Explicit deferrals | DF-29, DF-31, DF-42, DF-44, DF-46, DF-48, DF-49, DF-50, DF-51 (deferred to V1.34), DF-52, DF-55, DF-56 |

### V1.34 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) |
| Shipped at | 2026-06-05 |
| PR | Pending — integration branch `feature/v1.34-creator-workflow-and-agent-tools` ready for PR to `main` |
| Scope | **FL-E** generic creator workflow on V1.33 Work (5 stages × preset chain) + **Agent `nexus.*` tool bridge** via daemon `HostToolExecutor` (8 tools: 6 `nexus.*` + 2 `fs/*` baseline) |
| Plans | `2026-06-04-v1.34-residual-convergence` (P0), `2026-06-04-v1.34-fl-e-run-intents-and-stages` (P1), `2026-06-04-v1.34-agent-tool-registry-spec` (P3), `2026-06-04-v1.34-fl-e-preset-chain` (P2), `2026-06-04-v1.34-agent-tool-implementation` (P4), `2026-06-04-v1.34-spec-tracker-hygiene` (P5) |
| Closed DF items | DF-51 (creator.inject_prompt schema, P0), DF-54 (Work stage persistence, P1) |
| Key changes | Work `stage`/`stage_status` columns + DDL migration V9→V10 (P1); `creator run stage list|advance --stage <id> [--force]` CLI (P1); shared `check_stage_advance` gates (CLI + daemon PATCH); active FL-E schedule uniqueness invariant; 11 `fl_e_chain_demo` e2e + 5 `fl_e_schedule_api` hermetic; preset chain (research → novel-writing → reflection-loop → kb-extract / memory-review); agent-nexus-tool-bridge.md 504 lines Shipped; 8 tools in registry with 5-step admission pipeline; 26 `agent_tool_api` hermetic tests; error codes (POLICY_BLOCKED, FORBIDDEN, NOT_SUPPORTED, INVALID_INPUT) surface in HTTP + worker replies; audit log on every invocation; V1.33 residuals closed (4 of 7 v1.33-p1 + 2 v1.32 R-P2) |
| Open residuals at close | R-FL-E-DDL/DEAD/LIST/FNAME/ENDP (5, P1 qc3 + 4 deferred V1.34+); R-P2-W2/W3/S1/S2 (4, P2 qc3 deferred V1.34+); DF-47 (production caller wiring, P4 partial); TD-V130-* (11), TD-V131-* (8), R-V133P1-03/-08/-09 (3), R-V133P3-04 (1), R-V133P4-04 (1) — total 39 in `residual_findings` |
| Explicit deferrals | DF-29, DF-31, DF-46, DF-47 (still OPEN), DF-48, DF-49, DF-50, DF-52, DF-53 (`--auto-chain`), DF-55, DF-56 (conditional routing) |
| Platform integration | Paused (PD-05) — `nexus.context.assemble` returns local slice or `policy_blocked` |

### V1.16+ horizon (program)

### Items targeting V1.19 (superseded by V1.28 for Batch 1)

| Category | Count | IDs |
|----------|-------|-----|
| Features (Batch 1 — safety/correctness) | 6 | DF-18, DF-19, DF-20, DF-24, DF-25, DF-26 — **scheduled V1.28** (was V1.19) |
| Features (Batch 2 — hardening) | 5 | DF-21, DF-22, DF-23, DF-27, DF-28 — **target V1.29** (locked in compass) |
| **Total** | **11** | Original V1.18 §9 backlog; Batch 1 absorbed into V1.28 compass |

### Items targeting V2.0+

| Category | Count | IDs |
|----------|-------|-----|
| Features | 2 | DF-13 (Entitlements), DF-16 (Billing) |
| Tech-debt residuals | 0 | No V2.0-targeted rows in root `residual_findings` (2026-05-11); historical R5/R11/M1-W07 remain **knowledge / compass** follow-ups until re-filed |
| Backlog | 1 | BL-08 (Social/marketing) |
| **Total** | **3** | |

### Open backlog (no committed target)

| Category | Count | IDs |
|----------|-------|-----|
| Features | 1 | DF-03 (Preset registry/publish) |
| Backlog features | 8 | BL-01 through BL-08 |
| Tech-debt (open) | 20 | See active tracker §3.5 → `status.json` `residual_findings` |
| **Total** | **29** | |

### Cancelled / Superseded (V1.7 planning, 2026-04-21)

| ID | Status | Reason |
|----|--------|--------|
| DF-02 | Cancelled | Over-designed; OSS contributions sufficient |
| DF-05 | Cancelled | ACP permission not core product value |
| DF-06 | Superseded | Snapshot semantics correct; real need → DF-17 |
| DF-07 | Cancelled | Over-designed; built-in capabilities don't need platform registration |

### Decision log (V1.7 planning, 2026-04-21)

| ID | Decision | Rationale |
|----|----------|-----------|
| DF-02 | **Cancelled** | Over-designed; users can contribute capabilities via OSS code contributions |
| DF-03 | **→ Backlog** (independent project) | Too large for a nexus version feature; potentially standalone |
| DF-05 | **Cancelled** | ACP permission is not core value — ACP Session is an orchestration tool, not a product focus |
| DF-06 | **Superseded** | Snapshot semantics are correct; real need is DF-17 (`~/.nexus42/presets/` loading + CLI init) |
| DF-07 | **Cancelled** | Over-designed; built-in capabilities don't need platform registration |
| DF-08 | Keep (worth doing) | Schema drift detection |
| DF-09 | Keep (prioritize) | Must work with `~/.nexus42/presets/` (DF-17) |
| DF-10 | Keep (worth doing) | Daemon lifecycle hardening |
| DF-11 | Keep (worth doing) | Handlebars binding for CoreContext |

---

## 2) V1.x delivery snapshots

### V1.36 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.36-novel-writing-ux-delivery-compass-v1.md](../iterations/v1.36-novel-writing-ux-delivery-compass-v1.md) |
| Shipped at | 2026-06-07 |
| Scope | **Novel-writing正文产出 UX** on generic Work (`work_profile: novel`); `Works/<work_ref>/` layout; `novel-project-init` grill-me init preset; `novel-writing` chapter pipeline (outline → draft → finalize with `llm_judge` 五问 quality gate); completion stop; pre-1.0 full migration, no legacy `Stories/<story_ref>/` shims |
| Plans | `2026-06-07-v1.36-harness-docs-prepare` (Prepare, P-1), `2026-06-07-v1.36-novel-spec-and-compass` (P0), `2026-06-07-v1.36-novel-project-init-preset` (P1), `2026-06-07-v1.36-novel-artifact-layout-and-templates` (P2), `2026-06-07-v1.36-novel-chapter-drafting-pipeline` (P3), `2026-06-07-v1.36-novel-completion-and-chain-hygiene` (P4) |
| Key changes | `novel-workflow-profile.md` Draft overlay Shipped (V1.36): `work_profile: novel` + `work_ref` extension; `work_chapters` DB SSOT (replaces `work-status.md`); `Works/<work_ref>/` layout (README + Outlines/ + Stories/ + Logs/); per-Work `Worldbuilding/` subtree removed (cross-Work worldbuilding lives in World KB); preset gates mechanism in `orchestration-engine.md §7.9` Master + novel-specific gates in `novel-workflow-profile §5.3` Draft overlay + `world_binding: required \| optional` toggle + scaffold protocol enumeration in §5.4; `novel-project-init` preset (10 prompts incl. World binding question + 4 templates + `novel.project_scaffold` capability with atomic FS+DB transaction + sanitization + FK checks); `sync_module` rewritten for `Works/<work_ref>/Stories/` scan + DB-enriched bundle; `creator run reconcile-chapters <work_id>` CLI + daemon endpoint; `novel-writing` 4-state chapter-scoped graph with `llm_judge` 五问 quality gate on `finalize` (`opening three lines / conflict resonance / twist recall / new perspective / ending hook`); `is_work_completed` evaluator + completion banner in `creator run status` + schedule guard rejecting `novel-writing` on completed Work; P1-P4 used PM-validate path (analogous to V1.35 P4) under time pressure (no QC tri-review for P2/P3; P1 had QC tri-review with PM-override w/ residuals) |
| Closed DFs | DF-57 (V1.36 P2), DF-58 V1.36 (V1.36 P1) |
| Open residuals at close | R-V136P1-01, R-V136P1-02, R-V136P2-01, R-V136P2-02, R-V136P2-03, R-V136P3-01, R-V136P3-02 — 7 new V1.36 residuals (all medium-or-low severity); DF-47 stays conditional; DF-53 partial again on top of V1.35 P4; DF-59 stays backlog |
| Explicit deferrals | DF-29, DF-31, DF-47, DF-53, DF-56, DF-59, DF-60..DF-67 (novels-system pattern backlog for V1.37+) |


---

---

### V1.39 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) |
| Shipped at | 2026-06-09 (PR #50 merged ad9725d8) |
| Scope | **DF-53 full FL-E auto-chain (default true) + DF-68 daemon continuation checkpoint + DF-64/65/66/67 quality-loop full implement + DF-62 (V1.38) first-slice extension + DF-40 (V1.21) session-resume convergence**: P0 auto-chain engine (15 unit + 14 integration tests); P0.5 research-stage in chain (1 Critical + 2 AC Warnings closed in fix wave); P1 findings + routing (qc3 caught 1 Critical spec violation: missing `(work_id, chapter, status)` index per `novel-quality-loop.md` §2.1, PM closed it); P2 `novel-brainstorm` + `novel-review-master` presets; P3 three-layer rules + Logs/; P4 96h finding escalation banner + daemon scheduled task; P5 V1.38 residual hardening (7 V1.38 residuals triaged: 2 fix, 5 accept-with-doc). |
| Plans | `2026-06-09-v1.39-harness-docs-prepare` (P-1), `2026-06-09-v1.39-fl-e-auto-chain-engine` (P0), `2026-06-09-v1.39-research-stage-wiring` (P0.5), `2026-06-09-v1.39-findings-and-review-routing` (P1), `2026-06-09-v1.39-novel-review-presets` (P2), `2026-06-09-v1.39-rules-and-logs` (P3), `2026-06-09-v1.39-master-decision-timeout` (P4), `2026-06-09-v1.39-v138-hardening` (P5) — all 8 plans Done on `iteration/v1.39`; P0..P5 ran in parallel where independent (P0.5 + P5). Stats: 88 commits + 10826 / -285 lines, all 8 CI checks green at PR #50. |
| Key changes | **P0** (auto-chain engine): works table extended with `auto_chain_enabled`/`auto_chain_interrupted`/`driver_schedule_id` (migration 202606090001); pure `auto_chain` module with `evaluate_next_step(work) -> ChainAction` (15 unit tests) + DB helpers; `ScheduleSupervisor::on_schedule_terminal` hook → `process_auto_chain_after_terminal` → shared `enqueue_auto_chain_schedule` helper (W-A dedupe); boot recovery via `find_resumable_works` (W-E partial index `works_auto_chain_resume`); side-input 409 invariant; `--auto-chain`/`--no-auto-chain` flags; `creator run resume`; patch_work_stage atomicity (W-D reorder); 21 hermetic integration tests. **P0.5** (research stage): research preset v1→v2 with `run_intents: knowledge_ingest`, gates `intake_status==complete + work_ref required`; `exit_when: kind llm_judge` (auto-chain compatible; W-1 fix from manual); `research_artifacts_dir` in produce stage input (W-2 fix); gate conditional on work_id (C-1 fix); 14+3 research tests. **P1** (findings + routing): `findings` migration 202606090002 (severity/status/target_executor TEXT enums); DAO with `create_finding`/`list_findings`/`update_finding`; `from-review` endpoint + `ReviewVerdictFinding` hook; CLI status Findings section with routing hints (→ write/brainstorm/none/master); 7 hermetic API tests + PM C-1 fix (added spec-required composite index `(work_id, chapter, status)`). **P2** (review presets): `novel-brainstorm` + `novel-review-master` embedded presets (preset+prompts); 4 validation tests + 8 e2e smoke tests; CLI hints documented. **P3** (rules + logs): embedded Layer 1 `writing-craft.md`; Layer 2 scaffold `Works/<work_ref>/Rules/novel-rules.md`; Layer 3 atomic history writer; `read_rules_layers()` reads L1+L2; `Logs/{brainstorm,write,review,publish}/` subdirs scaffolded; sync exclusion in `sync_module.rs`; 8 hermetic tests. **P4** (master-decision timeout): stale-findings DAO; 24h-interval daemon watcher (env-var override); CLI status banner `⏰ N findings stale (>96h)`; per-Work `auto_review_master_on_timeout` opt-in (default false); RVM-prefixed review-master schedule helper; 7 hermetic tests. **P5** (V1.38 hardening): closed R-V138P0-05 (NULL/0 tests) + R-V138P1-01 (completion guard); accepted R-V138P0-01/02/03/04 + R-V138P1-04 with doc/rationale; registered 3 new low-severity follow-ups (N1/N2/N3). |
| QC & QA | **P0**: initial tri-review all Approve; consolidated gate Request Changes (3 medium Warnings: W-A dedupe enqueue, W-D non-atomic PATCH, W-E missing index) → fix wave (5 commits) closed all 3 → targeted re-review (qc1 W-A, qc2 W-A, qc3 W-D+W-E) all Approve → final Approve. **P0.5**: qc1+qc2 Approve, qc3 Request Changes (1 Critical C-1: 4 daemon-runtime tests fail because gates reject schedules without Work) → fix wave (3 commits) closed C-1 + 2 AC Warnings (W-1 manual exit, W-2 artifacts in produce input) → final Approve. **P1**: qc1+qc2 Approve, qc3 Request Changes (1 Critical C-1: missing `(work_id, chapter, status)` composite index per `novel-quality-loop.md` §2.1) → PM fix wave (1 commit + 1 test) closed C-1 → final Approve. **P2, P3, P4**: PM-validated (narrow scope, clean process, all evidence independently verified). **P5**: all 3 Approve. **PR #50 cursor security review (medium)**: P0.5 C-1 fix introduced a preset-gate authorization bypass; fix branch `fix/v1.39-preset-gate-bypass` (commit 3cc1601f) closed it before PR merge. All CI gates clean (cargo clippy --all -- -D warnings; cargo test --all green). |
| Closed residuals at close | **R-V139P0-SecFix** (medium, follow-up security fix from PR #50 review) — closed in `fix/v1.39-preset-gate-bypass` commit 3cc1601f, merged via 8d9405a9, archived to `.mstar/archived/residuals/2026-06-09-v1.39-research-stage-wiring.json`. **V1.38 residuals**: R-V138P0-05 (NULL/0 tests), R-V138P1-01 (completion guard `reject_produce_when_novel_complete`) — closed in P5. |
| Open residuals at close | 22 V1.39 residuals registered: 3 medium (R-V139P0-W-1 / R-V139P1-W-1 / R-V139P0-SecFix resolved + R-V139P0-SecFix registered as resolved per PR #50 review) + 19 low. Combined tech-debt summary at V1.39 ship: 66 open (1 medium + 39 low + 12 nit + 14 from pre-V1.39 plans). v1.39 = 23 in by_target. Most are V1.40 hygiene (W-B ID entropy, W-C resume timing, W-F tick scan, W-3..5 preset validation + status format + i18n, N1/N2/N3 follow-ups, etc.). |
| Explicit deferrals (open) | **DF-63** (World KB implementation — remains out per V1.39 scope; V1.40+ candidate), **multi-volume PK migration** `(work_id, chapter)` → `(work_id, volume, chapter)` (remains out per V1.39 scope; V1.40+ candidate). All other V1.36+ deferred items targeted by V1.39 are now Shipped (DF-53, DF-62 extension, DF-64, DF-65, DF-66, DF-67, DF-68, DF-40 convergence). |

---

*Append-only archive. Do not delete historical rows.*

### V1.37 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.37-novel-writing-foundation-delivery-compass-v1.md](../iterations/v1.37-novel-writing-foundation-delivery-compass-v1.md) |
| Shipped at | 2026-06-08 |
| Scope | **Novel Writing UX foundation-first**: harden the V1.36 single-chapter proof before expanding. P0 implemented runtime gate evaluation + `AddScheduleRequest.input` plumbing + scaffold atomicity + `--force-gates` audit. P1/P2/P3 produced spec/roadmap amendments (not implementation) for multi-chapter chronology, World KB continuity, and quality-loop backplane. |
| Plans | `2026-06-07-v1.37-harness-docs-prepare` (P-1), `2026-06-07-v1.37-novel-foundation-first` (P0), `2026-06-07-v1.37-novel-multi-chapter-chronology` (P1), `2026-06-07-v1.37-novel-world-kb-roadmap` (P2), `2026-06-07-v1.37-novel-quality-loop-roadmap` (P3) |
| Key changes | P0: `preset_gates.rs` (work_field \| filesystem \| previous_preset) per `orchestration-engine.md §7.9`; `AddScheduleRequest.input: HashMap` wired from `creator run start --init-preset` grill-me to daemon → `preset.input.*`; daemon handler routes input into `PresetInput.vars` + seeds; `force_gates_audit` table (append-only) with `creator_id, forced_at` index; `creator_schedules.work_id` column + composite index; `novel_scaffold` `seed_chapters` + `patch_work` wrapped in single DB transaction; `embedded-presets/novel-writing/preset.yaml` gates moved under `preset:` key with full §5.3.2 gate set; `patch_work_tx` returns `Result<bool>` (no dirty-write); `--force-gates` / `--gate-reason` CLI flags with 512-char cap + ANSI/control char filter; reserved input keys policy; 23 files / +1921 lines / -126 in impl + 12 files / +727 / -262 in fix + 8 `.sqlx` regen. P1: `novel-workflow-profile.md` extended with multi-chapter / multi-volume semantics — `next_chapter(work_id)` algorithm, `current_chapter` update rules, PK migration decision (defer to V1.37+), volume semantics + `Outlines/volume-outline.md` minimum structure, status UX example. P2: `entity-scope-model.md §5.1.1` extended with narrative World KB item taxonomy (foundation, background, character, location, society, rules, economy) + minimum-viable schemas; `novel-workflow-profile.md §3.5.1` extended with `world_id` validation contract, prompt-time World context block format (YAML/JSON), `world_refs` validation rules, Chapter → World KB extraction path via `kb-extract` / `persist` stage. P3: `novel-workflow-profile.md §5.5` extended with quality-loop roadmap — findings lifecycle + severity mapping + future local DB schema sketch (DF-64); executor mapping (write → novel-writing, brainstorm → future novel-brainstorm, none → manual, master → future novel-review-master); 96h master-decision timeout mapped to local DB + daemon scheduled lifecycle task + `creator run status` banner (DF-67); three-layer rules architecture (shared craft / per-work / append-only history) with SOUL/World KB boundaries (DF-65); `Logs/{brainstorm,write,review,publish}/` roadmap structure with `Logs/**` sync exclusion reaffirmed (DF-66) |
| QC & QA | P0: QC1+QC2+QC3 tri-review (initial Request Changes; targeted re-review #1 after fix wave; targeted re-review #2 after F-002 fix — all 3 finally Approve) + `qa-engineer` Approve (6/6 ACs, 981 tests pass, all CI gates clean). P1/P2/P3: single `qc-specialist` review each (docs-only per PM rules) — all 3 Approve. |
| Closed residuals at close | **R-V136P1-01** (V1.37 P0 — `AddScheduleRequest.input` wired), **R-V136P1-02** (V1.37 P0 — gate evaluator with work_field/filesystem/previous_preset strategies), **R-V136P3-02** (V1.37 P0 — scaffold atomicity via DB transaction) — 3 medium-or-low severity residuals from V1.36 closed in P0 |
| Open residuals at close | R-V137P0-01 (low — serde strict-mode for misplaced YAML keys, opened during P0 fix wave when `gates:` was found at YAML top-level instead of under `preset:`) |
| Explicit deferrals (open) | DF-53 (auto-chain — partial again), DF-47 (HostToolExecutor production caller — conditional), DF-56 (conditional routing — out), DF-59 (platform publish — backlog), **DF-62** (multi-chapter chronology implementation — V1.37 P1 roadmap-only), **DF-63** (World KB continuity implementation — V1.37 P2 roadmap-only), **DF-64** (findings lifecycle implementation — V1.37 P3 roadmap-only), **DF-65** (three-layer rules implementation — V1.37 P3 roadmap-only), **DF-66** (Logs/ subdirectory implementation — V1.37 P3 roadmap-only), **DF-67** (master-decision timeout implementation — V1.37 P3 roadmap-only), DF-60/61 (auto-switch / selection pool — backlog) |

---

### V1.38 delivery snapshot (Shipped)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.38-multi-chapter-serial-writing-delivery-compass-v1.md](../iterations/v1.38-multi-chapter-serial-writing-delivery-compass-v1.md) |
| Shipped at | 2026-06-09 |
| Scope | **DF-62 multi-chapter / serial writing first implementation slice**: P0 chapter selection/status foundation + P1 `novel-writing` selected-chapter parameterization. V1.38 turns the V1.37 multi-chapter roadmap into executable code. No auto-chain, no World KB, no quality loop, no multi-volume PK migration, no platform publish, no multi-work switch, no selection pool. |
| Plans | `2026-06-08-v1.38-harness-docs-prepare` (P-1), `2026-06-08-v1.38-multi-chapter-selection-status` (P0), `2026-06-08-v1.38-novel-writing-parameterization` (P1) — all Done |
| Key changes | **P0 (selection/status foundation)**: `next_chapter(work_id)` in `nexus-local-db/src/work_chapters.rs` as single `SELECT MIN(chapter) FROM work_chapters WHERE work_id = ? AND status IN ('not_started', 'outlined', 'draft')` — preserves chapter order, resumes earlier draft/outlined, advances only when no earlier active row; `is_work_completed()` now requires novel-profile-aware check (`intake_status == 'complete' AND current_chapter >= total_planned_chapters AND all rows finalized AND row count match`) with non-novel-profile legacy early-exit preserved; `novel_chapter_transition.rs` finalize-only `current_chapter` advance (draft branch removed); `WorkApiDto` enriched with `chapters[]` + `next_chapter` for novel-profile Works via `enrich_with_chapters`; `creator run status <work_id>` rewritten with per-chapter rows, progress count, current/total, next-action hint (non-enqueuing); `novel-writing` preset version4→5, all `chapter: 1` literals replaced with `{{preset.input.chapter}}` template variable; `stage_gates.rs::WorkFields.chapter` field + `build_preset_input()` extension; migration `202606080003_work_chapters_composite_index.sql` adds `(work_id, status, chapter)` index. **P1 (selected-chapter parameterization)**: `WorkFields` extended with `chapter_label` (zero-padded) / `outline_path` / `body_path` / `slug` optional fields; `build_preset_input()` serializes them; `novel-writing` preset version5→6; `outline-chapter.md` and `draft-chapter.md` templates parameterized with `{{outline_path}}` / `{{body_path}}` / `{{chapter_label}}` / `{{slug}}`; `ch0{{chapter}}` path literals removed; CLI `stage_advance` extracts chapter context from `WorkApiDto.chapters[]` and populates `WorkFields`; `validate_produce_chapter_context()` fail-fast at CLI boundary when chapter context absent (actionable error with remediation hint); `pub fn chapter_label()` helper extracted in `stage_gates.rs` (single source of truth); `draft-body.md` + `draft-intro.md` ch01-era prompt artifacts moved to `prompts/_deprecated/`. Tests: 19 work_chapters hermetic + 38 stage_gates (incl. chapter_label + 4 validation) + 28 works_api + 11 e2e_novel_writing + 11 fl_e_chain_demo + 749 nexus42 = ~1100 tests. 24 files / +1100 / -200 across P0+P1. |
| QC & QA | **P0**: QC1+QC2+QC3 tri-review (initial: QC1 Request Changes 1 Critical F-001 + 1 Warning F-002; QC2 Request Changes 2 Warnings; QC3 Request Changes 2 Warnings; 1 QC2 latent Warning deferred to residual) → fix wave (`f5c8ecc4` F-001 next_chapter lowest-active-chapter + `2dc2c892` W-1 composite index) → targeted re-review by qc-specialist Approve (all3 findings resolved) → Plan Done. **P1**: QC1+QC2+QC3 tri-review (initial: QC1 Request Changes 2 Warnings W-1 silent degradation + W-2 label duplication; QC2 Approve 1 latent W + 4 S; QC3 Approve 4 S) → fix wave (`612b81d9` W-1 fail-fast + `ba912fe1` W-2 chapter_label helper) → targeted re-review by qc-specialist Approve (both findings resolved) → Plan Done. **P-1**: single docs-only qc-specialist review Approve. All CI gates clean (clippy +nightly -D warnings on 4 crates). |
| Closed residuals at close | none (no V1.36/V1.37 residuals addressed in this iteration) |
| Open residuals at close | 12 new P0+P1 residuals: **R-V138P0-01** (medium) `next_chapter` selection race window under concurrent `creator run continue`; **R-V138P0-02** (low) T9 missing-file hint emission in CLI status partial; **R-V138P0-03** (medium) write-on-read anti-pattern in `GET /v1/local/works/{id}` lazy completion promotion; **R-V138P0-04** (low) `WorkApiDto.chapters` vector uncapped; **R-V138P0-05** (nit) `is_work_completed` total_planned_chapters=NULL explicit test missing; **R-V138P1-01** (low) pre-existing latent `next_chapter=None` completion UX; **R-V138P1-02** (nit) frontmatter field doc removed; **R-V138P1-03** (low) `_deprecated/` files still embedded; **R-V138P1-04** (low) `outline_path`/`body_path` `required: true` with no defaults; **R-V138P1-05** (nit) `chapter_label` no fixed-width beyond 2 digits; **R-V138P1-06** (nit) O(n) chapters scan; **R-V138P1-07** (low) `stage_advance` lacks audit logging |
| Explicit deferrals (open) | DF-53 (auto-chain — V1.38 explicitly OUT, no auto-reenqueue; next action shown but never enqueued), DF-63 (World KB implementation — remains out), DF-64/65/66/67 (quality loop implementation — remains out), DF-47/56 (conditional — out unless reopened), DF-59 (platform publish — backlog), DF-60/61 (auto-switch / selection pool — backlog), **multi-volume PK migration** (V1.38 keeps `(work_id, chapter)` PK; deferred to a future plan that explicitly reopens) |

---

*Append-only archive. Do not delete historical rows.*

---

### V1.40 delivery snapshot (Shipped — PR #52)

| Category | Position |
|----------|----------|
| Delivery SSOT | [v1.40-novel-world-kb-delivery-compass-v1.md](../iterations/v1.40-novel-world-kb-delivery-compass-v1.md) |
| Shipped at | 2026-06-11 |
| PR | https://github.com/42ch-dev/nexus/pull/52 |
| Scope | **DF-63 World KB cross-Work unification** (W1–W5) + **P4 V1.39 residual convergence** (17 V1.40-tagged items). V1.40 turns the V1.37 World KB roadmap into product-complete code: every new Work must bind a `world_id` (mandatory), the World KB taxonomy (`BlockType` + `novel_category` + `canonical_name` grammar) is enforced at production paths, prompt-time World context block ships via `{{ world_kb_block }}` template var, and chapter-finalize → World KB extraction is wired end-to-end. |
| Plans | `2026-06-10-v1.40-harness-docs-prepare` (P-1, Done earlier), `2026-06-10-v1.40-architecture-hygiene` (P0.5), `2026-06-10-v1.40-world-create-and-validation` (P0), `2026-06-10-v1.40-world-kb-taxonomy` (P1), `2026-06-10-v1.40-world-context-prompt-block` (P2), `2026-06-10-v1.40-world-kb-extract-binding` (P3), `2026-06-10-v1.40-hygiene` (P4) — all Done |
| Key changes | **P0.5 (architecture hygiene)**: `writing-craft.md` moved from `embedded-presets/rules/` to `embedded-rules/` (spec-compliant path); `world-kb-runtime-architecture.md` knowledge doc shipped (grill-me locked layering). **P0 (world create + validation, MANDATORY binding post user clarification 2026-06-10)**: `creator world create --title\|--name` + `show` + `list` + `event-add`; scaffold `create_world_tx(&mut Transaction)` atomic with chapter seeding; `works.rs` POST validates `world_id` existence + ownership (`owner_creator_id`) with 422 `preset_gates_failed`; PATCH rejects `world_id` clear on bound Works (`WORLD_CLEAR_FORBIDDEN`); adversarial `world_id` matrix (7 inputs); legacy V1.39 worldless read-only compat preserved. **P1 (taxonomy)**: `nexus-kb::validation` module (`validate_body` + `validate_canonical_name`); wire `BlockType` (8 values) reused from `nexus_contracts`; `body.attributes.novel_category` enforced per `entity-scope-model.md` §5.1.1; structured `ValidationError { kind: ValidationKind, field, message }` with 7-variant enum; `canonical_name` grammar rejects control chars / path seps / shell metas / >256 chars; `SqliteKbStore::insert_key_block` + `update_key_block` wired with `ValidationMode` (production-path enforcement); advisory `novel_category → block_type` emits `tracing::warn!`; `kb-extract/prompts/extract.md` updated. **P2 (prompt block)**: `nexus-moment-context-assembly::world_context.rs` (728 lines); `WorldKbQueryBuilder` + `build_chapter_kb_block` refactored from `fetch_world_kb` (no inline query in orchestration per grill-me #12); `{{ world_kb_block }}` template var in `outline-chapter.md` + `draft-chapter.md` with `{{#if}}` guard; thread-through via `WorkFields.world_id` + `build_preset_input`; token budget (~1500) enforced with truncation marker; YAML output deterministic (sorted); legacy V1.39 worldless Works get empty block (guard omits). **P3 (extract binding)**: `kb_extract_jobs` schema migration (additive: `source_kind`, `source_locator`, `profile_hint`, `work_id`); `nexus-kb::extract_finalize` (P1 validation, KeyBlock upsert, SourceAnchor); `kb.extract_work` capability extended (name preserved per grill-me #13); `creator kb queue-extract --chapter N` sugar (N >= 1, real body_path resolution); `novel-review-master sync_world_kb` (worldless skip + ownership re-check + `mark_done` AFTER `finalize_extract` + `mark_failed` on insert error); DF-63 W5 Shipped. **P4 (hygiene)**: 9 V1.40-tagged V1.39 residuals resolved (auto-chain ULID / resume tick / scoped `tick_inner` / preset_version from manifest / findings enum validation / ID mint SSOT / CLI HTTP timeout / EXPLAIN audit / from-review hook tracing); 5 waived with documented rationale (UX N1-N3, W-5, S3); 1 PM-accepted waiver (R-V140P4-W2 — sqlx::query_as! design tradeoff with SAFETY comments); 3 out of scope. |
| QC & QA | **P0.5**: QC1+QC2+QC3 (all Approve initial; QC3 targeted re-review after nightly fmt fix → all Approve) → QA Pass → Done. **P0**: spec amendment (`464d0fba`) shifted World binding to mandatory per user clarification; implementation adapted; QC1+QC2+QC3 initial Request Changes (8 blocking findings: SqliteKbStore unprotected, advisory dead code, Debug format, canonical_name format, opaque errors, PATCH clear, atomicity, ownership FK); fix `d3a18d14`; re-validation Approve all → QA Pass → Done. **P1**: QC3 Approve initial; QC1+QC2 Request Changes (SqliteKbStore unprotected + advisory dead code + structured errors); fix `fbd301c4`; re-validation Approve all → QA Pass → Done. **P2**: implementer stalled without committing; committed via follow-up dispatch; QC1+QC2+QC3 Request Changes (preset.input.world_kb_block never populated → strict-mode template failures; runtime_compatibility compile gate; chapter_text heuristic missing); fix 3 commits; re-validation Approve all → QA Pass → Done. **P3**: QC3 Approve initial; QC1+QC2 Request Changes (7 blocking findings: dead code, worldless guard, runtime sqlx, chapter validation, ownership check, mark_done order, magic 'auto'); fix 5 commits; re-validation Approve all → QA Pass → Done. **P4**: QC1+QC2+QC3 Request Changes (5 critical findings: tick_inner dependency bug, PatchWorkRequest compile failures, unsupported ALTER TABLE ADD CONSTRAINT, ID mint SSOT, unused import); fix 4 commits; QC1+QC2 re-validation Approve; QC3 re-validation Request Changes (W-2 only) → PM-accepted (sqlx::query_as! design tradeoff) → QA deferred pending `.sqlx/` cache refresh (pre-existing infra) → Done. |
| Closed residuals at close | 9 V1.40-tagged V1.39 residuals (W-B, W-C, W-F, S4, W-1, W-2, W-3, W-4, W-6); **DF-63 World KB** (5 slices W1–W5 all Shipped V1.40 P0–P3 → row closed) |
| Open residuals at close | **R-V140P4-W2** (medium, PM-accepted) — runtime `sqlx::query_as::<T>` in `supervisor.rs` (custom `FromRow` struct); same pattern as `nexus-local-db/src/kb_store.rs:list_by_creator`; SAFETY comments present; restore compile-time macros via `cargo sqlx prepare` in V1.41. **R-V140P4-INFRA** (low) — `.sqlx/` offline cache stale; full `cargo test` for `nexus-orchestration` + `nexus-local-db` requires `cargo sqlx prepare --workspace --all` with live DB. Suggestions (low/info) deferred to V1.41: R-V140P0-S1..S4, R-V140P1-S1..S6, R-V140P2-S1..S4, R-V140P3-S1..S5, R-V140P0.5-S1..S3. |
| Explicit deferrals (open) | Multi-volume PK (V1.40 explicitly OUT; same status as V1.39), DF-60/61 (auto-switch / selection pool — backlog), DF-59 (platform publish — paused), DF-56 (conditional routing — backlog), DF-47 (production caller wiring — backlog) |

### V1.45 delivery snapshot (Shipped 2026-06-14)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#58](https://github.com/42ch-dev/nexus/pull/58) MERGED at 9514bfdc (2026-06-14T13:54:43+08:00); CI 8/8 PASS (Rust fmt+clippy · Rust tests · Schema Consistency · TypeScript typecheck · Validate JSON Schemas · Verify Codegen · Verify daemon-runtime linkage · Verify sqlx offline metadata) |
|---|---|
| **Theme** | **Creator Run Preset Unification** — CLI IA correction: `creator run <preset_id>` as the sole preset dispatch entry; delete hardcoded `RunCommand` subcommands; **`creator bootstrap`** for composite Work onboarding; **atomic `creator works`** for non-preset ops; quickstart/spec convergence; (P4 author-desk deferred). |
| **Compass** | [v1.45-creator-run-preset-unification-delivery-compass-v1.md](../iterations/v1.45-creator-run-preset-unification-delivery-compass-v1.md) — 20 grill-me decisions locked; atomic merge required (Grill #19). |
| **Active spec** | [creator-run-preset-entry.md](../knowledge/specs/creator-run-preset-entry.md) promoted Draft → **Shipped Master V1.45** (P-last T1). |
| **Plans shipped** | 4 plans (P-1 harness prepare + P0 generic runner + P1 delete bespoke subcommands + P2 `creator bootstrap` + P3 quickstart+author spec) — all Done; Profile B compacted. P4 (author desk) optional; OUT of V1.45 scope. |
| **Closed at ship** | **BL-12** (generic `creator run <preset_id>` — V1.45 P0+P1+P2); **DF-52** (top-level `nexus42 preset` group — resolved by BL-12); **BL-13** (`STAGE_PRESET_ALLOWLIST` `memory-review` drift — P1 T4 removed). |
| **6 V1.45 Draft overlays** | Replaced with `Superseded by: [creator-run-preset-entry.md]` stub in: `creator-workflow.md` (FL-E CLI), `novel-quality-loop.md` (preset-id commands; body applied P3), `novel-manuscript-audit.md` (CLI entry + split presets), `work-experience-model.md` (side-input + run_intents), `orchestration-engine.md` (`run_intents` dispatch), `cli-spec.md` (`creator run` preset entry). |
| **Hard delete** | `RunCommand` variants: `Start`, `Continue`, `Stage`, `Resume`, `ReconcileChapters`, `AuditChapter`, `ReviewMaster`. `embedded-presets/novel-manuscript-audit/` (DEPRECATED parent dir) — split into `-review` and `-extract`. No deprecation aliases (pre-release; compass §0.1 #9). |
| **Three-plane IA shipped** | `creator bootstrap` (composite) · `creator works <sub>` (atomic) · `creator run <preset_id>` (strategy). Grill #10/#11: `creator works start` / `creator works create` rejected. |
| **Open residuals at ship** | 7 V1.45 B1 (QC1.S-1/2/3 + QC3.S-1/2/3/4, deferred Suggestions, severity: low) · 2 V1.45 B2 (broader spec-tree migration gaps + cross-link re-check, severity: low) · 1 V1.45 B3 (`R-V145B3-001`: cli-spec.md §6.2D/E body not yet rewritten to match new Master — out of plan scope, severity: low) |
| **Open deferrals (carry forward)** | Same as V1.44: DF-29, DF-31, DF-42, DF-44, DF-46, DF-48, DF-49, DF-50, DF-55, DF-59, DF-60/61 (V1.45 OUT as V1.44). No new deferrals registered in V1.45. |
| **QC & QA** | **P0+P1+P2 atomic merge** (Grill #19): QC1 Request Changes (C-1 + 3W + 3S) + QC2 Approve + QC3 Approve (2W + 4S) + QA PASS → targeted re-review fix round (1 dev, 6 commits) → QC1 revalidation **Approve** → consolidated **Approve**. **P3 quickstart+spec**: QC1 Request Changes (2W + 1S) + QC2 Approve + QC3 Request Changes (2W) + QA PASS → targeted re-review fix round (1 dev, 5 commits + 1 cross-ref hint) → QC1+QC3 revalidation **Approve** → consolidated **Approve**. **P-last hygiene**: PM-only closeout, no QC required (PM signoff per plan §5 T6). |
