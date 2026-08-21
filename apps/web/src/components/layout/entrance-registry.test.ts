/**
 * Entrance registry classification SSOT (V1.170 P1 — AR-15).
 *
 * Pins the AR-15 route table: develop-only list, allow-deep-link exception,
 * settings-section rules, and the index helpers mirroring
 * `SETTINGS_SECTION_IDS` / `SETTINGS_SECTION_BY_ID`.
 */
import { describe, expect, it } from 'vitest';

import {
  DEFAULT_ENTRANCE,
  ENTRANCE_BY_ID,
  ENTRANCE_DESCRIPTORS,
  ENTRANCE_IDS,
  ENTRANCE_ROUTE_RULES,
  firstSettingsSectionFor,
  isEntranceId,
  matchEntranceRouteRule,
} from '@/components/layout/entrance-registry';
import { SETTINGS_SECTION_IDS } from '@/components/layout/settings-section-registry';

describe('entrance registry (AR-15)', () => {
  it('defaults to content-creator (AR-16)', () => {
    expect(DEFAULT_ENTRANCE).toBe('content-creator');
  });

  it('isEntranceId accepts exactly the two pinned ids (AR-16)', () => {
    expect(isEntranceId('content-creator')).toBe(true);
    expect(isEntranceId('developer')).toBe(true);
    expect(isEntranceId('admin')).toBe(false);
    expect(isEntranceId('')).toBe(false);
    expect(isEntranceId(null)).toBe(false);
    expect(isEntranceId(undefined)).toBe(false);
  });

  it('indexes exactly the two pinned entrance ids', () => {
    expect(ENTRANCE_IDS).toEqual(['content-creator', 'developer']);
    expect(ENTRANCE_DESCRIPTORS.map((d) => d.id)).toEqual(ENTRANCE_IDS);
    for (const id of ENTRANCE_IDS) {
      expect(ENTRANCE_BY_ID[id].id).toBe(id);
    }
  });

  it('pins land routes — the single source for guard bounces and the index redirect (AR-18)', () => {
    expect(ENTRANCE_BY_ID['content-creator'].landRoute).toBe('/works');
    expect(ENTRANCE_BY_ID['developer'].landRoute).toBe('/developer');
  });

  it('hides the EL §3 settings sections on Create only', () => {
    expect(ENTRANCE_BY_ID['content-creator'].hiddenSettingsSections).toEqual([
      'agent',
      'modules',
      'advanced',
    ]);
    expect(ENTRANCE_BY_ID['developer'].hiddenSettingsSections).toEqual([]);
  });

  it('resolves the first entrance-visible settings section (W-2)', () => {
    // Create: `agent` (the historic default) is develop-only → workspace.
    expect(firstSettingsSectionFor('content-creator')).toBe('workspace');
    // Develop: full Control Room — the historic `agent` default is kept.
    expect(firstSettingsSectionFor('developer')).toBe('agent');
  });

  it('pins the develop-only bounce table verbatim (AR-15)', () => {
    const developOnly = ENTRANCE_ROUTE_RULES.filter(
      (rule) => rule.visibility === 'develop-only',
    ).map((rule) => rule.path);
    expect(developOnly).toEqual([
      '/strategies',
      '/strategies/:presetId',
      '/sessions',
      '/schedule',
      '/modules',
      '/capabilities',
      '/connect',
      '/developer',
      '/works/:workId/inspector',
      '/settings/agent',
      '/settings/modules',
      '/settings/advanced',
    ]);
  });

  it('marks the strategy canvas as the only allow-deep-link surface', () => {
    const deepLinks = ENTRANCE_ROUTE_RULES.filter((rule) => rule.allowDeepLink);
    expect(deepLinks).toHaveLength(1);
    expect(deepLinks[0]).toMatchObject({
      path: '/strategies/:presetId',
      visibility: 'develop-only',
      allowDeepLink: true,
    });
  });

  it('keeps settings-section rules in sync with the section registry', () => {
    const sectionRules = ENTRANCE_ROUTE_RULES.filter(
      (rule) => rule.settingsSection !== undefined,
    );
    expect(
      sectionRules.map((rule) => rule.settingsSection).sort((a, b) => (a ?? '').localeCompare(b ?? '')),
    ).toEqual(
      [...SETTINGS_SECTION_IDS].sort(),
    );
    // develop-only sections == Create's hiddenSettingsSections (guard parity).
    const developOnlySections = sectionRules
      .filter((rule) => rule.visibility === 'develop-only')
      .map((rule) => rule.settingsSection)
      .sort((a, b) => (a ?? '').localeCompare(b ?? ''));
    expect(developOnlySections).toEqual(
      [...ENTRANCE_BY_ID['content-creator'].hiddenSettingsSections].sort(),
    );
  });

  it('matches develop-only routes by longest prefix', () => {
    expect(matchEntranceRouteRule('/strategies')?.visibility).toBe('develop-only');
    expect(matchEntranceRouteRule('/strategies/preset-1')?.path).toBe(
      '/strategies/:presetId',
    );
    expect(matchEntranceRouteRule('/works/w1/inspector')?.path).toBe(
      '/works/:workId/inspector',
    );
    expect(matchEntranceRouteRule('/sessions')?.visibility).toBe('develop-only');
    expect(matchEntranceRouteRule('/schedule')?.visibility).toBe('develop-only');
  });

  it('passes through everything not in the develop-only table (both by default)', () => {
    expect(matchEntranceRouteRule('/works')).toBeNull();
    expect(matchEntranceRouteRule('/works/w1/outline')).toBeNull();
    expect(matchEntranceRouteRule('/worlds')).toBeNull();
    expect(matchEntranceRouteRule('/worlds/w1/timeline')).toBeNull();
    expect(matchEntranceRouteRule('/timeline')).toBeNull();
    expect(matchEntranceRouteRule('/findings')).toBeNull();
    expect(matchEntranceRouteRule('/memory')).toBeNull();
    expect(matchEntranceRouteRule('/strategiesx')).toBeNull();
    // Settings-driven paths resolve via resolveSettingsLocation, not prefix
    // matching — the `/settings/*` rules are section rules, not path rules.
    expect(matchEntranceRouteRule('/settings/agent')).toBeNull();
    // `/modules` is itself in the develop-only table (settings alias).
    expect(matchEntranceRouteRule('/modules')?.visibility).toBe('develop-only');
  });

  it('builds entrance-filtered nav trees (EL §3 / §4)', () => {
    const createTargets = ENTRANCE_BY_ID['content-creator'].navGroups.flatMap(
      (group) => group.items.map((item) => item.to),
    );
    expect(createTargets).toContain('/works');
    expect(createTargets).toContain('/worlds');
    expect(createTargets).toContain('/memory');
    expect(createTargets).not.toContain('/strategies');
    expect(createTargets).not.toContain('/sessions');
    expect(createTargets).not.toContain('/schedule');
    expect(createTargets).not.toContain('/capabilities');
    expect(createTargets).not.toContain('/developer');

    const developTargets = ENTRANCE_BY_ID['developer'].navGroups.flatMap(
      (group) => group.items.map((item) => item.to),
    );
    expect(developTargets).toContain('/developer');
    expect(developTargets).toContain('/strategies');
    expect(developTargets).toContain('/sessions');
    expect(developTargets).toContain('/schedule');
    expect(developTargets).toContain('/capabilities');
    expect(developTargets).toContain('/settings/modules');
    expect(developTargets).toContain('/memory');
  });
});
