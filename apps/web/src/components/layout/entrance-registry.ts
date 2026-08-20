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
import type { SettingsSectionId } from '@/components/layout/settings-section-registry';

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

export const ENTRANCE_DESCRIPTORS: readonly EntranceDescriptor[] = [
  {
    id: 'content-creator',
    layoutLabel: 'Create',
    personaLabel: 'Content creator',
    landRoute: '/works',
    // EL §3 — reduced tree: primary JTBD (works/worlds), Memory, findings.
    navGroups: [
      {
        id: 'works',
        label: 'nav.works',
        items: [
          { to: '/works', label: 'nav.works', icon: Layers },
          { to: '/timeline', label: 'nav.timeline', icon: History },
          { to: '/findings', label: 'nav.findings', icon: FileText },
        ],
      },
      {
        id: 'worlds',
        label: 'nav.worlds',
        items: [{ to: '/worlds', label: 'nav.worlds', icon: Globe }],
      },
      {
        id: 'memory',
        label: 'nav.memory',
        items: [{ to: '/memory', label: 'nav.memory', icon: BrainCircuit }],
      },
    ],
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
      {
        id: 'works',
        label: 'nav.works',
        items: [
          { to: '/works', label: 'nav.works', icon: Layers },
          { to: '/timeline', label: 'nav.timeline', icon: History },
          { to: '/findings', label: 'nav.findings', icon: FileText },
        ],
      },
      {
        id: 'worlds',
        label: 'nav.worlds',
        items: [{ to: '/worlds', label: 'nav.worlds', icon: Globe }],
      },
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
      {
        id: 'memory',
        label: 'nav.memory',
        items: [{ to: '/memory', label: 'nav.memory', icon: BrainCircuit }],
      },
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
