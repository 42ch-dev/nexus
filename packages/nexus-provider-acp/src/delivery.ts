import type { ProviderEventBatch } from '@42ch/nexus-contracts';
import type { CoreStreamGap, ProviderHostEvent } from './contracts.js';

export const MAX_PENDING_MESSAGES = 64;
export const MAX_PENDING_BYTES = 1024 * 1024;
export const MAX_EVENT_BYTES = 256 * 1024;
export const MAX_EMITTED_BYTES = 4 * 1024 * 1024;
export const MAX_CONTROL_BYTES = 4 * 1024;
export const MAX_BATCH_EVENTS = 16;
export const MAX_BATCH_BYTES = 256 * 1024;

export type DeliveryTerminal =
  | { kind: 'finished'; reason: 'end_turn' | 'cancelled' | 'max_tokens' | 'max_turn_requests' | 'refusal' }
  | { kind: 'failed'; category: string; message: string }
  | { kind: 'gap'; gap: CoreStreamGap };

type QueuedEvent = { bytes: number; event: ProviderHostEvent };

function eventBytes(event: ProviderHostEvent): number {
  return new TextEncoder().encode(JSON.stringify(event)).length;
}

export class OperationDelivery {
  readonly operationId: string;
  private readonly queue: QueuedEvent[] = [];
  private pendingMessages = 0;
  private pendingBytes = 0;
  private emittedBytes = 0;
  private terminal: DeliveryTerminal | null = null;
  private gap: CoreStreamGap | null = null;
  private pullInFlight = false;
  private overflow = false;
  private closed = false;

  constructor(operationId: string) {
    this.operationId = operationId;
  }

  get hasOverflow(): boolean {
    return this.overflow;
  }

  get isTerminal(): boolean {
    return this.terminal !== null || this.overflow;
  }

  tryBeginPull(): boolean {
    if (this.pullInFlight) return false;
    this.pullInFlight = true;
    return true;
  }

  endPull(): void {
    this.pullInFlight = false;
  }

  enqueue(event: ProviderHostEvent): boolean {
    if (this.closed || this.overflow) return false;
    const bytes = eventBytes(event);
    if (bytes > MAX_EVENT_BYTES) {
      this.markOverflow();
      return false;
    }
    if (
      this.pendingMessages + 1 > MAX_PENDING_MESSAGES ||
      this.pendingBytes + bytes > MAX_PENDING_BYTES ||
      this.emittedBytes + bytes > MAX_EMITTED_BYTES
    ) {
      this.markOverflow();
      return false;
    }
    this.pendingMessages += 1;
    this.pendingBytes += bytes;
    this.emittedBytes += bytes;
    this.queue.push({ bytes, event });
    return true;
  }

  setTerminal(terminal: DeliveryTerminal): void {
    if (this.closed || this.overflow) return;
    const controlBytes = new TextEncoder().encode(JSON.stringify(terminal)).length;
    if (controlBytes > MAX_CONTROL_BYTES) {
      this.markOverflow();
      return;
    }
    this.terminal = terminal;
  }

  setGap(gap: CoreStreamGap): void {
    if (this.closed || this.overflow) return;
    const controlBytes = new TextEncoder().encode(JSON.stringify(gap)).length;
    if (controlBytes > MAX_CONTROL_BYTES) {
      this.markOverflow();
      return;
    }
    this.gap = gap;
  }

  private markOverflow(): void {
    if (this.overflow) return;
    this.overflow = true;
    this.terminal = { kind: 'failed', category: 'provider_error', message: 'delivery_overflow' };
    this.queue.length = 0;
    this.pendingMessages = 0;
    this.pendingBytes = 0;
  }

  pull(maxEvents: number, maxBytes: number): ProviderEventBatch {
    const capEvents = Math.min(maxEvents, MAX_BATCH_EVENTS);
    const capBytes = Math.min(maxBytes, MAX_BATCH_BYTES);
    const events: ProviderHostEvent[] = [];
    let usedBytes = 0;
    while (events.length < capEvents && this.queue.length > 0) {
      const head = this.queue[0]!;
      if (usedBytes + head.bytes > capBytes && events.length > 0) break;
      if (head.bytes > capBytes && events.length === 0) break;
      this.queue.shift();
      this.pendingMessages = Math.max(0, this.pendingMessages - 1);
      this.pendingBytes = Math.max(0, this.pendingBytes - head.bytes);
      events.push(head.event);
      usedBytes += head.bytes;
    }
    const hasMore = this.queue.length > 0;
    return {
      operation_id: this.operationId,
      events,
      has_more: hasMore,
      gap: hasMore ? undefined : this.gap ?? undefined,
    };
  }

  drainTerminalEvents(sessionId: string, opId: string): ProviderHostEvent[] {
    if (!this.terminal) return [];
    if (this.terminal.kind === 'finished') {
      return [{ OpFinished: { session_id: sessionId, op_id: opId, reason: this.terminal.reason } }];
    }
    if (this.terminal.kind === 'failed') {
      return [{
        OpFailed: {
          session_id: sessionId,
          op_id: opId,
          error_category: this.terminal.category,
          error_message: this.terminal.message,
        },
      }];
    }
    return [];
  }

  close(): void {
    this.closed = true;
    this.queue.length = 0;
    this.pendingMessages = 0;
    this.pendingBytes = 0;
  }
}
