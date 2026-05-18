# Knowledge Base

Implementation-detail SSOT for the Nexus project.

This directory now excludes all iteration compass specs. Delivery compass documents live under [`.agents/iterations/`](../iterations/README.md). Keep this directory for reusable technical specs, architecture detail, and cross-version implementation references.

For boundaries, naming, and maintenance rules, see [AGENTS.md](AGENTS.md). Harness-wide conventions: [`.agents/AGENTS.md`](../AGENTS.md).

## Index (active)

| Document | Source Plan | Description | Status |
| --- | --- | --- | --- |
| [nexus42-single-binary-daemon-runtime-architecture.md](nexus42-single-binary-daemon-runtime-architecture.md) | PM brainstorming thread (2026-05-14) | **Implementation SSOT**: daemon runtime crates, batches, verification. Normative: `nexus-platform` `v1-spec/local/daemon-runtime-v1.md` (ADR-026/027). | Active |
| [agent-host-architecture.md](agent-host-architecture.md) | V1.18 planning | **Implementation SSOT**: `nexus-agent-host` crate, traits, routes. Normative: `v1-spec/local/agent-host-v1.md` (ADR-026/027). | Active |
| [daemon-api-workspace-write-architecture.md](daemon-api-workspace-write-architecture.md) | V1.15 | Daemon local API / workspace-write architecture (D1–D7). | Active |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | V1.15 | Novel-writing sync module contract and workspace artifact discovery rules. | Active |
| [canonical-hash.md](canonical-hash.md) | `2026-04-09-v1.1-arch-alignment-closure` | OSS companion to v1-spec ADR-006 (bundle content digest) with parity checklist and implementation notes. | Active |
| [crate-selection-best-practices.md](crate-selection-best-practices.md) | V1.4 dependency-hygiene review | Rust workspace dependency conventions and per-module crate decisions. | Active |
| [orchestration-engine.md](orchestration-engine.md) | V1.4 Orchestration brainstorm (2026-04-17) | Orchestration engine design (`nexus-orchestration`, preset loader, worker IPC, capability registry). | Active |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | V1.4 WS7 | Schedule/core-context data model, state machine, CLI/API surface, and extension rules. | Active |
| [deferred-features-cross-version-tracker.md](deferred-features-cross-version-tracker.md) | V1.7 planning | Cross-version deferred-feature tracker (open/shipped/cancelled/superseded lifecycle). | Active |
| [local-fs-layout-creator-workspace.md](local-fs-layout-creator-workspace.md) | `2026-04-10-local-fs-layout-ssot-and-implementation` | Local filesystem layout guidance and v1-spec anchor mapping. | Active |

## Archived

Superseded documents live under [`.agents/archived/knowledge/`](archived/knowledge/). See [archived knowledge index](archived/knowledge/README.md).
