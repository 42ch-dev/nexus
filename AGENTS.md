# Nexus AGENTS.md

This file provides decision rules, invariants, and indexes for agents working in the **nexus** open-source monorepo.
Domain-specific rules live in subdirectory AGENTS.md files listed below.

## Repository Identity

This is the **public open-source monorepo** containing `nexus42` CLI (Rust, with integrated daemon runtime), JSON Schema wire contracts (truth source for TypeScript/Rust codegen), and published package `@42ch/nexus-contracts` (npm). Rust `nexus-contracts` crate is monorepo-internal only.

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

**Harness `status.json`:** Open QC residual rows are stored under **root** `residual_findings` in `.agents/status.json`. Details: [`.agents/AGENTS.md`](.agents/AGENTS.md).

## Tech Stack & Protocol Decisions

- **CLI/daemon:** Rust-first (aligns with ACP official SDK availability)
- **Protocol:** ACP-first, skills-second — CLI is an ACP client, not an ACP agent/server
- **Wire format:** JSON Schema as truth source — generates both TypeScript and Rust types

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: **nexus42**
- Daemon: **nexus42d** (integrated into `nexus42` binary)
- npm scope: **@42ch**
- Contracts package: **@42ch/nexus-contracts**

## Subdirectory Index

See linked AGENTS.md files for per-directory decision rules and invariants:

| Directory | Scope | AGENTS.md |
|-----------|-------|-----------|
| `schemas/` | JSON Schema wire contracts | [`schemas/AGENTS.md`](schemas/AGENTS.md) |
| `tooling/` | Codegen pipeline & CI | [`tooling/AGENTS.md`](tooling/AGENTS.md) |
| `crates/nexus42/` | CLI executable | [`crates/nexus42/AGENTS.md`](crates/nexus42/AGENTS.md) |
| `crates/nexus-acp-host/` | ACP client adapter | [`crates/nexus-acp-host/AGENTS.md`](crates/nexus-acp-host/AGENTS.md) |
| `crates/nexus-contracts/` | Generated Rust wire types | [`crates/nexus-contracts/AGENTS.md`](crates/nexus-contracts/AGENTS.md) |
| `crates/nexus-home-layout/` | `~/.nexus42/` path layout | [`crates/nexus-home-layout/AGENTS.md`](crates/nexus-home-layout/AGENTS.md) |
| `crates/nexus-local-db/` | Local database layer | [`crates/nexus-local-db/AGENTS.md`](crates/nexus-local-db/AGENTS.md) |
| `crates/nexus-orchestration/` | Orchestration engine | [`crates/nexus-orchestration/AGENTS.md`](crates/nexus-orchestration/AGENTS.md) |
| `.agents/` | Harness infrastructure | [`.agents/AGENTS.md`](.agents/AGENTS.md) |

**New crate policy:** when adding a new package or crate to the monorepo, create an `AGENTS.md` in that directory — even if minimal — documenting its purpose, key rules, and dependencies.

## Development Policy

**Formatting:** `cargo fmt` must use the **nightly** toolchain: `cargo +nightly fmt --all`. Stable `cargo fmt` ignores `.rustfmt.toml`'s `ignore` field and will **incorrectly reformat** generated code under `crates/nexus-contracts/src/generated/`.

**Clippy:** Workspace-level config in root `Cargo.toml` enables `pedantic` + `nursery` as `warn`, inherited by all crates. CI runs `cargo clippy --all -- -D warnings`. When fixing clippy errors, auto-fix first (`cargo clippy --fix --allow-dirty --allow-staged`), then handle residual manually. **Do not suppress** with `#[allow(...)]` without a brief justification comment. **Do not change runtime behavior** when fixing lint errors.

**Git worktrees:** Place every additional `git worktree` checkout under this repository root at `.worktrees/<name>/` only (`.worktrees/` is gitignored).

## Versioning Policy

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI / daemon SemVer must reflect breaking wire changes
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- npm and Rust workspace versions may differ; `schema_version` is the cross-language lock

## Pre-release Development (Version < 1.0)

Breaking changes are expected and allowed — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period. Local persistence may be wiped rather than migrated. After first release, follow SemVer.

## Constraints & Pitfalls

- **Do not treat the daemon runtime as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript

## TypeScript Contract Package (cross-repo)

`nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock. **No handwritten second DTO source** in platform — all wire types come from this repo's schemas.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **nexus** (9467 symbols, 22295 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

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
