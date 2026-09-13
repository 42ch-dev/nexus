import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import type { ChildProcessWithoutNullStreams } from 'node:child_process';

/** Observed or Rust-admitted resulting process identity (schema-owned shape). */
export type ProcessIdentity = {
  pid: number;
  process_birth: string | null;
  group_id: string | null;
};

export function parseProcessIdentity(value: unknown): ProcessIdentity | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== 'object') throw new Error('invalid_recipe');
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record);
  const allowed = new Set(['pid', 'process_birth', 'group_id']);
  if (keys.some((key) => !allowed.has(key))) throw new Error('invalid_recipe');
  if (
    typeof record.pid !== 'number' ||
    !Number.isInteger(record.pid) ||
    record.pid < 0
  ) {
    throw new Error('invalid_recipe');
  }
  if (
    record.process_birth !== undefined &&
    record.process_birth !== null &&
    typeof record.process_birth !== 'string'
  ) {
    throw new Error('invalid_recipe');
  }
  if (
    record.group_id !== undefined &&
    record.group_id !== null &&
    typeof record.group_id !== 'string'
  ) {
    throw new Error('invalid_recipe');
  }
  return {
    pid: record.pid,
    process_birth: (record.process_birth as string | null | undefined) ?? null,
    group_id: (record.group_id as string | null | undefined) ?? null,
  };
}

function queryLinuxIdentity(pid: number): ProcessIdentity | null {
  try {
    const stat = readFileSync(`/proc/${pid}/stat`, 'utf8');
    const close = stat.lastIndexOf(')');
    if (close < 0) return null;
    const rest = stat.slice(close + 2).trim().split(/\s+/);
    const starttime = rest[19];
    const pgid = execFileSync('ps', ['-o', 'pgid=', '-p', String(pid)], {
      encoding: 'utf8',
    }).trim();
    if (!starttime || !pgid) return null;
    return { pid, process_birth: starttime, group_id: pgid };
  } catch {
    return null;
  }
}

function queryDarwinIdentity(pid: number): ProcessIdentity | null {
  try {
    const out = execFileSync('ps', ['-p', String(pid), '-o', 'lstart=,pgid='], {
      encoding: 'utf8',
    }).trim();
    const match = out.match(/^(.+?)\s+(\d+)\s*$/);
    if (!match) return null;
    const birth = match[1].trim();
    const pgid = match[2].trim();
    if (!birth || !pgid) return null;
    return { pid, process_birth: birth, group_id: pgid };
  } catch {
    return null;
  }
}

/** Query canonical OS-derived identity for a live pid. */
export function queryOsProcessIdentity(pid: number): ProcessIdentity | null {
  if (pid <= 0) return null;
  if (process.platform === 'linux') return queryLinuxIdentity(pid);
  if (process.platform === 'darwin') return queryDarwinIdentity(pid);
  return null;
}

/** Capture the spawned child's identity for cleanup fencing. */
export function observeProcessIdentity(child: ChildProcessWithoutNullStreams): ProcessIdentity {
  const pid = child.pid;
  if (pid === undefined || pid <= 0) throw new Error('provider_spawn_failed');
  const queried = queryOsProcessIdentity(pid);
  if (!queried) throw new Error('process_identity_unsupported');
  return queried;
}

/**
 * When `admitted` is non-null it is the Rust-admitted *resulting* identity
 * (post-spawn expectation). Null means observe-at-spawn only.
 */
export function bindProcessIdentity(
  admitted: ProcessIdentity | null,
  observed: ProcessIdentity,
): ProcessIdentity {
  if (!admitted) return observed;
  if (admitted.pid !== observed.pid) throw new Error('process_identity_mismatch');
  if (admitted.process_birth && admitted.process_birth !== observed.process_birth) {
    throw new Error('process_identity_mismatch');
  }
  if (admitted.group_id && admitted.group_id !== observed.group_id) {
    throw new Error('process_identity_mismatch');
  }
  return observed;
}

/** Re-query OS identity and compare all admitted fields before signal/reap. */
export function identityStillMatches(
  child: ChildProcessWithoutNullStreams,
  bound: ProcessIdentity,
): boolean {
  if (child.pid !== bound.pid) return false;
  // Already exited: pid match is sufficient; OS tables may be gone.
  if (child.exitCode !== null || child.signalCode !== null) return true;
  const current = queryOsProcessIdentity(bound.pid);
  if (!current) return false;
  if (bound.process_birth && current.process_birth !== bound.process_birth) return false;
  if (bound.group_id && current.group_id !== bound.group_id) return false;
  return true;
}

export function identitiesEqual(a: ProcessIdentity, b: ProcessIdentity): boolean {
  return (
    a.pid === b.pid &&
    a.process_birth === b.process_birth &&
    a.group_id === b.group_id
  );
}
