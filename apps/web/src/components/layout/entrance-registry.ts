/**
 * User-layer entrance registry (V1.170 P1 — AR-15).
 *
 * Single typed **descriptor** + **route classification** SSOT for the
 * Create | Develop layout split. Mirrors the `SETTINGS_SECTION_DESCRIPTORS`
 * pattern (`settings-section-registry.ts`): one readonly descriptor array plus
 * derived index maps, with a route-rule table driving the guard.
 *
 * Two layout trees = one registry: entrance-filtered sidebar `navGroups`,
 * land routes (guard bounces + index redirect), and route classification.
 * The entrance axis is orthogonal to the Creator | Orchestrator tabs
 * (`sidebar.tsx` `tabFromPathname` is untouched).
 *
 * Label convention: `ShellNavGroup`/`ShellNavItem` labels are **`shell`
 * namespace i18n keys** (e.g. `nav.memory`). The sidebar resolves them through
 * `t()` at render time; keys without values yet land in T4 (AR-21).
 */

import {
  Boxes,
  BrainCircuit,
  CalendarClock,
  Compass,
  Cpu,
  FileText,
  Globe,
  History,
  Layers,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import type { ShellNavGroup } from '@/components/layout/presentational/shell-sidebar-chrome';
import {
  DEFAULT_SETTINGS_SECTION,
  SETTINGS_SECTION_IDS,
  type SettingsSectionId,
} from '@/components/layout/settings-section-registry';

export type EntranceId = 'content-creator' | 'developer'; // chrome labels: Create | Develop

export interface EntranceRouteRule {
  /** Route path (App.tsx SSOT); longest-prefix match. */
  path: string;
  /** Nothing is create-only (Develop = full config). */
  visibility: 'both' | 'develop-only';
  /** Pass-through on the hidden tree (product lock: strategy canvas OK). */
  allowDeepLink?: boolean;
  /** For /settings/* + /modules modal-alias resolution. */
  settingsSection?: SettingsSectionId;
}

export interface EntranceDescriptor {
  id: EntranceId;
  /** "Create" | "Develop" (chrome labels). */
  layoutLabel: string;
  /** "Content creator" | "Developer" (identity page). */
  personaLabel: string;
  /** '/works' | '/developer' — single source for guard bounces AND the index redirect (AR-18). */
  landRoute: string;
  /** Entrance-filtered sidebar tree. */
  navGroups: readonly ShellNavGroup[];
  /** Sections hidden from the tree (Create: agent | modules | advanced). */
  hiddenSettingsSections: readonly SettingsSectionId[];
  /** i18n shell key for the guard bounce toast (AR-19). */
  bounceToastKey: string;
}

/**
 * Route classification SSOT (AR-15). `develop-only` routes bounce to the
 * entrance `landRoute` on Create unless `allowDeepLink`; everything else is
 * `both` by default (no rule = pass through). Settings sections resolve via
 * `resolveSettingsLocation` first (AR-19) — their rules carry `settingsSection`.
 */
export const ENTRANCE_ROUTE_RULES: readonly EntranceRouteRule[] = [
  // develop-only surfaces (guard-bounced on Create)
  { path: '/strategies', visibility: 'develop-only' },
  // Strategy canvas — support deep-link per product EL §3 (no bounce, no toast).
  { path: '/strategies/:presetId', visibility: 'develop-only', allowDeepLink: true },
  { path: '/sessions', visibility: 'develop-only' },
  { path: '/schedule', visibility: 'develop-only' },
  // Settings modal alias (V1.131 P2) — section resolution owns the check.
  { path: '/modules', visibility: 'develop-only' },
  { path: '/capabilities', visibility: 'develop-only' },
  { path: '/connect', visibility: 'develop-only' },
  { path: '/developer', visibility: 'develop-only' },
  { path: '/works/:workId/inspector', visibility: 'develop-only' },
  // Settings sections — section-level visibility via resolveSettingsLocation.
  { path: '/settings/agent', visibility: 'develop-only', settingsSection: 'agent' },
  { path: '/settings/modules', visibility: 'develop-only', settingsSection: 'modules' },
  { path: '/settings/advanced', visibility: 'develop-only', settingsSection: 'advanced' },
  { path: '/settings/workspace', visibility: 'both', settingsSection: 'workspace' },
  { path: '/settings/appearance', visibility: 'both', settingsSection: 'appearance' },
] as const;

export const ENTRANCE_IDS: readonly EntranceId[] = ['content-creator', 'developer'];

export const DEFAULT_ENTRANCE: EntranceId = 'content-creator';

/** Type guard for persisted/stored entrance strings (AR-16). Unparseable
 * values resolve to the default — never to a third state. */
export function isEntranceId(value: string | null | undefined): value is EntranceId {
  return value === 'content-creator' || value === 'developer';
}

/**
 * Both-visibility nav groups (EL §3 keep-visible table) — ONE source shared by
 * both entrance descriptors (plan QC S-1): adding a surface visible on both
 * trees edits this constant only, not the two descriptor lists.
 */
const WORKS_GROUP: ShellNavGroup = {
  id: 'works',
  label: 'nav.works',
  items: [
    { to: '/works', label: 'nav.works', icon: Layers },
    { to: '/timeline', label: 'nav.timeline', icon: History },
    { to: '/findings', label: 'nav.findings', icon: FileText },
  ],
};

const WORLDS_GROUP: ShellNavGroup = {
  id: 'worlds',
  label: 'nav.worlds',
  items: [{ to: '/worlds', label: 'nav.worlds', icon: Globe }],
};

const MEMORY_GROUP: ShellNavGroup = {
  id: 'memory',
  label: 'nav.memory',
  items: [{ to: '/memory', label: 'nav.memory', icon: BrainCircuit }],
};

const COMMON_NAV_GROUPS: readonly ShellNavGroup[] = [
  WORKS_GROUP,
  WORLDS_GROUP,
  MEMORY_GROUP,
];

export const ENTRANCE_DESCRIPTORS: readonly EntranceDescriptor[] = [
  {
    id: 'content-creator',
    layoutLabel: 'Create',
    personaLabel: 'Content creator',
    landRoute: '/works',
    // EL §3 — reduced tree: primary JTBD (works/worlds), Memory, findings.
    navGroups: COMMON_NAV_GROUPS,
    hiddenSettingsSections: ['agent', 'modules', 'advanced'],
    bounceToastKey: 'entrance.bounceToast',
  },
  {
    id: 'developer',
    layoutLabel: 'Develop',
    personaLabel: 'Developer',
    landRoute: '/developer',
    // EL §4 — full config surface + Develop hub as the land route.
    navGroups: [
      {
        id: 'hub',
        label: 'nav.develop',
        items: [{ to: '/developer', label: 'nav.develop', icon: Compass }],
      },
      WORKS_GROUP,
      WORLDS_GROUP,
      {
        id: 'strategies',
        label: 'nav.strategies',
        items: [{ to: '/strategies', label: 'nav.strategies', icon: Sparkles }],
      },
      {
        id: 'runtime',
        label: 'nav.runtime',
        items: [
          { to: '/sessions', label: 'nav.sessions', icon: ListChecks },
          { to: '/schedule', label: 'nav.schedule', icon: CalendarClock },
        ],
      },
      {
        id: 'compute',
        label: 'nav.compute',
        items: [
          { to: '/capabilities', label: 'nav.capabilities', icon: Boxes },
          { to: '/settings/modules', label: 'nav.modules', icon: Cpu },
        ],
      },
      MEMORY_GROUP,
    ],
    hiddenSettingsSections: [],
    bounceToastKey: 'entrance.bounceToast',
  },
] as const;

export const ENTRANCE_BY_ID: Readonly<
  Record<EntranceId, EntranceDescriptor>
> = Object.fromEntries(
  ENTRANCE_DESCRIPTORS.map((d) => [d.id, d]),
) as Record<EntranceId, EntranceDescriptor>;

/**
 * First Settings section visible on the given entrance (plan QC W-2): the
 * titlebar gear (and any default-section opener) must land on a section the
 * entrance does not hide — Create → `workspace`, Develop → `agent`
 * (unchanged full-Control-Room default). Section order follows
 * `SETTINGS_SECTION_DESCRIPTORS`.
 */
export function firstSettingsSectionFor(entrance: EntranceId): SettingsSectionId {
  const hidden = ENTRANCE_BY_ID[entrance].hiddenSettingsSections;
  return (
    SETTINGS_SECTION_IDS.find((id) => !hidden.includes(id)) ?? DEFAULT_SETTINGS_SECTION
  );
}

/**
 * Longest-prefix rule match for a pathname (AR-19). `:param` segments match
 * any single segment; settings-section rules are excluded here — the guard
 * resolves `/settings/*` and `/modules` through {@link resolveSettingsLocation}
 * first. No match → `null` → pass through (the `*` NotFoundPage owns unknowns).
 */
export function matchEntranceRouteRule(pathname: string): EntranceRouteRule | null {
  const segments = pathname.split('/').filter(Boolean);
  let best: EntranceRouteRule | null = null;
  let bestLength = -1;
  for (const rule of ENTRANCE_ROUTE_RULES) {
    if (rule.settingsSection) continue;
    const ruleSegments = rule.path.split('/').filter(Boolean);
    if (ruleSegments.length > segments.length || ruleSegments.length <= bestLength) {
      continue;
    }
    let matches = true;
    for (let i = 0; i < ruleSegments.length; i += 1) {
      const part = ruleSegments[i];
      if (part.startsWith(':')) continue;
      if (part !== segments[i]) {
        matches = false;
        break;
      }
    }
    if (matches) {
      best = rule;
      bestLength = ruleSegments.length;
    }
  }
  return best;
}
