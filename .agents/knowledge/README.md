# Knowledge Base

Dev-process knowledge for the Nexus project. These documents are **inputs to** or **outputs from** specific plans — they serve as context for agent handoff and cross-session continuity, but are not intended for external consumers.

For the distinction between this directory and user-facing `docs/`, see [AGENTS.md](../../../AGENTS.md) (same file explains the content boundary between `docs/` and `.agents/knowledge/`).

## Index (active)

| Document | Source Plan | Description | Status |
| --- | --- | --- | --- |
| [canonical-hash-v1.md](canonical-hash-v1.md) | `2026-04-09-v1.1-arch-alignment-closure` | **OSS companion** to v1-spec **ADR-006** (Bundle content digest): implementation pointers, golden vector copy, D2 graph-tag vs digest note, parity checklist. Normative SSOT remains ADR-006. | Active |
| [device-flow-oauth-scope-v1.md](device-flow-oauth-scope-v1.md) | `v1-tech-debt-cleanup` | TD-10: production OAuth dependency on platform; stub `verify_device_code` scope and deferral. | Active |
| [local-fs-layout-creator-workspace-v1.md](local-fs-layout-creator-workspace-v1.md) | `2026-04-10-local-fs-layout-ssot-and-implementation` | **Non-normative pointer** — definitions only in v1-spec (`adr-014`, `cli-spec` §6.2–§13, `local-db-schema`, `data-model` §5.14, `auth-session-model` §6). | Active — clone handoff only |
| [crate-selection-best-practices-v1.md](crate-selection-best-practices-v1.md) | V1.4 dependency-hygiene review (2026-04-17) | **Rust workspace crate selection SSOT**: six dependency conventions + per-module decisions for JSON-RPC (`jsonrpsee-core`), orchestration SessionStorage (`sqlx`), `state.db` (sqlx post-WS8), platform auth (`jsonwebtoken` only; `oauth2` deferred), challenge eval (hand-rolled), file watcher (`notify`), **cron (V1.5 WS-D implemented — hand-rolled `cron` + `chrono-tz`)**, layered config (`figment`), and snapshot testing (`insta`). | Active — authoritative for crate selection + dependency conventions |
| [creator-schedule-and-core-context-v1.md](creator-schedule-and-core-context-v1.md) | V1.4 WS7 (2026-04-17) | **Creator Schedule + immutable versioned `core_context`**: data model (Rust types + SQLite schema), Schedule state machine, multi-Schedule concurrency rules, `core_context` derivation kinds (Seed/UserEdit/PresetHook/PresetSeedExpansion/**LlmSummarize** — V1.5 implemented), preset YAML additions, CLI command surface (13 subcommands), HTTP surface (8 endpoints). V1.5 cron triggers and `context.summarize` capability now shipped. | Active — V1.4 WS7 authoritative design; V1.5 items implemented |
| [v1.1-overview-v2.md](v1.1-overview-v2.md) | V1.1 | **V1.1 program snapshot (nexus OSS).** Historical reference for V1.1 scope, plan counts, and residual state at 2026-04-09. | **Done (Frozen)** — V1.1 complete; superseded by subsequent delivery compasses |
| [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md) | V1.2 program planning | V1.2 delivery planning: scope lock, reclassification compass, milestone gates (M1/M2/M3), regression gate, risk controls, cross-repo dependencies. | **Done (Frozen)** — V1.2 delivery complete |
| [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md) | `2026-04-14-v1.2-reclassification-baseline` (WS1) | V1.2/V1.3/V1.4/Backlog reclassification matrix: maps every ADR anchor, spec entry, roadmap theme, and tech debt to a program version track. | **Done (Frozen)** — reclassification complete |
| [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md) | V1.3 | V1.3 delivery compass: scope lock (Creator Register CLI + 35 residual governance), wave decomposition, cross-repo dependency map, acceptance criteria. | **Done (Frozen)** — V1.3 delivery complete |
| [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md) | V1.4 Orchestration + schemas boundary + Schedule (2026-04-17) | V1.4 delivery compass: 7 workstreams (WS1–WS8), 5 milestones, 14-entry regression gate, 10-entry risk register, §10 SSOT allocation matrix. | **Done (Frozen)** — V1.4 delivery complete |
| [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md) | V1.5 (2026-04-18) | V1.5 delivery compass: Stabilization + Creator Intelligence. 5 workstreams (WS-A–WS-E), 4 milestones (M1–M4), 10-entry regression gate (R16–R25), 6-entry risk register. | **Done (Frozen)** — V1.5 delivery complete |

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
