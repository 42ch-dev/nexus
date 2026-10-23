/**
 * Agent-profile family (I-000064): the daemon-global agent profile persisted in
 * the daemon home `config.toml` under the `agent_profile` table.
 *
 * Storage semantics mirror the core CLI config (`read_cli_config` /
 * `write_cli_config` in `crates/nexus-core/src/creators.rs`): a missing file is
 * an empty document, every other key is preserved on write, and the document is
 * replaced atomically (temp file + rename in the same directory) so a reader
 * never observes a partially written config.
 *
 * ONE daemon-global profile — matching desktop semantics. The legacy
 * per-creator localStorage key was a browser artifact and is migrated by the
 * web read path, never re-created here.
 */
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { rename, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parse, stringify, type TomlTable } from 'smol-toml';
import type { AgentProfile, AgentProfileResponse } from '@42ch/nexus-contracts';

import { HttpError } from './errors.js';
import type { ServiceCore } from './lifecycle.js';
import type { DomainRoute } from './routes.js';
import { wirePayload } from './world-kb.js';

const CONFIG_FILE = 'config.toml';
const PROFILE_TABLE = 'agent_profile';
/** Bound on a stored agent name; longer values are rejected at the fold. */
const MAX_NAME_LENGTH = 200;
/** Bound on a stored launch command; longer values are rejected at the fold. */
const MAX_LAUNCH_COMMAND_LENGTH = 4096;

function isTomlTable(value: unknown): value is TomlTable {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Read the whole config document. A missing file is an empty document; an
 * existing unreadable/corrupt file is an error (never treated as absent — a
 * silent empty doc would drop the other stored config on the next write).
 */
function readConfigDocument(home: string): TomlTable {
  const configPath = join(home, CONFIG_FILE);
  let text: string;
  try {
    text = readFileSync(configPath, 'utf8');
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

/** Replace the config document atomically (same-directory temp file + rename). */
async function writeConfigDocument(home: string, document: TomlTable): Promise<void> {
  const configPath = join(home, CONFIG_FILE);
  const tempPath = join(home, `.${CONFIG_FILE}.${randomUUID()}.tmp`);
  try {
    await writeFile(tempPath, stringify(document), { mode: 0o600 });
    await rename(tempPath, configPath);
  } catch (error) {
    // The wire answer stays generic; the operator log keeps the cause. A temp
    // file that was already created must not survive the failed write.
    console.error(`[nexus-service] ${CONFIG_FILE} write failed:`, error);
    await rm(tempPath, { force: true }).catch(() => undefined);
    throw new HttpError(500, 'internal', `${CONFIG_FILE} could not be written`);
  }
}

/** The stored profile, or null when unset/malformed. */
export function readAgentProfile(home: string): AgentProfile | null {
  const table = readConfigDocument(home)[PROFILE_TABLE];
  if (!isTomlTable(table)) return null;
  const name = table['name'];
  if (typeof name !== 'string' || name.trim().length === 0) return null;
  const command = table['launch_command'];
  return typeof command === 'string' && command.length > 0
    ? { name, launch_command: command }
    : { name };
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
  if (request.name.length > MAX_NAME_LENGTH) {
    throw new HttpError(
      400,
      'invalid_input',
      `name must be at most ${MAX_NAME_LENGTH} characters`,
    );
  }
  if (
    request.launch_command !== undefined &&
    request.launch_command.length > MAX_LAUNCH_COMMAND_LENGTH
  ) {
    throw new HttpError(
      400,
      'invalid_input',
      `launch_command must be at most ${MAX_LAUNCH_COMMAND_LENGTH} characters`,
    );
  }

  const document = readConfigDocument(home);
  const existing = document[PROFILE_TABLE];
  // Preserve-on-omit is NAME-SCOPED: only the same profile keeps its custom
  // command. Switching to a different agent stores the new name with no
  // launch_command (the previous agent's override must not leak across).
  const storedName =
    isTomlTable(existing) && typeof existing['name'] === 'string'
      ? existing['name']
      : undefined;
  const storedCommand =
    isTomlTable(existing) &&
    storedName === name &&
    typeof existing['launch_command'] === 'string'
      ? existing['launch_command']
      : undefined;

  const command =
    request.launch_command === undefined
      ? storedCommand
      : request.launch_command.trim().length > 0
        ? request.launch_command.trim()
        : undefined;

  document[PROFILE_TABLE] =
    command !== undefined ? { name, launch_command: command } : { name };
  await writeConfigDocument(home, document);
  return command !== undefined ? { name, launch_command: command } : { name };
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
export function getAgentProfile(service: ServiceCore): AgentProfileResponse {
  return { profile: readAgentProfile(service.home) };
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
    handle: async (service) => ({ body: getAgentProfile(service) }),
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
