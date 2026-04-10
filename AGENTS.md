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
.agents/plans/
  archived/             # residuals/, plans/ (snapshots), knowledge/ (superseded knowledge docs — see knowledge/README.md)
  knowledge/            # Dev-process knowledge (architecture reviews, spec revisions, design decisions)
  reports/              # QC/QA review reports
.github/workflows/      # CI: see `.github/workflows/ci.yml` (schemas, codegen diff, fmt, clippy, TS typecheck)
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

**Index**: All knowledge documents are catalogued in `[.agents/plans/knowledge/README.md](.agents/plans/knowledge/README.md)` with source plan, description, and status.

**Maintenance rules**:

1. **Adding**: Name new documents `<topic>-<qualifier>-v<N>.md`. Add an entry to the README index table. Record the path in `status.json` under the plan's `metadata` (e.g. `wave_0_spec`).
2. **Reading**: Before implementing a plan, agents MUST read any knowledge documents referenced in that plan's `status.json` metadata (e.g. `wave_0_spec`, `spec_refs`). These are authoritative design input — do not silently diverge.
3. **Updating**: If an architecture review or spec revision modifies a knowledge document, update the README index status. If the document is fully consumed by implementation, mark it `Superseded` but do not delete — design rationale should be preserved.
4. **Reachability**: All knowledge documents MUST follow the reachability rules in §"Documentation & plans" below — no references to files outside this repository.

## External Design Specs

Nexus 是**开源仓库**，但设计规格位于**私有 `nexus-platform` 仓库**中。

### 设置（一次性）

```bash
cp .agents/local-paths.json.example .agents/local-paths.json
# 编辑 local-paths.json，填入 nexus_platform 实际路径
```

`local-paths.json` 已加入 `.gitignore`，不会提交到 git。

### 读取规格

使用 `.agents/local-paths.json`（从 `.agents/local-paths.json.example` 复制并填写）解析 `specs_root`：

- **Roadmap**（`roadmap.md`）: `{specs_root.roadmap}` — 由示例中的 `${nexus_platform}` 等占位符在本地展开后的路径；计划在 `status.json` 中标记为 `**Done`** 时需同步更新此文件（见下文 Pre-merge checklist）。
- Architecture: `{specs_root.v1-spec}/architecture/v1.md`
- Domain Model: `{specs_root.v1-spec}/domain/data-model-v1.md`

## Documentation & plans (mandatory reachability)

**Mandatory** for any in-repo documentation (for example `docs/`, `README`, design notes) and agent plans (for example `.agents/plans/`):

- **Do not** reference paths to files that are excluded by `.gitignore` or otherwise not present in a fresh clone. Readers who only `git clone` this repository must be able to open every cited path.
- **Do not** reference files **outside** this repository root (for example `~/.config/...`, absolute home paths, or arbitrary sibling directories). If external context is required, inline the essential content in the repo or link to a **stable, public** URL.

Violations break onboarding and agent handoff for anyone without your local machine layout.

### No local privacy in committed text

This repository is **public** and plan reports are often **tracked**. Anything you commit must not leak **machine-specific** or **personal** layout:

- **Do not** paste absolute paths that expose a home directory or OS username, for example macOS `/Users/<you>/...`, Linux `/home/<you>/...`, or Windows `C:\\Users\\<you>\\...`, even if they point “into” this clone. Those strings identify individuals and local folder choices.
- **Do not** treat “review cwd”, worktree location, or editor workspace paths as verbatim copy-paste into QC/QA reports, `status.json` prose, or knowledge notes **unless** you normalize them first.

**Use instead** (pick one style and stay consistent within a document):

- **Relative paths from the repository root** (preferred for real files in this repo), e.g. `.agents/plans/status.json`, `crates/nexus42/src/...`.
- **Neutral placeholders** when the exact mount point does not matter, e.g. `<repository-root>`, `<repository-root>/.worktrees/<branch-name>/` for git worktrees under this repo’s `.worktrees/` convention.
- `**{PLAN_DIR}`** / `.agents/plans/` when referring to the plan tree, per this file’s naming above.

**Also avoid** in committed artifacts: internal hostnames, private IP addresses, raw secrets or API keys, and full tool logs that embed your local paths (sanitize or excerpt). Redact before commit if a report must quote command output.

## Plans & Reports Structure

### Harness alignment (authoritative mirror)

Plan directory discovery, `status.json` / residual lifecycle, optional `notes.json` and cold snapshots, and merge SSOT expectations follow **Harness Engineering** conventions. This repo aligns with the published OpenCode team config **[btspoony/harness-opencode-team](https://github.com/btspoony/harness-opencode-team)**; the normative document is `[docs/agents/plan-convention.md](https://github.com/btspoony/harness-opencode-team/blob/main/docs/agents/plan-convention.md)` in that repository (same text as OpenCode global `docs/agents/plan-convention.md` when installed). The sections below are **this repo’s** binding summary; if anything conflicts, reconcile with that upstream document and update this file.

### `{PLAN_DIR}` discovery

Resolve the plan root in order (first match wins); call the result `**{PLAN_DIR}`**:

1. `.agents/plans/`
2. `.plans/`
3. `plans/`

If none exist, the project is treated as **not using** an on-disk plan tree; @project-manager may still run gates and track progress via conversation and completion reports. **This repository** ships with `**.agents/plans/`** as `{PLAN_DIR}`.

**Git:** Prefer **tracking** `{PLAN_DIR}` so clone-based handoff stays reachable; only ignore the whole tree for purely local/private setups, and then do not cite ignored paths as the sole authority in committed docs (same as reachability rules above).

**Superpowers `writing-plans`:** New plan files MUST land under the resolved `**{PLAN_DIR}`** (e.g. `.agents/plans/<plan-id>-<name>.md`), **not** `docs/superpowers/plans/` in this repo.

### Directory Organization

Paths below are under `**{PLAN_DIR}`** (here, usually `.agents/plans/`):

```
{PLAN_DIR}/
├── <plan-id>-<plan-name>.md     # Main plan files
├── status.json                   # SSOT: plan rows + open residual_findings (+ optional root metadata)
├── notes.json                    # Optional: program timeline (prefer over growing root metadata.notes)
├── reports/                      # Supplementary reports
│   ├── README.md
│   └── <plan-id>/               # Reports for each plan
│       ├── <plan-id>-review.md           # Architecture review
│       ├── <plan-id>-qc<#>.md            # QC reports (parallel review)
│       └── <plan-id>-qc-consolidated.md  # Consolidated QC decision
├── archived/
│   ├── plans/                    # Optional: full plan-row snapshots at Done (see § below)
│   └── residuals/                # Closed residual findings (per-plan JSON archives)
└── knowledge/                    # Dev-process knowledge (indexed in knowledge/README.md)
```

Initialize or extend `{PLAN_DIR}` per upstream **Initialize Plan directory** (see Harness `plan-convention.md`): `status.json`, optional `notes.json`, `reports/README.md`, optional `knowledge/README.md`, optional `archived/residuals/`.

### File Naming Conventions

**Main Plan Files**:

- Format: `<plan-id>-<plan-name>.md`
- Example: `2025-04-05-domain-models.md`

**Report Files**:

- Architecture review: `<plan-id>-review.md`
- QC individual reports: `<plan-id>-qc1.md`, `<plan-id>-qc2.md`, `<plan-id>-qc3.md`
- QC consolidated decision: `<plan-id>-qc-consolidated.md`

### Residual Findings Tracking

Full conventions (lifecycle, archive file shape, `tech_debt_summary`, QC severity mapping) are defined in **[plan-convention.md](https://github.com/btspoony/harness-opencode-team/blob/main/docs/agents/plan-convention.md)** (Harness mirror). Summary for this repo:

- **Entry location**: `status.json` → `metadata.residual_findings[<plan-id>]` (**open items only**; keys must match `plans[].id`).
- **Empty keys**: When a plan has **no** open residuals, **remove** that `plan-id` key from `metadata.residual_findings` entirely (do not keep `"plan-id": []`).
- **Close & archive**: set `lifecycle` to `resolved`/`waived`/`superseded`/`duplicate` → add `closed_at` + `closure_note` (and recommended `closure_evidence`) → append to `{PLAN_DIR}/archived/residuals/<plan-id>.json` → remove the row from the open list in `status.json`.
- `**severity` (JSON SSOT)**: only `critical`, `high`, `medium`, `low`, `**nit`** (lowercase). `**nit**` is lighter than `low` (style/nits). **Do not** write `warning` on new rows; legacy `"warning"` reads as `**low`**. Merge gate: `**critical` / `high`** per team policy and QC baseline; other levels may be tracked as residuals.
- `**residual_summary**` (optional, in `plans[].metadata`): one-line summary of **open** items only for that plan.

**Program timeline:** Prefer `**{PLAN_DIR}/notes.json`** for cross-plan milestones (see upstream schema). Root `**metadata.notes`** is **legacy** if present; migrate out when practical. **Per-plan `plans[].notes`**: short status string for that plan only.

### Plan Lifecycle

1. **Todo**: Plan created, not started
2. **InProgress**: Implementation underway
3. **InReview**: QC review in progress (reports in `reports/<plan-id>/`)
4. **Blocked**: Waiting on dependency, decision, or another plan (use `metadata.blocked_*` when applicable)
5. **Done**: Completed, merged to main

**Multi-batch plans:** Default QC triple-review **once** after the whole plan’s dev work completes (not necessarily per batch); see upstream `plan-convention.md` and `harness-loop.md` in the same Harness repo.

### Pre-merge checklist (mandatory)

**Before merging any feature branch or opening a PR that closes plan work**, update `**{PLAN_DIR}/status.json`** (this repo: `.agents/plans/status.json`) so it stays the single source of truth. This mirrors the private `nexus-platform` pre-merge discipline but uses **this repo’s** metadata shape (see root `metadata` in `status.json`: `versioning`, `tech_debt_summary`, `notes`, `residual_findings`).

#### Required updates

1. `**plans[].status`**, `**plans[].notes`**, `**plans[].updated_at**` / `**done_at**` (when applicable): reflect the real branch and merge outcome.
2. `**plans[].metadata.gates**` (or equivalent): QC / QA / CI parity — e.g. `qc_status`, `qa_status`, `tests`, `clippy`, `validation` — so reviewers see gate state without opening reports.
3. `**plans[].metadata.residual_summary**`: one-line summary of **open** residuals for that plan only (formal rows stay under `metadata.residual_findings`).
4. `**metadata.residual_findings[<plan-id>]`**: add or update structured findings from QC; **close and archive** per upstream convention (`{PLAN_DIR}/archived/residuals/<plan-id>.json`) when resolved. Keys use the full plan id (e.g. `2025-04-05-domain-models`), same as `plans[].id`. Remove **empty** `plan-id` keys from the map.
5. `**metadata.tech_debt_summary`**: refresh `updated_at`, `total_open`, `by_severity`, `by_plan`, and `**by_target`** when the open residual set changes; keep `**cross_cutting**` in sync if you add or resolve program-level debt items (e.g. `DEBT-X*`).
6. **Program timeline**: append a milestone to `**notes.json`** when the team uses it; otherwise `**metadata.notes`** in `status.json` for significant merges or residual cleanups (legacy; prefer `notes.json` for new program-level logs).
7. **Wire contracts / schemas (when `schemas/` or publish version changes)** — nexus-specific, not `contracts_schema`:
  - Run `**pnpm run codegen`** and commit `**packages/nexus-contracts/src/generated/`** and `**crates/nexus-contracts/src/generated/`** (CI `verify-codegen` enforces this).
  - Bump `**schema_version`** and package versions (`packages/nexus-contracts`, `crates/nexus-contracts`) per release policy; note downstream impact (`nexus-platform` consumes `@42ch/nexus-contracts`).
8. **Roadmap in `nexus-platform` (when a plan is `Done`)** — same discipline as on the private platform repo: in the **same change window** as updating `{PLAN_DIR}/status.json` for a completed plan, edit `**roadmap.md`** at the path configured as `**specs_root.roadmap`** in your local `**.agents/local-paths.json**` (see §External Design Specs and the example file). Reflect completion (e.g. align with `done_at` / merge reality), delivered scope, and any reprioritization so the roadmap matches `**plans[].status**` in this repo. The roadmap file is **not** in the nexus OSS tree; commit that edit in `**nexus-platform`**. Do not paste machine-specific absolute paths into tracked nexus OSS artifacts (QC notes, `status.json` prose, etc.); resolving `specs_root.roadmap` locally is sufficient for the edit.

#### Verification commands

```bash
# Open residuals by plan (keys are full plan ids)
jq '.metadata.residual_findings | to_entries[] | {plan: .key, count: (.value | length)}' .agents/plans/status.json

# Tech-debt rollup and branch-prefix conventions
jq '.metadata.tech_debt_summary, .metadata.versioning' .agents/plans/status.json

# Program timeline (legacy in status.json, or prefer notes.json when adopted)
jq '.metadata.notes' .agents/plans/status.json
# jq '.entries' .agents/plans/notes.json   # when notes.json exists

# Optional: sum of residual_findings entries (compare mentally with tech_debt_summary.total_open when both track the same scope)
jq '[.metadata.residual_findings | to_entries[] | .value | length] | add' .agents/plans/status.json
```

#### Common mistakes

- Marking a plan `**Done**` in `status.json` without updating `**roadmap.md**` at `**specs_root.roadmap**` in `**nexus-platform**` (roadmap drifts from actual plan completion).
- Leaving `**tech_debt_summary**` stale after QC triage (counts and `updated_at` disagree with `residual_findings`).
- **Schema edits without regenerated** `*/generated/` trees — CI fails on drift.
- **Missing timeline** (`notes.json` or, if legacy, `metadata.notes`) for a merge or bulk residual archival that future agents need for context.
- Duplicating finding detail only in `**plans[].notes`** instead of `**metadata.residual_findings`** (SSOT for open items).
- **Publishing local paths or other machine-specific identifiers** in tracked QC/QA reports or plan notes (for example verbatim `review_cwd` under `/Users/...` or `C:\Users\...`). Replace with repo-relative paths or placeholders per §"No local privacy in committed text" above before commit.

**Rule:** If `status.json` does not reflect reality, treat the branch as **not merge-ready** until it is corrected.

### Plan items in `status.json`

Each `plans[]` entry keeps **canonical top-level keys**: `id`, `title`, `file`, `status`, `owner`, `agents`, `progress`, `tags`, `created_at`, `updated_at`, `done_at`, `notes`, and optionally `**metadata`** (object; omit or use `{}` if nothing extra). **Do not** duplicate the plan id for residuals lookup; `**plans[].id`** is the only key into `metadata.residual_findings`.

`**plans[].metadata`** (optional) holds process context, for example: `branch_policy`, `phase`, `priority`, `description` **or** `scope` (use one as the long-form scope field), `working_branch`, `merge_target`, `gates`, `primary_spec` / `spec_refs` (this repo may use a spec path field such as `wave_0_spec` where plans already do), `blocked_since`, `blocked_reason`, `blocked_by_plan_id`, `dependency`, `next_action`, `qc_status`, `tests`, `commits`, `residual_summary`, and `**archived_record`** (relative path under `{PLAN_DIR}` to a cold snapshot when using optional compaction below). Formal QC rows remain only under **file-level** `metadata.residual_findings[<plan-id>]`.

### Plan row archival and `status.json` size (optional compaction)

**Why:** Many `Done` rows carry large `metadata` (gates, QC strings, tests, commits, long scope text), so `{PLAN_DIR}/status.json` grows without bound. Open `**metadata.residual_findings`** should stay bounded if closed items move to `archived/residuals/` per the rules above.

**SSOT:** Root `status.json` stays authoritative for **current execution** (non-terminal plans, root `metadata`, **open** residuals). The following is an **opt-in** way to keep the hot file small while preserving history in-repo (reachability: every path must exist in a fresh clone).

**Cold storage (plan row snapshot at `Done`):**

- **Path:** `{PLAN_DIR}/archived/plans/<plan-id>.json` (here, `.agents/plans/archived/plans/<plan-id>.json`)
- **Content:** Full `plans[]` element as it existed when the plan was marked `Done` (including rich `metadata`), for audit and agent handoff.
- **Relationship to residuals:** `archived/residuals/<plan-id>.json` stores **closed finding rows**; `archived/plans/<plan-id>.json` stores the **plan row snapshot**. Do not treat the plan snapshot as a second copy of **open** `residual_findings`.

**Ultra-compressed `Done` row in `status.json` (after the team adopts this):**

**Minimal field set** (per plan-convention.md §430-436):

- **Required fields** (machine navigation): `id`, `status` (`"Done"`), `file`, `metadata`
- **Optional fields** (human-friendly): `title`, `done_at`

**Removed fields** (available in `archived/plans/<plan-id>.json`):

- `owner`, `agents`, `progress`, `tags`, `created_at`, `updated_at`, `notes`
- Bulky metadata fields: `gates`, `qc_status`, `tests`, `commits`, `description`, `scope`, etc.

**Metadata content**:

- `**archived_record`**: path relative to `{PLAN_DIR}`, e.g. `archived/plans/<plan-id>.json`
- Optional one-line `**residual_summary`** only while that plan id still has **open** rows under `metadata.residual_findings`.

**Example ultra-compressed Done plan**:

```json
{
  "id": "2025-04-05-domain-models",
  "status": "Done",
  "file": ".agents/plans/2025-04-05-domain-models.md",
  "metadata": {
    "archived_record": "archived/plans/2025-04-05-domain-models.json",
    "residual_summary": "1 open residuals"
  },
  "title": "Domain Models Implementation",
  "done_at": "2026-04-06"
}
```

**Important**: Once the snapshot is written, the hot row **MUST NOT** carry `gates`, `qc_status`, `tests`, `commits`, long `description`/`scope`, or any other bulky fields. All complete information is available via `metadata.archived_record` pointer.

**When to write the snapshot:** Same change set as marking the plan `Done` and completing the pre-merge `status.json` updates (or immediately after merge), once compaction is adopted.

**Optional index:** `{PLAN_DIR}/archived/plans/_index.json` — map plan id → relative path for tools that do not glob.

**Optional rolling retention:** To shrink `status.json` further, retain only a **window** of `Done` slim rows in `plans[]` and list older ids only in `_index.json` / snapshot files. If you do this, document it here and update any scripts that assume every historical id appears in `plans[]`.

**Adoption:** Full `Done` rows without snapshots remain valid until the team opts in. Before relying on compaction, align with automation or docs that expect full `metadata` on every `Done` plan.

### Accessing Plan Information

```bash
# View plan status (plans is an array; filter by id)
jq '.plans[] | select(.id == "2025-04-05-domain-models")' .agents/plans/status.json

# View plan-local metadata
jq '.plans[] | select(.id == "2025-04-05-domain-models") | .metadata' .agents/plans/status.json

# Residual findings for one plan (SSOT key matches plans[].id)
jq '.metadata.residual_findings["2025-04-05-domain-models"]' .agents/plans/status.json

# Program-level timeline (if present)
jq '.metadata.notes' .agents/plans/status.json

# Open tech-debt rollup (if present)
jq '.metadata.tech_debt_summary' .agents/plans/status.json

# View detailed QC report
cat .agents/plans/reports/2025-04-05-domain-models/2025-04-05-domain-models-qc-consolidated.md

# If optional compaction is in use: full archived plan row
cat .agents/plans/archived/plans/2025-04-05-domain-models.json
```

## Development Workflow

**Git worktrees:**

- Put every additional `git worktree` checkout under **this repository root** at `.worktrees/<name>/` only. Do not add worktrees in arbitrary sibling directories outside the clone.
- The `.worktrees/` directory is listed in `.gitignore`; it keeps parallel branches in one predictable place for tooling and handoff.
- Example: `git worktree add .worktrees/my-branch -b my-branch`

**Schema/codegen flow:**

- JSON Schema (`schemas/`) → single codegen pass → Rust (`crates/nexus-contracts`) + TypeScript (`packages/nexus-contracts`)
- Both packages must be published and version-locked with `schema_version`
- CI validates schemas before generating code

**Schema URI placeholder (production domain TBD):** Committed schema files use `**https://nexus42.invalid`** in `$id` / `$ref` paths (RFC 6761 reserved name; valid HTTPS URIs for validators and tooling). In prose and external-facing docs, write the same logical origin as `**{NEXUS42_BASE_URL}**` (HTTPS origin only, no trailing slash), e.g. `{NEXUS42_BASE_URL}/schemas/...`. Do **not** embed `{NEXUS42_BASE_URL}` inside JSON `$id` / `$ref` strings — those must remain real URIs. See `schemas/meta/README.md` and `docs/CODEGEN.md`.

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

**TypeScript contract package:**

- `nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock
- **No handwritten second DTO source** in platform — all wire types come from this repo's schemas
- **SemVer:** bump `packages/nexus-contracts` npm version together with `schema_version` / `crates/nexus-contracts` per release policy. Breaking wire shapes (TypeScript unions or field renames from schema) require a **major** npm bump and a coordinated platform upgrade so consumers do not mix mismatched contract versions.

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

### Wire `schema_version` (generated SSOT)

- `**LATEST_SCHEMA_VERSION`:** `**1`** — constant emitted by codegen into `crates/nexus-contracts/src/generated/mod.rs` and `packages/nexus-contracts/src/generated/index.ts`.
- Individual DTOs carry a per-type `schema_version` in schema and generated code; the **bundle envelope** and tooling align on the latest value above after `pnpm run codegen`.

### Package versions (current repo snapshot)

These are the **declared versions in-tree** (refresh after releases or workspace bumps):


| Deliverable                                                                                                                  | Version                    | Declared in                                       |
| ---------------------------------------------------------------------------------------------------------------------------- | -------------------------- | ------------------------------------------------- |
| Rust crates `**nexus42`**, `**nexus42d`**, `**nexus-contracts**`, `**nexus-domain**`, `**nexus-sync**`, `**nexus-local-db**` | **0.1.0**                  | Root `Cargo.toml` → `[workspace.package] version` |
| `**nexus-contracts`** on crates.io                                                                                           | **0.1.0** (with workspace) | Same; publish from `crates/nexus-contracts`       |
| `**@42ch/nexus-contracts`** (npm)                                                                                            | **0.2.0**                  | `packages/nexus-contracts/package.json`           |
| `**nexus-codegen`** (private tooling)                                                                                        | **0.1.0**                  | `tooling/codegen/package.json`                    |
| Root `**nexus-monorepo`** meta package                                                                                       | **0.1.0**                  | Root `package.json`                               |


**npm vs Rust crate SemVer:** The npm package may use a **different** semantic version than the Rust workspace (e.g. **0.2.0** vs **0.1.0**) while both implement the same `**LATEST_SCHEMA_VERSION`** on the wire. Treat `**schema_version`** / schema compatibility as the cross-language lock; align npm major bumps and Rust breaking releases when wire shapes change.

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

