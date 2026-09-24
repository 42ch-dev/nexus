/**
 * Execution family HTTP surface (P5-T3 + v1.195 P0-T6): the durable schedule
 * control journey over the P3 `ExecutionHandle` authority — add/signal, the
 * durable schedule list/inspect reads, the durable run list/detail reads and
 * the core-context edit. The handle is the single execution owner established
 * on the native side: this surface never starts a second scheduler, never
 * reads SQL itself and never owns domain terminal truth.
 *
 * Query forwarding is the schema's, not the transport's: the generated DTO
 * travels to the native decoder, which is where an unknown key or a bad value
 * becomes a typed refusal. `limit` is the one key that needs a transport-side
 * parse (the JSON wire carries a number, not a string), and the transport
 * only checks that it IS an integer — the core owner applies the documented
 * default/cap, so the two surfaces cannot disagree about a page size.
 *
 * Deliberately NOT routed on this surface (no core authority exists): the
 * core-context HISTORY reads and the schedule label/delete mutations. Those
 * identities keep their truthful `route_not_migrated` refusal rather than a
 * degraded fake. The Compute run family is a family of its own (`compute.ts`)
 * over the same hosted owner, not part of this surface.
 */
import type {
  ListSchedulesQuery,
  ListSessionsQuery,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import {
  parseOptionalInteger,
  refuseUnknownQueryKeys,
  withPrincipal,
  wirePayload,
} from './world-kb.js';

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

/** `GET /v1/daemon/orchestration/schedules` — the Creator's durable page. */
export function listSchedules(service: ServiceCore, query: ListSchedulesQuery) {
  return withPrincipal(service, (principal) => service.core.listSchedules(principal, query));
}

/** `GET /v1/daemon/orchestration/schedules/{schedule_id}` — durable inspect. */
export function inspectSchedule(service: ServiceCore, scheduleId: string) {
  return withPrincipal(service, (principal) => service.core.inspectSchedule(principal, scheduleId));
}

/** `GET /v1/daemon/orchestration/sessions` — the Creator's durable run page. */
export function listWorkflowSessions(service: ServiceCore, query: ListSessionsQuery) {
  return withPrincipal(service, (principal) =>
    service.core.listWorkflowSessions(principal, query),
  );
}

/** `GET /v1/daemon/orchestration/sessions/{run_id}` — the durable run detail. */
export function getWorkflowSession(service: ServiceCore, sessionId: string) {
  return withPrincipal(service, (principal) =>
    service.core.getWorkflowSession(principal, sessionId),
  );
}

/** `PATCH /v1/daemon/orchestration/schedules/{schedule_id}/core-context`. */
export function editCoreContext(service: ServiceCore, scheduleId: string, body: unknown) {
  return withPrincipal(service, (principal) =>
    service.core.editCoreContext(principal, scheduleId, wirePayload(body, 'request')),
  );
}

/**
 * `ListSchedulesQuery` from query-string parameters.
 *
 * `creator_id`/`status`/`cursor`/`sort` are opaque strings forwarded verbatim —
 * the core owner validates `sort` and the pagination cursor, and the explicit
 * `creator_id` filter is what the core refuses for a foreign creator (a
 * transport-side drop would answer with a silently empty page instead).
 *
 * A key outside this schema's own key set is refused, never dropped: the DTO
 * is assembled here from a fixed set, so an unfiltered drop would answer a
 * misspelled filter with a broader (and misleading) success page.
 */
function listSchedulesQuery(search: URLSearchParams): ListSchedulesQuery {
  refuseUnknownQueryKeys(search, ['creator_id', 'status', 'sort', 'cursor', 'limit']);
  const creator_id = search.get('creator_id');
  const status = search.get('status');
  const sort = search.get('sort');
  const cursor = search.get('cursor');
  const limit = search.get('limit');
  return {
    ...(creator_id !== null ? { creator_id } : {}),
    ...(status !== null ? { status } : {}),
    ...(sort !== null ? { sort } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...(limit !== null ? { limit: parseOptionalInteger(search, 'limit') } : {}),
  };
}

/** `ListSessionsQuery` from query-string parameters (same forwarding rules). */
function listSessionsQuery(search: URLSearchParams): ListSessionsQuery {
  refuseUnknownQueryKeys(search, ['creator_id', 'sort', 'cursor', 'limit']);
  const creator_id = search.get('creator_id');
  const sort = search.get('sort');
  const cursor = search.get('cursor');
  const limit = search.get('limit');
  return {
    ...(creator_id !== null ? { creator_id } : {}),
    ...(sort !== null ? { sort } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...(limit !== null ? { limit: parseOptionalInteger(search, 'limit') } : {}),
  };
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
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/schedules$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, _params, search) => ({
      body: await listSchedules(service, listSchedulesQuery(search)),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/schedules\/([^/]+)$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, params) => ({
      body: await inspectSchedule(service, params[0]),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/orchestration\/schedules\/([^/]+)\/core-context$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, params, _search, body) => ({
      body: await editCoreContext(service, params[0], body),
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
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/sessions$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, _params, search) => ({
      body: await listWorkflowSessions(service, listSessionsQuery(search)),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/orchestration\/sessions\/([^/]+)$/,
    tier: 'tier2',
    family: 'execution',
    handle: async (service, params) => ({
      body: await getWorkflowSession(service, params[0]),
    }),
  },
];
