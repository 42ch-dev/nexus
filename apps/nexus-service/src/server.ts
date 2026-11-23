import {
  createServer as createHttpServer,
  ServerResponse as HttpServerResponse,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from 'node:http';
import {
  createServer as createHttpsServer,
} from 'node:https';
import { chmodSync, existsSync, lstatSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import net from 'node:net';
import { randomUUID } from 'node:crypto';
import type { CoreCloseReport, CoreServiceDiscovery, CoreServiceStopRequest } from '@42ch/nexus-contracts';
import {
  formatHttpAuthority,
  HEADER_DEADLINE_CHECK_MS,
  HEADER_READ_TIMEOUT_MS,
  MAX_REQUEST_BYTES,
  REQUEST_READ_TIMEOUT_MS,
  resolveAllowedOrigins,
  resolveSseSocketHighWaterMark,
  type ResolvedServiceConfig,
} from './config.js';
import { HttpError, mapNativeError, stringifyJsonSafe, toErrorBody } from './errors.js';
import { AuthSessionStore, sessionCookie } from './auth-session.js';
import { createDiscoveryRecord } from './discovery.js';
import type { ServiceCore } from './lifecycle.js';
import {
  checkApiKey,
  checkOrigin,
  corsAllowHeaders,
  corsAllowMethods,
  validateBindPolicy,
} from './security.js';
import { handleRoute, matchRoute } from './routes.js';
import { createStaticFileHandler } from './static-files.js';
import { releaseSessionSubscriber, reserveSessionSubscriber } from './sse.js';

export interface RunningService {
  /** HTTP(S) base URL; null on the unix transport (use `endpoint.path`). */
  readonly url: string | null;
  /** The tagged wire endpoint actually bound after listen. */
  readonly endpoint: CoreServiceDiscovery['endpoint'];
  /** The v1 discovery record published for this instance (§7). */
  readonly discovery: CoreServiceDiscovery;
  /**
   * null on the unix transport, where there is no HTTP URL to print.
   */
  readonly bootstrapUrl: string | null;
  /** Same-process observability seam for scoped tests; not part of the wire API. */
  readonly service: ServiceCore;
  close(): Promise<CoreCloseReport>;
}

/** Instance-bound operator stop endpoint (architecture §7). */
const RUNTIME_STOP_PATH = '/v1/daemon/runtime/stop';

/**
 * the closed v1 discovery record of *this* service instance, under the same
 * API-key/loopback admission as the operator stop path. A client compares it
 * against the private `<user_home>/.nexus42/run/service.json` record before
 * attaching, so the response is derived from this process's own state — never
 * read back from the replaceable published file.
 */
const RUNTIME_DISCOVERY_PATH = '/v1/daemon/runtime/discovery';

const AUTH_BOOTSTRAP_PATH = '/__bootstrap';

/** §6.2 copy, verbatim: the bootstrap route's own 401 bodies. */
const BOOTSTRAP_REPLAY_MESSAGE =
  '访问链接已被使用。请回到服务终端复制最新打印的访问链接。';
const BOOTSTRAP_EXPIRED_MESSAGE =
  '访问链接已过期。请回到服务终端复制最新打印的访问链接。';

/** Port the listener actually serves; 0 resolves to the kernel-assigned port. */
function boundPort(server: Server, config: ResolvedServiceConfig): number {
  const address = server.address();
  return address && typeof address === 'object' ? address.port : config.port;
}

/** Base URL for a bound HTTP listener on this config's host. */
function httpUrlFor(config: ResolvedServiceConfig, port: number): string {
  const protocol = config.tlsCert && config.tlsKey ? 'https' : 'http';
  return `${protocol}://${formatHttpAuthority(config.host, port)}`;
}

/** Tagged wire endpoint of the running listener (the value the record publishes). */
function boundEndpoint(server: Server, config: ResolvedServiceConfig): CoreServiceDiscovery['endpoint'] {
  if (config.transport === 'unix') {
    if (!config.socketPath) {
      throw new HttpError(500, 'internal', 'unix transport requires a socket path');
    }
    return { transport: 'unix', path: config.socketPath };
  }
  return { transport: 'http', url: httpUrlFor(config, boundPort(server, config)) };
}

function discoveryRecordFor(
  service: ServiceCore,
  config: ResolvedServiceConfig,
  server: Server,
): CoreServiceDiscovery {
  return createDiscoveryRecord({
    instanceId: service.instanceId,
    userHome: config.home,
    endpoint: boundEndpoint(server, config),
    tlsFingerprint: service.tlsFingerprint?.fingerprint ?? null,
    readiness: service.workspaceInitialized ? 'ready' : 'uninitialized',
    creatorId: service.creatorId,
    workspaceSlug: service.workspaceSlug,
    engineEpoch: service.engineEpoch,
  });
}

function parseStopRequest(body: unknown): CoreServiceStopRequest {
  if (body === null || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'invalid_input', 'stop request must be a JSON object');
  }
  const { expected_instance_id, expected_engine_epoch, ...extra } = body as Record<string, unknown>;
  if (typeof expected_instance_id !== 'string' || expected_instance_id.length === 0) {
    throw new HttpError(400, 'invalid_input', 'stop request requires expected_instance_id');
  }
  if (
    expected_engine_epoch !== null &&
    (typeof expected_engine_epoch !== 'number' ||
      !Number.isSafeInteger(expected_engine_epoch) ||
      expected_engine_epoch < 0)
  ) {
    throw new HttpError(
      400,
      'invalid_input',
      'expected_engine_epoch must be a non-negative integer or null',
    );
  }
  if (Object.keys(extra).length > 0) {
    throw new HttpError(400, 'invalid_input', 'stop request accepts no unknown fields');
  }
  return { expected_instance_id, expected_engine_epoch };
}

async function clearStaleUnixSocket(path: string): Promise<void> {
  if (!existsSync(path)) return;
  const live = await new Promise<boolean>((resolveProbe) => {
    const probe = net.connect(path);
    const settle = (value: boolean) => {
      probe.destroy();
      resolveProbe(value);
    };
    probe.once('connect', () => settle(true));
    probe.once('error', () => settle(false));
    setTimeout(() => settle(false), 500).unref();
  });
  if (live) {
    throw new HttpError(503, 'busy', 'another service is listening on this socket path');
  }
  rmSync(path, { force: true });
}

/** The socket file itself is locked down to owner-only after a successful bind. */
const SOCKET_MODE = 0o600;

/**
 * the socket must live in a real 0700 directory, so a shared parent such as
 * `/tmp` can never host a keyless service socket.
 */
function assertPrivateSocketParent(socketPath: string): void {
  const parent = dirname(socketPath);
  let stats;
  try {
    stats = lstatSync(parent);
  } catch {
    throw new HttpError(403, 'forbidden', `unix socket parent directory does not exist: ${parent}`);
  }
  if (!stats.isDirectory()) {
    throw new HttpError(403, 'forbidden', `unix socket parent is not a real directory: ${parent}`);
  }
  if ((stats.mode & 0o777) !== 0o700) {
    throw new HttpError(
      403,
      'forbidden',
      `unix socket parent directory must be private (0700): ${parent}`,
    );
  }
}

/**
 * `new URL()` collapses `%2e%2e` segments before the static boundary could
 * refuse them, so the static handler receives the undecoded target minus query.
 * An absolute-form target (`GET http://host/path`) has its scheme + authority
 * stripped so it resolves to the same file as the origin-form request.
 */
function rawRequestPath(target: string | undefined): string {
  let raw = target ?? '/';
  if (!raw.startsWith('/')) {
    const schemeEnd = raw.indexOf('://');
    if (schemeEnd !== -1) {
      const pathStart = raw.indexOf('/', schemeEnd + 3);
      raw = pathStart === -1 ? '/' : raw.slice(pathStart);
    }
  }
  const query = raw.indexOf('?');
  return query === -1 ? raw : raw.slice(0, query);
}

function collapseDotSegments(decoded: string): string {
  const rooted = decoded.startsWith('/');
  const stack: string[] = [];
  for (const segment of (rooted ? decoded.slice(1) : decoded).split('/')) {
    if (segment === '' || segment === '.') continue;
    if (segment === '..') stack.pop();
    else stack.push(segment);
  }
  return `${rooted ? '/' : ''}${stack.join('/')}`;
}

function requestId(req: IncomingMessage): string {
  const header = req.headers['x-request-id'];
  const value = Array.isArray(header) ? header[0] : header;
  return value && value.trim().length > 0 ? value.trim() : `req_${randomUUID()}`;
}

function writeCors(res: ServerResponse, origin: string | undefined, allowed: readonly string[]): void {
  if (origin && allowed.includes(origin)) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Access-Control-Allow-Credentials', 'true');
    res.setHeader('Vary', 'Origin');
  }
  res.setHeader('Access-Control-Allow-Methods', corsAllowMethods());
  res.setHeader('Access-Control-Allow-Headers', corsAllowHeaders());
}

async function readBody(req: IncomingMessage): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let total = 0;
  return await new Promise((resolve, reject) => {
    const bodyTimer = setTimeout(() => {
      reject(new HttpError(408, 'invalid_input', 'request body timed out'));
    }, REQUEST_READ_TIMEOUT_MS);

    req.on('data', (chunk: Buffer) => {
      total += chunk.length;
      if (total > MAX_REQUEST_BYTES) {
        reject(new HttpError(413, 'input_too_large', 'request body exceeds 1 MiB'));
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      clearTimeout(bodyTimer);
      resolve(Buffer.concat(chunks));
    });
    req.on('error', (error) => {
      clearTimeout(bodyTimer);
      reject(error);
    });
  });
}

function parseJsonBody(buffer: Buffer, method: string): unknown {
  if (buffer.length === 0) {
    return undefined;
  }
  try {
    return JSON.parse(buffer.toString('utf8')) as unknown;
  } catch {
    throw new HttpError(400, 'invalid_input', `${method} body must be valid JSON`);
  }
}

function sendJson(
  res: ServerResponse,
  status: number,
  payload: unknown,
  requestIdValue: string,
  origin: string | undefined,
  allowed: readonly string[],
): void {
  writeCors(res, origin, allowed);
  res.statusCode = status;
  res.setHeader('X-Request-Id', requestIdValue);
  if (status === 204) {
    res.end();
    return;
  }
  res.setHeader('Content-Type', 'application/json; charset=utf-8');
  res.end(stringifyJsonSafe(payload));
}

function sendError(
  res: ServerResponse,
  error: HttpError,
  requestIdValue: string,
  origin: string | undefined,
  allowed: readonly string[],
): void {
  sendJson(res, error.status, toErrorBody(error, requestIdValue), requestIdValue, origin, allowed);
}

export function createServiceServer(
  config: ResolvedServiceConfig,
  service: ServiceCore,
  closeFn: () => Promise<CoreCloseReport>,
  authSessions: AuthSessionStore = new AuthSessionStore(),
): { server: Server } {
  const protocol = config.tlsCert && config.tlsKey ? 'https' : 'http';
  const url = `${protocol}://${formatHttpAuthority(config.host, config.port)}`;

  const staticHandler = config.webDist
    ? createStaticFileHandler({ root: config.webDist, fallback: join(config.webDist, 'index.html') })
    : null;

  const handler = async (req: IncomingMessage, res: ServerResponse) => {
    const id = requestId(req);
    const origin = req.headers.origin;
    const remoteAddress = req.socket.remoteAddress;
    const method = req.method ?? 'GET';
    const urlObj = new URL(req.url ?? '/', url);

    try {
      writeCors(res, origin, config.allowedOrigins);
      if (method === 'OPTIONS') {
        res.statusCode = 204;
        res.setHeader('X-Request-Id', id);
        res.end();
        return;
      }

      const originFailure = checkOrigin(req, config.allowedOrigins, protocol);
      if (originFailure) {
        sendError(res, new HttpError(originFailure.status, originFailure.code, originFailure.message, originFailure.details), id, origin, config.allowedOrigins);
        return;
      }

      if (method === 'GET' && urlObj.pathname === AUTH_BOOTSTRAP_PATH) {
        const bindFailure = validateBindPolicy(config, remoteAddress, true);
        if (bindFailure) {
          sendError(
            res,
            new HttpError(bindFailure.status, bindFailure.code, bindFailure.message, bindFailure.details),
            id,
            origin,
            config.allowedOrigins,
          );
          return;
        }
        const token = urlObj.searchParams.get('token') ?? '';
        if (!authSessions.consumeToken(token)) {
          res.statusCode = 401;
          res.setHeader('X-Request-Id', id);
          res.setHeader('Content-Type', 'text/plain; charset=utf-8');
          res.end(
            authSessions.wasConsumed(token) ? BOOTSTRAP_REPLAY_MESSAGE : BOOTSTRAP_EXPIRED_MESSAGE,
          );
          return;
        }
        res.statusCode = 302;
        res.setHeader(
          'Set-Cookie',
          sessionCookie(authSessions.issueSession(), Boolean(config.tlsCert && config.tlsKey)),
        );
        res.setHeader('Location', '/');
        res.setHeader('X-Request-Id', id);
        res.end();
        return;
      }

      if (method === 'POST' && urlObj.pathname === RUNTIME_STOP_PATH) {
        const auth = checkApiKey(req, config, remoteAddress, authSessions);
        if (!auth.ok) {
          sendError(
            res,
            new HttpError(auth.status, auth.code, auth.message, auth.details),
            id,
            origin,
            config.allowedOrigins,
          );
          return;
        }
        const bodyBuffer = await readBody(req);
        const stopRequest = parseStopRequest(parseJsonBody(bodyBuffer, method));
        const matches =
          stopRequest.expected_instance_id === service.instanceId &&
          stopRequest.expected_engine_epoch === service.engineEpoch;
        if (!matches) {
          throw new HttpError(
            409,
            'instance_conflict',
            'stop request does not match this service instance',
            {
              expected_instance_id: stopRequest.expected_instance_id,
              expected_engine_epoch: stopRequest.expected_engine_epoch,
              actual_instance_id: service.instanceId,
              actual_engine_epoch: service.engineEpoch,
            },
          );
        }
        res.once('finish', () => {
          void closeFn().catch(() => undefined);
        });
        sendJson(res, 200, { status: 'stopping' }, id, origin, config.allowedOrigins);
        return;
      }

      if (method === 'GET' && urlObj.pathname === RUNTIME_DISCOVERY_PATH) {
        const auth = checkApiKey(req, config, remoteAddress, authSessions);
        if (!auth.ok) {
          sendError(
            res,
            new HttpError(auth.status, auth.code, auth.message, auth.details),
            id,
            origin,
            config.allowedOrigins,
          );
          return;
        }
        sendJson(res, 200, discoveryRecordFor(service, config, server), id, origin, config.allowedOrigins);
        return;
      }

      const route = matchRoute(method, urlObj.pathname);
      if (!route) {
        const staticTarget = rawRequestPath(req.url);
        let decodedTarget: string | null = null;
        try {
          decodedTarget = decodeURIComponent(staticTarget);
        } catch {
        }
        const normalizedTarget = decodedTarget?.startsWith('/')
          ? collapseDotSegments(decodedTarget)
          : null;
        const apiDomain =
          urlObj.pathname.startsWith('/v1/') ||
          normalizedTarget === '/v1' ||
          (normalizedTarget?.startsWith('/v1/') ?? false);
        if (staticHandler && !apiDomain) {
          await staticHandler(res, method, staticTarget);
          return;
        }
        sendError(res, new HttpError(501, 'route_not_migrated', `Route is not migrated: ${urlObj.pathname}`), id, origin, config.allowedOrigins);
        return;
      }

      if (route.tier !== 'unguarded') {
        const auth = checkApiKey(req, config, remoteAddress, authSessions);
        if (!auth.ok) {
          sendError(
            res,
            new HttpError(auth.status, auth.code, auth.message, auth.details),
            id,
            origin,
            config.allowedOrigins,
          );
          return;
        }
      }


      const bodyBuffer = method === 'GET' || method === 'HEAD' ? Buffer.alloc(0) : await readBody(req);
      const body = parseJsonBody(bodyBuffer, method);
      const lastEventId = req.headers['last-event-id'];
      const result = await handleRoute(
        service,
        method,
        urlObj.pathname,
        urlObj.searchParams,
        body,
        typeof lastEventId === 'string' ? { lastEventId } : {},
      );
      if (result.kind === 'sse') {
        const subscriberSessionId =
          route.sessionId && method === 'GET' && urlObj.pathname.endsWith('/events')
            ? route.sessionId
            : null;
        if (subscriberSessionId) {
          reserveSessionSubscriber(subscriberSessionId);
          let released = false;
          const release = () => {
            if (released) return;
            released = true;
            releaseSessionSubscriber(subscriberSessionId);
          };
          const subscriberSocket = req.socket;
          subscriberSocket?.once('close', release);
          try {
            res.setHeader('X-Request-Id', id);
            writeCors(res, origin, config.allowedOrigins);
            await result.run(res);
          } finally {
            subscriberSocket?.off('close', release);
            release();
          }
          return;
        }
        res.setHeader('X-Request-Id', id);
        writeCors(res, origin, config.allowedOrigins);
        await result.run(res);
        return;
      }
      sendJson(res, result.status ?? 200, result.body, id, origin, config.allowedOrigins);
    } catch (error) {
      const mapped = mapNativeError(error);
      if (!res.headersSent) {
        sendError(res, mapped, id, origin, config.allowedOrigins);
      } else if (!res.writableEnded) {
        res.end();
      }
    }
  };

  class BoundedSseHttpResponse extends HttpServerResponse {
    constructor(req: IncomingMessage) {
      // @ts-expect-error Node >=18 supports OutgoingMessage highWaterMark options.
      super(req, { highWaterMark: resolveSseSocketHighWaterMark() });
    }
  }

  const server =
    config.tlsCert && config.tlsKey
      ? createHttpsServer(
          { cert: config.tlsCert, key: config.tlsKey, ServerResponse: BoundedSseHttpResponse },
          handler,
        )
      : createHttpServer({ ServerResponse: BoundedSseHttpResponse }, handler);

  server.headersTimeout = HEADER_READ_TIMEOUT_MS;
  server.requestTimeout = REQUEST_READ_TIMEOUT_MS;
  (server as Server & { connectionsCheckingInterval: number }).connectionsCheckingInterval =
    HEADER_DEADLINE_CHECK_MS;


  return { server };
}

/**
 * Bind the listener and report the wire endpoint actually served: the tagged
 * http URL (with the bound port, so port 0 works) or the unix socket path.
 */
export async function listenServer(
  server: Server,
  config: ResolvedServiceConfig,
): Promise<CoreServiceDiscovery['endpoint']> {
  if (config.transport === 'unix') {
    const socketPath = config.socketPath;
    if (!socketPath) {
      throw new HttpError(500, 'internal', 'unix transport requires a socket path');
    }
    assertPrivateSocketParent(socketPath);
    await clearStaleUnixSocket(socketPath);
    await new Promise<void>((resolveListen, rejectListen) => {
      server.once('error', rejectListen);
      server.listen(socketPath, () => resolveListen());
    });
    try {
      chmodSync(socketPath, SOCKET_MODE);
    } catch {
      throw new HttpError(500, 'internal', 'unix socket permissions cannot be enforced');
    }
    return { transport: 'unix', path: socketPath };
  }
  await new Promise<void>((resolveListen, rejectListen) => {
    server.once('error', rejectListen);
    server.listen(config.port, config.host, () => resolveListen());
  });
  const port = boundPort(server, config);
  if (port !== config.port) {
    config.allowedOrigins = [
      ...new Set([...config.allowedOrigins, ...resolveAllowedOrigins(port, config.host)]),
    ];
  }
  return {
    transport: 'http',
    url: httpUrlFor(config, port),
  };
}
