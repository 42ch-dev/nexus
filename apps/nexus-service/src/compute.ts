/**
 * Compute family HTTP surface (v1.195 P2-T3): the retained C1–C8 operations
 * over the ONE hosted execution owner.
 *
 * Every handler is a thin adapter over the native compute facade — the module
 * registry (C1/C2), the existing WASM run authority (C3, which persists
 * proposals or an honest failure and mutates nothing), run detail/history
 * (C4/C5), the one transactional accept with its CAS (C6), the effect-free
 * discard (C7) and the World-scoped terminal clear (C8). The World-ownership
 * gate, the transaction, the terminal-only delete predicate and the module
 * manifest all stay in Rust: no handler here reads SQL, compiles a module,
 * mints a run id or decides a status.
 *
 * Query forwarding is the schema's, not the transport's — except for the two
 * things a JSON wire cannot carry. `limit` arrives as a decimal string and is
 * parsed here only as an integer (the core owner applies the documented
 * default/cap), and `status` is checked against its closed enum here because
 * the retained C8 contract answers a malformed Clear query with a typed 422
 * `invalid_input` envelope, which is a transport-shaped refusal (a raw
 * `status=running` must never reach a broad delete).
 */
import type {
  ClearRunsQuery,
  ListRunsQuery,
  RunAcceptRequest,
  RunRequest,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { HttpError } from './errors.js';
import {
  parseOptionalInteger,
  refuseUnknownQueryKeys,
  withPrincipal,
  wirePayload,
} from './world-kb.js';

/** `GET /v1/daemon/compute/modules` — the installed module registry (C1). */
export function listComputeModules(service: ServiceCore) {
  return withPrincipal(service, (principal) => service.core.listComputeModules(principal));
}

/** `GET /v1/daemon/compute/modules/{module_id}` — the manifest Run Studio renders (C2). */
export function getComputeModule(service: ServiceCore, moduleId: string) {
  return withPrincipal(service, (principal) =>
    service.core.getComputeModule(principal, moduleId),
  );
}

/** `POST /v1/daemon/compute/run` — invoke a module against an owned World (C3). */
export function runCompute(service: ServiceCore, body: unknown) {
  const request = wirePayload<RunRequest>(body, 'request');
  return withPrincipal(service, (principal) => service.core.computeRun(principal, request));
}

/** `GET /v1/daemon/compute/runs/{run_id}` — proposals or the recorded error (C4). */
export function getComputeRun(service: ServiceCore, runId: string) {
  return withPrincipal(service, (principal) => service.core.getComputeRun(principal, runId));
}

/** `GET /v1/daemon/compute/runs` — the creator's run history page (C5). */
export function listComputeRuns(service: ServiceCore, query: ListRunsQuery) {
  return withPrincipal(service, (principal) => service.core.listComputeRuns(principal, query));
}

/** `POST /v1/daemon/compute/runs/{run_id}/accept` — commit that run once (C6). */
export function acceptComputeRun(service: ServiceCore, runId: string, body: unknown) {
  // The retained route's request body is optional: an omitted body (or `{}`)
  // means "accept every proposal". Only a body of another shape is a fault.
  const request: RunAcceptRequest = body === undefined ? {} : wirePayload(body, 'request');
  return withPrincipal(service, (principal) =>
    service.core.acceptComputeRun(principal, runId, request),
  );
}

/** `POST /v1/daemon/compute/runs/{run_id}/discard` — drop the proposals (C7). */
export function discardComputeRun(service: ServiceCore, runId: string) {
  return withPrincipal(service, (principal) => service.core.discardComputeRun(principal, runId));
}

/** `DELETE /v1/daemon/compute/runs?world_id=…` — World-scoped clear (C8). */
export function clearComputeRuns(service: ServiceCore, query: ClearRunsQuery) {
  return withPrincipal(service, (principal) => service.core.clearComputeRuns(principal, query));
}

/**
 * `ListRunsQuery` from the query string.
 *
 * `world_id`/`module_id`/`status`/`cursor` are opaque strings forwarded
 * verbatim: the generated DTO's closed status enum is what refuses an unknown
 * status, so the two surfaces cannot disagree about the vocabulary, and the
 * core owner — not this adapter — applies the scope and the page size. A key
 * outside the schema is refused rather than dropped: assembled from a fixed
 * set, a misspelled filter would otherwise answer with a broader (and
 * misleading) success page.
 */
function listRunsQuery(search: URLSearchParams): ListRunsQuery {
  refuseUnknownQueryKeys(search, ['world_id', 'module_id', 'status', 'limit', 'cursor'], 400);
  const world_id = search.get('world_id');
  const module_id = search.get('module_id');
  const status = search.get('status');
  const cursor = search.get('cursor');
  const limit = search.get('limit');
  return {
    ...(world_id !== null ? { world_id } : {}),
    ...(module_id !== null ? { module_id } : {}),
    ...(status !== null ? { status: parseRunStatus(status) } : {}),
    ...(cursor !== null ? { cursor } : {}),
    ...(limit !== null ? { limit: parseOptionalInteger(search, 'limit') } : {}),
  };
}

/**
 * `ClearRunsQuery` from the query string, refusing a malformed Clear with the
 * retained 422 `invalid_input` envelope.
 *
 * `world_id` is REQUIRED — Clear is World-scoped, and the retired route
 * answered a missing scope with 422 `invalid_input`, never a World-wide
 * delete. `status` narrows Clear to ONE terminal state; `running` and
 * `succeeded` (which still need review) are not values of this query at all,
 * so an out-of-vocabulary filter is refused here instead of being silently
 * widened into "every terminal row".
 */
function clearRunsQuery(search: URLSearchParams): ClearRunsQuery {
  refuseUnknownQueryKeys(search, ['world_id', 'status'], 422);
  const world_id = search.get('world_id');
  if (world_id === null || world_id.length === 0) {
    throw new HttpError(422, 'invalid_input', 'world_id is required to clear run history', {
      field: 'world_id',
    });
  }
  const status = search.get('status');
  if (status === null) return { world_id };
  if (status !== 'applied' && status !== 'discarded' && status !== 'failed') {
    throw new HttpError(
      422,
      'invalid_input',
      `status '${status}' cannot be cleared: only terminal states ` +
        "(applied|discarded|failed) are deletable",
      { field: 'status' },
    );
  }
  return { world_id, status };
}

/**
 * The run-list status filter as the wire enum member it names.
 *
 * The generated `list-runs-query` schema owns this vocabulary; this lookup only
 * turns the one string spelling into that member so TypeScript sees the closed
 * type. A value outside it is the schema's own refusal, with the same status
 * and code as any other bad value on this route.
 */
function parseRunStatus(value: string): NonNullable<ListRunsQuery['status']> {
  switch (value) {
    case 'running':
    case 'succeeded':
    case 'failed':
    case 'applied':
    case 'discarded':
      return value;
    default:
      throw new HttpError(400, 'invalid_input', `status '${value}' is not a run status`, {
        field: 'status',
      });
  }
}

/** Exact path/verb/tier identities this family owns (composer input). */
export const COMPUTE_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/compute\/modules$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service) => ({ body: await listComputeModules(service) }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/compute\/modules\/([^/]+)$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, params) => ({
      body: await getComputeModule(service, params[0]),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/compute\/run$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, _params, _search, body) => ({
      body: await runCompute(service, body),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/compute\/runs$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, _params, search) => ({
      body: await listComputeRuns(service, listRunsQuery(search)),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/compute\/runs\/([^/]+)\/accept$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, params, _search, body) => ({
      body: await acceptComputeRun(service, params[0], body),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/compute\/runs\/([^/]+)\/discard$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, params) => ({
      body: await discardComputeRun(service, params[0]),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/compute\/runs$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, _params, search) => ({
      body: await clearComputeRuns(service, clearRunsQuery(search)),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/compute\/runs\/([^/]+)$/,
    tier: 'tier2',
    family: 'compute',
    handle: async (service, params) => ({ body: await getComputeRun(service, params[0]) }),
  },
];
