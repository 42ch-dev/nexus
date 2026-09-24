import type {
  ActorRef,
  SessionViewpoint,
  ProviderHostEvent,
} from '@42ch/nexus-contracts';
import { REGISTRY_MAX_TERMINAL_OPERATIONS } from './config.js';
import type { OperationEventHub } from './sse.js';

export interface ProviderSessionRecord {
  sessionId: string;
  providerId: string;
  state: string;
  activeOpId: string | null;
  model?: string;
  /**
   * The stored Actor pair echoed by native truth for a core-owned Actor
   * session. Its presence is what routes a command to the Actor Host authority
   * instead of the provider-only lane, so it is never inferred from a payload:
   * it is copied from the native create/query row the authority returned.
   */
  actorRef?: ActorRef;
  viewpoint?: SessionViewpoint;
}

export interface ProviderOperationRecord {
  operationId: string;
  sessionId: string;
  providerId: string;
  /**
   * Native-authoritative status when hydrated (`running` | `cancelling` |
   * `pending` | `completed` | `failed` | `unknown`), or the local `started`
   * value between admission and the first native observation. Never a constant.
   */
  status: string;
  terminalEvent: ProviderHostEvent | null;
  terminalTranscript: string | null;
  /**
   * True when this operation belongs to a stored Actor session: its terminal
   * truth and its live/terminal bounds are the core authority's (128 active /
   * 1024 terminal), not this transport mirror's, so it never charges the
   * provider-only live-operation cap.
   */
  actorBacked?: boolean;
}

/**
 * Statuses that mean the operation can no longer be cancelled. Covers the local
 * terminal mapping derived from `ProviderHostEvent` (`finished`/`failed`/
 * `stopped`), the native wire statuses transcribed from `operation_status_wire`
 * in `crates/nexus-agent-host/src/providers/port.rs` (`completed` for Ready/
 * Stopped, `failed` for the error terminals), the cancel acknowledgment
 * (`cancelled`), and the native `interrupted` terminal produced when a
 * `SessionStopped` settles an active JS-provider operation.
 */
const TERMINAL_OPERATION_STATUSES = new Set([
  'finished',
  'failed',
  'stopped',
  'completed',
  'cancelled',
  'interrupted',
]);

export function isTerminalOperationStatus(status: string): boolean {
  return TERMINAL_OPERATION_STATUSES.has(status);
}

export class ProviderRegistry {
  private sessions = new Map<string, ProviderSessionRecord>();
  private operations = new Map<string, ProviderOperationRecord>();
  private hubs = new Map<string, OperationEventHub>();
  private terminalOrder: string[] = [];

  sessionRecord(sessionId: string): ProviderSessionRecord | undefined {
    return this.sessions.get(sessionId);
  }

  operationRecord(operationId: string): ProviderOperationRecord | undefined {
    return this.operations.get(operationId);
  }

  hubForOperation(operationId: string): OperationEventHub | undefined {
    return this.hubs.get(operationId);
  }

  ensureHub(operationId: string, create: () => OperationEventHub): OperationEventHub {
    let hub = this.hubs.get(operationId);
    if (!hub) {
      hub = create();
      this.hubs.set(operationId, hub);
    }
    return hub;
  }

  /**
   * Live (non-terminal) provider-only operations. The transport admission cap
   * reads this before dispatching a provider effect, so a stalled/hung
   * provider population cannot grow without bound. Actor-backed operations are
   * excluded: the core authority owns their live bound, so this cap keeps
   * meaning exactly what it meant before Actor sessions were served here.
   */
  activeOperationCount(): number {
    let count = 0;
    for (const op of this.operations.values()) {
      if (op.actorBacked) continue;
      if (!op.terminalEvent && !isTerminalOperationStatus(op.status)) count += 1;
    }
    return count;
  }

  registerSession(record: ProviderSessionRecord): void {
    this.sessions.set(record.sessionId, record);
  }

  registerOperation(record: ProviderOperationRecord): void {
    this.operations.set(record.operationId, record);
    if (isTerminalOperationStatus(record.status)) {
      this.trackTerminal(record.operationId);
    } else {
      const session = this.sessions.get(record.sessionId);
      if (session) {
        session.activeOpId = record.operationId;
        session.state = 'Running';
      }
    }
    this.evictTerminalOperationsIfNeeded();
  }

  clearSessionOperation(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.activeOpId = null;
      if (session.state === 'Running') {
        session.state = 'Ready';
      }
    }
  }

  /**
   * Settle a cached operation to a terminal status (e.g. an accepted cancel).
   * The cached record is updated so `activeOperationCount` no longer charges it
   * and a later GET surfaces the same terminal truth as native.
   */
  settleOperationStatus(operationId: string, status: string): void {
    const op = this.operations.get(operationId);
    if (!op) return;
    if (isTerminalOperationStatus(op.status)) return;
    op.status = status;
    this.trackTerminal(operationId);
    const session = this.sessions.get(op.sessionId);
    if (session?.activeOpId === operationId) {
      session.activeOpId = null;
      session.state = 'Ready';
    }
    this.evictTerminalOperationsIfNeeded();
  }

  /**
   * Adopt the first terminal event for an operation. Returns whether the
   * event was accepted: a status settled terminal earlier (e.g. an accepted
   * cancel, or a native-hydrated terminal) is authoritative, so a later
   * provider event is refused rather than overwriting the outcome the
   * durable journal already recorded. Callers must not publish a refused
   * event as the stream's terminal.
   */
  finishOperation(operationId: string, terminal: ProviderHostEvent, transcript: string | null): boolean {
    const op = this.operations.get(operationId);
    if (!op || op.terminalEvent || isTerminalOperationStatus(op.status)) return false;
    op.terminalEvent = terminal;
    op.terminalTranscript = transcript;
    op.status = terminalEventStatus(terminal);
    const session = this.sessions.get(op.sessionId);
    if (session?.activeOpId === operationId) {
      session.activeOpId = null;
      session.state = 'Ready';
    }
    this.trackTerminal(operationId);
    this.evictTerminalOperationsIfNeeded();
    return true;
  }

  removeSession(sessionId: string): void {
    this.sessions.delete(sessionId);
    for (const [opId, op] of this.operations) {
      if (op.sessionId === sessionId) {
        this.disposeOperation(opId);
      }
    }
  }

  private trackTerminal(operationId: string): void {
    if (!this.terminalOrder.includes(operationId)) {
      this.terminalOrder.push(operationId);
    }
  }

  private disposeOperation(operationId: string): void {
    const hub = this.hubs.get(operationId);
    if (hub) {
      hub.dispose();
      this.hubs.delete(operationId);
    }
    this.operations.delete(operationId);
    this.terminalOrder = this.terminalOrder.filter((id) => id !== operationId);
  }

  private evictTerminalOperationsIfNeeded(): void {
    while (this.terminalOrder.length > REGISTRY_MAX_TERMINAL_OPERATIONS) {
      const evictId = this.terminalOrder.shift();
      if (!evictId) break;
      const op = this.operations.get(evictId);
      if (!op) continue;
      if (!op.terminalEvent && !isTerminalOperationStatus(op.status)) continue;
      const session = this.sessions.get(op.sessionId);
      if (session?.activeOpId === evictId) continue;
      this.disposeOperation(evictId);
    }
  }
}

function terminalEventStatus(event: ProviderHostEvent): string {
  if ('OpFinished' in event) return 'finished';
  if ('OpFailed' in event) return 'failed';
  if ('SessionStopped' in event) return 'stopped';
  return 'finished';
}
