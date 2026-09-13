import { spawn } from 'node:child_process';
import type { ChildProcessWithoutNullStreams } from 'node:child_process';
import { Readable, Writable } from 'node:stream';
import type { ValidatedProviderRecipe } from '@42ch/nexus-contracts';
import {
  ClientSideConnection,
  ndJsonStream,
  type Client,
  type Stream,
} from '@agentclientprotocol/sdk';

export const MAX_FRAME_BYTES = 1024 * 1024;

export type SessionUpdateHandler = (params: { sessionId: string; update: unknown }) => void;

export type OwnedConnection = {
  recipeGeneration: string;
  child: ChildProcessWithoutNullStreams;
  connection: ClientSideConnection;
  acpSessionId: string | null;
  stdinWritable: Writable;
};

function sanitizeEnv(env: Record<string, string | undefined>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(env)) {
    if (value !== undefined) out[key] = value;
  }
  return out;
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

export async function spawnOwnedConnection(
  recipe: ValidatedProviderRecipe,
  onSessionUpdate: SessionUpdateHandler,
): Promise<OwnedConnection> {
  const child = spawn(recipe.executable, recipe.args, {
    cwd: recipe.cwd,
    env: { ...process.env, ...sanitizeEnv(recipe.env) },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  if (!child.stdin || !child.stdout || !child.stderr) {
    child.kill();
    throw new Error('provider_spawn_failed');
  }
  child.stderr.on('data', () => undefined);

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
  await connection.initialize({
    protocolVersion: 1,
    clientInfo: { name: 'nexus-provider-acp', version: '0.1.0' },
  });

  return {
    recipeGeneration: recipe.recipe_generation,
    child,
    connection,
    acpSessionId: null,
    stdinWritable: child.stdin,
  };
}

export async function createAcpSession(owned: OwnedConnection, cwd: string): Promise<string> {
  const response = await owned.connection.newSession({ cwd, mcpServers: [] });
  const sessionId = String(response.sessionId);
  owned.acpSessionId = sessionId;
  return sessionId;
}

export async function reapChild(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGTERM');
  await new Promise<void>((resolve) => {
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      resolve();
    }, 2_000);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

export async function closeOwnedConnection(owned: OwnedConnection): Promise<void> {
  try {
    owned.stdinWritable.end();
  } catch {
    // already closed
  }
  await reapChild(owned.child);
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
