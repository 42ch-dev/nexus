# Deferred Features — Cross-Version Tracker v1

**Status**: Active (V1.18 **Done**; V1.17 **Done**; V1.16 **Done**; V1.15 **Done**; V1.14 **Done**; residual SSOT = 2 **accepted** backlog items)
**Purpose**: Single source of truth for all features/tech-debt items that have been **deferred** from any delivery compass (V1.2–V1.18), with their lifecycle status across versions. This file enables version planning by showing what was promised, deferred, shipped, or cancelled — without reading every compass.
**Scope**: `nexus` OSS repository only. Platform features are referenced only when they block or depend on nexus-side work.
**Predecessor**: Consolidated from all delivery compasses (v1.2 through v1.18) and the v1.2 reclassification matrix.
**Created**: 2026-04-21
**Last updated**: 2026-05-15

---

## 1) How to use this file

- **Planning a new version**: Scan the "Open" table (§3) for items targeting that version or "Any future". Evaluate whether to scope them in.
- **Closing an item**: Move its row from "Open" to "Closed" (§4) with the actual completion version, plan-id, and a brief note.
- **Deferring again**: Update the `Target` column; keep the row in "Open". Add a note in `Deferral history`.
- **Source of truth**: This file is the **tracker**; the **compass** of the active version is the **scope authority**. If this file and the active compass conflict, the compass wins.

---

## 2) Lifecycle status definitions

| Status | Meaning |
|--------|---------|
| **Open** | Item has not been implemented. May have a target version assigned, or be in backlog. |
| **Shipped** | Implemented and merged in the indicated version. |
| **Cancelled** | Explicitly removed from scope (no longer planned). Includes "accepted as tech debt" with no intent to fix. |
| **Superseded** | Replaced by a different approach; the original item is no longer relevant. |

---

## 3) Open items

### 3.1 Features (deferred from a compass "Out" section)

| ID | Feature | First deferred | Target | Effort est. | Deferral history | Blocking reason / Notes |
|----|---------|---------------|--------|-------------|-----------------|----------------------|
| DF-12 | Dual outbox consolidation (full merge) | V1.2 | Any future | L | V1.2 (no fixed milestone) | Batch D waived. Knowledge: `dual-outbox-architecture.md`. Single-writer rule follow-up. |
| DF-13 | Entitlements API consumption (`/me/entitlements`, `/official-creator/quota`) | V1.3 | V2.0+ | M | V1.3 (not in V1.3) | Platform API dependency. |
| DF-16 | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2 (V1.3/V1.4)→V1.3 (not in V1.3) | ADR-011/012/013. Platform dependency. |
| DF-18 | Native multi-turn conversation (persistent child process) | V1.18 | V1.19 (Batch 1) | M | V1.18 §9 D-001 | `NativeSession` scaffolded but unused; `ClaudeCliProvider::execute()` spawns per-op. HIGH priority — multi-turn is a basic feature, not a simplification. |
| DF-19 | ACP session/request_permission handling | V1.18 | V1.19 (Batch 1) | M | V1.18 §9 D-002 | `AcpProvider::execute()` ignores `session/request_permission`; provider will hang/timeout. Depends on DF-23 (risk classifier). |
| DF-20 | SetModel/SetMode capability truthfulness | V1.18 | V1.19 (Batch 1) | S | V1.18 §9 D-003 | `CapabilityDescriptor::acp_full()` claims `set_model/set_mode=true` but `AcpProvider` returns `CapabilityUnsupported`. Must implement or remove claim. |
| DF-21 | TimeoutConfig enforcement | V1.18 | V1.19 (Batch 2) | S | V1.18 §9 D-004 | `TimeoutConfig` values defined in `config.rs` but never enforced in any provider code path. |
| DF-22 | Auto tool-risk classification | V1.18 | V1.19 (Batch 2) | M | V1.18 §9 D-005 | Only `StaticToolRiskClassifier` (hardcoded deny list). `ToolRiskClassifier` trait is an extension point needing real implementation. |
| DF-23 | Provider-level streaming adaptation | V1.18 | V1.19 (Batch 2) | L | V1.18 §9 D-006 | ACP streaming events not yet translated to `StreamingChunk`. Scaffold exists (`into_event_stream`) but not wired. |
| DF-24 | HostManager shutdown → ProviderAdapter::shutdown() | V1.18 | V1.19 (Batch 1) | S | V1.18 §9 D-007 | `HostManager::shutdown()` never calls `ProviderAdapter::shutdown()` — orphan processes on daemon stop. Safety fix. |
| DF-25 | AdmissionPolicy enforcement wiring | V1.18 | V1.19 (Batch 1) | S | V1.18 §9 D-008 | `AdmissionPolicy` methods exist but never invoked from `create_session()` or `exec()`. Correctness fix. |
| DF-26 | Cross-platform command probe (replace Unix-only `which`) | V1.18 QC R3 | V1.19 (Batch 1) | S | V1.18 status.json R3 | `path_scan.rs` uses Unix-only `which` command. Breaks on Windows. |
| DF-27 | API handler input validation on session ID path params | V1.18 QC R4 | V1.19 (Batch 2) | S | V1.18 status.json R4 | Malformed/non-UUID session IDs in `/v1/local/agent-host/sessions/{id}/*` routes. |
| DF-28 | Config path traversal protection | V1.18 QC R5 | V1.19 (Batch 2) | S | V1.18 status.json R5 | `config_path` and `workspace_root` not validated against directory traversal. |

### 3.2 Backlog (no committed target version)

| ID | Feature | First deferred | Target | Effort est. | Notes |
|----|---------|---------------|--------|-------------|-------|
| DF-03 | Preset third-party registry / signing / publish | V1.4 | Backlog | XL | Requires trust model + distribution protocol. **Long-term backlog** — potentially an independent project, not a nexus version feature. |
| BL-01 | World Merge complete execution / rollback product form | V1.2 | Backlog | XL | Spec anchor: `platform/world-merge-execution-backlog-v1.md`. |
| BL-02 | Local Shadow Read / staged change full chain | V1.2 | Backlog | L | Requires product spec. |
| BL-03 | Advanced declarative Context Assembly API / DSL | V1.2 | Backlog | XL | Spec anchor: `platform/context-assembly-advanced-dsl-backlog-v1.md`. |
| BL-04 | Long-running task checkpoint (product-level) | V1.2 | Backlog | M | |
| BL-05 | Commonware / multi-workspace advanced narrative | V1.2 | Backlog | XL | |
| BL-06 | Independent search microservice | V1.2 | Backlog | L | Compatible with old "not mandatory" principle. |
| BL-07 | Explore ranking / cold-start strategy + Publish compliance determination matrix | V1.2 | Backlog | M | Elevated by ADR-011 + product spec in V1.2 matrix (originally V1.4). |
| BL-08 | Social / marketing features | V1.3 | V2.0+ | XL | ADR-011/012/013. |
| BL-09 | V1.17 Prompt + Skills Compass v1 (planning package) | V1.16 | V1.17 (GATED) | M | **Gate met** — V1.16 compass Done. Planning-only package activated for V1.17 execution scope. |

### 3.4 Gated planning package details (not executable scope yet)

#### BL-09 — V1.17 Prompt + Skills Compass v1

- **Status**: Execution complete (V1.17 shipped)
- **Gate**: ✅ Met — `v1.16-delivery-compass-v1.md` is Done.
- **Outcome**: V1.17 prompt/skills work delivered. See V1.17 delivery snapshot in §5.
- **Planned themes** (for reference):
  - **S1 Embedded skills quality**: Trigger rules + evidence standards; skill versioning and change records.
  - **S2 Preset prompt refinement**: `novel-writing` quality/consistency; `research` output structure + traceability.
  - **S3 Output evaluation**: Golden outputs + regression comparison; optional evaluation harness requires a separate ADR if enabled.
- **Entry criteria to activate execution planning**:
  1. `v1.16-delivery-compass-v1.md` is marked Done.
  2. Cross-repo contract updates are completed and traceable.
  3. A new V1.17 row is added in `{PLAN_DIR}` and recorded in `status.json`.

### 3.3 Open tech-debt residuals (tracked in `status.json`)

Authoritative machine state: **`status.json` root `residual_findings`**（`updated_at` **2026-05-11**）。`metadata.tech_debt_summary.total_open` is **0** — remaining rows below are **`decision: accept`** with `target_date: backlog` (QA-owned follow-ups, not blocking releases).

| ID | Title | Severity | Decision | `target_date` | Origin plan | Scope |
|----|-------|----------|----------|----------------|-------------|-------|
| R-V113-005 | UpstreamTimeout e2e test duration varies by OS/proxy (up to ~30s) | low | accept | backlog | `2026-05-06-v1.13-oss-forward-delivery` | `crates/nexus42/tests/creator_register_e2e.rs` |
| R-V113-007 | Pre-existing flaky test `auth::tests::get_returns_none_for_unknown_creator` | low | accept | backlog | `2026-05-06-v1.13-oss-forward-delivery` | `crates/nexus42/src/auth/mod.rs` |

**Hygiene note (2026-05-11)**: Older tracker ids (R5, R11, R-WA-*, R-M1-W*, R-V110-*, …) are **not** present in root `residual_findings` today. Recover narrative detail from `archived/residuals/` / plan QC reports if you need historical provenance.

> **Note**: `DEBT-RAND-073` (rand 0.7.3, blocked by wiremock) is **cancelled/accepted** — listed in §4 Closed.

---

## 4) Closed items

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

---

## 5) Per-version summary

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
| Cross-repo gates | Canonical: `nexus-platform/.agents/knowledge/v1.14-program-compass-v1.md` §5 |

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

### V1.16+ horizon

| Category | Position |
|----------|----------|
| Program | **Compass registered** — delivery SSOT：[v1.16-delivery-compass-v1.md](../iterations/v1.16-delivery-compass-v1.md). `status.json` `plans[]` **empty**. |
| Next version (gated) | ~~V1.17 prompt-skills planning package tracked as **BL-09** in §3.4~~ → V1.17 **Done**; BL-09 gate met and executed. |

### Items targeting V1.19

| Category | Count | IDs |
|----------|-------|-----|
| Features (Batch 1 — safety/correctness) | 6 | DF-18 (multi-turn), DF-19 (ACP permissions), DF-20 (capability truthfulness), DF-24 (shutdown wiring), DF-25 (admission wiring), DF-26 (cross-platform probe) |
| Features (Batch 2 — hardening) | 5 | DF-21 (timeout enforcement), DF-22 (risk classification), DF-23 (streaming adaptation), DF-27 (API validation), DF-28 (path traversal) |
| **Total** | **11** | All deferred from V1.18 §9 + QC residuals |

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
| Tech-debt (accepted backlog) | 2 | R-V113-005, R-V113-007（§3.3） |
| **Total** | **11** | |

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

## 6) Change control

- **Updates**: When a version ships, move all delivered items to §4 Closed. When an item is re-deferred, update §3.
- **Source compasses remain authoritative for scope decisions**: If the active compass says "Out" for an item but this tracker has it as "Open" with that version target, the compass controls whether it enters scope.
- **Effort estimates are approximate** (XS/S/M/L/XL agent-session scale) and for planning guidance only — not contractual. See `effort-estimation.md` for methodology.
- **Residual detail**: Machine-state residuals (§3.3) are authoritative in **`status.json` root `residual_findings`**. This file mirrors them for cross-version planning convenience; if there's a conflict, `status.json` wins.

---

## 7) Related index

Internal (this repo):

- V1.2 delivery compass: [v1.2-delivery-compass-v1.md](../iterations/v1.2-delivery-compass-v1.md)
- V1.2 reclassification matrix: [v1.2-reclassification-matrix-v1.md](../iterations/v1.2-reclassification-matrix-v1.md)
- V1.3 delivery compass: [v1.3-delivery-compass-v1.md](../iterations/v1.3-delivery-compass-v1.md)
- V1.4 delivery compass: [v1.4-delivery-compass-v1.md](../iterations/v1.4-delivery-compass-v1.md)
- V1.5 delivery compass: [v1.5-nexus-delivery-compass-v1.md](../iterations/v1.5-nexus-delivery-compass-v1.md)
- V1.6 delivery compass: [v1.6-delivery-compass-v1.md](../iterations/v1.6-delivery-compass-v1.md)
- V1.7 delivery compass: [v1.7-delivery-compass-v1.md](../iterations/v1.7-delivery-compass-v1.md)
- V1.8 delivery compass: [v1.8-delivery-compass-v1.md](../iterations/v1.8-delivery-compass-v1.md)
- V1.9 delivery compass: [v1.9-delivery-compass-v1.md](../iterations/v1.9-delivery-compass-v1.md)
- V1.10 delivery compass: [v1.10-delivery-compass-v1.md](../iterations/v1.10-delivery-compass-v1.md)
- V1.13 delivery compass: [v1.13-delivery-compass-v1.md](../iterations/v1.13-delivery-compass-v1.md)
- V1.14 delivery compass: [v1.14-delivery-compass-v1.md](../iterations/v1.14-delivery-compass-v1.md)
- V1.15 delivery compass: [v1.15-delivery-compass-v1.md](../iterations/v1.15-delivery-compass-v1.md)
- V1.16 delivery compass: [v1.16-delivery-compass-v1.md](../iterations/v1.16-delivery-compass-v1.md)
- V1.17 delivery compass: [v1.17-delivery-compass-v1.md](../iterations/v1.17-delivery-compass-v1.md)
- V1.18 delivery compass: [v1.18-delivery-compass-v1.md](../iterations/v1.18-delivery-compass-v1.md)
- V1.19 delivery compass: [v1.19-delivery-compass-v1.md](../iterations/v1.19-delivery-compass-v1.md)
- V1.17 prompt-skills compass: merged into this tracker under `BL-09` (§3.4)
- Orchestration engine design: [../knowledge/orchestration-engine.md](../knowledge/orchestration-engine.md)
- ACP client tech spec v2: [../archived/../archived/knowledge/acp-client-tech-spec.md](../archived/../archived/knowledge/acp-client-tech-spec.md)
- Creator schedule & core context: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)
- Crate selection best practices: [crate-selection-best-practices.md](crate-selection-best-practices.md)
- `status.json` (machine-state residuals): [../status.json](../status.json)

External (v1-spec, resolved via `.agents/local-paths.json`):

- `{v1-spec}/architecture/v1.md` — base architecture
- `{platform-designs}/roadmap.md` — program roadmap

---

*Created: 2026-04-21. Last updated: **2026-05-15**. Status: Active. V1.18 Done (agent-host-core, 11 deferred → V1.19); V1.19 Draft (hardening, 11 items, 2 batches); V1.17 Done (prompt-skills, BL-09 gate met); V1.16 Done; V1.15 Done (PR #23 merged); V1.14 Done; V1.13 DF-11/DF-14 shipped, DF-15 governance-closed. `residual_findings` 收敛为 **2** 条 accepted backlog（§3.3）+ V1.18 code-quality residuals (R1-R7, closing via V1.19). V1.19 hardening backlog: DF-18–DF-28 (11 items, 2 batches).*
