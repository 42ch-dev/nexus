# Deferred Features — Cross-Version Tracker v1

**Quick status**: **V1.31 Active** (FL-D Agentic Design Patterns) · **V1.30 Shipped** · Platform **paused** · Open DF rows targeting V1.31: **DF-30, DF-32–34, DF-37** · Residual SSOT: `status.json` (11 backlog items)

**Status**: Active  
**Purpose**: Single source of truth for **open** and **backlog** features/tech-debt deferred from delivery compasses. Closed/shipped history lives in [shipped-features-tracker.md](../archived/knowledge/shipped-features-tracker.md).  
**Scope**: `nexus` OSS repository only. Platform features referenced only when they block nexus-side work.  
**Predecessor**: Consolidated from delivery compasses (v1.2–v1.21) and the v1.2 reclassification matrix.  
**Created**: 2026-04-21  
**Last updated**: 2026-05-30

---

## 1) How to use this file

- **Product decisions (not deferrals)**: See §3.1 Program planning decisions (PD-*).
- **Future product lines (cross-version themes)**: See §3.2 Future product lines (FL-*).
- **Planning a new version**: Scan §3.3 Open features for items targeting that version or "Any future".
- **Closing an item**: Remove its row from §3.3; append to [shipped-features-tracker.md](../archived/knowledge/shipped-features-tracker.md) with completion version, plan-id, and note.
- **Deferring again**: Update the `Target` column; keep the row in §3.3. Add a note in `Deferral history`.
- **Shipped / cancelled history**: [shipped-features-tracker.md](../archived/knowledge/shipped-features-tracker.md) (§1 closed items, §2 per-version snapshots).
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

Recorded product rulings for iteration planning. **Not** implementation tasks — the active delivery compass is scope authority.

| ID | Decision | Notes |
|----|----------|-------|
| PD-01 | **World fork is platform-only** | Community/social feature; **no** local `nexus42` CLI or daemon fork. See DF-45 (Cancelled) in archive. |
| PD-02 | V1.28 primary product = structured KB query + context assembly convergence | Compass: [v1.28-context-and-agent-host-delivery-compass-v1.md](../iterations/v1.28-context-and-agent-host-delivery-compass-v1.md) |
| PD-03 | V1.28 mandatory = local SSOT doc refresh | Plan: `2026-05-25-v1.28-local-ssot-refresh` |
| PD-04 | Agent Host (ACP + native CLI) = local product P0–P1 | V1.28 Batch 1 + V1.29 Batch 2 shipped |
| PD-05 | Cloud sync is **not** a short-term iteration focus | CLI `sync push/pull` unchanged; orchestration `sync.pull`/`sync.push` stubs remain Open |
| PD-06 | Memory + SOUL deep build | **Shipped V1.29** (FL-A) |
| PD-07 | Writing-process KB extraction | **Shipped V1.29** (FL-B) |
| PD-08 | Preset orchestration + Agentic Design Patterns | See FL-D; research: https://github.com/evoiz/Agentic-Design-Patterns |
| PD-09 | V1.29 primary = **Author Intelligence Loop** (FL-A + FL-B) | Shipped V1.29 |
| PD-10 | FL-B **agent-driven**: CLI queue/status only; LLM via preset + capability | Shipped V1.29 |
| PD-11 | V1.29 secondary = **Agent Host Batch 2** | Shipped V1.29 |
| PD-12 | **V1.31 primary = FL-D Agentic Design Patterns** | De-stub orchestration capabilities (DF-30, DF-32–34, DF-37); 2 demonstrator presets; **out**: DF-29, DF-31, conditional routing engine. Compass: [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md) |

### 3.2 Future product lines (planning backlog)

Cross-version themes. Suggested targets are non-binding until locked in a compass.

| ID | Product line | Suggested target | Notes |
|----|--------------|------------------|-------|
| FL-A | Creator **Memory + SOUL** build-out | **V1.29** ✅ Shipped | Session review, Experience preset, Stage0 delimiters |
| FL-B | **KB extraction** from writing (work index → World KB) | **V1.29** ✅ Shipped | CLI queue + `kb.extract_work` preset; partial DF-35/36 |
| FL-D | **Preset orchestration** (Agentic Design Patterns) | **V1.31** 🔄 Active | Partial scope: judge/creator/summarize de-stub + 2 presets; DF-29/31 remain deferred post-V1.31 |

### 3.3 Open features (deferred from compass "Out" or audit)

| ID | Feature | First deferred | Target | Effort | Deferral history | Notes |
|----|---------|---------------|--------|--------|-----------------|-------|
| DF-12 | Dual outbox consolidation (full merge) | V1.2 | Any future | L | V1.2 | Knowledge: `dual-outbox-architecture.md` (archived). Single-writer follow-up. |
| DF-13 | Entitlements API consumption | V1.3 | V2.0+ | M | V1.3 | Platform API dependency. |
| DF-16 | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2→V1.3 | ADR-011/012/013. Platform dependency. |
| DF-29 | `registry.refresh` (synthetic output) | V1.21 audit | Any future | M | V1.21 | **Out of V1.31** — needs network/CDN. `builtins/registry.rs`. |
| DF-30 | `creator.read_memory` / `write_memory` / `inject_prompt` stubs | V1.21 audit | **V1.31** | M | V1.21 → V1.31 plan `creator-memory-capabilities` | Wire to `nexus-creator-memory`; prompt injection queue. |
| DF-31 | `workspace.open` / `workspace.commit` stubs | V1.21 audit | Any future | M | V1.21 | **Out of V1.31** — `nexus-home-layout` wiring deferred. |
| DF-32 | `judge.rule` (only `always_true`/`always_false`) | V1.21 audit | **V1.31** | S | V1.21 → V1.31 plan `judge-and-summarize-capabilities` | Real expression engine over `contextData`. |
| DF-33 | `judge.llm` (heuristic on prompt text) | V1.21 audit | **V1.31** | S | V1.21 → V1.31 plan `judge-and-summarize-capabilities` | Real via `acp.prompt` deny_all + parse. |
| DF-34 | `context.summarize` (`[SUMMARIZE_STUB]` marker) | V1.21 audit | **V1.31** | M | V1.21 → V1.31 plan `judge-and-summarize-capabilities` | Real LLM via `acp.prompt`. |
| DF-37 | InnerGraphNodeTask / AcpPromptTask stub fallback | V1.21 audit | **V1.31** | S | V1.21 → V1.31 plan `judge-and-summarize-capabilities` | Worker-handle plumbing; fold with judge/summarize IPC. |
| DF-40 | Session resume stub in daemon lifecycle | V1.21 audit | Any future | S | V1.21 | `daemon-runtime/lifecycle/actions.rs`. |
| DF-41 | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-42 | Full Local API redesign for World/User KB | V1.24 (KCA-003) | Any future | L | V1.24 | `/v1/local/kb/*` full scoping redesign. |
| DF-43 | SQLite persistence / crate-model alignment | V1.24 audit | Any future | M | V1.26–28 partial | Production owner = `nexus-local-db`; see decision note below. |
| DF-44 | Reference body externalization — refreshable scan pipeline | V1.26 | Any future | M | V1.26 | Static registration shipped; auto-refresh Open. |

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
| BL-09 | V1.17 Prompt + Skills Compass v1 | V1.16 | Done | M | **Shipped V1.17** — see archive §2. |

### 3.5 Open tech-debt residuals (tracked in `status.json`)

Authoritative machine state: **`status.json` root `residual_findings`** (`updated_at` **2026-05-26**). `metadata.tech_debt_summary.total_open` = **11** (TD-V130-01..11 + historical R-V113-005/007).

| ID | Title | Severity | Decision | `target_date` | Scope |
|----|-------|----------|----------|----------------|-------|
| R-V113-005 | UpstreamTimeout e2e test duration varies by OS/proxy | low | accept | backlog | `crates/nexus42/tests/creator_register_e2e.rs` |
| R-V113-007 | Flaky test `auth::tests::get_returns_none_for_unknown_creator` | low | accept | backlog | `crates/nexus42/src/auth/mod.rs` |
| TD-V130-01 | SessionCapture RwLock uses write() for all accesses | low | accept | backlog | `crates/nexus42/src/commands/acp_worker/mod.rs` |
| TD-V130-02 | cleanup_row fire-and-forget — DELETE failure silently lost | low | defer | backlog | `crates/nexus-local-db/src/reference_source.rs` |
| TD-V130-03 | JobLifecycleGuard FSM — no RAII guard | low | defer | backlog | `crates/nexus-orchestration/.../kb_extract_work.rs` |
| TD-V130-04 | Drop timeout 150ms may be insufficient | low | defer | backlog | `crates/nexus-agent-host/.../claude.rs` |
| TD-V130-05 | LIST_BY_WORLD_LIMIT=500 silently truncates | low | accept | backlog | `crates/nexus-local-db/src/kb_store.rs` |
| TD-V130-06 | mark_running lacks WHERE status guard | low | defer | backlog | `crates/nexus-local-db/src/kb_extract_job.rs` |
| TD-V130-07 | claim_job re-fetches row after commit | nit | accept | backlog | `crates/nexus-local-db/src/kb_extract_job.rs` |
| TD-V130-08 | insert_with_retry generic error on collision | nit | accept | backlog | `crates/nexus-local-db/src/kb_extract_job.rs` |
| TD-V130-09 | Dynamic SQL (format!) not parameterized | low | accept | backlog | `reference_source.rs`, `kb_store.rs` |
| TD-V130-10 | Extraction prompt format!() doubles memory | nit | accept | backlog | `kb_extract_work.rs` |
| TD-V130-11 | sqlx prepare CI enforcement | low | defer | backlog | CI pipeline |

V1.30 residuals R5–R20 closed — see `archived/residuals/v1.30-residual-convergence.json`.

---

## 4) Change control

- **Shipped rows**: Move from §3.3 to [shipped-features-tracker.md](../archived/knowledge/shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **Effort estimates**: XS/S/M/L/XL agent-session scale; guidance only. See `effort-estimation.md`.
- **Residual detail**: `status.json` wins over this file for machine-state residuals.

---

## 5) Related index

**Active iteration**

- V1.31 delivery compass: [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)

**Recent shipped compasses** (detail in archive §2)

- V1.30: [v1.30-residual-convergence-delivery-compass-v1.md](../iterations/v1.30-residual-convergence-delivery-compass-v1.md)
- V1.29: [v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md](../iterations/v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md)
- V1.28: [v1.28-context-and-agent-host-delivery-compass-v1.md](../iterations/v1.28-context-and-agent-host-delivery-compass-v1.md)

**Knowledge & specs**

- Shipped history archive: [shipped-features-tracker.md](../archived/knowledge/shipped-features-tracker.md)
- Orchestration engine: [specs/orchestration-engine.md](specs/orchestration-engine.md)
- Creator schedule & core context: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)
- Iteration index: [iterations/README.md](../iterations/README.md)
- Machine state: [status.json](../status.json)

External (via `.agents/local-paths.json`): `{v1-spec}/architecture/v1.md`, `{platform-designs}/roadmap.md`

---

*Created: 2026-04-21. Last updated: **2026-05-30**. Status: Active. **V1.31 Active**. **V1.30 Shipped** (2026-05-26). Platform integration paused.*
