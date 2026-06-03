# Shipped Features — Cross-Version Archive

**Status**: Archived (append-only)  
**Purpose**: Long-term **append-only** archive for closed deferred-feature tracker rows (shipped, cancelled, superseded) and per-version delivery snapshots. The **active** open/backlog tracker lives in [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md).  
**Scope**: `nexus` OSS repository only.  
**Location**: Top-level harness archive (`.mstar/archived/`) — not under `archived/knowledge/` (implementation knowledge supersession).  
**Split from**: [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md) §4–§5 (2026-05-30 restructure)  
**Created**: 2026-05-30  
**Last updated**: 2026-06-03

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

*Append-only archive. Do not delete historical rows.*
