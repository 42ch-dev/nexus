# Nexus AGENTS.md

This file provides development guidance for agents working in the **nexus** open-source repository.

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
- CLI executable: **`nexus42`**
- Daemon: **`nexus42d`**
- npm scope: **`@42ch`**
- Contracts package: **`@42ch/nexus-contracts`**

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
.agents/plans/
  knowledge/            # Dev-process knowledge (architecture reviews, spec revisions, design decisions)
  reports/              # QC/QA review reports
.github/workflows/      # CI: schema validation, Rust fmt/clippy/test, npm publish
```

## Content Boundary: `docs/` vs `.agents/plans/knowledge/`

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

### `.agents/plans/knowledge/` — Dev-Process Knowledge

Development process artifacts generated during planning and review:

- Architecture review reports and spec revision outputs
- Design decision records and rationale
- Gap analyses, constraint inventories, compliance checklists
- Any document that serves as **context for implementing a plan**

These documents are valuable for agent handoff and cross-session continuity, but are not intended for external consumers.

**Index**: All knowledge documents are catalogued in [`.agents/plans/knowledge/README.md`](.agents/plans/knowledge/README.md) with source plan, description, and status.

**Maintenance rules**:

1. **Adding**: Name new documents `<topic>-<qualifier>-v<N>.md`. Add an entry to the README index table. Record the path in `status.json` under the plan's `metadata` (e.g. `wave_0_spec`).
2. **Reading**: Before implementing a plan, agents MUST read any knowledge documents referenced in that plan's `status.json` metadata (e.g. `wave_0_spec`, `spec_refs`). These are authoritative design input — do not silently diverge.
3. **Updating**: If an architecture review or spec revision modifies a knowledge document, update the README index status. If the document is fully consumed by implementation, mark it `Superseded` but do not delete — design rationale should be preserved.
4. **Reachability**: All knowledge documents MUST follow the reachability rules in §"Documentation & plans" below — no references to files outside this repository.

## Documentation & plans (mandatory reachability)

**Mandatory** for any in-repo documentation (for example `docs/`, `README`, design notes) and agent plans (for example `.agents/plans/`):

- **Do not** reference paths to files that are excluded by `.gitignore` or otherwise not present in a fresh clone. Readers who only `git clone` this repository must be able to open every cited path.
- **Do not** reference files **outside** this repository root (for example `~/.config/...`, absolute home paths, or arbitrary sibling directories). If external context is required, inline the essential content in the repo or link to a **stable, public** URL.

Violations break onboarding and agent handoff for anyone without your local machine layout.

## Plans & Reports Structure

### Directory Organization

```
.agents/plans/
├── <plan-id>-<plan-name>.md     # Main plan files
├── status.json                   # SSOT: plan rows + file-level metadata (residual_findings, program notes)
├── reports/                      # Supplementary reports
│   ├── README.md
│   └── <plan-id>/               # Reports for each plan
│       ├── <plan-id>-review.md           # Architecture review
│       ├── <plan-id>-qc<#>.md            # QC reports (parallel review)
│       └── <plan-id>-qc-consolidated.md  # Consolidated QC decision
├── archived/                     # Archived plans
│   └── residuals/               # Closed residual findings (per-plan JSON archives)
└── knowledge/                    # Dev-process knowledge (indexed in knowledge/README.md)
```

### File Naming Conventions

**Main Plan Files**:

- Format: `<plan-id>-<plan-name>.md`
- Example: `2025-04-05-domain-models.md`

**Report Files**:

- Architecture review: `<plan-id>-review.md`
- QC individual reports: `<plan-id>-qc1.md`, `<plan-id>-qc2.md`, `<plan-id>-qc3.md`
- QC consolidated decision: `<plan-id>-qc-consolidated.md`

### Residual Findings Tracking

Full conventions for residual findings (i.e. tech debt) are defined in the global `plan-convention.md`: entry structure, lifecycle states (open/resolved/waived/superseded/duplicate), archival to `archived/residuals/<plan-id>.json`, `tech_debt_summary` rollup view, etc.

Project-level notes:

- **Entry location**: `status.json` → `metadata.residual_findings[<plan-id>]` (open items only).
- **Close & archive**: set `lifecycle` to `resolved`/`waived`/`superseded`/`duplicate` → add `closed_at` + `closure_note` → move to `archived/residuals/<plan-id>.json` → remove from `residual_findings`.
- **Severity**: `critical`/`high` must be addressed before merge; `medium`/`low`/`warning` can be tracked as residuals.
- **`residual_summary`** (optional, in `plans[].metadata`): one-line human-readable summary of open items only.

**Root `metadata.notes`** (optional): program-level timeline, usually an array of `{ "updated_at", "message" }`. **Per-plan `plans[].notes`**: short status string for that plan only.

### Plan Lifecycle

1. **Todo**: Plan created, not started
2. **InProgress**: Implementation underway
3. **InReview**: QC review in progress (reports in `reports/<plan-id>/`)
4. **Done**: Completed, merged to main

### Plan items in `status.json`

Each `plans[]` entry keeps **canonical top-level keys**: `id`, `title`, `file`, `status`, `owner`, `agents`, `progress`, `tags`, `created_at`, `updated_at`, `done_at`, `notes`, and optionally **`metadata`** (object; omit or use `{}` if nothing extra). **Do not** duplicate the plan id for residuals lookup; **`plans[].id`** is the only key into `metadata.residual_findings`.

**`plans[].metadata`** (optional) holds process context, for example: `branch_policy`, `phase`, `priority`, `description` **or** `scope` (use one as the long-form scope field), `working_branch`, `merge_target`, `gates`, `primary_spec` / `spec_refs` (this repo may use a spec path field such as `wave_0_spec` where plans already do), `blocked_since`, `blocked_reason`, `blocked_by_plan_id`, `dependency`, `next_action`, `qc_status`, `tests`, `commits`, `residual_summary`. Formal QC rows remain only under **file-level** `metadata.residual_findings[<plan-id>]`.

### Accessing Plan Information

```bash
# View plan status (plans is an array; filter by id)
jq '.plans[] | select(.id == "2025-04-05-domain-models")' .agents/plans/status.json

# View plan-local metadata
jq '.plans[] | select(.id == "2025-04-05-domain-models") | .metadata' .agents/plans/status.json

# Program-level timeline (if present)
jq '.metadata.notes' .agents/plans/status.json

# View detailed QC report
cat .agents/plans/reports/2025-04-05-domain-models/2025-04-05-domain-models-qc-consolidated.md
```

## Development Workflow

**Schema/codegen flow:**

- JSON Schema (`schemas/`) → single codegen pass → Rust (`crates/nexus-contracts`) + TypeScript (`packages/nexus-contracts`)
- Both packages must be published and version-locked with `schema_version`
- CI validates schemas before generating code

**Rust development:**

- Use official ACP Rust SDK (not custom protocol implementations)
- CLI and daemon share generated contract types from `crates/nexus-contracts`
- Daemon is `nexus42d`, started via `nexus42 daemon start`
- **Formatting:** use `cargo +nightly fmt --all` before commit. The workspace `.rustfmt.toml` ignores `crates/nexus-contracts/src/generated/` (stable `cargo fmt` cannot apply `ignore`, and formatting generated Rust would desync CI `verify-codegen` from `pnpm run codegen`). Install once: `rustup toolchain install nightly --component rustfmt`

**TypeScript contract package:**

- `nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock
- **No handwritten second DTO source** in platform — all wire types come from this repo's schemas

## Dev/Test Infrastructure

**Required containers:**

- Postgres + pgvector (`pgvector/pgvector:pg16`)
- Neo4j (`neo4j:5`)
- Redis (`redis/redis-stack-server:latest`)

**API keys (external, not in this repo's code but needed for integration):**

- LLM inference API (platform-side; CLI uses user's local agent)
- OAuth/IdP credentials (for web login + CLI device flow)

**CLI-only note:** ACP Registry is public (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`); CLI pulls from it, no API key required.

## Versioning & Compatibility

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI SemVer must reflect breaking wire changes
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- Compatibility matrix maintained in internal runbook (not in this OSS repo)

## Constraints & Pitfalls

- **Do not treat `nexus42d` as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript