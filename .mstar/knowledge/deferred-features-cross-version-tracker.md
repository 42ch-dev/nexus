# Deferred Features — Cross-Version Tracker v2

**Quick status**: **V1.113 active** — i18n completion (P0/P1) + tech-debt paydown (P2). V1.112 Shipped (i18n foundation + UI migration). Platform **paused**.

**Purpose**: Single source of truth for **open** and **backlog** features deferred from delivery compasses. Closed/shipped history lives in shipped archive.
**Scope**: `nexus` OSS repository only.
**Created**: 2026-04-21 · **Last updated**: 2026-07-14 (DF-71 desktop menu-bar daemon status deferred from hotfix)

---

## 1) How to use

- **Product decisions**: §2.1 (PD-*)
- **Future product lines**: §2.2 (FL-*)
- **Planning a new version**: Scan §2.3 Open features for items targeting that version or "Any future"
- **Closing an item**: Remove its row from §2.3; append to [shipped archive](../archived/shipped-features-tracker.md)
- **Deferring again**: Update `Target` column; keep the row. Add a note.
- **Shipped/cancelled history**: [shipped archive](../archived/shipped-features-tracker.md)
- **Tech-debt residuals**: [`status.json`](../status.json) `residual_findings` — SSOT. Do not mirror here.
- **Conflict**: Compass wins over tracker; `status.json` wins over tracker for machine-state residuals.

---

## 2) Open items

### 2.1 Program planning decisions

| ID | Decision | Notes |
|----|----------|-------|
| PD-01 | **World fork is platform-only** | Community/social feature; **no** local `nexus42` CLI or daemon fork. |
| PD-05 | Cloud sync is **not** a short-term iteration focus | CLI `sync push/pull` unchanged; orchestration `sync.pull`/`sync.push` stubs remain Open. |
| PD-08 | Preset orchestration + Agentic Design Patterns | See FL-D. |

### 2.2 Future product lines (cross-version themes)

| ID | Product line | Suggested target | Notes |
|----|--------------|------------------|-------|
| FL-D | Preset orchestration (Agentic Design Patterns) | Post-V1.34 | V1.31–32 shipped capabilities + quality gate; DF-29/31/56 all since closed. Remaining: DF-03 (3P registry). |

### 2.3 Open features

| ID | Feature | First deferred | Target | Effort | History | Notes |
|----|---------|---------------|--------|--------|---------|-------|
| DF-13 | Entitlements API consumption | V1.3 | V2.0+ | M | V1.3 | Platform API dependency. |
| DF-16 | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2→V1.3 | ADR-011/012/013. Platform dependency. |
| DF-41 | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-46 | Full `nexus.*` capability implementation | V1.34 audit | **Reduced — V1.60 local complete** | L | V1.34→V1.60 | Local scope complete: 32 shipped + 4 sync.* catalog-only (platform-blocked) + 2 publish.* OUT (DF-59). Remaining 4 sync.* are platform-gated per PD-05. |
| DF-47 | Host tool + `worker/agent_tool_request` unified registry | V1.34 audit | V1.42 P3 Narrowed | M | V1.34→V1.42 | V1.42 P3 shipped `DaemonToolDispatchAdapter` + `HostToolCallTask` + one tool proven E2E. |
| DF-55 | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | Local/read-only or `policy_blocked` (PD-05). |
| DF-59 | Platform publish integration for novel | V1.36 prepare | Backlog | L | V1.36 | Platform dependency. |
| DF-70 | **App Settings shell — execution-mode matrix** (W2 Workspace shipped V1.104) | V1.101 | **V1.105+** (execution-mode deferred) | M | V1.102→V1.103→V1.104 | **V1.103 Must shipped**: S3 shell + Agent preselect + Connection + Re-run setup. **V1.104 Must shipped**: Workspace W2 (path view/change + honesty copy + nav/route). **Execution-mode matrix still deferred** (BYOK, API-key). Compass: [v1.104-delivery-compass.md](../iterations/v1.104-delivery-compass.md). |
| DF-71 | **Desktop menu-bar / status-bar daemon control** (macOS) | V1.116 hotfix | **Any future** (opportunistic desktop polish) | M | V1.116 | Show daemon Running/Stopped in the macOS menu bar; actions: open Control Room, stop/start daemon, quit shell. **Interim (shipped on hotfix branch)**: quit dialog — Stop Daemon & Quit / Keep Daemon & Quit / Cancel. Spec non-goal today: [desktop-shell.md](../specs/desktop-shell.md) §2. Pick when a desktop-polish slice has spare capacity; no wire/schema change. |
| FEAT-WASM-COMPUTE | **Programmable Narrative Progression** — WASM compute for timeline narrative | V1.61 | **Shipped (V1.61)** — V2 backlog | XL | V1.61 | Core differentiator shipped in V1.61: wasmtime + KB structured layer + `narrative.compute` + `combat-engine` preset. Compass: [v1.61-programmable-narrative-progression-delivery-compass-v1.md](../iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md). V2 deferred: Generic Combat Protocol, CDN distrib, 3P game bridge, marketplace, GPU/SIMD. |

### 2.4 Backlog (no committed target)

| ID | Feature | First deferred | Target | Effort | Notes |
|----|---------|---------------|--------|--------|-------|
| DF-03 | Preset third-party registry / signing / publish | V1.4 | Backlog | XL | Potentially independent project. |
| BL-01 | World Merge complete execution / rollback | V1.2 | Backlog | XL | |
| BL-02 | Local Shadow Read / staged change full chain | V1.2 | Backlog | L | |
| BL-03 | Advanced declarative Context Assembly API / DSL | V1.2 | Backlog | XL | |
| BL-04 | Long-running task checkpoint (product-level) | V1.2 | Backlog | M | |
| BL-05 | Commonware / multi-workspace advanced narrative | V1.2 | Backlog | XL | |
| BL-06 | Independent search microservice | V1.2 | Backlog | L | |
| BL-07 | Explore ranking / cold-start + Publish compliance matrix | V1.2 | Backlog | M | |
| BL-08 | Social / marketing features | V1.3 | V2.0+ | XL | |
| BL-09 | Standalone maturation dashboard (multi-chart cross-Work/World aggregate view) | V1.79 | Backlog | M | V1.79 Track A shipped in-context lightweight maturation indicators only; standalone dashboard deferred. |

### 2.5 Reliability roadmap (cross-version)

Non-feature reliability work routed out of feature iterations; picked up by a dedicated reliability iteration or opportunistically.

| ID | Item | Source | Target | Notes |
|----|------|--------|--------|-------|

---

## 3) Residuals (SSOT pointer)

Residual findings are tracked in [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary`. The tracker does **not** mirror residual rows — `status.json` is authoritative. Current state: 78 open residuals tracked in [`status.json`](../status.json) via `metadata.tech_debt_summary` (updated 2026-07-12). Closed/historical: `.mstar/archived/residuals/<plan-id>.json`.

---

## 4) Change control

- **Shipped rows**: Move from §2.3 to [shipped archive](../archived/shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **Effort estimates**: XS/S/M/L/XL agent-session scale. Guidance only.

---

## 5) Quick index

**Active iteration**: [V1.113](../iterations/v1.113-delivery-compass.md) (**active** — i18n completion + tech-debt paydown; `iteration/v1.113`).

**Latest shipped**: [V1.112](../iterations/v1.112-delivery-compass.md) (i18n foundation + UI migration; 2026-07-12).

**Full iteration index**: [iterations/README.md](../iterations/README.md)

**Shipped archive**: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)

**Machine state**: [`status.json`](../status.json)
