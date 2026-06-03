# Deferred Features — Cross-Version Tracker v1

**Quick status**: **V1.32 Active planning** · Latest shipped: **V1.31** · Platform **paused** · Open FL-D deferrals: **DF-29, DF-31** · Tech debt SSOT: [`status.json`](../status.json) (`total_open`: 20; `SEC-V131-01` targeted by V1.32)

**Status**: Active  
**Purpose**: Single source of truth for **open** and **backlog** features/tech-debt deferred from delivery compasses. Closed/shipped history lives in [shipped-features-tracker.md](../archived/shipped-features-tracker.md).  
**Scope**: `nexus` OSS repository only. Platform features referenced only when they block nexus-side work.  
**Predecessor**: Consolidated from delivery compasses (v1.2–v1.21) and the v1.2 reclassification matrix.  
**Created**: 2026-04-21  
**Last updated**: 2026-06-03

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
| FL-D | **Preset orchestration** (Agentic Design Patterns) | Post-V1.31 | V1.32 active planning targets preset validator quality gate only; **still open**: DF-29, DF-31, conditional routing engine |

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

**Machine state**: [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary` (`status.json.updated_at` **2026-06-03**; `tech_debt_summary.updated_at` **2026-05-30**). Do **not** mirror full rows here — JSON wins on conflict.

| Bucket | Open count | `residual_findings` key |
|--------|------------|-------------------------|
| V1.30 post-QC | 11 | `v1.30-post-qc-tech-debt` |
| V1.31 post-QC | 9 | `v1.31-post-qc-tech-debt` (incl. **SEC-V131-01** → V1.32+) |
| **Total** | **20** | See `metadata.tech_debt_summary.total_open` |

**Closed / historical residuals**

- V1.30 convergence (R5–R20 fixed): [`archived/residuals/v1.30-residual-convergence.json`](../archived/residuals/v1.30-residual-convergence.json)
- V1.13 forward delivery (R-V113-005 waived, R-V113-007 resolved): [`archived/residuals/2026-05-06-v1.13-oss-forward-delivery.json`](../archived/residuals/2026-05-06-v1.13-oss-forward-delivery.json)
- Cross-cutting accept items (e.g. DEBT-RAND-073): `status.json` → `metadata.tech_debt_summary.cross_cutting`

---

## 4) Change control

- **Shipped rows**: Move from §3.3 to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **Effort estimates**: XS/S/M/L/XL agent-session scale; guidance only. See `effort-estimation.md`.
- **Residual detail**: `status.json` wins over this file for machine-state residuals.

---

## 5) Related index

**Latest shipped iteration**

- V1.31 delivery compass: [v1.31-agentic-design-patterns-delivery-compass-v1.md](../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)

**Latest active iteration**

- V1.32 delivery compass: [v1.32-preset-quality-gate-delivery-compass-v1.md](../iterations/v1.32-preset-quality-gate-delivery-compass-v1.md)

**Recent shipped compasses** (detail in archive §2)

- V1.30: [v1.30-residual-convergence-delivery-compass-v1.md](../iterations/v1.30-residual-convergence-delivery-compass-v1.md)
- V1.29: [v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md](../iterations/v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md)
- V1.28: [v1.28-context-and-agent-host-delivery-compass-v1.md](../iterations/v1.28-context-and-agent-host-delivery-compass-v1.md)

**Knowledge & specs**

- Shipped history archive: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)
- Done plans index: [archived/plans-done.json](../archived/plans-done.json)
- Orchestration engine: [specs/orchestration-engine.md](specs/orchestration-engine.md)
- Creator schedule & core context: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)
- Iteration index: [iterations/README.md](../iterations/README.md)
- Machine state: [status.json](../status.json)

External (via `.agents/local-paths.json`): `{v1-spec}/architecture/v1.md`, `{platform-designs}/roadmap.md`

---

*Last updated: 2026-06-03. Status: Active.*
