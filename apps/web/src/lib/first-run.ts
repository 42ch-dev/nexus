/**
 * Browser first-run gate (compass D21 / access spec §3.1).
 *
 * Derived from the unguarded `GET /v1/daemon/runtime/status` — never a
 * per-browser marker — so a fresh origin (including a LAN browser) lands on the
 * wizard while an initialized service skips it. The desktop build keeps its
 * `get_setup_completed` IPC; the two never share storage.
 */
import type { NexusClient } from '@/lib/nexus';

export async function isWorkspaceInitialized(client: NexusClient): Promise<boolean> {
  const status = await client.runtimeStatus();
  return status.workspace_initialized === true;
}
