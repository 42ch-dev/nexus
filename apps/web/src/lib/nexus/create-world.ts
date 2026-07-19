import type { NexusClient } from '@/lib/nexus/types';

/** Runtime feature detect for a future Create World client method (V1.125 P2). */
export function hasCreateWorldClient(
  client: NexusClient,
): client is NexusClient & { createWorld: (request: unknown) => Promise<unknown> } {
  return 'createWorld' in client && typeof (client as { createWorld?: unknown }).createWorld === 'function';
}
