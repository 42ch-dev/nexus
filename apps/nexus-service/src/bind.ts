import { isIP } from 'node:net';
import type { ResolvedServiceConfig } from './config.js';
import { HttpError } from './errors.js';

/** Match daemon `is_loopback_host` semantics for bind admission. */
export function isLoopbackBindHost(host: string): boolean {
  const trimmed = host.trim().replace(/^\[|\]$/g, '');
  if (trimmed.toLowerCase() === 'localhost') {
    return true;
  }
  const kind = isIP(trimmed);
  if (kind === 4) {
    return trimmed.startsWith('127.');
  }
  if (kind === 6) {
    const normalized = trimmed.toLowerCase();
    return normalized === '::1' || normalized === '0:0:0:0:0:0:0:1';
  }
  return false;
}

/**
 * Fail closed before native open/listen for non-loopback publication. A unix
 * transport performs no TCP bind: access is guarded by the 0700 run directory
 * holding the socket, so the remote-bind/TLS policy does not apply to it.
 */
export function validateStartupBind(config: ResolvedServiceConfig): void {
  if (config.transport === 'unix') {
    return;
  }
  if (isLoopbackBindHost(config.host)) {
    return;
  }
  if (!config.allowRemote) {
    throw new HttpError(
      403,
      'forbidden',
      'non-loopback connections require --allow-remote and TLS',
    );
  }
  if (!config.tlsCert || !config.tlsKey) {
    throw new HttpError(403, 'forbidden', 'remote access requires --tls-cert and --tls-key');
  }
  if (!config.apiKey) {
    throw new HttpError(401, 'auth_required', 'remote bind requires NEXUS42_DAEMON_API_KEY');
  }
}
