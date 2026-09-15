import { existsSync, readFileSync, statSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { isLoopbackBindHost } from './bind.js';

export interface ServiceOptions {
  home: string;
  host: string;
  port: number;
  allowRemote: boolean;
  domainOnly?: boolean;
  tlsCert?: string;
  tlsKey?: string;
  /** Wire transport (architecture §7). Defaults to `http`. */
  transport?: 'http' | 'unix';
  /** Absolute Unix domain socket path; required for `unix`, forbidden for `http`. */
  socketPath?: string;
  /** ACP registry CDN base; must pass the legacy public-HTTPS validation. */
  cdnUrl?: string;
  /** Enable the embedded MCP lane for peers that request it. */
  embeddedMcp?: boolean;
}

export interface ResolvedServiceConfig extends ServiceOptions {
  home: string;
  apiKey: string | null;
  allowedOrigins: string[];
  tlsCertMtimeMs?: number;
  transport: 'http' | 'unix';
  socketPath?: string;
  cdnUrl?: string;
  embeddedMcp: boolean;
}
export const MAX_REQUEST_BYTES = 1024 * 1024;
export const HEADER_READ_TIMEOUT_MS = 5_000;
/** Sweep interval that makes the frozen 5s header deadline observable on time. */
export const HEADER_DEADLINE_CHECK_MS = 500;
export const REQUEST_READ_TIMEOUT_MS = 10_000;
export const CLOSE_BUDGET_MS = 5_000;

/** Node Writable backpressure target for SSE socket handoff (architecture §7). */
export const SSE_SOCKET_HIGH_WATER_MARK = 64 * 1024;

/**
 * Effective socket HWM. The env override may only *lower* the frozen 64 KiB
 * value — a raised value is clamped back — so the per-socket reservation in the
 * environment proof can never be invalidated by configuration.
 */
export function resolveSseSocketHighWaterMark(): number {
  return resolveCeilEnv('NEXUS_SSE_SOCKET_HWM', SSE_SOCKET_HIGH_WATER_MARK);
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

/** Blocklist ranges (private/loopback/link-local/metadata) for four IPv4 octets. */
function isBlockedIpv4Octets(octets: readonly number[]): boolean {
  const [a, b] = [octets[0], octets[1]];
  // Private, loopback, link-local ranges plus the 169.254.0.0/16 metadata
  // endpoint, mirroring the legacy `is_blocked_ip` V4 arm.
  return (
    a === 10 ||
    a === 127 ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 169 && b === 254)
  );
}

/** Parse an IPv6 literal (zone id stripped) into its eight 16-bit groups. */
function parseIpv6Groups(input: string): number[] | null {
  const zone = input.indexOf('%');
  const text = zone === -1 ? input : input.slice(0, zone);
  // Embedded IPv4 tail: `::ffff:127.0.0.1` and friends become two groups.
  const v4Tail = text.match(/:(\d+\.\d+\.\d+\.\d+)$/);
  let head = text;
  const tailGroups: number[] = [];
  if (v4Tail) {
    const octets = v4Tail[1].split('.').map(Number);
    if (octets.some((octet) => octet > 255)) return null;
    tailGroups.push((octets[0] << 8) | octets[1], (octets[2] << 8) | octets[3]);
    head = text.slice(0, text.length - v4Tail[1].length);
  }
  const halves = head.split('::');
  if (halves.length > 2) return null;
  const left = halves[0] === '' ? [] : halves[0].split(':');
  const right = halves.length === 2 && halves[1] !== '' ? halves[1].split(':') : [];
  if (halves.length === 1 && left.length + tailGroups.length !== 8) return null;
  if (halves.length === 2 && left.length + right.length + tailGroups.length > 7) return null;
  const compressed = halves.length === 2 ? 8 - left.length - right.length - tailGroups.length : 0;
  const hexGroups = [...left, ...Array<string>(compressed).fill('0'), ...right];
  if (hexGroups.length + tailGroups.length !== 8) return null;
  const parsedHex = hexGroups.map((group) =>
    /^[0-9a-fA-F]{1,4}$/.test(group) ? parseInt(group, 16) : Number.NaN,
  );
  if (parsedHex.some((value) => Number.isNaN(value))) return null;
  return [...parsedHex, ...tailGroups];
}

function isBlockedIpv6(groups: readonly number[]): boolean {
  // Loopback ::1 (longhand `0:0:0:0:0:0:0:1` included).
  if (groups.slice(0, 7).every((group) => group === 0) && groups[7] === 1) return true;
  // IPv4-mapped ::ffff:0:0/96 — decimal or hexadecimal tail alike — and the
  // deprecated IPv4-compatible ::/96 all judge by the embedded IPv4 address.
  if (groups.slice(0, 5).every((group) => group === 0) && (groups[5] === 0xffff || groups[5] === 0)) {
    return isBlockedIpv4Octets([groups[6] >> 8, groups[6] & 0xff, groups[7] >> 8, groups[7] & 0xff]);
  }
  // Unique-local fc00::/7 and link-local fe80::/10.
  const first = groups[0];
  return (first >= 0xfc00 && first <= 0xfdff) || (first >= 0xfe80 && first <= 0xfebf);
}

function isBlockedCdnIp(ip: string): boolean {
  const lowered = ip.trim().toLowerCase().replace(/^\[/, '').replace(/\]$/, '');
  if (lowered.includes(':')) {
    const groups = parseIpv6Groups(lowered);
    return groups !== null && isBlockedIpv6(groups);
  }
  // WHATWG normalization: dotted-decimal, octal, hexadecimal, and packed
  // 32-bit IPv4 spellings all canonicalize here before classification, so
  // `0177.0.0.1` or `2130706433` can no longer smuggle a loopback past the
  // decimal-only matcher.
  try {
    const hostname = new URL(`https://${lowered}/`).hostname;
    const v4 = hostname.match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)$/);
    if (v4) {
      const octets = v4.slice(1).map(Number);
      if (octets.some((octet) => octet > 255)) return false;
      return isBlockedIpv4Octets(octets);
    }
  } catch {
    // Not a URL host: nothing address-like to block.
  }
  return false;
}

/**
 * Legacy CDN validation (registry.rs `validate_cdn_url_static`), preserved
 * in spirit: public HTTPS URLs only, literal-IP hosts in private, loopback,
 * link-local or metadata ranges are refused. Hosts go through real URL/IP
 * parsing — bracketed IPv6 (`[::1]`, ULA, link-local), both IPv4-mapped
 * spellings, and exotic IPv4 forms are canonicalized before classification.
 */
export function validateCdnUrl(url: string): void {
  if (url.trim().length === 0) {
    throw new Error('--cdn-url must be a public HTTPS CDN URL (https://...); got empty value');
  }
  if (!url.startsWith('https://')) {
    throw new Error(`--cdn-url must be a public HTTPS CDN URL (https://...); got ${JSON.stringify(url)}`);
  }
  let host: string;
  try {
    // URL parsing strips IPv6 brackets and canonicalizes exotic IPv4 forms.
    host = new URL(url).hostname.replace(/^\[/, '').replace(/\]$/, '');
  } catch {
    throw new Error(`--cdn-url must be a public HTTPS CDN URL (https://...); got ${JSON.stringify(url)}`);
  }
  if (host.length === 0 || isBlockedCdnIp(host)) {
    throw new Error(`--cdn-url must be a public HTTPS CDN URL (https://...); got ${JSON.stringify(url)}`);
  }
}

/**
 * Resolve the transport selection (architecture §7): `http` by default; a
 * unix socket requires an absolute `socketPath`, and the socket path conflicts
 * with an explicitly selected HTTP bind.
 */
function resolveTransport(
  options: Pick<ServiceOptions, 'transport' | 'socketPath'>,
): { transport: 'http' | 'unix'; socketPath?: string } {
  const transport = options.transport ?? 'http';
  if (transport === 'unix') {
    const socketPath = options.socketPath;
    if (!socketPath) {
      throw new Error('--socket <absolute-path> is required for the unix transport');
    }
    if (!isAbsolute(socketPath)) {
      throw new Error(`--socket must be an absolute path: ${JSON.stringify(socketPath)}`);
    }
    return { transport, socketPath };
  }
  if (options.socketPath) {
    throw new Error('--socket conflicts with the http transport; pass --transport unix');
  }
  return { transport };
}

export function resolveServiceConfig(options: ServiceOptions): ResolvedServiceConfig {
  const home = resolve(options.home);
  const host = options.host.trim() || DEFAULT_HOST;
  const port = options.port;
  const rawKey = process.env.NEXUS42_DAEMON_API_KEY?.trim() ?? '';
  const apiKey = rawKey.length > 0 ? rawKey : null;
  const { transport, socketPath } = resolveTransport(options);
  if (options.cdnUrl) validateCdnUrl(options.cdnUrl);
  const cdnUrl = options.cdnUrl;
  return {
    ...options,
    home,
    host,
    port,
    apiKey,
    allowedOrigins: resolveAllowedOrigins(port, host),
    transport,
    socketPath,
    cdnUrl,
    embeddedMcp: options.embeddedMcp === true,
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
  transport: 'http' | 'unix';
  socketPath?: string;
  cdnUrl?: string;
  embeddedMcp: boolean;
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
  let transport: 'http' | 'unix' = 'http';
  let socketPath: string | undefined;
  let cdnUrl: string | undefined;
  let embeddedMcp = false;
  let hostGiven = false;
  let portGiven = false;

  for (let i = 0; i < args.length; i += 1) {
    const token = args[i];
    switch (token) {
      case '--home':
        home = args[++i] ?? '';
        break;
      case '--host':
        host = args[++i] ?? DEFAULT_HOST;
        hostGiven = true;
        break;
      case '--port':
        port = Number.parseInt(args[++i] ?? '', 10);
        portGiven = true;
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
      case '--transport': {
        const value = args[++i] ?? '';
        if (value !== 'http' && value !== 'unix') {
          throw new Error(`--transport must be "http" or "unix", got ${JSON.stringify(value)}`);
        }
        transport = value;
        break;
      }
      case '--socket':
        socketPath = args[++i] ?? '';
        break;
      case '--cdn-url':
        cdnUrl = args[++i] ?? '';
        break;
      case '--embedded-mcp':
        embeddedMcp = true;
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
  // `--socket` is required only for unix and conflicts with an explicitly
  // selected HTTP bind (architecture §7): an explicit --host/--port on the
  // unix transport has no meaning, and a socket path on http is ambiguous.
  if (transport === 'unix') {
    if (!socketPath) {
      throw new Error('--socket <absolute-path> is required for --transport unix');
    }
    if (hostGiven || portGiven) {
      throw new Error('--socket conflicts with an explicit --host/--port HTTP bind');
    }
  } else if (socketPath) {
    throw new Error('--socket conflicts with the http transport; pass --transport unix');
  }
  if (socketPath && !isAbsolute(socketPath)) {
    throw new Error(`--socket must be an absolute path: ${JSON.stringify(socketPath)}`);
  }
  if (cdnUrl) validateCdnUrl(cdnUrl);

  return {
    home,
    host,
    port,
    domainOnly,
    allowRemote,
    tlsCert,
    tlsKey,
    transport,
    socketPath,
    cdnUrl,
    embeddedMcp,
  };
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
