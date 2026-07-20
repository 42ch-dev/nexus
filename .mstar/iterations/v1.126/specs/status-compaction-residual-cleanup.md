# Spec — status.json compaction + V1.121+…+V1.125 residual cleanup (V1.126 P3)

**Status:** product-reviewed, architect-locked, writing-hygiene done (Phase 1 §1.6 seat 3 inline fallback — empty subagent response; PM applied flagged hygiene per V1.124 pattern)
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1126-4
**Plan:** [`2026-07-20-v1.126-p3-status-compaction-residual-cleanup`](../../../plans/2026-07-20-v1.126-p3-status-compaction-residual-cleanup.md)
**Wire contracts:** `wire_contracts_changed: false` — harness hygiene; no schemas, no daemon, no web consumer.

## Problem

`.mstar/status.json` is **128 KB** (6.4× over the 20 KB threshold set by `.mstar/AGENTS.md` § Pre-merge checklist #5). 263 open residuals (1 high, 3 medium, 108 low, 151 nit) span V1.91 → V1.125; the V1.121+ cluster alone is ~80% of the volume. Profile B compaction is in effect for `plans[]` (Done rows snapshotted + removed from hot `plans[]`) but residuals stay in `status.json::residual_findings` indefinitely. `DF-V1122-STATUS-COMPACT` + `DF-V1123-STATUS-COMPACT` + `DF-V1123-RESIDUAL-CLEANUP` have been deferred for 3+ iterations.

## Normative decisions (PM initial — pending seat 1/2/3)

1. **Compaction strategy — extend Profile B to residuals.** Same shape as `plans[]` compaction: Done-plan residuals at `nit`/`low` severity move to `.mstar/archived/residuals/<plan-id>.json`; hot `plans[]` + open `residual_findings` stay under 20 KB.
2. **Eligibility rule (locked).** A `residual_findings[<plan-id>]` entry (or sub-entry) is eligible for archival when **all** of:
   - Plan row `status: Done`
   - The residual entry severity is `nit` or `low` (no `medium`/`high`/`critical`)
   - Plan is from V1.125 or earlier (V1.126 P0/P1/P2 plan residuals stay open — P3 runs last)
   
   **Mixed-severity plan_id handling (explicit at seat 1):** if a Done plan has a mix of nit/low and medium/high residuals, only the nit/low entries are archived individually (not the whole plan_id array). The medium/high entries stay in `status.json::residual_findings[<plan-id>]` until explicitly closed. This preserves plan_id traceability while shrinking volume.
3. **Per-entry closure labels (locked).** Every archived residual gets a `closure_note`:
   - `"Fixed by <plan-id>"` — observably fixed by a later plan (e.g. V1.124 P0 fixture gap closed by V1.126 P1 directed-axis).
   - `"Stale — <reason>"` — file path removed; code pattern already deleted; documentation gap since documented.
   - `"Superseded — <reason>"` — replaced by a different mechanism (e.g. a residual about "Need composite endpoint for fan-out" is superseded by V1.126 P2 itself; cross-reference the new mechanism).
   - `"Archived as low/nit bulk (V1.126 P3 gate)"` — real low/nit tech debt that does not warrant product work.
4. **Medium/high/critical NEVER archived without explicit closure.** The 4 currently-open `medium` + 1 `high` residuals stay in `status.json`:
   - `[medium]` V1.92 Resume TOFU fingerprint gate (in-band over TLS)
   - `[medium]` V1.112 i18n migration remainder (~25 secondary strings)
   - `[medium]` V1.116 CodexNativeProvider registration
   - `[high]` V1.16-hotfix creator PATCH 404 (already fixed via hotfix; verify still open / close if confirmed — if confirmed fixed, this becomes `"Fixed by <hotfix-plan-id>"` and can be archived, but the verification happens in T2 not by bulk archival)
5. **Notes.json entry.** One entry per V1.126 close records the compaction (baseline size, post size, residual counts before/after, archive path).
6. **Tracker discipline.** `DF-V1123-STATUS-COMPACT` + `DF-V1123-RESIDUAL-CLEANUP` + `DF-V1122-STATUS-COMPACT` all closed (the last is superseded by the V1.123 row; both retire together). Shipped archive gains three entries.
7. **Future gate — not retired.** Closing the DF rows does **not** mean the 20 KB gate is permanently retired. Future iterations must keep `status.json` < 20 KB by archiving Done-plan residuals in their P-last gate. The 20 KB gate is a **standing pre-merge checklist item**.
8. **Product-debt visibility (locked at seat 1).** Any archived residual whose `closure_note` is `"Archived as low/nit bulk (V1.126 P3 gate)"` AND whose original `title` or `notes` field suggests a ≥ medium product impact (subjective but documented in T2 closure-review notes) gets a corresponding entry in `.mstar/notes.json` so it stays visible in future tech-debt roadmap reviews. This is the safeguard against the eligibility rule hiding real product debt.

## Architecture locks (architect seat 2)

> Ratified 2026-07-20. All AQ verdicts are final — implementers treat these as non-negotiable architecture contracts.

### ND-A1 — T1 sizing (AQ-7)

- **Single SDD task is LOCKED.** The 8 steps are structurally sequential (archival → compaction → verify). Splitting would create intermediate malformed `status.json` states. Each step has its own checkbox for progress tracking.
- **Fallback:** If the single-task scope exceeds one SDD session duration, fall back to **inline execution** — PM dispatches `fullstack-dev` directly (`Execution mode: inline`) per the P3 plan preamble. The implementer ticks the 8 step checkboxes inline.
- **Task length note:** T1 is expected to be the longest task in V1.126. The implementer should track progress via step-by-step checkbox ticks within T1 (do not split into separate SDD tasks).

### ND-A2 — `closure_note` schema (LOCKED)

- **String enum of exactly four values** (per seat 1 ND-3):
  1. `"Fixed by <plan-id>"` — observably fixed by a later plan. `<plan-id>` is a concrete plan identifier (e.g., `"Fixed by 2026-07-20-v1.126-p1-canvas-directed-axis"`).
  2. `"Stale — <reason>"` — file path removed; code pattern already deleted; documentation gap since documented. `<reason>` is a short factual statement, not opinion (e.g., `"Stale — file path apps/web/src/old-component.tsx no longer exists"`).
  3. `"Superseded — <reason>"` — replaced by a different mechanism. `<reason>` includes a cross-reference to the new mechanism (e.g., `"Superseded — composite endpoint replaces per-World kb/graph fan-out; see V1.126 P2"`).
  4. `"Archived as low/nit bulk (V1.126 P3 gate)"` — real low/nit tech debt that does not warrant product work in V1.126.
- **Validation:** Each archived residual entry MUST carry exactly one `closure_note` string matching one of these patterns. Implementer spot-checks 10 random entries in T2.

### ND-A3 — Archived residual file shape (LOCKED)

- **Format:** JSON array at `.mstar/archived/residuals/<plan-id>.json`.
- **Each entry shape:**
  ```jsonc
  {
    // Original fields from status.json::residual_findings[<plan-id>][<index>]:
    "id": "R-<plan-id>-<seq>",
    "severity": "nit | low",
    "title": "<original title>",
    "notes": "<original notes | null>",
    "tracking_links": ["<original links>"],
    "lifecycle": "resolved",
    // P3-added fields:
    "closure_note": "<one of four enum values per ND-A2>",
    "archived_at": "2026-07-20T00:00:00Z"
  }
  ```
- **Mirror existing pattern:** `.mstar/archived/plans/<plan-id>.json` (Profile B) uses a flat JSON object. Residuals files use a JSON **array** (multiple entries per plan). This is the existing pattern for residuals.
- **Co-locate with existing archives:** `.mstar/archived/residuals/<plan-id>.json` lives alongside `.mstar/archived/plans/<plan-id>.json` in the same archive directory.

### ND-A4 — tech_debt_summary refresh procedure

- **Script:** `./.mstar/plan-artifacts/scripts/tech-debt-rollup.sh` (or equivalent binary/command).
- **Count semantics:** `total_open` counts open residuals across all active `residual_findings` entries (post-compaction). Target: ≤ 30 (from current 77). `by_severity` breaks down by `critical`/`high`/`medium`/`low`/`nit`. `by_target` and `by_plan` are optional.
- **No prose fields.** `tech_debt_summary` is structured metadata only per `.mstar/AGENTS.md` § `status.json` — structured metadata only. Narrative about the compaction goes to `.mstar/notes.json` (P3 T4).
- **Timing:** Run after T1 step 5 (mixed-severity archival) and before T1 step 7 (size verification). The refreshed counts must reflect the post-compaction state.

### ND-A5 — Wire contracts verdict

- **`wire_contracts_changed: false` — CONFIRMED.** Harness hygiene only. No `schemas/`, no `crates/`, no `packages/`, no `apps/`, no `tooling/`.

## Architecture notes (implementer)

| Step | Action |
|------|--------|
| 1 | `wc -c .mstar/status.json` baseline |
| 2 | For each Done plan row in `plans[]`: confirm `.mstar/archived/plans/<plan-id>.json` snapshot exists; if not, create from current row |
| 3 | For each `residual_findings[<plan-id>]` eligible per rule #2: `jq` move to `.mstar/archived/residuals/<plan-id>.json` (extending existing pattern); add `closure_note` per entry per rule #3 |
| 4 | Remove `plans[]` Done rows from hot `plans[]` (Profile B compacted state) |
| 5 | Run `.mstar/plan-artifacts/scripts/tech-debt-rollup.sh` to refresh `metadata.tech_debt_summary`. Verify `total_open ≤ 30` (ND-A4). |
| 6 | `wc -c .mstar/status.json` < 20_000 (target ≤ 18_000 headroom) |
| 7 | `.mstar/notes.json` one entry: compaction summary |
| 8 | Tracker update (T3 in plan) |

## Acceptance

| ID | What we see |
|----|-------------|
| AC-V1126-4 | `wc -c .mstar/status.json < 20_000`; ≥ 50 low/nit residuals archived; `tech_debt_summary.total_open` substantially reduced from 77 (pre-V1.126) / 277 (post-V1.126 P0+P1+P2 residual registration) to ≤ 50 (target ≤ 30 was stretch goal; floor set by V1.126 plan residuals + medium/high residuals + V1.121 design-elevation cluster); no medium/high/critical archived without closure label |

## Out of scope

V1.121 design-elevation token sweep + arbitrary-value Tailwind cleanup (`DF-V1122-V1121-RES` stays scoped roadmap); bulk-closing medium/high residuals (they need product/code work); retiring the 20 KB gate (it stays as a standing checklist); new tooling for residual auto-archival (manual `jq` is sufficient for V1.126).
