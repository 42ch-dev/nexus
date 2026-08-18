---
module: harness (status.json residual_findings), .mstar/archived/residuals
date: 2026-08-08
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-08-08-v1.155-p2-residual-findings-sweep
tags: [residual-findings, sweep, triage, status-json, archival, closure-evidence, verified-stale, blockers-only]
applies_when: running a bulk sweep of open residual findings at iteration close; triaging a large backlog of status.json residual rows; recomputing tech_debt_summary rollups
---

# Residual Findings Sweep Playbook (V1.155 P2 pattern)

## Context

At V1.155 iteration-start, `status.json` `residual_findings` carried **84 open
findings** across V1.138..V1.153 (codegen, terminology, adapter-era,
compute-era, dogfood sweeps, cross-cutting). Many were stale (superseded by
later iterations), some cheap to fix in-wave, a few true blockers (external
deps / Durable Roadmap). The P2 sweep reduced this to **15 blockers-only**
(−69) with every closed finding archived with closure evidence. This playbook
captures the triage process so future sweeps (iteration-close or standalone
tech-debt plans) do not rediscover the pitfalls.

## Guidance

### 1. Triage order = chronology (oldest first)

Older findings are most likely stale (superseded by later iterations). Newest
first risks double-closing findings owned by plans that closed them earlier in
the same iteration. Batch by era: e.g. V1.138–V1.141, V1.142–V1.146,
V1.147–V1.153 + cross-cutting.

### 2. Use the INCLUSIVE open-filter when counting

A finding is "open" if `lifecycle=="open"` OR `status=="open"` OR
(`decision=="defer"` without a closed lifecycle). Excludes the string key
`_recovery_note` (a recovery artifact, not a finding). Counts before/after MUST
use this same filter — naive single-field filters undercount and break the
"84 → 15" evidence.

### 3. SSOT cross-check before classifying (triage prerequisite)

Resolve every named item to its exact `status.json` residual id before
classifying. Plan names drift from the SSOT, and several named concerns are
**already closed** — e.g. in V1.155 P2, `compute_sessions` TTL/sweep and
`build.rs` rerun-if-changed were already closed by V1.147 P3, and `promote_adopt`
transaction-boundary was closed V1.142 P3. **Act only on findings still open;
never re-open or double-close.** Skip findings owned by plans closing them in
the same iteration (P0/P1 interplay); verify their archive exists at sweep end.

### 4. Three-way classification per finding

- **verified-stale** (superseded/fixed by later iterations) → archive with
  evidence.
- **fix-now** (cheap low/nit, ≤ ~30 min equivalent, no new surface) → fix
  in-wave, then close.
- **defer** (true blocker) → keep open with `decision: defer` + `lifecycle:
  open` + Durable Roadmap pointer or external trigger/target. Only external
  deps / partner-input / explicit user defer qualify; anything bigger than the
  cheap bar defaults to defer (do not burn the wave on tx-boundary/schema/
  product work).

### 5. Closure evidence must be factually correct — verify with recursive grep

A false-0 grep reclassifies a finding wrongly. V1.155 T1 lesson: an
`accepted`-classified finding was reclassified to `waived` with corrected
evidence when the grep that "proved" closure was actually searching the wrong
scope. The evidence bar for verified-stale: grep/read shows the concern no
longer exists (terminology swept, goldens refreshed, port now production);
closure note cites the artifact — no "trust me".

### 6. Archive flow: close → archive with closure_note + evidence

- Close in `status.json` and archive to `.mstar/archived/residuals/<plan-id>.json`
  (bare JSON array; **read first, merge, never overwrite** — same-file
  append discipline as Profile B compaction).
- Migrate in-place-closed rows (already `lifecycle: resolved` / status-closed
  but still listed) into archives, preserving their closure notes and
  re-verification.
- Duplicates: if two open rows cover the same alert, archive one as
  `lifecycle: duplicate` of the canonical row (keep the canonical one open).
- Never re-open archived findings without evidence.

### 7. Blockers stay open with dated targets

End state = **true blockers only**: external deps (with trigger + target, e.g.
dependabot trio: yamux/libp2p ≥0.57, hickory-proto, react-router 7→8) or
Durable Roadmap defers (roadmap pointer per row). Do not close external
blockers "because the EPIC dropped them" — they stay open with updated targets.

### 8. Non-array `residual_findings` keys break array-only tooling

Rollup/validation tooling treats `residual_findings` values as **arrays
only** — the historical rollup script counted with that filter, and the
current engine validator (`mstar status validate`) hard-fails on any
non-array key (observed 2026-08-18). A string-valued `_recovery_note` key
therefore breaks both. Prose/recovery notes belong in the program timeline —
never as a `residual_findings` key. After a sweep, recompute
`metadata.tech_debt_summary` with the array-only filter and assert
`total_open == by_severity == by_target == by_plan` (15 == 15 == 15 == 15 in
V1.155).

## Why This Matters

The residual SSOT becomes trustworthy only when **open = real blocker; closed =
verified with evidence**. A sweep that double-closes, fabricates closure
evidence, or miscounts destroys that trust and corrupts the next iteration's
scoping (see also `architecture-patterns/resolved-residual-verification.md`:
lifecycle is a claim, not a guarantee). The chronological order + inclusive
filter + evidence bar together make the sweep auditable.

## When to Apply

- Iteration-close compound rounds when `status.json` open findings exceed a
  small number (V1.155: 84 → 15; trigger: any bulk backlog).
- Convergence iterations scoping against a backlog of deferred residuals.
- Recomputing `tech_debt_summary` after any bulk archival.

## Examples

### Inclusive open-filter (canonical)

```python
open_rows = [r for r in findings
             if r.get("lifecycle") == "open"
             or r.get("status") == "open"
             or (r.get("decision") == "defer" and r.get("lifecycle") not in ("closed", "resolved", "archived"))]
# exclude string key _recovery_note from rollup input entirely
```

### Before → after (V1.155 P2 evidence)

| Step | Open count |
| --- | --- |
| Iteration start | 84 |
| T1–T3 triage + hotfix wave | 16 |
| QC fix wave (duplicate `R-V1140P0-001` archived) | 15 |
| End state | 15 = 3 external blockers + 10 roadmap defers + 1 P0 documented defer + 1 ops-track defer |

## See also

- `conventions/profile-b-residual-archival-procedure.md` — archival mechanics
  (eligibility, ND-A2 closure labels, 8-step procedure) for compaction; the
  sweep uses the same archive file discipline.
- `architecture-patterns/resolved-residual-verification.md` — lifecycle claims
  must be verified against current code before treating a class as closed.
