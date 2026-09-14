import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { isLoopbackBindHost } from './bind.js';

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
  tlsCertMtimeMs?: number;
}

export const MAX_REQUEST_BYTES = 1024 * 1024;
export const HEADER_READ_TIMEOUT_MS = 5_000;
/** Sweep interval that makes the frozen 5s header deadline observable on time. */
export const HEADER_DEADLINE_CHECK_MS = 500;
export const REQUEST_READ_TIMEOUT_MS = 10_000;
export const CLOSE_BUDGET_MS = 5_000;

/** Node Writable backpressure target for SSE socket handoff (architecture §7). */
export const SSE_SOCKET_HIGH_WATER_MARK = 64 * 1024;

export function resolveSseSocketHighWaterMark(): number {
  return resolvePositiveIntEnv('NEXUS_SSE_SOCKET_HWM', SSE_SOCKET_HIGH_WATER_MARK);
}

/** Defer the first provider pull so pauseImmediately clients can stall the socket. */
export function resolveSseFirstPullDelayMs(): number {
  return resolvePositiveIntEnv('NEXUS_SSE_FIRST_PULL_DELAY_MS', 450);
}
/** Max serialized UTF-8 bytes for one outstanding SSE data frame. */
export const SSE_MAX_OUTSTANDING_FRAME_BYTES = 512 * 1024;
/** Reserved control slot size for terminal/gap frames (not charged to data permits). */
export const SSE_RESERVED_CONTROL_BYTES = 4 * 1024;
/**
 * Node-owned retained/handoff frame byte budget (process-wide). The env var may
 * only *lower* this ceiling — a raised value is clamped back, so no override can
 * expand the frozen environment total.
 */
export const SSE_MAX_AGGREGATE_PENDING_BYTES_CEILING = 8 * 1024 * 1024;
export const SSE_MAX_AGGREGATE_PENDING_BYTES = resolveCeilEnv(
  'NEXUS_SSE_MAX_PENDING_BYTES',
  SSE_MAX_AGGREGATE_PENDING_BYTES_CEILING,
);
/** Handoff bytes reserved per live SSE socket until the socket is released. */
export const SSE_SOCKET_RESERVED_BYTES = 64 * 1024;
/** Process-wide live SSE subscriber cap (architecture §7). */
export const SSE_MAX_TOTAL_SUBSCRIBERS = 96;
/**
 * Max concurrently active (non-terminal) provider operations. Admitted at the
 * transport boundary as `busy` *before* the provider effect is dispatched.
 */
export const MAX_ACTIVE_PROVIDER_OPERATIONS = 6;
/** Frozen process-wide environment byte ceiling (architecture §7 / STREAM-2). */
export const ENVIRONMENT_TOTAL_CEILING_BYTES = 32 * 1024 * 1024;
/** Drain wait after write(false) before disconnecting a slow subscriber. */
export const SSE_DRAIN_TIMEOUT_MS = 2_000;
/** Live SSE subscribers per session (architecture §7). */
export const SSE_MAX_SUBSCRIBERS_PER_SESSION = 16;
/** Native pull batch: max events per nextProviderEvents call. */
export const PROVIDER_PULL_MAX_EVENTS = 16;
/** Native pull batch: max bytes per nextProviderEvents call. */
export const PROVIDER_PULL_MAX_BYTES = 262_144;
/** Retained provider data frames per operation hub (architecture §7). */
export const HUB_MAX_DATA_FRAMES = 64;
/** Retained provider data bytes per operation hub. */
export const HUB_MAX_DATA_BYTES = 1024 * 1024;
/** Max live SSE pending data frames per subscriber connection. */
export const SSE_MAX_PENDING_DATA_FRAMES = 16;
/** Max live SSE pending data bytes per subscriber connection. */
export const SSE_MAX_PENDING_DATA_BYTES = 1024 * 1024;
/** Max terminal operations retained in the HTTP registry after completion. */
export const REGISTRY_MAX_TERMINAL_OPERATIONS = 64;
/**
 * Bound on hubs that may simultaneously hold control frames: at most
 * {@link REGISTRY_MAX_TERMINAL_OPERATIONS} retained terminal hubs plus
 * {@link MAX_ACTIVE_PROVIDER_OPERATIONS} live hubs. Each hub holds at most one
 * terminal and one gap slot of ≤{@link SSE_RESERVED_CONTROL_BYTES}, so the
 * control reserve below is a hard ceiling that never competes with the data pool.
 */
export const ENVIRONMENT_MAX_TRACKED_HUBS =
  REGISTRY_MAX_TERMINAL_OPERATIONS + MAX_ACTIVE_PROVIDER_OPERATIONS;
/** Dedicated control-frame byte reserve (terminal+gap slots), separate from data. */
export const SSE_CONTROL_RESERVED_TOTAL_BYTES =
  ENVIRONMENT_MAX_TRACKED_HUBS * 2 * SSE_RESERVED_CONTROL_BYTES;
/** Default provider effect deadline. */
export const PROVIDER_DEFAULT_DEADLINE_MS = 30_000;

function resolvePositiveIntEnv(name: string, fallback: number): number {
  const raw = process.env[name]?.trim();
  if (!raw) return fallback;
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) return fallback;
  return parsed;
}

/**
 * Env override that can only tighten a frozen ceiling. An absent, malformed, or
 * *larger* value resolves to the ceiling, so no environment variable can raise
 * the process-wide bound the proof is computed against.
 */
function resolveCeilEnv(name: string, ceiling: number): number {
  const raw = process.env[name]?.trim();
  if (!raw) return ceiling;
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0 || parsed > ceiling) return ceiling;
  return parsed;
}

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 8421;

function isValidOriginHeaderValue(origin: string): boolean {
  try {
    const parsed = new URL(origin);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

/** Bracket IPv6 authorities for URL/origin construction. */
export function formatHttpAuthority(host: string, port: number): string {
  const trimmed = host.trim();
  if (trimmed.startsWith('[') || !trimmed.includes(':')) {
    return `${trimmed}:${port}`;
  }
  return `[${trimmed}]:${port}`;
}

export function httpOriginForBindHost(host: string, port: number): string {
  return `http://${formatHttpAuthority(host, port)}`;
}

export function resolveAllowedOrigins(port: number, host: string): string[] {
  const origins = new Set<string>([
    `http://127.0.0.1:${port}`,
    `http://localhost:${port}`,
    'tauri://localhost',
    'http://tauri.localhost',
    'http://localhost:5173',
    'http://127.0.0.1:5173',
  ]);

  if (isLoopbackBindHost(host)) {
    origins.add(httpOriginForBindHost(host, port));
  }

  const envOrigins = process.env.NEXUS_DAEMON_ALLOWED_ORIGINS;
  if (envOrigins) {
    for (const raw of envOrigins.split(',')) {
      const origin = raw.trim();
      if (!origin) continue;
      if (isValidOriginHeaderValue(origin)) {
        origins.add(origin);
      }
    }
  }

  return [...origins];
}

export function validateServiceHome(home: string): void {
  if (!existsSync(home)) {
    throw new Error(`home directory does not exist: ${home}`);
  }
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

export function loadTlsMaterial(
  certPath: string,
  keyPath: string,
): { cert: string; key: string; certPath: string; certMtimeMs: number } {
  return {
    cert: readFileSync(certPath, 'utf8'),
    key: readFileSync(keyPath, 'utf8'),
    certPath,
    certMtimeMs: statSync(certPath).mtimeMs,
  };
}
