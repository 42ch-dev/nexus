/**
 * Same-run workflow observation transport (v1.195 P1-T3): the retained
 * `GET /v1/daemon/orchestration/sessions/{run_id}/events` identity over the
 * native subscription authority P1-T1 built.
 *
 * This module is a TRANSPORT, not an authority. It authorizes nothing itself:
 * `subscribeWorkflowEvents` resolves the run's stored root ownership in core
 * before any ring, epoch or cursor is consulted, so an absent, foreign or
 * child session id is already a typed JSON refusal before an SSE header could
 * be written. What this module owns is exactly three things:
 *
 * 1. the Tier-2 admission a family route inherits (composer/server), the
 *    `Last-Event-ID` header forwarded VERBATIM into the native request, and
 *    this identity's refusal of query parameters it does not serve;
 * 2. writing the frames the core already bounded and encoded, in ring order,
 *    with `id`/`event`/`data` untouched — the `<epoch>:<sequence>` cursor is
 *    the core's, so this transport never renumbers and never borrows the Host
 *    SSE vocabulary;
 * 3. ending the stream truthfully: a terminal/closed batch, an unresumable
 *    history, a slow reader, a disconnect and a pull fault all stop the loop,
 *    and every one of those paths releases the run's subscriber permit.
 *
 * There is deliberately no second replay buffer, no JS ring and no fabricated
 * frame: an explicit `gap` or `history_unavailable` frame is produced by the
 * run's own ring and forwarded as-is. A fault after the headers cannot become
 * a JSON error, so it terminates the stream instead — the caller's
 * `Last-Event-ID` still names the last frame it actually received.
 */
import type { ServerResponse } from 'node:http';
import type { CoreWorkflowEventBatch } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute, RouteRequestContext } from './routes.js';
import {
  NO_STREAM_END,
  SseWriter,
  clientDisconnected,
  gatePullUntilDrain,
  wireFrame,
} from './sse.js';
import { refuseUnknownQueryKeys, withPrincipal } from './world-kb.js';

/** The run-event control frames: no data frame, no terminal run state. */
const CONTROL_EVENTS: Record<string, true> = { gap: true, history_unavailable: true };

/**
 * Stream one root run's authorized events as SSE.
 *
 * The subscription is opened BEFORE `writeHead`, so the core's refusals still
 * reach the client as the retained JSON error envelope (404 for an
 * absent/foreign/child run, 400 for a malformed or future cursor, 503 for the
 * per-run subscriber cap). Every end below releases the token — the `close`
 * listener covers a disconnect that lands while a pull is blocked, and the
 * `finally` covers every other path.
 */
export async function streamWorkflowEvents(
  service: ServiceCore,
  runId: string,
  lastEventId: string | undefined,
  res: ServerResponse,
): Promise<void> {
  await withPrincipal(service, async (principal) => {
    const subscription = await service.core.subscribeWorkflowEvents(
      principal,
      lastEventId === undefined
        ? { run_id: runId }
        : { run_id: runId, last_event_id: lastEventId },
    );
    const subscriptionId = subscription.subscription_id;
    // The permit outlives no path: a released/foreign token is the only
    // answer a second release can get, so both callers ignore it.
    const release = () => {
      void service.core.releaseWorkflowEvents(principal, subscriptionId).catch(() => undefined);
    };
    res.once('close', release);
    // The run's ring can legitimately deliver MORE than one explicit `gap`
    // frame in one stream (a stored oversize-gap record plus the retention
    // gap), so this writer must not collapse control frames the way the Host
    // hub's single-slot model does.
    const writer = new SseWriter(res, NO_STREAM_END, { collapseControlFrames: false });
    const pullGate = { initialReleased: false };
    try {
      res.writeHead(200, {
        'Content-Type': 'text/event-stream; charset=utf-8',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
        'X-Accel-Buffering': 'no',
      });
      for (;;) {
        if (clientDisconnected(res)) return;
        // The drain gate is the backpressure boundary: the core's ring already
        // holds the frames, so a reader that has not caught up must not be
        // pulled ahead of. The pull itself blocks until a frame, a stream end
        // or the release above wakes it.
        if (!(await gatePullUntilDrain(res, pullGate, writer))) return;
        let batch: CoreWorkflowEventBatch;
        try {
          batch = await service.core.nextWorkflowEvents(principal, subscriptionId);
        } catch {
          return;
        }
        for (const event of batch.events) {
          const outcome = await writer.writeFrame(
            wireFrame(event.id, event.event, event.data, CONTROL_EVENTS[event.event] === true),
          );
          if (outcome !== 'ok') return;
        }
        if (batch.closed) return;
      }
    } finally {
      res.off('close', release);
      release();
      writer.end();
    }
  });
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const WORKFLOW_OBSERVATION_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/sessions\/([^/]+)\/events$/,
    tier: 'tier2',
    family: 'workflow_observation',
    handle: async (
      service: ServiceCore,
      params: string[],
      search: URLSearchParams,
      _body: unknown,
      request: RouteRequestContext,
    ) => {
      refuseUnknownQueryKeys(search, []);
      const runId = params[0];
      const lastEventId = request.lastEventId;
      return {
        kind: 'sse',
        run: (res: ServerResponse) => streamWorkflowEvents(service, runId, lastEventId, res),
      };
    },
  },
];
