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
  private terminalReturned = false;
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

  get hasPendingTerminal(): boolean {
    return this.terminal !== null && !this.terminalReturned;
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
    if (this.closed || this.overflow || this.terminal) return false;
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

  /** First terminal wins; later attempts are ignored. */
  trySetTerminal(terminal: DeliveryTerminal): boolean {
    if (this.closed || this.overflow || this.terminal !== null) return false;
    const controlBytes = new TextEncoder().encode(JSON.stringify(terminal)).length;
    if (controlBytes > MAX_CONTROL_BYTES) {
      this.markOverflow();
      return false;
    }
    this.terminal = terminal;
    return true;
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

  /** Pull data events only; terminal uses a separate control slot on a later pull. */
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
    const hasMoreData = this.queue.length > 0;
    return {
      operation_id: this.operationId,
      events,
      has_more: hasMoreData || this.hasPendingTerminal,
      gap: hasMoreData ? undefined : this.gap ?? undefined,
    };
  }

  /** Consume the pending terminal control slot exactly once after data is drained. */
  consumeTerminal(sessionId: string, opId: string): ProviderHostEvent[] | null {
    if (!this.hasPendingTerminal || !this.terminal) return null;
    const events = this.materializeTerminal(sessionId, opId);
    const controlBytes = new TextEncoder().encode(JSON.stringify(events)).length;
    if (controlBytes > MAX_CONTROL_BYTES) {
      this.markOverflow();
      return this.materializeTerminal(sessionId, opId);
    }
    this.terminalReturned = true;
    return events;
  }

  private materializeTerminal(sessionId: string, opId: string): ProviderHostEvent[] {
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
