# Specs (`knowledge/specs/`)

**Functional and normative specifications** for the Nexus OSS repo: CLI, daemon runtime, ACP, orchestration, and related contracts.

**Not here:** cross-cutting rules, schema boundary matrices, or version trackers — see the parent [knowledge/](../README.md) directory.

## Layout

All spec files live **flat** in this directory (kebab-case filenames, no version suffix).

## Platform cross-repo references

When a spec needs platform architecture, shared contracts, or ADRs, cite **`nexus-platform`** paths (side-by-side checkout: `../nexus-platform/.mstar/designs/...`):

| Need | Platform path |
| --- | --- |
| Architecture umbrella | `v1-spec/architecture.md` |
| ADR | `v1-spec/adr/{adr-file-name}.md` |
| Shared contracts | `v1-spec/shared/...` |
| Platform HTTP / product | `v1-spec/platform/...` |

**Wire JSON in this repo:** `schemas/` → `nexus-contracts`.

## Normative hierarchy (read order)

When specs overlap, use this order (higher wins on conflict):

1. **`AGENTS.md`** (repo root) — naming, contracts, release discipline.
2. **[local-cloud-crate-architecture.md](local-cloud-crate-architecture.md)** — local vs cloud product lines, crate graph, contracts-first, forbidden daemon deps/API classes.
3. **[entity-scope-model.md](entity-scope-model.md)** — Global/User/Creator/World/Timeline/Event/Moment hierarchy, uniqueness, and scope-to-crate ownership.
4. **[schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md)** — what belongs in `schemas/` vs `nexus-contracts/src/local/`.
5. **[schemas-directory-layout.md](schemas-directory-layout.md)** — `schemas/` folder tree (`cloud-sync/`, `platform/`, …) and removed paths.
6. **Topology / process** — [local-runtime-boundary.md](local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md), [cli-spec.md](cli-spec.md).
7. **Local persistence / stores** — [local-db-schema.md](local-db-schema.md), [reference-store-layout.md](reference-store-layout.md).
8. **Subsystem specs** — [orchestration-engine.md](orchestration-engine.md), [agent-host.md](agent-host.md), feature contracts (`novel-writing-sync-contract.md`, …).
9. **Iteration compasses** — [`.mstar/iterations/`](../../iterations/README.md) — delivery milestones only; do not duplicate long-term rules from (2).

**Former filename:** `local-platform-isolation-and-crate-architecture.md` → renamed **2026-05-20** to `local-cloud-crate-architecture.md`.

## Index — local runtime (normative)

| Document | Description |
| --- | --- |
| [cli-spec.md](cli-spec.md) | OSS CLI, daemon runtime mode, commands, ACP-first |
| [daemon-runtime.md](daemon-runtime.md) | Single-binary daemon layering and process model |
| [agent-host.md](agent-host.md) | Hybrid Managed-only `nexus-agent-host` |
| [local-runtime-boundary.md](local-runtime-boundary.md) | CLI / daemon / Local API / ACP topology |
| [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) | Long-term SSOT: local/cloud split, crate graph, contracts-first; delivery → [v1.21 compass](../../iterations/v1.21-local-platform-isolation-delivery-compass-v1.md) |
| [entity-scope-model.md](entity-scope-model.md) | Entity scope hierarchy, uniqueness constraints, and scope-to-crate ownership |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | ACP Client technical spec |
| [acp-capability-set.md](acp-capability-set.md) | Logical `nexus.*` capability surface |
| [registry-integration.md](registry-integration.md) | ACP Registry integration |
| [skills-export-compatibility.md](skills-export-compatibility.md) | Skills export (CLI/local only) |
| [local-db-schema.md](local-db-schema.md) | Local `state.db` |
| [reference-store-layout.md](reference-store-layout.md) | Reference registry + `body.md` storage split |
| [creator-challenge-solver.md](creator-challenge-solver.md) | Creator registration challenge solver |

## Index — OSS feature specs

| Document | Description |
| --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | `nexus-orchestration`, presets, worker IPC |
| [work-experience-model.md](work-experience-model.md) | Work container, `creator run`, Creative Brief Intake, run_intents |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Schedules and core-context model |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | Novel-writing sync module |
| [canonical-hash.md](canonical-hash.md) | Bundle canonical hash (OSS implementation notes) |
| [schemas-directory-layout.md](schemas-directory-layout.md) | `schemas/` tree layout, cloud vs local folders, rename policy |

## Archived (superseded)

| Former spec | Notes |
| --- | --- |
| [daemon-api-workspace-write-architecture.md](../../archived/knowledge/daemon-api-workspace-write-architecture.md) | Stale route table |
| [local-fs-layout-creator-workspace.md](../../archived/knowledge/local-fs-layout-creator-workspace.md) | Pointer stub |
| `nexus42-single-binary-daemon-runtime-architecture.md` | Merged into [daemon-runtime.md](daemon-runtime.md) |
| `agent-host-architecture.md` | Merged into [agent-host.md](agent-host.md) §8 |
