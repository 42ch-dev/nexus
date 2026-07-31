/**
 * Typed Settings section registry + URL resolver (V1.131 P2).
 *
 * Single ordered **descriptor** SSOT for the Settings modal host and demoted
 * test shell: each entry carries `id`, i18n label key, icon, and content
 * renderer. Aliases normalize `/settings`, `/settings/connection`,
 * `/settings/setup`, and `/modules` without parallel route trees.
 */

import {
  Bot,
  Cpu,
  FolderOpen,
  Palette,
  Settings,
  type LucideIcon,
} from 'lucide-react';
import type { ComponentType } from 'react';

import { SettingsAgentSection } from '@/pages/settings/settings-agent-section';
import { SettingsAdvancedSection } from '@/pages/settings/settings-advanced-section';
import { SettingsAppearanceSection } from '@/pages/settings/settings-appearance-section';
import { SettingsModulesSection } from '@/pages/settings/settings-modules-section';
import { SettingsWorkspaceSection } from '@/pages/settings/settings-workspace-section';

export type SettingsSectionId =
  | 'agent'
  | 'workspace'
  | 'appearance'
  | 'modules'
  | 'advanced';

/** Ordered section descriptor — chrome + content share this SSOT. */
export interface SettingsSectionDescriptor {
  id: SettingsSectionId;
  /** i18n key under the `settings` namespace (e.g. `nav.agent`). */
  labelKey: string;
  icon: LucideIcon;
  Content: ComponentType;
}

export const SETTINGS_SECTION_DESCRIPTORS: readonly SettingsSectionDescriptor[] =
  [
    {
      id: 'agent',
      labelKey: 'nav.agent',
      icon: Bot,
      Content: SettingsAgentSection,
    },
    {
      id: 'workspace',
      labelKey: 'nav.workspace',
      icon: FolderOpen,
      Content: SettingsWorkspaceSection,
    },
    {
      id: 'appearance',
      labelKey: 'nav.appearance',
      icon: Palette,
      Content: SettingsAppearanceSection,
    },
    {
      id: 'modules',
      labelKey: 'nav.modules',
      icon: Cpu,
      Content: SettingsModulesSection,
    },
    {
      id: 'advanced',
      labelKey: 'nav.advanced',
      icon: Settings,
      Content: SettingsAdvancedSection,
    },
  ] as const;

export const SETTINGS_SECTION_IDS: readonly SettingsSectionId[] =
  SETTINGS_SECTION_DESCRIPTORS.map((d) => d.id);

export const SETTINGS_SECTION_BY_ID: Readonly<
  Record<SettingsSectionId, SettingsSectionDescriptor>
> = Object.fromEntries(
  SETTINGS_SECTION_DESCRIPTORS.map((d) => [d.id, d]),
) as Record<SettingsSectionId, SettingsSectionDescriptor>;

export const DEFAULT_SETTINGS_SECTION: SettingsSectionId = 'agent';

export const DEFAULT_SETTINGS_BACKGROUND_PATH = '/works';

export type SettingsCloseReason =
  | 'escape'
  | 'backdrop'
  | 'button'
  | 'route'
  | 'toggle';

export interface ResolvedSettingsLocation {
  section: SettingsSectionId;
  /** Hash fragment without leading `#` (e.g. `connection` for advanced). */
  hash: string;
  /** Canonical pathname under `/settings/:section`. */
  pathname: string;
}

function isSectionId(value: string): value is SettingsSectionId {
  return (SETTINGS_SECTION_IDS as readonly string[]).includes(value);
}

/** True when the browser URL should drive the Settings modal (not a product page). */
export function isSettingsDrivenPath(pathname: string): boolean {
  return (
    pathname === '/modules' ||
    pathname === '/settings' ||
    pathname.startsWith('/settings/')
  );
}

/**
 * Resolve a browser location into a canonical Settings section.
 * Unknown sections fall back to the default (`agent`).
 */
export function resolveSettingsLocation(
  pathname: string,
  hash = '',
): ResolvedSettingsLocation | null {
  if (!isSettingsDrivenPath(pathname)) return null;

  const hashId = hash.startsWith('#') ? hash.slice(1) : hash;

  if (pathname === '/modules') {
    return { section: 'modules', hash: '', pathname: '/settings/modules' };
  }

  if (pathname === '/settings' || pathname === '/settings/') {
    return {
      section: DEFAULT_SETTINGS_SECTION,
      hash: '',
      pathname: `/settings/${DEFAULT_SETTINGS_SECTION}`,
    };
  }

  if (pathname === '/settings/connection') {
    return {
      section: 'advanced',
      hash: 'connection',
      pathname: '/settings/advanced',
    };
  }

  if (pathname === '/settings/setup') {
    return {
      section: 'advanced',
      hash: 'setup',
      pathname: '/settings/advanced',
    };
  }

  const segment = pathname.slice('/settings/'.length).split('/')[0] ?? '';
  if (isSectionId(segment)) {
    return {
      section: segment,
      hash: hashId,
      pathname: `/settings/${segment}`,
    };
  }

  return {
    section: DEFAULT_SETTINGS_SECTION,
    hash: '',
    pathname: `/settings/${DEFAULT_SETTINGS_SECTION}`,
  };
}

/** Build a canonical Settings path for navigation. */
export function settingsPathFor(
  section: SettingsSectionId,
  hash?: string,
  search?: string,
): string {
  const base = `/settings/${section}`;
  const withSearch = search ? `${base}${search}` : base;
  return hash ? `${withSearch}#${hash}` : withSearch;
}

/** Canonical location string for compare/replace navigation. */
export function settingsLocationKey(
  resolved: ResolvedSettingsLocation,
): string {
  return resolved.hash
    ? `${resolved.pathname}#${resolved.hash}`
    : resolved.pathname;
}
