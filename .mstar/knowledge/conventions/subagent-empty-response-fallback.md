---
module: harness
date: 2026-07-20
problem_type: workflow_convention
category: conventions
severity: medium
plan_id: 2026-07-20-v1.126-p0-shell-selection-submenu
tags: [harness, subagents, dispatch, openCode, fallback, sdd]
applies_when: PM dispatches a specialist subagent (frontend-dev / qc-specialist* / etc.) for SDD task implementation or review and receives an empty / no-content response
---

# Subagent Empty-Response Fallback Pattern

## Context

OpenCode's task tool dispatches named role subagents (`frontend-dev`, `qc-specialist*`, `architect`, `writing-specialist`, etc.). On this harness + host combination, specialist subagents intermittently return `state: completed` with **empty / no-content** task results. The work was either:

- **Not done** (most common): the subagent session terminated without producing edits or commits. Working tree unchanged.
- **Done but unreported** (less common): files were modified but the report payload is empty (verify via `git status` + `git log`).

V1.124 retrospective first flagged this; V1.126 P0 + P1 + P2 + P3 hit it ~5 times across the iteration (frontend-dev empty; qc-specialist-3 empty twice; QC fix-wave general empty once).

## Guidance

### Detection

After every subagent dispatch, **always** verify the result:

```bash
# For implementer dispatch:
git log --oneline <base>..HEAD           # any commits?
git status --short                        # any uncommitted edits?

# For reviewer dispatch:
ls -la <report-path>                      # report file exists?
wc -l <report-path>                       # report non-empty?
```

If `git log` shows no new commits AND `git status` is clean AND the task_result is empty → **empty-response**.

### Fallback sequence (in order)

1. **Retry the same specialist subagent once** with a tighter, simpler prompt (sometimes the issue is prompt complexity or context length).
2. **Switch to `general` agent** with the same brief. `general` (OpenCode built-in) has direct tool access and is more reliable for multi-step implementation + review work. This is the documented V1.124 fallback pattern.
3. **For QC seat retries specifically**: if `qc-specialist-3` returns empty twice, fall back to `general` for that seat. The harness `mstar-roles` parameter table allows `general` as a fallback reviewer for L2 task review (per `mstar-dispatch-gates` per-task informal review allowance). **For L3 plan QC**, `general` is acceptable when specialist seats return empty; document the fallback in the QC report header.
4. **Escalate to user** only if BOTH specialist AND `general` return empty on the same task — this is a real `Blocked` condition (likely a host runtime issue).

### PM inline fallback (whitelist only)

For **non-behavioral text edits only** (e.g. updating spec Status lines, fixing obvious terminology drift flagged by a specialist report, registering residuals in status.json), PM may apply the fix inline per the PM whitelist (`mstar-roles/references/project-manager.md` § "Minimal non-behavioral text edits"). V1.126 P0 §1.6 writing-specialist seat 3 returned empty; PM applied the spec Status line updates + Iteration package column directly (documented as `inline fallback` in spec status). **PM MUST NOT inline-implement product code** — that violates `mstar-dispatch-gates`.

### Documentation requirement

When the fallback fires, document it in:

- The plan / spec / report (e.g. spec Status: `writing-hygiene done (Phase 1 §1.6 seat 3 inline fallback — empty subagent response; PM applied flagged hygiene per V1.124 pattern)`).
- The iteration's Retrospective section (`## Iteration Retrospective (minimal) > 可改进的`).
- The Compass's Phase 1 Review & Edit chain preamble.

This makes the empty-response count + fallback decision auditable.

## Why This Matters

Without a documented fallback, PM hits `Blocked` on every empty response and the iteration stalls. With the fallback, the iteration proceeds; the root cause (specialist subagent runtime instability) is tracked for separate investigation.

## When to Apply

- **Any time** a specialist subagent returns `state: completed` with empty / no-content task_result.
- **Plan QC tri-review** seat retries (qc-specialist-1/2/3 empty → general fallback).
- **Phase 1 Review & Edit chain** seat retries (product-manager / architect / writing-specialist empty).
- **SDD implementer dispatches** (frontend-dev / fullstack-dev empty).

**Do NOT apply** when the response is non-empty but the work is incorrect — that's a normal fix-wave, not a fallback.

## What Didn't Work (V1.126 evidence)

- Retrying `qc-specialist-3` with the same prompt twice — empty both times.
- Retrying `frontend-dev` with the same prompt — empty.
- Tighter prompt + `general` agent → worked on first try (5/5 successes in V1.126).

Prompt complexity appears correlated with empty-response rate; the specialist agent routing may have context-length or token-budget issues that `general` (with simpler routing) avoids.

## Examples

V1.126 dispatch log (5 fallbacks out of ~22 total dispatches ≈ 23% empty-response rate):

| Seat / Task | Specialist | Result | Fallback | Outcome |
|-------------|------------|--------|----------|---------|
| P0 T1 implementer | frontend-dev | empty | retry general | done |
| P0 QC seat 3 (first try) | qc-specialist-3 | empty | retry general | done |
| P0 QC seat 3 (second try) | qc-specialist-3 | empty | retry general (tighter prompt) | done |
| P0 QC fix-wave (first try) | general | empty | retry general (tighter prompt) | done |
| Phase 1 §1.6 seat 3 | writing-specialist | empty (touched files but no edits) | PM inline hygiene | done |

## See also

- `mstar-dispatch-gates` (反递归红线 + dispatch mechanics)
- `mstar-host/references/opencode.md` (OpenCode task tool behavior)
- V1.124 retrospective (first documented occurrence)
- V1.126 retrospective (this iteration's frequency data)
