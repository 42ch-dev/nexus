import {
  closeSync,
  existsSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  unlinkSync,
  writeSync,
} from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { randomUUID } from 'node:crypto';
import process from 'node:process';
import type { CoreServiceDiscovery } from '@42ch/nexus-contracts';
import { HttpError } from './errors.js';

/**
 * Launch/discovery file protocol (architecture §7, frozen before P5/P6).
 *
 * - Discovery record: `<user_home>/.nexus42/run/service.json`, written 0600 in
 *   a 0700 directory, published by atomic rename only after the listener, core
 *   and provider registry are ready (or as the explicit uninitialized shell).
 * - Start lock: `<user_home>/.nexus42/run/service.start.lock` serializes start
 *   publication between competing starts (including the removal that a close
 *   performs, so compare-instance-before-unlink cannot race a replacement).
 * - A stale lock whose owning pid is gone is removed; a lock held by a live
 *   process refuses the start. Pids are advisory context, never stop
 *   authorization — the record's `instance_id` is.
 * - Attacker-supplied symlinks are not followed: the run directory must be a
 *   real directory, the temp record is created exclusively with `wx`, and the
 *   final rename replaces the leaf instead of writing through it.
 * - Unix enforces 0700/0600 explicitly; Windows relies on the user-profile
 *   ACL (chmod has no POSIX meaning there), which already scopes the home to
 *   the current user.
 */

const RUN_DIR_SEGMENTS = ['.nexus42', 'run'] as const;
const RECORD_FILE = 'service.json';
const LOCK_FILE = 'service.start.lock';
const RECORD_MODE = 0o600;
const RUN_DIR_MODE = 0o700;
/** Bounded wait for a live owner's start window before refusing. */
const LOCK_ACQUIRE_TIMEOUT_MS = 3_000;
const LOCK_RETRY_DELAY_MS = 25;

/** The single ready line a service process prints on stdout (architecture §7). */
export const SERVICE_READY_PREFIX = 'NEXUS_SERVICE_READY ';

export function discoveryRecordPath(home: string): string {
  return join(home, ...RUN_DIR_SEGMENTS, RECORD_FILE);
}

export interface DiscoveryLockToken {
  pid: number;
  acquired_at: string;
  token: string;
}

function assertRealDirectory(path: string): void {
  let stats;
  try {
    stats = lstatSync(path);
  } catch {
    throw new HttpError(500, 'internal', 'service run directory is missing');
  }
  if (!stats.isDirectory()) {
    throw new HttpError(500, 'internal', 'service run path is not a directory');
  }
}

/** Create the private run directory without following a planted symlink. */
function ensureRunDir(home: string): string {
  const runDir = join(home, ...RUN_DIR_SEGMENTS);
  mkdirSync(runDir, { recursive: true, mode: RUN_DIR_MODE });
  assertRealDirectory(runDir);
  return runDir;
}

function pidAlive(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

function readLockToken(path: string): DiscoveryLockToken | null {
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, 'utf8'));
    if (
      parsed !== null &&
      typeof parsed === 'object' &&
      typeof (parsed as DiscoveryLockToken).pid === 'number' &&
      typeof (parsed as DiscoveryLockToken).token === 'string'
    ) {
      return parsed as DiscoveryLockToken;
    }
  } catch {
    // Unreadable/corrupt lock: treated as stale by the acquire loop.
  }
  return null;
}

function wait(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
}

function writeFileSyncExclusive(path: string, data: string, mode: number): void {
  const fd = openSync(path, 'wx', mode);
  try {
    const buffer = Buffer.from(data, 'utf8');
    let written = 0;
    while (written < buffer.length) {
      written += writeSync(fd, buffer, written);
    }
    closeSync(fd);
  } catch (error) {
    try {
      closeSync(fd);
    } catch {
      // The failing write already closed the descriptor.
    }
    rmSync(path, { force: true });
    throw error;
  }
}

/**
 * Serialize one start-publication/remove window per home. Fails with `busy`
 * while another live process holds the lock; clears a lock whose pid is gone.
 */
export async function withServiceStartLock<T>(home: string, fn: () => Promise<T>): Promise<T> {
  const runDir = ensureRunDir(home);
  const path = join(runDir, LOCK_FILE);
  const token: DiscoveryLockToken = {
    pid: process.pid,
    acquired_at: new Date().toISOString(),
    token: randomUUID(),
  };
  const deadline = Date.now() + LOCK_ACQUIRE_TIMEOUT_MS;
  let acquired = false;
  while (!acquired) {
    try {
      writeFileSyncExclusive(path, JSON.stringify(token), RECORD_MODE);
      acquired = true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      const holder = readLockToken(path);
      if (holder && pidAlive(holder.pid)) {
        if (Date.now() >= deadline) {
          throw new HttpError(
            503,
            'busy',
            'another service start owns the start lock for this home',
          );
        }
      } else {
        // Stale (dead owner) or corrupt: clear it and race the next attempt.
        // simplify: this clear races a successor's create in a ~25ms window;
        // the native writer protocol still fences the home, so a colliding
        // start fails loudly there. Use an O_EXCL re-verify (rename dance) if
        // multi-starter contention ever matters.
        rmSync(path, { force: true });
      }
      await wait(LOCK_RETRY_DELAY_MS);
    }
  }
  try {
    return await fn();
  } finally {
    // Release only if we still own the lock file (never delete a successor's).
    const holder = readLockToken(path);
    if (holder?.token === token.token) {
      rmSync(path, { force: true });
    }
  }
}

interface PublishedDiscovery {
  home: string;
  instanceId: string;
}

let published: PublishedDiscovery | null = null;

/** The record this process currently owns, if any. Test/observability seam. */
export function publishedDiscovery(): Readonly<PublishedDiscovery> | null {
  return published;
}

/**
 * Atomically publish the discovery record for `record.instance_id`:
 * exclusive temp file (0600) in the run directory, then rename over the leaf.
 * The rename replaces any attacker-planted symlink at the leaf instead of
 * writing through it.
 */
export async function publishDiscovery(record: CoreServiceDiscovery): Promise<void> {
  const home = record.user_home;
  if (!isAbsolute(home)) {
    throw new HttpError(500, 'internal', 'discovery home must be absolute');
  }
  const runDir = ensureRunDir(home);
  const tempPath = join(runDir, `${RECORD_FILE}.${process.pid}.${randomUUID()}.tmp`);
  writeFileSyncExclusive(tempPath, JSON.stringify(record), RECORD_MODE);
  renameSync(tempPath, discoveryRecordPath(home));
  published = { home, instanceId: record.instance_id };
}

export interface DiscoveryRemoval {
  /** True when this process owned the published record and removed it. */
  removed: boolean;
  /** Why nothing was removed: no published record, or a replacement owns it. */
  reason: 'no_record' | 'not_owner' | 'removed';
}

/**
 * Remove the published discovery record — only when it still names
 * `instanceId` (compare-instance-before-unlink, evaluated under the start
 * lock so a replacement's publication cannot race the unlink).
 */
export async function removeOwnedDiscovery(instanceId: string): Promise<DiscoveryRemoval> {
  const owned = published;
  if (!owned || owned.instanceId !== instanceId) {
    return { removed: false, reason: 'no_record' };
  }
  return withServiceStartLock(owned.home, async () => {
    const path = discoveryRecordPath(owned.home);
    if (!existsSync(path)) {
      published = null;
      return { removed: false, reason: 'no_record' };
    }
    const current = readPublishedDiscovery(owned.home);
    if (!current || current.instance_id !== instanceId) {
      return { removed: false, reason: 'not_owner' };
    }
    unlinkSync(path);
    published = null;
    return { removed: true, reason: 'removed' };
  });
}

/** Read whatever record is currently published for `home` (null if unreadable). */
export function readPublishedDiscovery(home: string): CoreServiceDiscovery | null {
  const path = discoveryRecordPath(home);
  if (!existsSync(path)) return null;
  try {
    const stats = statSync(path);
    if (!stats.isFile()) return null;
    return JSON.parse(readFileSync(path, 'utf8')) as CoreServiceDiscovery;
  } catch {
    return null;
  }
}

export interface DiscoveryRecordInput {
  instanceId: string;
  userHome: string;
  endpoint: CoreServiceDiscovery['endpoint'];
  tlsFingerprint: string | null;
  readiness: 'ready' | 'uninitialized';
  creatorId: string | null;
  workspaceSlug: string | null;
  engineEpoch: number | null;
}

/**
 * Build the closed v1 discovery record. Shell-vs-ready is structural: a ready
 * record carries creator/workspace/epoch, an uninitialized shell carries all
 * three as null — the frozen schema rejects anything else.
 */
export function createDiscoveryRecord(input: DiscoveryRecordInput): CoreServiceDiscovery {
  const shared = {
    schema_version: 1 as const,
    instance_id: input.instanceId,
    pid: process.pid,
    user_home: input.userHome,
    endpoint: input.endpoint,
    tls_fingerprint: input.tlsFingerprint,
    protocol_version: 1 as const,
  };
  if (input.readiness === 'ready') {
    if (!input.creatorId || !input.workspaceSlug || input.engineEpoch === null) {
      throw new HttpError(
        500,
        'internal',
        'a ready discovery record requires creator, workspace and engine epoch',
      );
    }
    return {
      ...shared,
      creator_id: input.creatorId,
      workspace_slug: input.workspaceSlug,
      engine_epoch: input.engineEpoch,
      readiness: 'ready',
    };
  }
  return {
    ...shared,
    creator_id: null,
    workspace_slug: null,
    engine_epoch: null,
    readiness: 'uninitialized',
  };
}
