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
  /**
   * Stable catalog key — the canonical agent identity used for override /
   * whitelist lookup and the default-grid / more-agents split. MAY be shared
   * by more than one scan row (e.g. two `claude` installs both resolve to
   * `claude-native`). Use {@link pickerId} for selection.
   */
  id: string;
  /**
   * Collision-safe selection handle, unique per scan row. Equals {@link id}
   * when no other row resolves to the same catalog key; suffixed
   * (`<id>__2`, `<id>__3`, …) on collision so each displayed card maps to
   * exactly one scan entry (PR#148 Greptile P1). Computed over the full scan
   * set by {@link resolveCatalogItems} / {@link buildPickerSelection}.
   */
  pickerId: string;
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
    // Single-entry default: equals the catalog key. Collision-safe suffixing
    // happens at the list level in `resolveCatalogItems` (needs full-set
    // context).
    pickerId: key,
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

/**
 * Resolve every scan entry to a catalog item with a collision-safe
 * {@link AgentCatalogItem.pickerId}.
 *
 * `id` stays the stable catalog key (override/whitelist lookup, default/more
 * split). `pickerId` is the unique-per-row selection handle: the first row for
 * a key keeps the key; later rows get `<key>__2`, `<key>__3`, … This guarantees
 * that selecting a displayed card always resolves to the exact scan row shown —
 * even when two installs resolve to the same native key, the saved
 * `launch_command` is the one on the card the author clicked (PR#148 Greptile
 * P1). Single source for `defaultGridEntries` / `moreAgentsEntries` /
 * `buildPickerSelection` so all three share one picker-id namespace.
 */
export function resolveCatalogItems(entries: AgentScanEntry[]): AgentCatalogItem[] {
  const seen = new Map<string, number>();
  return entries.map((entry) => {
    const item = resolveCatalogItem(entry);
    const occurrences = seen.get(item.id) ?? 0;
    seen.set(item.id, occurrences + 1);
    const pickerId = occurrences === 0 ? item.id : `${item.id}__${occurrences + 1}`;
    return { ...item, pickerId };
  });
}

export function defaultGridEntries(entries: AgentScanEntry[]): AgentCatalogItem[] {
  return resolveCatalogItems(entries)
    .filter((item) => !item.hiddenFromDefault)
    .filter((item) => item.installed || item.priority !== undefined)
    .sort((a, b) => {
      const pa = a.priority ?? Infinity;
      const pb = b.priority ?? Infinity;
      return pa - pb;
    });
}

export function moreAgentsEntries(entries: AgentScanEntry[]): AgentCatalogItem[] {
  const defaultItemIds = new Set(defaultGridEntries(entries).map((i) => i.id));
  return resolveCatalogItems(entries)
    .filter((item) => !defaultItemIds.has(item.id))
    .sort((a, b) => {
      if (a.installed !== b.installed) return a.installed ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
}

export function prioritizeInstalled(items: AgentCatalogItem[]): AgentCatalogItem[] {
  return [...items].sort((a, b) => {
    if (a.installed !== b.installed) return a.installed ? -1 : 1;
    return 0;
  });
}

/** Bidirectional picker-id ↔ scan-entry index over a scan result. */
export interface AgentPickerSelection {
  /** Map a scan entry (by reference) → its collision-safe picker id. */
  byEntry: Map<AgentScanEntry, string>;
  /** Map a collision-safe picker id → its scan entry (selection lookup). */
  byPickerId: Map<string, AgentScanEntry>;
}

/**
 * Build the bidirectional picker-id ↔ scan-entry index over a scan result.
 *
 * Hosts use `byPickerId` to resolve a selected card back to the exact scan row
 * — replacing a catalog-key map that silently collides and could save the
 * wrong `launch_command`. `byEntry` derives the selected card id from a held
 * `AgentScanEntry` (PR#148 Greptile P1). Keyed by object reference: entries
 * must come from the same scan array the picker items were built from.
 */
export function buildPickerSelection(entries: AgentScanEntry[]): AgentPickerSelection {
  const items = resolveCatalogItems(entries);
  const byEntry = new Map<AgentScanEntry, string>();
  const byPickerId = new Map<string, AgentScanEntry>();
  items.forEach((item, index) => {
    const entry = entries[index]!;
    byEntry.set(entry, item.pickerId);
    byPickerId.set(item.pickerId, entry);
  });
  return { byEntry, byPickerId };
}