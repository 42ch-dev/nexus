/**
 * Settings section registry + URL resolver (V1.131 P2).
 */
import { describe, expect, it } from 'vitest';

import {
  DEFAULT_SETTINGS_SECTION,
  SETTINGS_SECTION_BY_ID,
  SETTINGS_SECTION_DESCRIPTORS,
  SETTINGS_SECTION_IDS,
  isSettingsDrivenPath,
  resolveSettingsLocation,
  settingsPathFor,
} from './settings-section-registry';

describe('settings-section-registry', () => {
  it('exposes an ordered descriptor SSOT with id, labelKey, icon, Content', () => {
    expect(SETTINGS_SECTION_IDS).toEqual([
      'agent',
      'workspace',
      'appearance',
      'modules',
      'advanced',
    ]);
    expect(SETTINGS_SECTION_DESCRIPTORS.map((d) => d.id)).toEqual([
      ...SETTINGS_SECTION_IDS,
    ]);

    for (const descriptor of SETTINGS_SECTION_DESCRIPTORS) {
      expect(descriptor.labelKey).toBe(`nav.${descriptor.id}`);
      expect(descriptor.icon).toBeTruthy();
      expect(descriptor.Content).toBeTypeOf('function');
      expect(SETTINGS_SECTION_BY_ID[descriptor.id]).toBe(descriptor);
    }
  });

  it('treats /settings/* and /modules as settings-driven', () => {
    expect(isSettingsDrivenPath('/settings')).toBe(true);
    expect(isSettingsDrivenPath('/settings/agent')).toBe(true);
    expect(isSettingsDrivenPath('/modules')).toBe(true);
    expect(isSettingsDrivenPath('/works')).toBe(false);
  });

  it('resolves aliases and unknown sections', () => {
    expect(resolveSettingsLocation('/settings')).toEqual({
      section: DEFAULT_SETTINGS_SECTION,
      hash: '',
      pathname: '/settings/agent',
    });
    expect(resolveSettingsLocation('/settings/connection')).toEqual({
      section: 'advanced',
      hash: 'connection',
      pathname: '/settings/advanced',
    });
    expect(resolveSettingsLocation('/settings/setup')).toEqual({
      section: 'advanced',
      hash: 'setup',
      pathname: '/settings/advanced',
    });
    expect(resolveSettingsLocation('/modules')).toEqual({
      section: 'modules',
      hash: '',
      pathname: '/settings/modules',
    });
    expect(resolveSettingsLocation('/settings/nope')).toEqual({
      section: DEFAULT_SETTINGS_SECTION,
      hash: '',
      pathname: '/settings/agent',
    });
    expect(resolveSettingsLocation('/settings/advanced', '#connection')).toEqual({
      section: 'advanced',
      hash: 'connection',
      pathname: '/settings/advanced',
    });
  });

  it('builds canonical settings paths', () => {
    expect(settingsPathFor('modules')).toBe('/settings/modules');
    expect(settingsPathFor('advanced', 'setup')).toBe('/settings/advanced#setup');
  });
});
