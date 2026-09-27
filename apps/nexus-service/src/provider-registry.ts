import type {
  ActorRef,
  SessionViewpoint,
  ProviderHostEvent,
} from '@42ch/nexus-contracts';
import {
  MAX_ACTIVE_PROVIDER_OPERATIONS,
  REGISTRY_MAX_ACTOR_OPERATIONS,
  REGISTRY_MAX_TERMINAL_OPERATIONS,
} from './config.js';
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
   * value between admission and the first native observation, or the terminal
   * projection of the operation's own reason (`finished` | `incomplete` |
   * `failed` | `stopped` | `cancelled`). Never a constant.
   */
  status: string;
  terminalEvent: ProviderHostEvent | null;
  /**
   * The terminal payload of a completed `end_turn` turn — a successful run's
   * transcript. Every other terminal keeps its event and reason but retains no
   * payload here, so a non-success can never read as a captured successful run.
   */
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
 * terminal mapping derived from `ProviderHostEvent` (`finished`/`incomplete`/
 * `failed`/`stopped`/`cancelled`), the native wire statuses transcribed from
 * `operation_status_wire` in `crates/nexus-agent-host/src/providers/port.rs`
 * (`completed` for Ready/Stopped, `failed` for the error terminals), the cancel
 * acknowledgment (`cancelled`), and the native `interrupted` terminal produced
 * when a `SessionStopped` settles an active JS-provider operation. `incomplete`
 * is the provider-named unfinished run (`max_tokens`/`max_turn_requests`) — the
 * core authority's own row for the same terminal — and it is terminal here for
 * exactly the same reason as the rest: the reason arrived, so no later event
 * may overwrite it.
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
  incomplete: true,
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
  /**
   * Provider-only dispatches admitted before their async provider effect and
   * not yet registered, keyed by the dispatch's own provider request id. The
   * admission cap counts them, so parallel dispatches cannot all pass the same
   * free-slot check while none of them has a row yet.
   */
  private pendingProviderDispatches = new Set<string>();

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
   * Whether one more live provider-only row fits. Live rows charge the cap, and
   * so do the dispatches admitted for rows not registered yet: the cap has to
   * count both or parallel dispatches pass the same free slot. Synchronous, so a
   * registration that reads it last cannot be overtaken by another admission.
   */
  private hasLiveProviderOperationRoom(): boolean {
    return (
      this.activeOperationCount() + this.pendingProviderDispatches.size <
      MAX_ACTIVE_PROVIDER_OPERATIONS
    );
  }

  /**
   * Admit one provider-only dispatch BEFORE its async provider effect, named by
   * that dispatch's own provider request id.
   *
   * The cap is read and charged synchronously, so two requests cannot both see
   * the same free slot: the second sees the first's reservation even though the
   * first has not registered a row yet. `false` means the dispatch must not be
   * sent at all — the transport refuses it with `busy` before any effect.
   */
  beginProviderOperationDispatch(dispatchId: string): boolean {
    if (!this.hasLiveProviderOperationRoom()) return false;
    this.pendingProviderDispatches.add(dispatchId);
    return true;
  }

  /**
   * Abandon a dispatch reservation whose row never materialized (provider
   * failure, or a refusal after the reservation). Idempotent: a dispatch whose
   * row was already registered releases nothing here.
   */
  endProviderOperationDispatch(dispatchId: string): void {
    this.pendingProviderDispatches.delete(dispatchId);
  }

  /**
   * Register the live row a reserved dispatch produced, consuming that
   * dispatch's reservation. The reservation is what guaranteed the room — a
   * dispatch holds one slot from admission until here — so the row is inserted
   * without a second capacity check, and the slot it takes is the slot it held.
   * Returns whether the row was registered; a dispatch id with no reservation
   * never registers anything.
   */
  registerAdmittedProviderOperation(
    dispatchId: string,
    record: ProviderOperationRecord,
  ): boolean {
    if (!this.pendingProviderDispatches.delete(dispatchId)) return false;
    this.insertOperation(record);
    return true;
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
   *
   * A mark is admission, not observation, so it never replaces a row it already
   * holds: every Actor stream re-marks its operation on (re)connect, and
   * rebuilding the row there rewound a settled record to `started` with no
   * terminal event while its id stayed in the terminal FIFO — the record then
   * read as live to that FIFO's own non-terminal skip, so it could never age
   * out and the settled terminal the mirror had already observed was lost. An
   * existing row therefore keeps its identity, association, status, terminal
   * event/transcript and FIFO position, and a mark carrying a different
   * association never takes over the row that association owns.
   *
   * The arm stays bounded: a mark is retained only while the Actor arm has room
   * (`retainActorOperation`), which retires the oldest row whose reader is gone
   * before admitting the new one. An arm full of rows pinned by live readers
   * retains nothing here and answers `false` rather than evicting a row out from
   * under its reader — a stream-heavy workload cannot accumulate one hub per
   * connect either way. Returns whether the mirror holds a row for the operation
   * now, so a caller never builds a hub for a row the registry dropped.
   */
  markActorOperation(operationId: string, sessionId: string, providerId: string): boolean {
    const existing = this.operations.get(operationId);
    if (existing) {
      // A settled row is the outcome this mirror already observed: admission
      // never rewinds it, and its terminal FIFO position stands.
      if (existing.terminalEvent || isTerminalOperationStatus(existing.status)) return true;
      // A live row stays owned by the association that admitted it; a mark with
      // another association must not take it over.
      if (existing.sessionId !== sessionId || existing.providerId !== providerId) return true;
      existing.actorBacked = true;
      return true;
    }
    return this.retainActorOperation({
      operationId,
      sessionId,
      providerId,
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
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

  /**
   * Register one operation row — the single registration authority every live
   * lane and both hydration paths go through.
   *
   * An Actor-backed row is admitted by retention alone (`retainActorOperation`):
   * the authority has already accepted the run, so the mirror either retains the
   * bookkeeping inside the Actor arm's bound or retains nothing. A live
   * provider-only row is admitted only while the live population has room,
   * counting dispatches already reserved for rows not yet registered
   * (`hasLiveProviderOperationRoom`); a cold hydration of a natively-live
   * operation is refused here instead of widening the population past the cap
   * the control reserve is proven against. Terminal rows are bounded by the
   * terminal arm's own eviction.
   *
   * Returns whether the registry retains the row now. Every refusal is total: no
   * record, no session state, no hub — a caller must not build a hub for a row
   * that was not retained, because that hub would hold control slots the reserve
   * proof excludes.
   */
  registerOperation(record: ProviderOperationRecord): boolean {
    if (record.actorBacked === true) return this.retainActorOperation(record);
    if (!isTerminalOperationStatus(record.status) && !this.hasLiveProviderOperationRoom()) {
      return false;
    }
    this.insertOperation(record);
    return true;
  }

  /**
   * Insert one row and its bookkeeping. The caller owns admission: `insertOperation`
   * never refuses, so a row reaches the registry only after the arm that will
   * charge it has room, or after a reservation guaranteed that room.
   */
  private insertOperation(record: ProviderOperationRecord): void {
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

  /**
   * Retain one Actor-backed row inside the Actor arm's bound.
   *
   * Room is made by retiring the oldest Actor row that no live reader holds:
   * `disposeOperation` defers a row a reader is still reading, so a pinned row is
   * never taken out from under its stream. When every row in the arm is pinned
   * and the arm is at its bound there is no room at all, and the new row is not
   * retained rather than evicting a reader's row or letting the arm (and with it
   * the hubs holding control slots) grow past the bound. Nothing is lost by
   * that: the authority owns the run and its observation, and the mirror re-marks
   * the operation on its next stream admission.
   */
  private retainActorOperation(record: ProviderOperationRecord): boolean {
    this.evictActorOperations(REGISTRY_MAX_ACTOR_OPERATIONS - 1);
    if (this.actorBackedCount() >= REGISTRY_MAX_ACTOR_OPERATIONS) return false;
    this.insertOperation(record);
    return true;
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
   *
   * The status is the terminal's own reason ({@link terminalEventStatus}),
   * and only a completed `end_turn` turn retains its payload as a successful
   * transcript: every other terminal keeps its event and reason for the
   * stream, but a payload stored under it would read as a captured
   * successful run.
   */
  finishOperation(operationId: string, terminal: ProviderHostEvent, transcript: string | null): boolean {
    const op = this.operations.get(operationId);
    if (!op || op.terminalEvent || isTerminalOperationStatus(op.status)) return false;
    const status = terminalEventStatus(terminal);
    op.terminalEvent = terminal;
    op.terminalTranscript = status === 'finished' ? transcript : null;
    op.status = status;
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

  private actorBackedCount(): number {
    let count = 0;
    for (const op of this.operations.values()) {
      if (op.actorBacked) count += 1;
    }
    return count;
  }

  /**
   * Retire Actor-backed rows — oldest first, record and hub together — until the
   * arm holds at most `limit`.
   *
   * The stream-admission path (`markActorOperation`, one per Actor SSE connect)
   * otherwise accumulated a record and a hub per distinct connected operation,
   * and those hubs hold control slots (the retained gap or terminal frame)
   * charged against the shared control reserve. Every Actor stream re-marks its
   * operation on connect, so retiring the oldest record costs at most the replay
   * history of a stream that is not attached — and a record with a live reader is
   * deferred to that reader's detach (`disposeOperation`), so the population this
   * bounds is the arm's own, plus one pinned record per attached stream.
   */
  private evictActorOperations(limit: number): void {
    if (this.actorBackedCount() <= limit) return;
    for (const [operationId, op] of this.operations) {
      if (!op.actorBacked) continue;
      // `disposeOperation` releases the record and its hub together, and defers
      // to a live reader instead of taking the hub out from under it.
      this.disposeOperation(operationId);
      if (this.actorBackedCount() <= limit) return;
    }
  }
}

/**
 * The mirror's projection of one terminal `ProviderHostEvent` onto the status it
 * reports. Derived from the event's own reason, never defaulted to success:
 * `OpFinished` carries the provider's whole stop-reason set, so a blanket
 * `finished` recorded an exhausted run (`max_tokens`/`max_turn_requests` — the
 * core authority's own `incomplete` row) and a cancellation as successes on this
 * lane, and stored their payloads as successful transcripts.
 *
 * `refusal` is the failure row here (never `incomplete`): the ACP adapter
 * reports a post-start refusal as `OpFailed`, and only the core's own
 * `OpFinished(Refusal)` arm maps that reason to `incomplete`. A terminal the
 * enumeration does not carry — a reason outside the wire set, or an event that
 * is no terminal at all — fails closed as `failed` rather than reading as a
 * completed turn.
 */
function terminalEventStatus(event: ProviderHostEvent): string {
  if ('OpFailed' in event) return 'failed';
  if ('SessionStopped' in event) return 'stopped';
  if (!('OpFinished' in event)) return 'failed';
  switch (event.OpFinished.reason) {
    case 'end_turn':
      return 'finished';
    case 'max_tokens':
    case 'max_turn_requests':
      return 'incomplete';
    case 'cancelled':
      return 'cancelled';
    case 'refusal':
    default:
      return 'failed';
  }
}
