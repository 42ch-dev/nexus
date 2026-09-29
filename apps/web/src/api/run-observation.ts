/**
 * Run observation hook (v1.201 P1-T2, audit DIR-03) — the client-side state
 * machine over T1's `NexusClient.subscribeWorkflowEvents`.
 *
 * Frozen contract: `.mstar/iterations/v1.201/specs/p1-run-observation-consumption.md`
 * §3 (resume/reconnect/terminal policy) and §4 (author-visible states). This
 * hook owns only *observation state*: it never renders and it never touches the
 * wire — the transport is `subscribeWorkflowEvents`, whose frames carry the
 * `<epoch>:<sequence>` cursor verbatim.
 *
 * Phases:
 *  - `connecting` — a subscription is in flight for the current attempt and no
 *    frame has been observed yet in this view (also the resting phase when no
 *    run is selected).
 *  - `replaying` — the same run's already-received frames were restored from
 *    the retained cursor/history cache and are on screen; the current
 *    subscription has not yet handed off to a new frame.
 *  - `live` — the subscription delivered a data frame: the live tail is
 *    attached, and frames keep appending without a manual refresh.
 *  - `gapped` — the stream reported an explicit gap (history the client never
 *    received). Never silently skipped: the gap marker stays in `events`, and
 *    the hook reconnects from the last received id; exhausted retries stay
 *    honestly `gapped` with the `retry` affordance.
 *  - `terminal` — the stream ended (a closed/terminal batch, or the
 *    `history_unavailable` close). No automatic resubscribe: an
 *    auto-reconnecting transport would loop the replay.
 *  - `error` — a typed non-200 refusal (absent/foreign/child run, malformed
 *    cursor, subscriber cap) or exhausted transport retries; `error` carries
 *    the reason and `retry` re-arms the subscription.
 *
 * Replay/live handoff: the frames present when the view mounts (restored from
 * the retained cache) are the replayed history; frames appended after the
 * phase reaches `live` are the live tail. The phase flip is the atomic handoff
 * boundary the run view marks — the hook never duplicates a cursor, so a
 * replayed frame and its live re-delivery collapse into one entry.
 *
 * Re-entry (`Reopening a session's run view`) subscribes with the retained
 * cursor, which the server answers with a strictly-after replay — the browser
 * never re-renders a blank view and never re-requests frames it already shows.
 * Unmount/navigation aborts the iterator, which releases the run's subscriber
 * permit server-side.
 *
 * `simplify:` retention is one in-memory entry per observed run for the SPA
 * session and is never evicted — a run's own ring is bounded (256 frames /
 * 1 MiB), so the retained copy is bounded too. Swap for an LRU or a
 * `sessionStorage` cap if the set of observed runs ever stops being small.
 */
import { useCallback, useEffect, useState } from 'react';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import type { WorkflowObservationFrame } from '@/lib/nexus/types';

/** Observation phase of one run's stream (see the module header). */
export type RunObservationPhase =
  | 'connecting'
  | 'replaying'
  | 'live'
  | 'gapped'
  | 'terminal'
  | 'error';

export interface RunObservationState {
  /** Current phase of the observed run's stream. */
  phase: RunObservationPhase;
  /**
   * Every frame observed for this run, in receive order, each cursor `id` at
   * most once. `gap`/`history_unavailable` control frames are kept inline so
   * the run view can render where continuity was lost rather than smoothing
   * over it.
   */
  events: WorkflowObservationFrame[];
  /** Last received `<epoch>:<sequence>` cursor, or `null` before any frame. */
  lastEventId: string | null;
  /** Why the stream is in `error` (typed refusal or exhausted retries). */
  error: Error | null;
}

export interface RunObservationResult extends RunObservationState {
  /** Re-arms the subscription from the retained cursor and clears `error`. */
  retry: () => void;
}

/** Consecutive unproductive attempts allowed per gap/transport failure. */
const MAX_RECONNECT_ATTEMPTS = 3;

/** Backoff before reconnect attempt N (index N-1); one entry per attempt. */
const RECONNECT_BACKOFF_MS = [200, 400, 800];

interface RetainedObservation {
  events: WorkflowObservationFrame[];
  seenIds: Set<string>;
  lastEventId: string | null;
}

const retainedObservations = new Map<string, RetainedObservation>();

function retainedFor(sessionId: string): RetainedObservation {
  const existing = retainedObservations.get(sessionId);
  if (existing) return existing;
  const created: RetainedObservation = {
    events: [],
    seenIds: new Set(),
    lastEventId: null,
  };
  retainedObservations.set(sessionId, created);
  return created;
}

/** A non-200 answer is a typed refusal: retrying cannot change it. */
function isRefusal(error: unknown): error is NexusClientError {
  return error instanceof NexusClientError && error.status >= 400;
}

/**
 * Observe one root run's retained/live event stream.
 *
 * `sessionId` is the run id the events route is addressed by (orchestration
 * session rows are root runs); pass `null` when no run is selected — nothing is
 * subscribed and the hook rests in `connecting`.
 *
 * Suited to a real `NexusClient` via `useNexusClient`; tests inject a stub
 * `AsyncIterable` client through `ClientProvider`.
 */
export function useRunObservation(sessionId: string | null): RunObservationResult {
  const client = useNexusClient();
  const [retryToken, setRetryToken] = useState(0);
  const [snapshot, setSnapshot] = useState<RunObservationState>({
    phase: 'connecting',
    events: [],
    lastEventId: null,
    error: null,
  });

  useEffect(() => {
    if (!sessionId) {
      setSnapshot({ phase: 'connecting', events: [], lastEventId: null, error: null });
      return;
    }

    const retained = retainedFor(sessionId);
    const controller = new AbortController();
    let cancelled = false;

    // Re-entry shows what this run already emitted instead of a blank view; the
    // first frame of the new subscription hands off to the live tail.
    setSnapshot({
      phase: retained.events.length > 0 ? 'replaying' : 'connecting',
      events: retained.events,
      lastEventId: retained.lastEventId,
      error: null,
    });

    void (async () => {
      let consecutiveFailures = 0;

      for (;;) {
        if (cancelled) return;

        /** Why the attempt ended: a closed batch, a gap, or a throw. */
        let outcome: 'end' | 'gap' | 'unavailable' | 'failed' = 'end';
        let failure: unknown;
        let receivedData = false;
        /** A gap with no data frame after it: our cursor is behind the ring. */
        let gappedAtClose = false;

        try {
          for await (const frame of client.subscribeWorkflowEvents(sessionId, {
            lastEventId: retained.lastEventId ?? undefined,
            signal: controller.signal,
          })) {
            if (cancelled) return;

            if (frame.kind === 'gap') {
              // The ring skipped a range we never received. Record the hole and
              // keep reading: a retention gap is followed by the frames the
              // ring still holds. If the stream closes on this gap, the attempt
              // below reconnects from the last id we actually received.
              gappedAtClose = true;
              retained.events = [...retained.events, frame];
              setSnapshot((prev) => ({ ...prev, phase: 'gapped', events: retained.events }));
              continue;
            }

            if (frame.kind === 'history_unavailable') {
              // The retained history is gone (restart/eviction). Reconnecting
              // would only loop this same close, so this ends in `terminal`.
              retained.events = [...retained.events, frame];
              outcome = 'unavailable';
              setSnapshot((prev) => ({ ...prev, events: retained.events }));
              continue;
            }

            receivedData = true;
            gappedAtClose = false;
            if (!retained.seenIds.has(frame.id)) {
              retained.seenIds.add(frame.id);
              retained.lastEventId = frame.id;
              retained.events = [...retained.events, frame];
            }
            setSnapshot({
              phase: 'live',
              events: retained.events,
              lastEventId: retained.lastEventId,
              error: null,
            });
          }

          if (gappedAtClose) outcome = 'gap';
        } catch (error) {
          if (cancelled) return;
          outcome = 'failed';
          failure = error;
        }

        if (cancelled) return;

        if (outcome === 'end' || outcome === 'unavailable') {
          setSnapshot((prev) => ({ ...prev, phase: 'terminal', error: null }));
          return;
        }

        if (isRefusal(failure)) {
          setSnapshot((prev) => ({ ...prev, phase: 'error', error: failure }));
          return;
        }

        // A productive attempt earns a fresh retry budget.
        consecutiveFailures = receivedData ? 1 : consecutiveFailures + 1;
        if (consecutiveFailures > MAX_RECONNECT_ATTEMPTS) {
          setSnapshot((prev) => ({
            ...prev,
            phase: outcome === 'gap' ? 'gapped' : 'error',
            error:
              outcome === 'gap'
                ? null
                : failure instanceof Error
                  ? failure
                  : new Error('Run observation stream failed'),
          }));
          return;
        }

        setSnapshot((prev) => ({
          ...prev,
          phase: outcome === 'gap' ? 'gapped' : 'connecting',
        }));

        // `Promise.withResolvers` is ES2024; this app compiles against the
        // ES2022 lib (apps/web/tsconfig.json), so the executor form stays.
        await new Promise<void>((resolve) => {
          setTimeout(resolve, RECONNECT_BACKOFF_MS[consecutiveFailures - 1]);
        });
      }
    })();

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [client, sessionId, retryToken]);

  const retry = useCallback(() => {
    setRetryToken((token) => token + 1);
  }, []);

  return { ...snapshot, retry };
}
