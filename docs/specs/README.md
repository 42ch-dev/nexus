# Project specification corpus (`docs/specs/`)

## Scope

This corpus describes the **present state** of the Nexus OSS monorepo: architecture layers, boundaries, contracts, data flow, and current paths and commands. It answers "how does the system work today" — not "why this option won" (an ADR answers that) and not "what we learned doing it" (engineering knowledge answers that).

## Current state

The corpus is organized by subject area under `docs/specs/<class>/`. Every page follows the same sections in order — `Scope`, `Current state`, `Invariants`, `Where it lives`, `Change obligations` — and filenames carry no version number, date prefix, or iteration id.

| Layer | Location | Holds |
| --- | --- | --- |
| Knowledge | `.agents/knowledge/` | Reusable engineering experience and methodology |
| Specs | `docs/specs/` (this corpus) | Accurate description of the current state; latest-only |
| Change history | `.agents/adr/` | Decisions made among real alternatives, filed by fate and domain class |

## Invariants

- **Disjoint layers.** Experience belongs in `.agents/knowledge/`, a snapshot of current behavior belongs here, and "we chose X over Y and why" belongs in `.agents/adr/` (contract: `.agents/adr/README.md`).
- **Updated in place, no version suffixes.** A spec describes the latest state. Its history lives in Git and, where a change encodes a decision, in an ADR — never in a sibling `-v2` file.
- **No plan text.** These pages contain no acceptance criteria, local plan/iteration identifiers, merge references, or harness-internal process paths.
- **The registry is the catalog.** `mstar catalog list` reads a local, gitignored harness store; the table below is navigation, not a registry. Registration is a maintainer-local act and is not clone-shared.

## Where it lives

- Bodies: `docs/specs/<class>/<topic>.md` at the repository root.
- Resolution: `.mstarc` declares `specs_dir=docs/specs`; `mstar path resolve` prints the resolved `specsDir`.
- Registration: catalog rows carry identity, path, kind, relations, and lifecycle.

## Change obligations

- **Same-PR update duty.** A change that alters a path, name, default, command, or contract described in this corpus updates the affected spec in the same PR.
- **Adding a page.** Give it a subject directory, use the five sections in order, add it to the index below, and register it in the local catalog when one is active.
- **Removing a page.** Delete it instead of keeping a history directory; Git holds the history, and an ADR holds any decision worth outliving it.
- **Renaming or moving.** Fix every inbound link in the same change and re-register the local catalog row when one is active.

## Index

### `agents/`

| Document | Title |
| --- | --- |
| [`agents/acp-capability-set.md`](agents/acp-capability-set.md) | Nexus ACP Capability Set |
| [`agents/acp-client-tech-spec.md`](agents/acp-client-tech-spec.md) | ACP Client Integration — Technical Specification |
| [`agents/agent-host.md`](agents/agent-host.md) | Nexus Agent Host Architecture |
| [`agents/agent-nexus-tool-bridge.md`](agents/agent-nexus-tool-bridge.md) | Agent Nexus Tool Bridge — Normative Specification v1 |
| [`agents/capability-registry.md`](agents/capability-registry.md) | Capability Registry — Master v1 |
| [`agents/registry-integration.md`](agents/registry-integration.md) | Nexus ACP Registry Integration |

### `architecture/`

| Document | Title |
| --- | --- |
| [`architecture/actor-product-model.md`](architecture/actor-product-model.md) | Actor Product Model |
| [`architecture/agent-profile-service-persistence.md`](architecture/agent-profile-service-persistence.md) | Agent Profile Service Persistence |
| [`architecture/connect-event-delivery-and-operation-receipts.md`](architecture/connect-event-delivery-and-operation-receipts.md) | Connect Replay/Negotiation Wire + Operation Receipts |
| [`architecture/entity-scope-model.md`](architecture/entity-scope-model.md) | Nexus Entity Scope Model |
| [`architecture/holder-governance.md`](architecture/holder-governance.md) | Actor Holder Governance |
| [`architecture/local-runtime-boundary.md`](architecture/local-runtime-boundary.md) | Nexus Local Runtime Boundary |
| [`architecture/overview.md`](architecture/overview.md) | Nexus Architecture |
| [`architecture/rust-core-service-boundary.md`](architecture/rust-core-service-boundary.md) | Rust Core Service Boundary |
| [`architecture/schemas-directory-layout.md`](architecture/schemas-directory-layout.md) | `schemas/` Directory Layout (External-Consumer Contracts) |
| [`architecture/schemas-external-consumer-boundary.md`](architecture/schemas-external-consumer-boundary.md) | Schemas — External-Consumer Boundary |
| [`architecture/spoke-adapter-architecture.md`](architecture/spoke-adapter-architecture.md) | Spoke Adapter Architecture |
| [`architecture/spoke-memory-conformance-staging.md`](architecture/spoke-memory-conformance-staging.md) | Spoke Memory Conformance Staging |
| [`architecture/spoke-memory-provider-binding.md`](architecture/spoke-memory-provider-binding.md) | Spoke Memory Provider Binding |
| [`architecture/world-kb-runtime-architecture.md`](architecture/world-kb-runtime-architecture.md) | World KB Runtime Architecture |

### `cli/`

| Document | Title |
| --- | --- |
| [`cli/cli-spec.md`](cli/cli-spec.md) | Nexus CLI Spec |

### `compute/`

| Document | Title |
| --- | --- |
| [`compute/compute-module-abi.md`](compute/compute-module-abi.md) | Nexus Compute Module ABI (V1 Envelope) |
| [`compute/wasm-host.md`](compute/wasm-host.md) | Nexus WASM Compute Host (nexus-wasm-host) |

### `contracts/`

| Document | Title |
| --- | --- |
| [`contracts/canonical-hash.md`](contracts/canonical-hash.md) | Delta bundle `canonical_hash` (OSS companion) |
| [`contracts/findings-lifecycle.md`](contracts/findings-lifecycle.md) | Findings Lifecycle — Cross-Profile Master |
| [`contracts/world-delta-propose-apply.md`](contracts/world-delta-propose-apply.md) | World Delta Propose / Apply — Local Parity Overlay |

### `creator/`

| Document | Title |
| --- | --- |
| [`creator/creator-challenge-solver.md`](creator/creator-challenge-solver.md) | Creator Challenge Solver（CLI 侧 — 冻结） |
| [`creator/creator-memory-soul-lifecycle.md`](creator/creator-memory-soul-lifecycle.md) | Creator Memory SOUL Lifecycle — Specification v1 |
| [`creator/creator-workflow.md`](creator/creator-workflow.md) | Creator Workflow — Normative Specification |
| [`creator/essay-profile.md`](creator/essay-profile.md) | Essay Profile — Specification v1 |
| [`creator/game-bible-profile.md`](creator/game-bible-profile.md) | Game-Bible Profile — Master Specification V1.56 |
| [`creator/script-profile.md`](creator/script-profile.md) | Script Profile — Master Specification V1.60 |
| [`creator/work-experience-model.md`](creator/work-experience-model.md) | Work Experience Model — Normative Specification v1 |

### `novel-writing/`

| Document | Title |
| --- | --- |
| [`novel-writing/author-experience.md`](novel-writing/author-experience.md) | Novel Author Experience — Normative Supplement v1 |
| [`novel-writing/manuscript-audit.md`](novel-writing/manuscript-audit.md) | Novel Manuscript Audit — Normative Specification v1 |
| [`novel-writing/multi-work-lifecycle.md`](novel-writing/multi-work-lifecycle.md) | Novel Multi-Work Lifecycle — Normative Specification v1 |
| [`novel-writing/quality-loop.md`](novel-writing/quality-loop.md) | Novel Quality Loop — Normative Specification v1 |
| [`novel-writing/sync-contract.md`](novel-writing/sync-contract.md) | Novel-Writing Sync Module Contract |
| [`novel-writing/work-pool.md`](novel-writing/work-pool.md) | Novel Work Pool — Normative Specification v1 |
| [`novel-writing/workflow-profile.md`](novel-writing/workflow-profile.md) | Novel Workflow Profile — Normative Specification v1 |

### `orchestration/`

| Document | Title |
| --- | --- |
| [`orchestration/creator-schedule-and-core-context.md`](orchestration/creator-schedule-and-core-context.md) | Creator Schedule and Core Context — Specification |
| [`orchestration/llm-extract.md`](orchestration/llm-extract.md) | Nexus LLM Extract Capability |
| [`orchestration/orchestration-engine.md`](orchestration/orchestration-engine.md) | Orchestration Engine — Design Specification |
| [`orchestration/preset-conditional-routing.md`](orchestration/preset-conditional-routing.md) | Preset Conditional Routing — Specification |

### `runtime/`

| Document | Title |
| --- | --- |
| [`runtime/chapter-content-local-api.md`](runtime/chapter-content-local-api.md) | Chapter Content Daemon API |
| [`runtime/concurrency.md`](runtime/concurrency.md) | Concurrency — Master Specification |
| [`runtime/daemon-api-surface-conventions.md`](runtime/daemon-api-surface-conventions.md) | Daemon API Surface Conventions |
| [`runtime/embedding-readiness.md`](runtime/embedding-readiness.md) | Embedding Readiness Contract (RN-OGA-3) |
| [`runtime/local-db-schema.md`](runtime/local-db-schema.md) | Nexus Local DB Schema Spec |
| [`runtime/outbox-consolidation.md`](runtime/outbox-consolidation.md) | Outbox Consolidation — Single-Writer Contract & Schema Ownership |
| [`runtime/reference-knowledge.md`](runtime/reference-knowledge.md) | Reference Knowledge — Reference Body Refresh Pipeline (Retired V1.231) |
| [`runtime/reference-store-layout.md`](runtime/reference-store-layout.md) | Reference Store Layout (Retired V1.231) |
| [`runtime/web-ui-static-serve.md`](runtime/web-ui-static-serve.md) | Web UI Static Serve Contract |

### `surfaces/`

| Document | Title |
| --- | --- |
| [`surfaces/canvas-strategy-surface.md`](surfaces/canvas-strategy-surface.md) | Canvas Strategy Surface — Specification |
| [`surfaces/design-studio.md`](surfaces/design-studio.md) | Design Studio — Specification |
| [`surfaces/desktop-shell.md`](surfaces/desktop-shell.md) | Desktop Shell — Product and Host Contract |
| [`surfaces/web-rebuild-access.md`](surfaces/web-rebuild-access.md) | Web Access Model — Shipped |
