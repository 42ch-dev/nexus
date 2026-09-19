/**
 * Desktop config, workspace resolution and setup bootstrap (v1.192 P0-T2).
 *
 * Main-process owner of `~/.nexus42/config.toml` (workspace root precedence,
 * entrance, `setup_completed`, creator switch, setup bootstrap) and of the
 * `~/.nexus42/agent-host/config.toml` agent-profile entry. `home` and
 * `documentsPath` are trusted inputs resolved by main from the launch
 * environment — the renderer never supplies either, and nothing here returns
 * authority to the renderer (`get_workspace_root` is a display string only;
 * the guarded OS actions re-resolve the root on every call, P0-T3).
 *
 * Parity source (read-only reference): the retired Tauri host,
 * `apps/desktop/src-tauri/src/lib.rs` — `resolve_workspace_root*`,
 * `get_entrance`/`set_entrance`, `read_setup_completed*`/`write_setup_completed*`,
 * `switch_active_creator_at`, `ensure_setup_bootstrap_at`,
 * `read_agent_profile_at`/`write_agent_profile_at`. That tree is P2's deletion
 * target; this module is the Electron replacement.
 *
 * TOML discipline (plan §"Config, OS actions and secure storage"):
 *  - one maintained parser (`smol-toml`, lockfile-pinned) — no regex parsing;
 *  - every mutation is serialized per file and re-reads the document inside the
 *    gate, so concurrent writers cannot drop each other's keys;
 *  - replacement is atomic (sibling temp file + rename);
 *  - unrelated keys are preserved by a parse → mutate → stringify round-trip;
 *  - an existing document that cannot be parsed is an error — it is never
 *    replaced with an empty document.
 */

import { randomUUID } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { TomlDate, parse, stringify, type TomlTable, type TomlValue } from 'smol-toml';
import {
  desktopError,
  type EntranceId,
  type SetupBootstrapResult,
} from './desktop-contract.js';
import type { DesktopHandlers } from './desktop-ipc.js';

const NEXUS_DIR = '.nexus42';
const CONFIG_FILE = 'config.toml';
const AGENT_HOST_DIR = 'agent-host';

/** Default workspace segments appended to the trusted documents directory. */
const DEFAULT_WORKSPACE_SEGMENTS = ['nexus', 'default'] as const;

/** Entrance used when the key is absent or the document is unreadable (AR-16). */
const DEFAULT_ENTRANCE: EntranceId = 'content-creator';
const ENTRANCE_VALUES: readonly string[] = ['developer', 'content-creator'];

/** Slug written for a creator whose workspace was never explicitly chosen. */
const DEFAULT_WORKSPACE_SLUG = 'default';

/** Operations P0-T2 owns in the frozen host-contract table. */
export type DesktopConfigOperation =
  | 'get_workspace_root'
  | 'set_workspace_path'
  | 'switch_active_creator'
  | 'ensure_setup_bootstrap'
  | 'get_entrance'
  | 'set_entrance'
  | 'get_setup_completed'
  | 'set_setup_completed'
  | 'get_agent_profile'
  | 'set_agent_profile';

export type DesktopConfigHandlers = Pick<DesktopHandlers, DesktopConfigOperation>;

/**
 * The value `createDesktopConfig` returns: the config handler map T7 composes
 * into the full `DesktopHandlers`, plus the canonical active-workspace resolver
 * the guarded OS actions (P0-T3) re-resolve on every call.
 */
export type DesktopConfig = DesktopConfigHandlers & {
  resolveWorkspaceRoot(): Promise<string>;
};

// ---------------------------------------------------------------------------
// TOML document helpers
// ---------------------------------------------------------------------------

/** A parsed TOML table (smol-toml plain object) is never an array or a date. */
function isTomlTable(value: TomlValue | undefined): value is TomlTable {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    !(value instanceof TomlDate)
  );
}

/**
 * `ctr_local` + 12 hex characters — the generation pattern of
 * `nexus-creator/src/local_identity.rs` (first 12 hex chars of a UUID v4).
 */
function generateLocalCreatorId(): string {
  return `ctr_local${randomUUID().replaceAll('-', '').slice(0, 12)}`;
}

/**
 * Creator ids become path components outside this module
 * (`~/.nexus42/creators/<id>/…`), so separators and `..` are rejected here as
 * they were in the retired host's `validate_creator_id_safe`. Control
 * characters and the byte bound are already enforced by the frozen request
 * parser before a handler runs.
 */
function assertSafeCreatorId(creatorId: string): void {
  if (creatorId.includes('/') || creatorId.includes('\\') || creatorId.includes('..')) {
    throw desktopError('invalid_input', `invalid creator_id: ${creatorId}`);
  }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/**
 * Build the config handlers bound to one trusted home/documents pair.
 *
 * Read defaults follow the retired host: `setup_completed` absent → `false`,
 * `entrance` absent or unreadable → `content-creator`, agent profile absent →
 * `null`, workspace root → per-creator map → legacy mirror → `<documents>/nexus/default`.
 * Writes never replace an unparseable document: they fail with `config_corrupt`.
 */
export function createDesktopConfig(home: string, documentsPath: string): DesktopConfig {
  if (typeof home !== 'string' || home.length === 0) {
    throw desktopError('invalid_input', 'a trusted home directory is required');
  }
  if (typeof documentsPath !== 'string' || documentsPath.length === 0) {
    throw desktopError('invalid_input', 'a trusted documents path is required');
  }

  const configPath = join(home, NEXUS_DIR, CONFIG_FILE);
  const agentProfilePath = join(home, NEXUS_DIR, AGENT_HOST_DIR, CONFIG_FILE);
  const defaultWorkspaceRoot = join(documentsPath, ...DEFAULT_WORKSPACE_SEGMENTS);

  /**
   * Per-file mutation gates. Each holds the tail of its file's mutation chain,
   * so mutations of one file run strictly one at a time and a later mutation
   * always re-reads what an earlier one wrote.
   */
  const configGate: { tail: Promise<unknown> } = { tail: Promise.resolve() };
  const agentProfileGate: { tail: Promise<unknown> } = { tail: Promise.resolve() };

  function serialized<T>(gate: { tail: Promise<unknown> }, run: () => Promise<T>): Promise<T> {
    const next = gate.tail.catch(() => undefined).then(run);
    // The stored tail must never reject, or one failed mutation would poison
    // every later one.
    gate.tail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  /**
   * Read one TOML document. A missing file is an empty document; an existing
   * file that cannot be read or parsed throws instead of degrading to `{}`, so
   * a corrupt document is never treated as absent.
   */
  async function readDocument(path: string): Promise<TomlTable> {
    let text: string;
    try {
      text = await readFile(path, 'utf8');
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') return {};
      throw desktopError(
        'config_io',
        `cannot read ${path}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
    try {
      return parse(text, { integersAsBigInt: 'asNeeded' });
    } catch (err) {
      throw desktopError(
        'config_corrupt',
        `${path} is not valid TOML: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  /** Tolerant read for the fields whose contract defines a default (retired host semantics). */
  async function readDocumentOrNull(path: string): Promise<TomlTable | null> {
    try {
      return await readDocument(path);
    } catch {
      return null;
    }
  }

  /** Atomic replace: write a sibling temp file, then rename it into place. */
  async function writeDocument(path: string, doc: TomlTable): Promise<void> {
    const tmp = `${path}.${randomUUID()}.tmp`;
    try {
      await mkdir(dirname(path), { recursive: true });
      await writeFile(tmp, stringify(doc), 'utf8');
      await rename(tmp, path);
    } catch (err) {
      await rm(tmp, { force: true }).catch(() => undefined);
      throw desktopError(
        'config_io',
        `cannot write ${path}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  /** Serialized read-modify-write: re-read inside the gate, then replace atomically. */
  function mutate<T>(
    gate: { tail: Promise<unknown> },
    path: string,
    apply: (doc: TomlTable) => T,
  ): Promise<T> {
    return serialized(gate, async () => {
      const doc = await readDocument(path);
      const value = apply(doc);
      await writeDocument(path, doc);
      return value;
    });
  }

  /**
   * Strict string read for keys this module owns: absent (or empty) means unset,
   * a present non-string value means the document no longer means what it
   * claims — the retired host failed deserialization in exactly that case.
   */
  function ownedString(doc: TomlTable, key: string, label: string): string | undefined {
    const value = doc[key];
    if (value === undefined) return undefined;
    if (typeof value !== 'string') {
      throw desktopError('config_corrupt', `${label}: ${key} is not a string`);
    }
    return value.length > 0 ? value : undefined;
  }

  /**
   * Table this module owns, for writes: created when absent, an error when the
   * existing value is not a table (never silently replaced).
   */
  function ownedTableForWrite(doc: TomlTable, key: string, label: string): TomlTable {
    const value = doc[key];
    if (value === undefined) {
      const created: TomlTable = {};
      doc[key] = created;
      return created;
    }
    if (!isTomlTable(value)) {
      throw desktopError('config_corrupt', `${label}: ${key} is not a table`);
    }
    return value;
  }

  async function resolveWorkspaceRoot(): Promise<string> {
    const doc = await readDocument(configPath);
    const creatorId = ownedString(doc, 'active_creator_id', configPath);
    const byCreatorValue = doc['workspace_path_by_creator'];
    let byCreator: TomlTable | undefined;
    if (byCreatorValue !== undefined) {
      if (!isTomlTable(byCreatorValue)) {
        throw desktopError('config_corrupt', `${configPath}: workspace_path_by_creator is not a table`);
      }
      byCreator = byCreatorValue;
    }
    const perCreator =
      creatorId !== undefined && byCreator !== undefined
        ? ownedString(byCreator, creatorId, `${configPath}: [workspace_path_by_creator]`)
        : undefined;
    // Precedence: per-creator map → legacy mirror → default slug.
    return (
      perCreator ??
      ownedString(doc, 'workspace_path', configPath) ??
      defaultWorkspaceRoot
    );
  }

  const handlers: DesktopConfigHandlers = {
    async get_workspace_root() {
      return resolveWorkspaceRoot();
    },

    async set_workspace_path({ path }) {
      await mutate(configGate, configPath, (doc) => {
        const creatorId = ownedString(doc, 'active_creator_id', configPath);
        if (creatorId === undefined) {
          throw desktopError('no_active_creator', 'no active creator_id; run setup bootstrap first');
        }
        ownedTableForWrite(doc, 'workspace_path_by_creator', configPath)[creatorId] = path;
        doc['workspace_path'] = path;
      });
      return null;
    },

    async switch_active_creator({ creatorId }) {
      assertSafeCreatorId(creatorId);
      return mutate(configGate, configPath, (doc) => {
        const byCreator = ownedTableForWrite(doc, 'workspace_path_by_creator', configPath);
        const targetPath =
          ownedString(byCreator, creatorId, `${configPath}: [workspace_path_by_creator]`) ??
          defaultWorkspaceRoot;
        byCreator[creatorId] = targetPath;
        doc['active_creator_id'] = creatorId;
        doc['workspace_path'] = targetPath;
        ownedTableForWrite(doc, 'active_workspace_slug_by_creator', configPath)[creatorId] =
          DEFAULT_WORKSPACE_SLUG;
        return targetPath;
      });
    },

    async ensure_setup_bootstrap() {
      return serialized<SetupBootstrapResult>(configGate, async () => {
        const doc = await readDocument(configPath);
        const existing = ownedString(doc, 'active_creator_id', configPath);
        if (existing !== undefined) {
          // Never replaces an existing creator, and never rewrites the file.
          return { creator_id: existing, already_bootstrapped: true };
        }
        const creatorId = generateLocalCreatorId();
        doc['active_creator_id'] = creatorId;
        ownedTableForWrite(doc, 'active_workspace_slug_by_creator', configPath)[creatorId] =
          DEFAULT_WORKSPACE_SLUG;
        await writeDocument(configPath, doc);
        return { creator_id: creatorId, already_bootstrapped: false };
      });
    },

    async get_entrance() {
      const doc = await readDocumentOrNull(configPath);
      const stored = doc?.['entrance'];
      if (typeof stored !== 'string') return DEFAULT_ENTRANCE;
      if (!ENTRANCE_VALUES.includes(stored)) {
        throw desktopError('invalid_input', `invalid stored entrance value: ${stored}`);
      }
      return stored as EntranceId;
    },

    async set_entrance({ value }) {
      await mutate(configGate, configPath, (doc) => {
        doc['entrance'] = value;
      });
      return null;
    },

    async get_setup_completed() {
      const doc = await readDocumentOrNull(configPath);
      const stored = doc?.['setup_completed'];
      return typeof stored === 'boolean' ? stored : false;
    },

    async set_setup_completed({ value }) {
      await mutate(configGate, configPath, (doc) => {
        doc['setup_completed'] = value;
      });
      return null;
    },

    async get_agent_profile() {
      const doc = await readDocumentOrNull(agentProfilePath);
      const providers = doc?.['providers'];
      if (!Array.isArray(providers)) return null;
      for (const row of providers) {
        if (!isTomlTable(row) || row['protocol'] !== 'native_cli') continue;
        const name = row['id'];
        // Skip malformed rows so a later valid native_cli can still preselect.
        if (typeof name !== 'string' || name.length === 0) continue;
        const command = row['command'];
        return typeof command === 'string'
          ? { name, launchCommand: command }
          : { name };
      }
      return null;
    },

    async set_agent_profile(profile) {
      await mutate(agentProfileGate, agentProfilePath, (doc) => {
        const existing = doc['providers'];
        if (existing !== undefined && !Array.isArray(existing)) {
          throw desktopError('config_corrupt', `${agentProfilePath}: providers is not an array of tables`);
        }
        const providers: TomlValue[] = existing ?? [];
        // Upsert: the stored profile is the sole native_cli entry, so the agent
        // host and the Settings preselect (first native_cli) always agree.
        for (let index = providers.length - 1; index >= 0; index -= 1) {
          const row = providers[index];
          if (isTomlTable(row) && row['protocol'] === 'native_cli') providers.splice(index, 1);
        }
        const entry: TomlTable = { id: profile.name, protocol: 'native_cli' };
        if (profile.launchCommand !== undefined) entry['command'] = profile.launchCommand;
        providers.push(entry);
        doc['providers'] = providers;
      });
      return null;
    },
  };

  return { ...handlers, resolveWorkspaceRoot };
}
