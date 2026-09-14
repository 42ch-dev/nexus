import { randomUUID } from 'node:crypto';
import type { ServerResponse } from 'node:http';
import type { ProviderEventBatch, ProviderHostEvent } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
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
  releaseEnvironmentBytes,
  tryReserveEnvironmentBytes,
} from './environment-budget.js';
import { HttpError } from './errors.js';

type CoreStreamGap = NonNullable<ProviderEventBatch['gap']>;

export interface StoredFrame {
  id: string;
  event: string;
  data: string;
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
  private gapSlot: StoredFrame | null = null;
  private closed = false;

  constructor(operationId: string, sessionId: string) {
    this.operationId = operationId;
    this.sessionId = sessionId;
  }

  recordEvent(event: ProviderHostEvent): StoredFrame | null {
    if (isTerminalHostEvent(event) && this.terminalSlot) {
      return this.terminalSlot;
    }
    const data = JSON.stringify(event);
    const frame = this.makeFrame('provider_event', data, isTerminalHostEvent(event));
    if (frame.isTerminal) {
      this.terminalSlot = frame;
      this.closed = true;
      return frame;
    }
    this.pushDataFrame(frame);
    return frame;
  }

  recordGap(gap: CoreStreamGap): StoredFrame | null {
    const data = JSON.stringify(gap);
    const wireBytes = wireBytesFor('gap', `${this.epoch}:${this.sequence + 1}`, data);
    if (wireBytes > SSE_RESERVED_CONTROL_BYTES) {
      return null;
    }
    const sequence = ++this.sequence;
    const id = `${this.epoch}:${sequence}`;
    const frame: StoredFrame = {
      id,
      event: 'gap',
      data,
      wireBytes,
      isControl: true,
      isTerminal: false,
    };
    this.gapSlot = frame;
    return frame;
  }

  private pushDataFrame(frame: StoredFrame): void {
    this.dataFrames.push(frame);
    this.dataFrameBytes += frame.wireBytes;
    this.evictDataIfNeeded();
  }

  private evictDataIfNeeded(): void {
    while (
      this.dataFrames.length > HUB_MAX_DATA_FRAMES ||
      this.dataFrameBytes > HUB_MAX_DATA_BYTES
    ) {
      const evicted = this.dataFrames.shift();
      if (!evicted) break;
      this.dataFrameBytes = Math.max(0, this.dataFrameBytes - evicted.wireBytes);
      const evictedSeq = frameSequence(evicted);
      this.evictionWatermarkSeq = Math.max(this.evictionWatermarkSeq, evictedSeq + 1);
    }
  }

  private makeFrame(event: string, data: string, isTerminal: boolean): StoredFrame {
    const sequence = ++this.sequence;
    const id = `${this.epoch}:${sequence}`;
    const wireBytes = wireBytesFor(event, id, data);
    return {
      id,
      event,
      data,
      wireBytes,
      isControl: false,
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

  private collectFrames(): StoredFrame[] {
    const frames = [...this.dataFrames];
    if (this.gapSlot) frames.push(this.gapSlot);
    if (this.terminalSlot) frames.push(this.terminalSlot);
    return frames;
  }

  framesAfter(sequence: number): StoredFrame[] {
    const frames: StoredFrame[] = [];
    for (const frame of this.dataFrames) {
      if (frameSequence(frame) > sequence) frames.push(frame);
    }
    if (this.gapSlot && frameSequence(this.gapSlot) > sequence) {
      frames.push(this.gapSlot);
    }
    if (this.terminalSlot && frameSequence(this.terminalSlot) > sequence) {
      frames.push(this.terminalSlot);
    }
    return frames;
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

  isClosed(): boolean {
    return this.closed;
  }

  hasTerminal(): boolean {
    return this.terminalSlot !== null;
  }

  dispose(): void {
    this.dataFrames = [];
    this.dataFrameBytes = 0;
    this.terminalSlot = null;
    this.gapSlot = null;
    this.closed = true;
  }
}

function wireBytesFor(event: string, id: string, data: string): number {
  return Buffer.byteLength(`id: ${id}\nevent: ${event}\ndata: ${data}\n\n`, 'utf8');
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

function formatSse(frame: StoredFrame): string {
  return `id: ${frame.id}\nevent: ${frame.event}\ndata: ${frame.data}\n\n`;
}

function inspectUrl(operationId: string): string {
  return `/v1/daemon/agent-host/operations/${operationId}`;
}

export const sseTestHooks = {
  providerPullCount: 0,
  writeBlockedCount: 0,
  pullsWhileBlocked: 0,
};

class SseWriter {
  outboundBackpressured = false;
  private pendingDataFrames = 0;
  private pendingDataBytes = 0;
  private terminalWritten = false;
  private gapWritten = false;
  private envReserved = 0;

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
    if (!tryReserveEnvironmentBytes(frame.wireBytes)) return false;
    this.envReserved += frame.wireBytes;
    if (!frame.isControl) {
      this.pendingDataFrames += 1;
      this.pendingDataBytes += frame.wireBytes;
    }
    return true;
  }

  private releaseFrame(frame: StoredFrame): void {
    releaseEnvironmentBytes(frame.wireBytes);
    this.envReserved = Math.max(0, this.envReserved - frame.wireBytes);
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

    const payload = formatSse(frame);
    const chunkSize = Math.max(1, resolveSseSocketHighWaterMark());
    let offset = 0;
    while (offset < payload.length) {
      if (this.res.writableEnded || this.res.destroyed) {
        this.releaseFrame(frame);
        return 'disconnect';
      }
      const slice = payload.slice(offset, Math.min(offset + chunkSize, payload.length));
      for (;;) {
        if (this.res.writableEnded || this.res.destroyed) {
          this.releaseFrame(frame);
          return 'disconnect';
        }
        const accepted = this.res.write(slice, 'utf8');
        if (accepted) {
          break;
        }
        this.outboundBackpressured = true;
        sseTestHooks.writeBlockedCount += 1;
        const drained = await waitForDrain(this.res, SSE_DRAIN_TIMEOUT_MS);
        if (!drained) {
          this.releaseFrame(frame);
          this.outboundBackpressured = false;
          return 'disconnect';
        }
        this.outboundBackpressured = false;
      }
      offset += slice.length;
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
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), timeoutMs);
    const done = (ok: boolean) => {
      clearTimeout(timer);
      resolve(ok);
    };
    res.once('drain', () => done(true));
    res.once('close', () => done(false));
    res.once('error', () => done(false));
  });
}

export function reserveSessionSubscriber(sessionId: string): void {
  const count = sessionSubscriberCounts.get(sessionId) ?? 0;
  if (count >= SSE_MAX_SUBSCRIBERS_PER_SESSION) {
    throw new HttpError(409, 'busy', 'too many SSE subscribers for session');
  }
  sessionSubscriberCounts.set(sessionId, count + 1);
}

export function releaseSessionSubscriber(sessionId: string): void {
  const next = (sessionSubscriberCounts.get(sessionId) ?? 1) - 1;
  if (next <= 0) sessionSubscriberCounts.delete(sessionId);
  else sessionSubscriberCounts.set(sessionId, next);
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
    sseTestHooks.pullsWhileBlocked += 1;
    const drained = await waitForDrain(res, SSE_DRAIN_TIMEOUT_MS);
    if (!drained) return false;
    writer.outboundBackpressured = false;
  }
  return true;
}

function assertOperationForSession(
  service: ServiceCore,
  sessionId: string,
  operationId: string,
): void {
  const op = service.providerRegistry.operationRecord(operationId);
  if (!op) {
    throw new HttpError(404, 'not_found', `operation ${operationId} not found`, {
      resource: `operation:${operationId}`,
    });
  }
  if (op.sessionId !== sessionId) {
    throw new HttpError(403, 'forbidden', 'operation does not belong to session', {
      resource: `operation:${operationId}`,
    });
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

function ingestEvents(service: ServiceCore, operationId: string, events: ProviderHostEvent[]): void {
  const op = service.providerRegistry.operationRecord(operationId);
  if (!op) return;
  const hub = service.providerRegistry.ensureHub(
    operationId,
    () => new OperationEventHub(operationId, op.sessionId),
  );
  for (const event of events) {
    hub.recordEvent(event);
    if (isTerminalHostEvent(event) && !op.terminalEvent) {
      const transcript =
        'OpFinished' in event
          ? JSON.stringify(event.OpFinished)
          : 'OpFailed' in event
            ? JSON.stringify(event.OpFailed)
            : null;
      service.providerRegistry.finishOperation(operationId, event, transcript);
    }
  }
}

export async function streamSessionEvents(
  service: ServiceCore,
  sessionId: string,
  searchParams: URLSearchParams,
  res: ServerResponse,
): Promise<void> {
  const session = service.providerRegistry.sessionRecord(sessionId);
  const operationId = searchParams.get('operation_id') ?? session?.activeOpId;
  if (!operationId) {
    throw new HttpError(400, 'invalid_input', 'operation_id is required for session events');
  }

  assertOperationForSession(service, sessionId, operationId);

  const hub = service.providerRegistry.hubForOperation(operationId);
  if (!hub) {
    throw new HttpError(404, 'not_found', `operation hub ${operationId} not found`, {
      resource: `operation:${operationId}`,
    });
  }

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
      if (!hub.hasTerminal()) {
        const op = service.providerRegistry.operationRecord(operationId);
        if (op?.terminalEvent) {
          hub.recordEvent(op.terminalEvent);
        } else {
          await waitForTerminal(service, hub, writer, operationId, res, deliveredSequence, pullGate);
        }
      }
      return;
    }

    if (hub.hasTerminal()) {
      return;
    }

    await liveEventLoop(service, hub, writer, operationId, res, deliveredSequence, pullGate);
  } finally {
    writer.end();
  }
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
  while (!hub.hasTerminal()) {
    if (clientDisconnected(res)) return;

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

    if (batch.gap) {
      const gapFrame = hub.recordGap(batch.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
      }
    }

    const pending = hub.framesAfter(sequence);
    for (const frame of pending) {
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
  while (!hub.hasTerminal()) {
    if (clientDisconnected(res)) return;

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

    if (batch.gap) {
      const gapFrame = hub.recordGap(batch.gap);
      if (gapFrame) {
        const gapResult = await writer.writeFrame(gapFrame);
        if (gapResult !== 'ok') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
      }
    }

    const pending = hub.framesAfter(sequence);
    for (const frame of pending) {
      const result = await writer.writeFrame(frame);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }
      sequence = frameSequence(frame);
      if (frame.isTerminal) return;
    }

    if (!batch.has_more && hub.hasTerminal()) return;
    if (!batch.has_more && (batch.events?.length ?? 0) === 0) {
      await sleep(25);
    }
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
