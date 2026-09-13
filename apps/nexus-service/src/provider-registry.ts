import type { ProviderHostEvent } from '@42ch/nexus-contracts';
import type { OperationEventHub } from './sse.js';

export interface ProviderSessionRecord {
  sessionId: string;
  providerId: string;
  state: string;
  activeOpId: string | null;
  model?: string;
}

export interface ProviderOperationRecord {
  operationId: string;
  sessionId: string;
  providerId: string;
  status: string;
  terminalEvent: ProviderHostEvent | null;
  terminalTranscript: string | null;
}

export class ProviderRegistry {
  private sessions = new Map<string, ProviderSessionRecord>();
  private operations = new Map<string, ProviderOperationRecord>();
  private hubs = new Map<string, OperationEventHub>();

  sessionRecord(sessionId: string): ProviderSessionRecord | undefined {
    return this.sessions.get(sessionId);
  }

  operationRecord(operationId: string): ProviderOperationRecord | undefined {
    return this.operations.get(operationId);
  }

  hubForOperation(operationId: string): OperationEventHub | undefined {
    return this.hubs.get(operationId);
  }

  ensureHub(operationId: string, sessionId: string, create: () => OperationEventHub): OperationEventHub {
    let hub = this.hubs.get(operationId);
    if (!hub) {
      hub = create();
      this.hubs.set(operationId, hub);
    }
    return hub;
  }

  registerSession(record: ProviderSessionRecord): void {
    this.sessions.set(record.sessionId, record);
  }

  registerOperation(record: ProviderOperationRecord): void {
    this.operations.set(record.operationId, record);
    const session = this.sessions.get(record.sessionId);
    if (session) {
      session.activeOpId = record.operationId;
      session.state = 'Running';
    }
  }

  clearSessionOperation(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.activeOpId = null;
      session.state = 'Ready';
    }
  }

  finishOperation(operationId: string, terminal: ProviderHostEvent, transcript: string | null): void {
    const op = this.operations.get(operationId);
    if (!op || op.terminalEvent) return;
    op.terminalEvent = terminal;
    op.terminalTranscript = transcript;
    op.status = terminalEventStatus(terminal);
    const session = this.sessions.get(op.sessionId);
    if (session && session.activeOpId === operationId) {
      session.activeOpId = null;
      session.state = 'Ready';
    }
  }

  removeSession(sessionId: string): void {
    this.sessions.delete(sessionId);
    for (const [opId, op] of this.operations) {
      if (op.sessionId === sessionId) {
        this.operations.delete(opId);
        this.hubs.delete(opId);
      }
    }
  }
}

function terminalEventStatus(event: ProviderHostEvent): string {
  if ('OpFinished' in event) return 'finished';
  if ('OpFailed' in event) return 'failed';
  if ('SessionStopped' in event) return 'stopped';
  return 'terminal';
}
