import type { IpcRequest } from './ipc.js';

/** Tracks one outstanding pull per operation_id from admission through settlement. */
export class PullReservationLedger {
  private readonly byOperation = new Map<string, string>();

  pullOperationId(request: IpcRequest): string | null {
    if (request.operation !== 'pull') return null;
    const body = request.payload as { operation_id?: string } | undefined;
    if (typeof body?.operation_id !== 'string' || body.operation_id.length === 0) {
      return null;
    }
    return body.operation_id;
  }

  isReserved(operationId: string): boolean {
    return this.byOperation.has(operationId);
  }

  tryReserve(request: IpcRequest): boolean {
    const operationId = this.pullOperationId(request);
    if (!operationId) return false;
    if (this.byOperation.has(operationId)) return false;
    this.byOperation.set(operationId, request.request_id);
    return true;
  }

  release(request: IpcRequest): void {
    const operationId = this.pullOperationId(request);
    if (!operationId) return;
    if (this.byOperation.get(operationId) === request.request_id) {
      this.byOperation.delete(operationId);
    }
  }
}
