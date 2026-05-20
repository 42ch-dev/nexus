# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Concepts

| Symbol | Meaning | Path |
|--------|---------|------|
| `{HARNESS_DIR}` | Root of agent/engineering infrastructure | `.agents/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `.agents/plans/` |
| `{ITERATION_DIR}` | Iteration-level compass specs (version scope/acceptance/risk) | `.agents/iterations/` |
| `{KNOWLEDGE_DIR}` | Knowledge root (rules, trackers) + [`knowledge/specs/`](knowledge/specs/README.md) (functional/normative specs) | `.agents/knowledge/` |

## Upstream Harness

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework. Default harness behavior lives in upstream `mstar-*` skills; this file records **project-specific deviations** only.

**Load order (harness work):** Read `mstar-harness-core`, then `mstar-plan-conventions` (+ `mstar-review-qc` when touching `InReview` or QC reports). State machine, QC triple-review timing, and multi-batch rules are **not** duplicated here.

## Reachability

Git-tracked docs and plans must be openable after a fresh `git clone`: no `.gitignore`-d paths, machine-specific absolute paths, or untracked sibling directories as sole authorities. Use repo-relative paths or stable public URLs.

## Content Boundary: `docs/` vs `.agents/iterations/` vs `.agents/knowledge/`

- **`docs/`**: end-user and contributor documentation (installation, quickstart, architecture overview, contributing). **Do NOT** place architecture review reports, per-plan design decisions, or plan inputs/outputs here.
- **`.agents/iterations/`**: iteration-level specs for a delivery version — including `*-delivery-compass-*.md` and legacy `v1.*` compass artifacts (overview, matrix, program notes). Indexed in [`.agents/iterations/README.md`](iterations/README.md).
- **`.agents/knowledge/specs/`**: functional and normative OSS specs (including [`specs/`](knowledge/specs/README.md) migrated from platform `v1-spec/local/`). Index: [`knowledge/specs/README.md`](knowledge/specs/README.md).
- **`.agents/knowledge/`** (root files): cross-cutting rules and trackers only — see [`knowledge/README.md`](knowledge/README.md). Layout: [`knowledge/AGENTS.md`](knowledge/AGENTS.md).

## Pre-merge Checklist (this repository)

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` if a plan is marked `Done`
4. Archive Done plan rows per `mstar-plan-conventions` (`references/done-compaction.md`, Profile B)

## Project-Specific Deviations

### Plan compaction profile

**Profile B** — Morning Star `mstar-plan-conventions` → `references/done-compaction.md` (Template B). `status.json.plans[]` keeps **non-`Done`** plans only; historical `Done` discovery uses `archived/plans-done.json` and `archived/plans/<plan-id>.json`.

### Residual detail prose (`plans/residuals/`)

Optional Markdown under `plans/residuals/<plan-id>/`, named `<finding-id>-<short-label>.md`; supplements root `residual_findings` (see upstream `mstar-plan-conventions`). Archive prose with structured JSON to `archived/residuals/<plan-id>.json` when closed.
