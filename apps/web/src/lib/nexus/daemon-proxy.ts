import type { ServerResponse } from 'node:http';
import type { ProxyOptions } from 'vite';

/** HTTP status when the dev/preview proxy cannot reach the daemon listener. */
export const DAEMON_PROXY_UNAVAILABLE_STATUS = 503;

const CONNECT_ERROR_CODES = new Set([
  'ECONNREFUSED',
  'ECONNRESET',
  'ENOTFOUND',
  'EHOSTUNREACH',
  'ENETUNREACH',
]);

/**
 * True when an upstream connect failure means the daemon is not listening yet
 * (sidecar boot window) or loopback is unreachable.
 */
export function isDaemonProxyConnectError(err: unknown): boolean {
  if (!err || typeof err !== 'object') return false;
  const code = (err as NodeJS.ErrnoException).code;
  return typeof code === 'string' && CONNECT_ERROR_CODES.has(code);
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

type ProxyResponse = Pick<ServerResponse, 'writeHead' | 'end'> & { headersSent?: boolean };

/**
 * http-proxy `error` handler: avoid Vite's default empty HTTP 500 on
 * `ECONNREFUSED` while the sidecar is still booting (V1.134 P0).
 */
export function handleDaemonProxyError(
  err: unknown,
  _req: unknown,
  res: ProxyResponse,
): void {
  if (!res || res.headersSent) return;
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
