import type { ServerResponse } from 'node:http';
import type { WorldKbPatchEntityRequest } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError, routeNotMigrated } from './errors.js';
import {
  cancelProviderOperation,
  createProviderSession,
  executeProviderOperation,
  lookupProviderOperation,
  lookupProviderSession,
  shutdownProviderSession,
} from './provider.js';
import { streamSessionEvents } from './sse.js';
import { CONTENT_ROUTES } from './content.js';
import { KNOWLEDGE_ROUTES } from './knowledge.js';
import { WORLD_ROUTES } from './worlds.js';
import { WORK_ROUTES } from './works.js';
import {
  getCoreChanges,
  getWorldKbCandidates,
  getWorldKbGraph,
  hostQuery,
  parseBoundedLimit,
  parseClampedLimit,
  parseIncludeSuggested,
  patchWorldKbEntity,
} from './world-kb.js';
import { ACTOR_ROUTES } from './actors.js';
import { MEMORY_ROUTES } from './memory.js';
import { CONTEXT_ROUTES } from './context.js';
import { PRESET_ROUTES } from './presets.js';
import { EXECUTION_ROUTES } from './execution.js';

/**
 * The World/Work/content/knowledge families self-describe their retained
 * identities (exact path/verb/tier); this composer merges them ahead of the
 * legacy hand-written matcher. P5-T5 owns the final whole-service inventory.
 */
export type DomainFamily =
  | 'worlds'
  | 'works'
  | 'content'
  | 'knowledge'
  | 'actors'
  | 'memory'
  | 'context'
  | 'presets'
  | 'execution'
  | 'runtime'
  | 'host'
  | 'world_kb'

/** One retained family identity: exact path pattern, verb, tier and handler. */
export interface DomainRoute {
  readonly method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  /** Anchored pattern; capture groups are positional params. */
  readonly pattern: RegExp;
  readonly tier: RouteTier;
  readonly family: DomainFamily;
  /** Non-200 success status the retained surface keeps (201/204). */
  readonly status?: number;
  /** Declared capture name: the first pattern group publishes under this key. */
  readonly capture?: 'sessionId' | 'operationId' | 'worldId';
  readonly handle: (
    service: ServiceCore,
    params: string[],
    searchParams: URLSearchParams,
    body: unknown,
  ) => Promise<DomainResult>;
}

/** A family handler's success payload plus its retained status override. */
export type DomainResult =
  | { status?: number; body: unknown }
  | { kind: 'sse'; run: (res: ServerResponse) => Promise<void> };


export type RouteTier = 'unguarded' | 'tier1' | 'tier2' | 'provider_stream';

export type RouteHandlerResult =
  | { kind: 'json'; body: unknown; status?: number }
  | { kind: 'sse'; run: (res: ServerResponse) => Promise<void> };

export interface RouteMatch {
  tier: RouteTier;
  worldId?: string;
  sessionId?: string;
  operationId?: string;
  /** Set when the match came from a self-describing family route. */
  family?: DomainFamily;
  params?: string[];
  /** Retained non-200 success status (201/204) for the family handler. */
  status?: number;
  /** Provider-stream dispatch (SSE/session control) handled by the family entry. */
  providerStream?: boolean;
}


export function matchRoute(method: string, pathname: string): RouteMatch | null {
  for (const route of DOMAIN_ROUTES) {
    if (route.method !== method) continue;
    const match = pathname.match(route.pattern);
    if (match) {
      const captured: Partial<Record<'sessionId' | 'operationId' | 'worldId', string>> = {};
      if (route.capture !== undefined && match[1] !== undefined) {
        captured[route.capture] = match[1];
      }
      return {
        tier: route.tier,
        family: route.family,
        params: match.slice(1),
        ...captured,
        ...(route.status !== undefined ? { status: route.status } : {}),
        ...(route.tier === 'unguarded'
          ? {}
          : route.tier === 'provider_stream'
            ? { providerStream: true }
            : {}),
      };
    }
  }
  if (pathname.startsWith('/v1/daemon/')) {
    return null;
  }
  return null;
}

export async function handleRoute(
  service: ServiceCore,
  method: string,
  pathname: string,
  searchParams: URLSearchParams,
  body: unknown,
): Promise<RouteHandlerResult> {
  const route = matchRoute(method, pathname);
  if (!route) {
    throw routeNotMigrated(pathname);
  }

  if (route.family && route.params) {
    const entry = DOMAIN_ROUTES.find(
      (candidate) => candidate.family === route.family && candidate.pattern.test(pathname) && candidate.method === method,
    );
    if (!entry) {
      throw routeNotMigrated(pathname);
    }
    const result = await entry.handle(service, route.params, searchParams, body);
    if ('kind' in result && result.kind === 'sse') {
      return result;
    }
    const json = result as { status?: number; body: unknown };
    return {
      kind: 'json',
      body: json.body,
      ...(json.status !== undefined
        ? { status: json.status }
        : route.status !== undefined
          ? { status: route.status }
          : {}),
    };
  }
  throw routeNotMigrated(pathname);
}


/**
 * The embedded host is optional in the domain-only profile: "host not started"
 * is truthful degraded readiness, not a client error. Every other native
 * rejection (uninitialized, forbidden, not_found, invalid_input, ...) propagates.
 */

/** A legacy-branch route's success payload (family entries only). */
type LegacyBody = unknown;

/** Truthful unguarded runtime liveness/status surface (no API key). */
function runtimeUnguarded(service: ServiceCore, pathname: string): LegacyBody {
  if (pathname === '/v1/daemon/runtime/health') {
    return { status: 'ok', version: '0.1.0' };
  }
  if (pathname === '/v1/daemon/runtime/status') {
    return {
      version: '0.1.0',
      uptime_seconds: Math.floor((Date.now() - Date.parse(service.startedAt)) / 1000),
      workspace_initialized: service.workspaceInitialized,
      acp: {
        tool_execution_enabled:
          service.workspaceInitialized && !service.domainOnly && service.providerReady,
        active_sessions: 0,
        total_tool_executions: 0,
      },
      runtime_mode: runtimeMode(service),
    };
  }
  if (pathname === '/v1/daemon/runtime/cert-fingerprint') {
    return service.tlsFingerprint ?? { fingerprint: '', algorithm: 'sha256' };
  }
  if (pathname === '/v1/daemon/daemon/status') {
    const degraded = degradedSubsystems(service);
    return {
      schema_version: 2,
      lifecycle_state: 'running',
      version: '0.1.0',
      implementation_scope: 'standalone-service (P4-T1)',
      uptime_ms: Date.now() - Date.parse(service.startedAt),
      started_at: service.startedAt,
      pid: process.pid,
      degraded: {
        subsystems: degraded,
        reasons: degraded.map((name) => `${name} not ready`),
      },
      subsystems: {
        http: { status: 'up', last_check_ms: 0 },
        db: { status: service.workspaceInitialized ? 'up' : 'down', last_check_ms: 0 },
        engine: {
          status:
            service.workspaceInitialized && !service.domainOnly && service.providerReady
              ? 'up'
              : 'down',
          last_check_ms: 0,
        },
      },
      exit_code: null,
      last_error: null,
    };
  }
  throw routeNotMigrated(pathname);
}

/** Runtime liveness/status family (unguarded; the API-key gate skips it). */
export const RUNTIME_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/runtime\/health$/,
    tier: 'unguarded',
    family: 'runtime',
    handle: async (service, _params, _search, _body) => ({
      body: runtimeUnguarded(service, '/v1/daemon/runtime/health'),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/runtime\/status$/,
    tier: 'unguarded',
    family: 'runtime',
    handle: async (service) => ({ body: runtimeUnguarded(service, '/v1/daemon/runtime/status') }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/runtime\/cert-fingerprint$/,
    tier: 'unguarded',
    family: 'runtime',
    handle: async (service) => ({
      body: runtimeUnguarded(service, '/v1/daemon/runtime/cert-fingerprint'),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/daemon\/status$/,
    tier: 'unguarded',
    family: 'runtime',
    handle: async (service) => ({ body: runtimeUnguarded(service, '/v1/daemon/daemon/status') }),
  },
];

/** Agent-Host tier1 control family (API-key only, no active creator). */
export const HOST_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/health$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service) => {
      const response = await tryHostQuery(service, { query: 'health' });
      return { body: response?.health ?? { running: false, active_sessions: 0, active_operations: 0 } };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/providers$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service) => {
      const response = await tryHostQuery(service, { query: 'catalog', format: 'catalog' });
      return { body: response?.catalog ?? { providers: [] } };
    },
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/agent-host\/scan$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service) => {
      const response = await tryHostQuery(service, { query: 'catalog', format: 'scan' });
      return { body: response?.scan ?? { entries: [] } };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/sessions$/,
    tier: 'tier2',
    family: 'host',
    handle: async (service, _params, search) => {
      const response = await hostQuery(service, {
        query: 'list_sessions',
        limit: parseClampedLimit(search.get('limit'), 'limit'),
        cursor: search.get('cursor') ?? undefined,
      });
      return {
        body:
          response.sessions ??
          { items: [], pagination: { limit: 50, has_more: false, next_cursor: null } },
      };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)$/,
    tier: 'tier2',
    family: 'host',
    capture: 'sessionId',
    handle: async (service, params) => {
      const session = await lookupProviderSession(service, params[0]);
      if (!session) {
        throw new HttpError(404, 'not_found', `session ${params[0]} not found`, {
          resource: `session:${params[0]}`,
        });
      }
      return { body: session };
    },
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/operations\/([^/]+)$/,
    tier: 'tier2',
    family: 'host',
    capture: 'operationId',
    handle: async (service, params) => {
      const operation = await lookupProviderOperation(service, params[0]);
      if (!operation) {
        throw new HttpError(404, 'not_found', `operation ${params[0]} not found`, {
          resource: `operation:${params[0]}`,
        });
      }
      return {
        body: {
          operation_id: operation.operation_id,
          session_id: operation.session_id,
          status: operation.status,
        },
      };
    },
  },
  // Provider-stream session control: session create/shutdown, operation
  // execute/cancel and the bounded SSE event pull.
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/agent-host\/sessions$/,
    tier: 'provider_stream',
    family: 'host',
    handle: async (service, _params, _search, body) => ({
      body: await createProviderSession(service, body),
    }),
  },
  {
    method: 'DELETE',
    pattern: /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)$/,
    tier: 'provider_stream',
    family: 'host',
    capture: 'sessionId',
    handle: async (service, params) => ({
      body: await shutdownProviderSession(service, params[0]),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)\/operations$/,
    tier: 'provider_stream',
    family: 'host',
    capture: 'sessionId',
    handle: async (service, params, _search, body) => ({
      body: await executeProviderOperation(service, params[0], body),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/agent-host\/operations\/([^/]+)$/,
    tier: 'provider_stream',
    family: 'host',
    capture: 'operationId',
    handle: async (service, params) => ({
      body: await cancelProviderOperation(service, params[0]),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)\/events$/,
    tier: 'provider_stream',
    family: 'host',
    capture: 'sessionId',
    handle: async (service, params, search) => {
      const sessionId = params[0];
      return {
        kind: 'sse',
        run: async (res) => {
          await streamSessionEvents(service, sessionId, search, res);
        },
      };
    },
  },
];

/** World-KB and core changes family (tier2; handler bodies unchanged). */
export const WORLD_KB_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/graph$/,
    tier: 'tier2',
    family: 'world_kb',
    handle: async (service, params, search) => ({
      body: await getWorldKbGraph(service, params[0], parseIncludeSuggested(search)),
    }),
  },
  {
    method: 'POST',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/patch-entity$/,
    tier: 'tier2',
    family: 'world_kb',
    handle: async (service, params, _search, body) => ({
      body: await patchWorldKbEntity(service, params[0], body as WorldKbPatchEntityRequest),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/candidates$/,
    tier: 'tier2',
    family: 'world_kb',
    handle: async (service, params, search) => ({
      body: await getWorldKbCandidates(
        service,
        params[0],
        parseClampedLimit(search.get('limit'), 'limit'),
        search.get('cursor') ?? undefined,
      ),
    }),
  },
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/core\/changes$/,
    tier: 'tier2',
    family: 'world_kb',
    handle: async (service, _params, search) => {
      const afterSequence = search.get('after_sequence');
      if (!afterSequence) {
        throw new HttpError(400, 'invalid_input', 'after_sequence is required');
      }
      const limit = parseBoundedLimit(search.get('limit'), 'limit', { max: 256 });
      return {
        body: await getCoreChanges(service, {
          after_sequence: afterSequence,
          ...(limit !== undefined ? { limit } : {}),
        }),
      };
    },
  },
];

/** Merged family inventory; exported as the bounded route-inventory seam. */
export const DOMAIN_ROUTES: readonly DomainRoute[] = [
  ...WORLD_ROUTES,
  ...WORK_ROUTES,
  ...CONTENT_ROUTES,
  ...KNOWLEDGE_ROUTES,
  ...ACTOR_ROUTES,
  ...MEMORY_ROUTES,
  ...CONTEXT_ROUTES,
  ...PRESET_ROUTES,
  ...EXECUTION_ROUTES,
  ...RUNTIME_ROUTES,
  ...HOST_ROUTES,
  ...WORLD_KB_ROUTES,
];

async function tryHostQuery(service: ServiceCore, request: Parameters<typeof hostQuery>[1]) {
  try {
    return await hostQuery(service, request);
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'invalid_input' && mapped.message === 'host not started') {
      return null;
    }
    throw mapped;
  }
}

/** Truthful runtime mode: uninitialized is reported as such, never as ready. */
function runtimeMode(service: ServiceCore): string {
  if (!service.workspaceInitialized) return 'uninitialized';
  if (service.domainOnly) return 'domain_only';
  return service.providerReady ? 'provider_enabled' : 'provider_degraded';
}

function degradedSubsystems(service: ServiceCore): string[] {
  const degraded: string[] = [];
  if (!service.workspaceInitialized) degraded.push('db');
  if (!service.domainOnly && !service.providerReady) degraded.push('engine');
  return degraded;
}



