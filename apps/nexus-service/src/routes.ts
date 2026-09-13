import type { WorldKbPatchEntityRequest } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError, routeNotMigrated } from './errors.js';
import {
  getCoreChanges,
  getWorldKbCandidates,
  getWorldKbGraph,
  hostQuery,
  parseIncludeSuggested,
  parsePositiveInt,
  patchWorldKbEntity,
} from './world-kb.js';

export type RouteTier = 'unguarded' | 'tier1' | 'tier2' | 'provider_stream';

export interface RouteMatch {
  tier: RouteTier;
  worldId?: string;
  sessionId?: string;
  operationId?: string;
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
): Promise<unknown> {
  const route = matchRoute(method, pathname);
  if (!route) {
    throw routeNotMigrated(pathname);
  }

  switch (route.tier) {
    case 'unguarded':
      return handleUnguarded(service, pathname);
    case 'tier1':
      return handleTier1(service, method, pathname, body);
    case 'tier2':
      return handleTier2(service, method, pathname, searchParams, body, route);
    case 'provider_stream':
      throw routeNotMigrated(pathname);
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
      workspace_initialized: true,
      acp: {
        tool_execution_enabled: !service.domainOnly,
        active_sessions: 0,
        total_tool_executions: 0,
      },
      runtime_mode: service.domainOnly ? 'domain_only' : 'provider_enabled',
    };
  }
  if (pathname === '/v1/daemon/runtime/cert-fingerprint') {
    return {
      fingerprint: service.tlsFingerprint ?? '',
      algorithm: 'sha256',
      created_at: service.tlsFingerprint ? service.startedAt : null,
    };
  }
  if (pathname === '/v1/daemon/daemon/status') {
    return {
      schema_version: 2,
      lifecycle_state: 'running',
      version: '0.1.0',
      implementation_scope: 'standalone-service (P4-T1)',
      uptime_ms: Date.now() - Date.parse(service.startedAt),
      started_at: service.startedAt,
      pid: process.pid,
      degraded: { subsystems: [], reasons: [] },
      subsystems: {
        http: { status: 'up', last_check_ms: 0 },
        db: { status: 'up', last_check_ms: 0 },
        engine: { status: service.domainOnly ? 'down' : 'up', last_check_ms: 0 },
      },
      exit_code: null,
      last_error: null,
    };
  }
  throw routeNotMigrated(pathname);
}

async function tryHostQuery(service: ServiceCore, request: Parameters<typeof hostQuery>[1]) {
  try {
    return await hostQuery(service, request);
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.message.includes('host not started') || mapped.message.includes('provider port unavailable')) {
      return null;
    }
    throw mapped;
  }
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
    const limit = parsePositiveInt(searchParams.get('limit'), 'limit');
    const cursor = searchParams.get('cursor') ?? undefined;
    return getWorldKbCandidates(service, route.worldId, limit, cursor);
  }
  if (method === 'GET' && pathname === '/v1/daemon/core/changes') {
    const afterSequence = searchParams.get('after_sequence');
    if (!afterSequence) {
      throw new HttpError(400, 'invalid_input', 'after_sequence is required');
    }
    const limit = parsePositiveInt(searchParams.get('limit'), 'limit');
    return getCoreChanges(service, {
      after_sequence: afterSequence,
      ...(limit !== undefined ? { limit } : {}),
    });
  }
  if (method === 'GET' && pathname === '/v1/daemon/agent-host/sessions') {
    const response = await hostQuery(service, {
      query: 'list_sessions',
      limit: parsePositiveInt(searchParams.get('limit'), 'limit'),
      cursor: searchParams.get('cursor') ?? undefined,
    });
    return response.sessions ?? { items: [], pagination: { limit: 50, has_more: false, next_cursor: null } };
  }
  if (method === 'GET' && route.sessionId) {
    const response = await hostQuery(service, {
      query: 'get_session',
      session_id: route.sessionId,
    });
    if (!response.session) {
      throw new HttpError(404, 'not_found', `session ${route.sessionId} not found`);
    }
    return response.session;
  }
  if (method === 'GET' && route.operationId) {
    const response = await hostQuery(service, {
      query: 'get_operation',
      operation_id: route.operationId,
    });
    if (!response.operation) {
      throw new HttpError(404, 'not_found', `operation ${route.operationId} not found`);
    }
    return response.operation;
  }
  throw routeNotMigrated(pathname);
}
