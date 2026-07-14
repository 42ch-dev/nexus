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

/** Whitelisted URL values for membership checks (override.installUrl defence). */
const whitelistUrlValues = new Set(Object.values(loaded.install_whitelist));

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

/**
 * Extract the binary basename from a launch command — mirrors the basename
 * logic in `launchCommandMatches` (`apps/web/src/api/queries.ts`).
 *
 * The daemon PATH-scan emits the **full resolved binary path** (e.g.
 * `/usr/local/bin/claude`) as `launch_command`. To match the
 * `NATIVE_LAUNCH_MAP` (keyed on bare names like `claude`), we normalise to
 * the last `/`-separated segment of the first whitespace-delimited token,
 * case-insensitively.
 */
function launchCommandBasename(launch: string): string {
  const binary = launch.split(/\s+/)[0] ?? '';
  const segs = binary.split('/');
  return (segs[segs.length - 1] ?? '').toLowerCase();
}

export function resolveAgentKey(entry: AgentScanEntry): string {
  if (entry.registry_agent_id?.trim()) {
    return entry.registry_agent_id.trim();
  }
  const launch = entry.launch_command?.trim();
  if (launch) {
    const basename = launchCommandBasename(launch);
    if (basename && NATIVE_LAUNCH_MAP[basename]) {
      return NATIVE_LAUNCH_MAP[basename];
    }
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
    installUrl:
      agentOverride?.installUrl && whitelistUrlValues.has(agentOverride.installUrl)
        ? agentOverride.installUrl
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