# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).
> Domain-specific AGENTS.md files: [`schemas/`](../schemas/AGENTS.md) · [`tooling/`](../tooling/AGENTS.md) · [`crates/nexus42/`](../crates/nexus42/AGENTS.md) · [`crates/nexus42d/`](../crates/nexus42d/AGENTS.md) · [`crates/nexus-acp-host/`](../crates/nexus-acp-host/AGENTS.md)

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

**Index**: All knowledge documents are catalogued in [`.agents/knowledge/README.md`](knowledge/README.md). Maintenance rules (adding, reading, updating, archiving) apply per the upstream `mstar-plan-conventions` skill.

## External Design Specs

Nexus is an **open-source repo** but design specs live in the **private `nexus-platform` repo**. Configure paths via `.agents/local-paths.json` (copy from `.agents/local-paths.json.example`; the file is gitignored). Once configured, resolve `specs_root` for:

- **Roadmap**: `{specs_root.roadmap}` — update when a plan is marked `Done` (see pre-merge checklist below)
- **Architecture**: `{specs_root.v1-spec}/architecture/v1.md`
- **Domain Model**: `{specs_root.v1-spec}/domain/data-model-v1.md`

## Documentation & Plans (Mandatory Reachability)

All in-repo documentation and agent plans MUST be reachable from a fresh `git clone`:

- **Do not** reference `.gitignore`-excluded or out-of-repo paths (e.g. `~/.config/...`, absolute home paths, arbitrary sibling directories). Inline external context or link to stable public URLs.
- **Do not** paste machine-specific paths (`/Users/<you>/...`, `C:\Users\...`) in tracked artifacts — use repo-relative paths or neutral placeholders (`<repository-root>`).

Violations break onboarding and agent handoff.

## Plans & Reports Structure

`{PLAN_DIR}` = `.agents/plans/`. New plan files land under `{PLAN_DIR}`, not `docs/superpowers/plans/`.

### Plan Lifecycle

1. **Todo**: Plan created, not started
2. **InProgress**: Implementation underway
3. **InReview**: QC review in progress (reports in `reports/<plan-id>/`)
4. **Blocked**: Waiting on dependency, decision, or another plan
5. **Done**: Completed, merged to main; row archived to `{HARNESS_DIR}/archived/plans/`

**Multi-batch plans:** Default QC triple-review **once** after the whole plan's dev work completes (not per batch); see upstream `mstar-plan-conventions` skill.

### Pre-merge Checklist

Before merging plan work:

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` (at `{specs_root.roadmap}`) if a plan is marked `Done`
4. Archive Done plan rows per upstream `mstar-plan-conventions`

## Project-Specific Deviations

### Plan compaction profile

Uses the **unified compression rule** from upstream `mstar-plan-conventions`: `status.json.plans[]` keeps only non-`Done` rows. Done rows are moved to `archived/plans/` and indexed in `archived/plans-done.json`.

### Residual detail prose (`plans/residuals/`)

Open residuals that need more than the structured fields in `status.json` can have prose detail documents under `plans/residuals/<plan-id>/`. These complement (not replace) `metadata.residual_findings` entries.

- **What goes here**: pure deferral records, legacy issue explanations, "why this was deferred + current code state + future implementer pointers". Named `<td-or-r-id>-<short-label>.md`.
- **What stays in `knowledge/`**: design documents that incidentally mention residuals (e.g., crate-selection doc noting DoS guard TODOs).
- **Lifecycle**: when the residual is closed, the prose doc is archived alongside the structured JSON to `archived/residuals/<plan-id>.json`.

### Verification Commands

```bash
# Verify tech_debt_summary matches residual_findings
jq '.metadata.tech_debt_summary.total_open == (.metadata.residual_findings | to_entries | map(.value | length) | add)' .agents/status.json

# Verify no Done rows leak
jq '[.plans[] | select(.status == "Done")] | length' .agents/status.json

# View open residuals by plan
jq '.metadata.residual_findings | to_entries[] | {plan: .key, count: (.value | length)}' .agents/status.json
```
