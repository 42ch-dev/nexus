import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export interface ServiceOptions {
  home: string;
  host: string;
  port: number;
  allowRemote: boolean;
  domainOnly?: boolean;
  tlsCert?: string;
  tlsKey?: string;
}

export interface ResolvedServiceConfig extends ServiceOptions {
  home: string;
  apiKey: string | null;
  allowedOrigins: string[];
}

export const MAX_REQUEST_BYTES = 1024 * 1024;
export const HEADER_READ_TIMEOUT_MS = 5_000;
export const REQUEST_READ_TIMEOUT_MS = 10_000;
export const CLOSE_BUDGET_MS = 5_000;

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 8421;

export function resolveAllowedOrigins(port: number, host: string): string[] {
  const origins = new Set<string>([
    `http://127.0.0.1:${port}`,
    `http://localhost:${port}`,
    'tauri://localhost',
    'http://tauri.localhost',
    'http://localhost:5173',
    'http://127.0.0.1:5173',
  ]);
  const trimmed = host.trim();
  if (trimmed === 'localhost' || trimmed === '127.0.0.1' || trimmed === '::1') {
    origins.add(`http://${trimmed}:${port}`);
  }
  const envOrigins = process.env.NEXUS_DAEMON_ALLOWED_ORIGINS;
  if (envOrigins) {
    for (const raw of envOrigins.split(',')) {
      const origin = raw.trim();
      if (origin) origins.add(origin);
    }
  }
  return [...origins];
}

export function resolveServiceConfig(options: ServiceOptions): ResolvedServiceConfig {
  const home = resolve(options.home);
  const host = options.host.trim() || DEFAULT_HOST;
  const port = options.port;
  const rawKey = process.env.NEXUS42_DAEMON_API_KEY?.trim() ?? '';
  const apiKey = rawKey.length > 0 ? rawKey : null;
  return {
    ...options,
    home,
    host,
    port,
    apiKey,
    allowedOrigins: resolveAllowedOrigins(port, host),
  };
}

export interface CliArgs {
  home: string;
  host: string;
  port: number;
  domainOnly: boolean;
  allowRemote: boolean;
  tlsCert?: string;
  tlsKey?: string;
}

export function parseCliArgs(argv: string[]): CliArgs {
  const args = [...argv];
  let home = '';
  let host = DEFAULT_HOST;
  let port = DEFAULT_PORT;
  let domainOnly = false;
  let allowRemote = false;
  let tlsCert: string | undefined;
  let tlsKey: string | undefined;

  for (let i = 0; i < args.length; i += 1) {
    const token = args[i];
    switch (token) {
      case '--home':
        home = args[++i] ?? '';
        break;
      case '--host':
        host = args[++i] ?? DEFAULT_HOST;
        break;
      case '--port':
        port = Number.parseInt(args[++i] ?? '', 10);
        break;
      case '--domain-only':
        domainOnly = true;
        break;
      case '--allow-remote':
        allowRemote = true;
        break;
      case '--tls-cert':
        tlsCert = resolve(args[++i] ?? '');
        break;
      case '--tls-key':
        tlsKey = resolve(args[++i] ?? '');
        break;
      default:
        throw new Error(`unknown argument: ${token}`);
    }
  }

  if (!home) {
    throw new Error('--home <raw-home> is required');
  }
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error('--port must be an integer between 1 and 65535');
  }
  if ((tlsCert && !tlsKey) || (!tlsCert && tlsKey)) {
    throw new Error('--tls-cert and --tls-key must be supplied together');
  }

  return { home, host, port, domainOnly, allowRemote, tlsCert, tlsKey };
}

export function loadTlsMaterial(certPath: string, keyPath: string): { cert: string; key: string } {
  return {
    cert: readFileSync(certPath, 'utf8'),
    key: readFileSync(keyPath, 'utf8'),
  };
}
