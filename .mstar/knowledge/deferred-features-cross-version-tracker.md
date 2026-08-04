# Deferred Features — Cross-Version Tracker v2

**Quick status**: **V1.148 closed (2026-08-04)** — spoke `0.8.2` pin + RuleQueryPort production + `orchestrate_check` daemon route + **Connect Host N-C0 delivered** (DF-72 partial: opt-in host, signed-hello + allowlist, honest manifest, all inbound ops refused; dogfood green — see DF-72 row; **N-C1 next** owner architect, trigger: N-C0 dogfood green + partner demand). V1.147 **shipped** (direct compute lane: Run Studio + compute-on-Timeline; DF-V1122-COMPUTABLE-UI + DF-V1122-COMPUTE-ON-TIMELINE closed → shipped archive; DF-V1122-DEEPER-WB event-row read slice landed, row open). Platform **paused**. **Backlog themes:** FL-R Connect/`nexus-runtime` (PD-09; DF-72/73 — N-C0 delivered in V1.148; **N-C1/N-C2/integrator-DX + `nexus42 runtime` profile are must-do-first, NO partner trigger** — partners can't feedback on a surface that refuses every op / has no reasoning / no integration guide; only N-C3/multi-tenant-hardening/dedicated-DF-73-binary are partner-gated); **FL-L** SillyTavern lore-activation Phase B (PD-10; DF-74–79) — SPOKE Phase A complete (shipped against spoke `0.6.1`; nexus pin moved to `0.8.2` in V1.148).

**Purpose**: Single source of truth for **open** and **backlog** features deferred from delivery compasses. Closed/shipped history lives in shipped archive.
**Scope**: `nexus` OSS repository only.
**Created**: 2026-04-21 · **Last updated**: 2026-08-04 (V1.148 close: DF-72 row finalized — N-C0 delivered / N-C1 next; quick status + FL-R target refreshed)

---

## 1) How to use

- **Product decisions**: §2.1 (PD-*)
- **Future product lines**: §2.2 (FL-*)
- **Planning a new version**: Scan §2.3 Open features for items targeting that version or "Any future"
- **Closing an item**: Remove its row from §2.3; append to [shipped archive](shipped-features-tracker.md)
- **Deferring again**: Update `Target` column; keep the row. Add a note.
- **Shipped/cancelled history**: [shipped archive](shipped-features-tracker.md)
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
| PD-09 | **Third-party narrative reasoning prefers L1 SPOKE Connect over Daemon HTTP / Canvas** | Integrators that own their own UI + preset orchestration and only need headless narrative ops consume Nexus via **spoke-connect** → **NexusAdapter** `orchestrate_*` (Adapter-full). Daemon HTTP + Control Room + Canvas remain the **creator-facing** product surface (Product-full). Adapter-full ≠ Product-full: Connect does **not** expose Harness UI, ACP-as-server, or Canvas. Nexus stays an ACP **client**; no MCP server revival (DF-49 cancelled). Capability-token / world scoping required before exposing Adapter-full to multi-tenant end users. Delivery tracked under FL-R / DF-72 / DF-73. |
| PD-10 | **SillyTavern lore-activation lessons → Nexus engines (clean-room); SPOKE owns dialect only** | Absorb durable **lore activation + injection control** mechanisms from public ST docs into Nexus (engines + UX). **No AGPL code reuse.** SPOKE Phase A (naming triad / `modules.activation` / assemble recipes / Knowledge Pack handbook) is **complete** on spoke `0.6.1` — Nexus is consumer-only for wire. Explicit non-absorb: STscript VM, Tavern Card/PNG as core, Prompt Manager UI clone, inclusion-group RNG, Finding-as-lore Content, ranking/budget on SPOKE assemble wire. Research SSOT (spoke repo): `.mstar/references/sillytavern/05-absorption-roadmap.md` (+ `03-nexus-spoke-absorption.md`). Delivery: FL-L / DF-74–79. |

### 2.2 Future product lines (cross-version themes)

| ID | Product line | Suggested target | Notes |
|----|--------------|------------------|-------|
| FL-D | Preset orchestration (Agentic Design Patterns) | Post-V1.34 | V1.31–32 shipped capabilities + quality gate; DF-29/31/56 all since closed. Remaining: DF-03 (3P registry). |
| FL-R | **Third-party Narrative Runtime** (SPOKE Connect Host + optional headless `nexus-runtime`) | **V1.148 N-C0 delivered** (spoke-connect Stage 1 published 0.7.1; RuleQueryPort + `orchestrate_check` production landed in V1.148; **N-C1/N-C2/integrator-DX + `nexus42 runtime` profile = must-do-first — no partner trigger (build the demonstrable spine so partners can evaluate)**; N-C3/multi-tenant-hardening/dedicated DF-73 binary = partner-gated) | Product direction locked 2026-08-02 (PD-09): third parties provide narrative reasoning to ordinary users with **their** orchestration/UI; Nexus supplies World KB + check/assemble/compute (and other Adapter ops) over Connect. **Two delivery shapes under one line:** (1) **DF-72** Connect Host facade co-located with (or extractable from) the existing daemon; (2) **DF-73** dedicated embeddable `nexus-runtime` binary — headless, no web Control Room / Desktop / creator CLI surface — for partners who do not want the full creator stack. Upstream wire: spoke `spoke-connect` + `@42ch/spoke-connect-ts` (spoke roadmap Stage 1). Integrator DX: Connect cookbook + capability scopes; **not** a thick `nexus-reasoning-sdk`. Prerequisites inside Nexus: honest `HostCapabilityManifest`, `RuleQueryPort` production, `orchestrate_check` cutover, scoped `assemble` policy (CLI MCA vs Connect assemble), allowlist + capability-token issuance. Explicit non-goals: replace Daemon HTTP for `apps/web`; treat spoke libp2p spike as product runtime; Nexus-as-MCP/ACP-Agent. |
| FL-L | **Lore activation & assembly control** (SillyTavern absorption Phase B) | **V1.149 active (no partner trigger needed — SPOKE Phase A done + spoke 0.8.2 pin; FL-R N-C1+ is the partner-gated track)** | PD-10. Promote spoke research Phase B waves **W4→W7** into Nexus delivery (one wave per plan stream). **Sequence (HARD):** DF-74 → DF-75 → DF-76 → DF-77; optional DF-78 after DF-74, DF-79 after DF-75. **Already on Nexus (do not re-litigate):** V1.146 flag-gated `modules.activation` in MCA (`NEXUS_MCA_LORE_ACTIVATION`) + `--emit-packet`; V1.146 `creator world kb pack export\|import` CLI baseline. FL-L ships the **engines + creator UX** on top of that dialect. Synergy with FL-R: richer activation/assemble improves what Connect Host can offer third parties, but FL-L is creator-product-first. Research: spoke `.mstar/references/sillytavern/05-absorption-roadmap.md`. |

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
| DF-V1122-FORK-UI | Canvas | Fork creation + fork-merge authoring UI | V1.122 | V1.124+ | L | V1.122 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal; still deferred in V1.123. **Owner:** product-manager. **Trigger:** authors need alternate-history editing, not just read-only Fork-badge chrome. |
| DF-V1122-DEEPER-WB | Canvas | Deeper World-building on Timeline (richer projection, multi-timeline, World-scoped `TimelineEvent` HTTP route `GET /v1/daemon/worlds/{world_id}/timeline`) | V1.122 | V1.126+ (remainder slice) / V1.125+ remainder | L | V1.122→V1.123→V1.126→V1.147 | [V1.122 compass](../iterations/v1.122/delivery-compass.md) Non-Goal. **V1.126 P2 status:** Ships `GET /v1/daemon/timeline/overview` composite (overview slice, no event rows). **V1.147 P2 status:** event-row read slice landed — `GET /v1/daemon/worlds/{world_id}/timeline/events` (cursor-paginated, keyset `ev1:` cursor, `branch_id`/`status`/`event_type` filters, canon-first read). Remainder (richer projection, multi-timeline) stays open under the same DF ID. **Owner:** architect. **Trigger:** V1.124+ author demand for World-scoped `TimelineEvent` row access via HTTP (causality graph, fork-marker progression, publish-marker history) that the KnowledgeEntry-graph composition cannot satisfy. |
| DF-V1122-V1121-RES | Cross-cutting | V1.121 15 low/nit design-elevation residuals | V1.122 | V1.124 polish | S | V1.122 | Still deferred under V1.123 Non-Goals (not business scope). **Owner:** frontend-dev. **Trigger:** capacity after three-layer ship. **SSOT:** [`status.json`](../status.json) `residual_findings` — do not mirror detail here. Related rollup: `DF-V1123-RESIDUAL-CLEANUP`. |
| DF-V1123-WORLD-MOMENT | Canvas | World Timeline Moment layer (scene-precision within World history) | V1.123 | V1.124+ | M | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal / Deferred inventory. **Owner:** product-manager. **Trigger:** authors need scene-precision when reading world history, not only when writing Works. |
| DF-V1123-WORK-BRIEF | Canvas | Work Timeline Brief layer (world-shape projection for a Work) | V1.123 | V1.124+ | M | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Non-Goal. **Owner:** product-manager. **Trigger:** authors need Work-level world-shape context beyond Outline. |
| DF-V1123-ERA-TAXONOMY | Canvas | Rich era taxonomy for Brief layer (kingdoms, ages, sub-ages; not just era markers) | V1.123 | V1.124+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. **Owner:** product-manager. **Trigger:** Brief MVP proves the abstraction; richer taxonomy needed. |
| DF-V1123-MULTI-TIMELINE | Canvas | Multiple parallel Timelines per World (alternate-history branches beyond Fork) | V1.123 | V1.125+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. **Owner:** architect. **Trigger:** authors need branch comparison beyond Fork semantics. May absorb multi-timeline remainder of `DF-V1122-DEEPER-WB`. |
| DF-V1123-GLOBAL-TIMELINE-MERGE | Canvas | Cross-World Timeline merge (read-write merged view, not read-only overview) | V1.123 | V1.125+ | L | V1.123 | [V1.123 compass](../iterations/v1.123/delivery-compass.md) Deferred. P3 global Timeline is **read-only overview** only. **Owner:** product-manager. **Trigger:** P3 overview proves valuable; merge needed for cross-World narrative. |
| DF-V1123-CROSS-SURFACE-BINDING | Canvas | Cross-surface Timeline event binding (Work Timeline Moment ↔ World Timeline Narrative) — data link + UX | V1.123 | V1.124+ | S | V1.123 | P3 ships UX hints for cross-surface navigation but no formal data binding between Work events and World events. Track for V1.124+ data-binding iteration. QC1 R-V1123P0QC1-M001. |
| DF-V1127-COMPOSITE-PERF | Cross-cutting | Composite-endpoint performance round: `total_worlds` cleanup, dynamic-SQL → static refactor, N+1 assertion, sqlx prepared-statement caching (scan items 7–8 + V1.126 P2 residual cluster) | V1.127 | V1.128+ | M | V1.127 | [V1.127 compass](../iterations/v1.127/delivery-compass.md) Roadmap Position (e). Pure-scale perf; manual tester with <100 worlds never sees the symptom. **Owner:** architect. **Trigger:** V1.127 dogfood shipped + user's manual testing review feedback. |
| DF-V1127-NIT-CLOSEOUT | Cross-cutting | V1.126 nit residual close-out (22 nits beyond the 2 absorbed by V1.127 P0: R-V1126P0-QC-S-002, R-V1126P0-QC-S-003) | V1.127 | V1.128+ | S | V1.127 | [V1.127 compass](../iterations/v1.127/delivery-compass.md) Roadmap Position (f). Nits are polish, not test-blockers. **Owner:** frontend-dev. **Trigger:** capacity after V1.127 dogfood. |
| DF-74 | Harness | **FL-L W4 — Keyword / selective lore activation + Relation hop expand** | 2026-08-02 | **V1.149 (FL-L W4 active — trigger met; no partner dependency)** | L | 2026-08-02 | PD-10 / FL-L wave 1 (spoke absorption W4). Product engine: scan MomentRequest / recent manuscript / outline beats with per-entry `modules.activation` (`keys[]`, key_logic, match_mode, priority, always); emit ordered candidates into WorldContextBlock / AssemblePacket. **Relation hops:** on fire of entry A, expand 1..N Relation hops under MCA token budget (prefer graph recursion over ST content-scanning recursion). **Partial (V1.146):** flag-gated `apply_activation` + `NEXUS_MCA_LORE_ACTIVATION` + activation_trace — extend into default-on keyword scan + hop expand; do not put matching algorithms into `spoke-operations`. **Owner:** architect + fullstack-dev. **Trigger:** schedule FL-L; SPOKE Phase A Done. |
| DF-75 | Harness | **FL-L W5 — Preset injection slots + Moment Directive** | 2026-08-02 | FL-L (after DF-74) | L | 2026-08-02 | PD-10 / FL-L wave 2 (spoke absorption W5). Ordered preset slot IDs / outlets (e.g. `world.before`, `world.after`, `style.post_history`, `moment.directive`, `kb.outlet.<name>`) filled by activated lore; generation-type triggers aligned to Nexus job kinds. **Moment Directive** (Author's Note analogue): body + insert depth + TTL (generations/chapters) + optional clear on scene change — via inject-prompt / MCA, not a new SPOKE object. Clean-room; no Prompt Manager UI clone. **Owner:** product-manager + fullstack-dev. **Trigger:** DF-74 activation path usable. |
| DF-76 | Canvas | **FL-L W6 — Assembly inspector (Control Room / CLI)** | 2026-08-02 | FL-L (after DF-75) | M | 2026-08-02 | PD-10 / FL-L wave 3 (spoke absorption W6). Show which lore fired, budget used, slot order, activation_trace / placement matching spoke assemble-module recipes. Surfaces: Control Room + `assemble-moment` debug. **Partial (V1.146):** `--emit-packet` diagnostic when lore activation flag ON — promote to first-class inspector UX. Synergy with FL-R cookbook later. **Owner:** frontend-dev + fullstack-dev. **Trigger:** DF-74/75 produce inspectable traces. |
| DF-77 | Cross-cutting | **FL-L W7 — Knowledge Pack productization + optional ST lorebook importer** | 2026-08-02 | FL-L (after DF-76) | M | 2026-08-02 | PD-10 / FL-L wave 4 (spoke absorption W7). Product transport envelope for Narrative Knowledge Pack (Pack ≠ AssemblePacket; catalog not `modules.pack`). **Partial (V1.146):** CLI `creator world kb pack export\|import` + `pack_import` provenance — deepen UX/docs, Seed/Pool workflows, optional **ST lorebook → pack** clean-room importer (import adapter only; no Tavern Card/PNG as Nexus core). **Owner:** product-manager + fullstack-dev. **Trigger:** FL-L W4–W6 landed or pack UX demand independent. |

### 2.4 Backlog (no committed target)

| ID | Pillar | Feature | First deferred | Target | Effort | Notes |
|----|--------|---------|---------------|--------|--------|-------|
| DF-03 | Harness | Preset third-party registry / signing / publish | V1.4 | Backlog | XL | Potentially independent project. |
| DF-72 | Cross-cutting | **SPOKE Connect Host — Adapter-full invoke surface** | 2026-08-02 | **V1.148 N-C0 delivered** (P3) — **N-C1 next** (FL-R; owner + trigger below); N-C2..N-C3 future | L | PD-09 / FL-R. Nexus Connect Host: explicit peering + WS (default; align with `@42ch/spoke-connect-ts`) + session core; `ConnectInvoke` → `NexusAdapter` `orchestrate_*` (upsert / promote / relate / check / assemble / project / compute as capabilities allow). Honest `HostCapabilityManifest`; capability-token + world/tenant scoping. Phased: N-C0 manifest → N-C1 write ops → N-C2 check/assemble production → N-C3 `list_peer` / multi-host. **Status (2026-08-04, V1.148 close):** **N-C0 delivered** — opt-in host (`nexus42 connect start`, cargo feature `connect-host` default off, separate OS process), spoke-connect signed hello + fail-closed allowlist, honest single-builder manifest (device-id `host_id`, `data-store` role, no `reasoning-complete`), every inbound op refused (`op_unsupported`; `invoke_handler = None`). **Dogfood evidence:** P4 T1 real-process run vs a spoke-connect reference peer on a seeded workspace (handshake, §4.2 manifest honesty, 7 core ops + garbage-op refusal, non-allowlisted-peer rejection, capability-token structural gate, feature-off invariance) — all green; 2 defects fixed (device-id double-nesting canonicalization → `~/.nexus42/device-id`; `request_id` padding). **N-C1 next:** inbound write-op exchange over Connect (upsert/promote/relate with OCC + capability-token + world scoping; N-C0 refusal tests inverted for write ops only). **Owner:** architect. **Trigger:** N-C0 dogfood green (done, this plan T1) — **N-C1 is must-do-first (no partner trigger): partners cannot give feedback on a Connect surface that refuses every op; the demonstrable spine (N-C1 write ops → N-C2 check/assemble → integrator DX) must be built on speculation so partners have something concrete to evaluate.** Partner-gated items are N-C3 (multi-host `list_peer`) + production multi-tenant hardening. **Done-when:** write-op exchange over Connect with OCC + capability-token + world scoping. **N-C2** (check/assemble over Connect; "reasoning-complete" legitimate) and **N-C3** (`list_peer_host_capability_manifests` production / multi-host) remain future phases. **R-V1142P1-002 re-assessed (2026-08-04):** self-manifest is now production via the shared builder (`HostManifestPort::get_host_capability_manifest`), but `list_peer_host_capability_manifests` still returns empty — no multi-host in N-C0..N-C2; residual stays **open** with trigger **N-C3 multi-host collaboration** (spec §7.4 matrix + §10.6; SSOT: `status.json`). Coexists with Daemon HTTP (creator UI SSOT). |
| DF-73 | Cross-cutting | **Headless embeddable `nexus-runtime` binary** | 2026-08-02 | Backlog (FL-R) | XL | PD-09 / FL-R. Distinct from creator-facing `nexus42` CLI + Desktop + Control Room: a minimal process that loads workspace/SQLite, hosts **DF-72 Connect** (and optionally a thin local admin/health surface), ships **without** embedded `apps/web` / Canvas / Setup wizard. Target: third-party products that embed narrative reasoning for ordinary users while owning their own orchestration presets and UI. Composition: reuse `nexus-spoke-adapter` + `nexus-local-db` + quality-loop/checker hooks + compute host as needed; omit ACP Control Room paths unless explicitly flagged. Packaging/distribution and multi-tenant isolation are in-scope product design for the first FL-R compass. May start as a `nexus42 runtime …` subcommand / feature-flagged daemon profile before a separate binary. **Depends on:** DF-72 (or ships in the same FL-R program). **Owner:** architect + product-manager. **Trigger (reframed 2026-08-04):** TWO tiers — (1) **must-do-first (no partner):** the `nexus42 runtime` feature-flagged profile (headless mode without the creator-UI stack) is part of the demonstrable spine — a partner evaluating "headless embed" must see Nexus run without Control Room/Canvas/Setup, so this profile builds alongside N-C1/N-C2; (2) **partner-gated:** the **dedicated** `nexus-runtime` binary (XL: separate artifact, packaging/distribution, production multi-tenant isolation) waits for a partner who confirms they want the thin form vs full Desktop. |
| DF-78 | Harness | **FL-L optional — Vector lore activation beside keywords** | 2026-08-02 | Backlog (FL-L) | L | PD-10. After DF-74. Optional retrieval/vector path alongside deterministic keyword activation; keep SPOKE assemble wire free of ranking/embedding fields (product-local only). |
| DF-79 | Harness | **FL-L optional — Regex hygiene transforms (structured-first)** | 2026-08-02 | Backlog (FL-L) | S | PD-10. After DF-75. Small built-in I/O hygiene transforms + optional user regex packs; not the primary style system. |
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

Residual findings are tracked in [`status.json`](../status.json) → `residual_findings` + `metadata.tech_debt_summary`. The tracker does **not** mirror residual rows — `status.json` is authoritative. Current rollup: see `metadata.tech_debt_summary` (updated 2026-07-22). Closed/historical: `.mstar/archived/residuals/<plan-id>.json`.

**V1.131 residual slate (closed at iteration-close):** DF-V1130-* / DF-V1131-* shipped (see [shipped archive](shipped-features-tracker.md)). Open human smokes remain in `status.json`: `R-VI-003` (Dock live), `R-VI-002` / `R-VI-004` (gallery notes / wordmark sign-off), `R-V1131P0-QC2-W-001` (Overlay H2–H4).

---

## 4) Change control

- **Shipped rows**: Move from §2.3 to [shipped archive](shipped-features-tracker.md) §1; add per-version snapshot to archive §2 when an iteration closes.
- **Compass authority**: Active compass controls scope even if this tracker lists a different target.
- **In-flight “must ship”**: Rows marked **V1.132 in-flight — must ship** are **committed delivery** for the active compass; do not re-target without PM scope change.
- **Effort estimates**: XS/S/M/L/XL agent-session scale. Guidance only.

---

## 5) Quick index

**Active iteration**: [V1.148](../iterations/v1.148/delivery-compass.md) (**active** — spoke `0.6.1→0.8.2` pin + RuleQueryPort + `orchestrate_check` cutover + Connect Host N-C0; `iteration/v1.148`).

**Latest shipped**: [V1.147](../iterations/v1.147/delivery-compass.md) (direct compute lane — Run Studio + compute-on-Timeline; merged #194).

**Full iteration index**: [iterations/README.md](../iterations/README.md)

**Shipped archive**: [shipped-features-tracker.md](shipped-features-tracker.md)

**Machine state**: [`status.json`](../status.json)
