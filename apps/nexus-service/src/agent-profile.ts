/**
 * Agent-profile family (I-000064): the daemon-global agent profile persisted in
 * the daemon home `<user_home>/.nexus42/config.toml` under the `agent_profile`
 * table — the same store the native core owns for `active_creator_id` and the
 * workspace selection.
 *
 * Storage semantics mirror the core CLI config (`read_cli_config` /
 * `write_cli_config` in `crates/nexus-core/src/creators.rs`): a missing file is
 * an empty document, every other key is preserved on write, and the document is
 * replaced atomically (temp file + rename in the same directory) so a reader
 * never observes a partially written config. Writes additionally hold the
 * daemon-home advisory lock (`config.toml.lock`) for the whole read-modify-
 * write: the core CLI and the desktop main write this file too.
 *
 * ONE daemon-global profile — matching desktop semantics. The legacy
 * per-creator localStorage key was a browser artifact and is migrated by the
 * web read path, never re-created here.
 */
import { randomUUID } from 'node:crypto';
import { mkdir, open, readFile, rename, rm, stat } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { parse, stringify, type TomlTable } from 'smol-toml';
import type { AgentProfile, AgentProfileResponse } from '@42ch/nexus-contracts';

import { HttpError } from './errors.js';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { wirePayload } from './world-kb.js';

/** The daemon home directory inside the raw user home (nexus-home-layout). */
const NEXUS_DIR = '.nexus42';
const CONFIG_FILE = 'config.toml';
const PROFILE_TABLE = 'agent_profile';
/** Bound on a stored agent name, in code points; longer values are rejected at the fold. */
const MAX_NAME_LENGTH = 200;
/** Bound on a stored launch command, in code points; longer values are rejected at the fold. */
const MAX_LAUNCH_COMMAND_LENGTH = 4096;

/**
 * Advisory lock file beside the config document — the ONE protocol its four
 * cross-process writers share (see `crates/nexus-core/src/config_lock.rs`, the
 * CLI `apps/nexus42/src/config.rs` and
 * `apps/desktop-electron/src/desktop-config.ts`): exclusive create for the
 * whole read-modify-write, stale steal after 10 s, release by unlink. `flock`
 * is not an option: Node's stdlib has none, and mixing lockfile-existence with
 * flock breaks mutual exclusion.
 *
 * A normal hold is milliseconds (a small TOML document, at most two fsyncs);
 * the 10 s age is a stale heuristic, not proof of a crash: a paused live
 * holder can look stale, and stealing can transiently yield two holders. Safe
 * only while all four writers stay user-paced — nothing enforces that.
 */
const LOCK_FILE = `${CONFIG_FILE}.lock`;
const LOCK_STALE_MS = 10_000;
const LOCK_BACKOFF_INITIAL_MS = 25;
const LOCK_BACKOFF_CAP_MS = 500;
/**
 * A steal retries while the stale file cannot be removed (a root-owned or
 * immutable file: EACCES/EPERM). Past this deadline the write fails instead of
 * hot-looping — the same 2× stale window the core and desktop writers use.
 */
const LOCK_STEAL_DEADLINE_MS = 2 * LOCK_STALE_MS;

const sleepFor = (ms: number): Promise<void> => {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
};

/** A held lock: its path plus the ownership token written inside it. */
type HeldConfigLock = { lockPath: string; token: string };

function isTomlTable(value: unknown): value is TomlTable {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Take the config lock, or fail instead of waiting forever behind a live holder. */
async function acquireConfigLock(directory: string): Promise<HeldConfigLock> {
  const lockPath = join(directory, LOCK_FILE);
  await mkdir(directory, { recursive: true });
  const token = randomUUID();
  let backoff = LOCK_BACKOFF_INITIAL_MS;
  let stealDeadlineMs: number | undefined;
  let unlinkErrorLogged = false;
  for (;;) {
    try {
      const lock = await open(lockPath, 'wx', 0o600);
      try {
        // Diagnostics only (the age of a held lock comes from its mtime); the
        // trailing token is the ownership proof release compares.
        await lock.writeFile(`${process.pid} ${new Date().toISOString()} ${token}\n`);
      } catch (error) {
        // The create succeeded, so this attempt owns the file: release it
        // before failing, or the next writer stalls until the stale steal.
        await lock.close().catch(() => undefined);
        await rm(lockPath, { force: true }).catch(() => undefined);
        throw error;
      }
      // A close failure cannot strand the lock — the caller releases it.
      await lock.close().catch(() => undefined);
      return { lockPath, token };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') {
        throw new HttpError(500, 'internal', `${CONFIG_FILE} lock is unavailable`);
      }
      const ageMs = await stat(lockPath).then(
        (metadata) => Date.now() - metadata.mtimeMs,
        () => undefined, // Released between the create and the stat: retry now.
      );
      if (ageMs !== undefined && ageMs > LOCK_STALE_MS) {
        // Stale age is a heuristic: two reclaimers can both unlink and create,
        // leaving two live holders, and a live slow holder can lose its lock.
        // Accepted only because all four writers are user-paced.
        try {
          await rm(lockPath, { force: true });
        } catch (unlinkError) {
          stealDeadlineMs ??= Date.now();
          if (!unlinkErrorLogged) {
            unlinkErrorLogged = true;
            console.error(`[nexus-service] cannot remove the stale ${LOCK_FILE}:`, unlinkError);
          }
          if (Date.now() - stealDeadlineMs > LOCK_STEAL_DEADLINE_MS) {
            throw new HttpError(500, 'internal', `${CONFIG_FILE} lock is stale and cannot be removed`);
          }
        }
        // Back off here too: an unremovable stale lock must not hot-loop.
        await sleepFor(backoff);
        backoff = Math.min(backoff * 2, LOCK_BACKOFF_CAP_MS);
      } else if (ageMs !== undefined) {
        await sleepFor(backoff);
        backoff = Math.min(backoff * 2, LOCK_BACKOFF_CAP_MS);
      }
    }
  }
}

/**
 * Release the lock, but only while this holder still owns it: a holder stalled
 * past the stale window is stolen, and unlinking on resume would delete the new
 * owner's lock and admit a third writer into the second's read-modify-write.
 */
async function releaseConfigLock({ lockPath, token }: HeldConfigLock): Promise<void> {
  try {
    const contents = await readFile(lockPath, 'utf8');
    if (!contents.trimEnd().endsWith(token)) {
      console.error(`[nexus-service] ${LOCK_FILE} was stolen; leaving the new owner's lock`);
      return;
    }
    await rm(lockPath, { force: true });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return;
    console.error(`[nexus-service] ${LOCK_FILE} release failed:`, error);
  }
}

/**
 * Read the whole config document. A missing file is an empty document; an
 * existing unreadable/corrupt file is an error (never treated as absent — a
 * silent empty doc would drop the other stored config on the next write).
 */
async function readConfigDocument(home: string): Promise<TomlTable> {
  let text: string;
  try {
    text = await readFile(join(home, NEXUS_DIR, CONFIG_FILE), 'utf8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return {};
    throw new HttpError(500, 'internal', `${CONFIG_FILE} is unreadable`);
  }
  if (text.trim().length === 0) return {};
  try {
    return parse(text);
  } catch {
    throw new HttpError(500, 'internal', `${CONFIG_FILE} is not valid TOML`);
  }
}

/**
 * Replace the config document atomically: same-directory temp file, fsync of
 * the content and of the directory entry, then rename into place.
 */
async function writeConfigDocument(home: string, document: TomlTable): Promise<void> {
  const configPath = join(home, NEXUS_DIR, CONFIG_FILE);
  const directory = dirname(configPath);
  const tempPath = join(directory, `.${CONFIG_FILE}.${randomUUID()}.tmp`);
  try {
    await mkdir(directory, { recursive: true });
    const temp = await open(tempPath, 'w', 0o600);
    try {
      await temp.writeFile(stringify(document));
      await temp.sync();
    } finally {
      await temp.close();
    }
    await rename(tempPath, configPath);
    // The rename itself is only durable once the directory entry is synced.
    // POSIX only: Windows cannot open a directory as a file (EPERM/EISDIR), and
    // the failure would land AFTER the rename — reporting a successful save as
    // a 500 and inviting a pointless retry.
    if (process.platform !== 'win32') {
      const dir = await open(directory, 'r');
      try {
        await dir.sync();
      } finally {
        await dir.close();
      }
    }
  } catch (error) {
    // The wire answer stays generic; the operator log keeps the cause. A temp
    // file that was already created must not survive the failed write.
    console.error(`[nexus-service] ${CONFIG_FILE} write failed:`, error);
    await rm(tempPath, { force: true }).catch(() => undefined);
    throw new HttpError(500, 'internal', `${CONFIG_FILE} could not be written`);
  }
}

/**
 * The stored profile, or null when unset/malformed.
 *
 * Values are normalized on READ with the same rule as the write fold, so a
 * hand-edited/legacy document is presented exactly as the next save would
 * store it: trimmed name, trimmed command, an all-blank command is no command.
 */
export async function readAgentProfile(home: string): Promise<AgentProfile | null> {
  const table = (await readConfigDocument(home))[PROFILE_TABLE];
  if (!isTomlTable(table)) return null;
  const storedName = table['name'];
  const name = typeof storedName === 'string' ? storedName.trim() : '';
  if (name.length === 0) return null;
  const storedCommand = table['launch_command'];
  const command = typeof storedCommand === 'string' ? storedCommand.trim() : '';
  return command.length > 0 ? { name, launch_command: command } : { name };
}

/**
 * Persist the one daemon-global profile and answer with what was stored.
 *
 * Omitting `launch_command` preserves the stored custom command ONLY for the
 * same profile name (a name-only save never clears its own command; switching
 * to another agent stores no command). An explicit blank string clears it.
 * A blank name, or a name/command over the documented caps, is `invalid_input`
 * (existing fold → 400) and never touches the stored profile.
 */
export async function writeAgentProfile(
  home: string,
  request: AgentProfile,
): Promise<AgentProfile> {
  const name = request.name.trim();
  if (name.length === 0) {
    throw new HttpError(400, 'invalid_input', 'name must not be blank');
  }
  if ([...name].length > MAX_NAME_LENGTH) {
    throw new HttpError(
      400,
      'invalid_input',
      `name must be at most ${MAX_NAME_LENGTH} characters`,
    );
  }
  const requestedCommand = request.launch_command?.trim();
  if (requestedCommand !== undefined && [...requestedCommand].length > MAX_LAUNCH_COMMAND_LENGTH) {
    throw new HttpError(
      400,
      'invalid_input',
      `launch_command must be at most ${MAX_LAUNCH_COMMAND_LENGTH} characters`,
    );
  }

  // The lock spans the whole read-modify-write: a concurrent core/desktop
  // writer's document must be read AFTER it is committed, never before.
  const heldLock = await acquireConfigLock(join(home, NEXUS_DIR));
  try {
    const document = await readConfigDocument(home);
    const existing = document[PROFILE_TABLE];
    // Preserve-on-omit is NAME-SCOPED: only the same profile keeps its custom
    // command. Switching to a different agent stores the new name with no
    // launch_command (the previous agent's override must not leak across).
    const storedName =
      isTomlTable(existing) && typeof existing['name'] === 'string'
        ? existing['name'].trim()
        : undefined;
    // A preserved command is normalized on the way through too: preserving an
    // untrimmed legacy value would re-persist the very shape the write fold
    // forbids, and an all-blank stored command means "no command".
    const storedCommand =
      isTomlTable(existing) &&
      storedName === name &&
      typeof existing['launch_command'] === 'string'
        ? existing['launch_command'].trim()
        : undefined;

    const command =
      request.launch_command === undefined
        ? storedCommand !== undefined && storedCommand.length > 0
          ? storedCommand
          : undefined
        : requestedCommand !== undefined && requestedCommand.length > 0
          ? requestedCommand
          : undefined;

    document[PROFILE_TABLE] =
      command !== undefined ? { name, launch_command: command } : { name };
    await writeConfigDocument(home, document);
    return command !== undefined ? { name, launch_command: command } : { name };
  } finally {
    await releaseConfigLock(heldLock);
  }
}

/** Runtime shape check for the PUT body — the schema is not a validator. */
function parseProfileRequest(body: unknown): AgentProfile {
  const request = wirePayload<Partial<AgentProfile>>(body, 'profile');
  if (typeof request.name !== 'string') {
    throw new HttpError(400, 'invalid_input', 'name must be a string');
  }
  if (
    request.launch_command !== undefined &&
    typeof request.launch_command !== 'string'
  ) {
    throw new HttpError(400, 'invalid_input', 'launch_command must be a string');
  }
  return request as AgentProfile;
}

/** `GET /v1/daemon/agent-host/profile` — always 200; `null` = unset. */
export async function getAgentProfile(service: ServiceCore): Promise<AgentProfileResponse> {
  return { profile: await readAgentProfile(service.home) };
}

/** `PUT /v1/daemon/agent-host/profile`. */
export async function putAgentProfile(
  service: ServiceCore,
  body: unknown,
): Promise<AgentProfileResponse> {
  const profile = await writeAgentProfile(service.home, parseProfileRequest(body));
  return { profile };
}

/** Exact path/verb/tier identities this family adds to the agent-host family. */
export const AGENT_PROFILE_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/profile$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service) => ({ body: await getAgentProfile(service) }),
  },
  {
    method: 'PUT',
    pattern: /^\/v1\/daemon\/agent-host\/profile$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service, _params, _search, body) => ({
      body: await putAgentProfile(service, body),
    }),
  },
];
