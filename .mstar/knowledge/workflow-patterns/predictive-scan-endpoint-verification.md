---
module: harness
date: 2026-07-20
problem_type: workflow_convention
category: workflow-patterns
severity: medium
tags: [harness, predictive-scan, explore, endpoint-verification, ac-reframing, qc]
applies_when: PM uses an `explore` subagent (or any predictive read-only scan) to surface candidate bugs before manual testing, and the scan claims a user-visible symptom
---

# Predictive Scan Must Verify User-Visible Claims Against Actual Endpoints

## Context

In V1.127, the PM dispatched an `explore` subagent to scan V1.126 surfaces for manual-test friction points before the user's dogfood review. The scan produced a ranked P0/P1/P2 list of candidates with file:line evidence. The PM trusted the framing and locked direction + 2 plans around the top candidates.

One P0 candidate claimed: *"CodexNativeProvider / ClaudeNativeProvider never registered in daemon HostManager... the AgentPicker will show an empty agent list."* The plan (V1.127 P1) was scoped around this framing.

Plan-level QC (qc1 seat 1) caught the real picture: the AgentPicker does **not** use `HostManager::list_providers()`. It calls `POST /v1/daemon/agent-host/scan` (`crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs:577`), which runs `scan_path_in(...)` directly — an independent PATH scan that does NOT consult the registered providers in HostManager.

The original V1.116 residual `R-V1116P0QA-001` note already said: *"Discovery works (scan shows installed), but session creation fails."* The `explore` subagent missed this and framed the bug as "empty agent list" instead of "session-create failure".

T1 still shipped valuable work (closed the architecture-coherence gap + the `create_session` "provider not registered" failure mode), but the AC had to be reframed during the QC fix wave (qc1 C-001). The original framing would have shipped an unmet AC.

## Guidance

### Predictive scan must verify user-visible claims

When an `explore` or other read-only scan produces a candidate with a **user-visible symptom** ("the user sees X"), the scan report must include:

1. The exact endpoint / handler / UI component the user-visible symptom flows through.
2. A grep / file:line citation for that flow path (not just the data structure that's wrong).
3. Confirmation that fixing the cited data structure actually changes the user-visible symptom.

If the scan can't verify the user-visible path, it should mark the candidate `Confidence: low` and explicitly say "the data structure is wrong but the user-visible impact is inferred, not verified".

### Architect seat 2 should catch framing errors during AQ resolution

The Phase 1 §1.6 architect seat is the last chance to catch scan framing errors before plan locking. During AQ resolution, the architect should:

1. Read each scan finding's "user-visible symptom" claim.
2. Trace it to the actual endpoint / handler / component that produces the symptom.
3. If the trace diverges from the scan's claim, flag it as an AQ finding and propose AC reframing.

In V1.127 P1, the architect resolved AQ-5 (agent-list endpoint path) correctly to `GET /v1/daemon/agent-host/providers`, but did not flag that the AgentPicker uses `/scan` instead. The PM then wrote the AC against the wrong endpoint.

### AC reframing during QC fix wave is a legitimate outcome

When QC catches an AC that doesn't match delivered value, the right fix is usually **docs-only reframing** (not a code change). The plan should ship what was actually built, with the AC accurately describing the delivered value. Manual verification steps for the originally-promised-but-not-automatable user-visible outcome belong in the plan's `## QA Gate Summary`.

## Why This Matters

- **AC integrity:** shipping an AC that doesn't match delivered value erodes the harness gate system. qc1 C-001 caught it; future iterations might not.
- **Predictive scan trust:** the PM trusts `explore` to surface real bugs. If the scan's framing is unreliable, the PM has to re-verify every finding manually, defeating the purpose.
- **User-visible vs internal:** "internal data structure is wrong" and "user sees the wrong thing" are different problems. Conflating them produces plans that fix the wrong layer.

## When to Apply

- **Any time** an `explore` subagent produces a candidate with a user-visible symptom claim.
- **Phase 1 §1.6 architect seat 2** during AQ resolution — verify scan framing as part of technical verification.
- **Phase 2 plan QC tri-review** — qc1 architecture focus should ask "does the AC actually describe what the code does, given how the user-facing flow works?".

## Examples

### V1.127 P1 — explore conflation (caught at QC)

| Layer | What was claimed | What was actually true |
|-------|------------------|------------------------|
| `explore` scan | "AgentPicker shows empty list because providers not registered in HostManager" | AgentPicker uses `/agent-host/scan` (independent PATH scan); discovery works regardless of HostManager registration |
| Plan AC-V1127-7 (locked) | "AgentPicker shows discovered Codex/Claude; session create succeeds" | True value: closes architecture-coherence gap (HostManager invariant) + unblocks `create_session` "provider not registered" failure |
| QC fix wave (qc1 C-001) | "AC not connected end-to-end" — REQUEST_CHANGES | Reframed AC + manual verification step for session-create |

**Lesson:** the explore scan was right that `HostManager::new()` is empty; it was wrong about the user-visible symptom. The bug existed but at a different layer (session-create, not discovery).

### Positive pattern (V1.127 P0)

The same `explore` scan produced P0 findings for `worlds-page.tsx::handleCreateWorldClick` (empty no-op) and other frontend bugs. These were verified line-by-line by the architect seat 2 and matched the user-visible symptoms exactly. The P0 plan shipped cleanly with no AC reframing needed.

## See also

- `mstar-roles/references/qc-specialist/` (leaf QC focus areas — architecture coherence lens catches AC-vs-code mismatch)
- `mstar-dispatch-gates` (specialist review-and-edit dispatch — architect seat 2 verifies technical claims)
- V1.127 P1 QC consolidated `qc1.md` C-001 resolution (the original incident)
- `references/concepts-vocabulary.md` (no new CONCEPTS.md term needed — uses existing vocabulary)