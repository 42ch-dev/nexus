# Deferred Features — Cross-Version Tracker v1

**Status**: Active
**Purpose**: Single source of truth for all features/tech-debt items that have been **deferred** from any delivery compass (V1.2–V1.6), with their lifecycle status across versions. This file enables version planning by showing what was promised, deferred, shipped, or cancelled — without reading every compass.
**Scope**: `nexus` OSS repository only. Platform features are referenced only when they block or depend on nexus-side work.
**Predecessor**: Consolidated from all delivery compasses (v1.2 through v1.6) and the v1.2 reclassification matrix.
**Created**: 2026-04-21

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
| DF-01 | Multi-agent worker (single worker hosting >1 ACP agent) | V1.4 | **V1.7** | XL | V1.4→V1.5+→V1.6+→V1.7+ | Requires ACP session multiplexing design. See `orchestration-engine-v1.md` §11 OQ-7. Two approaches: (a) multiplex one worker, (b) spawn sibling workers. |
| DF-04 | ACP SDK migration to sacp v1.0 | V1.4 | **V1.7** | L | V1.4 (not forced)→V1.6+→V1.7+ | V1.6 DTO decoupling was preparation. SDK v0.11.0 available (2026-04-20). Adapter-trait policy in `acp-client-tech-spec-v2.md`. |
| DF-08 | Wire/local drift auto-detect tooling | V1.6 | V1.8+ | M | V1.6→V1.7+ | Automated detection of schema classification drift. Per V1.6 WS5 OQ-S3. Worth doing. |
| DF-09 | Template_file path validation | V1.6 | V1.8+ | S | V1.6→V1.7+ | Filesystem preset path traversal protection. Must work with `~/.nexus/strategies/` third-party presets (see DF-17). First noted V1.4 WS3 QC S-1. Worth prioritizing. |
| DF-10 | WS4 Starting lifecycle edge cases | V1.6 | V1.8+ | M | V1.6→V1.7+ | HealthDegraded during Starting, Starting.exit in-flight cancel, ActionContext not used. Daemon lifecycle hardening. Worth doing. |
| DF-11 | CoreContext Handlebars template engine binding | V1.6 | V1.8+ | L | V1.6→V1.7+ | Data produced by V1.4 WS7, template rendering not yet integrated. Worth doing. |
| DF-17 | Third-party preset loading (`~/.nexus/strategies/`) + CLI init templates | V1.6+ insight | V1.8+ | M | New (from DF-06 cancellation insight) | Read user-config directory `~/.nexus/strategies/` for third-party presets. CLI command to scaffold custom strategy templates. Prerequisite for DF-09 path validation in third-party context. |
| DF-12 | Dual outbox consolidation (full merge) | V1.2 | Any future | L | V1.2 (no fixed milestone) | Batch D waived. Knowledge: `dual-outbox-architecture-v1.md`. Single-writer rule follow-up. |
| DF-13 | Entitlements API consumption (`/me/entitlements`, `/official-creator/quota`) | V1.3 | V2.0+ | M | V1.3 (not in V1.3) | Platform API dependency. |
| DF-14 | CLI+Platform e2e integration | V1.2 | V2.0+ | L | V1.2 (V1.3)→V1.3 (not in V1.3) | Cross-repo integration. |
| DF-15 | OpenAPI export work | V1.3 | V2.0+ | M | V1.3 (not in V1.3) | |
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

These are QC-found issues with a target version. See `status.json` → `metadata.residual_findings` for authoritative machine state.

| ID | Title | Severity | Target | Origin plan | Scope |
|----|-------|----------|--------|-------------|-------|
| DTO-C1 | `sdk_new_session_request_from_nexus` silently drops `mcp_servers` field | medium | **V1.7** | v1.6-ws-b-acp-sdk-dto | `crates/nexus-acp-host/src/client.rs` |
| DTO-C2 | Protocol version `.parse::<u16>().unwrap_or(1)` silent degradation | low | **V1.7** | v1.6-ws-b-acp-sdk-dto | `crates/nexus-acp-host/src/client.rs` |
| DTO-W1 | `NexusContentBlock` missing `Eq` derive | nit | **V1.7** | v1.6-ws-b-acp-sdk-dto | `crates/nexus-contracts/src/local/acp/types.rs` |
| PERM-W1 | `policy.rs` save re-serializes TOML losing comments/format | low | **V1.7** | v1.6-ws-c-permission-cli | `crates/nexus-acp-host/src/policy.rs` |
| PERM-W2 | JSON permission list omits global rules when agent filter used | low | **V1.7** | v1.6-ws-c-permission-cli | `crates/nexus42/src/commands/permission.rs` |
| PERM-W3 | Unvalidated TOML keys in permission commands | low | **V1.7** | v1.6-ws-c-permission-cli | `crates/nexus42/src/commands/permission.rs` |
| R4 | `SystemClock` DST safety not implemented; doc overstates guarantee | low | **V1.7** | v1.5-stabilization | `crates/nexus-orchestration/src/scheduler/mod.rs` |
| R7 | `schedule_guards` HashMap grows unbounded | nit | **V1.7** | v1.5-stabilization | `crates/nexus-orchestration/src/schedule/derivation.rs` |
| TD-10 | Device flow OAuth — production auth deferred; stub `verify_device_code` only | low | Backlog | v1-tech-debt-cleanup | `crates/nexus42d/src/auth/device_flow.rs` |
| R5 | `nix` crate unconditionally included — Windows build blocked | low | V2.0 | v1.5-stabilization | `crates/nexus-orchestration/Cargo.toml` |

> **Note**: `DEBT-RAND-073` (rand 0.7.3, blocked by wiremock) is **cancelled/accepted** — listed in §4 Closed.

---

## 4) Closed items

| ID | Feature | Status | Shipped in / Cancelled in | Notes |
|----|---------|--------|--------------------------|-------|
| ~~DF-A~~ | `context.summarize` capability (LLM-driven core_context summarisation) | **Shipped** | V1.5 (WS-C) | V1.4 reserved `DerivationKind::LlmSummarize`; V1.5 implemented the capability. |
| ~~DF-B~~ | Schedule cron / wall-clock triggers (`scheduled_at` column) | **Shipped** | V1.5 (WS-D) | V1.4 reserved column; V1.5 added clock poller zero-migration. |
| ~~DF-C~~ | System-managed multi-preset scheduler (`_system/` directory) | **Shipped** | V1.6 (WS-D / Track D) | V1.4 backlog → V1.5 deferred → V1.6 implemented. Configurable `_system.*` preset directory scanning. |
| ~~DF-D~~ | V1.5 residual R1 — cancel signal ignores `pause_schedule()` error | **Shipped** | V1.6 (WS-A) | Medium severity. Fixed in `nexus42d/.../schedules.rs`. |
| ~~DF-E~~ | V1.5 residual R2 — `resume_schedule()` TOCTOU race | **Shipped** | V1.6 (WS-A) | Medium severity. Fixed in `nexus-orchestration/.../supervisor.rs`. |
| ~~DF-F~~ | V1.5 residual R3 — `Scheduler::tick()` dead code | **Shipped** | V1.6 (WS-A) | Low severity. Removed redundant DB query path. |
| ~~DF-G~~ | V1.5 residual R6 — Recovered sessions lack FlowRunner | **Shipped** | V1.6 (WS-A) | Low severity. Session recovery after daemon restart fixed. |
| ~~DF-H~~ | ACP SDK DTO decoupling (Nexus-owned trait types) | **Shipped** | V1.6 (WS-B) | Nexus-owned DTOs for `NexusAcpClient` trait. SDK types confined to `AcpSdkAdapter`. Preparation for DF-04 full migration. |
| ~~DF-I~~ | ACP permission policy CLI surface (`nexus42 permission`) | **Shipped** | V1.6 (WS-C) | CLI command group: list/grant/deny/ask/revoke/reset. Web UI (DF-05) remains deferred. |
| ~~DF-J~~ | Full daemon lifecycle state machine (6-state FSM) | **Shipped** | V1.4 (WS4) | `statig` HSM. Originally deferred from V1.2 matrix (TD-9-FU). |
| ~~DF-K~~ | User registration / Creator binding full story | **Shipped** | V1.3 | V1.2 deferred to V1.3. Creator register CLI delivered. |
| ~~DF-L~~ | DEBT-RAND-073 — rand 0.7.3 blocked by wiremock/http-types | **Cancelled** | V1.6 (accepted) | Low impact, upstream dependency. Decision: accept as permanent tech debt. No further action. |
| ~~DF-M~~ | DF-07 — Capability schema registry sharing with platform | **Cancelled** | 2026-04-21 (V1.7 planning) | Over-designed. Nexus OSS built-in capabilities do not need platform registration. |
| ~~DF-N~~ | DF-02 — User-authored capabilities (shell / WASM plugin ABI) | **Cancelled** | 2026-04-21 (V1.7 planning) | Over-designed. If users need new capabilities, OSS code contributions are the proper channel — no plugin ABI needed. |
| ~~DF-O~~ | DF-05 — Full ACP permission policy engine UI (web-based) | **Cancelled** | 2026-04-21 (V1.7 planning) | Over-designed. ACP permission is not a core product value — ACP Session is a tool for orchestration, not a focus area. CLI surface shipped in V1.6 is sufficient. |
| ~~DF-P~~ | DF-06 — Preset hot-reload with in-flight session migration | **Superseded** | 2026-04-21 (V1.7 planning) | Snapshot semantics are the correct design (running sessions keep their snapshot; new sessions pick up changes). In-flight migration is unnecessary complexity. The real need is **DF-17** (third-party preset loading from `~/.nexus/strategies/` + CLI init templates). |

---

## 5) Per-version summary

### Items targeting V1.7

| Category | Count | IDs |
|----------|-------|-----|
| Features (from compass "Out") | 2 | DF-01, DF-04 |
| Tech-debt residuals | 8 | DTO-C1, DTO-C2, DTO-W1, PERM-W1, PERM-W2, PERM-W3, R4, R7 |
| **Total** | **10** | |

### Items targeting V1.8+ (or "Any future")

| Category | Count | Key IDs |
|----------|-------|---------|
| Features | 5 | DF-08, DF-09, DF-10, DF-11, DF-17 |
| Backlog | 9 | DF-03, BL-01 through BL-08 |
| **Total** | **14** | |

### Cancelled / Superseded (V1.7 planning)

| ID | Status | Reason |
|----|--------|--------|
| DF-02 | Cancelled | Over-designed; OSS contributions sufficient |
| DF-05 | Cancelled | ACP permission not core product value |
| DF-06 | Superseded | Snapshot semantics correct; real need → DF-17 |
| DF-07 | Cancelled | Over-designed; built-in capabilities don't need platform registration |

### Items targeting V2.0+

| Category | Count | IDs |
|----------|-------|-----|
| Features | 2 | DF-13 (Entitlements), DF-16 (Billing) |
| Features (platform-dependent) | 2 | DF-14 (e2e), DF-15 (OpenAPI) |
| Tech-debt residuals | 1 | R5 (nix crate Windows) |
| Backlog | 1 | BL-08 (Social/marketing) |
| **Total** | **6** | |

---

### Decision log (V1.7 planning, 2026-04-21)

| ID | Decision | Rationale |
|----|----------|-----------|
| DF-02 | **Cancelled** | Over-designed; users can contribute capabilities via OSS code contributions |
| DF-03 | **→ Backlog** (independent project) | Too large for a nexus version feature; potentially standalone |
| DF-05 | **Cancelled** | ACP permission is not core value — ACP Session is an orchestration tool, not a product focus |
| DF-06 | **Superseded** | Snapshot semantics are correct; real need is DF-17 (`~/.nexus/strategies/` loading + CLI init) |
| DF-07 | **Cancelled** | Over-designed; built-in capabilities don't need platform registration |
| DF-08 | Keep (worth doing) | Schema drift detection |
| DF-09 | Keep (prioritize) | Must work with `~/.nexus/strategies/` (DF-17) |
| DF-10 | Keep (worth doing) | Daemon lifecycle hardening |
| DF-11 | Keep (worth doing) | Handlebars binding for CoreContext |

---

## 6) Change control

- **Updates**: When a version ships, move all delivered items to §4 Closed. When an item is re-deferred, update §3.
- **Source compasses remain authoritative for scope decisions**: If the active compass says "Out" for an item but this tracker has it as "Open" with that version target, the compass controls whether it enters scope.
- **Effort estimates are approximate** (XS/S/M/L/XL agent-session scale) and for planning guidance only — not contractual. See `effort-estimation.md` for methodology.
- **Residual detail**: Machine-state residuals (§3.3) are authoritative in `status.json` → `metadata.residual_findings`. This file mirrors them for cross-version planning convenience; if there's a conflict, `status.json` wins.

---

## 7) Related index

Internal (this repo):

- V1.2 delivery compass: [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md)
- V1.2 reclassification matrix: [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md)
- V1.3 delivery compass: [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md)
- V1.4 delivery compass: [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md)
- V1.5 delivery compass: [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md)
- V1.6 delivery compass: [v1.6-delivery-compass-v1.md](v1.6-delivery-compass-v1.md)
- Orchestration engine design: [../archived/knowledge/orchestration-engine-v1.md](../archived/knowledge/orchestration-engine-v1.md)
- ACP client tech spec v2: [../archived/knowledge/acp-client-tech-spec-v2.md](../archived/knowledge/acp-client-tech-spec-v2.md)
- Creator schedule & core context: [creator-schedule-and-core-context-v1.md](creator-schedule-and-core-context-v1.md)
- Crate selection best practices: [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md)
- `status.json` (machine-state residuals): [../status.json](../status.json)

External (v1-spec, resolved via `.agents/local-paths.json`):

- `{v1-spec}/architecture/v1.md` — base architecture
- `{platform-designs}/roadmap.md` — program roadmap

---

*Created: 2026-04-21. Status: Active. Review when any version ships or items are re-deferred.*
