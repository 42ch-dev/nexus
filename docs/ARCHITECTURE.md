# Nexus Architecture

Directional map of the **nexus** open-source monorepo: how product surfaces relate, which boundaries are hard, and where authority lives.

This document is for **orientation and decision-making**. It does not inventory crates, routes, CLI flags, or on-disk paths — those change often and are discoverable from code and normative specs.

| Need | Where to look |
|------|----------------|
| Product vision & tech rationale | [`STRATEGY.md`](../STRATEGY.md) |
| Domain vocabulary | [`CONCEPTS.md`](../CONCEPTS.md) |
| Day-to-day commands | [`README.md`](../README.md) → Development; [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Entity ownership & naming | [`.mstar/specs/entity-scope-model.md`](../.mstar/specs/entity-scope-model.md) |
| Local vs cloud crate rules | [`.mstar/specs/local-cloud-crate-architecture.md`](../.mstar/specs/local-cloud-crate-architecture.md) |
| Daemon trust / API classes | [`.mstar/specs/daemon-runtime.md`](../.mstar/specs/daemon-runtime.md), [`.mstar/specs/local-runtime-boundary.md`](../.mstar/specs/local-runtime-boundary.md) |
| Per-directory invariants | Root [`AGENTS.md`](../AGENTS.md) and each subtree’s `AGENTS.md` |

---

## What this repo is

Nexus OSS is a **local-first creative writing runtime** plus the clients that talk to it: the CLI/daemon producer, open-source UIs, and shared wire contracts published as `@42ch/nexus-contracts`.

Three product surfaces, one producer:

| Surface | Role | Rule of thumb |
|---------|------|----------------|
| **`nexus42`** | **Producer** — CLI + integrated daemon | Owns lifecycle, local persistence, Daemon API |
| **`apps/web`** | **Consumer** — Control Room + canvas SPA | Talks only through contracts + client interface |
| **`apps/desktop`** | **Consumer** — Tauri shell around `web` | Native extras only; no second SPA or DTO set |

Placement rule: runnable product surfaces → `apps/`; reusable Rust libraries → `crates/`; publishable npm libraries → `packages/`. App-owned nested Rust (e.g. `apps/desktop/src-tauri/`) stays with the surface until it becomes a shared building block.

---

## Two product lines

| Line | Purpose | Surface |
|------|---------|---------|
| **Local** | Daemon, orchestration, agent host, Creator / memory, narrative, World KB, User knowledge, moment assembly, web/desktop UI | `nexus42 daemon` → `/v1/daemon/*`; clients over that API |
| **Cloud enhancement** | Platform HTTP, sync, registration, optional cloud context stage | CLI / `nexus-cloud-sync` — **never** the Daemon API |

**Hard isolation:** the daemon runtime must not depend on cloud sync or cloud-domain crates, and must not expose cloud HTTP or sync proxies. Local UI is part of the local line: it talks only to the Daemon API and must not assume cloud-only product surfaces.

Operational actor for agents and orchestration is **`Creator`**. `User` / `Pairing` are platform-bridge concepts owned by the cloud domain.

---

## Contracts are truth

Cross-language wire shapes start in **`schemas/`** (JSON Schema). Codegen produces Rust (`crates/nexus-contracts`) and TypeScript (`@42ch/nexus-contracts`). Platform and OSS clients must consume those types — **no parallel handwritten DTO sets**.

- Types observed by platform or sync bundles live in schemas → generated contracts.
- Local-only daemon/orchestration shapes may live under `nexus-contracts` local modules when platform does not observe them; they still must not be redefined in app crates.
- After schema edits: validate → codegen → commit schemas and generated output together.

Schema layout and external-consumer boundary: [`.mstar/specs/schemas-directory-layout.md`](../.mstar/specs/schemas-directory-layout.md), [`.mstar/knowledge/schemas-external-consumer-boundary.md`](../.mstar/knowledge/schemas-external-consumer-boundary.md).

---

## Entity scope (ownership map)

Canonical hierarchy (normative detail in the entity-scope model):

```text
Global
└── User
    ├── Creator
    │   └── World
    │       ├── Timeline → Event → Moment
    │       └── KB graph / narrative knowledge assets
    └── User knowledge index
```

Guiding rules:

1. Every scoped entity has **exactly one** owning scope and primary owner crate.
2. **World history is immutable** — change via Fork, not in-place rewrite.
3. **`nexus-knowledge`** owns World-scoped KnowledgeEntries / SourceAnchors (merged from former `nexus-kb` in V1.139) and User-scoped knowledge. Do not conflate either with the CLI/daemon **work file index** under work-scope `kb` routes.
4. **`nexus-narrative`** coordinates World / Timeline / Event state; forks are platform-oriented where the entity model says so.
5. **`nexus-moment-context-assembly`** owns session-start context assembly (`assemble_moment` is the assembly SSOT).
6. A local `workspace_slug` is a **storage partition** under Creator, not a new entity scope.

When naming or placing a feature, ask: *which scope owns this, and which crate may mutate it?* Prefer the entity-scope model over copying older plan wording.

---

## Layering & dependency direction

```text
schemas/  →  contracts (Rust + npm)
                ↑
        domain / runtime crates
                ↑
     apps/nexus42 (composition root)
                ↑
     apps/web  ←──  apps/desktop (wraps web)
```

- **Foundation:** contracts, home layout, local DB mechanics — no product HTTP.
- **Domain crates:** Creator, memory, narrative, KB, knowledge, orchestration, agent host — own invariants for their scopes.
- **Daemon runtime:** local supervisor + HTTP surface; composes local domain crates only.
- **CLI binary:** composition root for daemon start, ACP client paths, and cloud CLI features (feature-gated so daemon builds stay free of cloud deps).
- **UI:** depends on contracts and a transport-agnostic client interface; screens must not call `fetch` / Tauri `invoke` directly.

**Cargo edge ≠ product integration.** Linking a crate does not mean every HTTP route or CLI command uses it. Prefer domain APIs over legacy file/SQLite shortcuts when extending behavior; do not invent a second ownership path for the same entity.

---

## Client & transport boundaries

| Boundary | Decision |
|----------|----------|
| **Daemon API** | Local HTTP under `/v1/daemon/*`. Default bind is loopback. Remote bind is opt-in and must stay fail-closed (API key + remote flag + TLS for non-loopback). |
| **NexusClient** | UI depends on the interface; browser vs Tauri implementations swap at the edge. |
| **Desktop** | Same SPA as web; Tauri adds sidecar lifecycle and OS affordances (open / reveal / path guard). Path checks are authoritative in Rust against the active workspace root. |
| **ACP** | Nexus is an **ACP client**, not an agent/server. Agent host adapts to the user’s local agents; do not ship a competing agent runtime or public ACP server surface on the daemon. |

Forbidden on the daemon: sync proxies, platform registration passthrough, treating the runtime as an ACP Agent/Server.

---

## Design & UI ownership

- **DESIGN.md / DESIGN.dark.md** at repo root are the brand/token SSOT. Surfaces consume shared projection (`@nexus/design-tokens`); they do not invent parallel tokens.
- **`@42ch/nexus-ui`** holds publishable brand and promoted presentational primitives. Product screens, routing, and daemon wiring stay in `apps/web`.
- **Design Studio** (`apps/design-studio`) is a contributor gallery — not shipped to authors and not daemon-coupled.

Studio-first and promotion rules live under design-studio / UI specs and guardrail scripts; follow those when moving primitives across packages.

---

## Pre-1.0 persistence stance

Breaking changes to API shapes, CLI flags, config, and on-disk layout are **allowed without a deprecation period**. Local data may be wiped rather than migrated.

Guidance for contributors:

- Treat `~/.nexus42/` and workspace-local state as **working copies**, not a stable public storage API.
- Domain semantics belong in owner crates (`nexus-local-db`, `nexus-creator-memory`, `nexus-knowledge`, …). File caches in the CLI composition root are conveniences, not long-term SSOT.
- Prefer structured deltas/bundles for sync — not full manuscript upload by default.
- Document intentional wipe/reset UX when a change invalidates local DBs (authors should not need to reverse-engineer migrations).

Exact path layouts: `nexus-home-layout` and local-db specs — do not duplicate them here.

---

## Standing constraints

These are architectural invariants, not style preferences:

1. **Local-first** — cloud is optional enhancement.
2. **Wire contracts single-sourced** — schemas → codegen; no app-local wire DTOs.
3. **Daemon ≠ ACP server** — client/host only.
4. **No daemon↔cloud Cargo or HTTP coupling.**
5. **World history immutable** — Fork, not rewrite.
6. **One scope owner per entity** — resolve naming (`kb` vs `knowledge` vs work index) via the entity-scope model.
7. **Consumers stay thin** — web/desktop do not re-own domain logic or invent second contracts.
8. **Simplicity** — do not abstract before proven need (see Strategy).

---

## Normative reading order

When designing or reviewing a change:

1. Root [`AGENTS.md`](../AGENTS.md) — repo invariants
2. This file — orientation
3. [`entity-scope-model.md`](../.mstar/specs/entity-scope-model.md) — ownership
4. [`local-cloud-crate-architecture.md`](../.mstar/specs/local-cloud-crate-architecture.md) — line split & forbidden edges
5. Domain Master for the subsystem (daemon, orchestration, CLI, web-ui, desktop-shell, …) under [`.mstar/specs/`](../.mstar/specs/)

Iteration plans and audit compasses record delivery history; they are not architecture SSOT. Prefer Master specs over dated gap tables when deciding what “should” be true.
