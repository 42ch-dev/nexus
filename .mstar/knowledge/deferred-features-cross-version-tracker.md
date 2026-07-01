# Deferred Features — Cross-Version Tracker v2

**Quick status**: **V1.79 delivered (2026-07-01)** — Author Reflection: Manuscript Reading Surface (Track A) + SOUL Personality Visualization (Track B), the first post-loop-closure iteration. Track A (P0) promoted the post-V1.75-pivot residual into a designed reading surface (typography + nav + session-only progress + lightweight maturation indicators: completion-state badge + World KB density + open-findings "N+" via `has_more`). Track B (P1) added keyword clusters + temporal-drift visualization over internalized memory fragments; additive wire DTO (`memory-fragment-info` + `keywords`/`created_at`) → `@42ch/nexus-contracts` 0.13.0 → 0.14.0. QC tri-review 3/3 Approve both tracks (P0 after a fix-wave for pagination correctness). **DF-49 (Standalone MCP server) CANCELLED** — conflicts with ACP-client product direction (`STRATEGY.md`: CLI is an ACP client, not a server) + circular-invocation risk. Platform **paused**. Residuals SSOT: [`status.json`](../status.json). Shipped/cancelled history: [shipped-features-tracker.md](../archived/shipped-features-tracker.md).

**Purpose**: Single source of truth for **open** and **backlog** features deferred from delivery compasses. Closed/shipped history lives in shipped archive.
**Scope**: `nexus` OSS repository only.
**Created**: 2026-04-21 · **Last updated**: 2026-07-01 (V1.79 delivered: Author Reflection — manuscript reading surface + SOUL personality visualization; DF-49 cancelled — conflicts with ACP-client product direction; R-V178P0-QC3-001 closed via pretypecheck; R-V178P0-QC3-003 → reliability roadmap; 4 low V1.79-QC residuals → V1.80+)

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
| DF-49 | ~~Standalone MCP server for Nexus capabilities~~ **CANCELLED (V1.79)** | V1.34 | Cancelled | L | V1.34→V1.79 cancelled | **Cancelled — conflicts with ACP-client product direction** (`STRATEGY.md`: CLI is an ACP client, not a server) + circular-invocation risk (Nexus drives an agent via ACP → agent calls back into Nexus via MCP → loop). Not deferred. See archived shipped-features-tracker cancelled section. |
| DF-55 | `nexus.context.assemble` cloud/platform path | V1.34 | V2.0+ | M | V1.34 | Local/read-only or `policy_blocked` (PD-05). |
| DF-59 | Platform publish integration for novel | V1.36 prepare | Backlog | L | V1.36 | Platform dependency. |
| PF-ESSAY | `essay` Work profile | V1.52 lock | **V1.63 Shipped** | M | V1.52→V1.63 | Shipped: scaffold + `essay-writing` preset + 4-dim rubric + completion + optional KB. Spec: [essay-profile.md](specs/essay-profile.md) (Draft → Shipped V1.63). |
| PF-GAME-BIBLE | `game-bible` Work profile | V1.52 lock | **V1.55 P2 (Master)** | L | V1.52→V1.55 | Shipped Depth 3.5: `design-writing` + 五问 + section completion + KB extraction. Spec: [game-bible-profile.md](specs/game-bible-profile.md). |
| PF-SCRIPT | `script` Work profile | V1.52 lock | **V1.60 P1 (Master)** | L | V1.52→V1.55→V1.60 | V1.55 scaffold; V1.60 Depth 3.5: `script-writing` preset + 五问 + completion. Spec: [script-profile.md](specs/script-profile.md). |
| FEAT-WASM-COMPUTE | **Programmable Narrative Progression** — WASM compute for timeline narrative | V1.61 | **V1.61 (Prepare active)** | XL | V1.61 | Core differentiator: wasmtime + KB structured layer (attributes/state/computable) + `narrative.compute` capability + `combat-engine` preset + `basic-combat` sample. 6 plans, 4 waves. Canvas: `canvases/programmable-narrative-progression.canvas.tsx`. Compass: [v1.61-programmable-narrative-progression-delivery-compass-v1.md](../iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md). V2 deferred: Generic Combat Protocol, CDN distrib, 3P game bridge, marketplace, GPU/SIMD. |
| FEAT-WORLD-KB-RELATIONSHIPS | World KB relationships surface (`world_kb.patch_relationship` + `kb_relationships` table) | V1.73 | **V1.74 Shipped** | L | V1.73→V1.74 | Shipped: typed relationship β — hybrid taxonomy (`WorldKbRelationshipKind` core enum + `custom_label`) + directed/`symmetric` single-row semantics + single `world_kb.patch_relationship` route (add/update/remove, per-row OCC on `kb_relationships.revision`) + `GET graph` populates `relationships[]` (symmetric reverse auto-projected) + anchors-optional + confidence display-only. `@42ch/nexus-contracts` 0.9.0 → 0.10.0. Compass: [v1.74-...compass-v1.md](../iterations/v1.74-world-kb-relationships-and-hygiene-compass-v1.md). V1.75 followup: confidence-weighting, relationship auto-extraction, 8 QC suggestions (`tbd-v1.75-qc-followup`). |

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
| BL-10 | Independent SOUL growth-curve view (separate from temporal-drift timeline) | V1.79 | Backlog | S | V1.79 Track B folded growth-count into the temporal-drift timeline; standalone view deferred. |
| BL-11 | Deeper manuscript reading — persisted reading progress + annotations/highlights + profile-specific reading chrome | V1.79 | Backlog | M-L | V1.79 reading surface is session-only + novel-first; persistence/annotations/profile-chrome deferred. |
| BL-12 | SOUL viz refinements — per-World filtering + realtime updates + LLM-generated personality-score narrative | V1.79 | Backlog | M | V1.79 SOUL viz is per-creator + poll/manual-refresh + raw keyword/timeline; refinements deferred. |

### 2.5 Reliability roadmap (cross-version)

Non-feature reliability work routed out of feature iterations; picked up by a dedicated reliability iteration or opportunistically.

| ID | Item | Source | Target | Notes |
|----|------|--------|--------|-------|
| REL-01 | POST /memory/review synchronous whole-queue pipeline — add bound/timeout/cancellation/concurrency | `R-V178P0-QC3-003` (V1.78 QC) | Reliability iteration | V1.79 P-last reclassified from "V1.79+ reliability roadmap" to this dedicated grouping. Synchronous whole-queue rewrite; not silently dropped. |

---

## 3) Residuals (SSOT pointer)

Residual findings are tracked in [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary`. The tracker does **not** mirror residual rows — `status.json` is authoritative. Current state: 9 open V1.60 residuals (all low, V1.61+). Closed/historical: `.mstar/archived/residuals/<plan-id>.json`.

---

## 4) Change control

- **Shipped rows**: Move from §2.3 to [shipped archive](../archived/shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **Effort estimates**: XS/S/M/L/XL agent-session scale. Guidance only.

---

## 5) Quick index

**Active iteration**: V1.79 delivered (2026-07-01) — Author Reflection (manuscript reading surface + SOUL personality visualization); DF-49 cancelled (ACP-client conflict); next: V1.80 candidate evaluation at next `/iteration-start` (backlog: standalone maturation dashboard, independent SOUL growth-curve view, deeper manuscript reading/annotations, reliability hardening iteration to pick up R-V178P0-QC3-003)

**Latest shipped**: [V1.66](../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md) (Tauri Desktop Shell, PR #90 — 2026-06-26)

**Full iteration index**: [iterations/README.md](../iterations/README.md)

**Shipped archive**: [shipped-features-tracker.md](../archived/shipped-features-tracker.md)

**Machine state**: [`status.json`](../status.json)
