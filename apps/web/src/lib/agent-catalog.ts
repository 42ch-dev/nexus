import type { AgentScanEntry } from '@42ch/nexus-contracts';
import overrides from '@config/agent-catalog-overrides.json';

export interface AgentOutboundUrls {
  installUrl?: string | null;
  docsUrl?: string | null;
}

export interface AgentOverride {
  displayName?: string;
  docsUrl?: string;
  installUrl?: string;
  iconUrl?: string;
  hiddenFromDefault?: boolean;
  priority?: number;
}

export interface AgentCatalogOverrides {
  schema_version: number;
  install_whitelist: Record<string, string>;
  agents: Record<string, AgentOverride>;
}

export interface AgentCatalogItem {
  id: string;
  name: string;
  displayName: string;
  version?: string | null;
  description?: string | null;
  iconUrl?: string | null;
  installed: boolean;
  installUrl?: string | null;
  docsUrl?: string | null;
  hiddenFromDefault: boolean;
  priority?: number;
  registryAgentId?: string | null;
  launchCommand?: string | null;
}

const loaded = overrides as AgentCatalogOverrides;

export function resolveInstallUrl(key: string): string | null {
  return loaded.install_whitelist[key] ?? null;
}

export function isHiddenFromDefault(key: string): boolean {
  return loaded.agents[key]?.hiddenFromDefault === true;
}

const NATIVE_LAUNCH_MAP: Record<string, string> = {
  claude: 'claude-native',
  codex: 'codex-native',
};

export function resolveAgentKey(entry: AgentScanEntry): string {
  if (entry.registry_agent_id?.trim()) {
    return entry.registry_agent_id.trim();
  }
  const launch = entry.launch_command?.trim().toLowerCase();
  if (launch && NATIVE_LAUNCH_MAP[launch]) {
    return NATIVE_LAUNCH_MAP[launch];
  }
  return entry.name.trim();
}

export function resolveCatalogItem(entry: AgentScanEntry): AgentCatalogItem {
  const key = resolveAgentKey(entry);
  const agentOverride = loaded.agents[key];
  const whitelistUrl = resolveInstallUrl(key);
  return {
    id: key,
    name: entry.name,
    displayName: agentOverride?.displayName ?? entry.name,
    version: entry.version,
    description: entry.description,
    iconUrl: entry.icon_url ?? agentOverride?.iconUrl ?? null,
    installed: entry.installed,
    installUrl: agentOverride?.installUrl
      ? (resolveInstallUrl(agentOverride.installUrl) ?? whitelistUrl)
      : whitelistUrl,
    docsUrl: agentOverride?.docsUrl ?? null,
    hiddenFromDefault: isHiddenFromDefault(key),
    priority: agentOverride?.priority,
    registryAgentId: entry.registry_agent_id,
    launchCommand: entry.launch_command,
  };
}

export function defaultGridEntries(entries: AgentScanEntry[]): AgentCatalogItem[] {
  const items = entries
    .map(resolveCatalogItem)
    .filter((item) => !item.hiddenFromDefault)
    .filter((item) => item.installed || item.priority !== undefined)
    .sort((a, b) => {
      const pa = a.priority ?? Infinity;
      const pb = b.priority ?? Infinity;
      return pa - pb;
    });
  return items;
}

export function moreAgentsEntries(entries: AgentScanEntry[]): AgentCatalogItem[] {
  const defaultItemIds = new Set(defaultGridEntries(entries).map((i) => i.id));
  const items = entries
    .map(resolveCatalogItem)
    .filter((item) => !defaultItemIds.has(item.id))
    .sort((a, b) => {
      if (a.installed !== b.installed) return a.installed ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
  return items;
}

export function prioritizeInstalled(items: AgentCatalogItem[]): AgentCatalogItem[] {
  return [...items].sort((a, b) => {
    if (a.installed !== b.installed) return a.installed ? -1 : 1;
    return 0;
  });
}