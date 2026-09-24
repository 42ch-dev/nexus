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
 * (`cancelled`), the native `interrupted` terminal produced when a
 * `SessionStopped` settles an active JS-provider operation.
 *
 * The core authority's own run statuses are deliberately NOT part of this set:
 * an Actor operation's cancellability and outcome are the authority's, read on
 * demand, and marking a settled Actor run terminal here would make the mirror
 * refuse the terminal frame the authority's retained observation still replays.
 * The Actor arm is aged out from the authority's own truth instead
 * (`retireActorOperation`).
 */
const TERMINAL_OPERATION_STATUS: Record<string, true> = {
  finished: true,
  failed: true,
  stopped: true,
  completed: true,
  cancelled: true,
  interrupted: true,
};

export function isTerminalOperationStatus(status: string): boolean {
  return TERMINAL_OPERATION_STATUS[status] === true;
}

export class ProviderRegistry {
  private sessions = new Map<string, ProviderSessionRecord>();
  private operations = new Map<string, ProviderOperationRecord>();
  private hubs = new Map<string, OperationEventHub>();
  private terminalOrder: string[] = [];
  /** Live SSE readers per operation: a hub with one is never disposed under it. */
  private attachedStreams = new Map<string, number>();
  /** Operations whose disposal was requested while a stream was attached. */
  private retirePending = new Set<string>();

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
   * Register one live SSE reader of an operation's hub, before that stream can
   * start pulling.
   *
   * A reader is already inside its delivery loop, so the record and the hub it
   * reads must outlive that loop. Disposal under it loses the outcome twice: the
   * record is gone, so `ingestEvents` drops the batch it had in flight, and the
   * hub reads as closed, so the loop ends the stream with neither a terminal
   * frame nor a resync gap. While a stream is attached, disposal is deferred to
   * the detach of the LAST reader instead of executed under it.
   */
  attachOperationStream(operationId: string): void {
    this.attachedStreams.set(operationId, (this.attachedStreams.get(operationId) ?? 0) + 1);
  }

  /**
   * Release one live SSE reader. The last detach applies any disposal that was
   * deferred while it read, so a retirement requested mid-stream still lands —
   * after the stream, never inside it.
   */
  detachOperationStream(operationId: string): void {
    const count = this.attachedStreams.get(operationId);
    if (count === undefined) return;
    if (count > 1) {
      this.attachedStreams.set(operationId, count - 1);
      return;
    }
    this.attachedStreams.delete(operationId);
    if (this.retirePending.delete(operationId)) this.disposeNow(operationId);
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

  /**
   * The Actor-backed records the mirror still holds. The Actor arm is bookkeeping
   * only — the authority owns the outcome and the observation — so it is aged out
   * by the authority's own truth (see `retireActorOperation`).
   */
  actorBackedOperations(): ProviderOperationRecord[] {
    return [...this.operations.values()].filter((op) => op.actorBacked === true);
  }

  /**
   * Mark one Actor operation on the transport mirror: its Actor ownership and
   * its session association, and nothing else. The authority owns the run's
   * status and outcome (read on demand through its own Character/generic
   * observation), so the mirror must not transcribe a status into a
   * cancellability claim here — nor leave a settled Actor run's session marked
   * busy by a transport cache.
   */
  markActorOperation(operationId: string, sessionId: string, providerId: string): void {
    this.operations.set(operationId, {
      operationId,
      sessionId,
      providerId,
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
    this.evictTerminalOperationsIfNeeded();
  }

  /**
   * Release the mirror's retention for one Actor operation — its record and its
   * hub together. The authority's detailed outcome and bounded observation live
   * in core (technical contract §5), so retiring transport bookkeeping here can
   * never lose a result; it only stops the mirror outliving the retention the
   * authority itself bounds. A live stream reading the operation defers the
   * release to its own detach, so no pull can be left holding a retired hub. A
   * provider-only record is never touched.
   */
  retireActorOperation(operationId: string): void {
    if (this.operations.get(operationId)?.actorBacked !== true) return;
    this.disposeOperation(operationId);
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
    // Never dispose a hub a live stream is reading (`attachOperationStream`):
    // the reader would observe the closed hub mid-pull and lose the outcome it
    // has in flight. Defer to that stream's detach instead.
    if ((this.attachedStreams.get(operationId) ?? 0) > 0) {
      this.retirePending.add(operationId);
      return;
    }
    this.disposeNow(operationId);
  }

  private disposeNow(operationId: string): void {
    const hub = this.hubs.get(operationId);
    if (hub) {
      hub.dispose();
      this.hubs.delete(operationId);
    }
    this.operations.delete(operationId);
    this.terminalOrder = this.terminalOrder.filter((id) => id !== operationId);
    this.retirePending.delete(operationId);
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
