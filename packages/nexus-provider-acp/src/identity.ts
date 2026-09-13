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

/** Capture the spawned child's identity for cleanup fencing. */
export function observeProcessIdentity(child: ChildProcessWithoutNullStreams): ProcessIdentity {
  const pid = child.pid;
  if (pid === undefined || pid <= 0) throw new Error('provider_spawn_failed');
  return {
    pid,
    process_birth: String(process.hrtime.bigint()),
    group_id: String(pid),
  };
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

export function identityStillMatches(
  child: ChildProcessWithoutNullStreams,
  bound: ProcessIdentity,
): boolean {
  return child.pid === bound.pid;
}
