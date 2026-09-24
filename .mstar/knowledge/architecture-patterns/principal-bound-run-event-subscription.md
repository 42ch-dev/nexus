---
module: nexus-core (run event rings and subscriptions) + nexus-service (SSE transport)
date: 2026-09-24
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-22-v1.195-p1-workflow-observation
applies_when:
  - "Exposing a live event stream for one durable run to a browser/client, including late subscribe and reconnect"
  - "Deciding the cursor vocabulary for a reconnecting stream"
  - "A stream must stay honest when history was trimmed, evicted or lost to a restart"
  - "Reviewing an event or detail projection for substituted or synthesized fields"
related_components:
  - nexus-core
  - nexus-service
  - nexus-contracts
tags:
  - event-stream
  - sse
  - principal-binding
  - epoch-cursor
  - bounded-ring
  - explicit-gap
  - restart-honesty
status: active
---

# Principal-bound same-run event subscription — bind before lookup, gap explicitly, never fabricate

## Context

The retained workflow observation operations (same-run stream, bounded replay/reconnect, closed observation across restart) need one run's live events carried over HTTP. Two shortcuts are always available and both are wrong: reuse the **Host** stream (a different identity — a Host prompt or Host SSE is not the workflow run), or page the existing numeric-cursor read surface and call it a stream. A subscriber's cursor, a trimmed ring, a restart and a foreign creator all have to be answered honestly, including the answer "history is gone".

## Guidance

### 1. Authorize the stored owner before the ring or the cursor is touched

A subscription token carries (principal, core generation, root run) and every pull checks that binding **before** any ring or cursor lookup, on the stored owner (`orchestration_sessions` row of the principal's creator). Unknown run, foreign run and child session id then close with the same not-found answer — none of them can infer existence, epoch or payload from a stream or a page. A child run is never independently authorized by stripping `:child:` from a caller's string; the ring resolves the owning root, and child levels strip to that root.

### 2. Keep the two cursor vocabularies apart

| Surface | Cursor | Meaning |
|---|---|---|
| live subscription / SSE | `<epoch>:<sequence>` (UUID epoch + decimal) | resumption across a registry generation; opaque, never resequenced by the transport |
| durable paged read | numeric `after_sequence` | a bounded page over the same ring, a *different* operation |

The numeric page is not a substitute for the subscription and the subscription cursor must not be reinterpreted as a numeric one: an epoch-bearing cursor that a numeric-only request cannot carry is exactly how a reconnect silently drops or repeats frames. Both share the same bounded ring, so caps and gap rules cannot drift between them.

### 3. Bound every dimension, and refuse capacity before headers

Verified caps (per run unless noted): 256 records / 1 MiB per ring; 512 KiB maximum frame; 64 live + 64 terminal rings; 16 subscribers per run; 16 pending frames + 1 MiB per subscriber with one reserved control slot; 16 frames / 1 MiB per pull; exactly **one outstanding pull** per subscription. Capacity exceeded is a typed busy answer **before** SSE headers, never an unbounded allocation and never a silently dropped subscriber.

### 4. Gaps are explicit; fabrication is forbidden

- trimmed ring / skipped cursor → an explicit `gap` frame carrying run, epoch, from, to;
- prior epoch or evicted ring after a restart → exactly one `history_unavailable` control frame with the same run id and the inspect URL, then close — no invented historical events;
- malformed or future cursor → typed invalid-input **before** SSE headers;
- retained frames and the gap range must stay **disjoint**: a replay that both delivers and gaps the same sequence is a false green, not a conservative answer.

### 5. The transport forwards frames; it does not own them

Auth completes before any header is written. The service writes `id`/`event`/`data` verbatim, awaits drain, stops pulling on disconnect and **always releases** the subscription — including error and close paths. Release wakes a blocked pull with `closed` instead of parking it, and core close releases every subscription the owner minted, so no token outlives its generation. There is no second JS replay buffer.

### 6. Project only what the run actually recorded

When a client needs a durable result from the same run (for example the workspace-commit revision behind an authorized effect), project it from the **checkpointed successful capability output** already in that root run's durable context, after root/Creator authorization. An absent, malformed, failed, uncheckpointed or foreign result yields **no** value — never a substituted one. Substituting a different state counter (or minting an SSE frame the run never emitted) converts a missing receipt into a confident wrong answer.

### 7. Restart re-classifies, it does not replay

On restart: workspace/durable intent recovery runs first; completed, cancelled and failed runs classify as terminal; an interrupted external prompt attempt is **not** automatically runnable; a human wait is preserved; only the existing safe re-drive classes converge/merge. Inspect and session detail stay readable even when the event ring is gone (that is what `history_unavailable` is for).

## Why This Matters

Streaming lies are the most expensive kind: a renumbered frame looks like a valid event, a fabricated history fill makes a lost window invisible, and a stream authorized by the wrong identity leaks another creator's run. The bounded caps are also a denial-of-service boundary — an unbounded queue turns one slow reader into process-wide memory growth. Every rule here is a case where the *convenient* implementation passes a happy-path test and fails only under reconnect, restart, eviction or a foreign id — exactly the conditions the product names.

## When to Apply

- Adding or reviewing a live stream, subscription token, or reconnectable event surface for one durable run/session.
- Choosing or changing a cursor format; adding a "resume from here" path.
- Any detail projection that reports a result the write path recorded elsewhere.
- Reviewing restart/recovery classification for a run that may be mid-effect.

## Examples

### Before — the ring answers first, the identity is checked later

```ts
// route handler
const frames = await core.readRunEvents({ run_id });   // unknown/foreign run leaks existence
res.writeHead(200, sseHeaders);                        // headers before authorization
```

### After — bind, then look up, then stream

```ts
assertBound(subscription, principal, coreGeneration, rootRunId);  // before any lookup
const batch = await core.nextWorkflowEvents(subscriptionId);      // bounded pull, one in flight
for (const frame of batch.events) writeFrame(frame);             // id/event/data verbatim
if (batch.closed) { release(); res.end(); }                      // release on close, not only on error
```

## Evidence

- Rings, caps, epoch cursor, gap/history-unavailable frames, binding and release — `crates/nexus-core/src/execution/run_events.rs` (`MAX_RECORDS_PER_RUN`, `MAX_BYTES_PER_RUN`, `MAX_FRAME_BYTES`, `MAX_LIVE_RINGS`, `MAX_TERMINAL_RINGS`, `MAX_SUBSCRIBERS_PER_RUN`, `MAX_PENDING_FRAMES_PER_SUB`, `MAX_PENDING_BYTES_PER_SUB`, `MAX_PULL_FRAMES`, `HISTORY_UNAVAILABLE_EVENT`, `WorkflowSubscription::is_bound_to`, `subscribe_live`, `read_page`, `close_all`, `WorkflowSubscriptionRegistry`).
- Subscription API and the pre-lookup authorization on the paged read — `crates/nexus-core/src/execution/handle_ops.rs` (`subscribe_workflow_events`, `next_workflow_events`, `release_workflow_events`, `run_events`).
- Close ends every minted subscription — `crates/nexus-core/src/execution/lifecycle.rs` (`WorkflowRunCoordinator`/subscription registry `close_all` in the retained drain).
- Transport — `apps/nexus-service/src/workflow-observation.ts` (SSE route, `Last-Event-ID` pass-through, release on disconnect) with regressions in `apps/nexus-service/tests/workflow-observation-http.test.mjs`: exact cursor replay, disconnect releases the permit, late-live cancel closes, trimmed ring answers an explicit gap, stalled socket bounded by the drain deadline, restart answers history-unavailable, and unknown/foreign/child ids close before any SSE header.
- Ring-level regressions — `replay_dedupes_with_last_event_id`, `pre_restart_cursor_yields_history_unavailable_after_registry_restart`, `replay_after_eviction_emits_explicit_gap_for_skipped_cursor`, `draining_subscriber_never_false_gaps_under_wire_overhead`, `subscription_tokens_bind_their_identity_and_withdraw_on_release`, `pull_holds_back_the_frame_that_exceeds_the_batch_ceiling` (`crates/nexus-core/src/execution/run_events.rs`).
- Restart/effect truth and the authorized commit projection — `crates/nexus-core/src/execution/handle_ops.rs` (`get_workflow_session` detail projection) and `crates/nexus-core/src/execution/production.rs`/`workflow.rs` recovery ordering.

## Coverage gaps

- Literal in-flight `SIGKILL` during a stream and installed-deployment/live-model end-to-end observation were **not** qualified (development-layer verification only).
- The two-second drain deadline was exercised with a paused socket (`write(false)`), which proved frames are delivered once and the reader is bounded, but did not force the deadline expiry itself.
- Server-sent backpressure beyond the pending-frame/byte caps is enforced by the caps, not by an OS-level flow-control contract.
