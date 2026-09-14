import {
  ENVIRONMENT_TOTAL_CEILING_BYTES,
  MAX_ACTIVE_PROVIDER_OPERATIONS,
  SSE_CONTROL_RESERVED_TOTAL_BYTES,
  SSE_MAX_AGGREGATE_PENDING_BYTES,
  SSE_MAX_TOTAL_SUBSCRIBERS,
  SSE_SOCKET_RESERVED_BYTES,
} from './config.js';

/**
 * Fixed native ceilings transcribed from the actual Rust code — never from a
 * label. Each is a hard `busy`/`input_too_large` admission bound inside the
 * native core, so the aggregate worst case is real, not aspirational:
 *
 * - `crates/nexus-core-node/src/env_state.rs` `MAX_PENDING_BYTES_TOTAL`
 *   → shared TSFN callback byte budget (1 MiB).
 * - `crates/nexus-agent-host/src/providers/port.rs` `MAX_PENDING_BYTES_PER_OP`
 *   → undelivered provider wire bytes per operation (1 MiB).
 * - `crates/nexus-acp-host/src/localset_bridge.rs` `MAX_PENDING_BYTES`
 *   → queued ACP delivery payload bytes per operation (1 MiB).
 * - native generic handoff + LocalSet bridge, combined worst case (2 MiB).
 */
export const TSFN_SHARED_BYTES = 1024 * 1024;
export const NATIVE_PROVIDER_BYTES_PER_OP = 1024 * 1024;
export const ACP_DELIVERY_BYTES_PER_OP = 1024 * 1024;
export const NATIVE_GENERIC_LOCALSET_BYTES = 2 * 1024 * 1024;

/**
 * Conservative process-wide byte proof. The Node-owned categories are capped by
 * live admission; the native categories are static per-operation ceilings
 * multiplied by the active-operation cap. `totalBytes` is the worst case the
 * environment can reach; it MUST stay at or below
 * {@link ENVIRONMENT_TOTAL_CEILING_BYTES}.
 */
export interface EnvironmentBudgetProof {
  tsfnSharedBytes: number;
  nativeProviderBytes: number;
  acpDeliveryBytes: number;
  nativeGenericLocalsetBytes: number;
  nodeRetainedBytes: number;
  nodeControlBytes: number;
  socketReservedBytes: number;
  totalBytes: number;
  ceilingBytes: number;
}

export function environmentBudgetProof(): EnvironmentBudgetProof {
  const ops = MAX_ACTIVE_PROVIDER_OPERATIONS;
  const nativeProviderBytes = NATIVE_PROVIDER_BYTES_PER_OP * ops;
  const acpDeliveryBytes = ACP_DELIVERY_BYTES_PER_OP * ops;
  const socketReservedBytes = SSE_SOCKET_RESERVED_BYTES * SSE_MAX_TOTAL_SUBSCRIBERS;
  // Data pool: retained hub *data* frames only. Control slots (terminal/gap) are
  // accounted separately via `nodeControlBytes`, so a saturated data pool still
  // permits a bounded terminal/gap.
  const nodeRetainedBytes = SSE_MAX_AGGREGATE_PENDING_BYTES;
  const nodeControlBytes = SSE_CONTROL_RESERVED_TOTAL_BYTES;
  const totalBytes =
    TSFN_SHARED_BYTES +
    nativeProviderBytes +
    acpDeliveryBytes +
    NATIVE_GENERIC_LOCALSET_BYTES +
    nodeRetainedBytes +
    nodeControlBytes +
    socketReservedBytes;
  return {
    tsfnSharedBytes: TSFN_SHARED_BYTES,
    nativeProviderBytes,
    acpDeliveryBytes,
    nativeGenericLocalsetBytes: NATIVE_GENERIC_LOCALSET_BYTES,
    nodeRetainedBytes,
    nodeControlBytes,
    socketReservedBytes,
    totalBytes,
    ceilingBytes: ENVIRONMENT_TOTAL_CEILING_BYTES,
  };
}

const proof = environmentBudgetProof();
if (proof.totalBytes > proof.ceilingBytes) {
  throw new Error(
    `environment byte proof ${proof.totalBytes} exceeds ceiling ${proof.ceilingBytes}`,
  );
}

/** Node-owned serialized data frames retained by operation hubs. */
let frameBytes = 0;
/** Control-frame slot bytes (terminal+gap), a pool entirely separate from data. */
let controlBytes = 0;
/** Per-socket reservations held for the lifetime of an SSE subscriber. */
let socketReservations = 0;

export function environmentBudgetReserved(): number {
  return frameBytes + controlBytes + socketReservations * SSE_SOCKET_RESERVED_BYTES;
}

/**
 * Charge one serialized control frame (terminal/gap) against the dedicated
 * control reserve. This pool never competes with the data pool, so a full data
 * budget still permits a bounded terminal/gap — the stream can always fail
 * closed with an explicit resync gap.
 */
export function tryReserveControlBytes(bytes: number): boolean {
  if (bytes <= 0) return true;
  if (controlBytes + bytes > SSE_CONTROL_RESERVED_TOTAL_BYTES) return false;
  controlBytes += bytes;
  return true;
}

export function releaseControlBytes(bytes: number): void {
  if (bytes <= 0) return;
  controlBytes = Math.max(0, controlBytes - bytes);
}

/**
 * Charge a serialized data frame before retaining it in an operation hub.
 * Eviction/disposal releases the charge, so this counter tracks live retained
 * bytes rather than a monotonic label.
 */
export function tryReserveEnvironmentBytes(bytes: number): boolean {
  if (bytes <= 0) return true;
  if (frameBytes + bytes > SSE_MAX_AGGREGATE_PENDING_BYTES) return false;
  frameBytes += bytes;
  return true;
}

export function releaseEnvironmentBytes(bytes: number): void {
  if (bytes <= 0) return;
  frameBytes = Math.max(0, frameBytes - bytes);
}

/** Reserve one socket's handoff bytes; false when the socket sub-cap is reached. */
export function reserveEnvironmentSocket(): boolean {
  if (socketReservations + 1 > SSE_MAX_TOTAL_SUBSCRIBERS) return false;
  socketReservations += 1;
  return true;
}

export function releaseEnvironmentSocket(): void {
  socketReservations = Math.max(0, socketReservations - 1);
}

export function resetEnvironmentBudgetForTests(): void {
  frameBytes = 0;
  controlBytes = 0;
  socketReservations = 0;
}
