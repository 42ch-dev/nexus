# Specs (`knowledge/specs/`)

**Functional and normative specifications** for the Nexus OSS repo: CLI, daemon runtime, ACP, orchestration, and related contracts.

**Not here:** cross-cutting rules, schema boundary matrices, or version trackers — see the parent [knowledge/](../README.md) directory.

## Layout

All spec files live **flat** in this directory (kebab-case filenames, no version suffix).

## Platform cross-repo references

When a spec needs platform architecture, shared contracts, or ADRs, cite **`nexus-platform`** paths (side-by-side checkout: `../nexus-platform/.agents/designs/...`):

| Need | Platform path |
| --- | --- |
| Architecture umbrella | `v1-spec/architecture.md` |
| ADR | `v1-spec/adr/{adr-file-name}.md` |
| Shared contracts | `v1-spec/shared/...` |
| Platform HTTP / product | `v1-spec/platform/...` |

**Wire JSON in this repo:** `schemas/` → `nexus-contracts`.

## Index — local runtime (normative)

| Document | Description |
| --- | --- |
| [cli-spec.md](cli-spec.md) | OSS CLI, daemon runtime mode, commands, ACP-first |
| [daemon-runtime.md](daemon-runtime.md) | Single-binary daemon layering and process model |
| [agent-host.md](agent-host.md) | Hybrid Managed-only `nexus-agent-host` |
| [local-runtime-boundary.md](local-runtime-boundary.md) | CLI / daemon / Local API / ACP topology |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | ACP Client technical spec |
| [acp-capability-set.md](acp-capability-set.md) | Logical `nexus.*` capability surface |
| [registry-integration.md](registry-integration.md) | ACP Registry integration |
| [skills-export-compatibility.md](skills-export-compatibility.md) | Skills export (CLI/local only) |
| [local-db-schema.md](local-db-schema.md) | Local `state.db` |
| [creator-challenge-solver.md](creator-challenge-solver.md) | Creator registration challenge solver |

## Index — OSS feature specs

| Document | Description |
| --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | `nexus-orchestration`, presets, worker IPC |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Schedules and core-context model |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | Novel-writing sync module |
| [canonical-hash.md](canonical-hash.md) | Bundle canonical hash (OSS implementation notes) |

## Archived (superseded)

| Former spec | Notes |
| --- | --- |
| [daemon-api-workspace-write-architecture.md](../../archived/knowledge/daemon-api-workspace-write-architecture.md) | Stale route table |
| [local-fs-layout-creator-workspace.md](../../archived/knowledge/local-fs-layout-creator-workspace.md) | Pointer stub |
| `nexus42-single-binary-daemon-runtime-architecture.md` | Merged into [daemon-runtime.md](daemon-runtime.md) |
| `agent-host-architecture.md` | Merged into [agent-host.md](agent-host.md) §8 |
