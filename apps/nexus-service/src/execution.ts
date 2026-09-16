/**
 * Execution family HTTP surface (P5-T3): the durable schedule mutations
 * (add/signal) over the P3 `ExecutionHandle` authority. The handle is the
 * single execution owner established on the native side — this surface never
 * starts a second scheduler and never owns domain terminal truth.
 *
 * Deliberately NOT routed on this surface (no core authority exists; the
 * legacy daemon handlers are the only producers): orchestration session
 * CRUD/list (still the embedded host's own state, mirrored through
 * `hostQuery` where retained), schedule list/inspect/delete and
 * core-context/history, and the compute run family (the WASM edge is a
 * daemon-cohort capability). Those identities keep their truthful
 * `route_not_migrated` refusal rather than a degraded fake.
 */
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { withPrincipal, wirePayload } from './world-kb.js';

/** `POST /v1/daemon/orchestration/schedules` — add a durable schedule. */
export function addSchedule(service: ServiceCore, body: unknown) {
  return withPrincipal(service, (principal) =>
    service.core.addSchedule(principal, wirePayload(body, 'request')),
  );
}

/** `POST /v1/daemon/orchestration/schedules/{schedule_id}/signal`. */
export function signalSchedule(service: ServiceCore, scheduleId: string, body: unknown) {
  return withPrincipal(service, (principal) =>
    service.core.signalSchedule(principal, scheduleId, wirePayload(body, 'request')),
  );
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const EXECUTION_ROUTES: readonly DomainRoute[] = [
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/orchestration\/schedules$/,
    tier: 'tier2',
    family: 'execution',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await addSchedule(service, body),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/orchestration\/schedules\/([^/]+)\/signal$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, params, _search, body) => ({
      body: await signalSchedule(service, params[0], body),
    }),
  },
];
