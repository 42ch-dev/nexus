# Deferred Features — Cross-Version Tracker v1

**Status**: Active (V1.14 **platform Done**; OSS `plans[]` empty — **no** new nexus `{PLAN_DIR}` row; residual SSOT = 2 **accepted** backlog items)
**Purpose**: Single source of truth for all features/tech-debt items that have been **deferred** from any delivery compass (V1.2–V1.10), with their lifecycle status across versions. This file enables version planning by showing what was promised, deferred, shipped, or cancelled — without reading every compass.
**Scope**: `nexus` OSS repository only. Platform features are referenced only when they block or depend on nexus-side work.
**Predecessor**: Consolidated from all delivery compasses (v1.2 through v1.10) and the v1.2 reclassification matrix.
**Created**: 2026-04-21
**Last updated**: 2026-05-10

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
| DF-12 | Dual outbox consolidation (full merge) | V1.2 | Any future | L | V1.2 (no fixed milestone) | Batch D waived. Knowledge: `dual-outbox-architecture-v1.md`. Single-writer rule follow-up. |
| DF-13 | Entitlements API consumption (`/me/entitlements`, `/official-creator/quota`) | V1.3 | V2.0+ | M | V1.3 (not in V1.3) | Platform API dependency. |
| DF-16 | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2 (V1.3/V1.4)→V1.3 (not in V1.3) | ADR-011/012/013. Platform dependency. |

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

### 3.3 Open tech-debt residuals (tracked in `status.json`)

Authoritative machine state: **`status.json` root `residual_findings`**（`updated_at` **2026-05-10**）。`metadata.tech_debt_summary.total_open` is **0** — remaining rows below are **`decision: accept`** with `target_date: backlog` (QA-owned follow-ups, not blocking releases).

| ID | Title | Severity | Decision | `target_date` | Origin plan | Scope |
|----|-------|----------|----------|----------------|-------------|-------|
| R-V113-005 | UpstreamTimeout e2e test duration varies by OS/proxy (up to ~30s) | low | accept | backlog | `2026-05-06-v1.13-oss-forward-delivery` | `crates/nexus42/tests/creator_register_e2e.rs` |
| R-V113-007 | Pre-existing flaky test `auth::tests::get_returns_none_for_unknown_creator` | low | accept | backlog | `2026-05-06-v1.13-oss-forward-delivery` | `crates/nexus42/src/auth/mod.rs` |

**Hygiene note (2026-05-10)**: Older tracker ids (R5, R11, R-WA-*, R-M1-W*, R-V110-*, …) are **not** present in root `residual_findings` today. Recover narrative detail from `archived/residuals/` / plan QC reports if you need historical provenance.

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
| Delivery SSOT | [v1.14-delivery-compass-v1.md](v1.14-delivery-compass-v1.md)（§0 scope lock **合并于**本 compass） |
| Machine state | `status.json` `plans[]` **空**；`residual_findings` 仅 **R-V113-005** / **R-V113-007**（accepted / backlog） |
| Platform execution | **Done** — `nexus-platform` Plans **86–87**（rate-limit/JWKS + OpenAPI doc batch）；详见平台仓 `status.json` `metadata.tech_debt_summary.note` |
| Cross-repo gates | Canonical: `nexus-platform/.agents/knowledge/v1.14-program-compass-v1.md` §5 |

### V1.15+ horizon

| Category | Position |
|----------|----------|
| Program | **未登记** — 等待新 compass + `{PLAN_DIR}` plan；本 tracker §3.1/§3.2 继续承载长期 defer / backlog |

### Items targeting V2.0+

| Category | Count | IDs |
|----------|-------|-----|
| Features | 2 | DF-13 (Entitlements), DF-16 (Billing) |
| Tech-debt residuals | 0 | No V2.0-targeted rows in root `residual_findings` (2026-05-10); historical R5/R11/M1-W07 remain **knowledge / compass** follow-ups until re-filed |
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

- V1.2 delivery compass: [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md)
- V1.2 reclassification matrix: [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md)
- V1.3 delivery compass: [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md)
- V1.4 delivery compass: [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md)
- V1.5 delivery compass: [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md)
- V1.6 delivery compass: [v1.6-delivery-compass-v1.md](v1.6-delivery-compass-v1.md)
- V1.7 delivery compass: [v1.7-delivery-compass-v1.md](v1.7-delivery-compass-v1.md)
- V1.8 delivery compass: [v1.8-delivery-compass-v1.md](v1.8-delivery-compass-v1.md)
- V1.9 delivery compass: [v1.9-delivery-compass-v1.md](v1.9-delivery-compass-v1.md)
- V1.10 delivery compass: [v1.10-delivery-compass-v1.md](v1.10-delivery-compass-v1.md)
- V1.13 delivery compass: [v1.13-delivery-compass-v1.md](v1.13-delivery-compass-v1.md)
- V1.14 delivery compass: [v1.14-delivery-compass-v1.md](v1.14-delivery-compass-v1.md)
- Orchestration engine design: [../archived/knowledge/orchestration-engine-v1.md](../archived/knowledge/orchestration-engine-v1.md)
- ACP client tech spec v2: [../archived/knowledge/acp-client-tech-spec-v2.md](../archived/knowledge/acp-client-tech-spec-v2.md)
- Creator schedule & core context: [creator-schedule-and-core-context-v1.md](creator-schedule-and-core-context-v1.md)
- Crate selection best practices: [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md)
- `status.json` (machine-state residuals): [../status.json](../status.json)

External (v1-spec, resolved via `.agents/local-paths.json`):

- `{v1-spec}/architecture/v1.md` — base architecture
- `{platform-designs}/roadmap.md` — program roadmap

---

*Created: 2026-04-21. Last updated: **2026-05-10**. Status: Active. V1.13 DF-11/DF-14 shipped, DF-15 governance-closed. V1.14: **platform** Plans 86–87 **Done**; OSS `plans[]` 空；`residual_findings` 收敛为 **2** 条 accepted backlog（§3.3）；§3.1 Open 表移除已交付 DF-11/DF-14。*
