---
module: harness
date: 2026-07-20
problem_type: workflow_convention
category: conventions
severity: low
plan_id: 2026-07-20-v1.126-p3-status-compaction-residual-cleanup
tags: [harness, profile-b, status-json, residual-management, iteration-close]
applies_when: Running Profile B status.json compaction as part of iteration-close or a dedicated tech-debt plan
---

# Profile B Residual Archival Procedure

## Context

Profile B compaction (`mstar-plan-artifacts/references/done-compaction.md` Template B) keeps `.mstar/status.json` small by snapshoting Done plan rows to `.mstar/archived/plans/<plan-id>.json` and removing them from hot `plans[]`. As of V1.126, the same pattern extends to **residual findings** — without it, `status.json` accumulates low/nit residuals forever and crosses the 20 KB hygiene line (`.mstar/AGENTS.md` § Pre-merge checklist #5).

## Guidance

### When to archive residuals (eligibility rule)

A `residual_findings[<plan-id>]` array is eligible for archival when **all three** are true:

1. The plan row is `status: Done`.
2. Every entry in the array is `severity: nit` or `severity: low` (no `medium` / `high` / `critical`).
3. The plan is from a prior iteration (not the currently-closing iteration — current-iteration plan residuals stay open so their QC findings remain visible during plan QC + QA).

### Mixed-severity handling

If a Done plan has e.g. 9 low + 1 medium residuals: archive only the 9 low; keep the medium with the plan_id key in `status.json::residual_findings`. The plan_id key is preserved as a "stub" so the medium entry remains discoverable. Do **not** drop the plan_id key when splitting.

### Closure labels (ND-A2 enum)

Every archived entry gets a `closure_note`:

| Label | When |
|-------|------|
| `Fixed by <plan-id>` | A later plan observably resolved it (cite the plan_id; verify via grep) |
| `Stale — <reason>` | File path removed; code pattern deleted; doc gap since documented |
| `Archived as low/nit bulk (<iteration-id> gate)` | Default for real low/nit tech debt that does not warrant product work |
| `Superseded by <X>` | Replaced by a newer residual or roadmap item |

Inherited (pre-existing) archived entries with non-enum formats may be ratified as legacy drift (record in `notes.json`); new archival MUST use the enum.

### Archive file shape

`.mstar/archived/residuals/<plan-id>.json` is a bare JSON array of entries (same shape as the in-status.json entries plus `closure_note` + `archived_at`). Extend the existing file if it exists; do not overwrite.

### Procedure (8 steps)

1. Snapshot Done plans to `.mstar/archived/plans/<plan-id>.json` if not already there; append plan_id to `archived/plans-done.json`.
2. Remove Done rows from hot `plans[]`.
3. For each `residual_findings[<plan-id>]`: decide archive eligibility per the rule.
4. Add `closure_note` per entry per the enum.
5. Move (not copy) eligible arrays (or filtered entries) to `archived/residuals/<plan-id>.json`.
6. Refresh `metadata.tech_debt_summary` (total_open, by_severity, by_target, by_plan).
7. Verify `wc -c .mstar/status.json < 20_000`.
8. Add `notes.json` entry summarising the compaction.

## Why This Matters

Without this procedure, `status.json` grows monotonically. V1.126 P3 drove it from 145 KB (6.4× the threshold) back to 14.6 KB. The standing gate (`DF-V1123-STATUS-COMPACT` retired; `.mstar/AGENTS.md` § Pre-merge checklist #5 still applies) is the floor — every iteration-close should run a light version of steps 3–7 to keep status.json under threshold.

## When to Apply

- **Mandatory**: at iteration-close (`mstar-iteration` §3.2 compound) when `wc -c .mstar/status.json >= 18_000` (headroom margin).
- **Recommended**: any plan whose Done + low/nit residual count exceeds 10 entries.
- **Skip**: if `status.json` is already well under 18 KB and the iteration added < 5 low/nit residuals.

## Examples

V1.126 P3 archived 244 V1.91–V1.124 residuals + 15 V1.121 cluster residuals = 259 entries. Result: status.json 145 KB → 14.6 KB; total_open 277 → 32 (the 32 floor = 24 V1.126 plan own-residuals + 4 medium + 1 high + 3 carry-forward).

## See also

- `.mstar/AGENTS.md` § Pre-merge checklist #5 (size gate)
- `mstar-plan-artifacts/references/done-compaction.md` (Profile A/B)
- `mstar-compound` (knowledge crystallization)
- `DF-V1123-STATUS-COMPACT` (closed V1.126 P3 — gate stays as standing checklist)
