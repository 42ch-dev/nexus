import { SSE_MAX_AGGREGATE_PENDING_BYTES } from './config.js';

/** Process-wide Node-owned serialized frame / handoff byte budget. */
let reservedBytes = 0;

export function environmentBudgetReserved(): number {
  return reservedBytes;
}

export function tryReserveEnvironmentBytes(bytes: number): boolean {
  if (bytes <= 0) return true;
  if (reservedBytes + bytes > SSE_MAX_AGGREGATE_PENDING_BYTES) return false;
  reservedBytes += bytes;
  return true;
}

export function releaseEnvironmentBytes(bytes: number): void {
  if (bytes <= 0) return;
  reservedBytes = Math.max(0, reservedBytes - bytes);
}

export function resetEnvironmentBudgetForTests(): void {
  reservedBytes = 0;
}
