# Nexus AGENTS.md

This file provides development guidance for agents working in the **nexus** open-source repository.

> For harness directory layout, plan conventions, residual tracking, and verification commands, see [`.agents/AGENTS.md`](.agents/AGENTS.md).

## Repository Identity

This is the **public open-source monorepo** containing:

- `nexus42` CLI executable (Rust)
- `nexus42d` daemon/supervisor (Rust)
- JSON Schema wire contracts (truth source for TypeScript/Rust code generation)
- Published packages: `@42ch/nexus-contracts` (npm) and `nexus-contracts` (crates.io)

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

## Tech Stack & Protocol Decisions

- **CLI/daemon: Rust-first** (not Go/Python) — aligns with ACP official SDK availability
- **Protocol: ACP-first, skills-second** — CLI is an ACP client, not an ACP agent/server
- **Wire format: JSON Schema as truth source** — generates both TypeScript and Rust types
- **Platform direction: TypeScript/Next.js/Vercel AI SDK** — but this repo only publishes wire contracts, not platform code

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: `**nexus42`**
- Daemon: `**nexus42d`**
- npm scope: `**@42ch**`
- Contracts package: `**@42ch/nexus-contracts**`

## Monorepo Structure (Target)

```
schemas/                # JSON Schema truth source (codegen input)
crates/
  nexus-contracts/      # Generated Rust types
  nexus42/              # CLI binary
  nexus42d/             # Daemon
  nexus-sync/           # Bundle/outbox state machine (library)
  nexus-acp-*/          # ACP client adapters (optional subcrates)
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm package)
tooling/
  codegen/              # Schema → TS + Rust pipeline
docs/                   # User & contributor docs (installation, architecture, codegen, contributing)
.agents/                # Harness infrastructure (see .agents/AGENTS.md)
  knowledge/            # Dev-process knowledge (architecture reviews, design decisions)
  archived/             # Closed plan snapshots, residuals, superseded knowledge
  status.json           # SSOT: active plans + open residual_findings
  notes.json            # Cross-plan program timeline
  plans/                # {PLAN_DIR}: plan .md files + reports/
.github/workflows/      # CI: see `.github/workflows/ci.yml` (schemas, codegen diff, fmt, clippy, TS typecheck)
```

## Content Boundary: `docs/` vs `.agents/knowledge/`

### `docs/` — User & Contributor Documentation

End-user and contributor-facing content that anyone cloning the repo should read:

- Installation, quickstart, usage guides
- Architecture overview (high-level, stable)
- Code generation workflow
- Contributing guidelines

**Do NOT place** the following in `docs/`:

- Architecture review reports, spec revision outputs, gap analyses
- Per-plan design decisions or implementation notes
- Any document that is an **input to** or **output from** a specific plan

### `.agents/knowledge/` — Dev-Process Knowledge

Development process artifacts generated during planning and review: architecture review reports, design decision records, gap analyses, and any document that serves as **context for implementing a plan**. These are valuable for agent handoff but not intended for external consumers.

**Index**: All knowledge documents are catalogued in [`.agents/knowledge/README.md`](.agents/knowledge/README.md). Maintenance rules (adding, reading, updating, archiving) are in [`.agents/AGENTS.md`](.agents/AGENTS.md).

## External Design Specs

Nexus is an **open-source repo** but design specs live in the **private `nexus-platform` repo**. Configure paths via `.agents/local-paths.json` (copy from `.agents/local-paths.json.example`; the file is gitignored). Once configured, resolve `specs_root` for:
- **Roadmap**: `{specs_root.roadmap}` — update when a plan is marked `Done` (see pre-merge checklist in `.agents/AGENTS.md`)
- **Architecture**: `{specs_root.v1-spec}/architecture/v1.md`
- **Domain Model**: `{specs_root.v1-spec}/domain/data-model-v1.md`

## Documentation & plans (mandatory reachability)

All in-repo documentation and agent plans MUST be reachable from a fresh `git clone`:

- **Do not** reference `.gitignore`-excluded or out-of-repo paths (e.g. `~/.config/...`, absolute home paths, arbitrary sibling directories). Inline external context or link to stable public URLs.
- **Do not** paste machine-specific paths (`/Users/<you>/...`, `C:\Users\...`) in tracked artifacts — use repo-relative paths or neutral placeholders (`<repository-root>`).

Violations break onboarding and agent handoff.

## Plans & Reports Structure

> **Full details** — directory layout, file naming, residual tracking, archival rules, verification commands, and pre-merge checklist — are in [`.agents/AGENTS.md`](.agents/AGENTS.md). Only a brief overview is kept here.

### Harness alignment

Plan conventions follow **[Harness Engineering](https://github.com/btspoony/mstar-harness)** upstream defaults. `{HARNESS_DIR}` = `.agents/`, `{PLAN_DIR}` = `.agents/plans/`.

### `{PLAN_DIR}` discovery

Resolve in order (first match wins): `.agents/plans/` → `.plans/` → `plans/`. **This repository** uses `.agents/plans/`. New plan files land under `{PLAN_DIR}`, not `docs/superpowers/plans/`.

### Plan Lifecycle

1. **Todo**: Plan created, not started
2. **InProgress**: Implementation underway
3. **InReview**: QC review in progress (reports in `reports/<plan-id>/`)
4. **Blocked**: Waiting on dependency, decision, or another plan
5. **Done**: Completed, merged to main; row archived to `{HARNESS_DIR}/archived/plans/`

**Multi-batch plans:** Default QC triple-review **once** after the whole plan's dev work completes (not per batch); see upstream `plan-convention.md`.

### Pre-merge checklist (summary)

Before merging plan work: update `status.json` (plans, residuals, gates, timeline); run codegen and commit regenerated output if schemas changed; update `roadmap.md` in `nexus-platform` if a plan is marked `Done`. See [`.agents/AGENTS.md`](.agents/AGENTS.md) for the full checklist.

## Development Workflow

**Git worktrees:**

- Put every additional `git worktree` checkout under **this repository root** at `.worktrees/<name>/` only. Do not add worktrees in arbitrary sibling directories outside the clone.
- The `.worktrees/` directory is listed in `.gitignore`; it keeps parallel branches in one predictable place for tooling and handoff.
- Example: `git worktree add .worktrees/my-branch -b my-branch`

**Schema/codegen flow:**

- JSON Schema (`schemas/`) → single codegen pass → Rust (`crates/nexus-contracts`) + TypeScript (`packages/nexus-contracts`)
- Both packages must be published and version-locked with `schema_version`
- CI validates schemas before generating code

**Schema URI placeholder (production domain TBD):** Committed schema files use `https://nexus42.invalid` in `$id` / `$ref` paths (RFC 6761 reserved name). In prose docs, use `{NEXUS42_BASE_URL}` as the origin placeholder. Do **not** embed `{NEXUS42_BASE_URL}` inside JSON `$id` / `$ref` strings. See `schemas/meta/README.md` and `docs/CODEGEN.md`.

**⚠️ Mandatory: run codegen after any schema change**

The CI job `verify-codegen` runs `pnpm run codegen` and then checks `git diff` on the generated output directories (`packages/nexus-contracts/src/generated/`, `crates/nexus-contracts/src/generated/`). If generated files are out of sync with committed versions, **CI will fail**.

Rule: **any commit that touches files under `schemas/` MUST also include the corresponding regenerated output**. Before committing:

```bash
pnpm run codegen
git add packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

If you modify schemas without regenerating, the commit will be rejected by CI. Do NOT hand-edit files under `*/generated/` — always regenerate from schemas.

- **enum_conversions.rs (Rust):** `crates/nexus-contracts/src/enum_conversions.rs` is maintained **next to** generated types, not produced by codegen. When JSON Schema adds or renames enum values, update this file in the same commit as regenerated `src/generated/` and verify with `cargo test -p nexus-contracts`.

**Before opening a PR or merging to `main`:** run the same checks as the `CI` workflow (`.github/workflows/ci.yml`) so local results match GitHub Actions.

```bash
# 1) JSON Schemas (pnpm install at repo root, then:)
pnpm run validate-schemas

# 2) Codegen matches committed output (must produce no diff on generated dirs)
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/

# 3) Rust formatting (nightly rustfmt required — see below)
cargo +nightly fmt --all -- --check

# 4) Rust lints (warnings fail CI)
cargo clippy --all -- -D warnings

# 5) TypeScript contract package
pnpm install   # if needed
pnpm run typecheck
```

`CI` does not run `cargo test`; run `cargo test --all` locally when you touch Rust behavior.

**Rust development:**

- Use official ACP Rust SDK (not custom protocol implementations)
- CLI and daemon share generated contract types from `crates/nexus-contracts`
- Daemon is `nexus42d`, started via `nexus42 daemon start`
- **Formatting:** use `cargo +nightly fmt --all` before commit. The workspace `.rustfmt.toml` ignores `crates/nexus-contracts/src/generated/` (stable `cargo fmt` cannot apply `ignore`, and formatting generated Rust would desync CI `verify-codegen` from `pnpm run codegen`). Install once: `rustup toolchain install nightly --component rustfmt`

## sqlx Compile-Time Macros (Mandatory)

### Default: use compile-time macros

All new sqlx queries **MUST** use compile-time checked macros:

- Use `sqlx::query!("SQL", params...)` for execute-only statements.
- Use `sqlx::query_as!(Type, "SQL", params...)` for queries returning typed rows.
- Use `sqlx::query_scalar!("SQL", params...)` for single-value returns.

**Do NOT** use runtime `sqlx::query()` or `sqlx::query_as::<T>()` for static SQL.

### When runtime `sqlx::query()` is acceptable

Runtime queries are **only** acceptable for:

1. **DDL statements** (`CREATE TABLE`, `CREATE INDEX`, `ALTER TABLE`) — sqlx macros cannot validate DDL.
2. **PRAGMA statements** — no table schema to validate.
3. **Truly dynamic SQL** — where the query string is constructed at runtime based on user input or configuration. Each such usage MUST include a `// SAFETY: dynamic SQL — compile-time macro not applicable.` comment explaining why a macro cannot be used.

If in doubt, use the macro.

### DATABASE_URL convention

All sqlx metadata generation uses a single, self-contained SQLite file as the analysis database:

```
DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
```

- **Location:** `.sqlx/state.db` (repo-relative, next to the metadata directory).
- **`?mode=rwc`** creates the file on first access; no manual `sqlite3` setup needed.
- **Pre-existing data:** if `.sqlx/state.db` already exists, `cargo sqlx prepare` picks up the current schema automatically (no `database reset` needed for routine query changes).
- **`cargo sqlx database reset`** is only required when **adding new migrations** under `crates/nexus-local-db/migrations/`.

### `.sqlx/` commit and ignore rules

The `.sqlx/` directory contains query metadata (JSON files) used by sqlx compile-time macros. These files are **committed to git** so that CI can verify queries without a live database.

- **Commit:** all `*.json` metadata files under `.sqlx/` (auto-generated by `cargo sqlx prepare`).
- **Gitignore:** `state.db`, `state.db-wal`, `state.db-shm` — these are local build artifacts, not needed by CI or other contributors.

### Adding new queries or migrations

When adding or modifying SQL queries:

1. Write the query using `sqlx::query!()` / `sqlx::query_as!()`.
2. Run `cargo sqlx prepare --workspace --all -- --all-targets` to update `.sqlx/` metadata.
3. Commit the updated `.sqlx/` files alongside your code changes.

When adding new migrations under `crates/nexus-local-db/migrations/`:

1. Write the migration SQL file.
2. Run `export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc" && cargo sqlx database reset && cargo sqlx prepare --workspace --all -- --all-targets`.
3. Commit `crates/nexus-local-db/migrations/` **and** `.sqlx/` in the same commit.

CI will reject PRs where `.sqlx/` is out of sync with the committed macro invocations.

**TypeScript contract package:**
- `nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock
- **No handwritten second DTO source** in platform — all wire types come from this repo's schemas
- **SemVer:** bump npm version together with `schema_version` / Rust crate per release policy. Breaking wire shapes require a **major** npm bump and coordinated platform upgrade.

## Dev/Test Infrastructure

**Required containers:** Postgres + pgvector (`pgvector/pgvector:pg16`), Neo4j (`neo4j:5`), Redis (`redis/redis-stack-server:latest`)

**API keys** (external, not in this repo's code but needed for integration): LLM inference API, OAuth/IdP credentials

**CLI-only note:** ACP Registry is public (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`); CLI pulls from it, no API key required.

## Pre-release development (Version in Cargo.toml < 1.0)

Until first release is explicitly shipped and communicated, this repository and its deliverables are in **early / pre-release** development.

- **Breaking changes are expected and allowed** — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period or compatibility layer unless the team deliberately chooses one.
- **Local persistence (SQLite, `~/.nexus42/`, workspace layout):** do **not** treat pre-1.0 user data as a long-term migration contract. When schema or layout changes, it is acceptable to **replace, wipe, or require re-init** rather than building multi-version upgrade paths. Prefer the smallest implementation that matches the current spec; document notable breaks in PRs or plan notes when useful.
- **After first release**, tighten expectations: follow SemVer for published packages and binaries, coordinate wire `schema_version` / npm majors as in **Versioning & Compatibility** below, and treat end-user data + upgrade paths as product commitments unless explicitly scoped as experimental.

## Versioning & Compatibility

### Wire `schema_version` (generated SSOT)

- `**LATEST_SCHEMA_VERSION`:** `**1`** — constant emitted by codegen into `crates/nexus-contracts/src/generated/mod.rs` and `packages/nexus-contracts/src/generated/index.ts`.
- Individual DTOs carry a per-type `schema_version` in schema and generated code; the **bundle envelope** and tooling align on the latest value above after `pnpm run codegen`.

### Package versions (current repo snapshot)

Declared versions in-tree (refresh after releases or workspace bumps):

| Deliverable                                                                                                                  | Version                    | Declared in                                       |
| ---------------------------------------------------------------------------------------------------------------------------- | -------------------------- | ------------------------------------------------- |
| Rust crates `nexus42`, `nexus42d`, `nexus-contracts`, `nexus-domain`, `nexus-sync`, `nexus-local-db`                         | **0.1.0**                  | Root `Cargo.toml` → `[workspace.package] version` |
| `nexus-contracts` on crates.io                                                                                               | **0.1.0** (with workspace) | Same; publish from `crates/nexus-contracts`       |
| `@42ch/nexus-contracts` (npm)                                                                                                | **0.3.0**                  | `packages/nexus-contracts/package.json`           |
| `nexus-codegen` (private tooling)                                                                                            | **0.1.0**                  | `tooling/codegen/package.json`                    |
| Root `nexus-monorepo` meta package                                                                                           | **0.1.0**                  | Root `package.json`                               |

**npm vs Rust crate SemVer:** The npm package may use a **different** semantic version than the Rust workspace while both implement the same `LATEST_SCHEMA_VERSION` on the wire. Treat `schema_version` as the cross-language lock.

### Policy (unchanged)

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI / daemon crate SemVer must reflect breaking wire changes when you version the binaries
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- Compatibility matrix maintained in internal runbook (not in this OSS repo)

## Constraints & Pitfalls

- **Do not treat `nexus42d` as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript

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
