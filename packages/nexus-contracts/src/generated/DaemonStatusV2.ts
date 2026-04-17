import type { SchemaVersion } from './CommonTypes';
/**
 * Daemon Status v2
 *
 * Response shape for GET /v1/local/daemon/status. Superset of v1 running-probe, wire-compatible. Per daemon-lifecycle-api-v2.md §7.1.
 *
 * @schema_version 2
 * @source daemon-status-v2.schema.json
 */

/** Inline enum type */
export type DaemonStatusV2LifecycleState = 'starting' | 'running' | 'degraded' | 'stopping' | 'failed';

/** Inline enum type */
export type DaemonStatusV2DegradedSubsystems = 'http' | 'db' | 'sync' | 'engine' | 'worker_mgr' | 'acp_registry';

/** Inline enum type */
export type SubsystemHealthEntryStatus = 'up' | 'degraded' | 'down';

/** Response shape for GET /v1/local/daemon/status. Superset of v1 running-probe, wire-compatible. Per daemon-lifecycle-api-v2.md §7.1. */
export interface DaemonStatusV2 {
  schema_version: number;
  lifecycle_state: DaemonStatusV2LifecycleState;
  version: string;
  implementation_scope: string;
  uptime_seconds?: number;
  started_at?: string;
  pid?: number;
  degraded?: { subsystems?: DaemonStatusV2DegradedSubsystems[]; reasons?: string[] };
  subsystems?: { http?: SubsystemHealthEntry; db?: SubsystemHealthEntry; sync?: SubsystemHealthEntry; engine?: SubsystemHealthEntry; worker_mgr?: SubsystemHealthEntry; acp_registry?: SubsystemHealthEntry };
  exit_code?: number;
  last_error?: string;
}
/** SubsystemHealthEntry */
export interface SubsystemHealthEntry {
  status: SubsystemHealthEntryStatus;
  last_check_ms?: number;
  active_sessions?: number;
  active_workers?: number;
  cache_age_ms?: number;
}
