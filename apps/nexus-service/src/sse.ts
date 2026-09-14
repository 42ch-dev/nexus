import { randomUUID } from 'node:crypto';
import type { ServerResponse } from 'node:http';
import type { ProviderEventBatch, ProviderHostEvent } from '@42ch/nexus-contracts';
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
import { HttpError, mapNativeError } from './errors.js';
import { hostQuery } from './world-kb.js';

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
  | { kind: 'stale'; gap: CoreStreamGap };

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
    // Fail closed: once the stream has ended — a terminal retained, a
    // terminal refused, or the hub disposed — every later record is refused,
    // terminal or not. A post-close event must never append a frame or
    // consume a sequence: the resync gap is the stream's final word and
    // canonical truth lives behind the inspect URL.
    if (this.closed) return this.terminalSlot;
    const terminal = isTerminalHostEvent(event);
    const data = JSON.stringify(event);
    // Build against the *candidate* sequence and charge before committing, so a
    // frame that is not retained never consumes a sequence and retained ids stay
    // contiguous. Every *retained* Buffer is charged against the global Node byte
    // pool; a frame that cannot be charged is never retained uncharged — the hub
    // fails closed with a bounded interrupted gap so the client resyncs.
    const frame = this.buildFrame(this.sequence + 1, 'provider_event', data, terminal);
    if (terminal) {
      if (frame.wireBytes > SSE_RESERVED_CONTROL_BYTES) {
        return this.failClosed();
      }
      // A terminal is a control frame: it draws on the control pool, never the
      // data pool, so a full data budget cannot block the terminal transition.
      if (!tryReserveControlBytes(frame.wireBytes)) {
        return this.failClosed();
      }
      this.sequence += 1;
      this.closed = true;
      this.terminalSlot = frame;
      return frame;
    }
    // Data frames are charged against the global data pool; a frame that cannot
    // be charged is never retained uncharged — the hub fails closed with a
    // bounded interrupted gap so the client resyncs.
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
    // Control frames draw on the dedicated control pool, never the data pool, so
    // a saturated data budget can never block an explicit resync gap.
    if (!tryReserveControlBytes(frame.wireBytes)) return null;
    this.sequence += 1;
    // A replaced gap releases its charge so the slot never leaks bytes.
    if (this.gapSlot) releaseControlBytes(this.gapSlot.wireBytes);
    frame.isControl = true;
    this.gapSlot = frame;
    return frame;
  }

  /**
   * Fail closed: stop the stream and expose a bounded, charged interrupted
   * gap. Used both when a terminal cannot be represented (byte budget) and
   * when a provider terminal is refused by the registry — canonical truth
   * (the first terminal) then lives behind the inspect URL, never in a
   * fabricated stream frame.
   */
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
    const id = `${this.epoch}:${sequence}`;
    const buffer = Buffer.from(formatSse(id, event, data), 'utf8');
    return {
      id,
      event,
      buffer,
      wireBytes: buffer.length,
      isControl: isTerminal,
      isTerminal,
    };
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
      return { kind: 'wait' };
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

function inspectUrl(operationId: string): string {
  return `/v1/daemon/agent-host/operations/${operationId}`;
}

export const sseTestHooks = {
  providerPullCount: 0,
  writeBlockedCount: 0,
};

/** Exported so scoped tests can drive backpressure deterministically. */
export class SseWriter {
  outboundBackpressured = false;
  private pendingDataFrames = 0;
  private pendingDataBytes = 0;
  private terminalWritten = false;
  private gapWritten = false;

  constructor(
    private readonly res: ServerResponse,
    private readonly hub: OperationEventHub,
  ) {
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
    // Retained hub Buffers are already charged globally; the per-socket 64 KiB
    // reservation covers the writable queue, so the writer must not double-charge
    // the same Buffer here.
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
    if (frame.isTerminal && this.terminalWritten) return 'ok';
    if (frame.event === 'gap' && this.gapWritten) return 'ok';
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
      // `write()` returning false means the chunk WAS accepted into the socket
      // buffer and the caller must stop until 'drain' — it never means the chunk
      // was dropped. Re-writing it here would duplicate the frame.
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
      this.hub.markClosed();
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
  const timer = setTimeout(() => resolve(false), timeoutMs);
  const done = (ok: boolean) => {
    clearTimeout(timer);
    resolve(ok);
  };
  res.once('drain', () => done(true));
  res.once('close', () => done(false));
  res.once('error', () => done(false));
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

function clientDisconnected(res: ServerResponse): boolean {
  return res.writableEnded || res.destroyed || res.socket?.destroyed === true;
}

interface PullGateState {
  initialReleased: boolean;
}

/** Do not pull provider events until the TCP reader has caught up. */
async function gatePullUntilDrain(
  res: ServerResponse,
  pullGate: PullGateState,
  writer: SseWriter,
): Promise<boolean> {
  if (!pullGate.initialReleased) {
    // Defer first pull until pauseImmediately clients have paused the socket.
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

/**
 * Hydrate a session from native truth when the transport cache is cold. The
 * registry is a cache only: a restart (or a cleared cache) must still resync
 * from `hostQuery` rather than fabricate a 404.
 */
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
    service.providerRegistry.registerSession(record);
    return record;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'not_found') return null;
    if (mapped.code === 'invalid_input' && mapped.message === 'host not started') return null;
    throw mapped;
  }
}

/**
 * Hydrate an operation from native truth, preserving the native status so a
 * terminal operation is never mistaken for a cancellable one.
 */
async function hydrateOperationRecord(
  service: ServiceCore,
  operationId: string,
): Promise<ProviderOperationRecord | null> {
  const cached = service.providerRegistry.operationRecord(operationId);
  if (cached) return cached;
  try {
    const response = await hostQuery(service, { query: 'get_operation', operation_id: operationId });
    const operation = response.operation;
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
    service.providerRegistry.registerOperation(record);
    return record;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code === 'not_found') return null;
    if (mapped.code === 'invalid_input' && mapped.message === 'host not started') return null;
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
  if (frame) await writer.writeFrame(frame);
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
    // The registry decides acceptance BEFORE recording: a late terminal that
    // contradicts the canonical first terminal (e.g. an OpFinished arriving
    // after an accepted cancel) must never enter the hub as the stream's
    // terminal, or replay and the stream would disagree with the status the
    // durable journal recorded.
    if (service.providerRegistry.finishOperation(operationId, event, transcript)) {
      hub.recordEvent(event);
    } else if (!hub.isClosed()) {
      // Refused terminal with no stream terminal yet: fail closed with a
      // bounded resync gap so the client re-inspects canonical truth instead
      // of consuming a fabricated terminal. The rest of the batch is dead on
      // arrival: stop immediately so no later event can append or emit after
      // the interrupted resync marker.
      hub.failClosed();
      break;
    }
  }
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

  const existingHub = service.providerRegistry.hubForOperation(operationId);
  const hub =
    existingHub ??
    service.providerRegistry.ensureHub(operationId, () => new OperationEventHub(operationId, sessionId));

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
    // Cold registry + already-terminal native op: no retained stream to replay.
    // A bounded resync gap is the truthful answer, never a fabricated terminal.
    if (!existingHub && isTerminalOperationStatus(operation.status) && !hub.hasTerminal()) {
      const gapFrame = hub.recordGap({
        reason: 'history_unavailable',
        operation_id: operationId,
        resync_required: true,
        inspect_url: inspectUrl(operationId),
      });
      if (gapFrame) await writer.writeFrame(gapFrame);
      return;
    }

    if (plan.kind === 'stale') {
      const gapFrame = hub.recordGap(plan.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') await emitInterruptedGap(writer, hub, operationId);
      }
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
          await waitForTerminal(service, hub, writer, operationId, res, deliveredSequence, pullGate);
        }
      }
      return;
    }

    if (hub.isClosed()) {
      return;
    }

    await liveEventLoop(service, hub, writer, operationId, res, deliveredSequence, pullGate);
  } finally {
    writer.end();
  }
}

/**
 * A canonical operation that is already terminal (e.g. an accepted cancel)
 * while the stream still carries no terminal frame: its provider
 * confirmation was refused as a late terminal, or never arrives. End the
 * stream truthfully — one bounded resync gap pointing at the inspect URL —
 * instead of hanging on a stream that may never confirm or fabricating a
 * terminal that contradicts canonical status. Returns the gap frame when the
 * stream was closed here.
 */
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
  hub: OperationEventHub,
  writer: SseWriter,
  operationId: string,
  res: ServerResponse,
  deliveredSequence: number,
  pullGate: PullGateState,
): Promise<void> {
  let sequence = deliveredSequence;
  while (!hub.isClosed()) {
    if (clientDisconnected(res)) return;

    // Canonical truth already terminal with no stream terminal: end the
    // stream with one truthful resync gap instead of hanging or fabricating.
    const canonicalGap = canonicalTerminalGap(service, hub, operationId);
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
      batch = await service.core.nextProviderEvents(
        operationId,
        PROVIDER_PULL_MAX_EVENTS,
        PROVIDER_PULL_MAX_BYTES,
      );
    } catch {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    ingestEvents(service, operationId, batch.events ?? []);

    // A batch whose ingest closed the hub (an accepted terminal, a refused
    // late terminal, or a memory fail-close) has already recorded the
    // canonical closing frame. A provider gap from that same batch must
    // never overwrite the fail-closed `interrupted` resync marker — the
    // canonical interrupted resync gap is the stream's truthful ending.
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
}

async function liveEventLoop(
  service: ServiceCore,
  hub: OperationEventHub,
  writer: SseWriter,
  operationId: string,
  res: ServerResponse,
  deliveredSequence: number,
  pullGate: PullGateState,
): Promise<void> {
  let sequence = deliveredSequence;
  while (!hub.isClosed()) {
    if (clientDisconnected(res)) return;

    // Canonical truth already terminal with no stream terminal: end the
    // stream with one truthful resync gap instead of hanging or fabricating.
    const canonicalGap = canonicalTerminalGap(service, hub, operationId);
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
      batch = await service.core.nextProviderEvents(
        operationId,
        PROVIDER_PULL_MAX_EVENTS,
        PROVIDER_PULL_MAX_BYTES,
      );
    } catch {
      await emitInterruptedGap(writer, hub, operationId);
      return;
    }

    ingestEvents(service, operationId, batch.events ?? []);

    // A batch whose ingest closed the hub (an accepted terminal, a refused
    // late terminal, or a memory fail-close) has already recorded the
    // canonical closing frame. A provider gap from that same batch must
    // never overwrite the fail-closed `interrupted` resync marker — the
    // canonical interrupted resync gap is the stream's truthful ending.
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

    if (!batch.has_more && hub.isClosed()) return;
    if (!batch.has_more && (batch.events?.length ?? 0) === 0) {
      await sleep(25);
    }
  }
}

function sleep(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
}
