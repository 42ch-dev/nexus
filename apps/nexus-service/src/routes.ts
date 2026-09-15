import type { ServerResponse } from 'node:http';
import type {
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';
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
  | 'context';

/** One retained family identity: exact path pattern, verb, tier and handler. */
export interface DomainRoute {
  readonly method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  /** Anchored pattern; capture groups are positional params. */
  readonly pattern: RegExp;
  readonly tier: Exclude<RouteTier, 'provider_stream' | 'unguarded'>;
  readonly family: DomainFamily;
  /** Non-200 success status the retained surface keeps (201/204). */
  readonly status?: number;
  readonly handle: (
    service: ServiceCore,
    params: string[],
    searchParams: URLSearchParams,
    body: unknown,
  ) => Promise<DomainResult>;
}

/** A family handler's success payload plus its retained status override. */
export type DomainResult = { status?: number; body: unknown };

/** Merged family inventory; exported as the bounded route-inventory seam. */
export const DOMAIN_ROUTES: readonly DomainRoute[] = [
  ...WORLD_ROUTES,
  ...WORK_ROUTES,
  ...CONTENT_ROUTES,
  ...KNOWLEDGE_ROUTES,
  ...ACTOR_ROUTES,
  ...MEMORY_ROUTES,
  ...CONTEXT_ROUTES,
];

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
}

const WORLD_KB_GRAPH = /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/graph$/;
const WORLD_KB_PATCH = /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/patch-entity$/;
const WORLD_KB_CANDIDATES = /^\/v1\/daemon\/worlds\/([^/]+)\/kb\/candidates$/;
const CORE_CHANGES = /^\/v1\/daemon\/core\/changes$/;
const SESSION = /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)$/;
const SESSION_OPS = /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)\/operations$/;
const SESSION_EVENTS = /^\/v1\/daemon\/agent-host\/sessions\/([^/]+)\/events$/;
const OPERATION = /^\/v1\/daemon\/agent-host\/operations\/([^/]+)$/;

export function matchRoute(method: string, pathname: string): RouteMatch | null {
  for (const route of DOMAIN_ROUTES) {
    if (route.method !== method) continue;
    const match = pathname.match(route.pattern);
    if (match) {
      return {
        tier: route.tier,
        family: route.family,
        params: match.slice(1),
        ...(route.status !== undefined ? { status: route.status } : {}),
      };
    }
  }
  if (method === 'GET' && pathname === '/v1/daemon/runtime/health') {
    return { tier: 'unguarded' };
  }
  if (method === 'GET' && pathname === '/v1/daemon/runtime/status') {
    return { tier: 'unguarded' };
  }
  if (method === 'GET' && pathname === '/v1/daemon/runtime/cert-fingerprint') {
    return { tier: 'unguarded' };
  }
  if (method === 'GET' && pathname === '/v1/daemon/daemon/status') {
    return { tier: 'unguarded' };
  }

  if (method === 'GET' && pathname === '/v1/daemon/agent-host/health') {
    return { tier: 'tier1' };
  }
  if (method === 'GET' && pathname === '/v1/daemon/agent-host/providers') {
    return { tier: 'tier1' };
  }
  if (method === 'POST' && pathname === '/v1/daemon/agent-host/scan') {
    return { tier: 'tier1' };
  }

  const graph = pathname.match(WORLD_KB_GRAPH);
  if (method === 'GET' && graph) {
    return { tier: 'tier2', worldId: graph[1] };
  }
  const patch = pathname.match(WORLD_KB_PATCH);
  if (method === 'POST' && patch) {
    return { tier: 'tier2', worldId: patch[1] };
  }
  const candidates = pathname.match(WORLD_KB_CANDIDATES);
  if (method === 'GET' && candidates) {
    return { tier: 'tier2', worldId: candidates[1] };
  }
  if (method === 'GET' && CORE_CHANGES.test(pathname)) {
    return { tier: 'tier2' };
  }

  if (method === 'GET' && pathname === '/v1/daemon/agent-host/sessions') {
    return { tier: 'tier2' };
  }
  if (method === 'POST' && pathname === '/v1/daemon/agent-host/sessions') {
    return { tier: 'provider_stream' };
  }
  const session = pathname.match(SESSION);
  if (session) {
    if (method === 'GET') return { tier: 'tier2', sessionId: session[1] };
    if (method === 'DELETE') return { tier: 'provider_stream', sessionId: session[1] };
  }
  const sessionOps = pathname.match(SESSION_OPS);
  if (method === 'POST' && sessionOps) {
    return { tier: 'provider_stream', sessionId: sessionOps[1] };
  }
  const sessionEvents = pathname.match(SESSION_EVENTS);
  if (method === 'GET' && sessionEvents) {
    return { tier: 'provider_stream', sessionId: sessionEvents[1] };
  }
  const operation = pathname.match(OPERATION);
  if (operation) {
    if (method === 'GET') return { tier: 'tier2', operationId: operation[1] };
    if (method === 'POST') return { tier: 'provider_stream', operationId: operation[1] };
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
    return {
      kind: 'json',
      body: result.body,
      ...(result.status !== undefined
        ? { status: result.status }
        : route.status !== undefined
          ? { status: route.status }
          : {}),
    };
  }
  switch (route.tier) {
    case 'unguarded':
      return { kind: 'json', body: handleUnguarded(service, pathname) };
    case 'tier1':
      return { kind: 'json', body: await handleTier1(service, method, pathname, body) };
    case 'tier2':
      return { kind: 'json', body: await handleTier2(service, method, pathname, searchParams, body, route) };
    case 'provider_stream':
      return await handleProviderStream(service, method, pathname, searchParams, body, route);
    default:
      throw routeNotMigrated(pathname);
  }
}

function handleUnguarded(service: ServiceCore, pathname: string): unknown {
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
    if (!service.tlsFingerprint) {
      return {
        fingerprint: '',
        algorithm: 'sha256',
      };
    }
    return service.tlsFingerprint;
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

/**
 * The embedded host is optional in the domain-only profile: "host not started"
 * is truthful degraded readiness, not a client error. Every other native
 * rejection (uninitialized, forbidden, not_found, invalid_input, ...) propagates.
 */
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

async function handleTier1(
  service: ServiceCore,
  method: string,
  pathname: string,
  _body: unknown,
): Promise<unknown> {
  if (pathname === '/v1/daemon/agent-host/health') {
    const response = await tryHostQuery(service, { query: 'health' });
    return response?.health ?? { running: false, active_sessions: 0, active_operations: 0 };
  }
  if (pathname === '/v1/daemon/agent-host/providers') {
    const response = await tryHostQuery(service, { query: 'catalog', format: 'catalog' });
    return response?.catalog ?? { providers: [] };
  }
  if (method === 'POST' && pathname === '/v1/daemon/agent-host/scan') {
    const response = await tryHostQuery(service, { query: 'catalog', format: 'scan' });
    return response?.scan ?? { entries: [] };
  }
  throw routeNotMigrated(pathname);
}

async function handleTier2(
  service: ServiceCore,
  method: string,
  pathname: string,
  searchParams: URLSearchParams,
  body: unknown,
  route: RouteMatch,
): Promise<unknown> {
  if (method === 'GET' && route.worldId && pathname.endsWith('/kb/graph')) {
    return getWorldKbGraph(service, route.worldId, parseIncludeSuggested(searchParams));
  }
  if (method === 'POST' && route.worldId && pathname.endsWith('/kb/patch-entity')) {
    return patchWorldKbEntity(service, route.worldId, body as WorldKbPatchEntityRequest);
  }
  if (method === 'GET' && route.worldId && pathname.endsWith('/kb/candidates')) {
    const limit = parseClampedLimit(searchParams.get('limit'), 'limit');
    const cursor = searchParams.get('cursor') ?? undefined;
    return getWorldKbCandidates(service, route.worldId, limit, cursor);
  }
  if (method === 'GET' && pathname === '/v1/daemon/core/changes') {
    const afterSequence = searchParams.get('after_sequence');
    if (!afterSequence) {
      throw new HttpError(400, 'invalid_input', 'after_sequence is required');
    }
    const limit = parseBoundedLimit(searchParams.get('limit'), 'limit', { max: 256 });
    return getCoreChanges(service, {
      after_sequence: afterSequence,
      ...(limit !== undefined ? { limit } : {}),
    });
  }
  if (method === 'GET' && pathname === '/v1/daemon/agent-host/sessions') {
    const response = await hostQuery(service, {
      query: 'list_sessions',
      limit: parseClampedLimit(searchParams.get('limit'), 'limit'),
      cursor: searchParams.get('cursor') ?? undefined,
    });
    return response.sessions ?? { items: [], pagination: { limit: 50, has_more: false, next_cursor: null } };
  }
  if (method === 'GET' && route.sessionId) {
    const session = await lookupProviderSession(service, route.sessionId);
    if (!session) {
      throw new HttpError(404, 'not_found', `session ${route.sessionId} not found`, {
        resource: `session:${route.sessionId}`,
      });
    }
    return session;
  }
  if (method === 'GET' && route.operationId) {
    const operation = await lookupProviderOperation(service, route.operationId);
    if (!operation) {
      throw new HttpError(404, 'not_found', `operation ${route.operationId} not found`, {
        resource: `operation:${route.operationId}`,
      });
    }
    return {
      operation_id: operation.operation_id,
      session_id: operation.session_id,
      status: operation.status,
    };
  }
  throw routeNotMigrated(pathname);
}

async function handleProviderStream(
  service: ServiceCore,
  method: string,
  pathname: string,
  searchParams: URLSearchParams,
  body: unknown,
  route: RouteMatch,
): Promise<RouteHandlerResult> {
  if (method === 'POST' && pathname === '/v1/daemon/agent-host/sessions') {
    return { kind: 'json', body: await createProviderSession(service, body) };
  }
  if (method === 'DELETE' && route.sessionId) {
    return { kind: 'json', body: await shutdownProviderSession(service, route.sessionId) };
  }
  if (method === 'POST' && route.sessionId && pathname.endsWith('/operations')) {
    return { kind: 'json', body: await executeProviderOperation(service, route.sessionId, body) };
  }
  if (method === 'POST' && route.operationId) {
    return { kind: 'json', body: await cancelProviderOperation(service, route.operationId) };
  }
  if (method === 'GET' && route.sessionId && pathname.endsWith('/events')) {
    const sessionId = route.sessionId;
    return {
      kind: 'sse',
      run: async (res) => {
        await streamSessionEvents(service, sessionId, searchParams, res);
      },
    };
  }
  throw routeNotMigrated(pathname);
}
