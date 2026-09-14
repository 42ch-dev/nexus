import {
  createServer as createHttpServer,
  ServerResponse as HttpServerResponse,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import { randomUUID } from 'node:crypto';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import {
  formatHttpAuthority,
  HEADER_DEADLINE_CHECK_MS,
  HEADER_READ_TIMEOUT_MS,
  MAX_REQUEST_BYTES,
  REQUEST_READ_TIMEOUT_MS,
  resolveSseSocketHighWaterMark,
  type ResolvedServiceConfig,
} from './config.js';
import { HttpError, mapNativeError, stringifyJsonSafe, toErrorBody } from './errors.js';
import type { ServiceCore } from './lifecycle.js';
import { checkApiKey, checkOrigin, corsAllowHeaders, corsAllowMethods } from './security.js';
import { handleRoute, matchRoute } from './routes.js';
import { releaseSessionSubscriber, reserveSessionSubscriber } from './sse.js';

export interface RunningService {
  readonly url: string;
  /** Same-process observability seam for scoped tests; not part of the wire API. */
  readonly service: ServiceCore;
  close(): Promise<CoreCloseReport>;
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
): { server: Server; running: RunningService } {
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
          // Safety net for a stream that outlives this handler: the socket's
          // close still releases the admission.
          req.socket?.once('close', release);
          try {
            res.setHeader('X-Request-Id', id);
            writeCors(res, origin, config.allowedOrigins);
            await result.run(res);
          } finally {
            // Primary release: HTTP keep-alive holds the TCP socket open long
            // after an SSE response completed, so a completed or thrown stream
            // must free its admission here, not at socket close.
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

  const running: RunningService = {
    url,
    service,
    close: closeFn,
  };

  return { server, running };
}

export function listenServer(server: Server, host: string, port: number): Promise<void> {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, host, () => resolve());
  });
}
