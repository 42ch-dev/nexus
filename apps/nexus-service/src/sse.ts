import { randomUUID } from 'node:crypto';
import type { ServerResponse } from 'node:http';
import type { ProviderEventBatch, ProviderHostEvent } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import {
  PROVIDER_PULL_MAX_BYTES,
  PROVIDER_PULL_MAX_EVENTS,
  SSE_DRAIN_TIMEOUT_MS,
  SSE_MAX_AGGREGATE_PENDING_BYTES,
  SSE_MAX_OUTSTANDING_FRAME_BYTES,
  SSE_MAX_SUBSCRIBERS_PER_SESSION,
  SSE_RESERVED_CONTROL_BYTES,
  SSE_SOCKET_HIGH_WATER_MARK,
} from './config.js';
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

const sessionSubscriberCounts = new Map<string, number>();

export class OperationEventHub {
  readonly operationId: string;
  readonly sessionId: string;
  readonly epoch = randomUUID();
  private sequence = 0;
  private frames: StoredFrame[] = [];
  private closed = false;
  private terminalRecorded = false;

  constructor(operationId: string, sessionId: string) {
    this.operationId = operationId;
    this.sessionId = sessionId;
  }

  recordEvent(event: ProviderHostEvent): StoredFrame {
    const data = JSON.stringify(event);
    const frame = this.makeFrame('provider_event', data, isTerminalHostEvent(event));
    this.frames.push(frame);
    if (frame.isTerminal) {
      this.terminalRecorded = true;
      this.closed = true;
    }
    return frame;
  }

  recordGap(gap: CoreStreamGap): StoredFrame {
    const frame = this.makeFrame('gap', JSON.stringify(gap), false, true);
    this.frames.push(frame);
    return frame;
  }

  private makeFrame(event: string, data: string, isTerminal: boolean, isControl = false): StoredFrame {
    const sequence = ++this.sequence;
    const id = `${this.epoch}:${sequence}`;
    const wireBytes = Buffer.byteLength(`id: ${id}\nevent: ${event}\ndata: ${data}\n\n`, 'utf8');
    return { id, event, data, wireBytes, isControl: isControl || event === 'gap', isTerminal };
  }

  replayAfter(cursor: string | undefined): StoredFrame[] {
    if (!cursor) return [...this.frames];
    const parsed = parseCursor(cursor);
    if (!parsed) throw new HttpError(400, 'invalid_input', 'malformed SSE cursor');
    if (parsed.sequence >= this.sequence) {
      throw new HttpError(400, 'invalid_input', 'SSE cursor is in the future', { variant: 'future_cursor' });
    }
    if (parsed.epoch !== this.epoch) {
      throw new HttpError(400, 'invalid_input', 'SSE cursor epoch unavailable', { variant: 'history_unavailable' });
    }
    return this.frames.filter((frame) => frameSequence(frame) > parsed.sequence);
  }

  framesAfter(sequence: number): StoredFrame[] {
    return this.frames.filter((frame) => frameSequence(frame) > sequence);
  }

  markClosed(): void {
    this.closed = true;
  }

  isClosed(): boolean {
    return this.closed;
  }

  hasTerminal(): boolean {
    return this.terminalRecorded;
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

function formatSse(frame: StoredFrame): string {
  return `id: ${frame.id}\nevent: ${frame.event}\ndata: ${frame.data}\n\n`;
}

function inspectUrl(operationId: string): string {
  return `/v1/daemon/agent-host/operations/${operationId}`;
}

class SseWriter {
  private aggregatePending = 0;
  private outstandingFrame: StoredFrame | null = null;
  private terminalWritten = false;

  constructor(
    private readonly res: ServerResponse,
    private readonly hub: OperationEventHub,
  ) {
    if (this.res.socket) this.res.socket.setNoDelay(true);
  }

  private charge(frame: StoredFrame): boolean {
    if (!frame.isControl && frame.wireBytes > SSE_MAX_OUTSTANDING_FRAME_BYTES) return false;
    if (!frame.isControl && this.outstandingFrame) return false;
    const chargeBytes = frame.isControl
      ? Math.min(frame.wireBytes, SSE_RESERVED_CONTROL_BYTES)
      : frame.wireBytes;
    if (!frame.isControl && this.aggregatePending + chargeBytes > SSE_MAX_AGGREGATE_PENDING_BYTES) return false;
    if (!frame.isControl) {
      this.aggregatePending += chargeBytes;
      this.outstandingFrame = frame;
    }
    return true;
  }

  private release(frame: StoredFrame): void {
    if (!frame.isControl) {
      this.aggregatePending = Math.max(0, this.aggregatePending - frame.wireBytes);
      if (this.outstandingFrame?.id === frame.id) this.outstandingFrame = null;
    }
  }

  async writeFrame(frame: StoredFrame): Promise<'ok' | 'blocked' | 'overflow' | 'disconnect'> {
    if (this.res.writableEnded || this.res.destroyed) return 'disconnect';
    if (frame.isTerminal && this.terminalWritten) return 'ok';
    if (!this.charge(frame)) return 'overflow';
    const accepted = this.res.write(formatSse(frame), 'utf8');
    if (!accepted) {
      const drained = await waitForDrain(this.res, SSE_DRAIN_TIMEOUT_MS);
      this.release(frame);
      if (!drained) return 'disconnect';
      return 'blocked';
    }
    this.release(frame);
    if (frame.isTerminal) {
      this.terminalWritten = true;
      this.hub.markClosed();
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

function sessionSubscriberCount(sessionId: string): number {
  return sessionSubscriberCounts.get(sessionId) ?? 0;
}

function isSocketWritable(res: ServerResponse): boolean {
  if (res.writableEnded || res.destroyed) return false;
  const writableLength = res.socket?.writableLength ?? 0;
  return writableLength < SSE_SOCKET_HIGH_WATER_MARK;
}

async function emitInterruptedGap(writer: SseWriter, hub: OperationEventHub, operationId: string): Promise<void> {
  const gap: CoreStreamGap = {
    reason: 'interrupted',
    operation_id: operationId,
    resync_required: true,
    inspect_url: inspectUrl(operationId),
  };
  await writer.writeFrame(hub.recordGap(gap));
}

function ingestEvents(service: ServiceCore, operationId: string, events: ProviderHostEvent[]): void {
  const op = service.providerRegistry.operationRecord(operationId);
  if (!op) return;
  const hub = service.providerRegistry.ensureHub(operationId, op.sessionId, () => new OperationEventHub(operationId, op.sessionId));
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

  const hub = service.providerRegistry.hubForOperation(operationId) ??
    service.providerRegistry.ensureHub(operationId, sessionId, () => new OperationEventHub(operationId, sessionId));

  const cursor = searchParams.get('cursor') ?? searchParams.get('last_event_id') ?? undefined;
  const replay = hub.replayAfter(cursor);

  res.writeHead(200, {
    'Content-Type': 'text/event-stream; charset=utf-8',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
  });

  const writer = new SseWriter(res, hub);
  let deliveredSequence = replay.length > 0 ? frameSequence(replay[replay.length - 1]!) : 0;
  if (cursor) {
    const parsed = parseCursor(cursor);
    if (parsed) deliveredSequence = parsed.sequence;
  }

  try {
    for (const frame of replay) {
      const result = await writer.writeFrame(frame);
      if (result === 'overflow' || result === 'disconnect') {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }
      if (result === 'blocked') return;
      deliveredSequence = frameSequence(frame);
    }

    while (!hub.hasTerminal()) {
      if (!isSocketWritable(res)) {
        await emitInterruptedGap(writer, hub, operationId);
        return;
      }

      const batch = await service.core.nextProviderEvents(
        operationId,
        PROVIDER_PULL_MAX_EVENTS,
        PROVIDER_PULL_MAX_BYTES,
      );
      ingestEvents(service, operationId, batch.events ?? []);

      if (batch.gap) {
        const gapResult = await writer.writeFrame(hub.recordGap(batch.gap));
        if (gapResult !== 'ok') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
      }

      const pending = hub.framesAfter(deliveredSequence);
      for (const frame of pending) {
        const result = await writer.writeFrame(frame);
        if (result === 'overflow' || result === 'disconnect') {
          await emitInterruptedGap(writer, hub, operationId);
          return;
        }
        if (result === 'blocked') return;
        deliveredSequence = frameSequence(frame);
        if (frame.isTerminal) return;
      }

      if (!batch.has_more && hub.hasTerminal()) return;
      if (!batch.has_more && (batch.events?.length ?? 0) === 0) {
        await sleep(25);
      }
    }
  } finally {
    writer.end();
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
