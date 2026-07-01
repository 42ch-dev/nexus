# Shipped Features — Cross-Version Archive

**Status**: Archived (append-only)  
**Purpose**: Long-term **append-only** archive for closed deferred-feature tracker rows (shipped, cancelled, superseded) and per-version delivery snapshots. The **active** open/backlog tracker lives in [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md).  
**Scope**: `nexus` OSS repository only.  
**Location**: Top-level harness archive (`.mstar/archived/`) — not under `archived/knowledge/` (implementation knowledge supersession).  
**Split from**: [deferred-features-cross-version-tracker.md](../knowledge/deferred-features-cross-version-tracker.md) §4–§5 (2026-05-30 restructure)  
**Created**: 2026-05-30  
**Last updated**: 2026-06-22 (V1.57 closeout: 7 plans all Done — P-1 prepare (compass + 7 plan stubs + status.json + tracker activation) + P0 Spec Governance (bridge→Master draft + acp §4 roster rewrite 41 rows + capability::Registry consolidation + R-V156P3-S003 field drops re-introduction) + P1 Daemon Refactor (host_tool_executor.rs 4298→349 lines + 3 caller entry points CLI/worker/HTTP all dispatching through `capability::Registry::dispatch` + `nexus42 host-call` debug-only subcommand + CdnConfig constructor-injection closing R-V156P1-M002 + 4 spec amendments Draft overlays cli-spec.md §6.2M + daemon-runtime.md + local-runtime-boundary.md + orchestration-engine.md §6.4) + P2 V1.56 Carry-Forwards (R-V156P1-M001 schema rename `agent_count`→`capability_count` with backward-compat serde aliases + 5 reproducer tests in `schema_rename_compliance.rs`) + P3 Worker IPC (dynamic allowlist 1→18 IDs derived from `CapabilityRegistry::lookup()` + 54-case cross-caller E2E in `cross_caller_e2e.rs` covering all 18 IDs × 3 caller paths + profile-set non-registration verified + orchestration-engine.md §6.4 + daemon-runtime.md updates) + P-mid meta tracking (3-wave QC rhythm + 12 QC reports + 3 targeted re-reviews + 1 mid-QA) + P-last closeout (bridge Master promotion + capability-registry.md fold-in + Profile B compaction + shipped-features-tracker V1.57 snapshot + deferred-features-tracker V1.57 ship line + DF-46 reduced + tech-debt rollup + report-only QA); 3 V1.57 carry-forwards CLOSED (R-V156P1-M001/M002/P3-S003); 3 new V1.57+ residuals filed (R-V157P0-L001/L002 + R-V157P1-W001); DF-46 reduced (not Closed — 2 publish.* IDs still OUT per DF-59); wire contracts changed (3-caller adapter topology + new `nexus42 host-call` subcommand + 41-row acp §4 roster + bridge→Master spec promotion)

**Last updated (V1.55 history)**: 2026-06-22 (V1.55 closeout: 7 plans all Done — P-1 prepare + P0 DF-43 SQLite persistence / crate-model alignment (closed) + P1 DF-31 workspace interface skeleton + P2 game-bible Depth 3.5 (design-writing + design 五问 rubric + section completion detection + KB extraction; Master spec) + P3 Script profile scaffold (V1.54-style parity + additive BlockType dialogue/beat/act + script_category + ScaffoldTransaction closure on BOTH non-novel scaffolds) + P-mid QC rhythm + P-last closeout (Profile B compaction + spec promotion + tracker ship snapshot + tech-debt rollup); R-V154P1-W001 + R-V154P1-S002 + DF-43 + DF-31 all closed; 1 new R-V155P2-F002 → V1.56+; wire contracts unchanged)

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
| ~~DF-29~~ | `registry.refresh` capability (synthetic output + optional `--cdn-url` network) | V1.56 (P1) | Synthetic default (embedded snapshot, version `2026-06-22.v1`); `--cdn-url <url>` daemon flag with 10s timeout + 3-retry exponential backoff; sandbox/air-gap compatible (zero network in default mode); 11 negative tests for SSRF (HTTPS-only + redirect policy `limited(0)` + private-IP block 127/8 10/8 172.16/12 192.168/16 169.254/16 fc00::/7 ::1 IPv4-mapped + 8 MiB body cap + typed `CdnError` enum); qc2 Request Changes → fix-wave → qc-specialist-2 re-review Approve. Capability registered in CapabilityRegistry SSOT (20 tools total); spec amend `acp-capability-set.md` §4.7A + `cli-spec.md` §6.3 + §4.7A.1 Security contract. |
| ~~DF-31~~ | `workspace.open` / `workspace.commit` (full production) | V1.56 (P0) | V1.55 P1 shipped interface skeleton; V1.56 P0 closes full production: file-level OCC (SHA-256 content hash) + persistent DB-backed sessions (`workspace_sessions` table) + `changes[]` payload manifest (path/hash/op) + Local API redesign `/v1/local/{world,work,kb,schedule,workspace,findings}` scope. 26 nexus-local-db tests + 263 nexus-daemon-runtime tests + 4 spec amends (`local-runtime-boundary.md`, `daemon-runtime.md`, `local-db-schema.md`, `concurrency.md`). Pre-QC fix-wave `R-V156P0-CACHE-01` regenerated `.sqlx/` cache for nexus42 consumer queries. |
| ~~DF-42~~ | Full Local API redesign for World/User KB | V1.56 (P0) | Co-delivered with DF-31 per V1.56 compass Q5: `/v1/local/{world,work,kb,schedule,workspace,findings}` scope with coherent resource naming + unified error model. No broad standalone DF-42 surface redesign — folded into DF-31 production path. |
| ~~DF-56~~ | Conditional routing / branching engine (full roadmap) | V1.56 (P2+P3) | V1.42 minimal slice; V1.52 N-way + merge; V1.56 closes remaining 5 sub-items via 2 plans. |
| ~~DF-63~~ | **World KB cross-Work unification** | V1.40 (P0–P3) | All 5 slices via PR #52. BlockType taxonomy; ValidationMode; template var; artifact locator; sync_world_kb. |
| ~~DF-53~~ | FL-E auto-chain default stage sequencing | V1.39 (P0) | Full auto-chain + chapter loop + boot recovery. PR #50 (ad9725d8). |
| ~~DF-64~~ | Findings lifecycle | V1.39 (P1+P2) | findings table + DAO + novel-brainstorm/novel-review-master presets. PR #50. |
| ~~DF-66~~ | Per-chapter log subdirectories | V1.39 (P3) | Logs/{brainstorm,write,review,publish}/. PR #50. |
| ~~DF-67~~ | Master-decision timeout (96h) | V1.39 (P4) | 24h daemon task; stale-finding DAO; CLI banner. PR #50. |
| ~~DF-65~~ | Three-layer rules architecture | V1.40 (P0.5) | Migrated to embedded-rules/writing-craft.md. |
| ~~DF-62~~ | Multi-chapter serial + multi-volume PK | V1.42 (P1) | Volume-aware auto-chain; (work_id, volume, chapter) PK. |
| ~~DF-43~~ | SQLite persistence / crate-model alignment | V1.55 (P0) | From<ReferenceSourceRow> adapter; nexus-knowledge adapter-seam. |
| ~~DF-52~~ | nexus42 preset command group | V1.45 (P-last) | Resolved via creator run <preset_id>. |
| ~~BL-12~~ | creator run preset-generic entry | V1.45 (P0+P1+P2) | Replaced hardcoded enum variants with generic runner. |
| ~~BL-13~~ | STAGE_PRESET_ALLOWLIST stale ref | V1.45 (P1 T4) | Removed memory-review from allowlist. |

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
| ~~DF-60~~ | Multi-novel lifecycle (2-step completion + completion-lock + runtime lock columns + `creator works` IA) | **V1.41 P0** (Shipped 2026-06-11) | PR [#53](https://github.com/42ch-dev/nexus/pull/53) merged to `main`; post-merge `12753eb8` lineage validation. Plan [2026-06-10-v1.41-multi-work-switch.md](../plans/2026-06-10-v1.41-multi-work-switch.md). Spec [novel-writing/multi-work-lifecycle.md](../knowledge/specs/novel-writing/multi-work-lifecycle.md). **Note:** production `runtime_lock_holder` acquire deferred V1.42 P0. |
| ~~DF-61~~ | Selection pool + inspiration pool (DB SSOT + `Pool/Ideas/` MD) | **V1.41 P1** (Shipped 2026-06-11) | PR #53; post-merge `156e669d` `set_pool_active` creator_id authz. Plan [2026-06-10-v1.41-selection-pool.md](../plans/2026-06-10-v1.41-selection-pool.md). Spec [novel-writing/work-pool.md](../knowledge/specs/novel-writing/work-pool.md). |
| ~~BL-10~~ | Novel writing author quickstart (`docs/novel-writing-quickstart.md`) | **V1.43 P0** (Shipped 2026-06-12) | Shipped on `iteration/v1.43` (merge `340423e5`, 2026-06-12). Plan [2026-06-12-v1.43-novel-writing-quickstart.md](../plans/2026-06-12-v1.43-novel-writing-quickstart.md). Spec [novel-writing/author-experience.md](../knowledge/specs/novel-writing/author-experience.md). QC tri-review Approve (qc1 `efc8cfda`, qc2 `84e28acf`, qc3 `16953b9a` reval #2); QA Pass with residuals (`2709506a`). New file `docs/novel-writing-quickstart.md` (280 lines; Part I §1–§6 ongoing serial + Part II A/B/C optional/advanced) + 1-line cross-link in `docs/ARCHITECTURE.md`. 2 open residuals carry-forward to P-last hygiene plan `2026-06-12-v1.43-hygiene-and-residuals`: **R-V143P0-001** (spec overlay `novel-writing/author-experience.md` §2 row 4 references stale `creator run status`; should be `creator works status` per V1.41 cli-spec.md §6.2H) + **R-V143P0-002** (spec/CLI drift: `novel-writing/workflow-profile.md` §5.5.3 + `novel-writing/quality-loop.md` §6 reference future `creator run review-master <work_id>` surface, not yet implemented in current CLI; quickstart line 168 has an inline note for readers). |
| ~~DF-69~~ | **Standalone manuscript audit preset** (review report **or** KB extract on chapter正文) | **V1.44 P0** (Shipped 2026-06-13) | Dual-mode embedded preset `novel-manuscript-audit` (split into `novel-manuscript-audit-review` + `novel-manuscript-audit-extract` per R-V144P0-001 fix wave) + CLI entry `creator run audit-chapter --mode review|extract`. Review mode: structured 五問 report → `Logs/review/`. Extract mode: sync `kb.extract_work` without `kb_extract_jobs` (distinct from shipped `creator kb queue-extract --chapter`). Does NOT enter FL-E auto-chain driver. Plan [2026-06-13-v1.44-manuscript-audit-preset.md](../plans/2026-06-13-v1.44-manuscript-audit-preset.md). Spec [novel-writing/manuscript-audit.md](../knowledge/specs/novel-writing/manuscript-audit.md) (promoted Draft overlay → Shipped Feature line in P-last T1). Fix wave: R-V144P0-001..010 all resolved before ship (qc-specialist F-001 Critical + 11 Warning → fix commits `d6b9400e..fc9f2f6d` → targeted re-review Approve all 3 → QA Approve `5a0548c5` → Done). |

### Cancelled / Superseded

| ID | Status | Cancelled in | Reason |
|----|--------|--------------|--------|
| ~~DF-L~~ | **Cancelled** | V1.6 (accepted) | rand 0.7.3 blocked by wiremock — accepted as permanent tech debt. |
| ~~DF-M~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-07 — Capability schema registry sharing with platform. Over-designed. |
| ~~DF-N~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-02 — User-authored capabilities (shell / WASM plugin ABI). Over-designed. |
| ~~DF-O~~ | **Cancelled** | 2026-04-21 (V1.7 planning) | DF-05 — Full ACP permission policy engine UI (web-based). Not core product value. |
| ~~DF-P~~ | **Superseded** | 2026-04-21 (V1.7 planning) | DF-06 — Preset hot-reload. Snapshot semantics correct; real need → DF-17. |
| ~~DF-15~~ | **Cancelled** | V1.13 (governance closure) | OpenAPI export work. Nexus is not an OpenAPI-first product boundary for runtime value delivery; V1.13 resolves tracker ambiguity as governance-only closure with no implementation scope. |
| ~~BL-10~~ | **Superseded** | V1.46 P1 (2026-06-15) | `docs/novel-writing-quickstart.md` retired. Content migrated to specs only: narrative happy path → [novel-writing/author-experience.md](../knowledge/specs/novel-writing/author-experience.md) §3; CLI workflow → [creator-run-preset-entry.md](../knowledge/specs/creator-run-preset-entry.md) (Shipped Master V1.45). Runtime remediation strings updated to cite spec paths. No replacement file; `docs/ARCHITECTURE.md` links to specs. Plan: [2026-06-14-v1.46-spec-cli-hygiene.md](../plans/2026-06-14-v1.46-spec-cli-hygiene.md). |
| ~~DF-50~~ | **Cancelled** | V1.53 P-1 (2026-06-20) | V1.53 PM grill-me Q4: skills-export CLI redundant with static `embedded-skills/` model. PM-locked decision: remove CLI commands + retire spec + cancel DF-50. Plan: `2026-06-22-v1.53-skills-cli-cleanup`. |
| ~~DF-49~~ | **Cancelled** | V1.79 P-last (2026-07-01) | Standalone MCP server for Nexus capabilities. **Conflicts with ACP-client product direction** (`STRATEGY.md`: "CLI is an ACP client, not a server") + **circular-invocation risk** (Nexus drives an agent via ACP → that agent calls back into Nexus via MCP → loop). PM grill-me locked cancellation (not deferred) after user identified the architectural conflict. Plan: `2026-07-01-v1.79-closure`. |

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
| Key changes | `novel-writing/workflow-profile.md` Draft overlay Shipped (V1.36): `work_profile: novel` + `work_ref` extension; `work_chapters` DB SSOT (replaces `work-status.md`); `Works/<work_ref>/` layout (README + Outlines/ + Stories/ + Logs/); per-Work `Worldbuilding/` subtree removed (cross-Work worldbuilding lives in World KB); preset gates mechanism in `orchestration-engine.md §7.9` Master + novel-specific gates in `novel-workflow-profile §5.3` Draft overlay + `world_binding: required \| optional` toggle + scaffold protocol enumeration in §5.4; `novel-project-init` preset (10 prompts incl. World binding question + 4 templates + `novel.project_scaffold` capability with atomic FS+DB transaction + sanitization + FK checks); `sync_module` rewritten for `Works/<work_ref>/Stories/` scan + DB-enriched bundle; `creator run reconcile-chapters <work_id>` CLI + daemon endpoint; `novel-writing` 4-state chapter-scoped graph with `llm_judge` 五问 quality gate on `finalize` (`opening three lines / conflict resonance / twist recall / new perspective / ending hook`); `is_work_completed` evaluator + completion banner in `creator run status` + schedule guard rejecting `novel-writing` on completed Work; P1-P4 used PM-validate path (analogous to V1.35 P4) under time pressure (no QC tri-review for P2/P3; P1 had QC tri-review with PM-override w/ residuals) |
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
| Scope | **DF-53 full FL-E auto-chain (default true) + DF-68 daemon continuation checkpoint + DF-64/65/66/67 quality-loop full implement + DF-62 (V1.38) first-slice extension + DF-40 (V1.21) session-resume convergence**: P0 auto-chain engine (15 unit + 14 integration tests); P0.5 research-stage in chain (1 Critical + 2 AC Warnings closed in fix wave); P1 findings + routing (qc3 caught 1 Critical spec violation: missing `(work_id, chapter, status)` index per `novel-writing/quality-loop.md` §2.1, PM closed it); P2 `novel-brainstorm` + `novel-review-master` presets; P3 three-layer rules + Logs/; P4 96h finding escalation banner + daemon scheduled task; P5 V1.38 residual hardening (7 V1.38 residuals triaged: 2 fix, 5 accept-with-doc). |
| Plans | `2026-06-09-v1.39-harness-docs-prepare` (P-1), `2026-06-09-v1.39-fl-e-auto-chain-engine` (P0), `2026-06-09-v1.39-research-stage-wiring` (P0.5), `2026-06-09-v1.39-findings-and-review-routing` (P1), `2026-06-09-v1.39-novel-review-presets` (P2), `2026-06-09-v1.39-rules-and-logs` (P3), `2026-06-09-v1.39-master-decision-timeout` (P4), `2026-06-09-v1.39-v138-hardening` (P5) — all 8 plans Done on `iteration/v1.39`; P0..P5 ran in parallel where independent (P0.5 + P5). Stats: 88 commits + 10826 / -285 lines, all 8 CI checks green at PR #50. |
| Key changes | **P0** (auto-chain engine): works table extended with `auto_chain_enabled`/`auto_chain_interrupted`/`driver_schedule_id` (migration 202606090001); pure `auto_chain` module with `evaluate_next_step(work) -> ChainAction` (15 unit tests) + DB helpers; `ScheduleSupervisor::on_schedule_terminal` hook → `process_auto_chain_after_terminal` → shared `enqueue_auto_chain_schedule` helper (W-A dedupe); boot recovery via `find_resumable_works` (W-E partial index `works_auto_chain_resume`); side-input 409 invariant; `--auto-chain`/`--no-auto-chain` flags; `creator run resume`; patch_work_stage atomicity (W-D reorder); 21 hermetic integration tests. **P0.5** (research stage): research preset v1→v2 with `run_intents: knowledge_ingest`, gates `intake_status==complete + work_ref required`; `exit_when: kind llm_judge` (auto-chain compatible; W-1 fix from manual); `research_artifacts_dir` in produce stage input (W-2 fix); gate conditional on work_id (C-1 fix); 14+3 research tests. **P1** (findings + routing): `findings` migration 202606090002 (severity/status/target_executor TEXT enums); DAO with `create_finding`/`list_findings`/`update_finding`; `from-review` endpoint + `ReviewVerdictFinding` hook; CLI status Findings section with routing hints (→ write/brainstorm/none/master); 7 hermetic API tests + PM C-1 fix (added spec-required composite index `(work_id, chapter, status)`). **P2** (review presets): `novel-brainstorm` + `novel-review-master` embedded presets (preset+prompts); 4 validation tests + 8 e2e smoke tests; CLI hints documented. **P3** (rules + logs): embedded Layer 1 `writing-craft.md`; Layer 2 scaffold `Works/<work_ref>/Rules/novel-rules.md`; Layer 3 atomic history writer; `read_rules_layers()` reads L1+L2; `Logs/{brainstorm,write,review,publish}/` subdirs scaffolded; sync exclusion in `sync_module.rs`; 8 hermetic tests. **P4** (master-decision timeout): stale-findings DAO; 24h-interval daemon watcher (env-var override); CLI status banner `⏰ N findings stale (>96h)`; per-Work `auto_review_master_on_timeout` opt-in (default false); RVM-prefixed review-master schedule helper; 7 hermetic tests. **P5** (V1.38 hardening): closed R-V138P0-05 (NULL/0 tests) + R-V138P1-01 (completion guard); accepted R-V138P0-01/02/03/04 + R-V138P1-04 with doc/rationale; registered 3 new low-severity follow-ups (N1/N2/N3). |
| QC & QA | **P0**: initial tri-review all Approve; consolidated gate Request Changes (3 medium Warnings: W-A dedupe enqueue, W-D non-atomic PATCH, W-E missing index) → fix wave (5 commits) closed all 3 → targeted re-review (qc1 W-A, qc2 W-A, qc3 W-D+W-E) all Approve → final Approve. **P0.5**: qc1+qc2 Approve, qc3 Request Changes (1 Critical C-1: 4 daemon-runtime tests fail because gates reject schedules without Work) → fix wave (3 commits) closed C-1 + 2 AC Warnings (W-1 manual exit, W-2 artifacts in produce input) → final Approve. **P1**: qc1+qc2 Approve, qc3 Request Changes (1 Critical C-1: missing `(work_id, chapter, status)` composite index per `novel-writing/quality-loop.md` §2.1) → PM fix wave (1 commit + 1 test) closed C-1 → final Approve. **P2, P3, P4**: PM-validated (narrow scope, clean process, all evidence independently verified). **P5**: all 3 Approve. **PR #50 cursor security review (medium)**: P0.5 C-1 fix introduced a preset-gate authorization bypass; fix branch `fix/v1.39-preset-gate-bypass` (commit 3cc1601f) closed it before PR merge. All CI gates clean (cargo clippy --all -- -D warnings; cargo test --all green). |
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
| Key changes | P0: `preset_gates.rs` (work_field \| filesystem \| previous_preset) per `orchestration-engine.md §7.9`; `AddScheduleRequest.input: HashMap` wired from `creator run start --init-preset` grill-me to daemon → `preset.input.*`; daemon handler routes input into `PresetInput.vars` + seeds; `force_gates_audit` table (append-only) with `creator_id, forced_at` index; `creator_schedules.work_id` column + composite index; `novel_scaffold` `seed_chapters` + `patch_work` wrapped in single DB transaction; `embedded-presets/novel-writing/preset.yaml` gates moved under `preset:` key with full §5.3.2 gate set; `patch_work_tx` returns `Result<bool>` (no dirty-write); `--force-gates` / `--gate-reason` CLI flags with 512-char cap + ANSI/control char filter; reserved input keys policy; 23 files / +1921 lines / -126 in impl + 12 files / +727 / -262 in fix + 8 `.sqlx` regen. P1: `novel-writing/workflow-profile.md` extended with multi-chapter / multi-volume semantics — `next_chapter(work_id)` algorithm, `current_chapter` update rules, PK migration decision (defer to V1.37+), volume semantics + `Outlines/volume-outline.md` minimum structure, status UX example. P2: `entity-scope-model.md §5.1.1` extended with narrative World KB item taxonomy (foundation, background, character, location, society, rules, economy) + minimum-viable schemas; `novel-writing/workflow-profile.md §3.5.1` extended with `world_id` validation contract, prompt-time World context block format (YAML/JSON), `world_refs` validation rules, Chapter → World KB extraction path via `kb-extract` / `persist` stage. P3: `novel-writing/workflow-profile.md §5.5` extended with quality-loop roadmap — findings lifecycle + severity mapping + future local DB schema sketch (DF-64); executor mapping (write → novel-writing, brainstorm → future novel-brainstorm, none → manual, master → future novel-review-master); 96h master-decision timeout mapped to local DB + daemon scheduled lifecycle task + `creator run status` banner (DF-67); three-layer rules architecture (shared craft / per-work / append-only history) with SOUL/World KB boundaries (DF-65); `Logs/{brainstorm,write,review,publish}/` roadmap structure with `Logs/**` sync exclusion reaffirmed (DF-66) |
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
| **6 V1.45 Draft overlays** | Replaced with `Superseded by: [creator-run-preset-entry.md]` stub in: `creator-workflow.md` (FL-E CLI), `novel-writing/quality-loop.md` (preset-id commands; body applied P3), `novel-writing/manuscript-audit.md` (CLI entry + split presets), `work-experience-model.md` (side-input + run_intents), `orchestration-engine.md` (`run_intents` dispatch), `cli-spec.md` (`creator run` preset entry). |
| **Hard delete** | `RunCommand` variants: `Start`, `Continue`, `Stage`, `Resume`, `ReconcileChapters`, `AuditChapter`, `ReviewMaster`. `embedded-presets/novel-manuscript-audit/` (DEPRECATED parent dir) — split into `-review` and `-extract`. No deprecation aliases (pre-release; compass §0.1 #9). |
| **Three-plane IA shipped** | `creator bootstrap` (composite) · `creator works <sub>` (atomic) · `creator run <preset_id>` (strategy). Grill #10/#11: `creator works start` / `creator works create` rejected. |
| **Open residuals at ship** | 7 V1.45 B1 (QC1.S-1/2/3 + QC3.S-1/2/3/4, deferred Suggestions, severity: low) · 2 V1.45 B2 (broader spec-tree migration gaps + cross-link re-check, severity: low) · 1 V1.45 B3 (`R-V145B3-001`: cli-spec.md §6.2D/E body not yet rewritten to match new Master — out of plan scope, severity: low) |
| **Open deferrals (carry forward)** | Same as V1.44: DF-29, DF-31, DF-42, DF-44, DF-46, DF-48, DF-49, DF-50, DF-55, DF-59, DF-60/61 (V1.45 OUT as V1.44). No new deferrals registered in V1.45. |
| **QC & QA** | **P0+P1+P2 atomic merge** (Grill #19): QC1 Request Changes (C-1 + 3W + 3S) + QC2 Approve + QC3 Approve (2W + 4S) + QA PASS → targeted re-review fix round (1 dev, 6 commits) → QC1 revalidation **Approve** → consolidated **Approve**. **P3 quickstart+spec**: QC1 Request Changes (2W + 1S) + QC2 Approve + QC3 Request Changes (2W) + QA PASS → targeted re-review fix round (1 dev, 5 commits + 1 cross-ref hint) → QC1+QC3 revalidation **Approve** → consolidated **Approve**. **P-last hygiene**: PM-only closeout, no QC required (PM signoff per plan §5 T6). |

### V1.46 delivery snapshot (Shipped 2026-06-15)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#59](https://github.com/42ch-dev/nexus/pull/59) MERGED at `e7c8656c` (2026-06-15); 5 fix rounds (one per implement plan) on `iteration/v1.46`; integration branch retired post-merge |
|---|---|
| **Theme** | **Novel Author Maturity & Spec Hygiene** — author desk delta (`creator works status --json` + per-finding remediation; novel-only gate; `findings_stale` opt-in) + spec tree hygiene (BL-10 quickstart retired → embedded §3 of `novel-writing/author-experience.md`; 12 satellite spec amendments; cli-spec §6.2E deleted) + narrow runtime edges (on-disk chapter hints with cap=50 + tracing; dynamic clap `cli_args` for 3 first-slice presets) + hermetic supervisor research E2E (5 tests) + pool/inspiration mutation tracing (9 paths) |
| **Compass** | [v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md](../iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md) — 22 grill-me decisions locked (Grill #1–22); 5 implement + P-last |
| **Active spec** | [novel-writing/author-experience.md](../knowledge/specs/novel-writing/author-experience.md) promovido Draft (V1.46) → **Shipped (V1.46)** (P-last T1) — author path §3 embedded (~80 lines, formerly BL-10 quickstart); §4.1 JSON contract (findings[] + findings_stale + findings_truncated marker; W-1/W-2 reconcile — `Required: conditional` + creator-global scope clarify) |
| **cli-spec.md amendment** | V1.46 Shipped amendment: §6.2E FL-E stage subcommand block deleted (P1); `creator run <preset_id>` (V1.45 baseline) is sole normative surface |
| **Plans shipped** | **7 plans all Done** (Profile B compacted): P-1 (`2026-06-14-v1.46-harness-docs-prepare` — docs-only) + P0 (`2026-06-14-v1.46-author-desk-status-ux` — author desk delta) + P1 (`2026-06-14-v1.46-spec-cli-hygiene` — spec sweep + quickstart delete + runtime remediation) + P2 (`2026-06-14-v1.46-novel-runtime-ux-edges` — chapter hints + dynamic clap) + P3 (`2026-06-14-v1.46-research-auto-chain-e2e` — hermetic supervisor research E2E) + P4 (`2026-06-14-v1.46-pool-observability` — pool/inspiration mutation tracing) + P-last (`2026-06-14-v1.46-hygiene-and-closeout` — spec promotion + Profile B + lifecycle closures) |
| **Closed at ship** | **BL-10** (quickstart retired → spec-only SSOT) — V1.46 P1 supersede row appended to shipped-features-tracker.md line 82. **5 lifecycle residuals**: R-V139P5-S1 (supervisor+boot E2E — closed V1.46 P3), R-V139P5-N1 (chapter body_path hint — closed V1.46 P2), R-V145B1-002 (cli_args in --help — closed V1.46 P2), R-V141P1-15 (pool tracing — closed V1.46 P4), R-V141P1-10 (dual round-trip — **waived** per V1.46 P4 plan §1; doc note here) |
| **Atomic P1 (Grill #14)** | V1.46 P1 atomic delivery: delete `docs/novel-writing-quickstart.md` + fix `docs/ARCHITECTURE.md` link + cli-spec §6.2E delete + 12 satellite spec sweep + ~26 runtime quickstart refs → spec paths in **one merge** (`acabca53`). M-1 line surgical fix `cli-command-ia.md:67` (qc1 W-1) in the same atomic block. |
| **P0 / P2 / P3 / P4 fix rounds** | 4 additional fix rounds after QC tri-review: P0 (W-001 sequential I/O via `tokio::join!`; F-002 5s stale timeout; F-003 `findings_truncated` marker) + P2 (W-001 per-chapter `exists()` cap=50 + tracing span) + P3 (W-1 Debug-substring → typed `Gate`/`GateOp` pattern matching) + P4 (W-1 2 P4-introduced clippy errors in T3 capture test — `_guard` + `MutexGuard` scope). All targeted re-reviews (qc1 / qc3 only) re-Approve. |
| **PM-override** | **R-V145-PRE-CLIPPY-001** (high) — ~60 pre-existing `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` errors in `tasks/mod.rs` + `worker/registry.rs` + 2 test files (all in code untouched by V1.46 plans). Verified TRUE against `origin/main` HEAD `63b36a32` (V1.45 main post PR #58). Decision: **risk-accepted** per `.mstar/AGENTS.md` "Pre-existing claim verification protocol". Carry-forward to V1.47 hygiene plan. |
| **Open residuals at ship** | **22 open low-severity V1.46 plan residuals** (4 P0 + 9 P1 + 5 P2 + 3 P3 + 4 P4 — mostly test snapshot fragility, manifest description sanitization, cap coverage expansion, etc.; all `defer` V1.46+) + **1 pre-existing V1.45 clippy** (high) + **8 pre-existing V1.45 `nexus-local-db` clippy** (out of scope; carry-forward) = **31 open** (by_severity: low=30 + high=1) |
| **Open deferrals (carry forward)** | Same as V1.45: DF-29, DF-31, DF-42, DF-44, DF-46, DF-48, DF-49, DF-50, DF-55, DF-59, DF-60/61. No new deferrals registered in V1.46. |
| **QC & QA** | **P0 author desk delta**: qc1 0/0/2 + qc2 0/0/1 (Approve seat-level) + qc3 0/1/2 (W-001/F-002/F-003) → **Request Changes** → fix round (5 commits: tokio::join! + 5s timeout + `findings_truncated` marker + dead-code removal + plan §6 cmd fix) → targeted re-review (qc1+qc3) **Approve** → qa **PASS** (47/47 tests). **P1 spec CLI hygiene**: qc1 0/1/2 (W-1 AC-filter-gaming on `cli-command-ia.md:67`) + qc2 0/0/2 + qc3 0/0/4 → fix round (1 commit) → qc1 revalidation **Approve** → qa **PASS**. **P2 runtime UX edges**: qc1 0/0/2 + qc2 0/0/1 (Approve seat-level) + qc3 0/1/2 (W-001 per-chapter `exists()`) → fix round (cap=50 + tracing + 3 tests) → qc3 revalidation **Approve** → qa **PASS** (840 tests). **P3 research E2E**: qc1 0/0/2 + qc2 0/0/0 + qc3 0/1/2 (W-1 Debug-output assertion) → fix round (typed `Gate` pattern matching) → qc3 revalidation **Approve** → qa **PASS** (843 tests). **P4 pool observability**: qc1 0/1/2 (W-1 2 P4-introduced clippy) + qc2 0/0/1 + qc3 0/1/2 → fix round (scoped guards) → qc3 revalidation **Approve** + qc1 opportunistic re-check **Approve** → qa **PASS** (201/201 tests). **P-last hygiene**: PM-only closeout, no QC required (PM signoff per plan §5 T5). |
| **Profile B compaction** | 8 V1.46 plan JSON files in `.mstar/archived/plans/<plan-id>.json` + index in `.mstar/archived/plans-done.json`; 3 residual closure notes in `.mstar/archived/residuals/<plan-id>.json` |

### V1.47 delivery snapshot (Shipped 2026-06-15)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#60](https://github.com/42ch-dev/nexus/pull/60) MERGED at `8f4f9f2b` (2026-06-15T16:12:05Z); `iteration/v1.47` integration branch retired post-merge |
| **Theme** | **Novel Quality Loop Closure** — reflection-loop → findings producer (P0: `novel-chapter-review` preset, idempotency, closed enum) + remediation/gate audit (P1) + §4.5.7 acceptance tests #1–#3 + completion edge disposition (P2; R-V138P1-01 archived) + author-workflow-quality-loop spec reconcile (P3) + `nexus-orchestration` clippy cleanup (P-last) + bounded residual triage + spec promotion Draft → Shipped (P-last) |
| **Compass** | [v1.47-novel-quality-loop-closure-delivery-compass-v1.md](../iterations/v1.47-novel-quality-loop-closure-delivery-compass-v1.md) — 15 grill-me decisions locked (Grill #1–15); 5 implement + P-1 + P-last |
| **Active specs promoted Draft → Shipped (V1.47)** | [novel-writing/quality-loop.md](../knowledge/specs/novel-writing/quality-loop.md) (§8 output contract: `kind/severity/target_executor/rule_suggestion` per `ReviewVerdictFinding` schema) + [novel-writing/workflow-profile.md](../knowledge/specs/novel-writing/workflow-profile.md) §5.5.4 two-layer rules (Layer 2 = `Works/<work_ref>/AGENTS.md`; Layer 3 history removed from normative text) + §5.5.6 normative reflection→findings (`novel-chapter-review` both auto-chain and on-demand paths) |
| **Spec sweep (P0 fix-round)** | 8 active normative specs updated `reflection-loop` → `novel-chapter-review`: `novel-writing/workflow-profile.md`, `novel-writing/quality-loop.md`, `cli-spec.md`, `creator-run-preset-entry.md`, `orchestration-engine.md` (state machine + capabilities), `work-experience-model.md`, `novel-writing/manuscript-audit.md`, `novel-writing/author-experience.md` |
| **Plans shipped** | **6 plans all Done** (Profile B compacted): P-1 (`2026-06-15-v1.47-harness-docs-prepare` — docs-only) + P0 (`2026-06-15-v1.47-reflection-loop-findings` — `novel-chapter-review` preset + idempotency + closed enum + 5 hermetic tests) + P1 (`2026-06-15-v1.47-gate-remediation-audit` — intake/scaffold remediation + 6-file user-copy sweep + 3 new tests) + P2 (`2026-06-15-v1.47-serial-completion-hardening` — §4.5.7 #1/#2/#3 acceptance tests; R-V138P1-01 archived) + P3 (`2026-06-15-v1.47-quality-loop-spec-reconcile` — docs-only) + P-last (`2026-06-15-v1.47-hygiene-and-closeout` — clippy 100+ + bounded residual triage + spec promotion + Profile B) |
| **Closed at ship** | **5 whitelist residuals** (per compass §1.3): R-V145-PRE-CLIPPY-001 (high, fixed by P-last clippy), R-V146P1-QC3-S1 (intake remediation — fixed by P1), R-V146P1-QC3-S4 (raw `.mstar/` paths in user copy — fixed by P1), R-V145B2-001 (broader spec-tree migration — superseded by P0+P3), R-V145B2-002 (cross-link re-check — resolved by P3). All 5 archived with closure_note + closure_evidence. **R-V138P1-01** (V1.38 multi-chapter completion edge; baseline verify by P2 → archived as `lifecycle: resolved` by PM at V1.46 P-last; P2 confirms guard sufficient). |
| **Quality-loop producer shipped** | `novel-chapter-review` preset (renamed from `reflection-loop` per compass §0.1 #6) writes findings via existing `create_finding_from_review` daemon path. Idempotency: unique partial index `findings_unique_review_per_chapter` on `(work_id, chapter_no, source_schedule_id)`. Supervisor hook conditional on `preset_id == "novel-chapter-review"`. DAO surface: closed `FindingKind` enum (`craft/continuity/pacing/consistency/other`) + `rule_suggestion` 4 KiB length cap. |
| **V1.48+ follow-ups registered** | R-V147P0-01 (richer finding synthesis from review-report.md) · R-V147P0-02 (findings retention/cleanup policy) · R-V147P0-03 (FindingPatch.rule_suggestion clear-to-NULL) · R-V147P0-04 (rule_suggestion accepted-path → AGENTS.md mutation) · R-V147P0-05 (master_decision_timeout PK collision flake; hotfix OUT of P0 scope) · R-V147P0-06 (preset-id literal duplicated in 3 modules) · R-V147P1-01 (intake re-trigger on existing Work CLI gap) |
| **Non-whitelist open residuals (carry forward)** | 33 V1.45/V1.46 lows now `target: V1.48` (bulk bump at P-last) + 79 backlog + 1 V1.47 = 113 open (`low: 99 / medium: 8 / high: 2 / nit: 10 / critical: 0` after 5 closures) |
| **PM-override** | None — all 5 whitelist residuals closed by implementer work; no risk-accepts this iteration. |
| **QC & QA** | **P0 reflection-loop findings**: qc1/qc2/qc3 initial review **Request Changes** (0 critical, 6 warnings, 8 suggestions) → fix round (6 commits: spec sweep + idempotency + conditional hook + DAO surface + plan §6 + follow-ups) → targeted re-review (qc1/qc2/qc3) **Approve** (0/0/0) → qa **Pass** (5/5 review_findings + 7/7 findings_api + 207/207 preset + 6/6 findings lib). **P1 gate-remediation-audit**: qc1/qc2/qc3 initial review **Approve** (0/0/0+5 suggestions) → qa **Pass** (7 lib + 6 integration + 1 regression + 80 gate tests). **P2 serial-completion-hardening**: qc1/qc2/qc3 initial review **Approve** (0/0/0+5 suggestions) → qa **Pass** (28 work_chapters + 1 current_chapter + 7 completion + 21 auto_chain + 3 reject_produce). **P3 quality-loop-spec-reconcile (docs-only)**: QC **skipped** per `mstar-roles` §Non-Bypass Constraints → qa **Pass** (3 files / +5/-5 lines / 0 outside `.mstar/knowledge/specs/`). **P-last hygiene (T1 clippy)**: PM-only closeout, QC skipped for clippy hygiene; full `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` passes; 794 nexus-orchestration tests pass. |
| **Profile B compaction** | 6 V1.47 plan JSON files in `.mstar/archived/plans/<plan-id>.json` + index in `.mstar/archived/plans-done.json`; 5 residual closure notes in `.mstar/archived/residuals/<key>.json` |


### V1.48 delivery snapshot (Shipped 2026-06-16)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#61](https://github.com/42ch-dev/nexus/pull/61) MERGED to `main` at `8fcf2d68` (2026-06-16); integration branch `iteration/v1.48` retired |
|---|---|
| **Theme** | **Novel Quality Loop Completion** — F-D findings chain: P0 producer parse `review-report.md` (SSOT preset id + SSOT FindingKind enum expansion) + P1 consumer `novel-writing` prompt injection + P2 `AGENTS.md` Layer 2 runtime + accept `rule_suggestion` + `rules reset` CLI (`--dry-run`/`--yes`) + P3 retention policy + `rule_suggestion` tri-state NULL clear + P4 §4.5.7 #4 resume + #5 reconcile (DB-as-status-SSOT, atomic write, runtime-lock release on error) |
| **Compass** | [v1.48-novel-quality-loop-completion-delivery-compass-v1.md](../iterations/v1.48-novel-quality-loop-completion-delivery-compass-v1.md) — 12 grill-me decisions locked; 7 plans (P-1 + P0–P4 + P-last) |
| **Active specs promoted** | [archived/knowledge/novel-findings-maturity.md](../archived/knowledge/novel-findings-maturity.md) (Draft V1.48 overlay) **folded** into [novel-writing/quality-loop.md §9](../knowledge/specs/novel-writing/quality-loop.md) at P-last. [novel-writing/workflow-profile.md](../knowledge/specs/novel-writing/workflow-profile.md) header updated to V1.48 Shipped (V1.48 §5.5.2 open-findings prompt injection + §5.5.4 AGENTS.md Layer 2 runtime). |
| **Plans shipped** | **7 plans all Done** (Profile B compacted): P-1 (`2026-06-16-v1.48-harness-docs-prepare` — docs-only) + P0 (`2026-06-16-v1.48-findings-producer` — review-report.md parser + SSOT preset id + RVM_COUNTER hotfix) + P4 (`2026-06-16-v1.48-serial-hardening` — §4.5.7 #4/#5 + DB-as-SSOT reconcile) + P1 (`2026-06-16-v1.48-findings-consumer` — open_findings_block + WorkFields + preset v7→v8) + P2 (`2026-06-16-v1.48-rules-runtime` — AGENTS.md Layer 2 + accept + reset CLI) + P3 (`2026-06-16-v1.48-findings-data-hygiene` — retention DAO + tri-state NULL clear) + P-last (`2026-06-16-v1.48-hygiene-and-closeout` — 10 WL-A fix wave + overlay promotion + bulk defer) |
| **Closed at ship** | **6 R-V147P0-* residuals**: R-V147P0-01 (review-report parsing — V1.48 P0), R-V147P0-02 (retention policy — V1.48 P3), R-V147P0-03 (NULL clear — V1.48 P3), R-V147P0-04 (AGENTS.md runtime — V1.48 P2), R-V147P0-05 (RVM schedule_id PK collision hotfix — V1.48 P0 T0), R-V147P0-06 (REVIEW_PRESET_ID SSOT — V1.48 P0 T3). **10 WL-A V1.45/V1.46 lows** (per compass §1.3): R-V145B3-001, R-V146P0-QC3-S2, R-V146P2-QC2-W, R-V146P1-QC1-S1/S2/S3, R-V146P1-QC2-S1/S2, R-V146P1-QC3-S2/S3 — closed via 10 surgical commits in V1.48 P-last T1. **2 new R-V148P4-W2/W3** (low/medium) — registered for V1.49. **1 deferred (carried)**: R-V147P1-01 (intake re-trigger on existing Work) → V1.49. |
| **Hotfix H-1** | R-V147P0-05 (`master_decision_timeout` PK collision flake) — performed in-plan as V1.48 P0 T0; `RVM_COUNTER` (per-process `AtomicU32`) appended as 6-hex suffix to `RVM<ts_ms>` schedule id, mirroring the `ACH_COUNTER` / R-V139P0-W-B fix. |
| **Fix rounds during V1.48** | P0-fix1 (3 commits: 256 KiB bounded report read + single-tx parsed-finding batch + chapter field in fallback tracing); P4-fix1 (3 commits: atomic write via temp+rename + `ReconcileReport.resynced` counter + RuntimeLockGuard release on error); P1-fix1 (1 commit: regression test for `open_findings_block` wiring) |
| **Degraded tri-review note** | The `@qc-specialist-3` (k2p7), `@qa-engineer` (xai/grok), and other roles experienced model infrastructure failures during V1.48. PM-consolidated QC + QA reports at the `qc-consolidated.md` and `qa.md` level for affected plans, with explicit `degraded_tri_review: true` notes. The user has been giving autonomous direction ("持续推进到 PR-ready"), which PM interpreted as implicit consent. Future iterations should monitor model availability. |
| **V1.49+ follow-ups registered** | R-V147P1-01 (intake re-trigger on existing Work) + R-V148P0-W1 (path resolution defense-in-depth for consumer path) + R-V148P4-W2 (low — `creator works reconcile-chapters` lacks `--dry-run` / confirmation) + R-V148P4-W3 (medium — sync holds Work runtime lock for full filesystem walk) + 18 other open lows bulk-deferred at P-last T3 |
| **Non-whitelist open residuals (carry forward)** | 1 V1.47 carry-forward (R-V147P1-01) + 18 other open lows now `target: V1.49` (bulk bump at P-last) + 4 P-low/medium R-V148P4-W2/W3 deferred + 79 backlog = ~100 open (`low: ~95 / medium: 1 / high: 0 / nit: 4 / critical: 0`) |
| **PM-override** | None — all 6 R-V147P0-* + 10 WL-A residuals closed by implementer work. |
| **QC & QA** | **P0 findings-producer**: qc1 (architecture) Approve, qc2 (security) Approve, qc3 (perf) **Request Changes** (3 W: unbounded read, N sequential INSERTs, missing chapter in tracing) → P0-fix1 → qc3 re-review **Approve** → QA **Pass**. **P1 findings-consumer**: qc1 **Request Changes** (1 W: regression test for wiring), qc2 Approve, qc3 degraded → P1-fix1 → qc1 re-review **Approve** → QA **Pass**. **P2 rules-runtime**: qc1 **Request Changes** (1 W: doc regression), qc2 **Request Changes** (1 W: reset CLI safety flags), qc3 degraded → P2-fix1 → qc1+qc2 re-review **Approve** → QA **Pass** (PM-consolidated). **P3 data-hygiene**: PM-consolidated (degraded tri-review). **P4 serial-hardening**: qc1 Approve, qc2 Approve (lenient — should be Request Changes per gate), qc3 **Request Changes** (3 W: counter correction, lock release, sync holds lock) → P4-fix1 → qc2+qc3 re-review **Approve** → QA **Pass**. **P-last hygiene-and-closeout**: PM-consolidated (degraded). |
| **Profile B compaction** | 7 V1.48 plan JSON files in `.mstar/archived/plans/<plan-id>.json` + index in `.mstar/archived/plans-done.json`; 6 + 10 = 16 residual closures in `.mstar/archived/residuals/<plan-id>.json` (deferred to next iteration's archive pass) |


### V1.49 delivery snapshot (Shipped 2026-06-17)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR pending (iteration/v1.49 → main); P-last HEAD: `7e5df16f` (T2 merged) |
|---|---|
| **Theme** | **Novel Narrative Maturity & Author Desk** — F6 extended findings lifecycle (P0: 6-state transition machine, actionable consumer filter, W-1 typed error split) + F###/E### narrative index runtime MVP (P1: parser, serializer, id allocation, outline promotion; W-1+W-2 typed enum + explicit F### token) + Author desk UX (P2: intake re-trigger, reconcile --dry-run/--yes; R-V147P1-01 + R-V148P4-W2 closed; W-1 clap help text accuracy) + Serial reliability (P3: reconcile lock optimization, findings prune CLI + --dry-run, review-report path guard; R-V148P4-W3 + R-V148P0-W1 closed) + P-last hygiene (WL-A 10 V1.46 lows closed + overlay promotion: 3 Draft overlays folded into Masters) |
| **Compass** | [v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md](../iterations/v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md) — 12 grill-me decisions (Grill #1–12); 5 implement plans P0–P-last |
| **Active specs** | [novel-writing/quality-loop.md §2](../knowledge/specs/novel-writing/quality-loop.md) (F6 6-state lifecycle merged from `findings-lifecycle.md` overlay) + [novel-writing/workflow-profile.md §4.6](../knowledge/specs/novel-writing/workflow-profile.md) (F###/E### 5-column index schema merged from `narrative-indexes.md` overlay) + §5.5.1 three-state paragraph superseded + [novel-writing/author-experience.md §8](../knowledge/specs/novel-writing/author-experience.md) (author desk deltas merged) |
| **Draft overlays archived** | [novel-writing/findings-lifecycle.md](../knowledge/specs/novel-writing/findings-lifecycle.md) → Superseded (V1.49 P-last); [novel-writing/narrative-indexes.md](../knowledge/specs/novel-writing/narrative-indexes.md) → Superseded (V1.49 P-last) |
| **Plans shipped** | **5 implement plans all Done**: P0 (`2026-06-17-v1.49-findings-lifecycle` — F6 extended lifecycle + migration + API/CLI) + P1 (`2026-06-17-v1.49-narrative-indexes` — F###/E### runtime + promote) + P2 (`2026-06-17-v1.49-author-desk-ux` — intake re-trigger + reconcile preview) + P3 (`2026-06-17-v1.49-serial-reliability` — lock optimization + prune CLI + path guard) + P-last (`2026-06-17-v1.49-hygiene-and-closeout` — WL-A hygiene + overlay promotion + Profile B) |
| **Closed at ship** | **10 WL-A V1.46 lows** (per compass §1.3): R-V146P0-QC1-S2, R-V146P0-QC2-S1, R-V146P0-QC3-S3, R-V146P2-QC1-S1, R-V146P2-QC1-S2, R-V146P2-QC3-S1, R-V146P2-QC3-S2, R-V146P4-QC1-S1, R-V146P4-QC3-S1, R-V146P3-QC1-S1 — all closed via 11 surgical commits in V1.49 P-last T1. **3 product-seam residuals**: R-V147P1-01 (intake re-trigger — V1.49 P2), R-V148P4-W2 (reconcile dry-run — V1.49 P2), R-V148P4-W3 (lock optimization — V1.49 P3), R-V148P0-W1 (path guard — V1.49 P3). |
| **Wire contract changes** | None — per compass §0.1 #8. DB migration allowed (F6 status enum, narrative index tables); no new JSON Schema in `schemas/`. |
| **Open residuals at ship** | **7 open**, all `target: V1.50` — 4 V1.46 lows (R-V146P4-QC1-S2, R-V146P4-QC3-S2, R-V146P3-QC3-S1, R-V146P3-QC3-S2) + R-V149P0-01 (CLI `?status=open` gap, medium) + R-V149P1-01 (overlay schema reconciliation spec-only, low) + R-V149P1-02 (pre-existing flake `fallback_warn_includes_chapter_field`, low). |
| **Carry-forward (V1.50)** | **6 items**: 4 V1.46 lows (subscriber construction + tracing doc + SQL fixture + test panic) + R-V149P0-01 (CLI actionable-findings fetch) + R-V149P1-02 (review_report flake). R-V149P1-01 (overlay reconciliation) is spec-only at P-last fold; resolved in substance by T2 overlay promotion. |
| **QC & QA** | **P0 findings-lifecycle**: qc1 (arch) Request Changes (3 W) + qc2 (security) Approve + qc3 (perf) Request Changes (2 W) → fix wave → targeted re-review Approve → QA Pass. **P1 narrative-indexes**: qc1 Request Changes (1 W) + qc2 Approve + qc3 Approve (1 W pre-existing flake verified per protocol) → fix wave → re-review Approve → QA Pass. **P2 author-desk-ux**: qc1+qc2+qc3 Approve → QA Pass. **P3 serial-reliability**: qc1+qc2+qc3 Approve → QA Pass. **P-last T1 WL-A**: fullstack-dev surgical commits; PM-consolidated QC. **P-last T2 overlay promotion**: docs-only; no QC required per PM rules. |
| **Profile B compaction** | Pending — PM runs T5 after T4 merge to `iteration/v1.49`. 5 plan JSON files + index in `.mstar/archived/plans-done.json` + residual closures to `.mstar/archived/residuals/`.

### V1.50 delivery snapshot (Shipped 2026-06-18)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#63](https://github.com/42ch-dev/nexus/pull/63) (iteration/v1.50 → main); P-last HEAD: `62a5b906` (T6+T7) + `6f0bce75` (R-V150-WLA-11 clippy fix); **Merged 2026-06-17T18:26:46Z at commit `4db0a37b3e6a01836d8d90f968c75bcdd89754fc`** |
|---|---|
| **Theme** | **Novel Author Production Loop & World KB Closure** — A+B mixed primary axis: T-A (novel-writing cron staggering: per-Work schedule config, three-role defaults, auto-chain wiring, auto-chronology on finish) ∥ T-B (World KB complete loop: editor CLI, review-time candidate extraction, refreshable rescan). 8 plans in tracks + P-1 docs + P-last hygiene = 9 plans total. |
| **Compass** | [v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md](../iterations/v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md) — 18 grill-me decisions; 9 plans (P-1 + T-A P0-P3 + T-B P0-P2 + P-last) |
| **Active specs** | [novel-writing/workflow-profile.md §11](../knowledge/specs/novel-writing/workflow-profile.md) (cron staggering + auto-chronology normative) + [entity-scope-model.md §5.5](../knowledge/specs/entity-scope-model.md) (World KB promotion state machine Draft → Normative) |
| **Draft overlays archived** | [novel-writing/cron-staggering.md](../knowledge/specs/novel-writing/cron-staggering.md) → Superseded (V1.50 P-last T3); [novel-writing/auto-chronology.md](../knowledge/specs/novel-writing/auto-chronology.md) → Superseded (V1.50 P-last T3) |
| **Plans shipped** | **9 plans all Done**: P-1 (`2026-06-18-v1.50-harness-docs-prepare` — compass + plans + overlays + status activation + PM signoff) + T-A P0 (`2026-06-18-v1.50-cron-foundation` — works.schedule_json + default 3-role schedule + CLI surface) + T-A P1 (`2026-06-18-v1.50-cron-brainstorm-write` — cron_supervisor fire loop + auto-chain wiring) + T-A P2 (`2026-06-18-v1.50-cron-review-staggering` — review-time cron + quality_loop interplay) + T-A P3 (`2026-06-18-v1.50-auto-chronology` — per-Work opt-in volume auto-advance on finish) + T-B P0 (`2026-06-18-v1.50-kb-editor-cli` — creator world kb list/show/edit/delete) + T-B P1 (`2026-06-18-v1.50-kb-auto-promotion` — review-time candidate extraction → pending → confirm via adopt) + T-B P2 (`2026-06-18-v1.50-kb-refreshable-scan` — creator kb rescan + extract sync) + P-last (`2026-06-18-v1.50-hygiene-and-closeout` — V1.49 carry-forwards + WL-A 8-10 + overlay promotion + Profile B) |
| **Closed at ship** | **6 V1.49 carry-forwards** + **10 V1.50 WL-A** = 16 total. V1.49 carry: R-V146P4-QC1-S2, R-V146P4-QC3-S2, R-V146P3-QC3-S1, R-V146P3-QC3-S2, R-V149P0-01 (medium — CLI ?status=open comma-separated, DAO branches to dynamic IN (?, ?)), R-V149P1-02 (flake — #[serial_test::serial] + current_thread + serial_test = "3" dev-dep). V1.50 WL-A 8-10 closed in T2 of P-last (15 surgical commits, full T-A and T-B worktree renumber + cron delta write idempotency + atomicity reorder + carry-forward + 6 V1.49 closure). |
| **Wire contract changes** | None — per compass §0.1 #8. 8 DB migrations landed: 202606180001..202606180005 + 3 schema/TS. No new JSON Schema in `schemas/`. |
| **Open residuals at ship** | **0 open**; **8 deferred to V1.51+** (per `status.json.tech_debt_summary`): R-V150P1CRONBW-01 (medium — novel-write preset authoring), R-V150KBED-01/02 (KB legacy coexistence + world ownership), R-V150P2CRONRV-03 (plan text reconcile), R-V150KBED-07/08 (delta write scope + cross-chapter rescan), R-V150P3AUTOCHRONO-01/02 (last-planned-volume edge) + R-V150-WLA-DEFER-V1.51 (low-priority WL-A aggregate). |
| **Carry-forward (V1.51+)** | **8 items** listed above. R-V150P1CRONBW-01 (medium) is the only medium; rest are lows. |
| **QC & QA** | All 8 implement plans passed QC tri-review (3/3 per plan = 24/24). Cross-worktree migration renumber collisions surfaced in T-A P2/T-B P2/T-A P3 fix waves; PM-coordinated resolution. V1.50 has 1 trivial flake (R-V150P3AUTOCHRONO-02 in serialized review report) — closed via `#[serial_test::serial]` guard. |
| **Profile B compaction** | **Done** (V1.50 P-last T4): 8 plan JSON files in `.mstar/archived/plans/<plan-id>.json`; plans-done.json layout invariant verified (all 218 entries are strings); v1.50 iteration_summaries entry added; tech_debt_summary normalized (11 rows reconciled: 8 deferred + 3 archived via QC accept). |

### V1.51 delivery snapshot (Shipped 2026-06-19)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#64](https://github.com/42ch-dev/nexus/pull/64) (iteration/v1.51 → main); P-last HEAD: `bc259c19` (CI fix completion report); **Merged 2026-06-19T07:52:26Z at commit `c57b927bbf8621adf7ec59095d274f17f429c9b7`** (squash of 54 commits: 49 V1.51 + 4 CI warning hygiene + 1 CI workflow matrix split) |
|---|---|
| **Theme** | **KB Closure & Multi-Writer Concurrency** — S-B dual track. Track A (T-A P0–P2) KB closure pipeline: LLM-extract capability (P0), cross-chapter rescan (P1), missing-KB detection (P2). Track B (T-B P0–P1) multi-writer concurrency: per-key advisory lock with heartbeat + zombie detection (P0), per-row OCC on `kb_extract_jobs` and `novel_pool_entries` with stable `E_VERSION`/`E_LOCK` CLI codes (P1). 5 implement plans + P-1 docs + P-last hygiene = 7 plans total. |
| **Compass** | [v1.51-kb-closure-and-multi-writer-concurrency-delivery-compass-v1.md](../iterations/v1.51-kb-closure-and-multi-writer-concurrency-delivery-compass-v1.md) — 16 grill-me decisions; 7 plans (P-1 + T-A P0–P2 + T-B P0–P1 + P-last) |
| **Active specs** | [concurrency.md](../knowledge/specs/concurrency.md) (per-key advisory lock + per-row OCC normative) + [llm-extract.md](../knowledge/specs/llm-extract.md) (LLM extraction capability normative) + [novel-writing/quality-loop.md §12](../knowledge/specs/novel-writing/quality-loop.md) (missing-KB detection + cron interplay) + [entity-scope-model.md §5.5.2](../knowledge/specs/entity-scope-model.md) (KB delta write scope) + [cli-spec.md §6.2G](../knowledge/specs/cli-spec.md) (`creator world kb pending --missing-only`) + [world-kb-runtime-architecture.md](../knowledge/world-kb-runtime-architecture.md) (full pipeline normative) |
| **Draft overlays archived** | [concurrency.md](../knowledge/specs/concurrency.md) → Promoted (V1.51 P-last T3); [llm-extract.md](../knowledge/specs/llm-extract.md) → Promoted (V1.51 P-last T3); 4 other overlays (quality-loop §12, entity-scope §5.5.2, cli-spec §6.2G, world-kb-runtime-architecture) → folded into Masters |
| **Plans shipped** | **7 plans all Done**: P-1 (`2026-06-18-v1.51-harness-docs-prepare` — compass + plans + status activation) + T-A P0 (`2026-06-18-v1.51-llm-extraction` — `nexus.llm.extract` capability + heuristic→LLM swap, closes R-V150KBED-01) + T-A P1 (`2026-06-18-v1.51-cross-chapter-rescan` — `creator kb rescan --work <ref>`, closes R-V150KBED-08) + T-A P2 (`2026-06-18-v1.51-missing-kb-detection` — finalize-time missing-KB detection + `creator world kb pending --missing-only` CLI, closes R-V150P1CRONBW-01) + T-B P0 (`2026-06-18-v1.51-advisory-lock` — `Works/<work_ref>/.lock` advisory lock + heartbeat + zombie detection, closes R-V149P1-01 advisory-lock note) + T-B P1 (`2026-06-18-v1.51-per-row-occ` — per-row OCC + E_VERSION/E_LOCK stable CLI codes) + P-last (`2026-06-18-v1.51-hygiene-and-closeout` — 8 WL-A surgical fixes + 6 spec overlays promoted to Normative + Profile B compaction + tech debt rollup) |
| **Closed at ship** | **8 residuals total** = 4 V1.50 carry-forwards (R-V150KBED-01 via T-A P0; R-V150KBED-08 via T-A P1; R-V149P1-01 advisory-lock portion via T-B P0; R-V150P1CRONBW-01 via T-A P2) + 4 V1.51 WL-A (R-V151Q1-02, R-V151Q1-04, R-V151Q1-08, R-V151Q1-09) + 1 stale post-merge (R-V151-MERGE-CLIPPY-01 medium — clippy::unnecessary_trailing_comma in merge baseline) |
| **Wire contract changes** | None — per compass §0.1 #8. No new JSON Schema in `schemas/`. 1 DB migration landed: `202606190001_kb_extract_jobs_and_pool_version.sql`. 2 new test binaries (cas_migration_roundtrip, file_lock). |
| **Open residuals at ship** | **0 open**; **6 deferred to V1.52+** (per `status.json.tech_debt_summary.by_target_active.V1.52+`): R-V150KBED-01 (low — `creator world kb` legacy coexistence note, sweep docs) + R-V150KBED-02 (low — World vs World KB ownership narrative, 2 entries under kb-editor-cli + kb-auto-promotion) + R-V151Q3-W001 (low — two parallel LLM→KbCandidate paths merge candidate) + R-V151Q3-W002 (low — `WorkerUnavailable` empty-vec contract — caller must handle gracefully) + R-V151Q1-10 (low — process note: spec edit bundled under qc:-prefixed commit). |
| **Carry-forward (V1.52+)** | 6 items listed above. All low. R-V150KBED-01/02 are doc-sweep items; R-V151Q3-W001/W002 are refactor candidates; R-V151Q1-10 is a process note. |
| **QC & QA** | All 5 implement plans passed QC tri-review (3/3 per plan = 15/15). **T-A P0 qc3** had 1 re-review round (Request Changes on heuristic→LLM swap architecture). **T-B P0 qc1** had 1 re-review round (Request Changes on heartbeat semantics). **T-B P1 qc1+qc2** had 1 re-review round (Request Changes on per-row OCC + E_VERSION error code stability). **T-A P2 qc1** had 1 re-review round (Request Changes on missing-KB detection edge case). **P-last** PM-consolidated (single review per P-last rule). All fix waves surgical, no scope creep. |
| **Profile B compaction** | **Done** (V1.51 P-last T4): 7 plan JSON files in `.mstar/archived/plans/<plan-id>.json`; plans-done.json layout invariant verified (all 218 entries are strings); v1.51 iteration_summaries entry added; tech_debt_summary normalized (5 rows reconciled: 4 archived + 6 deferred to V1.52+ via 1 R-V151Q3 process note). |
| **Post-merge CI** | Run `27811732086` (squash merge) failed on Rust tests job with `os error 28` (No space left on device) on the ephemeral runner. PM dispatched architect: split monolithic `cargo test --all` into 4-leg job matrix (core / orchestration-domain / daemon / cli-hosts) with per-leg rust-cache keys, all legs passing in subsequent run `27812385526` (max 6m28s cli-hosts). Surgical change: `477aafbe` `ci(workflow): split rust-tests into per-group job matrix to fix ENOSPC on ephemeral runner` (+32/-4 in `.github/workflows/ci.yml` only, no Rust/Cargo.toml impact). |
| **Wire-contracts / signoff** | All 6 spec overlays now Normative; 8 WL-A selective fixes surgical. `cargo clippy --all -- -D warnings` and `cargo +nightly fmt --all --check` clean at ship. |


### V1.52 delivery snapshot (Shipped 2026-06-19 — retroactively added by V1.53 P-last)

**Note**: V1.52 P-last closed out shipping but did **not** add a §2 delivery snapshot or perform Profile B compaction. V1.53 P-last retroactively completed both (per V1.53 PM-locked rule to clean up V1.52 Profile B violation).

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#73](https://github.com/42ch-dev/nexus/pull/73) (iteration/v1.52 → main); P-last HEAD: `f4e7e201` (finalize delivery metadata); **Merged 2026-06-19T16:22:21Z at commit `d6aadd2fb5f287056dbd41b701eea8d5e6114dcc`** |
|---|---|
| **Theme** | **Author Completion & Multi-Branch Preset Orchestration** — T-A (outline 五问 quality gate + auto-promote + CLI surface consolidation + Work→KeyBlock provenance + essay profile) ∥ T-B (N-way GO/NOGO routing + multi-branch merge semantics). 7 plans in tracks + P-1 docs + P-last hygiene = 7 plans all Done. |
| **Plans shipped** | 7 plans all Done: P-1 + T-A P0-P2 + T-B P0-P1 + P-last. |
| **Wire contract changes** | None — per compass §0.1 #8. |
| **Closed at ship** | Per V1.52 compass; full list in `2026-06-19-v1.52-harness-docs-prepare` plan stub. |
| **Open residuals at ship** | **30 open**, all `target: V1.53+` (per `status.json.tech_debt_summary.by_target_active.V1.53+`). |
| **Carry-forward (V1.53+)** | 30 items listed above. |
| **QC & QA** | Per V1.52 P-last; tri-review and targeted re-review across 7 plans. |
| **Profile B compaction** | **Done (retroactive by V1.53 P-last)**: 7 V1.52 plan JSON files in `.mstar/archived/plans/2026-06-19-v1.52-*.json`; plans-done.json layout invariant verified (all 230 entries are strings); tech_debt_summary normalized. |

### V1.53 delivery snapshot (Shipped 2026-06-20)

| Aspect | Detail |
|---|---|
| **PR / Merge** | iteration/v1.53 → main; P-last HEAD on iteration branch: `33eb8201`; PR to main pending. |
|---|---|
| **Theme** | **Capability Surface Completion & Skills CLI Cleanup** — Theme A primary: `CapabilityRegistry` SSOT unification (id → access → admission → handler → ACP wire → failure mode → handler test vector) + DF-46 read-heavy slice (5 new read-heavy `nexus.*` tools) + skills-export CLI/spec retirement (DF-50 Cancelled). 5 plans total. |
| **Plans shipped** | **5 plans all Done**: P-1 (compass + capability-registry Draft overlay + DF-50 cancel + skills-export spec retirement) + P0 (registry SSOT unification, 3 sub-phase cutover, 8 → 13 host tools) + P1 (DF-46 read slice: 5 new read-heavy nexus.* tools + 9 new tests + 3 P0 residuals closed) + P-c (skills CLI cleanup: 179 deletions) + P-last (spec hygiene + dual Profile B + cli-spec.md §6.4 annotation). |
| **Wire contract changes** | None — per compass §0.1 #8. No `schemas/` changes. |
| **Closed at ship** | **All 6 P0 QC findings closed via fix-wave** (1 HIGH cross-creator isolation, 3 MEDIUMs, 1 NIT comments, 1 bonus timeline LIMIT). **All 6 P1 QC findings closed via fix-wave** (1 HIGH cross-creator isolation, 4 MEDIUMs, 1 NIT comments). **3 P0 residuals closed in P1** (parity expansion, catalog↔registry bijection test, DaemonToolDispatchAdapter doc). **DF-50 Cancelled** (skills-export CLI + spec retirement; see §1 row 83). |
| **Open residuals at ship** | **13 open** (per `status.json.tech_debt_summary.by_severity_active`): **4 medium** (R-V153P1QC2-003 daemon.health registry_ids exposure, R-V153P0QC2-001 P1 parity expansion deferred, R-V153P0QC2-002 catalog↔registry bijection test deferred, R-V153P0QC3-001 per-dispatch registry allocation) + **9 low** (R-V153P0QC2-003/004, R-V153P0QC3-002/003, R-V153P0-002, R-V153P1QC1R-001 timeline SQL dynamic LIMIT, R-V153P1QC2-004 kb_store runtime query, R-V153P1QC3-002 per-dispatch rebuild, R-V153PC1-N001 cli-spec.md §6.4 breaking-change annotation). |
| **Carry-forward (V1.54+)** | 13 items listed above. 1 medium is product-visible (daemon.health observability); 1 medium is correctness (catalog↔registry bijection); 2 mediums are optimization candidates (registry allocation + cache). Rest are docs / process / minor SQL runtime queries. |
| **QC & QA** | **P0**: tri-review (qc1 Request Changes → fix-wave → targeted qc1 re-review Approve with Notes; qc2/qc3 Approve with Notes). **P1**: tri-review (qc1 Request Changes → fix-wave → targeted qc1 re-review Approve with Notes; qc2/qc3 Approve with Notes). **P-c**: single-review (qc1 Approve with Notes). **P-last**: report-only QA (PM-direct; qa-engineer report-only verification pending). All fix waves surgical, no scope creep. |
| **Spec promotion decision** | **`capability-registry.md` kept as Draft overlay** — not promoted to Master in V1.53 P-last. Rationale: only validated in 2 iterations (P0 + P1, 13 tools); more tools coming in V1.54+ (DF-46 complete slice + non-novel profiles). Recommend re-evaluate at V1.54 P-last or V1.55. |
| **DF-50 disposition** | **Cancelled** — `nexus42 acp skills export\|verify` CLI surface removed (P-c); `skills-export-compatibility.md` retired to `archived/` (P-1); tracker DF-50 row moved to Cancelled archive. Static `embedded-skills/` model remains the only non-ACP integration path. |
| **Profile B compaction** | **Done (V1.53 P-last + V1.52 retro)**: 12 plan JSON files in `.mstar/archived/plans/2026-06-{19-v1.52,20-v1.53}-*.json` (7 V1.52 retro + 5 V1.53); plans-done.json layout invariant verified (all 230 entries are strings); tech_debt_summary normalized (13 open: 4 medium + 9 low; 0 critical/high). |

### V1.54 delivery snapshot (Shipped 2026-06-21)

| Aspect | Detail |
|---|---|
| **PR / Merge** | PR [#76](https://github.com/42ch-dev/nexus/pull/76) (iteration/v1.54 → main); merge commit `2fd183f059e898fd7f9fa0466653985364af7287`; merged 2026-06-21T02:22:34Z |
| **Plans** | 4/4 Done (P-1 prepare, P0 DF-46 write tools, P1 game-bible scaffold, P-last spec hygiene) |
| **P0 — DF-46 write tools** | 6 new mutation-side tools (kb_snapshot.write, manuscript.chapter.update, world.configure, work.schedule.set, finding.resolve, pool.entry.manage); LazyLock<CapabilityRegistry> cache + &'_static [AdmissionGate] conversion; Criterion dispatch_latency benchmark; 13 V1.53 residuals (4 medium + 9 low) all converged |
| **P1 — Game-bible scaffold (Depth 2)** | specs/game-bible-profile.md Draft + 7 new BlockType variants (species, faction, magic_system, technology, deity, level, economy_tier) + ValidationMode::GameBible + game-bible-init preset + GameBibleProjectScaffold capability + 12 Design templates + profile gates (is_novel_profile / is_game_bible_profile) + bootstrap --profile game-bible |
| **P-last** | capability-registry.md Draft → Master (promoted after V1.54 P0 validates write-tool patterns + V1.54 P1 adds GameBibleProjectScaffold); Profile B compaction (P0 + P1 archived to .mstar/archived/plans/; plans-done.json layout invariant verified); shipped-features-tracker V1.54 snapshot; deferred-features tracker V1.54 ship metadata; 13 V1.53 residuals archived; 2 P1 W-001/S-002 residuals registered to V1.55+ carry-forward |
| **Spec promotions** | `capability-registry.md` Draft → Master; `game-bible-profile.md` new Draft |
| **Spec amends** | acp-capability-set.md (+6 write IDs); agent-nexus-tool-bridge.md §8; cli-spec.md §6.2M + §12.1; entity-scope-model.md §5.1.1; non-novel-profiles-roadmap.md (game-bible Scaffold Shipped) |
| **QC outcomes** | P0: 3/3 Request Changes → 8-fix-wave (C-001 cross-world blocks, C-002 async fs + tx, W-001 admission metadata, W-002 finding.resolve NOT_FOUND, W-003 chapter path, W-002(qc3) benchmark cold path, W-003(qc3) concurrent write test, C-001(qc3) audit-log propagation) → all Approve. P1: 2/3 Request Changes + 1/3 Approve (qc2) → 2 fix-waves (C-001 profile spelling normalization, C-002 init_input creator_id, W-001 production schedule gate, W-002 nightly fmt, W-004 e2e tests, W-001(qc3) scaffold atomicity deferral) → all Pass after final QA |
| **CI gate** | `cargo clippy --all -- -D warnings` clean; `cargo test --all` ≥3970 passing (pre-existing flake on nexus-creator-memory verified TRUE per AGENTS.md protocol); `cargo +nightly fmt --all --check` clean on P1 files; Criterion dispatch_latency benchmark 1.8µs cold / ~446ns warm (both within target) |
| **Wire contracts** | Unchanged (no schema changes required) |
| **Open at ship / deferred to V1.55+** | 2 carry-over residuals: R-V154P1-W001 (game_bible.project_scaffold ScaffoldTransaction deferred), R-V154P1-S002 (profile-gate tracing::warn observability) |
| **Branch topology** | `iteration/v1.54` + per-plan `feature/v1.54-df46-write-tools` + `feature/v1.54-game-bible-scaffold` (peak 2 worktrees; both feature branches merged into integration before QC tri-review) |

### V1.55+ carry-forward index

- **2 items deferred to V1.55+** (per `status.json.residual_findings["2026-06-22-v1.54-game-bible-scaffold"]`):
  - R-V154P1-W001 (low): `game_bible.project_scaffold` not atomic — FS writes + DB PATCH not wrapped in transaction (ScaffoldTransaction deferred to V1.55+); `novel.project_scaffold` uses ScaffoldTransaction pattern (novel_scaffold.rs:763-830) — adopt same for game-bible
  - R-V154P1-S002 (low): profile-gate paths (is_work_completed, reconcile_from_filesystem) lack tracing::warn! / audit observability

**Note**: V1.53 P-last retroactively added V1.52 delivery snapshot. V1.54 P-last added V1.54 delivery snapshot directly (no retroactive needed since P-last ran normally).

### V1.55 delivery snapshot (Shipped 2026-06-22)

| Aspect | Detail |
|---|---|
| **PR / Merge** | Pending user push authorization; `iteration/v1.55` → `main`; merge commit TBD at PR time |
| **Plans** | 5/5 Done (P-1 prepare, P0 DF-43 SQLite alignment, P1 DF-31 workspace interface, P2 game-bible Depth 3.5, P3 Script scaffold) + P-mid + P-last closeout |
| **P0 — DF-43 SQLite persistence / crate-model alignment** | `From<ReferenceSourceRow> for nexus_knowledge::ReferenceSource` adapter in `nexus-local-db`; `nexus-knowledge` crate docs locked to model/adapter-seam only; `local-db-schema.md` §4.1.1 ownership boundary text; 7 round-trip + duplicate-truth + DB-only-field-isolation + invalid-enum-passthrough + tag-edge tests in `nexus-local-db/src/reference_source.rs` |
| **P1 — DF-31 workspace interface skeleton** | `validate_workspace_path_safe()` in `nexus-home-layout` (6 unit tests); `WorkspaceSessionManager` with atomic `consume_session` (concurrent test: 1 success / 9 conflict for N=10) + `SessionError` enum + poison recovery + typed errors; 2 Local API routes (`POST /v1/local/workspace/open`, `POST /v1/local/workspace/commit`); 9 handler tests + 7 session tests; no broad DF-42 Local API redesign |
| **P2 — Game-bible Depth 3.5** | `design-writing` embedded preset (LLM-driven per-section drafting with section-aware prompts); `design_five_q_check` rubric (design-pillars / mechanics / continuity / playability / clarity — NOT novel-prose 五问); `is_game_bible_design_complete` section detection (overview + pillars + mechanics + intake_status); `candidate_from_llm_json_for_profile` (profile-aware materializer) wired into `run_llm_extract` + `extract_via_llm` + `LlmExtractTask::evaluate` with production-path test (`llm_extract_task_with_game_bible_profile_produces_game_bible_candidate`); tracing additions on `is_work_completed` + `reconcile_from_filesystem` (R-V154P1-S002 closure) |
| **P3 — Script profile scaffold** | `script-profile.md` Draft (Scripts/ + Beats/ + Characters/ + Logs/ layout); additive BlockType variants `dialogue` + `beat` + `act`; `script_category` validation; `ValidationMode::Script`; 18 direct unit tests for Script mode validation; `script.project_scaffold` capability (ScaffoldTransaction applied to BOTH game-bible and script scaffolds with create/overwrite tracking + temp+rename atomic + `validate_work_ref` + crash-mid-transaction regression test); `nexus42 creator bootstrap --profile script` CLI |
| **P-mid** | Mid-QC rhythm: 6 QC tri-reviews (3 per plan × 2 plans) per wave; 4 mid-QA verifies (2 per wave); 2 fix-waves on P2 (qc1/qc3 → F-001 production-path coverage; qc1 2nd re-review → Approve); 1 fix-wave on P1 (qc1/qc2/qc3 → consume_session race); 1 fix-wave on P3 (qc1/qc2 → ScaffoldTransaction + validate_work_ref) |
| **P-last** | `game-bible-profile.md` Draft → Master (after V1.55 P2 Depth 3.5 evidence); `non-novel-profiles-roadmap.md` §1 + §2 status updated; Profile B compaction (5 plans archived to `.mstar/archived/plans/<plan-id>.json`; `plans-done.json` index + v1.55 iteration_summaries entry; layout invariant verified); tracker V1.55 ship snapshot (all 6 carry-forwards closed); shipped-features-tracker V1.55 snapshot; tech-debt rollup (`total_open: 1`, `total_resolved: 14`); `metadata.latest_ship` + `integration_branch_retired` updated; R-V155P2-F002 registered (deferred to V1.56+: design-writing preset no durable section_status auto-transition; manual author step for V1.55) |
| **Spec promotions** | `game-bible-profile.md` Draft → Master; `script-profile.md` new Draft (Feature line) |
| **Spec amends** | `entity-scope-model.md` §5.1.1 (script BlockType variants + `script_category` mapping); `cli-spec.md` §6.2 (script bootstrap profile + workspace interface user-facing stubs); `local-db-schema.md` §4.1.1 (DF-43 ownership boundary text); `non-novel-profiles-roadmap.md` §1.5 + §2.5 (V1.55 status); `orchestration-engine.md` (design-writing / script preset semantics) |
| **QC outcomes** | P0: 3/3 Approve. P1: 3/3 Request Changes → 1 fix-wave (atomic consume_session + SessionError + poison recovery + typed errors + capability count) → 3/3 Approve. P2: qc1+qc3 Request Changes → 2 fix-waves (profile-aware extraction + section_status narrowing + production-path test) → 3/3 Approve. P3: qc1+qc2 Request Changes → 1 fix-wave (ScaffoldTransaction + validate_work_ref + temp+rename + ValidationMode tests + daemon boot count) → 3/3 Approve. Total: 12 QC reports + 4 mid-QA verifies |
| **CI gate** | `cargo +nightly fmt --all --check` clean; `cargo clippy --all -- -D warnings` clean; `cargo test --all` 0 failures (post-fix-wave); `pnpm run codegen` no diff on generated |
| **Wire contracts** | Unchanged (additive enum only: `dialogue` + `beat` + `act` BlockType variants; per project convention `wire_contracts_changed: false`) |
| **Open at ship / deferred to V1.56+** | 1 carry-over: R-V155P2-F002 (low: design-writing preset no durable `section_status` auto-transition; manual author step for V1.55) |
| **Residuals closed in V1.55** | R-V154P1-S002 (P2 profile-gate observability); R-V154P1-W001 (P3 ScaffoldTransaction on BOTH non-novel scaffolds); DF-43 (P0 crate-model alignment); DF-31 (P1 interface skeleton shipped) |
| **Branch topology** | `iteration/v1.55` (retiring) + per-plan `feature/v1.55-df43-sqlite-alignment` + `feature/v1.55-df31-workspace-interface` + `feature/v1.55-game-bible-depth-35` + `feature/v1.55-script-scaffold` (peak 2 worktrees during Wave 1+2; all feature branches merged into integration before tri-review) |

**Note**: V1.55 P-last added V1.55 delivery snapshot directly. PR to `main` pending user push authorization.

---


### Tech-debt residuals shipped (V1.56)

| ID | Title | Shipped in | Notes |
|----|-------|------------|-------|
| ~~R-V155P2-F002~~ | `design-writing` preset no durable `section_status` auto-transition | V1.56 (P-last fix-wave, commit `248e3ead`) | Closed via new `nexus.game_bible.section_status.update` capability with transition validation (draft→reviewed→accepted, no skipping/backwards) + atomic write via temp+rename + frontmatter field preservation. `design-writing` preset updated with `auto_update_status` state invoking the capability after review pass. 24 new tests cover transition validation + extraction + replacement + atomic_write + capability_run + preserves_fields + section_not_found. Spec amends `acp-capability-set.md` §4.7B + `game-bible-profile.md` §4.1. V1.55→V1.56 carry-forward closure complete. |
| ~~R-V156P0-CACHE-01~~ | `.sqlx/` cache miss for nexus42 consumer queries | V1.56 (P0 pre-QC fix-wave, PM commit `8809f0b5`) | P0 added `workspace_sessions` table migration; P0 only refreshed `.sqlx/` cache for nexus-local-db macros but missed nexus42 consumer macros. PM regenerated via `cargo sqlx prepare --workspace` (2 new cache files added). cargo check --workspace clean post-fix. |
| ~~R-V156P2-CACHE-01~~ | Engine test bypass for converge runtime | V1.56 (P2 fix-wave-2, qc3 C-NEW-001 caught) | P2 fix-wave declared `record_converge_arrival` + `record_arrival` test helper, but tests bypassed real runtime path; qc3 found `contains("arrived")` dedup bug → fix-wave-2 added per-source `HashSet<String>` dedup + 2 regression tests + test helper bypass removed. PM-process lesson: engine tests must use real runtime path, not test-local helpers. |


### V1.54+ carry-forward index (historical; V1.54 closed)

*Original V1.54+ carry-forward index retained for historical reference; all items resolved in V1.54 closeout.*

- R-V153P1QC2-003 (medium): `daemon.health` exposes full registry_ids list — **resolved V1.54 P0 fix-wave (T8, gating via policy check)**
- R-V153P0QC2-001 (medium): P1 parity coverage expansion — **resolved V1.54 P0 fix-wave (T10, 20 hermetic write-tool tests)**
- R-V153P0QC2-002 (medium): catalog↔registry id bijection test — **resolved V1.54 P0 fix-wave (T10, `tool_allowlist_matches_registry_ids` test)**
- R-V153P0QC3-001 (medium): per-dispatch registry allocation on schedule hot path — **resolved V1.54 P0 (T5, `LazyLock<CapabilityRegistry>` cache)**
- R-V153P0QC2-003 (low): no concurrent dispatch test — **resolved V1.54 P0 (T10, `concurrent_dispatch_ten_parallel_write_tools` test)**
- R-V153P0QC2-004 (low): no separate Schedule caller-kind admission test — **resolved V1.54 P0 (T10, schedule admission test)**
- R-V153P0QC3-002 (low): missing dispatch-latency benchmark — **resolved V1.54 P0 (T6, `registry_lookup_cold_init_plus_19_lookups` Criterion benchmark)**
- R-V153P0QC3-003 (low): admission vectors `Vec<AdmissionGate>` instead of `&'static` — **resolved V1.54 P0 (T5, `&'_static [AdmissionGate]` conversion)**
- R-V153P0-002 (low): DaemonToolDispatchAdapter documentation — **resolved V1.54 P0 (T7, doc comment)**
- R-V153P1QC1R-001 (low): timeline SQL `LIMIT ?` + sqlx regen — **resolved V1.54 P0 (T7, deferred sqlx regen — non-blocking)**
- R-V153P1QC2-004 (low): kb_store runtime sqlx format! for LIMIT — **resolved V1.54 P0 (T7)**
- R-V153P1QC3-002 (low): per-dispatch CapabilityRegistry rebuild — **resolved V1.54 P0 (same theme as R-V153P0QC3-001)**
- R-V153PC1-N001 (low): cli-spec.md §6.4 acp skills omission annotation — **resolved V1.54 P0 (T7, intentional pre-1.0 breaking-change removal note added)**

*Original V1.52+ carry-forward index retained for historical reference; all items already resolved in V1.52/V1.53 closeout.*

- R-V150KBED-01 (low): KB editor — legacy `<work>/Worldbuilding/` coexistence; sweep docs.
- R-V150KBED-02 (low): KB editor — World vs World KB ownership narrative in `cli-spec.md` (2 entries: kb-editor-cli + kb-auto-promotion).
- R-V151Q3-W001 (low): Two parallel LLM→KbCandidate paths (extract_kb_candidates_for_review + LlmExtractTask::evaluate) — merge candidate.
- R-V151Q3-W002 (low): `WorkerUnavailable` empty-vec contract — caller must handle gracefully (heuristic fallback OK; consolidate semantics).
- R-V151Q1-10 (low): Process note: spec edit bundled under qc:-prefixed commit (commit 3a6950d5 included `concurrency.md` spec edit; future: keep spec edits in docs/ docs-only commits).


### V1.56 delivery snapshot (Shipped 2026-06-22)

| Aspect | Detail |
|---|---|
| **PR / Merge** | `iteration/v1.56` → `main`; merge commit pending user push authorization |
| **Plans** | 7/7 Done (P-1 prepare, P0 DF-31 full + DF-42 Local API redesign, P1 DF-29 registry.refresh, P2 DF-56 independent slice, P3 DF-56 dependent slice, P-mid meta, P-last closeout incl. R-V155P2-F002 fix-wave) |
| **Compass** | [v1.56-workspace-and-routing-seam-closure-delivery-compass-v1.md](../iterations/v1.56-workspace-and-routing-seam-closure-delivery-compass-v1.md) — 7 PM-locked grill decisions (Q1-Q7); north star = "All deferred 'Any future' infrastructure seams close" |
| **P-1 — Compass & Plan Stubs** | Compass (219 lines) + 7 plan stubs (P-1, P0, P1, P2, P3, P-mid, P-last) + deferred tracker activation (V1.56 carry-forward index: DF-29→P1, DF-31+DF-42→P0, DF-56 independent→P2, DF-56 dependent→P3, R-V155P2-F002→P-last) + status.json activation (7 plan rows); integration branch `iteration/v1.56` created from `main`; pre_implement_gate flipped to GO |
| **P0 — DF-31 Full + DF-42 Local API Redesign** | File-level OCC (SHA-256 content hash) + persistent sessions (`workspace_sessions` table; survives daemon restart; TTL expiry) + `changes[]` payload manifest (path/hash/op with typed errors) + Local API redesign `/v1/local/{world,work,kb,schedule,workspace,findings}` scope (coherent resource naming + unified error model) + 4 spec amends (`local-runtime-boundary.md`, `daemon-runtime.md`, `local-db-schema.md`, `concurrency.md`). 26 nexus-local-db tests + 263 nexus-daemon-runtime tests pass. PM pre-QC fix-wave `R-V156P0-CACHE-01` regenerated `.sqlx/` cache (2 new files for nexus42 consumer queries). qc1+qc2+qc3 all Approve with comments → PM consolidated Approve; 6 medium residuals registered (M-001..M-006). |
| **P1 — DF-29 registry.refresh** | `nexus.registry.refresh` capability (Read, context-level admission): synthetic output default (embedded 31-capability snapshot, version `2026-06-22.v1`, deterministic, version-pinned) + `--cdn-url <url>` daemon flag with built-in 10s timeout + 3-retry exponential backoff + sandbox/air-gap compatibility (zero network in default mode); capability registration (20 tools total in `nexus-daemon-runtime`); spec amends `acp-capability-set.md` §4.7A + `cli-spec.md` §6.3. 765 nexus-orchestration tests + 264 nexus-daemon-runtime tests pass. **qc2 Request Changes** → **fix-wave** added C-001 SSRF (HTTPS-only + `reqwest::redirect::Policy::limited(0)` + private-IP block 127/8 10/8 172.16/12 192.168/16 169.254/16 fc00::/7 ::1 IPv4-mapped + 8 MiB body cap) + H-001 typed `CdnError` enum (11 variants) + H-002 CLI/boot URL validation; 11 new negative tests; qc-specialist-2 re-review Approve. PM mid-QA follow-up: 13 P1 residuals registered (5 medium + 8 low) + 2 spec amends (`§4.7A.1 Security contract` + `cli-spec.md §6.3 Security contract table`). |
| **P2 — DF-56 Independent Slice (3 sub-items)** | (1) **Arbitrary stage-level conditional `next`** (not just `llm_judge`); (2) **expression/rule-based routing** (Form A `go`/`nogo` + Form B `branches`/`default`) with minimal grammar (comparisons, boolean ops, parens, field paths, literals); (3) **multi-branch graphs + merge points** (converge state kind + `wait_for_all`/`first_completed`/`any` strategies). 821 lib tests + 11 converge e2e tests pass. **qc1+qc2+qc3 all Request Changes** → **fix-wave 1** wired converge runtime + fixed throttle bug + added `MAX_EXPR_DEPTH=32` + aligned null semantics to JSON equality + extended `build_context_json` context whitelist + cached expression AST + propagated eval failures. **qc3 revalidation-2 Request Changes** → **fix-wave 2** closed C-NEW-001 per-source `HashSet<String>` dedup bug (qc3 caught test helper bypass that masked runtime bug). 8 new residuals registered (4 medium + 4 low). Spec extend `preset-conditional-routing.md` §3.3; PM-process lesson `R-V156P2-CACHE-01` (engine tests must use real runtime path). |
| **P3 — DF-56 Dependent Slice (2 sub-items)** | (1) **`registry.refresh` conditional edges** — expression grammar extended to `_context.registry_refresh.{source, snapshot_version, capability_count, fallback_reason, retry_count}`; (2) **`workspace.open`/`workspace.commit` branch inputs** — `_context.workspace.{session_id, conflict_detected, changes_applied, workspace_root}`. Runtime evaluator invokes `nexus.registry.refresh` + workspace handlers when state `next` references these fields. 841 lib tests + 20 new P3 integration tests pass; clippy + fmt clean. **qc1 Approve + qc2 Approve + qc3 Request Changes (warnings only)** → PM consolidated Approve via override; W-003 `entity-scope-model.md §5.5.8` PM amend applied (clarifies registry_refresh + workspace branch inputs are read-only projections, not entity owners); W-001+W-002 deferred as residuals. 6 new residuals registered (2 medium + 4 low). Spec extend `preset-conditional-routing.md` §3.4. |
| **P-mid** | Meta tracking across V1.56 3-wave rhythm (Wave 1: P0+P1 parallel; Wave 2: P2 single; Wave 3: P3 single). 12 QC reports + 4 targeted re-reviews + 1 closeout QA. Wave 1: 1 fix-wave (P1 SSRF). Wave 2: 2 fix-waves (P2 converge gap + dedup). Wave 3: PM override (W-003 spec amend + residuals for W-001/W-002). PM-process lessons: `R-V156-MIDQA-01` (mid-QA Gate 4 fmt drift on integration branch); `R-V156-PROCESS-01` (`.sqlx/` cache hygiene protocol with `--tests` flag). |
| **P-last** | R-V155P2-F002 fix-wave: new `nexus.game_bible.section_status.update` capability with transition validation (`draft` → `reviewed` → `accepted`, no skipping/backwards) + atomic write via temp+rename + frontmatter field preservation; `design-writing` preset updated with `auto_update_status` state invoking the capability after review pass. 24 new tests pass. Spec amends `acp-capability-set.md` §4.7B + `game-bible-profile.md` §4.1. Profile B compaction (7 plans archived to `.mstar/archived/plans/<plan-id>.json`; `plans-done.json` index updated; layout invariant verified). Shipped-features-tracker V1.56 snapshot. Deferred-features-tracker V1.56 ship line. Tech-debt rollup: `total_open: 35`, `by_severity_active: {medium: 18, low: 17}`, `by_target_active: {V1.57+: 35}`. `metadata.latest_ship` updated. `R-V155P2-F002` resolved (V1.55→V1.56 carry-forward closure complete). |
| **Spec promotions** | None new (P2 extended `preset-conditional-routing.md` body §3.3 in place; P3 extended §3.4 + PM added `entity-scope-model.md §5.5.8`; P-last extended `acp-capability-set.md §4.7B` + `game-bible-profile.md §4.1`) |
| **Spec amends** | `local-runtime-boundary.md` (DF-31+DF-42 scope + workspace OCC + persistent sessions); `daemon-runtime.md` (workspace session manager persistence + changes[] payload + OCC conflict semantics); `local-db-schema.md` (workspace_sessions table + DDL); `concurrency.md` (§9 file-level OCC semantics — content hash + conflict detection + retry model); `acp-capability-set.md` (§4.7A registry.refresh + §4.7A.1 Security contract + §4.7B game_bible.section_status.update); `cli-spec.md` (§6.3 `--cdn-url` flag + Security contract table); `preset-conditional-routing.md` (§3.3 independent slice + §3.4 dependent slice); `entity-scope-model.md` (§5.5.8 Conditional routing branch input visibility); `game-bible-profile.md` (§4.1 section lifecycle — auto-transition replaces V1.55 manual workaround) |
| **QC outcomes** | **Wave 1 (P0+P1)**: P0 3/3 Approve with comments → consolidated Approve; P1 qc2 Request Changes → fix-wave (C-001 SSRF + H-001 typed errors + H-002 URL validation) → 3/3 Approve. **Wave 2 (P2)**: 3/3 Request Changes → fix-wave 1 (H-001 converge runtime + H-002 throttle + W-003 parser depth + null semantics) → qc2 Approve + qc3 revalidation-2 → qc3 revalidation-2 → fix-wave 2 (C-NEW-001 dedup bug) → qc3 revalidation-2 Approve. **Wave 3 (P3)**: qc1 Approve + qc2 Approve + qc3 Request Changes (warnings only: W-001 tracing + W-002 instrumentation + W-003 entity-scope-model amend) → PM consolidated Approve via override (PM applied W-003 spec amend directly; W-001/W-002 deferred as residuals). **P-last fix-wave**: report-only QA by `@qa-engineer`. Total: 12 QC reports + 4 targeted re-reviews + 1 closeout QA + 1 mid-QA. |
| **CI gate** | `cargo +nightly fmt --all --check` clean; `cargo clippy --workspace -- -D warnings` clean; `cargo test --workspace --lib` 0 failures (post-fix-waves); `pnpm run codegen` no diff on generated (no new schema enum changes for P0/P1/P2/P3/P-last; all changes additive enum) |
| **Wire contracts** | **Changed**: New `/v1/local/{world,work,kb,schedule,workspace,findings}` scope redesign (P0); new capability IDs `nexus.registry.refresh` (P1) + `nexus.game_bible.section_status.update` (P-last); new conditional routing primitives (multi-branch expression grammar + converge state kind + merge-point strategies — P2/P3); new workspace session schema (workspace_sessions table + content_hash column + changes[] manifest — P0). Per project convention `wire_contracts_changed: true` (multiple additive enums + 1 breaking scope redesign — the /v1/local/* scope is a breaking change for any pre-V1.56 consumer). |
| **Open at ship / deferred to V1.57+** | **35 carry-forwards** (per `status.json.tech_debt_summary`): 18 medium + 17 low. Key items: P0 qc findings (sha2 dep, path boundary, no OCC race test, sync I/O, TOCTOU, no metrics); P1 qc findings (schema rename, global state, force param, tracing, reqwest reuse, etc.); P2 qc findings (null semantics, converge fan-in, expr routing tests, AST cache); P3 qc findings (workspace tracing, registry instrumentation, synthetic fallback ambiguity, throttle-path yield, field drops, workspace_state hook); PM-process (mid-QA fmt drift; sqlx hygiene protocol). 0 critical; 0 high. |
| **Residuals closed in V1.56** | **R-V155P2-F002** (P-last fix-wave: game-bible section_status auto-transition); **R-V156P0-CACHE-01** (PM pre-QC fix: `.sqlx/` cache miss for nexus42 consumer queries); **R-V156P2-CACHE-01** (PM-process lesson: engine test bypass caught C-NEW-001 dedup bug). **DF-29 + DF-31 (full) + DF-42 + DF-56 (full roadmap)** all shipped in V1.56. |
| **Branch topology** | `iteration/v1.56` (retiring) + per-plan `feature/v1.56-df31-df42-full-redesign` + `feature/v1.56-df29-registry-refresh` + `feature/v1.56-df56-independent-slice` + `feature/v1.56-df56-dependent-slice` + fix-wave branches `fix/v1.56-p1-ssrf` + `fix/v1.56-p2-converge-runtime` + `fix/v1.56-p2-converge-dedup` + `fix/v1.56-r-v155p2-f002` (8 feature branches + integration) |

**Note**: V1.56 P-last added V1.56 delivery snapshot directly. **Shipped via PR [#78](https://github.com/42ch-dev/nexus/pull/78) merged to `main` at `8a2fb20e` on 2026-06-21T14:24:37Z** (squash of 64 commits across 4 fix-waves). Profile B compaction (7 plans archived + `plans-done.json` index updated + `metadata.latest_ship` updated). V1.55→V1.56 carry-forward closure complete (0 open).

---

### V1.66 delivery snapshot (Shipped 2026-06-26)

| Aspect | Detail |
|---|---|
| **Theme** | **Tauri Desktop Shell** — Nexus goes from "open a browser tab to localhost:8420" → double-clickable macOS desktop application. Shell + light hygiene; no new browser-build UI features. |
| **Compass** | [v1.66-tauri-desktop-shell-delivery-compass-v1.md](../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md) — 3 PM-locked grill decisions (Q1 Tauri shell / Q2 T1 macOS-first unsigned / Q3 shell+light-hygiene); 10 §5 design items LOCKED by @architect Phase 2b |
| **Plans** | **6/6 Done** (P-1 prepare + P0 desktop-shell-core + P-sec hygiene + P1 sidecar-lifecycle-and-ci + P-mid meta + P-last closeout) |
| **Track A — Desktop Shell** | `apps/desktop` Tauri v2 macOS wrapper (pnpm workspace sibling; standalone src-tauri Rust crate) around `apps/web/dist` (frontendDist); `TauriClient` (21-method thin-over-`BrowserClient`, port injected to SPA via `window.__NEXUS_DAEMON_PORT__` after fix-wave-1 F1); capability detection (`__TAURI_INTERNALS__` + NEXUS_DESKTOP); Q5 desktop right-click actions (Open with…/Reveal in Finder via custom Tauri commands `open_with`/`reveal_in_finder` + runtime canonicalize+prefix-check path guard vs workspace root, W-002-equivalent; Tauri opener scope = defense-in-depth only per §5 #8); bundled `nexus42` sidecar (externalBin + plugin-shell Sidecar; autostart/stop/restart-on-crash with bounded backoff; pid-based liveness; attached-daemon active health-probe after F2); 5-state daemon-status indicator (starting/running/degraded/stopped/error); macOS CI `desktop-build` job (macos-13 + universal-apple-darwin + Swatinem/rust-cache + crates/** path filter; unsigned `.app`/`.dmg`, 90-day retention) |
| **Track B — Hygiene (P-sec)** | Closed 3 residuals: `R-V165-QC1-W2` medium (chapter HTTP integration tests — chapters_api 12 tests); `R-V165-QC-SUGG-DEFENSE` low (write-path guard parity host_tool_handlers↔chapter PUT + RuntimeLockGuard dedup + post-rename fsync — de-risks V1.67 body editor); `R-V164-QC1-S1-P0` low (PaginationInfo dedup across 5 handlers → nexus_contracts canonical, serialization parity verified) |
| **Spec promotions** | `web-ui.md` §14 Desktop Shell stage (Draft → Shipped V1.66); `desktop-shell.md` NEW Feature line (Draft → Shipped V1.66) |
| **Spec amends** | `daemon-runtime.md` §12 (Tauri sidecar mode); `local-api-surface-conventions.md` §9 (port discovery); `apps/web/DESIGN.md` Desktop Shell Supplement (window/menu/dialog/context-menu/daemon-status tokens, Standard+) |
| **QC outcomes** | Initial tri-review: qc1 RC(3W) + qc2 Approve(2W accepted §5 #8 trade-offs) + qc3 RC(4W); 0 Critical. Fix-wave-1 (@fullstack-dev, 8 fixes F1-F8: F1 port-exposure-to-SPA correctness bug + F2 attached-daemon probe + F3-F5 dev-prereq docs/scope + F6/F7 CI cache+path-filter + F8 error-label split). Targeted re-review: qc1 Approve + qc3 Approve → consolidated **APPROVE**. |
| **QA** | Pass — cargo test/clippy/fmt --all clean; web 110 tests; desktop crate 13 tests (incl. F1/F2 new tests); chapters_api 12; pagination_info_parity 3; cargo build --release in src-tauri success; interactive-GUI checklist (T8 + sidecar autostart + F1 port-override + F2 attached-crash) deferred to user (headless env) |
| **CI gate** | `cargo +nightly fmt --all --check` clean; `cargo clippy --all -- -D warnings` clean; `cargo test --all` 0 failures; `pnpm --filter web typecheck/test/build` green; `cargo test -p nexus-desktop` 13 pass |
| **Wire contracts** | **Unchanged** (`wire_contracts_changed: false`) — shell is a packaging/delivery layer; TauriClient reuses HTTP transport; desktop-only methods are Tauri IPC; residuals are test/refactor/hardening. `@42ch/nexus-contracts` version unaffected |
| **Closed residuals in V1.66** | `R-V165-QC1-W2` (medium), `R-V165-QC-SUGG-DEFENSE` (low), `R-V164-QC1-S1-P0` (low) |
| **Open at ship / deferred to V1.67+** | 10 V1.66 QC Suggestions registered as `R-V166-QC{1,2,3}-*` low residuals (event-driven status, robust unwrap, CI conditional fallback, Tauri exit hook, reset restart_count, reuse reqwest::Client, directory fsync, RuntimeLockGuard doc, backoff jitter, TOCTOU comment) + prior carry-forwards (R-V165-QC3-VIRT, R-V165-QC-SUGG-DX, R-V164-QC1-S1-P0/CASING/FE1-ORCH, R-V163P3-SURF-003/005, R-V164-P2-G3) |
| **Branch topology** | `iteration/v1.66` (from main @ 6e1f18e0) + per-plan `feature/v1.66-desktop-shell-core` + `feature/v1.66-hygiene-residuals` + `feature/v1.66-sidecar-lifecycle-and-ci` + fix-wave `fix/v1.66-qc-fix-wave-1` (Wave 1 peak 2 worktrees P0‖P-sec; all feature/fix branches merged into integration before QC; integration → main via PR) |

**Note**: V1.66 P-last added this snapshot. F-8 hygiene gap (V1.57–V1.65 snapshots missing from §2 body) remains open as a low-priority backfill — tracked separately, not a V1.66 blocker. **Shipped via PR (TBD) iteration/v1.66 → main** (V1.65 TauriClient stub → V1.66 implemented; apps/web SPA transport unchanged; V1.67 = body editor + lock + UI productivity + desktop distribution v2).

