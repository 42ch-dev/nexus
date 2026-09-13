import { execFile, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { PassThrough, Readable, Writable } from 'node:stream';
import type { ValidatedProviderRecipe } from '@42ch/nexus-contracts';
import {
  ClientSideConnection,
  ndJsonStream,
  type Client,
  type Stream,
} from '@agentclientprotocol/sdk';
import { CleanupUnconfirmedError } from './errors.js';
import {
  bindProcessIdentity,
  identityStillMatches,
  observeProcessIdentity,
  parseProcessIdentity,
  type ProcessIdentity,
} from './identity.js';

function execFileAsync(command: string, args: readonly string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(command, args, (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}

export const MAX_FRAME_BYTES = 1024 * 1024;
export const MAX_STDERR_BYTES = 256 * 1024;
export const MAX_PROMPT_INPUT_BYTES = 1024 * 1024;

export type SessionUpdateHandler = (params: { sessionId: string; update: unknown }) => void;

export type OwnedConnection = {
  recipeGeneration: string;
  child: ChildProcessWithoutNullStreams;
  connection: ClientSideConnection;
  acpSessionId: string | null;
  stdinWritable: Writable;
  admittedIdentity: ProcessIdentity | null;
  boundIdentity: ProcessIdentity;
};

export type ReapResult = {
  confirmed: boolean;
  exitCode: number | null;
  signal: string | null;
};

type ExitRace = {
  exited: boolean;
  promise: Promise<never>;
};

function reapTimeoutMs(): number {
  const raw = process.env.NEXUS_ACP_TEST_REAP_MS;
  if (!raw) return 5_000;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 5_000;
}

function buildChildEnv(env: Record<string, string>): Record<string, string> {
  return { ...env };
}

function attachExitRace(child: ChildProcessWithoutNullStreams): ExitRace {
  let exited = false;
  const promise = new Promise<never>((_, reject) => {
    child.once('exit', () => {
      exited = true;
      reject(new Error('provider_eof'));
    });
  });
  return {
    get exited() {
      return exited || child.exitCode !== null || child.signalCode !== null;
    },
    promise,
  };
}

function attachStderrGuard(
  child: ChildProcessWithoutNullStreams,
  onOverflow: () => void,
): { overflowed: () => boolean } {
  let bytes = 0;
  let overflow = false;
  child.stderr?.on('data', (chunk: Uint8Array) => {
    bytes += chunk.length;
    if (bytes > MAX_STDERR_BYTES) {
      overflow = true;
      onOverflow();
      try {
        child.kill('SIGTERM');
      } catch {
        // already dead
      }
    }
  });
  return { overflowed: () => overflow };
}

function createFrameGuardTransform(maxFrameBytes: number): TransformStream<Uint8Array, Uint8Array> {
  let carry = new Uint8Array(0);
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  return new TransformStream<Uint8Array, Uint8Array>({
    transform(chunk, controller) {
      const merged = new Uint8Array(carry.length + chunk.length);
      merged.set(carry, 0);
      merged.set(chunk, carry.length);
      carry = merged;
      let start = 0;
      while (true) {
        let newline = -1;
        for (let i = start; i < carry.length; i += 1) {
          if (carry[i] === 0x0a) {
            newline = i;
            break;
          }
        }
        if (newline < 0) {
          if (carry.length - start > maxFrameBytes) throw new Error('frame_too_large');
          carry = carry.slice(start);
          return;
        }
        const line = carry.slice(start, newline);
        if (line.length > maxFrameBytes) throw new Error('frame_too_large');
        controller.enqueue(encoder.encode(`${decoder.decode(line)}\n`));
        start = newline + 1;
      }
    },
    flush(controller) {
      if (carry.length > 0) {
        if (carry.length > maxFrameBytes) throw new Error('frame_too_large');
        controller.enqueue(encoder.encode(`${decoder.decode(carry)}\n`));
        carry = new Uint8Array(0);
      }
    },
  });
}

function nodeToWebReadable(nodeStream: Readable): ReadableStream<Uint8Array> {
  return Readable.toWeb(nodeStream) as ReadableStream<Uint8Array>;
}

function nodeToWebWritable(nodeStream: Writable): WritableStream<Uint8Array> {
  return Writable.toWeb(nodeStream) as WritableStream<Uint8Array>;
}

function makeOwnedStub(
  recipe: ValidatedProviderRecipe,
  child: ChildProcessWithoutNullStreams,
  stdinWritable: Writable,
  boundIdentity: ProcessIdentity,
  admittedIdentity: ProcessIdentity | null,
): OwnedConnection {
  return {
    recipeGeneration: recipe.recipe_generation,
    child,
    connection: null as unknown as ClientSideConnection,
    acpSessionId: null,
    stdinWritable,
    admittedIdentity,
    boundIdentity,
  };
}

async function signalBoundProcessTree(
  owned: OwnedConnection,
  signal: NodeJS.Signals,
): Promise<void> {
  if (!identityStillMatches(owned.child, owned.boundIdentity)) {
    throw new Error('process_identity_mismatch');
  }
  const pid = owned.boundIdentity.pid;
  if (process.platform === 'win32') {
    throw new Error('process_identity_unsupported');
  }
  const pkillSignal = signal === 'SIGKILL' ? 'KILL' : 'TERM';
  await execFileAsync('pkill', [`-${pkillSignal}`, '-P', String(pid)]).catch(() => undefined);
  try {
    process.kill(pid, signal);
  } catch {
    // already dead
  }
}

export async function spawnOwnedConnection(
  recipe: ValidatedProviderRecipe,
  onSessionUpdate: SessionUpdateHandler,
): Promise<OwnedConnection> {
  const admittedIdentity = parseProcessIdentity(recipe.process_identity ?? null);
  const child = spawn(recipe.executable, recipe.args, {
    cwd: recipe.cwd,
    env: buildChildEnv(recipe.env as Record<string, string>),
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  child.on('error', () => {
    // ENOENT and similar spawn failures surface via initialize/cleanup paths.
  });

  const stdinWritable = child.stdin ?? new PassThrough();
  let ownedStub: OwnedConnection | null = null;

  if (!child.stdin || !child.stdout || !child.stderr) {
    let boundIdentity: ProcessIdentity;
    try {
      boundIdentity = observeProcessIdentity(child);
    } catch {
      if (child.pid === undefined || child.pid <= 0) throw new Error('provider_spawn_failed');
      boundIdentity = { pid: child.pid, process_birth: null, group_id: null };
    }
    ownedStub = makeOwnedStub(recipe, child, stdinWritable, boundIdentity, admittedIdentity);
    const reap = await reapChild(child, boundIdentity, reapTimeoutMs());
    if (!reap.confirmed) {
      throw new CleanupUnconfirmedError('spawn_stdio_cleanup_unconfirmed', ownedStub);
    }
    throw new Error('provider_spawn_failed');
  }

  let boundIdentity: ProcessIdentity;
  try {
    const observed = observeProcessIdentity(child);
    boundIdentity = bindProcessIdentity(admittedIdentity, observed);
  } catch (error) {
    let observed: ProcessIdentity;
    try {
      observed = observeProcessIdentity(child);
    } catch {
      if (child.pid === undefined || child.pid <= 0) throw error;
      observed = { pid: child.pid, process_birth: null, group_id: null };
    }
    ownedStub = makeOwnedStub(recipe, child, child.stdin, observed, admittedIdentity);
    const reap = await reapChild(child, ownedStub.boundIdentity, reapTimeoutMs());
    if (!reap.confirmed) {
      throw new CleanupUnconfirmedError('spawn_identity_cleanup_unconfirmed', ownedStub);
    }
    throw error;
  }

  ownedStub = makeOwnedStub(recipe, child, child.stdin, boundIdentity, admittedIdentity);

  const exitRace = attachExitRace(child);
  const stderrGuard = attachStderrGuard(child, () => undefined);

  const guardedStdout = nodeToWebReadable(child.stdout).pipeThrough(
    createFrameGuardTransform(MAX_FRAME_BYTES),
  );
  const stream: Stream = ndJsonStream(nodeToWebWritable(child.stdin), guardedStdout);

  const client: Client = {
    sessionUpdate: async (params) => {
      onSessionUpdate({ sessionId: String(params.sessionId), update: params.update });
    },
    requestPermission: async () => ({ outcome: { outcome: 'cancelled' } }),
  };

  const connection = new ClientSideConnection(() => client, stream);
  ownedStub.connection = connection;

  try {
    await Promise.race([
      connection.initialize({
        protocolVersion: 1,
        clientInfo: { name: 'nexus-provider-acp', version: '0.1.0' },
      }),
      exitRace.promise,
    ]);
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    if (
      exitRace.exited ||
      stderrGuard.overflowed() ||
      child.exitCode !== null ||
      child.signalCode !== null
    ) {
      throw new Error(stderrGuard.overflowed() ? 'stderr_overflow' : 'provider_eof');
    }
  } catch (error) {
    const reap = await cleanupOwnedConnection(ownedStub);
    if (!reap.confirmed) {
      throw new CleanupUnconfirmedError('init_cleanup_unconfirmed', ownedStub);
    }
    throw error;
  }

  return ownedStub;
}

export async function createAcpSession(owned: OwnedConnection, cwd: string): Promise<string> {
  const exitRace = attachExitRace(owned.child);
  try {
    const response = await Promise.race([
      owned.connection.newSession({ cwd, mcpServers: [] }),
      exitRace.promise,
    ]);
    const sessionId = String(response.sessionId);
    owned.acpSessionId = sessionId;
    return sessionId;
  } catch (error) {
    const reap = await cleanupOwnedConnection(owned);
    if (!reap.confirmed) {
      throw new CleanupUnconfirmedError('new_session_cleanup_unconfirmed', owned);

    }
    throw error;
  }
}

export async function waitForChildExit(
  child: ChildProcessWithoutNullStreams,
  timeoutMs: number,
): Promise<boolean> {
  if (child.exitCode !== null || child.signalCode !== null) return true;
  return await new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => resolve(false), timeoutMs);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

export async function reapChild(
  child: ChildProcessWithoutNullStreams,
  boundIdentity: ProcessIdentity,
  timeoutMs = reapTimeoutMs(),
): Promise<ReapResult> {
  if (child.pid !== boundIdentity.pid) {
    return { confirmed: false, exitCode: null, signal: 'identity_mismatch' };
  }
  if (child.exitCode !== null || child.signalCode !== null) {
    return { confirmed: true, exitCode: child.exitCode, signal: child.signalCode };
  }
  if (!identityStillMatches(child, boundIdentity)) {
    return { confirmed: false, exitCode: null, signal: 'identity_mismatch' };
  }

  const owned: OwnedConnection = {
    recipeGeneration: '',
    child,
    connection: null as unknown as ClientSideConnection,
    acpSessionId: null,
    stdinWritable: child.stdin as Writable,
    admittedIdentity: null,
    boundIdentity,
  };
  try {
    await signalBoundProcessTree(owned, 'SIGTERM');
  } catch {
    return { confirmed: false, exitCode: null, signal: 'identity_mismatch' };
  }

  const termWait = Math.min(2_000, timeoutMs);
  const termReaped = await waitForChildExit(child, termWait);
  if (termReaped) {
    return { confirmed: true, exitCode: child.exitCode, signal: child.signalCode };
  }

  if (!identityStillMatches(child, boundIdentity)) {
    return { confirmed: false, exitCode: null, signal: 'identity_mismatch' };
  }
  try {
    await signalBoundProcessTree(owned, 'SIGKILL');
  } catch {
    return { confirmed: false, exitCode: null, signal: 'identity_mismatch' };
  }

  const killReaped = await waitForChildExit(child, Math.max(0, timeoutMs - termWait));
  return {
    confirmed: killReaped,
    exitCode: child.exitCode,
    signal: child.signalCode,
  };
}

export async function cleanupOwnedConnection(owned: OwnedConnection): Promise<ReapResult> {
  try {
    owned.stdinWritable.end();
  } catch {
    // already closed
  }
  return await reapChild(owned.child, owned.boundIdentity);
}

export async function closeOwnedConnection(owned: OwnedConnection): Promise<ReapResult> {
  return await cleanupOwnedConnection(owned);
}
