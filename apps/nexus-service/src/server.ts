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
import { dirname } from 'node:path';
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
import type { ServiceCore } from './lifecycle.js';
import { checkApiKey, checkOrigin, corsAllowHeaders, corsAllowMethods } from './security.js';
import { handleRoute, matchRoute } from './routes.js';
import { releaseSessionSubscriber, reserveSessionSubscriber } from './sse.js';

export interface RunningService {
  /** HTTP(S) base URL; null on the unix transport (use `endpoint.path`). */
  readonly url: string | null;
  /** The tagged wire endpoint actually bound after listen. */
  readonly endpoint: CoreServiceDiscovery['endpoint'];
  /** The v1 discovery record published for this instance (§7). */
  readonly discovery: CoreServiceDiscovery;
  /** Same-process observability seam for scoped tests; not part of the wire API. */
  readonly service: ServiceCore;
  close(): Promise<CoreCloseReport>;
}

/** Instance-bound operator stop endpoint (architecture §7). */
const RUNTIME_STOP_PATH = '/v1/daemon/runtime/stop';

/**
 * Closed admission of the generated `CoreServiceStopRequest` shape. Rejected
 * here because the stop path never crosses the native wire validator.
 */
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

/**
 * A leftover socket file from a crashed owner is removable only when nothing
 * answers on it; a live listener refuses the bind with `busy` instead of a
 * cryptic EADDRINUSE.
 */
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
 * The unix transport's only access guard is its parent directory (bind.ts):
 * the socket must live in a real 0700 directory, so a shared parent such as
 * `/tmp` can never host a keyless service socket.
 */
function assertPrivateSocketParent(socketPath: string): void {
  const parent = dirname(socketPath);
  let stats;
  try {
    // lstat: a symlinked parent is not a private directory, it is a redirect.
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

function requestId(req: IncomingMessage): string {
  const header = req.headers['x-request-id'];
  const value = Array.isArray(header) ? header[0] : header;
  return value && value.trim().length > 0 ? value.trim() : `req_${randomUUID()}`;
}

function writeCors(res: ServerResponse, origin: string | undefined, allowed: readonly string[]): void {
  if (origin && allowed.includes(origin)) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Vary', 'Origin');
  }
  res.setHeader('Access-Control-Allow-Methods', corsAllowMethods());
  res.setHeader('Access-Control-Allow-Headers', corsAllowHeaders());
}

/**
 * Body/read deadline only. The header deadline is owned by the server's
 * `headersTimeout` (parsed before this handler runs), never by a timer that
 * can only start once the headers already arrived.
 */
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
  res.setHeader('Content-Type', 'application/json; charset=utf-8');
  res.setHeader('X-Request-Id', requestIdValue);
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
): { server: Server } {
  // URL base for relative request-URL parsing only; the wire endpoint is
  // reported from the bound listener, never from this placeholder.
  const protocol = config.tlsCert && config.tlsKey ? 'https' : 'http';
  const url = `${protocol}://${formatHttpAuthority(config.host, config.port)}`;

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

      const originFailure = checkOrigin(req, config.allowedOrigins);
      if (originFailure) {
        sendError(res, new HttpError(originFailure.status, originFailure.code, originFailure.message, originFailure.details), id, origin, config.allowedOrigins);
        return;
      }

      // Instance-bound operator stop (architecture §7): same guarded-tier
      // admission as domain routes, handled before the domain route matcher
      // because it controls the transport itself rather than a family.
      if (method === 'POST' && urlObj.pathname === RUNTIME_STOP_PATH) {
        const auth = checkApiKey(req, config, remoteAddress);
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
          // Mismatch returns conflict and performs no stop: a stale record
          // never stops its replacement, and a read-only attach never kills
          // an unowned service.
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
        // Respond BEFORE the close runs: the close owner force-destroys
        // sockets, including the one carrying this response, so the close is
        // deferred until the response is flushed ('finish') and its result is
        // observed by polling the health route (§7 semantics).
        res.once('finish', () => {
          void closeFn().catch(() => undefined);
        });
        sendJson(res, 200, { status: 'stopping' }, id, origin, config.allowedOrigins);
        return;
      }

      const route = matchRoute(method, urlObj.pathname);
      if (!route) {
        sendError(res, new HttpError(501, 'route_not_migrated', `Route is not migrated: ${urlObj.pathname}`), id, origin, config.allowedOrigins);
        return;
      }

      if (route.tier !== 'unguarded') {
        const auth = checkApiKey(req, config, remoteAddress);
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
      const result = await handleRoute(service, method, urlObj.pathname, urlObj.searchParams, body);
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
          // Safety net for a stream that outlives this handler. Remove it on
          // normal completion so repeated SSE requests over one keep-alive
          // socket do not accumulate dead listeners.
          const subscriberSocket = req.socket;
          subscriberSocket?.once('close', release);
          try {
            res.setHeader('X-Request-Id', id);
            writeCors(res, origin, config.allowedOrigins);
            await result.run(res);
          } finally {
            // Primary release: HTTP keep-alive holds the TCP socket open long
            // after an SSE response completed, so a completed or thrown stream
            // must free its admission here, not at socket close.
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
      sendJson(res, 200, result.body, id, origin, config.allowedOrigins);
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

  // Frozen ordinary HTTP bounds: headers 5s (parser-owned, before the handler)
  // and request read 10s (`requestTimeout` >= `headersTimeout`).
  server.headersTimeout = HEADER_READ_TIMEOUT_MS;
  server.requestTimeout = REQUEST_READ_TIMEOUT_MS;
  // Node only *checks* those deadlines on this interval; without it the frozen
  // 5s header bound would not be enforced until the 30s default sweep.
  // `connectionsCheckingInterval` is a runtime server property (Node >= 18.7);
  // the bundled @types/node does not declare it on `Server`.
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
  const bound = server.address();
  const port = bound && typeof bound === 'object' ? bound.port : config.port;
  if (port !== config.port) {
    // The allowlist was frozen from the *requested* port; with port 0 the
    // kernel picked the real one. Extend it with the actually-served origin
    // before the first request can run: the listening callback resolves into
    // this synchronous continuation ahead of any connection event, and the
    // handler reads `config.allowedOrigins` per request.
    config.allowedOrigins = [
      ...new Set([...config.allowedOrigins, ...resolveAllowedOrigins(port, config.host)]),
    ];
  }
  const protocol = config.tlsCert && config.tlsKey ? 'https' : 'http';
  return {
    transport: 'http',
    url: `${protocol}://${formatHttpAuthority(config.host, port)}`,
  };
}
