import { resolveCatalogItem } from '@/lib/agent-catalog';
import type { AgentOutboundUrls } from '@/lib/agent-catalog';

export type { AgentOutboundUrls } from '@/lib/agent-catalog';

export function lookupAgentOutboundUrls(
  registryAgentId: string | null | undefined,
  name: string,
): AgentOutboundUrls {
  const item = resolveCatalogItem({
    name,
    registry_agent_id: registryAgentId ?? null,
    installed: false,
    launch_command: null,
    version: null,
    description: null,
    icon_url: null,
  });
  return {
    installUrl: item.installUrl ?? null,
    docsUrl: item.docsUrl ?? null,
  };
}