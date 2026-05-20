# Specs (`knowledge/specs/`)

**Functional and normative specifications** for the Nexus OSS repo: CLI, daemon, ACP, orchestration, sync contracts, and related architecture.

**Not here:** cross-cutting engineering rules, schema boundary matrices, or version trackers — those stay in the parent [knowledge/](../README.md) directory.

## Layout

All spec files live **flat** in this directory (no `specs/` subtree). Frozen v1 local module files use the `*-v1.md` naming convention.

## Cross-repo links in `*-v1.md` (local module)

Many normative specs still use relative paths such as `../adr/`, `../shared/`, `../platform/`, `../architecture.md`. Those resolve under **`nexus-platform`** `.agents/designs/v1-spec/` when both repositories are checked out side by side:

| Link in spec body | Resolve in platform repo |
| --- | --- |
| `../architecture.md` | `v1-spec/architecture.md` |
| `../adr/*` | `v1-spec/adr/*` |
| `../shared/*` | `v1-spec/shared/*` |
| `../platform/*` | `v1-spec/platform/*` |
| `../../references-learnings.md` | `designs/references-learnings.md` |

**Migration:** [ADR-029](https://github.com/42ch/nexus-platform/blob/main/.agents/designs/v1-spec/adr/adr-029-oss-local-specs-in-nexus-knowledge-v1.md) — OSS SSOT for former `v1-spec/local/` lives here.

**Wire JSON in this repo:** `schemas/` → `nexus-contracts`.

## Index — frozen local v1 (normative)

| Document | Description |
| --- | --- |
| [cli-spec-v1.md](cli-spec-v1.md) | `nexus42`, daemon runtime mode, commands, ACP-first |
| [daemon-runtime-v1.md](daemon-runtime-v1.md) | Single-binary daemon layering and process model |
| [agent-host-v1.md](agent-host-v1.md) | Hybrid Managed-only `nexus-agent-host` (normative + §8 implementation) |
| [local-runtime-boundary-v1.md](local-runtime-boundary-v1.md) | CLI / daemon / Local API / ACP topology |
| [acp-client-tech-spec-v1.md](acp-client-tech-spec-v1.md) | ACP Client technical spec (ADR-004) |
| [acp-capability-set-v1.md](acp-capability-set-v1.md) | Logical `nexus.*` capability surface |
| [registry-integration-v1.md](registry-integration-v1.md) | ACP Registry integration |
| [skills-export-compatibility-v1.md](skills-export-compatibility-v1.md) | Skills export (CLI/local only) |
| [local-db-schema-v1.md](local-db-schema-v1.md) | Local `state.db` (ADR-005) |
| [creator-challenge-solver-v1.md](creator-challenge-solver-v1.md) | Creator registration challenge solver |

## Index — OSS feature / implementation specs

| Document | Description |
| --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | `nexus-orchestration`, presets, worker IPC |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Schedules and core-context model |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | Novel-writing sync module |
| [canonical-hash.md](canonical-hash.md) | Bundle canonical hash (ADR-006 companion) |

## Archived (superseded)

| Former path | Archive |
| --- | --- |
| `daemon-api-workspace-write-architecture.md` | [.agents/archived/knowledge/daemon-api-workspace-write-architecture.md](../../archived/knowledge/daemon-api-workspace-write-architecture.md) |
| `local-fs-layout-creator-workspace.md` | [.agents/archived/knowledge/local-fs-layout-creator-workspace.md](../../archived/knowledge/local-fs-layout-creator-workspace.md) |
| `nexus42-single-binary-daemon-runtime-architecture.md` | Merged into [daemon-runtime-v1.md](daemon-runtime-v1.md) §8–9 |
| `agent-host-architecture.md` | Merged into [agent-host-v1.md](agent-host-v1.md) §8 |
