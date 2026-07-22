/**
 * Typed Settings section registry + URL resolver (V1.131 P2).
 *
 * Single ordered registry for the Settings modal host. Aliases normalize
 * `/settings`, `/settings/connection`, `/settings/setup`, and `/modules`
 * without parallel route trees.
 */

export type SettingsSectionId =
  | 'agent'
  | 'workspace'
  | 'appearance'
  | 'modules'
  | 'advanced';

export const SETTINGS_SECTION_IDS: readonly SettingsSectionId[] = [
  'agent',
  'workspace',
  'appearance',
  'modules',
  'advanced',
] as const;

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
): string {
  const base = `/settings/${section}`;
  return hash ? `${base}#${hash}` : base;
}

/** Canonical location string for compare/replace navigation. */
export function settingsLocationKey(
  resolved: ResolvedSettingsLocation,
): string {
  return resolved.hash
    ? `${resolved.pathname}#${resolved.hash}`
    : resolved.pathname;
}
