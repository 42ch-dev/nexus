import type { ServerResponse } from 'node:http';
import type { Socket } from 'node:net';
import type { ProxyOptions } from 'vite';

/** HTTP status when the dev/preview proxy cannot reach the daemon listener. */
export const DAEMON_PROXY_UNAVAILABLE_STATUS = 503;

const CONNECT_ERROR_CODES: Record<string, true> = {
  ECONNREFUSED: true,
  ECONNRESET: true,
  ENOTFOUND: true,
  EHOSTUNREACH: true,
  ENETUNREACH: true,
};

/**
 * True when an upstream connect failure means the daemon is not listening yet
 * (sidecar boot window) or loopback is unreachable.
 */
export function isDaemonProxyConnectError(err: unknown): boolean {
  if (!err || typeof err !== 'object') return false;
  const code = (err as NodeJS.ErrnoException).code;
  return typeof code === 'string' && CONNECT_ERROR_CODES[code] === true;
}

/**
 * Daemon API error envelope for proxy transport failures (matches F-E1 shape).
 */
export function buildDaemonProxyUnavailableBody(err: unknown): string {
  const code = isDaemonProxyConnectError(err) ? 'daemon_unavailable' : 'bad_gateway';
  const message =
    code === 'daemon_unavailable'
      ? 'Local daemon is not reachable on the configured port.'
      : 'Daemon API proxy failed.';
  return JSON.stringify({
    success: false,
    error: { code, message },
  });
}

/**
 * http-proxy `error` handler: avoid Vite's default empty HTTP 500 on
 * `ECONNREFUSED` while the sidecar is still booting (V1.134 P0).
 *
 * Vite 8's proxy passes either the HTTP `ServerResponse` or the raw websocket
 * `Socket` (its own error listener discriminates with `'req' in res`). A socket
 * has no HTTP response surface, so it is only ended; HTTP responses get the
 * JSON envelope once, respecting `headersSent` / `writableEnded`.
 */
export function handleDaemonProxyError(
  err: unknown,
  _req: unknown,
  res: ServerResponse | Socket,
): void {
  if (!res) return;
  if (!('writeHead' in res)) {
    res.end();
    return;
  }
  if (res.headersSent || res.writableEnded) return;
  const status = isDaemonProxyConnectError(err)
    ? DAEMON_PROXY_UNAVAILABLE_STATUS
    : 502;
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(buildDaemonProxyUnavailableBody(err));
}

/** Shared `/v1/daemon` proxy route for `vite dev` and `vite preview`. */
export function createDaemonProxyRoute(target: string): ProxyOptions {
  return {
    target,
    changeOrigin: false,
    configure: (proxy) => {
      proxy.on('error', handleDaemonProxyError);
    },
  };
}
