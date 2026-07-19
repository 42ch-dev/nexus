# Deferred Features — Cross-Version Tracker v2

**Quick status**: **V1.123 active** — Three-Layer Timeline (Brief · Narrative · Moment) + World/Work layer split + Timeline-first IA deepening. V1.122 Shipped (three-pillar pivot + Timeline-first World entry). Platform **paused**.

**Purpose**: Single source of truth for **open** and **backlog** features deferred from delivery compasses. Closed/shipped history lives in shipped archive.
**Scope**: `nexus` OSS repository only.
**Created**: 2026-04-21 · **Last updated**: 2026-07-18 (V1.123 Phase 1 product-manager seat 1 — DF-V1123-* inventory + DF-V1122-DEEPER-WB status note)

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

> **Pillar framing (V1.122 re-home):** the `Pillar` column maps each item to the three product pillars — **Harness** (control strategy / orchestration), **Canvas** (spatial steering, incl. Timeline-first World building), **Computable** (WASM reactivity) — or **Cross-cutting** (platform / desktop / infra). Pillar definitions: [`STRATEGY.md`](../../STRATEGY.md) + [`CONCEPTS.md`](../../CONCEPTS.md). Re-homing preserves all DF-*/BL-* IDs; only the framing column + V1.122-deferred rows are added. Archived rows untouched.

| ID | Pillar | Feature | First deferred | Target | Effort | History | Notes |
|----|--------|---------|---------------|--------|--------|---------|-------|
| DF-13 | Cross-cutting | Entitlements API consumption | V1.3 | V2.0+ | M | V1.3 | Platform API dependency. |
| DF-16 | Cross-cutting | Stripe / billing integration | V1.2 | V2.0+ | L | V1.2→V1.3 | ADR-011/012/013. Platform dependency. |
| DF-41 | Harness | Agent slot ACP connection stub | V1.7 audit | Any future | S | V1.7 | `nexus42/.../agent_slot.rs`. |
| DF-46 | Harness | Full `nexus.*` capability implementation | V1.34 audit | **Reduced — V1.60 local complete** | L | V1.34→V1.60 | Local scope complete: 32 shipped + 4 sync.* catalog-only (platform-blocked) + 2 publish.* OUT (DF-59). Remaining 4 sync.* are platform-gated per PD-05. |
| DF-47 | Harness | Host tool + `worker/agent_tool_request` unified registry | V1.34 audit | V1.42 P3 Narrowed | M | V1.34→V1.42 | V1.42 P3 shipped `DaemonToolDispatchAdapter` + `HostToolCallTask` + one tool proven E2E. |
| DF-55 | Cross-cutting | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | Local/read-only or `policy_blocked` (PD-05). |
| DF-59 | Cross-cutting | Platform publish integration for novel | V1.36 prepare | Backlog | L | V1.36 | Platform dependency. |
| DF-70 | Harness | **App Settings shell — execution-mode matrix** (W2 Workspace shipped V1.104) | V1.101 | **V1.105+** (execution-mode deferred) | M | V1.102→V1.103→V1.104 | **V1.103 Must shipped**: S3 shell + Agent preselect + Connection + Re-run setup. **V1.104 Must shipped**: Workspace W2 (path view/change + honesty copy + nav/route). **Execution-mode matrix still deferred** (BYOK, API-key). Compass: [v1.104/delivery-compass.md](../iterations/v1.104/delivery-compass.md). |
| DF-71 | Cross-cutting | **Desktop menu-bar / status-bar daemon control** (macOS) | V1.116 hotfix | **Any future** (opportunistic desktop polish) | M | V1.116 | Show daemon Running/Stopped in the macOS menu bar; actions: open Control Room, stop/start daemon, quit shell. **Interim (shipped on hotfix branch)**: quit dialog — Stop Daemon & Quit / Keep Daemon & Quit / Cancel. Spec non-goal today: [desktop-shell.md](../specs/desktop-shell.md) §2. Pick when a desktop-polish slice has spare capacity; no wire/schema change. |
| FEAT-WASM-COMPUTE | Computable | **Programmable Narrative Progression** — WASM compute for timeline narrative | V1.61 | **Shipped (V1.61)** — V2 backlog | XL | V1.61 | Core differentiator shipped in V1.61: wasmtime + KB structured layer + `narrative.compute` + `combat-engine` preset. Compass: [v1.61/delivery-compass.md](../iterations/v1.61/delivery-compass.md). V2 deferred: Generic Combat Protocol, CDN distrib, 3P game bridge, marketplace, GPU/SIMD. |
| DF-V1122-HARNESS-RENAME | Harness | Strategy/Preset → Harness product copy (breaking UX rename) | V1.122 | V1.124+ | M | V1.122 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal; still deferred in [V1.123 compass](../iterations/v1.123/delivery-compass.md). **Owner:** product-manager. **Trigger:** V1.123 three-layer ship + copy audit. UI strings stay "Strategy" until this lands. |
| DF-V1122-COMPUTABLE-UI | Computable | Computable pillar UI surfacing (compute registry / canvas marketing) | V1.122 | V1.124+ | M | V1.122 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal; still deferred in V1.123. **Owner:** product-manager. **Trigger:** dogfood shows authors cannot discover compute. |
| DF-V1122-COMPUTE-ON-TIMELINE | Computable + Canvas | Invoke WASM compute from the Timeline surface | V1.122 | V1.124+ | L | V1.122 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal; still deferred in V1.123. **Owner:** architect. **Trigger:** FEAT-WASM-COMPUTE V2 follow-ons + three-layer Timeline stable. Related: FEAT-WASM-COMPUTE V2 backlog. |
| DF-V1122-FORK-UI | Canvas | Fork creation + fork-merge authoring UI | V1.122 | V1.124+ | L | V1.122 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal; still deferred in V1.123. **Owner:** product-manager. **Trigger:** authors need alternate-history editing, not just read-only Fork-badge chrome. |
| DF-V1122-DEEPER-WB | Canvas | Deeper World-building on Timeline (richer projection, multi-timeline, World-scoped `TimelineEvent` HTTP route `GET /v1/daemon/worlds/{world_id}/timeline`) | V1.122 | V1.124+ (route slice) / V1.125+ remainder | L | V1.122→V1.123 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal. **V1.123 status (architect seat 2 LOCKED 2026-07-18):** Brief + Narrative layers on World Timeline compose from the existing `GET /v1/daemon/worlds/{world_id}/kb/graph` route (Brief = `block_type=era` KeyBlock; Narrative = V1.122 `block_type=event` KeyBlock). The route promotion `GET /v1/daemon/worlds/{world_id}/timeline` is **not required** for V1.123 PMF and stays deferred to V1.124+ — see [`iterations/v1.123/specs/three-layer-architecture.md`](../iterations/v1.123/specs/three-layer-architecture.md) §5. Multi-timeline remainder split to `DF-V1123-MULTI-TIMELINE`. **Owner:** architect. **Trigger:** V1.124+ author demand for World-scoped `TimelineEvent` row access via HTTP (causality graph, fork-marker progression, publish-marker history) that the KeyBlock-graph composition cannot satisfy. |
| DF-V1122-V1121-RES | Cross-cutting | V1.121 15 low/nit design-elevation residuals | V1.122 | V1.124 polish | S | V1.122 | Still deferred under V1.123 Non-Goals (not business scope). **Owner:** frontend-dev. **Trigger:** capacity after three-layer ship. **SSOT:** [`status.json`](../status.json) `residual_findings` — do not mirror detail here. Related rollup: `DF-V1123-RESIDUAL-CLEANUP`. |
| DF-V1122-STATUS-COMPACT | Cross-cutting | `status.json` size hygiene (<20 KB per `.mstar/AGENTS.md`) | V1.122 | Opportunistic / pre-P-last | S | V1.122 | Superseded for tracking by `DF-V1123-STATUS-COMPACT` (same hygiene gate). Keep row until compaction ships; then archive both. **Owner:** project-manager. |
| DF-V1123-WORLD-MOMENT | Canvas | World Timeline Moment layer (scene-precision within World history) | V1.123 | V1.124+ | M | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal / Deferred inventory. **Owner:** product-manager. **Trigger:** authors need scene-precision when reading world history, not only when writing Works. |
| DF-V1123-WORK-BRIEF | Canvas | Work Timeline Brief layer (world-shape projection for a Work) | V1.123 | V1.124+ | M | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal. **Owner:** product-manager. **Trigger:** authors need Work-level world-shape context beyond Outline. |
| DF-V1123-ERA-TAXONOMY | Canvas | Rich era taxonomy for Brief layer (kingdoms, ages, sub-ages; not just era markers) | V1.123 | V1.124+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. **Owner:** product-manager. **Trigger:** Brief MVP proves the abstraction; richer taxonomy needed. |
| DF-V1123-MULTI-TIMELINE | Canvas | Multiple parallel Timelines per World (alternate-history branches beyond Fork) | V1.123 | V1.125+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. **Owner:** architect. **Trigger:** authors need branch comparison beyond Fork semantics. May absorb multi-timeline remainder of `DF-V1122-DEEPER-WB`. |
| DF-V1123-GLOBAL-TIMELINE-MERGE | Canvas | Cross-World Timeline merge (read-write merged view, not read-only overview) | V1.123 | V1.125+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. P3 global Timeline is **read-only overview** only. **Owner:** product-manager. **Trigger:** P3 overview proves valuable; merge needed for cross-World narrative. |
| DF-V1123-CROSS-SURFACE-BINDING | Canvas | Cross-surface Timeline event binding (Work Timeline Moment ↔ World Timeline Narrative) — data link + UX | V1.123 | V1.124+ | S | V1.123 | P3 ships UX hints for cross-surface navigation but no formal data binding between Work events and World events. Track for V1.124+ data-binding iteration. QC1 R-V1123P0QC1-M001. |
| DF-V1123-RESIDUAL-CLEANUP | Cross-cutting | V1.121 + V1.122 + V1.123 low/nit residuals cleanup | V1.123 | V1.124 polish | M | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal as business scope. **Owner:** frontend-dev. **Trigger:** capacity after three-layer ship. **SSOT:** [`status.json`](../status.json) `residual_findings` — do not mirror detail here. |
| DF-V1123-STATUS-COMPACT | Cross-cutting | `status.json` size hygiene (<20 KB per `.mstar/AGENTS.md`) | V1.123 | Opportunistic / pre-P-last | S | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal (harness hygiene, not a business plan). **Owner:** project-manager. **Trigger:** before any P-last close when `wc -c .mstar/status.json` ≥ 20 KB. |

### 2.4 Backlog (no committed target)

| ID | Pillar | Feature | First deferred | Target | Effort | Notes |
|----|--------|---------|---------------|--------|--------|-------|
| DF-03 | Harness | Preset third-party registry / signing / publish | V1.4 | Backlog | XL | Potentially independent project. |
| BL-01 | Canvas | World Merge complete execution / rollback | V1.2 | Backlog | XL | |
| BL-02 | Cross-cutting | Local Shadow Read / staged change full chain | V1.2 | Backlog | L | |
| BL-03 | Cross-cutting | Advanced declarative Context Assembly API / DSL | V1.2 | Backlog | XL | |
| BL-04 | Cross-cutting | Long-running task checkpoint (product-level) | V1.2 | Backlog | M | |
| BL-05 | Canvas | Commonware / multi-workspace advanced narrative | V1.2 | Backlog | XL | |
| BL-06 | Cross-cutting | Independent search microservice | V1.2 | Backlog | L | |
| BL-07 | Cross-cutting | Explore ranking / cold-start + Publish compliance matrix | V1.2 | Backlog | M | |
| BL-08 | Cross-cutting | Social / marketing features | V1.3 | V2.0+ | XL | |
| BL-09 | Canvas | Standalone maturation dashboard (multi-chart cross-Work/World aggregate view) | V1.79 | Backlog | M | V1.79 Track A shipped in-context lightweight maturation indicators only; standalone dashboard deferred. |

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

**Active iteration**: [V1.123](../iterations/v1.123/delivery-compass.md) (**active** — Three-Layer Timeline Brief · Narrative · Moment; World/Work layer split; `iteration/v1.123`).

**Latest shipped**: [V1.122](../iterations/v1.122/delivery-compass.md) (three-pillar pivot + Timeline-first World entry; 2026-07-18).

**Full iteration index**: [iterations/README.md](../iterations/README.md)

**Shipped archive**: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)

**Machine state**: [`status.json`](../status.json)
