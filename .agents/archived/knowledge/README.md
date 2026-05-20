# Archived Knowledge

Superseded or cold-storage implementation knowledge documents. Active SSOT lives in [`.agents/knowledge/`](../../knowledge/README.md).

Paths in archived plan JSON / metadata may still point here for historical reference.

## Index

| Document | Source Plan | Description | Status |
| --- | --- | --- | --- |
| [dual-outbox-architecture.md](dual-outbox-architecture.md) | `v1-tech-debt-cleanup` | TD-8: dual-outbox model (nexus-local-db vs nexus-sync). Superseded by WS8 sqlx migration. | Archived — WS8 migration superseded |
| [architecture-alignment-review.md](architecture-alignment-review.md) | 2026-04-08 / `2026-04-09-v1.1-arch-alignment-closure` | Architecture alignment baseline + TD resolution matrix. ACP paths stale; CLI commands now exist. | Archived — historical reference; ACP paths outdated |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | V1.4 Orchestration brainstorm (2026-04-17) | V1.4 ACP client amendment: worker-delegated hosting, `nexus-acp-host` crate extraction, orchestration control endpoints. Schema moved to local types; `capabilities/` dir not created. | Archived — V1.4 shipped; schema locations stale |
| [daemon-lifecycle-api.md](daemon-lifecycle-api.md) | V1.4 Orchestration brainstorm (2026-04-17) | 6-state HSM closure of TD-9 via `statig`. Schema moved to local types; handler paths differ. | Archived — V1.4 shipped; reference paths stale |
| [restructured-context-assembly.md](restructured-context-assembly.md) | `2025-04-05-context-assembly` | Context Assembly design decisions SSOT. CLI context module implemented; task references outdated. | Archived — implementation complete |
| [schemas-boundary.md](schemas-boundary.md) | V1.4 WS5 (2026-04-17) | Wire vs local rule for `schemas/`. WS5 audit executed; paths updated to `schemas/cloud-sync/` in §5.2. **Layout SSOT:** [schemas-directory-layout.md](../../knowledge/specs/schemas-directory-layout.md). | Archived — methodology + audit; layout superseded |
| [sqlx-compile-time-migration.md](sqlx-compile-time-migration.md) | V1.4 sqlx macro migration | Migration plan from runtime to compile-time sqlx macros. Migration completed; AGENTS.md convention adopted. | Archived — migration complete |
| [local-db-refactor.md](local-db-refactor.md) | `2026-04-08-local-db-refactor` (WS8 T9 revision) | Local SQLite (`state.db`) refactor design: sqlx ownership, migration runner, async pool APIs. | Archived — WS8 complete; migration count differs |
| [acp-client-tech-spec-legacy.md](acp-client-tech-spec-legacy.md) | `2025-04-05-acp-client` | V1.0 ACP Client integration spec. | Archived — superseded by v2 |
| [daemon-lifecycle-api-legacy.md](daemon-lifecycle-api-legacy.md) | `v1-tech-debt-cleanup` | TD-9 partial slice: single "running" probe. | Archived — superseded by v2 |
| [challenge-solver-design.md](challenge-solver-design.md) | V1.3 | Challenge solver module design for Creator Register. | Archived — V1.3 shipped |
| [fork-branch-contract-alignment.md](fork-branch-contract-alignment.md) | `v1-tech-debt-cleanup` | TD-7: `parent_branch_id` alignment. | Archived — TD-7 closed |
| [specs-align-review.md](specs-align-review.md) | `2026-04-10-v1.1-specs-alignment-remediation` | v1-spec ↔ nexus OSS audit + remediation. | Archived — remediation Done |
| [local-db-refactor-legacy.md](local-db-refactor-legacy.md) | `2026-04-08-local-db-refactor` (WS8 T9 archival) | V1 design baseline for local SQLite refactor. | Archived — superseded by v2 |
| [revised-domain-models-spec.md](revised-domain-models-spec.md) | `2025-04-05-domain-models` | Field-by-field revision of 15 domain aggregates. | Archived — superseded by implementation |
| [phase1-architecture-review.md](phase1-architecture-review.md) | V1.0-phase1 Retrospective | Comprehensive architecture review: 36 findings. | Archived — summarized in v1.1 overview |
| [phase1-product-review.md](phase1-product-review.md) | V1.0-phase1 Retrospective | Comprehensive product review: 32 CLI commands analyzed. | Archived — summarized in v1.1 overview |
| [phase2-product-plan.md](phase2-product-plan.md) | V1.0-phase2 | V1.1 Beta product plan. | Archived — merged into program overview |
| [phase2-architecture-plan.md](phase2-architecture-plan.md) | V1.0-phase2 | V1.1 architecture plan. | Archived — merged into program overview |
| [outbox-schema.md](outbox-schema.md) | `v1-tech-debt-cleanup` (Batch C / SYNC-R10) | Sync outbox schema + migration guidance. | Archived — historical reference |
| [regression-gate.md](regression-gate.md) | `2026-04-14-v1.2-regression-suite` | V1.2 M3 regression-gate definition. | Archived — historical reference |
| [program-overview-legacy.md](program-overview-legacy.md) | V1.1 | Long-form V1.1 overview. | Archived — superseded by [v1.1-overview-v2](../../iterations/v1.1-overview-v2.md) |
