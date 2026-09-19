/**
 * Serialized owner of the app-managed TypeScript service (v1.192 P0-T4).
 *
 * Main-process module with no Electron import: the lifecycle logic is testable
 * under plain Node (`tests/service-controller.test.mjs`) and the utility entry
 * imports the wire protocol from here. The Electron `utilityProcess.fork`, the
 * renderer status sender and the detached handoff are injected adapters that
 * `src/main.ts` (P0-T7) supplies.
 *
 * Frozen shape (`rust-core-service-boundary.md` §8.1, plan "TS-service lifecycle
 * and quit"): `start`/`stop`/`restart`/`resetLocalState` are serialized,
 * `getStatus`/`subscribe` expose the existing `DaemonStatus`, `keepForQuit`
 * hands a running service to an independent owner.
 */

import { randomUUID } from 'node:crypto';
import { lstatSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { CoreServiceDiscovery } from '@42ch/nexus-contracts';
import type { ServiceOptions } from '@42ch/nexus-service';
import {
  DESKTOP_STATUS_CHANNEL,
  MAX_REQUEST_BYTES,
  type DaemonState,
  type DaemonStatus,
} from './desktop-contract.js';
import { DESKTOP_SERVICE_HOST } from './env.js';
import { crashBackoffDelayMs, isConfirmedCloseReport, isStaleGeneration } from './lifecycle-coord.js';

/** §9 frozen total close budget; the service owns the same constant for its own close. */
export const SERVICE_CLOSE_BUDGET_MS = 5_000;
/** Readiness / attach probe deadline for authenticated discovery plus health. */
export const READINESS_DEADLINE_MS = 15_000;
/** Per-request probe timeout inside that deadline. */
export const PROBE_REQUEST_TIMEOUT_MS = 2_000;
/** Bound on waiting for a stopped attached service to release its port. */
export const PORT_RELEASE_DEADLINE_MS = 3_000;
/**
 * Remainder of the frozen 30s `reset_local_database` invoke budget after the 5s
 * close and the 15s restart readiness window.
 */
export const NATIVE_RESET_TIMEOUT_MS = 10_000;
/** Poll cadence: 100ms for the first second, then 250ms. */
const PROBE_FAST_INTERVAL_MS = 100;
const PROBE_SLOW_INTERVAL_MS = 250;
const PROBE_FAST_WINDOW_MS = 1_000;
/** Published private record path (`rust-core-service-boundary.md` §8.1). */
const RECORD_SEGMENTS = ['.nexus42', 'run', 'service.json'] as const;

// ---------------------------------------------------------------------------
// Main → utility wire protocol (frozen message contract)
// ---------------------------------------------------------------------------

export type UtilityOperation = 'start' | 'close' | 'reset-local-state';

export const UTILITY_OPERATIONS: readonly UtilityOperation[] = ['start', 'close', 'reset-local-state'];

export interface UtilityRequest {
  generation: number;
  request_id: string;
  operation: UtilityOperation;
  payload: unknown;
}

export interface UtilityReply {
  generation: number;
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: { code: string; message: string };
}

/** Published, schema-derived discovery of the service the utility just started. */
export interface ServiceReadyMessage {
  type: 'service-ready';
  generation: number;
  discovery: CoreServiceDiscovery;
}

export interface UtilityReadyMessage {
  type: 'utility-ready';
}

export interface UtilityCrashedMessage {
  type: 'utility-crashed';
  message: string;
}

export type UtilityMessage = UtilityReply | ServiceReadyMessage | UtilityReadyMessage | UtilityCrashedMessage;

const REQUEST_ID_RE = /^[A-Za-z0-9._-]{1,128}$/;

export function utilityError(code: string, message: string): Error {
  return Object.assign(new Error(message), { code });
}

export function utilityMessageCode(err: unknown): string {
  if (err && typeof err === 'object' && 'code' in err && typeof err.code === 'string') return err.code;
  return 'internal';
}

export function utilityMessageText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Closed admission of a main→utility frame: bounded, versioned by generation, no unknown fields. */
export function parseUtilityRequest(raw: unknown): UtilityRequest {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    throw utilityError('invalid_input', 'request must be an object');
  }
  const { generation, request_id, operation, payload, ...extra } = raw as Record<string, unknown>;
  if (!Number.isSafeInteger(generation) || (generation as number) < 1) {
    throw utilityError('invalid_input', 'generation must be a positive integer');
  }
  if (typeof request_id !== 'string' || !REQUEST_ID_RE.test(request_id)) {
    throw utilityError('invalid_input', 'request_id must be a bounded identifier');
  }
  if (typeof operation !== 'string' || !UTILITY_OPERATIONS.includes(operation as UtilityOperation)) {
    throw utilityError('invalid_input', `unsupported operation: ${String(operation)}`);
  }
  if (Object.keys(extra).length > 0) {
    throw utilityError('invalid_input', 'request accepts no unknown fields');
  }
  let bytes = 0;
  try {
    bytes = payload === undefined ? 0 : Buffer.byteLength(JSON.stringify(payload), 'utf8');
  } catch {
    throw utilityError('invalid_input', 'payload is not serializable');
  }
  if (bytes > MAX_REQUEST_BYTES) {
    throw utilityError('input_too_large', 'request payload exceeds 1 MiB');
  }
  return { generation: generation as number, request_id, operation: operation as UtilityOperation, payload };
}

export function utilityOk(generation: number, request_id: string, result: unknown): UtilityReply {
  return { generation, request_id, ok: true, result: result ?? null };
}

export function utilityErr(generation: number, request_id: string, code: string, message: string): UtilityReply {
  return { generation, request_id, ok: false, error: { code, message } };
}

export function isUtilityReply(raw: unknown): raw is UtilityReply {
  if (!raw || typeof raw !== 'object') return false;
  const body = raw as Partial<UtilityReply>;
  return (
    typeof body.generation === 'number' &&
    typeof body.request_id === 'string' &&
    typeof body.ok === 'boolean'
  );
}

export function isServiceReadyMessage(raw: unknown): raw is ServiceReadyMessage {
  return (
    !!raw &&
    typeof raw === 'object' &&
    'type' in raw &&
    raw.type === 'service-ready' &&
    'generation' in raw &&
    typeof raw.generation === 'number'
  );
}

export function isUtilityCrashedMessage(raw: unknown): raw is UtilityCrashedMessage {
  return !!raw && typeof raw === 'object' && 'type' in raw && raw.type === 'utility-crashed';
}

/**
 * Structural admission of the closed v1 discovery record at the child-process
 * boundary. The service constructs it through the schema-derived builder; this
 * only refuses a frame that cannot be that record (no schema fork).
 */
export function assertDiscoveryShape(raw: unknown): CoreServiceDiscovery {
  if (!raw || typeof raw !== 'object') {
    throw utilityError('invalid_input', 'service discovery must be an object');
  }
  if (!('schema_version' in raw) || raw.schema_version !== 1) {
    throw utilityError('invalid_input', 'service discovery schema version is not supported');
  }
  if (!('protocol_version' in raw) || raw.protocol_version !== 1) {
    throw utilityError('invalid_input', 'service discovery protocol version is not supported');
  }
  if (!('instance_id' in raw) || typeof raw.instance_id !== 'string' || raw.instance_id.length === 0) {
    throw utilityError('invalid_input', 'service discovery has no instance id');
  }
  if (!('pid' in raw) || typeof raw.pid !== 'number') {
    throw utilityError('invalid_input', 'service discovery has no pid');
  }
  if (!('user_home' in raw) || typeof raw.user_home !== 'string' || raw.user_home.length === 0) {
    throw utilityError('invalid_input', 'service discovery has no user home');
  }
  if (!('tls_fingerprint' in raw) || (raw.tls_fingerprint !== null && typeof raw.tls_fingerprint !== 'string')) {
    throw utilityError('invalid_input', 'service discovery fingerprint is invalid');
  }
  if (!('readiness' in raw) || (raw.readiness !== 'ready' && raw.readiness !== 'uninitialized')) {
    throw utilityError('invalid_input', 'service discovery readiness is not supported');
  }
  if (!('endpoint' in raw) || !raw.endpoint || typeof raw.endpoint !== 'object') {
    throw utilityError('invalid_input', 'service discovery endpoint is not tagged');
  }
  const endpoint = raw.endpoint;
  if (!('transport' in endpoint) || (endpoint.transport !== 'http' && endpoint.transport !== 'unix')) {
    throw utilityError('invalid_input', 'service discovery endpoint is not tagged');
  }
  if (endpoint.transport === 'http' && (!('url' in endpoint) || typeof endpoint.url !== 'string')) {
    throw utilityError('invalid_input', 'http discovery endpoint has no url');
  }
  if (endpoint.transport === 'unix' && (!('path' in endpoint) || typeof endpoint.path !== 'string')) {
    throw utilityError('invalid_input', 'unix discovery endpoint has no path');
  }
  // Fully shape-checked above against the closed v1 record; the generated type
  // is the same shape.
  return raw as CoreServiceDiscovery;
}

// ---------------------------------------------------------------------------
// Injected adapters
// ---------------------------------------------------------------------------

/** The subset of Electron's `UtilityProcess` this controller drives. */
export interface UtilityChildProcess {
  readonly pid?: number;
  postMessage(message: unknown): void;
  kill(): boolean;
  on(event: 'message', listener: (message: unknown) => void): this;
  on(event: 'exit', listener: (code: number) => void): this;
}

/** Time seam: tests drive readiness, backoff and close budgets deterministically. */
export interface ControllerClock {
  now(): number;
  sleep(ms: number): Promise<void>;
}

const realClock: ControllerClock = {
  now: () => Date.now(),
  sleep: (ms) =>
    new Promise<void>((resolve) => {
      // A settled race must not hold the app (or a test process) open.
      setTimeout(resolve, ms).unref?.();
    }),
};

/**
 * Detached handoff for Keep-on-quit (P0-T6 owns the Node/entry adapter): verify
 * the installed Node runtime and packaged service entry, then start the same
 * service entry as an independent child on the same home/port.
 */
export interface DetachedServiceHandoff {
  prepare(): Promise<void>;
  start(options: { home: string; host: string; port: number }): Promise<void>;
}

export interface DesktopServiceControllerOptions {
  /** Trusted raw user home — main-owned, never renderer supplied. */
  home: string;
  /** Port resolved from the trusted launch config (`resolveDesktopServicePort`). */
  resolvedPort: number;
  /** API key for the guarded discovery/health/stop probes (main-only secret). */
  apiKey?: string;
  /** Fork the utility entry for one owner generation. */
  spawnUtility: (generation: number) => UtilityChildProcess;
  /** Main-owned status sender; P0-T7 wires it to the renderer event channel. */
  emitStatus?: (channel: string, status: DaemonStatus) => void;
  handoff?: DetachedServiceHandoff;
  clock?: ControllerClock;
}

async function withTimeout<T>(
  clock: ControllerClock,
  work: Promise<T>,
  ms: number,
  message: string,
): Promise<T> {
  const timeout = clock.sleep(ms).then(() => {
    throw utilityError('interrupted', message);
  });
  return Promise.race([work, timeout]);
}

function endpointOrigin(endpoint: CoreServiceDiscovery['endpoint']): string | null {
  return endpoint.transport === 'http' ? endpoint.url.replace(/\/+$/, '') : null;
}

function endpointPort(endpoint: CoreServiceDiscovery['endpoint']): number | null {
  const origin = endpointOrigin(endpoint);
  if (!origin) return null;
  try {
    const url = new URL(origin);
    const port = url.port ? Number.parseInt(url.port, 10) : url.protocol === 'https:' ? 443 : 80;
    return Number.isInteger(port) ? port : null;
  } catch {
    return null;
  }
}

function sameEndpoint(a: CoreServiceDiscovery['endpoint'], b: CoreServiceDiscovery['endpoint']): boolean {
  if (a.transport !== b.transport) return false;
  if (a.transport === 'unix' && b.transport === 'unix') return a.path === b.path;
  if (a.transport === 'http' && b.transport === 'http') return a.url.replace(/\/+$/, '') === b.url.replace(/\/+$/, '');
  return false;
}

/**
 * Attach identity (`§8.1`): the authenticated response must name the same
 * instance, home, endpoint and engine epoch as the private published record.
 * A replaced instance or a stale epoch never attaches.
 */
function identityMatches(response: CoreServiceDiscovery, expected: CoreServiceDiscovery): boolean {
  return (
    response.instance_id === expected.instance_id &&
    response.user_home === expected.user_home &&
    response.engine_epoch === expected.engine_epoch &&
    sameEndpoint(response.endpoint, expected.endpoint)
  );
}

interface PendingEntry {
  generation: number;
  settle: (reply: UtilityReply) => void;
  fail: (error: Error) => void;
}

interface ReadyWaiter {
  generation: number;
  settle: (discovery: CoreServiceDiscovery) => void;
}

interface LiveEndpoint {
  endpoint: CoreServiceDiscovery['endpoint'];
  port: number | null;
  discovery: CoreServiceDiscovery;
}

type OwnerProbe =
  | { kind: 'attached'; live: LiveEndpoint; version?: string }
  | { kind: 'conflict'; detail: string }
  | { kind: 'none' };

export class DesktopServiceController {
  private readonly home: string;
  private readonly host: string;
  private readonly targetPort: number;
  private readonly apiKey: string | undefined;
  private readonly spawnUtility: (generation: number) => UtilityChildProcess;
  private readonly emitStatus: ((channel: string, status: DaemonStatus) => void) | undefined;
  private readonly handoff: DetachedServiceHandoff | undefined;
  private readonly clock: ControllerClock;

  private queue: Promise<unknown> = Promise.resolve();
  private status: DaemonStatus;
  private readonly listeners = new Set<(status: DaemonStatus) => void>();
  private generation = 0;
  private child: UtilityChildProcess | null = null;
  private readonly pending = new Map<string, PendingEntry>();
  private readonly readyWaiters = new Set<ReadyWaiter>();
  /** Live service owned by this app's utility (`owned`) or an independent process (`attached`). */
  private owned = false;
  private attached: LiveEndpoint | null = null;
  private liveEndpoint: CoreServiceDiscovery['endpoint'] | null = null;
  private unconfirmedClose = false;
  private closeInFlight = false;
  private intentionalStop = false;
  private recovery: { cancelled: boolean } | null = null;
  /** Pre-exit crash diagnostic reported by the owner, folded into recovery text. */
  private crashDetail: string | null = null;
  private restartFlight: Promise<void> | null = null;

  constructor(options: DesktopServiceControllerOptions) {
    this.home = options.home;
    this.host = DESKTOP_SERVICE_HOST;
    this.targetPort = options.resolvedPort;
    this.apiKey = options.apiKey;
    this.spawnUtility = options.spawnUtility;
    this.emitStatus = options.emitStatus;
    this.handoff = options.handoff;
    this.clock = options.clock ?? realClock;
    // First state: stopped, no child.
    this.status = { state: 'stopped', port: this.targetPort };
  }

  /** Existing status shape; internal instance/home/epoch never leak here. */
  getStatus(): DaemonStatus {
    return { ...this.status };
  }

  /** Registers the listener first, then delivers the current snapshot. */
  subscribe(listener: (status: DaemonStatus) => void): () => void {
    this.listeners.add(listener);
    this.deliver(listener, this.status);
    return () => {
      this.listeners.delete(listener);
    };
  }

  start(): Promise<void> {
    return this.serialize(() => this.startLocked({ manual: true }));
  }

  /**
   * Ordinary stop: the owned handle is closed cooperatively; an attached
   * independent service is left running (row 8). `explicit` is the
   * authenticated instance/epoch-bound stop that explicit Restart and
   * Stop-and-Quit require (P0-T6), including for an attached service.
   */
  stop(options?: { explicit?: boolean }): Promise<void> {
    return this.serialize(() => this.stopLocked(options?.explicit === true));
  }

  /** Single-flight restart: concurrent callers join the one attempt. */
  restart(): Promise<void> {
    if (this.restartFlight) return this.restartFlight;
    const flight = this.serialize(async () => {
      this.cancelRecovery();
      this.intentionalStop = true;
      if (this.unconfirmedClose) {
        throw utilityError(
          'interrupted',
          `a previous close was not confirmed; the retained service owner must be released before restarting (${this.status.detail ?? 'unconfirmed cleanup'})`,
        );
      }
      if (this.owned) {
        await this.closeOwnedService();
      } else if (this.attached) {
        await this.stopAttachedService();
      }
      await this.startLocked({ manual: true });
    });
    const tracked = flight.finally(() => {
      if (this.restartFlight === tracked) this.restartFlight = null;
    });
    this.restartFlight = tracked;
    return tracked;
  }

  /** Serialized against start/stop/restart: confirmed close, native reset, real recovery. */
  resetLocalState(): Promise<void> {
    return this.serialize(() => this.resetLocked());
  }

  /** Keep-on-quit: transfer an owned service to an independent owner, or detach. */
  keepForQuit(): Promise<void> {
    return this.serialize(() => this.keepLocked());
  }

  // ── serialization ────────────────────────────────────────────────────────

  private serialize<T>(fn: () => Promise<T>): Promise<T> {
    const next = this.queue.then(fn, fn);
    this.queue = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  // ── status ───────────────────────────────────────────────────────────────

  private effectivePort(): number {
    const attachedPort = this.attached?.port ?? null;
    if (attachedPort !== null) return attachedPort;
    const livePort = this.liveEndpoint ? endpointPort(this.liveEndpoint) : null;
    return livePort ?? this.targetPort;
  }

  private setStatus(next: { state: DaemonState; detail?: string; version?: string; port?: number }): void {
    const merged: DaemonStatus = { state: next.state, port: next.port ?? this.effectivePort() };
    if (next.version !== undefined) merged.version = next.version;
    if (next.detail !== undefined) merged.detail = next.detail.slice(0, 2_048);
    if (
      merged.state === this.status.state &&
      merged.port === this.status.port &&
      merged.version === this.status.version &&
      merged.detail === this.status.detail
    ) {
      return;
    }
    this.status = merged;
    for (const listener of this.listeners) this.deliver(listener, merged);
    try {
      this.emitStatus?.(DESKTOP_STATUS_CHANNEL, { ...merged });
    } catch {
      // A destroyed window must not break the lifecycle transition.
    }
  }

  private deliver(listener: (status: DaemonStatus) => void, status: DaemonStatus): void {
    try {
      listener({ ...status });
    } catch {
      // Listeners are an isolation boundary: one must not break the others.
    }
  }

  // ── utility child ────────────────────────────────────────────────────────

  private ensureUtility(): UtilityChildProcess {
    if (this.child) return this.child;
    this.generation += 1;
    const generation = this.generation;
    const child = this.spawnUtility(generation);
    this.child = child;
    child.on('message', (message) => this.onChildMessage(generation, message));
    child.on('exit', (code) => this.onChildExit(generation, code));
    return child;
  }

  private onChildMessage(generation: number, raw: unknown): void {
    // Stale owner generations are ignored, never applied.
    if (isStaleGeneration(generation, this.generation)) return;
    if (isServiceReadyMessage(raw)) {
      let discovery: CoreServiceDiscovery;
      try {
        discovery = assertDiscoveryShape(raw.discovery);
      } catch {
        return;
      }
      for (const waiter of [...this.readyWaiters]) {
        if (waiter.generation === generation) waiter.settle(discovery);
      }
      return;
    }
    if (isUtilityCrashedMessage(raw)) {
      // The owner's own pre-exit diagnostic; the exit carries the recovery.
      this.crashDetail = raw.message.slice(0, 512);
      return;
    }
    if (!isUtilityReply(raw) || raw.generation !== generation) return;
    const entry = this.pending.get(raw.request_id);
    if (!entry || entry.generation !== generation) return;
    entry.settle(raw);
  }

  private onChildExit(generation: number, code: number): void {
    if (isStaleGeneration(generation, this.generation)) return;
    this.child = null;
    this.rejectPending(generation, `service owner exited (${code})`);
    if (this.closeInFlight) {
      // The close attempt owns the outcome: it reports interrupted/unconfirmed.
      return;
    }
    if (this.unconfirmedClose) {
      // The retained owner is gone: nothing is held any more, so a later start
      // is legal, but the unconfirmed cleanup stays an explicit diagnostic.
      this.clearRetainedOwner();
      this.setStatus({
        state: 'error',
        port: this.targetPort,
        detail: 'the service owner exited after an unconfirmed close; cleanup was never confirmed',
      });
      return;
    }
    if (this.intentionalStop) {
      this.owned = false;
      this.liveEndpoint = null;
      this.setStatus({ state: 'stopped', port: this.targetPort });
      return;
    }
    // An owner that dies while starting is as unexpected as one that dies
    // while running: both retry on the row 10 backoff schedule.
    if (this.owned || this.status.state === 'starting') {
      this.owned = false;
      this.liveEndpoint = null;
      this.scheduleRecovery(`service owner exited unexpectedly (${code})`);
      return;
    }
    // An idle owner process (no service running) dying is not a service crash.
    if (this.status.state !== 'stopped') {
      this.setStatus({ state: 'stopped', port: this.effectivePort() });
    }
  }

  private rejectPending(generation: number, message: string): void {
    for (const [requestId, entry] of [...this.pending]) {
      if (entry.generation !== generation) continue;
      this.pending.delete(requestId);
      entry.fail(utilityError('interrupted', message));
    }
  }

  private async request(
    child: UtilityChildProcess,
    generation: number,
    operation: UtilityOperation,
    payload: unknown,
    budgetMs: number,
  ): Promise<UtilityReply> {
    const request_id = randomUUID();
    const { promise, resolve, reject } = Promise.withResolvers<UtilityReply>();
    this.pending.set(request_id, {
      generation,
      settle: (reply) => {
        this.pending.delete(request_id);
        if (reply.ok) resolve(reply);
        else reject(utilityError(reply.error?.code ?? 'internal', reply.error?.message ?? 'utility request failed'));
      },
      fail: (error) => {
        this.pending.delete(request_id);
        reject(error);
      },
    });
    try {
      child.postMessage({ generation, request_id, operation, payload: payload ?? null });
      return await withTimeout(
        this.clock,
        promise,
        budgetMs,
        `${operation} was not answered within ${budgetMs}ms`,
      );
    } finally {
      this.pending.delete(request_id);
    }
  }

  private waitForReady(generation: number, deadline: number): Promise<CoreServiceDiscovery> {
    const { promise, resolve } = Promise.withResolvers<CoreServiceDiscovery>();
    const waiter: ReadyWaiter = { generation, settle: resolve };
    this.readyWaiters.add(waiter);
    const remaining = Math.max(1, deadline - this.clock.now());
    return withTimeout(
      this.clock,
      promise,
      remaining,
      'service-ready was not reported within the readiness deadline',
    ).finally(() => {
      this.readyWaiters.delete(waiter);
    });
  }

  // ── crash recovery (row 10) ──────────────────────────────────────────────

  private scheduleRecovery(reason: string): void {
    if (this.recovery) return;
    const crashDetail = this.crashDetail;
    this.crashDetail = null;
    const detail = crashDetail ? `${reason}: ${crashDetail}` : reason;
    const token = { cancelled: false };
    this.recovery = token;
    const loop = (async () => {
      for (let attempt = 1; attempt <= 5; attempt += 1) {
        const delay = crashBackoffDelayMs(attempt);
        if (delay === null) break;
        this.setStatus({
          state: 'degraded',
          port: this.effectivePort(),
          detail: `${detail}; automatic restart ${attempt} of 5 in ${delay}ms`,
        });
        await this.clock.sleep(delay);
        if (token.cancelled || this.intentionalStop) return;
        try {
          await this.serialize(() => this.startLocked({ manual: false }));
          return;
        } catch {
          // The next backoff step retries; exhaustion ends in stopped.
        }
        if (token.cancelled || this.intentionalStop) return;
      }
      this.setStatus({
        state: 'stopped',
        port: this.targetPort,
        detail: `${detail}; automatic recovery gave up after 5 attempts — start the service manually`,
      });
    })();
    void loop.finally(() => {
      if (this.recovery === token) this.recovery = null;
    });
  }

  private cancelRecovery(): void {
    if (this.recovery) this.recovery.cancelled = true;
  }

  // ── start ────────────────────────────────────────────────────────────────

  private async startLocked(input: { manual: boolean }): Promise<void> {
    if (input.manual) {
      this.cancelRecovery();
    }
    this.intentionalStop = false;
    if (this.unconfirmedClose) {
      throw utilityError(
        'interrupted',
        `a previous close was not confirmed; the retained service owner must be released before starting (${this.status.detail ?? 'unconfirmed cleanup'})`,
      );
    }
    if (this.owned || this.attached) return;
    this.setStatus({ state: 'starting', port: this.targetPort });
    const probe = await this.probeOwner();
    if (probe.kind === 'attached') {
      this.attached = probe.live;
      this.liveEndpoint = probe.live.endpoint;
      this.setStatus({
        state: 'running',
        port: probe.live.port ?? this.targetPort,
        ...(probe.version !== undefined ? { version: probe.version } : {}),
        detail: 'attached to an independent service',
      });
      return;
    }
    if (probe.kind === 'conflict') {
      this.setStatus({ state: 'error', port: this.targetPort, detail: probe.detail });
      throw utilityError('conflict', probe.detail);
    }
    await this.startOwned();
  }

  /**
   * Attach before spawning: a live independent owner for this home must never
   * be joined by a second engine owner. A listener that cannot prove the
   * published identity is a conflict — never attach-as-TS, never kill by pid
   * (D-7, row 7).
   */
  private async probeOwner(): Promise<OwnerProbe> {
    const record = this.readRecord();
    if (record) {
      const port = endpointPort(record.endpoint);
      if (port === null) {
        return {
          kind: 'conflict',
          detail: 'published service endpoint is not an attachable HTTP endpoint',
        };
      }
      const confirmed = await this.confirmIdentity(record.endpoint, record, this.clock.now() + READINESS_DEADLINE_MS);
      if (confirmed) return { kind: 'attached', live: confirmed.live, version: confirmed.version };
    }
    if (await this.listenerAnswers()) {
      return {
        kind: 'conflict',
        detail: `a listener on ${this.host}:${this.targetPort} does not provide matching authenticated service discovery`,
      };
    }
    return { kind: 'none' };
  }

  private async startOwned(): Promise<void> {
    const child = this.ensureUtility();
    const generation = this.generation;
    const deadline = this.clock.now() + READINESS_DEADLINE_MS;
    const options: ServiceOptions = {
      home: this.home,
      host: this.host,
      port: this.targetPort,
      allowRemote: false,
      domainOnly: false,
    };
    const remaining = Math.max(1, deadline - this.clock.now());
    const [reply, ready] = await Promise.all([
      this.request(child, generation, 'start', options, remaining),
      this.waitForReady(generation, deadline),
    ]);
    const reported = reply.result;
    const reportedDiscovery =
      reported && typeof reported === 'object' && 'discovery' in reported ? reported.discovery : undefined;
    const replyDiscovery = assertDiscoveryShape(reportedDiscovery);
    if (replyDiscovery.instance_id !== ready.instance_id) {
      this.setStatus({
        state: 'error',
        port: this.targetPort,
        detail: 'service owner reported inconsistent service identity',
      });
      throw utilityError('internal', 'service owner reported inconsistent service identity');
    }
    // Readiness means the service's own published record is this instance's,
    // answered over authenticated discovery and healthy HTTP.
    const record = this.readRecord();
    if (!record || record.instance_id !== ready.instance_id) {
      const detail =
        'the published discovery record does not name the service instance the owner started';
      this.setStatus({ state: 'error', port: this.targetPort, detail });
      throw utilityError('internal', detail);
    }
    const confirmed = await this.confirmIdentity(record.endpoint, record, deadline);
    if (!confirmed) {
      const detail = `service did not answer authenticated discovery and health within ${READINESS_DEADLINE_MS}ms`;
      this.setStatus({ state: 'error', port: this.targetPort, detail });
      throw utilityError('interrupted', detail);
    }
    this.owned = true;
    this.liveEndpoint = confirmed.live.endpoint;
    this.setStatus({
      state: 'running',
      port: confirmed.live.port ?? this.targetPort,
      ...(confirmed.version !== undefined ? { version: confirmed.version } : {}),
    });
  }

  // ── stop / close ─────────────────────────────────────────────────────────

  private async stopLocked(explicit: boolean): Promise<void> {
    this.cancelRecovery();
    this.intentionalStop = true;
    if (this.owned) {
      await this.closeOwnedService();
      return;
    }
    if (this.attached) {
      // Ordinary stop leaves an independent service running (row 8); the
      // authenticated instance/epoch stop is the explicit path.
      if (!explicit && !this.unconfirmedClose) return;
      await this.stopAttachedService();
      return;
    }
    // Nothing is owned or attached: a stop cancels any pending recovery and
    // clears the retained-close fence.
    this.unconfirmedClose = false;
    this.setStatus({ state: 'stopped', port: this.targetPort, detail: undefined, version: undefined });
  }

  /** Cooperative close inside the 5s budget; unconfirmed cleanup is never success. */
  private async closeOwnedService(): Promise<void> {
    const child = this.child;
    const generation = this.generation;
    if (!child) {
      this.markUnconfirmed('service owner is no longer alive; close cannot be confirmed');
    }
    this.closeInFlight = true;
    let report: unknown = null;
    try {
      const reply = await this.request(child, generation, 'close', null, SERVICE_CLOSE_BUDGET_MS);
      report = reply.result;
    } catch (error) {
      this.closeInFlight = false;
      this.owned = true;
      this.markUnconfirmed(
        `service close was not confirmed (${utilityMessageText(error)}); the retained owner blocks restart and quit`,
      );
    }
    this.closeInFlight = false;
    if (!isConfirmedCloseReport(report)) {
      this.owned = true;
      this.markUnconfirmed(
        'service close reported interrupted cleanup (cleanup_confirmed:false); the retained owner blocks restart and quit',
      );
    }
    this.owned = false;
    this.unconfirmedClose = false;
    this.liveEndpoint = null;
    this.setStatus({ state: 'stopped', port: this.targetPort, detail: undefined, version: undefined });
  }

  /**
   * An unconfirmed cleanup is an interrupted diagnostic, never success. While
   * the owner process is still alive it stays retained and fences further
   * ownership; a lost owner retains nothing, so the fence is cleared and only
   * the diagnostic remains.
   */
  private markUnconfirmed(detail: string): never {
    if (this.child === null && this.attached === null) {
      this.clearRetainedOwner();
      const lost = `${detail}; the service owner is gone, so the interrupted cleanup is diagnostic only`;
      this.setStatus({ state: 'error', port: this.targetPort, detail: lost });
      throw utilityError('interrupted', lost);
    }
    this.unconfirmedClose = true;
    this.setStatus({ state: 'error', port: this.effectivePort(), detail });
    throw utilityError('interrupted', detail);
  }

  private clearRetainedOwner(): void {
    this.owned = false;
    this.liveEndpoint = null;
    this.unconfirmedClose = false;
  }

  /** Authenticated instance/epoch-bound stop of an attached service, then verified port release. */
  private async stopAttachedService(): Promise<void> {
    const live = this.attached;
    if (!live) {
      this.unconfirmedClose = false;
      this.setStatus({ state: 'stopped', port: this.targetPort, detail: undefined });
      return;
    }
    const origin = endpointOrigin(live.endpoint);
    if (!origin) throw utilityError('invalid_input', 'attached service endpoint is not HTTP');
    const response = await this.probeJson(`${origin}/v1/daemon/runtime/stop`, {
      method: 'POST',
      body: {
        expected_instance_id: live.discovery.instance_id,
        expected_engine_epoch: live.discovery.engine_epoch,
      },
    });
    if (!response.ok) {
      this.markUnconfirmed(
        `attached service did not accept the authenticated stop (${response.code}: ${response.message}); it keeps running`,
      );
    }
    // A 200 {status:'stopping'} is not confirmed closure: wait for the port to be released.
    const released = await this.waitForPortRelease(origin, PORT_RELEASE_DEADLINE_MS);
    if (!released) {
      this.markUnconfirmed(
        `attached service did not release ${this.host}:${live.port ?? this.targetPort} within ${PORT_RELEASE_DEADLINE_MS}ms`,
      );
    }
    this.attached = null;
    this.liveEndpoint = null;
    this.unconfirmedClose = false;
    this.setStatus({ state: 'stopped', port: this.targetPort, detail: undefined, version: undefined });
  }

  private async waitForPortRelease(origin: string, budgetMs: number): Promise<boolean> {
    const deadline = this.clock.now() + budgetMs;
    for (;;) {
      const discovery = await this.probeJson(`${origin}/v1/daemon/runtime/discovery`, { method: 'GET' });
      if (!discovery.ok || discovery.body === null) return true;
      const now = this.clock.now();
      if (now >= deadline) return false;
      await this.clock.sleep(Math.min(PROBE_FAST_INTERVAL_MS, Math.max(1, deadline - now)));
    }
  }

  // ── reset ────────────────────────────────────────────────────────────────

  private async resetLocked(): Promise<void> {
    this.cancelRecovery();
    this.intentionalStop = true;
    if (this.owned) {
      await this.closeOwnedService();
    } else if (this.attached) {
      await this.stopAttachedService();
    }
    const child = this.ensureUtility();
    const generation = this.generation;
    // The native binding lives in the utility only; main forwards the trusted
    // home and never a renderer-supplied path.
    const reply = await this.request(
      child,
      generation,
      'reset-local-state',
      { home: this.home },
      NATIVE_RESET_TIMEOUT_MS,
    );
    const result = reply.result;
    const removed = result && typeof result === 'object' && 'removed' in result ? result.removed : undefined;
    if (typeof removed !== 'number') {
      const detail = 'local-state reset returned no store count';
      this.setStatus({ state: 'error', port: this.targetPort, detail });
      throw utilityError('internal', detail);
    }
    // Same recovery/restart outcome as before: the service comes back up.
    await this.startLocked({ manual: true });
  }

  // ── keep (quit) ──────────────────────────────────────────────────────────

  private async keepLocked(): Promise<void> {
    this.cancelRecovery();
    if (this.unconfirmedClose) {
      throw utilityError(
        'interrupted',
        `a previous close was not confirmed; the retained service owner must be released before keeping the service (${this.status.detail ?? 'unconfirmed cleanup'})`,
      );
    }
    if (!this.owned) {
      // Attached: the GUI simply detaches. Nothing running: nothing to keep.
      return;
    }
    const handoff = this.handoff;
    if (!handoff) {
      throw utilityError(
        'unavailable',
        'keeping the service requires a detached handoff (installed Node runtime and packaged service entry)',
      );
    }
    const port = this.effectivePort();
    await handoff.prepare();
    await this.closeOwnedService();
    await handoff.start({ home: this.home, host: this.host, port });
    const record = this.readRecord();
    if (!record) throw utilityError('interrupted', 'the detached service published no discovery record');
    const confirmed = await this.confirmIdentity(
      record.endpoint,
      record,
      this.clock.now() + READINESS_DEADLINE_MS,
    );
    if (!confirmed) {
      throw utilityError(
        'interrupted',
        `the detached service did not confirm authenticated discovery and health within ${READINESS_DEADLINE_MS}ms`,
      );
    }
    this.attached = confirmed.live;
    this.liveEndpoint = confirmed.live.endpoint;
    this.setStatus({
      state: 'running',
      port: confirmed.live.port ?? port,
      ...(confirmed.version !== undefined ? { version: confirmed.version } : {}),
      detail: 'independent service (detached handoff)',
    });
  }

  // ── probes ───────────────────────────────────────────────────────────────

  private readRecord(): CoreServiceDiscovery | null {
    const path = join(this.home, ...RECORD_SEGMENTS);
    try {
      // lstat, never stat: a planted symlink at the leaf is refused, not read.
      if (!lstatSync(path).isFile()) return null;
      return assertDiscoveryShape(JSON.parse(readFileSync(path, 'utf8')));
    } catch {
      return null;
    }
  }

  /** Authenticated discovery plus health; identity must match the published record. */
  private async confirmIdentity(
    endpoint: CoreServiceDiscovery['endpoint'],
    expected: CoreServiceDiscovery,
    deadline: number,
  ): Promise<{ live: LiveEndpoint; version?: string } | null> {
    const origin = endpointOrigin(endpoint);
    if (!origin) return null;
    const started = this.clock.now();
    for (;;) {
      const discovery = await this.probeJson(`${origin}/v1/daemon/runtime/discovery`, { method: 'GET' });
      if (discovery.ok && discovery.body) {
        let record: CoreServiceDiscovery;
        try {
          record = assertDiscoveryShape(discovery.body);
        } catch {
          return null;
        }
        if (identityMatches(record, expected)) {
          const health = await this.probeJson(`${origin}/v1/daemon/runtime/health`, { method: 'GET' });
          const healthBody = health.body;
          const healthStatus =
            healthBody && typeof healthBody === 'object' && 'status' in healthBody
              ? healthBody.status
              : undefined;
          const healthVersion =
            healthBody && typeof healthBody === 'object' && 'version' in healthBody
              ? healthBody.version
              : undefined;
          if (health.ok && healthStatus === 'ok') {
            return {
              live: { endpoint: record.endpoint, port: endpointPort(record.endpoint), discovery: record },
              ...(typeof healthVersion === 'string' ? { version: healthVersion } : {}),
            };
          }
        }
      }
      const now = this.clock.now();
      if (now >= deadline) return null;
      const interval = now - started < PROBE_FAST_WINDOW_MS ? PROBE_FAST_INTERVAL_MS : PROBE_SLOW_INTERVAL_MS;
      await this.clock.sleep(Math.min(interval, Math.max(1, deadline - now)));
    }
  }

  /** Any HTTP answer on the target port means a listener exists (identity unproven). */
  private async listenerAnswers(): Promise<boolean> {
    try {
      await fetch(`http://${this.host}:${this.targetPort}/v1/daemon/runtime/health`, {
        method: 'GET',
        redirect: 'error',
        signal: AbortSignal.timeout(PROBE_REQUEST_TIMEOUT_MS),
      });
      return true;
    } catch {
      return false;
    }
  }

  /** Guarded probe: never follows a redirect with the API key attached. */
  private async probeJson(
    url: string,
    request: { method: 'GET' | 'POST'; body?: unknown },
  ): Promise<{ ok: boolean; body: unknown; code: string; message: string }> {
    try {
      const response = await fetch(url, {
        method: request.method,
        redirect: 'error',
        signal: AbortSignal.timeout(PROBE_REQUEST_TIMEOUT_MS),
        headers: {
          ...(this.apiKey ? { 'X-API-Key': this.apiKey } : {}),
          ...(request.body === undefined ? {} : { 'Content-Type': 'application/json' }),
        },
        ...(request.body === undefined ? {} : { body: JSON.stringify(request.body) }),
      });
      let body: unknown = null;
      try {
        body = await response.json();
      } catch {
        body = null;
      }
      if (response.ok) return { ok: true, body, code: 'ok', message: 'ok' };
      const envelope = body && typeof body === 'object' && 'error' in body ? body.error : undefined;
      const code =
        envelope && typeof envelope === 'object' && 'code' in envelope && typeof envelope.code === 'string'
          ? envelope.code
          : `http_${response.status}`;
      const message =
        envelope && typeof envelope === 'object' && 'message' in envelope && typeof envelope.message === 'string'
          ? envelope.message
          : `service probe answered ${response.status}`;
      return { ok: false, body, code, message };
    } catch (error) {
      return { ok: false, body: null, code: 'unreachable', message: utilityMessageText(error) };
    }
  }
}
