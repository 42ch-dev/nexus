/**
 * Work family HTTP surface (P5-T1): Work lifecycle, authoring pool and
 * inspiration, durable selection, completion-lock release and chapter
 * reconcile (P1-T1 authority). Every operation is a thin translation over the
 * native family surface — the stored Principal is minted natively, ownership,
 * the per-Work locks and pool rules stay in the single Rust core authority,
 * and the wire shapes are the schema-owned generated DTOs
 * (`schemas/core/works-api.schema.json`, `schemas/daemon-api/works/…`).
 */
import type {
  AppendInspirationRequest,
  AppendInspirationResponse,
  CoreWorkSelection,
  CreateWorkRequest,
  CreateWorkResponse,
  ListWorksQuery,
  ListWorksResponse,
  PatchWorkRequest,
  ReleaseCompletionLockRequest,
  WorkDetailResponse,
  WorkInspirationAddRequest,
  WorkInspirationAddResponse,
  WorkInspirationArchiveRequest,
  WorkInspirationItem,
  WorkInspirationListQuery,
  WorkInspirationListResponse,
  WorkInspirationPromoteRequest,
  WorkInspirationPromoteResponse,
  WorkPoolArchiveRequest,
  WorkPoolEntry,
  WorkPoolListQuery,
  WorkPoolListResponse,
  WorkPoolPromoteRequest,
  WorkPoolSetActiveRequest,
  WorkReconcileReport,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import {
  parseOptionalBoolean,
  parseOptionalInteger,
  withPrincipal,
  wirePayload,
} from './world-kb.js';

/** `GET /v1/daemon/works`. */
export function listWorks(
  service: ServiceCore,
  query: ListWorksQuery,
): Promise<ListWorksResponse> {
  return withPrincipal(service, (principal) => service.core.listWorks(principal, query));
}

/** `GET /v1/daemon/works/{work_id}`. */
export function getWork(service: ServiceCore, workId: string): Promise<WorkDetailResponse> {
  return withPrincipal(service, (principal) => service.core.getWork(principal, workId));
}

/**
 * `POST /v1/daemon/works` — returns the retained `{created, response}` pair;
 * the route maps `created` onto the legacy 201 (new) / 200 (idempotent
 * replay) seam.
 */
export function createWork(
  service: ServiceCore,
  request: CreateWorkRequest,
): Promise<{ created: boolean; response: CreateWorkResponse }> {
  return withPrincipal(service, async (principal) => {
    const [created, response] = await service.core.createWork(principal, request);
    return { created, response };
  });
}

/**
 * `PATCH /v1/daemon/works/{work_id}` — tri-state binding patch: `world_id` /
 * `story_ref` omitted keeps the stored binding, `null` clears it, a value
 * sets it. The generated wire carrier keeps all three states distinguishable
 * and the native boundary enforces the grammar before any effect.
 */
export function patchWork(
  service: ServiceCore,
  workId: string,
  request: PatchWorkRequest,
): Promise<WorkDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.patchWork(principal, workId, request),
  );
}

/** `DELETE /v1/daemon/works/{work_id}` — 204. */
export async function deleteWork(service: ServiceCore, workId: string): Promise<void> {
  await withPrincipal(service, async (principal) => service.core.deleteWork(principal, workId));
}

/** `POST /v1/daemon/works/{work_id}/inspiration`. */
export function appendWorkInspiration(
  service: ServiceCore,
  workId: string,
  request: AppendInspirationRequest,
): Promise<AppendInspirationResponse> {
  return withPrincipal(service, (principal) =>
    service.core.appendWorkInspiration(principal, workId, request),
  );
}

/**
 * `POST /v1/daemon/works/pool` — the durable Work selection mutation over
 * HTTP (same persisted selection seam as CLI `works use`).
 */
export function setWorkPoolActive(
  service: ServiceCore,
  request: WorkPoolSetActiveRequest,
): Promise<WorkPoolEntry> {
  return withPrincipal(service, (principal) =>
    service.core.setWorkPoolActive(principal, request),
  );
}

/** `POST /v1/daemon/works/{work_id}/completion-lock/release`. */
export function releaseWorkCompletionLock(
  service: ServiceCore,
  workId: string,
  request: ReleaseCompletionLockRequest,
): Promise<WorkDetailResponse> {
  return withPrincipal(service, (principal) =>
    service.core.releaseWorkCompletionLock(principal, workId, request),
  );
}

/** `POST /v1/daemon/works/{work_id}/reconcile-chapters?dry_run=`. */
export function reconcileWorkChapters(
  service: ServiceCore,
  workId: string,
  searchParams: URLSearchParams,
): Promise<WorkReconcileReport> {
  const dry_run = parseOptionalBoolean(searchParams, 'dry_run');
  return withPrincipal(service, (principal) =>
    service.core.reconcileWorkChapters(principal, workId, {
      ...(dry_run !== undefined ? { dry_run } : {}),
    }),
  );
}

/** `GET /v1/daemon/works/pool`. */
export function listWorkPool(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<WorkPoolListResponse> {
  const query: WorkPoolListQuery = {
    status: searchParams.get('status') ?? undefined,
    limit: parseOptionalInteger(searchParams, 'limit'),
    offset: parseOptionalInteger(searchParams, 'offset'),
  };
  return withPrincipal(service, (principal) => service.core.listWorkPool(principal, query));
}

/** `POST /v1/daemon/works/pool/promote`. */
export function promoteWorkPoolEntry(
  service: ServiceCore,
  request: WorkPoolPromoteRequest,
): Promise<WorkPoolEntry> {
  return withPrincipal(service, (principal) =>
    service.core.promoteWorkPoolEntry(principal, request),
  );
}

/** `POST /v1/daemon/works/pool/archive`. */
export function archiveWorkPoolEntry(
  service: ServiceCore,
  request: WorkPoolArchiveRequest,
): Promise<WorkPoolEntry> {
  return withPrincipal(service, (principal) =>
    service.core.archiveWorkPoolEntry(principal, request),
  );
}

/** `POST /v1/daemon/works/pool/inspiration` — 201 created. */
export function addWorkInspiration(
  service: ServiceCore,
  request: WorkInspirationAddRequest,
): Promise<WorkInspirationAddResponse> {
  return withPrincipal(service, (principal) =>
    service.core.addWorkInspiration(principal, request),
  );
}

/** `GET /v1/daemon/works/pool/inspiration`. */
export function listWorkInspiration(
  service: ServiceCore,
  searchParams: URLSearchParams,
): Promise<WorkInspirationListResponse> {
  const query: WorkInspirationListQuery = {
    status: searchParams.get('status') ?? undefined,
    limit: parseOptionalInteger(searchParams, 'limit'),
    offset: parseOptionalInteger(searchParams, 'offset'),
  };
  return withPrincipal(service, (principal) =>
    service.core.listWorkInspiration(principal, query),
  );
}

/** `POST /v1/daemon/works/pool/inspiration/promote`. */
export function promoteWorkInspiration(
  service: ServiceCore,
  request: WorkInspirationPromoteRequest,
): Promise<WorkInspirationPromoteResponse> {
  return withPrincipal(service, (principal) =>
    service.core.promoteWorkInspiration(principal, request),
  );
}

/** `POST /v1/daemon/works/pool/inspiration/archive`. */
export function archiveWorkInspiration(
  service: ServiceCore,
  request: WorkInspirationArchiveRequest,
): Promise<WorkInspirationItem> {
  return withPrincipal(service, (principal) =>
    service.core.archiveWorkInspiration(principal, request),
  );
}

/**
 * Durable Work selection (CLI `works use` seam): persisted via the authoring
 * pool, never session-local. Exposed for the route composer and the bounded
 * integration test.
 */
export function selectWork(
  service: ServiceCore,
  workId: string,
): Promise<CoreWorkSelection> {
  return withPrincipal(service, (principal) => service.core.selectWork(principal, workId));
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const WORK_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, search) => ({
      body: await listWorks(service, worksListQuery(search)),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => {
      const { created, response } = await createWork(
        service,
        wirePayload<CreateWorkRequest>(body, 'request'),
      );
      return { status: created ? 201 : 200, body: response };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/pool$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, search) => ({ body: await listWorkPool(service, search) }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => ({
      body: await setWorkPoolActive(
        service,
        wirePayload<WorkPoolSetActiveRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool\/promote$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => ({
      body: await promoteWorkPoolEntry(
        service,
        wirePayload<WorkPoolPromoteRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool\/archive$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => ({
      body: await archiveWorkPoolEntry(
        service,
        wirePayload<WorkPoolArchiveRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/pool\/inspiration$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, search) => ({
      body: await listWorkInspiration(service, search),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool\/inspiration$/,
    tier: 'tier2',
    family: 'works',
    status: 201,
    handle: async (service, _params, _search, body) => ({
      body: await addWorkInspiration(
        service,
        wirePayload<WorkInspirationAddRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool\/inspiration\/promote$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => ({
      body: await promoteWorkInspiration(
        service,
        wirePayload<WorkInspirationPromoteRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/pool\/inspiration\/archive$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, _params, _search, body) => ({
      body: await archiveWorkInspiration(
        service,
        wirePayload<WorkInspirationArchiveRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/works\/([^/]+)$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, params) => ({ body: await getWork(service, params[0]) }),
  },
  {
    method: 'PATCH',
    pattern: /^\/v1\/daemon\/works\/([^/]+)$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, params, _search, body) => ({
      body: await patchWork(service, params[0], wirePayload<PatchWorkRequest>(body, 'request')),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/works\/([^/]+)$/,
    tier: 'tier2',
    family: 'works',
    status: 204,
    handle: async (service, params) => {
      await deleteWork(service, params[0]);
      return { body: null };
    },
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/inspiration$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, params, _search, body) => ({
      body: await appendWorkInspiration(
        service,
        params[0],
        wirePayload<AppendInspirationRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/completion-lock\/release$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, params, _search, body) => ({
      body: await releaseWorkCompletionLock(
        service,
        params[0],
        wirePayload<ReleaseCompletionLockRequest>(body, 'request'),
      ),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/works\/([^/]+)\/reconcile-chapters$/,
    tier: 'tier2',
    family: 'works',
    handle: async (service, params, search) => ({
      body: await reconcileWorkChapters(service, params[0], search),
    }),
  },
];

/** `ListWorksQuery` from query-string parameters (status/sort are opaque). */
function worksListQuery(search: URLSearchParams): ListWorksQuery {
  const status = search.get('status');
  const intake_status = search.get('intake_status');
  const sort = search.get('sort');
  const cursor = search.get('cursor');
  const limit = search.get('limit');
  return {
    ...(status !== null ? { status } : {}),
    ...(intake_status !== null ? { intake_status } : {}),
    ...(sort !== null ? { sort } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...(limit !== null ? { limit: parseOptionalInteger(search, 'limit', { max: 250 }) } : {}),
  };
}
