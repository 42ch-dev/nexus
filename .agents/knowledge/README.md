# Knowledge Base

Dev-process knowledge for the Nexus project. These documents are **inputs to** or **outputs from** specific plans — they serve as context for agent handoff and cross-session continuity, but are not intended for external consumers.

For the distinction between this directory and user-facing `docs/`, see [AGENTS.md](../../../AGENTS.md) (same file explains the content boundary between `docs/` and `.agents/knowledge/`).

### Current focus (as of 2026-05-13)

- **Active delivery track**: **V1.17** (gated — prompt + embedded skills quality iteration; compass draft at [v1.17-prompt-skills-compass-v1.md](v1.17-prompt-skills-compass-v1.md)). V1.16 is **Done**.
- **V1.16 closure summary**: Big-bang V2 repositioning delivered — ACP-first control plane, Creator-owned knowledge plane, command IA reorg, trace correlation, creator identity cache, and multi-scope KB baseline. SSOT was [v1.16-delivery-compass-v1.md](v1.16-delivery-compass-v1.md). 8 implementation plans + 1 verification/closeout plan completed.
- **`status.json`**: `plans[]` holds only non-Done rows (Profile B); root `residual_findings` carries **11** open items across 5 plan sources (V1.13, V1.16 daemon-acp-topology, creator-kb-multiscope, creator-identity-auth-compat, trace-correlation), all deferred to `v1.17` or `backlog`.
- **Cross-repo spec**: authoritative **`cli-spec-v1.md`** (CLI / workspace semantics) and **ADR-023 / ADR-024** live in the **private platform repository**'s frozen `v1-spec/` tree (not shipped inside this clone); wire contracts remain **`schemas/`** here.

## Index (active)

| Document | Source Plan | Description | Status |
| --- | --- | --- | --- |
| [nexus42-single-binary-daemon-runtime-architecture-v1.md](nexus42-single-binary-daemon-runtime-architecture-v1.md) | PM brainstorming thread (2026-05-14) | Combined architecture spec for Topic #1 + Sub-spec #2: single-binary daemon runtime (`nexus42` + `nexus-daemon-runtime`) and `nexus-agent-host` Hybrid/Managed-only design for ACP + common Agent CLIs. | Active |
| [canonical-hash-v1.md](canonical-hash-v1.md) | `2026-04-09-v1.1-arch-alignment-closure` | **OSS companion** to v1-spec **ADR-006** (Bundle content digest): implementation pointers, golden vector copy, D2 graph-tag vs digest note, parity checklist. Normative SSOT remains ADR-006. | Active |
| [local-fs-layout-creator-workspace-v1.md](local-fs-layout-creator-workspace-v1.md) | `2026-04-10-local-fs-layout-ssot-and-implementation` | **Non-normative pointer** — definitions only in v1-spec (`adr-014`, `cli-spec` §6.2–§13, `local-db-schema`, `data-model` §5.14, `auth-session-model` §6). | Active — clone handoff only |
| [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md) | V1.4 dependency-hygiene review (2026-04-17) | **Rust workspace crate selection SSOT**: six dependency conventions + per-module decisions for JSON-RPC (`jsonrpsee-core`), orchestration SessionStorage (`sqlx`), `state.db` (sqlx post-WS8), platform auth (`jsonwebtoken` only; `oauth2` deferred), challenge eval (hand-rolled), file watcher (`notify`), **cron (V1.5 WS-D implemented — hand-rolled `cron` + `chrono-tz`)**, layered config (`figment`), and snapshot testing (`insta`). | Active — authoritative for crate selection + dependency conventions |
| [creator-schedule-and-core-context-v1.md](creator-schedule-and-core-context-v1.md) | V1.4 WS7 (2026-04-17) | **Creator Schedule + immutable versioned `core_context`**: data model (Rust types + SQLite schema), Schedule state machine, multi-Schedule concurrency rules, `core_context` derivation kinds (Seed/UserEdit/PresetHook/PresetSeedExpansion/**LlmSummarize** — V1.5 implemented), preset YAML additions, CLI command surface (13 subcommands), HTTP surface (8 endpoints). V1.5 cron triggers and `context.summarize` capability now shipped. | Active — V1.4 WS7 authoritative design; V1.5 items implemented |
| [v1.1-overview-v2.md](v1.1-overview-v2.md) | V1.1 | **V1.1 program snapshot (nexus OSS).** Historical reference for V1.1 scope, plan counts, and residual state at 2026-04-09. | **Done (Frozen)** — V1.1 complete; superseded by subsequent delivery compasses |
| [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md) | V1.2 program planning | V1.2 delivery planning: scope lock, reclassification compass, milestone gates (M1/M2/M3), regression gate, risk controls, cross-repo dependencies. | **Done (Frozen)** — V1.2 delivery complete |
| [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md) | `2026-04-14-v1.2-reclassification-baseline` (WS1) | V1.2/V1.3/V1.4/Backlog reclassification matrix: maps every ADR anchor, spec entry, roadmap theme, and tech debt to a program version track. | **Done (Frozen)** — reclassification complete |
| [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md) | V1.3 | V1.3 delivery compass: scope lock (Creator Register CLI + 35 residual governance), wave decomposition, cross-repo dependency map, acceptance criteria. | **Done (Frozen)** — V1.3 delivery complete |
| [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md) | V1.4 Orchestration + schemas boundary + Schedule (2026-04-17) | V1.4 delivery compass: 7 workstreams (WS1–WS8), 5 milestones, 14-entry regression gate, 10-entry risk register, §10 SSOT allocation matrix. | **Done (Frozen)** — V1.4 delivery complete |
| [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md) | V1.5 (2026-04-18) | V1.5 delivery compass: Stabilization + Creator Intelligence. 5 workstreams (WS-A–WS-E), 4 milestones (M1–M4), 10-entry regression gate (R16–R25), 6-entry risk register. | **Done (Frozen)** — V1.5 delivery complete |
| [v1.6-delivery-compass-v1.md](v1.6-delivery-compass-v1.md) | V1.6 (2026-04-20) | V1.6 delivery compass: Residual Governance + ACP SDK Preparation + Creator Tooling. 4 workstreams (WS-A–WS-D), 3 milestones, 7-entry regression gate (R26–R32), 8-entry risk register. | **Done (Frozen)** — V1.6 delivery complete |
| [deferred-features-cross-version-tracker-v1.md](deferred-features-cross-version-tracker-v1.md) | V1.7 planning (2026-04-21) | **Cross-version deferred feature tracker**: lifecycle for deferred features and tech-debt (Open/Shipped/Cancelled/Superseded), target versions, per-version snapshots **through V1.15+ horizon**, and pointers to both repos' `status.json`. | **Active** — review when a version closes or compass scope shifts |
| [v1.7-delivery-compass-v1.md](v1.7-delivery-compass-v1.md) | V1.7 (2026-04-21) | V1.7 delivery compass: Residual Closure + ACP SDK Migration + Multi-Agent Worker. 5 workstreams (WS-A–WS-E), 4 milestones (M1–M4), 10-entry regression gate (R33–R42), 8-entry risk register. | **Done (Frozen)** — V1.7 delivery complete |
| [v1.8-delivery-compass-v1.md](v1.8-delivery-compass-v1.md) | V1.8 (2026-04-23) | V1.8 delivery compass: CLI Spec Alignment + Handle Support. 3 workstreams (WS-A–WS-C), 3 milestones (M1–M3), 6-entry regression gate (R43–R48), 4-entry risk register. | **Done (Frozen)** — V1.8 delivery complete |
| [v1.9-delivery-compass-v1.md](v1.9-delivery-compass-v1.md) | V1.9 (2026-04-24) | V1.9 delivery compass: Orchestration Presets + CLI Spec Hardening. 3 workstreams (WS-A–WS-C), 3 milestones, regression gate, risk register. | **Done (Frozen)** — V1.9 delivery complete |
| [v1.10-delivery-compass-v1.md](v1.10-delivery-compass-v1.md) | V1.10 (2026-04-26) | V1.10 delivery compass: Device Flow Login + DeviceID Header + Daemon Auth/Context Mock Removal. | **Done (Frozen)** — V1.10 delivery complete |
| [v1.11-delivery-compass-v1.md](v1.11-delivery-compass-v1.md) | V1.11 (2026-04-27) | V1.11 delivery compass: Daemon Stub Cleanup + DeviceID TOCTOU Fix + CLI refresh_token. 3 workstreams (WS-A–WS-C), 2 milestones. | **Done (Frozen)** — V1.11 shipped 2026-04-28 |
| [v1.12-delivery-compass-v1.md](v1.12-delivery-compass-v1.md) | V1.12 (2026-04-30) | V1.12 delivery compass: 收口迭代 — Preset Module Completion + V1.11+Warning Residual Sweep + Backlog Closure. 3 themes (A–C), 17 residuals + 1 doc fix, target: open residual = 0. | **Done (Frozen)** — compass frontmatter **Done**; ship complete |
| [v1.13-delivery-compass-v1.md](v1.13-delivery-compass-v1.md) | V1.13 (2026-05-06) | V1.13 delivery compass: OSS-forward — DF-11/DF-14 + DF-15 governance closure. | **Done** |
| [v1.14-delivery-compass-v1.md](v1.14-delivery-compass-v1.md) | V1.14 (2026-05-09) | V1.14 delivery compass: balanced runtime hardening — OSS-side template/CLI harness + residual narrative; **§0** scope lock. Cross-repo closure led by **platform** Plans **86–87**; this repo had **no** new `plans[]` row. | **Active** — planning SSOT in file; cross-repo **V1.14 Done** per program timeline |
| [v1.15-delivery-compass-v1.md](v1.15-delivery-compass-v1.md) | V1.15 (2026-05-10) | V1.15 delivery compass: orchestration-first — `embedded-skills/` → **`$HOME/.nexus42/skills/`**, preset **`recommended_skills`**, hard-remove **`research` / `manuscript` / `publish`** CLI groups, **`novel-writing`** paths + **sync** submodule; align with platform program compass **§5** (G1–G5). | **Active** — platform `v1-spec` **documentation** for V1.15 (`cli-spec` + ADR-023/024) updated ahead of Rust delivery; `{PLAN_DIR}` / `plans[]` still optional |
| [v1.16-delivery-compass-v1.md](v1.16-delivery-compass-v1.md) | V1.16 (2026-05-11) | V1.16 delivery compass: Big-bang V2 (ACP-first control plane + Creator-owned knowledge plane + command IA reorg). 9 plans: 8 implementation + 1 verification/closeout. Key deliverables: command surface contract, daemon/ACP topology, creator knowledge topology, system/platform topology, ACP execution consolidation, creator KB multi-scope, trace correlation, creator identity/auth compat. | **Done (Frozen)** — V1.16 delivery complete; 11 open residuals deferred to V1.17/backlog |
| [v1.17-prompt-skills-compass-v1.md](v1.17-prompt-skills-compass-v1.md) | V1.17 (2026-05-11) | V1.17 compass: prompt + embedded skills quality iteration; **only starts after V1.16 Done**. | **Draft (gated)** |

## Archived (`archived/knowledge/`)

Superseded or cold-storage knowledge documents live under [`../archived/knowledge/`](../archived/knowledge/). Paths in archived plan JSON / metadata may still point here for historical SSOT.

| Document | Source Plan | Description | Status |
| --- | --- | --- | --- |
| [dual-outbox-architecture-v1.md](../archived/knowledge/dual-outbox-architecture-v1.md) | `v1-tech-debt-cleanup` | TD-8: dual-outbox model (nexus-local-db vs nexus-sync). Superseded by WS8 sqlx migration. | Archived — WS8 migration superseded |
| [architecture-alignment-review-v1.md](../archived/knowledge/architecture-alignment-review-v1.md) | 2026-04-08 / `2026-04-09-v1.1-arch-alignment-closure` | Architecture alignment baseline + TD resolution matrix. ACP paths stale; CLI commands now exist. | Archived — historical reference; ACP paths outdated |
| [acp-client-tech-spec-v2.md](../archived/knowledge/acp-client-tech-spec-v2.md) | V1.4 Orchestration brainstorm (2026-04-17) | V1.4 ACP client amendment: worker-delegated hosting, `nexus-acp-host` crate extraction, orchestration control endpoints. Schema moved to local types; `capabilities/` dir not created. | Archived — V1.4 shipped; schema locations stale |
| [daemon-lifecycle-api-v2.md](../archived/knowledge/daemon-lifecycle-api-v2.md) | V1.4 Orchestration brainstorm (2026-04-17) | 6-state HSM closure of TD-9 via `statig`. Schema moved to local types; handler paths differ. | Archived — V1.4 shipped; reference paths stale |
| [orchestration-engine-v1.md](../archived/knowledge/orchestration-engine-v1.md) | V1.4 Orchestration brainstorm (2026-04-17) | Primary design spec for V1.4 Orchestration track. Task impls moved from `tasks/` to `capability/builtins/`; V1.5 scheduler implemented. | Archived — V1.4/V1.5 shipped; structure outdated |
| [restructured-context-assembly-v1.md](../archived/knowledge/restructured-context-assembly-v1.md) | `2025-04-05-context-assembly` | Context Assembly design decisions SSOT. CLI context module implemented; task references outdated. | Archived — implementation complete |
| [schemas-boundary-v1.md](../archived/knowledge/schemas-boundary-v1.md) | V1.4 WS5 (2026-04-17) | Wire vs local rule for `schemas/`. WS5 audit fully executed; npm version now 0.4.0. | Archived — WS5 fully executed |
| [sqlx-compile-time-migration-v1.md](../archived/knowledge/sqlx-compile-time-migration-v1.md) | V1.4 sqlx macro migration | Migration plan from runtime to compile-time sqlx macros. Migration completed; AGENTS.md convention adopted. | Archived — migration complete |
| [local-db-refactor-v2.md](../archived/knowledge/local-db-refactor-v2.md) | `2026-04-08-local-db-refactor` (WS8 T9 revision) | Local SQLite (`state.db`) refactor design: sqlx ownership, migration runner, async pool APIs. | Archived — WS8 complete; migration count differs |
| [acp-client-tech-spec-v1.md](../archived/knowledge/acp-client-tech-spec-v1.md) | `2025-04-05-acp-client` | V1.0 ACP Client integration spec. | Archived — superseded by v2 |
| [daemon-lifecycle-api-v1.md](../archived/knowledge/daemon-lifecycle-api-v1.md) | `v1-tech-debt-cleanup` | TD-9 partial slice: single "running" probe. | Archived — superseded by v2 |
| [challenge-solver-design-v1.md](../archived/knowledge/challenge-solver-design-v1.md) | V1.3 | Challenge solver module design for Creator Register. | Archived — V1.3 shipped |
| [fork-branch-contract-alignment-v1.md](../archived/knowledge/fork-branch-contract-alignment-v1.md) | `v1-tech-debt-cleanup` | TD-7: `parent_branch_id` alignment. | Archived — TD-7 closed |
| [v1.1-specs-align-review-v1.md](../archived/knowledge/v1.1-specs-align-review-v1.md) | `2026-04-10-v1.1-specs-alignment-remediation` | v1-spec ↔ nexus OSS audit + remediation. | Archived — remediation Done |
| [local-db-refactor-v1.md](../archived/knowledge/local-db-refactor-v1.md) | `2026-04-08-local-db-refactor` (WS8 T9 archival) | V1 design baseline for local SQLite refactor. | Archived — superseded by v2 |
| [revised-domain-models-spec-v1.md](../archived/knowledge/revised-domain-models-spec-v1.md) | `2025-04-05-domain-models` | Field-by-field revision of 15 domain aggregates. | Archived — superseded by implementation |
| [phase1-architecture-review-v1.md](../archived/knowledge/phase1-architecture-review-v1.md) | V1.0-phase1 Retrospective | Comprehensive architecture review: 36 findings. | Archived — summarized in v1.1 overview |
| [phase1-product-review-v1.md](../archived/knowledge/phase1-product-review-v1.md) | V1.0-phase1 Retrospective | Comprehensive product review: 32 CLI commands analyzed. | Archived — summarized in v1.1 overview |
| [phase2-product-plan-v1.md](../archived/knowledge/phase2-product-plan-v1.md) | V1.0-phase2 | V1.1 Beta product plan. | Archived — merged into v1.1-overview-v1 |
| [phase2-architecture-plan-v1.md](../archived/knowledge/phase2-architecture-plan-v1.md) | V1.0-phase2 | V1.1 architecture plan. | Archived — merged into v1.1-overview-v1 |
| [outbox-schema-v2.md](../archived/knowledge/outbox-schema-v2.md) | `v1-tech-debt-cleanup` (Batch C / SYNC-R10) | Sync outbox schema + migration guidance. | Archived — historical reference |
| [regression-gate-v1.md](../archived/knowledge/regression-gate-v1.md) | `2026-04-14-v1.2-regression-suite` | V1.2 M3 regression-gate definition. | Archived — historical reference |
| [v1.1-overview-v1.md](../archived/knowledge/v1.1-overview-v1.md) | V1.1 | Long-form V1.1 overview. | Archived — superseded by v2 |

## Maintenance

### Adding a new document

1. Choose a descriptive name: `<topic>-<qualifier>-v<N>.md` (e.g. `cli-auth-model-v1.md`)
2. Write the document following the reachability rules in [AGENTS.md](../../../AGENTS.md) (section *Documentation and plans*) — **no references to files outside this repository**
3. Add an entry to the **Index (active)** table above (source plan, description, status)
4. If the document is an output of a plan, record the path in `status.json` under that plan's `metadata` (e.g. `wave_0_spec`, `spec_refs`)

### Reading knowledge during implementation

When an agent starts work on a plan, it should:

1. Check `status.json` for the plan's `metadata.wave_0_spec` or `metadata.spec_refs` fields — these point to knowledge base documents that serve as authoritative input
2. Read the referenced knowledge documents **before** writing implementation code
3. Treat knowledge documents as the **ground truth** for design decisions within their scope — do not silently diverge from them

### Archiving

When a knowledge document is fully consumed by implementation or merged into a newer SSOT:

1. Move the file to **`{HARNESS_DIR}/archived/knowledge/`** (e.g. `.agents/archived/knowledge/`) with `git mv` (preserves history); **do not delete** — design rationale stays in-repo
2. Update the **Index** (move the row from Active to **Archived**), and update **all** in-repo paths (plan `.md` files, `v1.1-overview-v2.md`, archived plan JSON snapshots in `.agents/archived/plans/`, code comments, etc.)
3. **`archived/plans/`** and **`archived/residuals/`** hold plan snapshots and closed residuals; **`archived/knowledge/`** is only for superseded knowledge markdown — keep the three subtrees distinct
