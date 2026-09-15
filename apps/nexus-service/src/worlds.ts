/**
 * World family HTTP surface (P5-T1): World lifecycle (P0-T1 authority),
 * fork / pack / rules / world findings (P0-T2) and the World timeline reads
 * (P0-T3). Every operation is a thin translation over the native family
 * surface — the stored Principal is minted natively, ownership/CAS/effects
 * stay in the single Rust core authority, and the wire shapes are the
 * schema-owned generated DTOs (`schemas/core/narrative-api.schema.json`,
 * `schemas/daemon-api/{worlds,timeline}/…`).
 */
import type {
  CreateForkRequest,
  CreateForkResponse,
  CreateWorldRequest,
  CreateWorldResponse,
  CoreTimelineEventsQuery,
  ListTimelineEventsResponse,
  NarrativeWorldResponse,
  NarrativeWorldsListResponse,
  PackExportRequest,
  PackExportResponse,
  PackImportRequest,
  PackImportResponse,
  TimelineOverviewResponse,
  WorldFindingsListResponse,
  WorldRuleCreateRequest,
  WorldRuleResponse,
  WorldRuleUpdateRequest,
  WorldRulesListResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import { withPrincipal, wirePayload } from './world-kb.js';

/** `GET /v1/daemon/narrative/worlds`. */
export function listWorlds(service: ServiceCore): Promise<NarrativeWorldsListResponse> {
  return withPrincipal(service, (principal) => service.core.narrativeListWorlds(principal));
}

/** `GET /v1/daemon/narrative/worlds/{world_id}` (404 for an unknown id). */
export function getWorld(service: ServiceCore, worldId: string): Promise<NarrativeWorldResponse> {
  return withPrincipal(service, (principal) =>
    service.core.narrativeGetWorld(principal, worldId),
  );
}

/** `POST /v1/daemon/worlds` — 201 created. */
export function createWorld(
  service: ServiceCore,
  request: CreateWorldRequest,
): Promise<CreateWorldResponse> {
  return withPrincipal(service, (principal) => service.core.createWorld(principal, request));
}

/** `DELETE /v1/daemon/worlds/{world_id}` — hard delete, 204. */
export async function deleteWorld(service: ServiceCore, worldId: string): Promise<void> {
  await withPrincipal(service, async (principal) => service.core.deleteWorld(principal, worldId));
}

/** `POST /v1/daemon/worlds/{world_id}/forks`. */
export function createWorldFork(
  service: ServiceCore,
  worldId: string,
  request: CreateForkRequest,
): Promise<CreateForkResponse> {
  return withPrincipal(service, (principal) =>
    service.core.createWorldFork(principal, worldId, request),
  );
}

/** `POST /v1/daemon/worlds/{world_id}/kb/pack/export`. */
export function exportWorldPack(
  service: ServiceCore,
  worldId: string,
  request: PackExportRequest,
): Promise<PackExportResponse> {
  return withPrincipal(service, (principal) =>
    service.core.exportWorldPack(principal, worldId, request),
  );
}

/** `POST /v1/daemon/worlds/{world_id}/kb/pack/import`. */
export function importWorldPack(
  service: ServiceCore,
  worldId: string,
  request: PackImportRequest,
): Promise<PackImportResponse> {
  return withPrincipal(service, (principal) =>
    service.core.importWorldPack(principal, worldId, request),
  );
}

/** `GET /v1/daemon/worlds/{world_id}/rules`. */
export function listWorldRules(
  service: ServiceCore,
  worldId: string,
): Promise<WorldRulesListResponse> {
  return withPrincipal(service, (principal) => service.core.listWorldRules(principal, worldId));
}

/** `POST /v1/daemon/worlds/{world_id}/rules` — 201 created. */
export function createWorldRule(
  service: ServiceCore,
  worldId: string,
  request: WorldRuleCreateRequest,
): Promise<WorldRuleResponse> {
  return withPrincipal(service, (principal) =>
    service.core.createWorldRule(principal, worldId, request),
  );
}

/** `PATCH /v1/daemon/worlds/{world_id}/rules/{rule_id}`. */
export function updateWorldRule(
  service: ServiceCore,
  worldId: string,
  ruleId: string,
  request: WorldRuleUpdateRequest,
): Promise<WorldRuleResponse> {
  return withPrincipal(service, (principal) =>
    service.core.updateWorldRule(principal, worldId, ruleId, request),
  );
}

/** `GET /v1/daemon/worlds/{world_id}/findings`. */
export function listWorldFindings(
  service: ServiceCore,
  worldId: string,
): Promise<WorldFindingsListResponse> {
  return withPrincipal(service, (principal) =>
    service.core.listWorldFindings(principal, worldId),
  );
}

/** `GET /v1/daemon/timeline/overview?cursor=`. */
export function timelineOverview(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<TimelineOverviewResponse> {
  const cursor = searchParams.get('cursor');
  return withPrincipal(service, (principal) =>
    service.core.timelineOverview(principal, cursor === null ? {} : { cursor }),
  );
}
/** The wire timeline-event status vocabulary (`CoreTimelineEventsQuery.status`). */
const TIMELINE_EVENT_STATUSES: readonly NonNullable<
  CoreTimelineEventsQuery['status']
>[] = ['canon', 'provisional', 'rejected'];

/** Validate `?status=` against the wire union (400 on anything else). */
function timelineEventStatus(raw: string): NonNullable<CoreTimelineEventsQuery['status']> {
  if ((TIMELINE_EVENT_STATUSES as readonly string[]).includes(raw)) {
    return raw as NonNullable<CoreTimelineEventsQuery['status']>;
  }
  throw new HttpError(
    400,
    'invalid_input',
    `status must be one of: ${TIMELINE_EVENT_STATUSES.join(', ')}`,
  );
}

/** `GET /v1/daemon/worlds/{world_id}/timeline/events` — bounded page read. */
export function listTimelineEvents(
  service: ServiceCore,
  worldId: string,
  searchParams: URLSearchParams,
): Promise<ListTimelineEventsResponse> {
  const branch_id = searchParams.get('branch_id');
  const status = searchParams.get('status');
  const event_type = searchParams.get('event_type');
  const limit = searchParams.get('limit');
  const cursor = searchParams.get('cursor');
  return withPrincipal(service, (principal) =>
    service.core.listTimelineEvents(principal, worldId, {
      ...(branch_id !== null ? { branch_id } : {}),
      ...(status !== null ? { status: timelineEventStatus(status) } : {}),
      ...(event_type !== null ? { event_type } : {}),
      ...(limit !== null ? { limit: Number(limit) } : {}),
      ...(cursor !== null ? { cursor } : {}),
    }),
  );
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const WORLD_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/narrative\/worlds$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service) => ({ body: await listWorlds(service) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/narrative\/worlds\/([^/]+)$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params) => ({ body: await getWorld(service, params[0]) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds$/,
    tier: 'tier2',
    family: 'worlds',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await createWorld(service, wirePayload<CreateWorldRequest>(body, 'request')),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)$/,
    tier: 'tier2',
    family: 'worlds',
    status: 204,
    handle: async (service, params) => {
      await deleteWorld(service, params[0]);
      return { body: null };
    },
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/forks$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params, _search, body) => ({
      body: await createWorldFork(service, params[0], wirePayload<CreateForkRequest>(body, 'request')),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/pack\/export$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params, _search, body) => ({
      body: await exportWorldPack(service, params[0], wirePayload<PackExportRequest>(body, 'request')),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/pack\/import$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params, _search, body) => ({
      body: await importWorldPack(service, params[0], wirePayload<PackImportRequest>(body, 'request')),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/rules$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params) => ({ body: await listWorldRules(service, params[0]) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/rules$/,
    tier: 'tier2',
    family: 'worlds',
    status: 201,
    handle: async (service, params, _search, body) => ({
      body: await createWorldRule(
        service,
        params[0],
        wirePayload<WorldRuleCreateRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/rules\/([^/]+)$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params, _search, body) => ({
      body: await updateWorldRule(
        service,
        params[0],
        params[1],
        wirePayload<WorldRuleUpdateRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/findings$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params) => ({ body: await listWorldFindings(service, params[0]) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/timeline\/overview$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, _params, search) => ({ body: await timelineOverview(service, search) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/timeline\/events$/,
    tier: 'tier2',
    family: 'worlds',
    handle: async (service, params, search) => ({
      body: await listTimelineEvents(service, params[0], search),
    }),
  },
];
