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
docs/                   # User docs (installation, sync, troubleshooting)
.github/workflows/      # CI: schema validation, Rust fmt/clippy/test, npm publish
```

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
└── knowledge/                    # Plan-related knowledge
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

**Authoritative residual rows** live in `status.json` under **`metadata.residual_findings[<plan-id>]`** (same key as `plans[].id`). Optional **`plans[].metadata.residual_summary`** is a one-line human hint only; it does not replace the structured list below.

```json
{
  "metadata": {
    "residual_findings": {
      "<plan-id>": [
        {
          "id": "R1",
          "title": "Finding title",
          "severity": "critical|high|medium|low|warning",
          "source": "QC-#1, QC-#3",
          "scope": "Affected file or component",
          "decision": "defer|accept|risk-accepted",
          "owner": "@fullstack-dev",
          "target": "When to address (e.g., 'Before next plan')",
          "tracking": "Issue URL or null"
        }
      ]
    }
  }
}
```

**Root `metadata.notes`** (optional): program-level timeline, usually an array of `{ "updated_at", "message" }`. **Per-plan `plans[].notes`**: short status string for that plan only.

### Severity Levels

- **Critical**: Must fix before merge (blocking)
- **High**: Should fix before merge or immediately after
- **Medium**: Should address in near-term (next 1-2 plans)
- **Low**: Accept as-is or optional improvement
- **Warning**: Non-blocking, informational

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

# View residual findings (when present)
jq '.metadata.residual_findings["2025-04-05-domain-models"]' .agents/plans/status.json

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