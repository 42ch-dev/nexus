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
 *    received). The marker stays inline in `events`; the phase is *transient*
 *    while the stream is alive (frames that follow return it to
 *    `live`/`replaying` on the same subscription), and *resting* only when the
 *    stream ended at the gap or the bounded reconnect budget is spent — both
 *    with the `retry` affordance.
 *  - `terminal` — the lifecycle ended on a terminal statement: a
 *    `history_unavailable` close, or a clean stream end whose latest observed
 *    durable run status is terminal. No automatic resubscribe.
 *  - `error` — a typed non-200 refusal (absent/foreign/child run, malformed
 *    cursor, subscriber cap) or exhausted transport retries; `error` carries
 *    the reason and `retry` re-arms the subscription.
 *
 * Replay/live boundary (`liveFrom`): the wire carries no replay/live marker —
 * the service forwards the run ring's frames verbatim, so no frame says whether
 * the server replayed it or emitted it live. The split this hook renders is
 * therefore *inferred from attach ordering*, not observed provenance, and the
 * run view discloses that inference: the restored history plus the server's
 * strictly-after replay arrive at the head of the stream, and the live tail is
 * what follows. The catch-up window starts at the attempt's FIRST RECEIVED frame
 * — never before the stream connects, or a slow connect would freeze the
 * boundary ahead of the very replay it is waiting for — and it ends at whichever
 * comes first: a `CATCH_UP_SETTLE_MS` quiet gap, `CATCH_UP_MAX_MS` since that
 * first frame, or `CATCH_UP_MAX_FRAMES` frames received. Only the quiet gap is
 * re-armed per frame; the two totals are hard, so an actively streaming run
 * always hands off. `liveFrom` is the index of the first live-tail
 * event: `events.slice(0, liveFrom)` is replayed history, the rest is live. It
 * re-arms at the end of the window while the catch-up is open and freezes at the
 * handoff — and it is *derived* from an eviction-stable anchor rather than stored
 * as an index (see `boundaryIndex`), because the retained window slides.
 *
 * `gap` handling: the server writes a `gap` control frame and then keeps
 * writing the frames its ring still holds, on the SAME stream — the gap record
 * is one entry of that ring, forwarded in order by `workflow-observation.ts`,
 * which then blocks for the next batch. The hook therefore does NOT abort on a
 * gap: it records the marker inline, rests in `gapped`, and keeps consuming, so
 * the retained frames that follow land where they belong and the stream's own
 * live tail is never thrown away. Recovery is considered only when the stream
 * ENDS while still gapped, and only if that stream moved PAST the gap — its
 * last received DATA cursor is strictly newer than the cursor at the gap report
 * — because reconnecting from the gap's own cursor asks the ring for exactly
 * the range whose gap it just reported. A stream that ends at the gap instead
 * stays honestly `gapped` with the `retry` affordance and charges no attempt.
 *
 * Stream end: only a terminal statement ends the lifecycle. A clean transport
 * EOF while the latest observed run status is non-terminal (or unknown) is a
 * transport failure — the server can also close an active stream on a pull
 * fault after the SSE headers — and reconnects from the cursor under the same
 * bounded policy (a gapped stream that moved past its gap takes this same
 * path). The reconnect budget is strictly monotonic: every completed
 * attempt charges it once, no outcome resets it, and only the user's explicit
 * `retry()` re-arms it — so neither a server that keeps closing nor a ring that
 * keeps outrunning the observer can turn the hook into a reconnect loop.
 *
 * Retention: one entry per observed run, keyed by the CONNECTION it was
 * observed over (the client instance) as well as the run id, and bounded three
 * ways. The connection half of the key is the point: a rebuilt client — a
 * daemon/client reconnect, a changed endpoint or key — is a different
 * connection, and the frames a previous connection retained must never be shown
 * under it before that connection's own subscription has been authorized.
 * A different connection therefore starts cold (cursorless subscribe, no
 * restored history). (1) An entry is dropped as soon as the run reaches its
 * terminal lifecycle end — a finished run is re-observable from the server's own
 * ring instead of being pinned in the SPA. (2) A non-terminal entry is dropped
 * once its last view has unmounted and it has been idle past
 * `RETAINED_IDLE_TTL_MS`; re-entry after that is a fresh subscription with no
 * cursor — an honest re-fetch, the contract's re-entry path taken from scratch.
 * (3) Each entry keeps at most `MAX_RETAINED_EVENTS` frames
 * — the server ring's own 256-frame window — and a single monotonic cursor
 * instead of a growing id set, so neither the frame list nor the cursor store
 * grows without bound over a long-lived run. `events` is that retained buffer
 * itself, mutated in place and re-published per frame (no per-frame copy of the
 * whole history); memoize on the published `revision` — it advances on every
 * retained frame, control frames included, unlike `events.length` or
 * `lastEventId` — rather than on the array identity.
 */
import { useCallback, useEffect, useState } from 'react';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError, type NexusClient } from '@/lib/nexus';
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
   * directly, and memoize on `revision` — never on the array identity,
   * `events.length` or `lastEventId`, none of which change when a control frame
   * is appended to a full window.
   */
  events: WorkflowObservationFrame[];
  /**
   * Monotonic publish revision: the number of frames retained for this run over
   * the current observation's lifetime. Every retained frame — a data frame or
   * an inline `gap`/`history_unavailable` control frame — advances it, so it is
   * the safe memoization key for a consumer deriving from `events`.
   */
  revision: number;
  /**
   * Index into `events` of the first live-tail frame: `events.slice(0, liveFrom)`
   * is replayed history (the re-entered span plus the server's strictly-after
   * replay), `events.slice(liveFrom)` arrived once the view had caught up.
   *
   * Derived per publish from the entry's monotonic append count, so it moves
   * with the sliding window instead of stranding the live tail off the end.
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

/**
 * Idle lifetime of a non-terminal run's retained observation once no view is
 * mounted on it. Five minutes matches React Query's default `gcTime`, which this
 * app leaves at its default (`main.tsx` configures only `staleTime`/`retry`), so
 * warm run-view retention expires on the same clock as every other cached
 * resource: long enough for the navigate-away-and-back a run view invites, short
 * enough that abandoned runs cannot pin their window for the SPA session.
 */
const RETAINED_IDLE_TTL_MS = 5 * 60_000;

/**
 * Quiet period, measured from the attempt's first received frame, that ends the
 * catch-up burst and hands off to the live tail.
 */
const CATCH_UP_SETTLE_MS = 120;

/** Ceiling on the catch-up window, so an actively streaming run still hands off. */
const CATCH_UP_MAX_MS = 1_000;

/**
 * Frame ceiling on the catch-up window. A burst arriving faster than
 * `CATCH_UP_SETTLE_MS` never goes quiet, and on a fast enough stream can still
 * outrun `CATCH_UP_MAX_MS`; the first frame past this total is the live tail.
 */
const CATCH_UP_MAX_FRAMES = 64;

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
  /** Frames appended over this entry's lifetime — the window slides, this does not. */
  appended: number;
  /** `appended` count at which the live tail begins (0 = every retained frame is live). */
  liveFromAppended: number;
  /** Views currently mounted on this run. */
  subscribers: number;
  /** `Date.now()` of the last retained frame, for the idle sweep. */
  lastActivity: number;
  /** Idle-eviction timer, armed when the last view unmounts. */
  releaseTimer: ReturnType<typeof setTimeout> | undefined;
}

const retainedObservations = new Map<number, Map<string, RetainedObservation>>();

/**
 * Connection identity of the client a view observes over. A rebuilt client
 * (daemon/client reconnect, changed endpoint or key) is a new instance and so a
 * new identity; the identity is a monotonic number rather than the instance
 * itself, and the WeakMap never pins a retired client alive. Identities are
 * never reused, so a swept connection's entries can never be read back by a
 * later one.
 */
const connectionIdentities = new WeakMap<NexusClient, number>();
let latestConnectionIdentity = 0;

function connectionIdentityOf(client: NexusClient): number {
  const existing = connectionIdentities.get(client);
  if (existing !== undefined) return existing;
  latestConnectionIdentity += 1;
  connectionIdentities.set(client, latestConnectionIdentity);
  return latestConnectionIdentity;
}

/** This connection's run-id → observation map, created on first use. */
function observationsFor(identity: number): Map<string, RetainedObservation> {
  const existing = retainedObservations.get(identity);
  if (existing) return existing;
  const created = new Map<string, RetainedObservation>();
  retainedObservations.set(identity, created);
  return created;
}

/** Forget one run's observation, dropping the connection entry when it empties. */
function dropRetained(identity: number, sessionId: string): void {
  const bySession = retainedObservations.get(identity);
  if (!bySession) return;
  bySession.delete(sessionId);
  if (bySession.size === 0) retainedObservations.delete(identity);
}

/**
 * Drop entries no view is observing whose last activity is older than
 * `RETAINED_IDLE_TTL_MS`. Terminal runs are dropped immediately by the hook;
 * this is the bound for the non-terminal runs a user has navigated away from.
 * An entry with a mounted view is never swept: an observed run may legitimately
 * produce nothing for longer than the TTL (the server's pull gate blocks until
 * the next event).
 */
function sweepIdleObservations(now: number): void {
  for (const [identity, bySession] of retainedObservations) {
    for (const [sessionId, entry] of bySession) {
      if (entry.subscribers > 0) continue;
      if (now - entry.lastActivity < RETAINED_IDLE_TTL_MS) continue;
      clearTimeout(entry.releaseTimer);
      bySession.delete(sessionId);
    }
    if (bySession.size === 0) retainedObservations.delete(identity);
  }
}

function retainedFor(identity: number, sessionId: string): RetainedObservation {
  sweepIdleObservations(Date.now());
  const bySession = observationsFor(identity);
  const existing = bySession.get(sessionId);
  if (existing) return existing;
  const created: RetainedObservation = {
    events: [],
    lastEventId: null,
    latestStatus: null,
    appended: 0,
    liveFromAppended: 0,
    subscribers: 0,
    lastActivity: Date.now(),
    releaseTimer: undefined,
  };
  bySession.set(sessionId, created);
  return created;
}

/** Append one frame, keeping the retained window and the activity clock current. */
function retain(retained: RetainedObservation, frame: WorkflowObservationFrame): void {
  retained.appended += 1;
  retained.lastActivity = Date.now();
  retained.events.push(frame);
  const overflow = retained.events.length - MAX_RETAINED_EVENTS;
  if (overflow > 0) retained.events.splice(0, overflow);
}

/**
 * Index into `events` of the first live-tail frame — the `liveFrom` the run view
 * renders from.
 *
 * The boundary is *stored* as an append count
 * (`RetainedObservation.liveFromAppended`), never as an index: the retained
 * window slides (`retain` splices the oldest frame off a full window), so a
 * stored index would keep pointing past the end of the window and hide the live
 * tail — a full window would report `liveFrom === events.length` forever, even
 * while the phase is `live`. Deriving the index from the monotonic append count
 * keeps the boundary anchored to the same frame as the window moves; once that
 * frame is itself evicted, every retained frame is live and the boundary is 0.
 */
function boundaryIndex(retained: RetainedObservation): number {
  const evicted = retained.appended - retained.events.length;
  return Math.max(0, retained.liveFromAppended - evicted);
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
 * True when `cursor` is strictly newer than `previous` — the stream carried
 * data past the point `previous` named. A changing epoch is a different ring and
 * is never stale, and an unparsable pair falls back to plain inequality.
 */
function dataCursorAdvanced(cursor: string | null, previous: string | null): boolean {
  if (cursor === null) return false;
  if (previous === null) return true;
  const current = cursorParts(cursor);
  const last = cursorParts(previous);
  if (current === null || last === null) return cursor !== previous;
  if (current.epoch !== last.epoch) return true;
  return current.sequence > last.sequence;
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
    revision: 0,
    liveFrom: 0,
    lastEventId: null,
    error: null,
  });

  useEffect(() => {
    if (!sessionId) {
      setSnapshot({
        phase: 'connecting',
        events: [],
        revision: 0,
        liveFrom: 0,
        lastEventId: null,
        error: null,
      });
      return;
    }

    const connectionIdentity = connectionIdentityOf(client);
    const retained = retainedFor(connectionIdentity, sessionId);
    retained.subscribers += 1;
    retained.lastActivity = Date.now();
    clearTimeout(retained.releaseTimer);
    retained.releaseTimer = undefined;
    let cancelled = false;
    /** The subscription attempt in flight: aborted on unmount. */
    let attempt: AbortController | null = null;
    /** Re-entry catch-up: quiet timer re-armed per frame, hard totals armed once. */
    let settleTimer: ReturnType<typeof setTimeout> | undefined;
    let capTimer: ReturnType<typeof setTimeout> | undefined;
    /** Catch-up frames received — the hard frame total that ends the window. */
    let catchUpFrames = 0;

    // Re-entry restores what this run already emitted; a first view starts at
    // `connecting` and its frames are the live tail (there is no catch-up span).
    // A retained cursor IS the history: control frames alone are no catch-up.
    let phase: RunObservationPhase = retained.lastEventId !== null ? 'replaying' : 'connecting';
    let catchingUp = retained.lastEventId !== null;
    // A re-entry starts with the whole retained window on the replay side of the
    // boundary; each catch-up frame re-arms it at the end of the window and the
    // handoff freezes it there (see `boundaryIndex`).
    if (catchingUp) retained.liveFromAppended = retained.appended;

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
        revision: retained.appended,
        liveFrom: boundaryIndex(retained),
        lastEventId: retained.lastEventId,
        error,
      });
    };

    /** End the catch-up window: everything received so far is history, the rest is live. */
    const handOff = (): void => {
      clearCatchUp();
      if (cancelled || !catchingUp) return;
      catchingUp = false;
      if (phase === 'replaying') {
        phase = 'live';
        publish(null);
      }
    };

    /**
     * Arm/extend the catch-up window for one received frame. The quiet timer is
     * re-armed by every frame (a gap of `CATCH_UP_SETTLE_MS` hands off), while
     * the hard totals — `CATCH_UP_MAX_MS` since the first frame and
     * `CATCH_UP_MAX_FRAMES` received — are armed/charged once per catch-up and
     * are never re-armed by a later frame, so a busy stream cannot keep replaying
     * forever: whichever total is reached first hands off to the live tail
     * (QC3-002).
     */
    const armCatchUp = (): void => {
      clearTimeout(settleTimer);
      settleTimer = setTimeout(handOff, CATCH_UP_SETTLE_MS);
      capTimer ??= setTimeout(handOff, CATCH_UP_MAX_MS);
      catchUpFrames += 1;
      if (catchUpFrames >= CATCH_UP_MAX_FRAMES) handOff();
    };

    publish(null);

    void (async () => {
      let consecutiveFailures = 0;

      for (;;) {
        if (cancelled) return;

        /** Why the attempt ended: a closed batch, an unavailable history, or a throw. */
        let outcome: 'end' | 'unavailable' | 'failed' = 'end';
        let failure: unknown;
        /** This stream reported a gap, and the data cursor at that report. */
        let gapReported = false;
        let gapCursor: string | null = null;

        const controller = new AbortController();
        attempt = controller;

        try {
          for await (const frame of client.subscribeWorkflowEvents(sessionId, {
            lastEventId: retained.lastEventId ?? undefined,
            signal: controller.signal,
          })) {
            if (cancelled) return;

            if (frame.kind === 'gap') {
              // The ring skipped a range this view never received. Record the
              // hole inline and KEEP CONSUMING: the server writes the frames it
              // still holds after the gap on this same stream, so aborting here
              // would throw away the retained events and reconnect into the very
              // range whose gap was just reported. Recovery is decided at stream
              // end instead (below).
              retain(retained, frame);
              gapReported = true;
              gapCursor = retained.lastEventId;
              catchingUp = false;
              clearCatchUp();
              phase = 'gapped';
              publish(null);
              continue;
            }

            if (frame.kind === 'history_unavailable') {
              // The retained history is gone (restart/eviction). Reconnecting
              // would only loop this same close, so this ends in `terminal`.
              retain(retained, frame);
              outcome = 'unavailable';
              break;
            }

            // The catch-up burst is anchored at the first frame this attempt
            // received — a re-delivered duplicate counts — so an attempt that
            // connects late still keeps its replay on the replay side.
            if (catchingUp) armCatchUp();
            if (isReplayedCursor(frame.id, retained.lastEventId)) continue;

            retained.lastEventId = frame.id;
            if (frame.kind === 'run_state') retained.latestStatus = frame.payload.status;
            retain(retained, frame);

            // A frame arriving after a gap is what the ring retained past the
            // hole: the subscription is alive again, so the phase leaves
            // `gapped` while the marker itself stays inline in `events`.
            if (catchingUp) {
              retained.liveFromAppended = retained.appended;
              phase = 'replaying';
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
          clearTimeout(retained.releaseTimer);
          dropRetained(connectionIdentity, sessionId);
          phase = 'terminal';
          publish(null);
          return;
        }

        if (isRefusal(failure)) {
          phase = 'error';
          publish(failure);
          return;
        }

        if (outcome === 'end' && gapReported && !dataCursorAdvanced(retained.lastEventId, gapCursor)) {
          // The stream ended at the gap it reported: it never carried a data
          // frame past it, so `Last-Event-ID` still names exactly the range the
          // ring just gapped. Reconnecting would re-report the same gap and
          // burn the budget on it, so the view stays honestly `gapped` with the
          // `retry` affordance and no attempt is charged.
          phase = 'gapped';
          publish(null);
          return;
        }

        // Every completed attempt charges the reconnect budget exactly once. A
        // clean EOF under a non-terminal (or unknown) status is a transport
        // failure, not an ending — the same episode a gapped stream that did
        // move past its gap belongs to. The budget is strictly monotonic within
        // one subscription: a productive or long-lived attempt earns no extra
        // attempts (progress is reported by the attach boundary and the UI
        // phase, not the retry count), so nothing can reset the count mid-episode
        // and only the user's explicit `retry()` re-arms it by re-running the
        // effect (QC3-001).
        consecutiveFailures += 1;

        if (consecutiveFailures > MAX_RECONNECT_ATTEMPTS) {
          // Only a clean END that is still gapped rests on the gap affordance;
          // a throw keeps its own failure on screen instead of hiding it behind
          // the gap the stream happened to report first.
          const restingGapped = outcome === 'end' && gapReported;
          phase = restingGapped ? 'gapped' : 'error';
          publish(
            restingGapped
              ? null
              : failure instanceof Error
                ? failure
                : new Error('Run observation stream ended while the run was still running'),
          );
          return;
        }

        phase = 'connecting';
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
      if (retainedObservations.get(connectionIdentity)?.get(sessionId) !== retained) return;
      retained.subscribers -= 1;
      retained.lastActivity = Date.now();
      if (retained.subscribers > 0) return;
      // The last view left: drop the entry once it has been idle past the TTL,
      // unless something re-observes the run first (that mount clears the timer).
      // The timer arms exactly one TTL after the last activity, so it runs the
      // same predicate as the access-time sweep rather than a second rule.
      retained.releaseTimer = setTimeout(() => {
        sweepIdleObservations(Date.now());
      }, RETAINED_IDLE_TTL_MS);
    };
  }, [client, sessionId, retryToken]);

  const retry = useCallback(() => {
    setRetryToken((token) => token + 1);
  }, []);

  return { ...snapshot, retry };
}
