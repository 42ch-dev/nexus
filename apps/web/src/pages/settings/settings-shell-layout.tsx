/**
 * Settings shell layout — demoted / test-only (V1.131 P2).
 *
 * Full-page Settings is no longer the product primary. The App mounts
 * {@link SettingsModalHost} with {@link SettingsSectionFrame}. This layout
 * remains for isolated section unit tests that still compose an Outlet tree.
 * Nav entries are generated from {@link SETTINGS_SECTION_DESCRIPTORS} so the
 * test shell cannot drift from the modal registry SSOT.
 */

import { useTranslation } from 'react-i18next';
import { NavLink, Outlet } from 'react-router-dom';

import {
  SETTINGS_SECTION_DESCRIPTORS,
  settingsPathFor,
} from '@/components/layout/settings-section-registry';
import { cn } from '@/lib/utils';

export function SettingsShellLayout() {
  const { t } = useTranslation('settings');

  return (
    <div
      className="flex flex-col gap-6 max-w-2xl w-full"
      data-testid="settings-shell"
    >
      <div className="flex flex-col gap-2">
        {/* Visual page title; document h1 lives in RootLayout Header. */}
        <h2 className="text-heading-24 font-heading text-gray-1000">{t('title')}</h2>
        <p className="text-copy-14 text-gray-900">{t('helper')}</p>
      </div>

      <nav
        aria-label={t('aria.sections')}
        className="flex flex-wrap gap-1 border-b border-gray-alpha-200 pb-px"
        data-testid="settings-section-nav"
      >
        {SETTINGS_SECTION_DESCRIPTORS.map(({ id, labelKey, icon: Icon }) => (
          <NavLink
            key={id}
            to={settingsPathFor(id)}
            data-testid={`settings-section-nav-${id}`}
            className={({ isActive }) =>
              cn(
                'inline-flex items-center gap-2 px-3 py-2 text-label-14 font-medium',
                'border-b-2 -mb-px transition-colors duration-state ease-standard',
                isActive
                  ? 'text-gray-1000 border-blue-700'
                  : 'text-gray-700 border-transparent hover:text-gray-1000 hover:border-gray-alpha-400',
              )
            }
          >
            <Icon className="size-4 shrink-0" aria-hidden="true" />
            <span>{t(labelKey)}</span>
          </NavLink>
        ))}
      </nav>

      <div data-testid="settings-shell-outlet">
        <Outlet />
      </div>
    </div>
  );
}
