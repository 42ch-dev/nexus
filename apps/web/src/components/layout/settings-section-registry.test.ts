/**
 * Settings section registry + URL resolver (V1.131 P2).
 */
import { describe, expect, it } from 'vitest';

import {
  DEFAULT_SETTINGS_SECTION,
  isSettingsDrivenPath,
  resolveSettingsLocation,
  settingsPathFor,
} from './settings-section-registry';

describe('settings-section-registry', () => {
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
