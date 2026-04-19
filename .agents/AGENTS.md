# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> This file documents the `.agents/` directory structure for this repository.
> For project-level rules and code conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Concepts

| Symbol | Meaning | Path in this repo |
|--------|---------|-------------------|
| `{HARNESS_DIR}` | Root of all agent/engineering infrastructure | `.agents/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `{HARNESS_DIR}/plans/` = `.agents/plans/` |

## Upstream Harness Convention

All harness conventions (residual findings lifecycle, Done archival profiles, `status.json` structure, `knowledge/` management, QC/QA report naming, pre-merge checklist, etc.) are defined in the upstream Harness Engineering spec. This repo follows upstream defaults unless noted in **Project-Specific Deviations** below.

- **Plan management**: [`plan-convention.md`](https://github.com/btspoony/harness-opencode-team/blob/main/docs/agents/plan-convention.md)
- **Task lifecycle & QC gates**: [`harness-loop.md`](https://github.com/btspoony/harness-opencode-team/blob/main/docs/agents/harness-loop.md)
- **QC review checklist**: [`review-harness.md`](https://github.com/btspoony/harness-opencode-team/blob/main/docs/agents/review-harness.md)

## Directory Structure

```
{HARNESS_DIR}/                          # .agents/
├── AGENTS.md                           # This file — harness directory documentation
├── .gitignore                          # Ignore local config and worktrees
├── local-paths.json.example            # Template for external spec paths (gitignored when filled)
├── knowledge/                          # Dev-process knowledge artifacts
│   ├── README.md                       #   Index of knowledge docs
│   └── <topic>-<qualifier>-v<N>.md     #   Architecture reviews, design decisions, gap analyses
├── archived/                           # Closed/archived artifacts
│   ├── plans/                          #   Done plan-row snapshots (cold storage)
│   ├── plans-done.json                 #   Minimal index of all Done plans
│   ├── residuals/                      #   Closed residual findings per plan
│   └── knowledge/                      #   Superseded knowledge snapshots
├── status.json                         # SSOT: active plan rows + open residual_findings + root metadata
├── notes.json                          # Cross-plan program timeline (preferred over status.json metadata.notes)
└── plans/                              # {PLAN_DIR} — plan documents and QC/QA reports
    ├── <plan-id>-<plan-name>.md        #   Plan documents
    └── reports/<plan-id>/              #   QC/QA reports per plan
        ├── <plan-id>-review.md         #     Architecture review
        ├── <plan-id>-qc<#>.md          #     QC individual reports
        └── <plan-id>-qc-consolidated.md #     Consolidated QC decision
```

## knowledge/ — Maintenance Rules

1. **Adding**: Name new documents `<topic>-<qualifier>-v<N>.md`. Add an entry to `knowledge/README.md` index table. Record the path in `status.json` under the plan's `metadata` (e.g. `wave_0_spec`).
2. **Reading**: Before implementing a plan, agents MUST read any knowledge documents referenced in that plan's `status.json` metadata. These are authoritative design input — do not silently diverge.
3. **Updating**: If an architecture review or spec revision modifies a knowledge document, update the README index status. If fully consumed by implementation, mark it `Superseded` but do not delete — design rationale should be preserved.
4. **Archiving**: When a knowledge document is superseded, `git mv` it to `{HARNESS_DIR}/archived/knowledge/` (preserves history); update the README index to point to the new location.

## archived/ — Archival Rules

- **plans-done.json**: Minimal index of all `Done` plans with fields `id`, `title`, `done_at`, `plan_file`, `archived_record`.
- **plans/<plan-id>.json**: Full `plans[]` row snapshot at `Done` for audit/handoff. Created in the same commit as marking a plan `Done`.
- **residuals/<plan-id>.json**: Closed residual findings. When a finding is resolved (lifecycle set to `resolved`/`waived`/`superseded`/`duplicate`), move it here and remove from open `residual_findings` in `status.json`.
- **knowledge/**: Superseded knowledge documents (moved here via `git mv` from `knowledge/`).
- **Unified compression rule (adopted)**: `status.json.plans[]` keeps only non-`Done` rows. Done rows are removed and stored as cold snapshots in `archived/plans/`.

## status.json — SSOT

`status.json` is the single source of truth for:
- **Active plan rows** (`plans[]`): Todo / InProgress / InReview / Blocked only
- **Open residual findings** (`metadata.residual_findings[<plan-id>]`): empty keys removed
- **Tech debt summary** (`metadata.tech_debt_summary`): refreshed when residual set changes
- **Program metadata** (`metadata`): versioning, notes, cross-cutting debt

**Rule**: If `status.json` does not reflect reality, the branch is not merge-ready.

## Plan Lifecycle

| Status | Meaning |
|--------|---------|
| `Todo` | Plan created, not started |
| `InProgress` | Implementation underway |
| `InReview` | QC review in progress |
| `Blocked` | Waiting on dependency or decision |
| `Done` | Completed, merged; row archived to `archived/plans/` |

## Project-Specific Deviations

### Plan compaction profile

This repository uses the **unified compression rule** from upstream `plan-convention.md`: `status.json.plans[]` keeps only active plans; Done rows are moved to `archived/plans/` and indexed in `archived/plans-done.json`.

### Pre-merge additions

In addition to the upstream pre-merge checklist, this repo requires:

- **Wire contracts / schemas** (when `schemas/` or publish version changes): run `pnpm run codegen` and commit regenerated `packages/nexus-contracts/src/generated/` and `crates/nexus-contracts/src/generated/`. Bump package versions per release policy.
- **Roadmap in `nexus-platform`** (when a plan is `Done`): edit `roadmap.md` at the path configured as `specs_root.roadmap` in `.agents/local-paths.json` to reflect completion.
- **Common mistakes**: stale `tech_debt_summary`, missing timeline entries, duplicated finding detail in `plans[].notes` instead of `metadata.residual_findings`, publishing machine-specific paths in tracked artifacts.

### External design specs

Design specs live in the private `nexus-platform` repo. Configure paths via `.agents/local-paths.json` (copy from `.agents/local-paths.json.example`). The file is gitignored.

## Verification Commands

```bash
# Verify tech_debt_summary matches residual_findings count
jq '.metadata.tech_debt_summary.total_open == (.metadata.residual_findings | to_entries | map(.value | length) | add)' .agents/status.json

# Verify no Done rows leak into status.json
jq '[.plans[] | select(.status == "Done")] | length' .agents/status.json

# View open residuals by plan
jq '.metadata.residual_findings | to_entries[] | {plan: .key, count: (.value | length)}' .agents/status.json

# View tech-debt rollup
jq '.metadata.tech_debt_summary' .agents/status.json

# View program timeline
jq '.entries' .agents/notes.json
```

## File Naming Conventions

- **Plan files**: `<plan-id>-<plan-name>.md` (e.g. `2025-04-05-domain-models.md`)
- **Architecture review**: `<plan-id>-review.md`
- **QC individual reports**: `<plan-id>-qc1.md`, `<plan-id>-qc2.md`, `<plan-id>-qc3.md`
- **QC consolidated**: `<plan-id>-qc-consolidated.md`
- **Knowledge docs**: `<topic>-<qualifier>-v<N>.md`

## Residual Findings — Severity (JSON SSOT)

Only these severity levels are valid: `critical`, `high`, `medium`, `low`, `nit`. Merge gate: `critical` / `high` per team policy.
