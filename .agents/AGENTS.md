# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Concepts

| Symbol | Meaning | Path |
|--------|---------|------|
| `{HARNESS_DIR}` | Root of agent/engineering infrastructure | `.agents/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `.agents/plans/` |

## Upstream Harness

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework. Harness conventions (residual lifecycle, `status.json` structure, `knowledge/` management, QC/QA report naming, severity levels, etc.) are defined by upstream `mstar-*` skills. This repo follows upstream defaults unless noted in **Project-Specific Deviations** below.

## Documentation & Plans (Mandatory Reachability)

All in-repo documentation and agent plans MUST be reachable from a fresh `git clone`:

- **Do not** reference `.gitignore`-excluded or out-of-repo paths (e.g., `~/.config/...`, absolute home paths, sibling directories). Inline external context or link to stable public URLs.
- **Do not** paste machine-specific paths (`/Users/<you>/...`) in tracked artifacts — use repo-relative paths or neutral placeholders.

## Content Boundary: `docs/` vs `.agents/knowledge/`

- **`docs/`**: end-user and contributor documentation (installation, quickstart, architecture overview, contributing). **Do NOT** place architecture review reports, per-plan design decisions, or plan inputs/outputs here.
- **`.agents/knowledge/`**: dev-process artifacts (architecture review reports, design decision records, gap analyses, plan implementation context). Indexed in [`.agents/knowledge/README.md`](knowledge/README.md).

## Plan Lifecycle

1. **Todo** → 2. **InProgress** → 3. **InReview** (QC reports in `reports/<plan-id>/`) → 4. **Blocked** → 5. **Done** (archived to `archived/plans/`).

**Multi-batch plans:** default QC triple-review once after all dev work completes (not per batch).

### Pre-merge Checklist

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` if a plan is marked `Done`
4. Archive Done plan rows per upstream `mstar-plan-conventions`

## Project-Specific Deviations

### Plan compaction profile (this repository)

This repository uses **Profile B** from the Morning Star `mstar-plan-conventions` skill.

- `status.json.plans[]` keeps **non-`Done`** plans only.
- Every `Done` plan MUST be represented in both `archived/plans/<plan-id>.json` (full snapshot) and `archived/plans-done.json` (minimal catalog).
- Historical `Done` discovery MUST read `archived/plans-done.json`, not `status.json.plans[]`.

### Residual detail prose (`plans/residuals/`)

Open residuals needing more than structured `status.json` fields may have prose detail documents under `plans/residuals/<plan-id>/`. These complement (not replace) **root-level** `residual_findings` entries in `status.json` (canonical per upstream `mstar-plan-conventions`). Named `<td-or-r-id>-<short-label>.md`. When the residual is closed, the prose doc is archived alongside the structured JSON to `archived/residuals/<plan-id>.json`.
