import { randomUUID } from 'node:crypto';
import type { ServerResponse } from 'node:http';
import type {
  CharacterOperationResult,
  ProviderEventBatch,
  ProviderHostEvent,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import type { ProviderOperationRecord, ProviderSessionRecord } from './provider-registry.js';
import { isTerminalOperationStatus } from './provider-registry.js';
import {
  HUB_MAX_DATA_BYTES,
  HUB_MAX_DATA_FRAMES,
  PROVIDER_PULL_MAX_BYTES,
  PROVIDER_PULL_MAX_EVENTS,
  SSE_DRAIN_TIMEOUT_MS,
  SSE_MAX_OUTSTANDING_FRAME_BYTES,
  SSE_MAX_PENDING_DATA_BYTES,
  SSE_MAX_PENDING_DATA_FRAMES,
  SSE_MAX_SUBSCRIBERS_PER_SESSION,
  SSE_RESERVED_CONTROL_BYTES,
  resolveSseFirstPullDelayMs,
  resolveSseSocketHighWaterMark,
} from './config.js';
import {
  releaseControlBytes,
  releaseEnvironmentBytes,
  releaseEnvironmentSocket,
  reserveEnvironmentSocket,
  tryReserveControlBytes,
  tryReserveEnvironmentBytes,
} from './environment-budget.js';
import { HttpError, isAbsentHostError, mapNativeError } from './errors.js';
import { hostQuery, withPrincipal } from './world-kb.js';

type CoreStreamGap = NonNullable<ProviderEventBatch['gap']>;

export interface StoredFrame {
  id: string;
  event: string;
  /** Serialized once; the socket writes these exact bytes (no UTF-16 slicing). */
  buffer: Buffer;
  wireBytes: number;
  isControl: boolean;
  isTerminal: boolean;
}

export type ReplayPlan =
  | { kind: 'all'; frames: StoredFrame[] }
  | { kind: 'frames'; frames: StoredFrame[] }
  | { kind: 'wait' }
  | { kind: 'stale'; gap: CoreStreamGap }
  | { kind: 'end' };

const sessionSubscriberCounts = new Map<string, number>();

export class OperationEventHub {
  readonly operationId: string;
  readonly sessionId: string;
  readonly epoch = randomUUID();
  private sequence = 0;
  private dataFrames: StoredFrame[] = [];
  private dataFrameBytes = 0;
  private evictionWatermarkSeq = 0;
  private terminalSlot: StoredFrame | null = null;
  private terminalRejected = false;
  private gapSlot: StoredFrame | null = null;
  private closed = false;

  constructor(operationId: string, sessionId: string) {
    this.operationId = operationId;
    this.sessionId = sessionId;
  }

  recordEvent(event: ProviderHostEvent): StoredFrame | null {
    if (this.closed) return this.terminalSlot;
    const terminal = isTerminalHostEvent(event);
    const data = JSON.stringify(event);
    const frame = this.buildFrame(this.sequence + 1, 'provider_event', data, terminal);
    if (terminal) {
      if (frame.wireBytes > SSE_RESERVED_CONTROL_BYTES) {
        return this.failClosed();
      }
      if (!tryReserveControlBytes(frame.wireBytes)) {
        return this.failClosed();
      }
      this.sequence += 1;
      this.closed = true;
      this.terminalSlot = frame;
      return frame;
    }
    if (!tryReserveEnvironmentBytes(frame.wireBytes)) {
      return this.failClosed();
    }
    this.sequence += 1;
    this.dataFrames.push(frame);
    this.dataFrameBytes += frame.wireBytes;
    this.evictDataIfNeeded();
    return frame;
  }

  recordGap(gap: CoreStreamGap): StoredFrame | null {
    const data = JSON.stringify(gap);
    const frame = this.buildFrame(this.sequence + 1, 'gap', data, false);
    if (frame.wireBytes > SSE_RESERVED_CONTROL_BYTES) return null;
    if (!tryReserveControlBytes(frame.wireBytes)) return null;
    this.sequence += 1;
    if (this.gapSlot) releaseControlBytes(this.gapSlot.wireBytes);
    frame.isControl = true;
    this.gapSlot = frame;
    return frame;
  }

  failClosed(): StoredFrame | null {
    this.closed = true;
    this.terminalRejected = true;
    this.recordGap({
      reason: 'interrupted',
      operation_id: this.operationId,
      resync_required: true,
      inspect_url: inspectUrl(this.operationId),
    });
    return this.gapSlot;
  }

  private evictDataIfNeeded(): void {
    while (
      this.dataFrames.length > HUB_MAX_DATA_FRAMES ||
      this.dataFrameBytes > HUB_MAX_DATA_BYTES
    ) {
      const evicted = this.dataFrames.shift();
      if (!evicted) break;
      this.dataFrameBytes = Math.max(0, this.dataFrameBytes - evicted.wireBytes);
      releaseEnvironmentBytes(evicted.wireBytes);
      this.evictionWatermarkSeq = Math.max(this.evictionWatermarkSeq, frameSequence(evicted) + 1);
    }
  }

  private buildFrame(sequence: number, event: string, data: string, isTerminal: boolean): StoredFrame {
    return wireFrame(`${this.epoch}:${sequence}`, event, data, isTerminal, isTerminal);
  }

  planReplay(cursor: string | undefined): ReplayPlan {
    if (!cursor) {
      return { kind: 'all', frames: this.collectFrames() };
    }
    const parsed = parseCursor(cursor);
    if (!parsed) {
      throw new HttpError(400, 'invalid_input', 'malformed SSE cursor');
    }
    if (parsed.epoch !== this.epoch) {
      throw new HttpError(400, 'invalid_input', 'SSE cursor epoch unavailable', {
        variant: 'history_unavailable',
      });
    }
    if (parsed.sequence > this.sequence) {
      throw new HttpError(400, 'invalid_input', 'SSE cursor is in the future', {
        variant: 'future_cursor',
      });
    }
    if (parsed.sequence < this.evictionWatermarkSeq) {
      return {
        kind: 'stale',
        gap: {
          reason: 'history_unavailable',
          operation_id: this.operationId,
          resync_required: true,
          inspect_url: inspectUrl(this.operationId),
        },
      };
    }
    if (parsed.sequence === this.sequence) {
      return this.closed ? { kind: 'end' } : { kind: 'wait' };
    }
    return { kind: 'frames', frames: this.framesAfter(parsed.sequence) };
  }

  /** Data, gap, and terminal merged and strictly ordered by epoch sequence. */
  private collectFrames(): StoredFrame[] {
    const frames = [...this.dataFrames];
    if (this.gapSlot) frames.push(this.gapSlot);
    if (this.terminalSlot) frames.push(this.terminalSlot);
    frames.sort((a, b) => frameSequence(a) - frameSequence(b));
    return frames;
  }

  /** Frames strictly after `sequence`, ordered — a gap between data never reorders. */
  framesAfter(sequence: number): StoredFrame[] {
    return this.collectFrames().filter((frame) => frameSequence(frame) > sequence);
  }

  evictionWatermark(): number {
    return this.evictionWatermarkSeq;
  }

  retainedMemoryBytes(): number {
    let total = this.dataFrameBytes;
    if (this.gapSlot) total += this.gapSlot.wireBytes;
    if (this.terminalSlot) total += this.terminalSlot.wireBytes;
    return total;
  }

  markClosed(): void {
    this.closed = true;
  }

  /** The operation's stream has ended: a terminal was retained or rejected. */
  isClosed(): boolean {
    return this.closed;
  }

  hasTerminal(): boolean {
    return this.terminalSlot !== null;
  }

  /** A resync gap is retained — the stream's closing frame when no terminal is. */
  hasGap(): boolean {
    return this.gapSlot !== null;
  }

  dispose(): void {
    releaseEnvironmentBytes(this.dataFrameBytes);
    let controlFreed = 0;
    if (this.gapSlot) controlFreed += this.gapSlot.wireBytes;
    if (this.terminalSlot) controlFreed += this.terminalSlot.wireBytes;
    releaseControlBytes(controlFreed);
    this.dataFrames = [];
    this.dataFrameBytes = 0;
    this.terminalSlot = null;
    this.gapSlot = null;
    this.closed = true;
  }

  /** Data bytes this hub holds against the global data pool. */
  chargedBytes(): number {
    return this.dataFrameBytes;
  }
}

function frameSequence(frame: StoredFrame): number {
  return Number.parseInt(frame.id.split(':')[1] ?? '0', 10);
}


function isTerminalHostEvent(event: ProviderHostEvent): boolean {
  return 'OpFinished' in event || 'OpFailed' in event || 'SessionStopped' in event;
}

export function parseCursor(cursor: string): { epoch: string; sequence: number } | null {
  const idx = cursor.indexOf(':');
  if (idx <= 0) return null;
  const epoch = cursor.slice(0, idx);
  const seqText = cursor.slice(idx + 1);
  if (!/^\d+$/.test(seqText)) return null;
  const sequence = Number.parseInt(seqText, 10);
  if (!Number.isSafeInteger(sequence) || sequence < 0) return null;
  return { epoch, sequence };
}

function formatSse(id: string, event: string, data: string): string {
  return `id: ${id}\nevent: ${event}\ndata: ${data}\n\n`;
}

/**
 *
 * `id`/`event`/`data` are written exactly as the producer encoded them — the
 * run-event ring owns the `<epoch>:<sequence>` cursor, so a transport that
 * replays them verbatim never renumbers, re-encodes or reorders. A frame with
 * an EMPTY `id` (the `history_unavailable` control frame) writes no `id:`
 * line at all: an empty `id:` field would reset the client's last-event-id
 * instead of leaving it alone.
 *
 * `isControl` and `isTerminal` are independent and BOTH must travel: a Host
 * terminal is a control frame (not charged to the data budget) AND the frame
 * that ends its stream, while a retention gap is only the first. A frame the
 * ceiling cannot mark terminal lets the replay loop walk past it and write
 * whatever the hub retained behind it.
 */
export function wireFrame(
  id: string,
  event: string,
  data: string,
  isControl: boolean,
  isTerminal = false,
): StoredFrame {
  const text = id.length === 0 ? `event: ${event}\ndata: ${data}\n\n` : formatSse(id, event, data);
  const buffer = Buffer.from(text, 'utf8');
  return { id, event, buffer, wireBytes: buffer.length, isControl, isTerminal };
}

export interface SseStreamEnd {
  markClosed(): void;
}

/** For a producer that owns its stream end itself: the writer marks nothing. */
export const NO_STREAM_END: SseStreamEnd = { markClosed: () => undefined };

function inspectUrl(operationId: string): string {
  return `/v1/daemon/agent-host/operations/${operationId}`;
}

export const sseTestHooks = {
  providerPullCount: 0,
  writeBlockedCount: 0,
  now: (): number => Date.now(),
};

export const sseUnretainedGapEvents = { count: 0 };

function noteUnretainedGap(operationId: string, site: string): void {
  sseUnretainedGapEvents.count += 1;
  console.error(
    `[nexus-service] stream ${operationId} ended without a typed resync gap at ${site}: control-frame reserve exhausted`,
  );
}

/** How a writer treats repeated control frames. */
export interface SseWriterOptions {
  readonly collapseControlFrames?: boolean;
}

/** Exported so scoped tests can drive backpressure deterministically. */
export class SseWriter {
  outboundBackpressured = false;
  private readonly collapseControlFrames: boolean;
  private pendingDataFrames = 0;
  private pendingDataBytes = 0;
  private terminalWritten = false;
  private gapWritten = false;

  constructor(
    private readonly res: ServerResponse,
    private readonly streamEnd: SseStreamEnd,
    options: SseWriterOptions = {},
  ) {
    this.collapseControlFrames = options.collapseControlFrames ?? true;
    if (this.res.socket) this.res.socket.setNoDelay(true);
  }

  private reserveFrame(frame: StoredFrame): boolean {
    if (frame.isControl) {
      if (frame.wireBytes > SSE_RESERVED_CONTROL_BYTES) return false;
      if (frame.event === 'gap' && this.gapWritten) return true;
      if (frame.isTerminal && this.terminalWritten) return true;
    } else {
      if (frame.wireBytes > SSE_MAX_OUTSTANDING_FRAME_BYTES) return false;
      if (this.pendingDataFrames >= SSE_MAX_PENDING_DATA_FRAMES) return false;
      if (this.pendingDataBytes + frame.wireBytes > SSE_MAX_PENDING_DATA_BYTES) return false;
    }
    if (!frame.isControl) {
      this.pendingDataFrames += 1;
      this.pendingDataBytes += frame.wireBytes;
    }
    return true;
  }

  private releaseFrame(frame: StoredFrame): void {
    if (!frame.isControl) {
      this.pendingDataFrames = Math.max(0, this.pendingDataFrames - 1);
      this.pendingDataBytes = Math.max(0, this.pendingDataBytes - frame.wireBytes);
    }
  }

  async writeFrame(frame: StoredFrame): Promise<'ok' | 'overflow' | 'disconnect'> {
    if (this.res.writableEnded || this.res.destroyed) return 'disconnect';
    if (this.collapseControlFrames && frame.isTerminal && this.terminalWritten) return 'ok';
    if (this.collapseControlFrames && frame.event === 'gap' && this.gapWritten) return 'ok';
    if (!this.reserveFrame(frame)) return 'overflow';

    const payload = frame.buffer;
    const chunkSize = Math.max(1, resolveSseSocketHighWaterMark());
    let offset = 0;
    while (offset < payload.length) {
      const chunk = payload.subarray(offset, Math.min(offset + chunkSize, payload.length));
      if (this.res.writableEnded || this.res.destroyed) {
        this.releaseFrame(frame);
        return 'disconnect';
      }
      const accepted = this.res.write(chunk);
      if (!accepted) {
        this.outboundBackpressured = true;
        sseTestHooks.writeBlockedCount += 1;
        const drained = await waitForDrain(this.res, SSE_DRAIN_TIMEOUT_MS);
        this.outboundBackpressured = false;
        if (!drained) {
          this.releaseFrame(frame);
          return 'disconnect';
        }
      }
      offset += chunk.length;
    }
    this.releaseFrame(frame);
    if (frame.isTerminal) {
      this.terminalWritten = true;
      this.streamEnd.markClosed();
    }
    if (frame.event === 'gap') {
      this.gapWritten = true;
    }
    return 'ok';
  }

  end(): void {
    if (!this.res.writableEnded) this.res.end();
  }
}

function waitForDrain(res: ServerResponse, timeoutMs: number): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>();
  const settle = (ok: boolean) => {
    clearTimeout(timer);
    res.off('drain', onDrain);
    res.off('close', onClose);
    res.off('error', onError);
    resolve(ok);
  };
  const onDrain = () => settle(true);
  const onClose = () => settle(false);
  const onError = () => settle(false);
  const timer = setTimeout(() => settle(false), timeoutMs);
  res.once('drain', onDrain);
  res.once('close', onClose);
  res.once('error', onError);
  return promise;
}

export function reserveSessionSubscriber(sessionId: string): void {
  const count = sessionSubscriberCounts.get(sessionId) ?? 0;
  if (count >= SSE_MAX_SUBSCRIBERS_PER_SESSION) {
    throw new HttpError(409, 'busy', 'too many SSE subscribers for session');
  }
  if (!reserveEnvironmentSocket()) {
    throw new HttpError(409, 'busy', 'too many SSE subscribers');
  }
  sessionSubscriberCounts.set(sessionId, count + 1);
}

export function releaseSessionSubscriber(sessionId: string): void {
  const count = sessionSubscriberCounts.get(sessionId);
  if (count === undefined) return;
  releaseEnvironmentSocket();
  if (count <= 1) sessionSubscriberCounts.delete(sessionId);
  else sessionSubscriberCounts.set(sessionId, count - 1);
}

export function clientDisconnected(res: ServerResponse): boolean {
  return res.writableEnded || res.destroyed || res.socket?.destroyed === true;
}

export interface PullGateState {
  initialReleased: boolean;
}

/**
 * Do not pull events until the TCP reader has caught up: the first pull waits
 * out `pauseImmediately` clients so their pause lands before any frame is
 * written, and every later pull waits for a pending drain. `false` means the
 * socket went away or never drained — the caller must stop pulling.
 */
export async function gatePullUntilDrain(
  res: ServerResponse,
  pullGate: PullGateState,
  writer: SseWriter,
): Promise<boolean> {
  if (!pullGate.initialReleased) {
    await sleep(resolveSseFirstPullDelayMs());
    pullGate.initialReleased = true;
  }
  if (writer.outboundBackpressured) {
    const drained = await waitForDrain(res, SSE_DRAIN_TIMEOUT_MS);
    if (!drained) return false;
    writer.outboundBackpressured = false;
  }
  return true;
}

async function hydrateSessionRecord(
  service: ServiceCore,
  sessionId: string,
): Promise<ProviderSessionRecord | null> {
  const cached = service.providerRegistry.sessionRecord(sessionId);
  if (cached) return cached;
  try {
    const response = await hostQuery(service, { query: 'get_session', session_id: sessionId });
    const session = response.session;
    if (!session) return null;
    const record: ProviderSessionRecord = {
      sessionId: session.session_id,
      providerId: session.provider_id,
      state: session.state,
      activeOpId: session.active_op_id ?? null,
      model: session.model,
    };
    if (session.actor_ref !== undefined) record.actorRef = session.actor_ref;
    if (session.viewpoint !== undefined) record.viewpoint = session.viewpoint;
    service.providerRegistry.registerSession(record);
    return record;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'not_found' || isAbsentHostError(mapped)) return null;
    throw mapped;
  }
}

async function authorityOperationRow(
  service: ServiceCore,
  operationId: string,
): Promise<{ operation_id: string; session_id: string; status: string } | null> {
  try {
    const response = await hostQuery(service, { query: 'get_operation', operation_id: operationId });
    return response.operation ?? null;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'not_found' || isAbsentHostError(mapped)) return null;
    throw mapped;
  }
}

async function hydrateOperationRecord(
  service: ServiceCore,
  operationId: string,
): Promise<ProviderOperationRecord | null> {
  const cached = service.providerRegistry.operationRecord(operationId);
  if (cached) return cached;
  const operation = await authorityOperationRow(service, operationId);
  if (!operation) return null;
  const session = await hydrateSessionRecord(service, operation.session_id);
  const record: ProviderOperationRecord = {
    operationId: operation.operation_id,
    sessionId: operation.session_id,
    providerId: session?.providerId ?? '',
    status: operation.status,
    terminalEvent: null,
    terminalTranscript: null,
  };
  if (!service.providerRegistry.registerOperation(record)) {
    throw new HttpError(503, 'busy', 'too many active provider operations');
  }
  return record;
}

async function hydrateCharacterOperation(
  service: ServiceCore,
  operationId: string,
): Promise<CharacterOperationResult | null> {
  try {
    return await withPrincipal(service, (principal) =>
      service.core.hostCharacterOperation(principal, operationId),
    );
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'not_found' || isAbsentHostError(mapped)) return null;
    throw mapped;
  }
}

async function emitInterruptedGap(
  writer: SseWriter,
  hub: OperationEventHub,
  operationId: string,
): Promise<void> {
  const gap: CoreStreamGap = {
    reason: 'interrupted',
    operation_id: operationId,
    resync_required: true,
    inspect_url: inspectUrl(operationId),
  };
  const frame = hub.recordGap(gap);
  if (!frame) {
    noteUnretainedGap(operationId, 'interrupted-gap');
    return;
  }
  await writer.writeFrame(frame);
}

/**
 *
 * A stream must always end with either a terminal or a typed resync gap, so a
 * hub that closed out of band — the mirror retired the record while this loop's
 * pull was in flight, or a fail-close could not retain its own gap — ends with
 * the bounded `interrupted` resync marker instead of closing bare. A hub still
 * retaining its ending (a terminal, or the gap the loop just wrote) is left
 * exactly as it is: this only fills a missing ending, never replaces one.
 */
async function endStreamWithoutEnding(
  writer: SseWriter,
  hub: OperationEventHub,
  operationId: string,
): Promise<void> {
  if (hub.hasTerminal() || hub.hasGap()) return;
  await emitInterruptedGap(writer, hub, operationId);
}

/** Exported so scoped tests can drive terminal acceptance exactly as the SSE loops do. */
export function ingestEvents(service: ServiceCore, operationId: string, events: ProviderHostEvent[]): void {
  const op = service.providerRegistry.operationRecord(operationId);
  if (!op) return;
  const hub = service.providerRegistry.ensureHub(
    operationId,
    () => new OperationEventHub(operationId, op.sessionId),
  );
  for (const event of events) {
    if (!isTerminalHostEvent(event)) {
      hub.recordEvent(event);
      continue;
    }
    const transcript =
      'OpFinished' in event
        ? JSON.stringify(event.OpFinished)
        : 'OpFailed' in event
          ? JSON.stringify(event.OpFailed)
          : null;
    if (service.providerRegistry.finishOperation(operationId, event, transcript)) {
      hub.recordEvent(event);
    } else if (!hub.isClosed()) {
      hub.failClosed();
      break;
    }
  }
}

/**
 *
 * Both legal owners of an operation — the provider-only lane and the core Actor
 * authority — deliver the same generated event batches under the same bounded
 * caps; only the pull call and the canonical-terminal read differ. One source
 * per stream keeps the SSE loops a single implementation, so the Actor path can
 * never drift into a second delivery policy (and vice versa).
 */
interface OperationEventSource {
  readonly hub: OperationEventHub;
  /** Frames the contract says to replay before any pull (or a truthful gap). */
  initialTruthGap(): StoredFrame | null;
  /** Canonical truth is terminal while the hub retains no terminal frame. */
  canonicalGap(): Promise<StoredFrame | null>;
  /** One bounded batch from the owner authorized to serve this operation. */
  pull(): Promise<ProviderEventBatch>;
}

/** The provider-only lane: canonical truth is the mirror record it hydrates. */
function providerEventSource(
  service: ServiceCore,
  operation: ProviderOperationRecord,
  operationId: string,
  existingHub: OperationEventHub | undefined,
): OperationEventSource {
  const hub =
    existingHub ??
    service.providerRegistry.ensureHub(
      operationId,
      () => new OperationEventHub(operationId, operation.sessionId),
    );
  return {
    hub,
    initialTruthGap: () =>
      existingHub === undefined &&
      isTerminalOperationStatus(operation.status) &&
      !hub.hasTerminal()
        ? hub.recordGap({
            reason: 'history_unavailable',
            operation_id: operationId,
            resync_required: true,
            inspect_url: inspectUrl(operationId),
          })
        : null,
    canonicalGap: async () => canonicalTerminalGap(service, hub, operationId),
    pull: () =>
      service.core.nextProviderEvents(
        operationId,
        PROVIDER_PULL_MAX_EVENTS,
        PROVIDER_PULL_MAX_BYTES,
      ),
  };
}

const ACTOR_SETTLEMENT_RECHECK_MS = 1000;

const actorSettlementCheckedAt = new WeakMap<OperationEventHub, number>();

function actorSettlementDue(hub: OperationEventHub): boolean {
  const checkedAt = actorSettlementCheckedAt.get(hub);
  const now = sseTestHooks.now();
  if (checkedAt !== undefined && now - checkedAt < ACTOR_SETTLEMENT_RECHECK_MS) return false;
  actorSettlementCheckedAt.set(hub, now);
  return true;
}

async function actorEventSource(
  service: ServiceCore,
  actor: ProviderSessionRecord,
  sessionId: string,
  operationId: string,
): Promise<OperationEventSource> {
  const character = await hydrateCharacterOperation(service, operationId);
  if (!character) {
    throw new HttpError(404, 'not_found', `operation ${operationId} not found`, {
      resource: `operation:${operationId}`,
    });
  }
  if (character.session_id !== sessionId) {
    throw new HttpError(403, 'forbidden', 'operation does not belong to session', {
      resource: `operation:${operationId}`,
    });
  }
  const existingHub = service.providerRegistry.hubForOperation(operationId);
  if (!service.providerRegistry.markActorOperation(operationId, sessionId, actor.providerId)) {
    throw new HttpError(503, 'busy', 'too many retained Actor operations');
  }
  const hub =
    existingHub ??
    service.providerRegistry.ensureHub(
      operationId,
      () => new OperationEventHub(operationId, sessionId),
    );
  return {
    hub,
    initialTruthGap: () => null,
    canonicalGap: async () => null,
    pull: async () => {
      const batch = await withPrincipal(service, (principal) =>
        service.core.nextHostEvents(
          principal,
          sessionId,
          operationId,
          PROVIDER_PULL_MAX_EVENTS,
          PROVIDER_PULL_MAX_BYTES,
        ),
      );
      // Amortized: an observation with nothing left is polled, and this status
      // read is the second native read that used to ride along with every empty
      // batch — it is now at most one per window per operation, shared by that
      // hub's readers. A batch carrying events or a gap never reaches the timer,
      // so a delivered frame stays immediate; a run that settles inside the
      // window ends at the next window's check.
      if (
        (batch.events?.length ?? 0) === 0 &&
        !batch.has_more &&
        batch.gap === undefined &&
        actorSettlementDue(hub)
      ) {
        const latest = await hydrateCharacterOperation(service, operationId);
        if (latest && latest.run_status !== 'running') {
          throw new Error('actor observation exhausted after settlement');
        }
      }
      return batch;
    },
  };
}

async function runEventStream(
  service: ServiceCore,
  source: OperationEventSource,
  operationId: string,
  searchParams: URLSearchParams,
  res: ServerResponse,
): Promise<void> {
  const hub = source.hub;
  const cursor = searchParams.get('cursor') ?? searchParams.get('last_event_id') ?? undefined;
  const plan = hub.planReplay(cursor);

  res.writeHead(200, {
    'Content-Type': 'text/event-stream; charset=utf-8',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
  });

  const writer = new SseWriter(res, hub);
  const pullGate: PullGateState = { initialReleased: false };
  let deliveredSequence = 0;
  if (cursor) {
    const parsed = parseCursor(cursor);
    if (parsed) deliveredSequence = parsed.sequence;
  }

  try {
    // A bounded resync gap is the truthful answer, never a fabricated terminal.
    const truthGap = source.initialTruthGap();
    if (truthGap) {
      await writer.writeFrame(truthGap);
      return;
    }

    if (plan.kind === 'stale') {
      const gapFrame = hub.recordGap(plan.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') await emitInterruptedGap(writer, hub, operationId);
      } else {
        noteUnretainedGap(operationId, 'stale-plan-gap');
      }
      return;
    }

    if (plan.kind === 'end') {
      return;
    }

    const initialFrames =
      plan.kind === 'all' ? plan.frames : plan.kind === 'frames' ? plan.frames : [];

    for (const frame of initialFrames) {
      const result = await writer.writeFrame(frame);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }
      deliveredSequence = frameSequence(frame);
      if (frame.isTerminal) return;
    }

    if (plan.kind === 'wait') {
      if (!hub.isClosed()) {
        const op = service.providerRegistry.operationRecord(operationId);
        if (op?.terminalEvent) {
          hub.recordEvent(op.terminalEvent);
        } else {
          await waitForTerminal(service, source, writer, operationId, res, deliveredSequence, pullGate);
        }
      }
      return;
    }

    if (hub.isClosed()) {
      await endStreamWithoutEnding(writer, hub, operationId);
      return;
    }

    await liveEventLoop(service, source, writer, operationId, res, deliveredSequence, pullGate);
  } finally {
    writer.end();
  }
}

/**
 * The Creator Actor arm: the authority owns the operation and answers for it,
 * but retains no event stream for a Creator prompt — its observation is the
 * generic operation row, not a Character result (technical contract §5). The
 * stream therefore reports the bounded `history_unavailable` resync gap that
 * points a consumer at that canonical read, instead of 404-ing an operation the
 * authority does own or narrowing the Actor id into the provider-only lane.
 */
async function creatorActorEventSource(
  service: ServiceCore,
  actor: ProviderSessionRecord,
  sessionId: string,
  operationId: string,
): Promise<OperationEventSource> {
  const operation = await authorityOperationRow(service, operationId);
  if (!operation) {
    throw new HttpError(404, 'not_found', `operation ${operationId} not found`, {
      resource: `operation:${operationId}`,
    });
  }
  if (operation.session_id !== sessionId) {
    throw new HttpError(403, 'forbidden', 'operation does not belong to session', {
      resource: `operation:${operationId}`,
    });
  }
  if (!service.providerRegistry.markActorOperation(operationId, sessionId, actor.providerId)) {
    throw new HttpError(503, 'busy', 'too many retained Actor operations');
  }
  const hub = service.providerRegistry.ensureHub(
    operationId,
    () => new OperationEventHub(operationId, sessionId),
  );
  return {
    hub,
    initialTruthGap: () =>
      hub.recordGap({
        reason: 'history_unavailable',
        operation_id: operationId,
        resync_required: true,
        inspect_url: inspectUrl(operationId),
      }),
    canonicalGap: async () => null,
    pull: () => Promise.reject(new Error('a Creator Actor operation retains no event stream')),
  };
}

export async function streamSessionEvents(
  service: ServiceCore,
  sessionId: string,
  searchParams: URLSearchParams,
  res: ServerResponse,
): Promise<void> {
  const session = await hydrateSessionRecord(service, sessionId);
  if (!session) {
    throw new HttpError(404, 'not_found', `session ${sessionId} not found`, {
      resource: `session:${sessionId}`,
    });
  }
  const operationId = searchParams.get('operation_id') ?? session.activeOpId;
  if (!operationId) {
    throw new HttpError(400, 'invalid_input', 'operation_id is required for session events');
  }

  // This request is a live reader of `operationId`'s hub from here on, so it
  // registers BEFORE the source (and its hub) is built and releases only when
  // the stream is over: retiring the record must never dispose the hub under the
  // pull this stream has in flight (`ProviderRegistry.attachOperationStream`).
  service.providerRegistry.attachOperationStream(operationId);
  try {
    const source = session.actorRef
      ? session.actorRef.actor_kind === 'creator'
        ? await creatorActorEventSource(service, session, sessionId, operationId)
        : await actorEventSource(service, session, sessionId, operationId)
      : providerEventSource(
          service,
          await requireProviderOperation(service, sessionId, operationId),
          operationId,
          service.providerRegistry.hubForOperation(operationId),
        );

    await runEventStream(service, source, operationId, searchParams, res);
  } finally {
    service.providerRegistry.detachOperationStream(operationId);
  }
}

/** Provider-only admission kept exactly as it was: hydrate, associate, retain. */
async function requireProviderOperation(
  service: ServiceCore,
  sessionId: string,
  operationId: string,
): Promise<ProviderOperationRecord> {
  const operation = await hydrateOperationRecord(service, operationId);
  if (!operation) {
    throw new HttpError(404, 'not_found', `operation ${operationId} not found`, {
      resource: `operation:${operationId}`,
    });
  }
  if (operation.sessionId !== sessionId) {
    throw new HttpError(403, 'forbidden', 'operation does not belong to session', {
      resource: `operation:${operationId}`,
    });
  }
  return operation;
}

function canonicalTerminalGap(
  service: ServiceCore,
  hub: OperationEventHub,
  operationId: string,
): StoredFrame | null {
  if (hub.hasTerminal()) return null;
  const op = service.providerRegistry.operationRecord(operationId);
  if (!op || op.terminalEvent || !isTerminalOperationStatus(op.status)) return null;
  return hub.failClosed();
}

async function waitForTerminal(
  service: ServiceCore,
  source: OperationEventSource,
  writer: SseWriter,
  operationId: string,
  res: ServerResponse,
  deliveredSequence: number,
  pullGate: PullGateState,
): Promise<void> {
  const hub = source.hub;
  let sequence = deliveredSequence;
  while (!hub.isClosed()) {
    if (clientDisconnected(res)) return;

    const canonicalGap = await source.canonicalGap();
    if (canonicalGap) {
      const result = await writer.writeFrame(canonicalGap);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
      }
      return;
    }

    const readyToPull = await gatePullUntilDrain(res, pullGate, writer);
    if (!readyToPull) {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    let batch: ProviderEventBatch;
    try {
      sseTestHooks.providerPullCount += 1;
      batch = await source.pull();
    } catch {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    ingestEvents(service, operationId, batch.events ?? []);

    if (batch.gap && !hub.isClosed()) {

      const gapFrame = hub.recordGap(batch.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
      }
    }

    for (const frame of hub.framesAfter(sequence)) {
      const result = await writer.writeFrame(frame);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }
      sequence = frameSequence(frame);
      if (frame.isTerminal) return;
    }

    if (!batch.has_more && (batch.events?.length ?? 0) === 0) {
      await sleep(25);
    }
  }
  await endStreamWithoutEnding(writer, hub, operationId);
}

async function liveEventLoop(
  service: ServiceCore,
  source: OperationEventSource,
  writer: SseWriter,
  operationId: string,
  res: ServerResponse,
  deliveredSequence: number,
  pullGate: PullGateState,
): Promise<void> {
  const hub = source.hub;
  let sequence = deliveredSequence;
  while (!hub.isClosed()) {
    if (clientDisconnected(res)) return;

    const canonicalGap = await source.canonicalGap();
    if (canonicalGap) {
      const result = await writer.writeFrame(canonicalGap);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
      }
      return;
    }

    const readyToPull = await gatePullUntilDrain(res, pullGate, writer);
    if (!readyToPull) {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    let batch: ProviderEventBatch;
    try {
      sseTestHooks.providerPullCount += 1;
      batch = await source.pull();
    } catch {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    ingestEvents(service, operationId, batch.events ?? []);

    if (batch.gap && !hub.isClosed()) {
      const gapFrame = hub.recordGap(batch.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
      }
    }

    for (const frame of hub.framesAfter(sequence)) {
      const result = await writer.writeFrame(frame);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }
      sequence = frameSequence(frame);
      if (frame.isTerminal) return;
    }

    if (!batch.has_more && hub.isClosed()) break;
    if (!batch.has_more && (batch.events?.length ?? 0) === 0) {
      await sleep(25);
    }
  }
  await endStreamWithoutEnding(writer, hub, operationId);
}

function sleep(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
}
