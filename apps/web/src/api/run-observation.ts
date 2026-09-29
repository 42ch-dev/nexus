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
 *  - `connecting` — a subscription attempt is in flight and no frame has been
 *    observed yet (also the resting phase when no run is selected).
 *  - `replaying` — history is on screen that has not been confirmed as the live
 *    tail: the re-entered view's restored frames, plus the strictly-after replay
 *    the server delivers when the view reattaches. See the boundary note below.
 *  - `live` — the view has caught up; frames append without a manual refresh.
 *  - `gapped` — the stream reported an explicit gap (history this view never
 *    received). The marker stays inline in `events` and the hook recovers by
 *    resubscribing from the last received id; exhausted retries stay honestly
 *    `gapped` with the `retry` affordance.
 *  - `terminal` — the lifecycle ended on a terminal statement: a
 *    `history_unavailable` close, or a clean stream end whose latest observed
 *    durable run status is terminal. No automatic resubscribe.
 *  - `error` — a typed non-200 refusal (absent/foreign/child run, malformed
 *    cursor, subscriber cap) or exhausted transport retries; `error` carries
 *    the reason and `retry` re-arms the subscription.
 *
 * Replay/live boundary (`liveFrom`): the wire carries no replay/live marker —
 * the service forwards the run ring's frames verbatim and the retained
 * contract's "atomic replay/live handoff" is a server-side guarantee. The one
 * boundary the client can honor is therefore its own attach boundary: on
 * re-entry the restored history and the server's strictly-after replay are the
 * catch-up, and the live tail begins once the stream is observed to have come
 * to rest (the server writes the replay immediately after the response headers
 * and then blocks on its pull gate). `liveFrom` is the index of the first live
 * event: `events.slice(0, liveFrom)` is replayed history, the rest is live. It
 * advances while the catch-up window is open and freezes at the handoff.
 *
 * `gap` handling: a gap ends the current subscription immediately (`abort`) and
 * the hook resubscribes from the last cursor it actually received under the
 * bounded retry policy. The server delivers a gap and then keeps the stream
 * open for the live tail (`workflow-observation.ts`), so waiting for the
 * iterator to end would stall recovery behind the very stream the gap
 * invalidated.
 *
 * Stream end: only a terminal statement ends the lifecycle. A clean transport
 * EOF while the latest observed run status is non-terminal (or unknown) is a
 * transport failure — the server can also close an active stream on a pull
 * fault after the SSE headers — and reconnects from the cursor under the same
 * bounded policy. A clean EOF never earns a fresh retry budget, so a server
 * that keeps closing cannot turn the hook into a reconnect loop.
 *
 * Retention: one entry per observed run, dropped as soon as the run reaches its
 * terminal lifecycle end (a finished run is re-observable from the server's own
 * ring instead of being pinned in the SPA). Each entry keeps at most
 * `MAX_RETAINED_EVENTS` frames — the server ring's own 256-frame window — and a
 * single monotonic cursor instead of a growing id set, so neither the frame
 * list nor the cursor store grows without bound over a long-lived run.
 * `events` is that retained buffer itself, mutated in place and re-published
 * per frame (no per-frame copy of the whole history); memoize on
 * `events.length` / `lastEventId` rather than on the array identity.
 *
 * `simplify:` the module map still keeps every run that has not ended yet; swap
 * for an LRU if the set of simultaneously-observed runs ever stops being small.
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
   * Every frame observed for this run, in ring order, each cursor `id` at most
   * once. `gap`/`history_unavailable` control frames are kept inline so the run
   * view can render where continuity was lost rather than smoothing over it.
   *
   * This is the hook's bounded retained buffer, re-published per frame: read it
   * directly, and memoize on `events.length` / `lastEventId` rather than on the
   * array identity.
   */
  events: WorkflowObservationFrame[];
  /**
   * Index into `events` of the first live-tail frame: `events.slice(0, liveFrom)`
   * is replayed history (the re-entered span plus the server's strictly-after
   * replay), `events.slice(liveFrom)` arrived once the view had caught up.
   */
  liveFrom: number;
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

/** Frames retained per run — the server ring's own window (`daemon-runtime` §20.2). */
const MAX_RETAINED_EVENTS = 256;

/** Quiet period that ends the view's catch-up replay and hands off to the live tail. */
const CATCH_UP_SETTLE_MS = 120;

/** Ceiling on the catch-up window, so an actively streaming run still hands off. */
const CATCH_UP_MAX_MS = 1_000;

/**
 * Durable run statuses that end the lifecycle — the core's
 * `SessionStatus::is_terminal` (`crates/nexus-orchestration/src/engine.rs`).
 * Every other status (and an unknown one) keeps the run's stream alive, so a
 * clean EOF under it is a transport failure, not a terminal state.
 */
const TERMINAL_RUN_STATUS: Record<string, true> = {
  completed: true,
  failed: true,
  cancelled: true,
  interrupted: true,
};

interface RetainedObservation {
  events: WorkflowObservationFrame[];
  lastEventId: string | null;
  /** Durable status of the latest `run_state` frame, or `null` if none seen. */
  latestStatus: string | null;
}

const retainedObservations = new Map<string, RetainedObservation>();

function retainedFor(sessionId: string): RetainedObservation {
  const existing = retainedObservations.get(sessionId);
  if (existing) return existing;
  const created: RetainedObservation = { events: [], lastEventId: null, latestStatus: null };
  retainedObservations.set(sessionId, created);
  return created;
}

/** Append one frame, keeping the retained window bounded. */
function retain(retained: RetainedObservation, frame: WorkflowObservationFrame): void {
  retained.events.push(frame);
  const overflow = retained.events.length - MAX_RETAINED_EVENTS;
  if (overflow > 0) retained.events.splice(0, overflow);
}

/** The `<epoch>:<sequence>` halves of a cursor, or `null` when it is not one. */
function cursorParts(cursor: string): { epoch: string; sequence: number } | null {
  const at = cursor.indexOf(':');
  if (at <= 0) return null;
  const sequence = Number(cursor.slice(at + 1));
  return Number.isSafeInteger(sequence) ? { epoch: cursor.slice(0, at), sequence } : null;
}

/**
 * True when `id` names a cursor this view already holds (or one older than it)
 * — a re-delivered frame, which must never be appended twice. The server's
 * `Last-Event-ID` resume is strictly-after, with the sent cursor itself the
 * only inclusive redelivery, so a monotonic per-epoch comparison IS the
 * deduplication; a changing epoch is a different ring and is never stale.
 */
function isReplayedCursor(id: string, previous: string | null): boolean {
  if (previous === null) return false;
  if (id === previous) return true;
  const current = cursorParts(id);
  const last = cursorParts(previous);
  if (current === null || last === null) return false;
  return current.epoch === last.epoch && current.sequence <= last.sequence;
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
    liveFrom: 0,
    lastEventId: null,
    error: null,
  });

  useEffect(() => {
    if (!sessionId) {
      setSnapshot({ phase: 'connecting', events: [], liveFrom: 0, lastEventId: null, error: null });
      return;
    }

    const retained = retainedFor(sessionId);
    let cancelled = false;
    /** The subscription attempt in flight: aborted at a gap or on unmount. */
    let attempt: AbortController | null = null;
    /** Re-entry catch-up window: re-armed per catch-up frame, capped in total. */
    let settleTimer: ReturnType<typeof setTimeout> | undefined;
    let capTimer: ReturnType<typeof setTimeout> | undefined;

    // Re-entry restores what this run already emitted; a first view starts at
    // `connecting` and its frames are the live tail (there is no catch-up span).
    // A retained cursor IS the history: control frames alone are no catch-up.
    let phase: RunObservationPhase = retained.lastEventId !== null ? 'replaying' : 'connecting';
    let liveFrom = retained.events.length;
    let catchingUp = retained.lastEventId !== null;

    const clearCatchUp = (): void => {
      clearTimeout(settleTimer);
      clearTimeout(capTimer);
      settleTimer = undefined;
      capTimer = undefined;
    };

    const publish = (error: Error | null): void => {
      setSnapshot({
        phase,
        events: retained.events,
        liveFrom,
        lastEventId: retained.lastEventId,
        error,
      });
    };

    /** End the catch-up window: everything received so far is history, the rest is live. */
    const handOff = (): void => {
      clearCatchUp();
      if (cancelled || !catchingUp) return;
      catchingUp = false;
      liveFrom = retained.events.length;
      if (phase === 'replaying') {
        phase = 'live';
        publish(null);
      }
    };

    const armCatchUp = (): void => {
      clearTimeout(settleTimer);
      settleTimer = setTimeout(handOff, CATCH_UP_SETTLE_MS);
      capTimer ??= setTimeout(handOff, CATCH_UP_MAX_MS);
    };

    publish(null);

    void (async () => {
      let consecutiveFailures = 0;

      for (;;) {
        if (cancelled) return;

        /** Why the attempt ended: a closed batch, a gap, or a throw. */
        let outcome: 'end' | 'gap' | 'unavailable' | 'failed' = 'end';
        let failure: unknown;
        let receivedData = false;

        const controller = new AbortController();
        attempt = controller;
        if (catchingUp) armCatchUp();

        try {
          for await (const frame of client.subscribeWorkflowEvents(sessionId, {
            lastEventId: retained.lastEventId ?? undefined,
            signal: controller.signal,
          })) {
            if (cancelled) return;

            if (frame.kind === 'gap') {
              // The ring skipped a range this view never received. Record the
              // hole inline, end this subscription now, and recover by
              // resubscribing from the last cursor actually received (§3) —
              // the server follows a gap with the frames it still holds and
              // then keeps the stream open, so waiting for the iterator to end
              // would stall recovery behind the live tail.
              retain(retained, frame);
              outcome = 'gap';
              catchingUp = false;
              clearCatchUp();
              phase = 'gapped';
              publish(null);
              controller.abort();
              break;
            }

            if (frame.kind === 'history_unavailable') {
              // The retained history is gone (restart/eviction). Reconnecting
              // would only loop this same close, so this ends in `terminal`.
              retain(retained, frame);
              outcome = 'unavailable';
              break;
            }

            receivedData = true;
            if (isReplayedCursor(frame.id, retained.lastEventId)) continue;

            retained.lastEventId = frame.id;
            if (frame.kind === 'run_state') retained.latestStatus = frame.payload.status;
            retain(retained, frame);

            if (catchingUp) {
              liveFrom = retained.events.length;
              phase = 'replaying';
              armCatchUp();
            } else {
              phase = 'live';
            }
            publish(null);
          }
        } catch (error) {
          if (cancelled) return;
          outcome = 'failed';
          failure = error;
        }

        attempt = null;
        clearCatchUp();
        if (cancelled) return;

        if (
          outcome === 'unavailable' ||
          (outcome === 'end' && retained.latestStatus !== null
            && TERMINAL_RUN_STATUS[retained.latestStatus] === true)
        ) {
          // A terminal statement: `history_unavailable`, or a closed batch whose
          // latest durable status is terminal. The lifecycle is over and the run
          // is dropped from the retention map — a finished run is re-observable
          // from the server's own ring instead of being pinned for the session.
          retainedObservations.delete(sessionId);
          phase = 'terminal';
          publish(null);
          return;
        }

        if (isRefusal(failure)) {
          phase = 'error';
          publish(failure);
          return;
        }

        if (outcome === 'end') {
          // A clean EOF under a non-terminal (or unknown) status is a transport
          // failure, not an ending. It never earns a fresh budget: a server that
          // keeps closing must not turn this into a reconnect loop.
          consecutiveFailures += 1;
        } else {
          // A productive attempt earns a fresh retry budget.
          consecutiveFailures = receivedData ? 1 : consecutiveFailures + 1;
        }

        if (consecutiveFailures > MAX_RECONNECT_ATTEMPTS) {
          phase = outcome === 'gap' ? 'gapped' : 'error';
          publish(
            outcome === 'gap'
              ? null
              : failure instanceof Error
                ? failure
                : new Error('Run observation stream ended while the run was still running'),
          );
          return;
        }

        phase = outcome === 'gap' ? 'gapped' : 'connecting';
        publish(null);

        // `Promise.withResolvers` is ES2024; this app compiles against the
        // ES2022 lib (apps/web/tsconfig.json), so the executor form stays.
        await new Promise<void>((resolve) => {
          setTimeout(resolve, RECONNECT_BACKOFF_MS[consecutiveFailures - 1]);
        });
      }
    })();

    return () => {
      cancelled = true;
      clearCatchUp();
      attempt?.abort();
    };
  }, [client, sessionId, retryToken]);

  const retry = useCallback(() => {
    setRetryToken((token) => token + 1);
  }, []);

  return { ...snapshot, retry };
}
