import { describe, expect, it, vi } from 'vitest';

import { isWorkspaceInitialized } from './first-run';
import type { NexusClient } from '@/lib/nexus';

function client(workspaceInitialized: boolean): NexusClient {
  return {
    runtimeStatus: vi.fn().mockResolvedValue({ workspace_initialized: workspaceInitialized }),
  } as unknown as NexusClient;
}

describe('isWorkspaceInitialized', () => {
  it('reports first run for a service without an initialized workspace', async () => {
    const probe = client(false);
    await expect(isWorkspaceInitialized(probe)).resolves.toBe(false);
    expect(probe.runtimeStatus).toHaveBeenCalledTimes(1);
  });

  it('reports an initialized service as returning', async () => {
    await expect(isWorkspaceInitialized(client(true))).resolves.toBe(true);
  });
});
