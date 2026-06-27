---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-26-v1.67-frontend-scope-gaps"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness (focus per role parameters)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-frontend-scope-gaps
- Review range / Diff basis: P1 commits `e74321db`+`963fa1ed`+`aeaaf91a` merged at HEAD; diff basis vs `26e477ee`
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6 (apps/web/src/pages/dialogs/create-work-dialog.tsx, create-work-dialog.test.tsx, apps/web/src/lib/nexus/browser-client.ts, browser-client.test.ts, types.ts, tauri-client.ts; plus AGENTS.md housekeeping)
- Commit range: e74321db, 963fa1ed, aeaaf91a (P1 changes only; merged via fedf82e4)
- Tools run: git rev-parse/branch/log/diff, read (source + tests), glob (report dir)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None (no blocking correctness or security issues in reviewed scope).

### 🟢 Suggestion
- **G1 (work_profile)**: The four valid profiles (`novel`, `essay`, `game-bible`, `script`) are defined in a `const WORK_PROFILES` array and surfaced exclusively via a native `<Select>` whose `<option value>`s come from that array. The selected value is passed through verbatim as `work_profile: workProfile` in the `CreateWorkRequest` body. There is no additional client-side runtime guard (e.g., `WORK_PROFILES.map(p => p.value).includes(...)` or branded type) before the POST. An untouched form correctly defaults to `'novel'`. If the component state were mutated outside the select (devtools, future prop injection, or test harness), an arbitrary string could be sent. The plan states the daemon already accepts the field; server-side rejection of unknown values is the intended enforcement. Consider adding a small const-based guard or documenting the reliance on server validation for future-proofing.
- **G2 (preset CRUD transport)**: `getPreset` / `updatePreset` / `deletePreset` correctly map to `GET` / `PATCH` / `DELETE` `/v1/local/presets/{id}` and use the generated request/response types (`GetPresetResponse`, `UpdatePresetRequest`/`UpdatePresetResponse`). The `presetId` path segment is always wrapped with `encodeURIComponent(...)`. The private `delete<T>()` helper mirrors the existing transport pattern and correctly treats 204 No Content as `void`. No form-based management UI for these methods is introduced in this change (explicitly deferred), so no new surface for bypassing "user preset only" restrictions exists in the delivered scope. Daemon-side enforcement (source == 'user' for update/delete) remains the boundary.
- **Security (input to fetch/path)**: No free-text or unvalidated user input reaches URL paths for the new selector. `workProfile` originates only from the controlled select options. For preset methods, only the `presetId` parameter is interpolated (after `encodeURIComponent`). No XSS vectors in the selector (plain `<option>` text, wire identifier values, no `dangerouslySetInnerHTML` or raw HTML interpolation).
- **No wire change**: Confirmed — only client interface promotion and a UI control; generated TS types and daemon routes pre-existed.

## Source Trace
- Finding ID: QC2-001 (work_profile pass-through)
- Source Type: manual code review + diff inspection
- Source Reference: `apps/web/src/pages/dialogs/create-work-dialog.tsx:10-15` (WORK_PROFILES), `41`, `50`, `68` (state + submit); `create-work-dialog.test.tsx:70-72` (assertion)
- Confidence: High

- Finding ID: QC2-002 (preset CRUD routes + verbs + types)
- Source Type: manual code review + diff inspection
- Source Reference: `apps/web/src/lib/nexus/types.ts:124-129`, `browser-client.ts:159-170` (implementations + encodeURIComponent), `159-170` (private delete), test block at `browser-client.test.ts:233-276`
- Confidence: High

- Finding ID: QC2-003 (XSS / unvalidated input)
- Source Type: manual code review
- Source Reference: `create-work-dialog.tsx:118-128` (native Select + options from const), `browser-client.ts:160,164,169` (encodeURIComponent on all preset ids)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (non-blocking; 1 on client guard hygiene, 2 on transport correctness + security surface) |

**Verdict**: Approve

## Revalidation
N/A (initial wave).
