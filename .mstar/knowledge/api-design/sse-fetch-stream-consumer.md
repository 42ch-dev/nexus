---
module: apps/web (BrowserClient SSE consumption)
date: 2026-09-30
problem_type: api_design
category: api-design
severity: medium
plan_id: 2026-09-30-v1.201-p1-run-observation-consumer
tags:
  - sse
  - fetch-stream
  - eventsource
  - browser-client
  - workflow-observation
  - replay-live-boundary
related_components:
  - apps/nexus-service
---

# SSE fetch-stream consumer on BrowserClient (and the inferred replay/live boundary)

## Context

Nexus serves daemon SSE surfaces (`text/event-stream`) that require an `X-API-Key` request header and answer refusals as typed JSON before streaming. The shipped consumption pattern is a fetch-stream async generator on `BrowserClient` (`subscribeAgentHostEvents`, then `subscribeWorkflowEvents` for `/v1/daemon/orchestration/sessions/{id}/events`), not browser `EventSource`. The workflow-observation variant froze its client-side frame union and recovery rules in `.mstar/iterations/v1.201/specs/p1-run-observation-consumption.md` (promoted here).

## Guidance

- **Use a fetch-stream reader, not `EventSource`**, for any daemon SSE surface that (a) authenticates via request header, (b) returns typed JSON refusals (404/400/503) before streaming, (c) ends streams deliberately at terminal (auto-reconnect would resurrect a finished run), or (d) emits data-less named control frames (`event: gap` with no `data:`). `EventSource` fails all four: no custom headers, opaque `onerror`, uncontrollable reconnect, data-only parsing.
- **Parse `{id, event, data}` triples**, not data-only frames — control frames (`gap`, `history_unavailable`) may carry no `data:` and no `id:`; a data-only parser silently drops them. Reuse the chunk-boundary splitter but enumerate ALL blank-line delimiters: `\n\n`, `\r\n\r\n`, `\r\n\n`, **and `\n\r\n`** (mixed endings across chunk splits are real).
- **Never fabricate cursors**: a data frame without `id:` is a contract violation — fail with a typed error (`invalid_response`); an empty-string id would be persisted and resent as if received. Control frames legitimately carry no id and never advance the resume cursor.
- **Abort must stop buffered yields**: after the first `yield` a consumer may abort; re-check `signal.aborted` inside the buffered-frame drain loop, not only at `reader.read()`.
- **Replay/live boundary is inferred from attach order** — the frozen wire carries no handoff marker; the server writes the replay batch at subscription open before the live tail (pull gate blocks until new events). Anchor the catch-up window at the FIRST RECEIVED frame (never at subscribe time), and bound it by hard totals: first quiet gap (120 ms) OR 1 s since first frame (armed once, never re-armed per frame) OR N frames (64) — whichever first. Disclose in the UI that the split is inferred ("replay/live split is inferred from attach order"), not server-reported provenance.
- **Reconnect budget is strictly monotonic**: every completed attempt charges +1 (hard total; ~4 attempts, exponential backoff); no stability carve-outs — a "productive" attempt that then fails must still consume budget, or data+gap/throw churn reconnects forever. Only an explicit user retry re-arms the budget. A clean EOF under a non-terminal durable status is a transport failure (reconnect from cursor); terminal status or `history_unavailable` ends the lifecycle.
- **Bound retained state**: evict terminal runs; per-run history capped at the server ring window (256); publish a monotonic revision (advancing on control frames too) as the view memoization key instead of array identity/length.
- **Honest loss disclosure**: when a resume cursor names a prior epoch/evicted ring, the server answers `history_unavailable` as the sole frame of that subscription — render an explicit "server history unavailable, showing locally retained frames" notice; never present it as an ordinary stream end over stale frames.

## Why This Matters

`EventSource` looks like the platform-blessed choice and passes a happy-path demo; every failure above surfaced only under adversarial review (dropped control frames, auto-resubscribe after terminal end, unrefreshable auth) or adversarial wire shapes (mixed line endings, oversized/id-less frames). The inferred boundary is the honest maximum without a wire change — asserting exact provenance from attach order was rejected in QC tri (v1.201) as unverifiable.

## When to Apply

Any new `/v1/daemon/*` SSE surface consumed by the SPA (workflow observation shipped v1.201 P1; agent-host events already on this pattern; future canvas live-run would join it). Also applies to diagnosing "events missing/duplicated/reordered" reports on existing SSE surfaces.

## Examples

- `apps/web/src/lib/nexus/browser-client.ts` — `subscribeWorkflowEvents` + triple parser beside the agent-host consumer.
- `apps/web/src/api/run-observation.ts` — `useRunObservation` state machine (replaying/live/gapped/terminal/error, `liveFrom` derived boundary, monotonic revision).
- Tests: `browser-client.test.ts` (injected `fetchImpl`), `run-observation.test.tsx` (stubbed AsyncIterable).
