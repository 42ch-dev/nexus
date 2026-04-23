# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> This file documents the `.agents/` directory structure for this repository.
> For project-level rules and code conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Concepts

| Symbol | Meaning | Path in this repo |
|--------|---------|-------------------|
| `{HARNESS_DIR}` | Root of all agent/engineering infrastructure | `.agents/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `{HARNESS_DIR}/plans/` = `.agents/plans/` |

## Upstream Harness Convention

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework — a skill-driven multi-agent harness. All harness conventions (residual findings lifecycle, Done archival profiles, `status.json` structure, `knowledge/` management, QC/QA report naming, pre-merge checklist, file naming, severity levels, etc.) are defined as `mstar-*` skills in the upstream `skills/` directory. This repo follows upstream defaults unless noted in **Project-Specific Deviations** below.

| Skill | Scope |
|-------|-------|
| `mstar-harness-core` | State machine, Spec-Driven gates, task routing, branch/worktree invariants |
| `mstar-plan-conventions` | `status.json` SSOT, residual lifecycle, `knowledge/`, Done archival, pre-merge checklist |
| `mstar-review-qc` | QC triple-review workflow, report template, gate rules |
| `mstar-roles` | Role prompt bus (role bodies in `references/`) |
| `mstar-coding-behavior` | Cross-role coding behavior baseline |
| `mstar-superpowers-align` | Alignment and conflict handling with Superpowers |

## Directory Structure

```
{HARNESS_DIR}/                          # .agents/
├── AGENTS.md                           # This file
├── .gitignore                          # Ignore local config
├── local-paths.json.example            # Template for external spec paths (gitignored when filled)
├── knowledge/                          # Dev-process knowledge artifacts (indexed in knowledge/README.md)
├── archived/                           # Closed/archived artifacts
│   ├── plans/                          #   Done plan-row snapshots (cold storage)
│   ├── plans-done.json                 #   Minimal index of all Done plans
│   ├── residuals/                      #   Closed residual findings per plan (structured JSON)
│   └── knowledge/                      #   Superseded knowledge snapshots
├── status.json                         # SSOT: active plan rows + open residual_findings + root metadata
├── notes.json                          # Cross-plan program timeline
└── plans/                              # {PLAN_DIR}
    ├── <plan-id>-<plan-name>.md        #   Plan documents
    ├── reports/<plan-id>/              #   QC/QA reports per plan
    └── residuals/<plan-id>/            #   Open residual detail (prose, complements status.json)
```

## Project-Specific Deviations

### Plan compaction profile

Uses the **unified compression rule** from upstream `plan-convention.md`: `status.json.plans[]` keeps only non-`Done` rows. Done rows are moved to `archived/plans/` and indexed in `archived/plans-done.json`.

### Pre-merge additions

In addition to the upstream pre-merge checklist, this repo requires:

- **Wire contracts / schemas** (when `schemas/` or publish version changes): run `pnpm run codegen` and commit regenerated output. Bump package versions per release policy.
- **Roadmap in `nexus-platform`** (when a plan is `Done`): edit `roadmap.md` at the path configured as `specs_root.roadmap` in `.agents/local-paths.json` to reflect completion.

### Residual detail prose (`plans/residuals/`)

Open residuals that need more than the structured fields in `status.json` can have prose detail documents under `plans/residuals/<plan-id>/`. These complement (not replace) `metadata.residual_findings` entries.

- **What goes here**: pure deferral records, legacy issue explanations, "why this was deferred + current code state + future implementer pointers". Named `<td-or-r-id>-<short-label>.md`.
- **What stays in `knowledge/`**: design documents that incidentally mention residuals (e.g., crate-selection doc noting DoS guard TODOs).
- **Lifecycle**: when the residual is closed, the prose doc is archived alongside the structured JSON to `archived/residuals/<plan-id>.json`.

### External design specs

Design specs live in the private `nexus-platform` repo. Configure paths via `.agents/local-paths.json` (copy from `.agents/local-paths.json.example`). The file is gitignored.

### Verification commands

```bash
# Verify tech_debt_summary matches residual_findings
jq '.metadata.tech_debt_summary.total_open == (.metadata.residual_findings | to_entries | map(.value | length) | add)' .agents/status.json

# Verify no Done rows leak
jq '[.plans[] | select(.status == "Done")] | length' .agents/status.json

# View open residuals by plan
jq '.metadata.residual_findings | to_entries[] | {plan: .key, count: (.value | length)}' .agents/status.json
```
