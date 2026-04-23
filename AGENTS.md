# Nexus AGENTS.md

This file provides an overview for agents working in the **nexus** open-source monorepo. Domain-specific rules live in subdirectory AGENTS.md files listed below.

## Repository Identity

This is the **public open-source monorepo** containing:

- `nexus42` CLI executable (Rust)
- `nexus42d` daemon/supervisor (Rust)
- JSON Schema wire contracts (truth source for TypeScript/Rust code generation)
- Published packages: `@42ch/nexus-contracts` (npm) and `nexus-contracts` (crates.io)

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

## Tech Stack & Protocol Decisions

- **CLI/daemon:** Rust-first (aligns with ACP official SDK availability)
- **Protocol:** ACP-first, skills-second — CLI is an ACP client, not an ACP agent/server
- **Wire format:** JSON Schema as truth source — generates both TypeScript and Rust types

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: `**nexus42`**
- Daemon: `**nexus42d`**
- npm scope: `**@42ch**`
- Contracts package: `**@42ch/nexus-contracts**`

## Monorepo Structure

```
schemas/                # JSON Schema truth source → see schemas/AGENTS.md
crates/
  nexus-contracts/      # Generated Rust types → see crates/nexus-contracts/AGENTS.md
  nexus42/              # CLI binary → see crates/nexus42/AGENTS.md
  nexus42d/             # Daemon → see crates/nexus42d/AGENTS.md
  nexus-sync/           # Bundle/outbox state machine (library)
  nexus-acp-host/       # ACP client adapter → see crates/nexus-acp-host/AGENTS.md
  nexus-domain/         # Domain types and logic
  nexus-home-layout/    # ~/.nexus42/ path layout → see crates/nexus-home-layout/AGENTS.md
  nexus-local-db/       # Local database layer → see crates/nexus-local-db/AGENTS.md
  nexus-orchestration/  # Orchestration engine → see crates/nexus-orchestration/AGENTS.md
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm package)
tooling/
  codegen/              # Schema → TS + Rust pipeline → see tooling/AGENTS.md
docs/                   # User & contributor docs
.agents/                # Harness infrastructure → see .agents/AGENTS.md
.github/workflows/      # CI
```

## Subdirectory Conventions

| Directory | Scope | Key Rules |
|-----------|-------|-----------|
| [`schemas/AGENTS.md`](schemas/AGENTS.md) | JSON Schema wire contracts | Schema URI, codegen flow, mandatory regeneration |
| [`tooling/AGENTS.md`](tooling/AGENTS.md) | Codegen pipeline & CI | Pre-merge checklist, formatting, linting |
| [`crates/nexus42/AGENTS.md`](crates/nexus42/AGENTS.md) | CLI executable | ACP client behavior, shared contract types, daemon control |
| [`crates/nexus42d/AGENTS.md`](crates/nexus42d/AGENTS.md) | Daemon / supervisor | Not an ACP server, sqlx macros, database migrations |
| [`crates/nexus-acp-host/AGENTS.md`](crates/nexus-acp-host/AGENTS.md) | ACP client adapter | ACP protocol rules, official SDK usage |
| [`crates/nexus-contracts/AGENTS.md`](crates/nexus-contracts/AGENTS.md) | Generated Rust wire types | No hand-editing generated code, `enum_conversions.rs` |
| [`crates/nexus-home-layout/AGENTS.md`](crates/nexus-home-layout/AGENTS.md) | `~/.nexus42/` path layout | ADR-014 canonical paths, no hardcoded paths |
| [`crates/nexus-local-db/AGENTS.md`](crates/nexus-local-db/AGENTS.md) | Local database layer | Migrations, sqlx compile-time macros |
| [`crates/nexus-orchestration/AGENTS.md`](crates/nexus-orchestration/AGENTS.md) | Orchestration engine | Embedded presets, validation rules |
| [`.agents/AGENTS.md`](.agents/AGENTS.md) | Harness infrastructure | Plans, residuals, knowledge, QC/QA, upstream mstar-harness |

**New crate policy:** when adding a new package or crate directory to the monorepo, create an `AGENTS.md` in that directory documenting its purpose, key rules, and dependencies — even if it starts minimal. This keeps the onboarding index complete.

## Development Workflow

**Git worktrees:**

- Put every additional `git worktree` checkout under **this repository root** at `.worktrees/<name>/` only.
- The `.worktrees/` directory is listed in `.gitignore`.
- Example: `git worktree add .worktrees/my-branch -b my-branch`

## Dev/Test Infrastructure

**Required containers:** Postgres + pgvector (`pgvector/pgvector:pg16`), Neo4j (`neo4j:5`), Redis (`redis/redis-stack-server:latest`)

**API keys** (external, not in this repo's code but needed for integration): LLM inference API, OAuth/IdP credentials

## Versioning & Compatibility

### Package versions (current snapshot)

| Deliverable | Version | Declared in |
|---|---|---|
| Rust workspace crates | **0.1.0** | Root `Cargo.toml` → `[workspace.package] version` |
| `nexus-contracts` on crates.io | **0.1.0** | Same; publish from `crates/nexus-contracts` |
| `@42ch/nexus-contracts` (npm) | **0.3.0** | `packages/nexus-contracts/package.json` |
| `nexus-codegen` (private tooling) | **0.1.0** | `tooling/codegen/package.json` |
| Root `nexus-monorepo` meta package | **0.1.0** | Root `package.json` |

**npm vs Rust crate SemVer:** The npm package may use a **different** semantic version than the Rust workspace while both implement the same `LATEST_SCHEMA_VERSION` on the wire. Treat `schema_version` as the cross-language lock.

### Policy

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI / daemon crate SemVer must reflect breaking wire changes when you version the binaries
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package

## Pre-release Development (Version < 1.0)

Breaking changes are expected and allowed — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period. Local persistence (SQLite, `~/.nexus42/`) may be wiped rather than migrated. After first release, follow SemVer.

## Constraints & Pitfalls

- **Do not treat `nexus42d` as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript

## TypeScript Contract Package

- `nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock
- **No handwritten second DTO source** in platform — all wire types come from this repo's schemas
- **SemVer:** Breaking wire shapes require a **major** npm bump and coordinated platform upgrade

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **nexus** (7607 symbols, 17777 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/nexus/context` | Codebase overview, check index freshness |
| `gitnexus://repo/nexus/clusters` | All functional areas |
| `gitnexus://repo/nexus/processes` | All execution flows |
| `gitnexus://repo/nexus/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
