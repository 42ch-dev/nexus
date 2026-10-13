---
module: local-api, apps/nexus-service, web-ui, qa
date: 2026-07-02
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: high
tags:
  - on-demand-synthesis
  - llm-gating
  - read-path-invariant
  - poll-side-effects
  - headless-qa-gap
  - static-analysis-review
  - greptile
  - local-api
applies_when:
  - designing (or reviewing) an on-demand LLM/agent-call endpoint where a client polls for status and synthesis must fire only on explicit user intent
---

# On-demand synthesis read-path invariant — gate every LLM call behind explicit intent; verify the poll path never reaches the synthesizer

## Context

`POST /v1/local/memory/soul/reflect` is an **on-demand** LLM synthesis endpoint (Creator-SOUL Narrative). The web UI **polls** it every ~30s with `force_regenerate: false` to render status (ungenerated / current / stale / insufficient); the author explicitly opts into the expensive synthesis by sending `force_regenerate: true` (the "Reflect on my SOUL" CTA). This on-demand discipline is a proportional-to-local-only model (no background jobs; synchronous, user-triggered).

A status/poll endpoint that can silently reach an LLM/agent call is the failure this doc guards against: a creator who passes the data gate but has never generated a narrative must return `ungenerated`, and must never fall through to **LLM synthesis on every 30s background poll** — silently, without user intent, unbounded.

This doc captures two lessons: the read-path invariant that a poll path must honour, and the verification gap that lets QC + QA miss a violation.

## Guidance

### The invariant

> **An on-demand synthesis endpoint must reach the synthesizer ONLY via the
> explicit-intent code path. The read/poll path (`force=false` / GET-style) is
> side-effect-free: it returns a state enum and NEVER invokes the LLM/agent.**

The correct control flow for `/soul/reflect`:

```
if below data-gate         → state: "insufficient_data"   (no LLM)
if !force:
    cached + has-narrative + !stale → state: "current"     (no LLM)
    cached + has-narrative + stale  → state: "stale"       (no LLM, banner prompts re-reflect)
    else (no cache / stats-only)    → state: "ungenerated"  (no LLM)   ← THE MISSING BRANCH
// ONLY reached when force == true
synthesize → persist → state: "current"
```

The failure shape: the `!force && !stale` block returns `current` only `if let
Some(cached)`; when `cached` is `None` (ungenerated) it **falls through** past
the stale block (stale is false when there's no cache) into the synthesis block.
A single missing `else → ungenerated` return makes the poll path fire the LLM.

**Anti-pattern name**: *the fall-through-to-synthesis gap*. It is one missing
`else`/early-return away in a status-branching handler, and it is silent — there
is no crash, no test failure (headless), no log that distinguishes "poll" from
"explicit reflect."

### The verification gap

> **Headless QA cannot exercise a live LLM/agent-call path** (no configured
> worker / ACP registry in the test environment), so a "synthesis silently fires
> on poll" bug is invisible to runtime tests — the buggy path either no-ops (no
> registry → error mapped away) or is never asserted as "must NOT be reached."

The limit is exactly where the bug hides: the gate (force vs !force) is not
verified end-to-end because the synthesis side cannot run.

**Mitigations (apply ≥1 to any on-demand synthesis endpoint):**

1. **Negative-path test that the synthesizer is NOT reached on the read path.**
   In test mode the capability registry is `None`; a `force_regenerate: false`
   + above-gate + no-cache request must **succeed** (return `ungenerated`) —
   *not* error with "capability registry not available." If the read path
   reaches the registry lookup at all, the test fails. The fix adds
   exactly this test (`force=false` → `ungenerated` succeeds without registry;
   `force=true` → `ServiceUnavailable` proves the synth path is reached only
   for `force=true`). This is the cheap, deterministic, headless-friendly guard.
2. **Complementary static-analysis review (Greptile / equivalent).** A
   control-flow analysis catches "this state value is never produced" / "this
   code path is reachable from a poll" that runtime tests structured around the
   happy path miss. Treat the greploop review as a required gate for endpoints
   with side effects gated by a request flag.
3. **State-enum exhaustiveness.** If the response carries a `state` enum
   (`ungenerated | current | stale | insufficient_data`), every enum value must
   be reachable by some input combination — and a test must assert each is
   reachable. An unreachable enum value is the signature of a fall-through gap.

### Why this matters

Any "poll for status, act on explicit intent" endpoint repeats this shape:
on-demand generation (narratives, summaries, reports), trigger-vs-status action
endpoints, cache-warm-vs-refresh APIs. The bug class is one missing branch in a
status handler; the detection gap is universal wherever the side effect can't
run in CI. The negative-path "synthesizer not reached" test is the portable fix.

## Examples

- **Status-branching synthesis handler** (`POST /soul/reflect`) — a `force=false`
  poll fell through to `acp.prompt` for ungenerated creators. The fix gates
  synthesis behind `force=true`, adds the `ungenerated` return, and adds the
  negative-path test. A static-analysis review caught it; QC tri-review and
  headless QA missed it.
- **Hypothetical**: any future `POST /generate-report` with a `?watch=true` poll
  mode — same trap if the watch path can reach the generator.

## Relationship / consolidation flag

Part of the same `/soul/reflect` endpoint family as
[`fingerprint-cached-live-aggregate.md`](fingerprint-cached-live-aggregate.md)
(read-path **cost**) and
[`bounded-drain-completion-contract.md`](bounded-drain-completion-contract.md)
(write-side **drain semantics**). This doc is the read-path **side-effect
gating** lesson. The three together cover the endpoint's read-cost /
write-drain / read-gating axes; a future unifying "local on-demand endpoint
contract" doc could consolidate them.

## QC checklist note

For endpoints whose side effect (LLM/agent call) is not CI-exercisable, add an
explicit "read path does not reach the side-effect site" assertion to the QC
checklist — do not rely on the implementer's self-check or headless tests alone.
