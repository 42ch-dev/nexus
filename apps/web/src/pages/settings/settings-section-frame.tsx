/**
 * Settings section frame — modal-primary chrome (V1.131 P2).
 *
 * Presentational section nav + outlet region. Host supplies active section and
 * selection; section bodies remain content-only (no Dialog ownership).
 */

import {
  Bot,
  Cpu,
  FolderOpen,
  Palette,
  Settings,
  type LucideIcon,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ReactNode } from 'react';

import {
  SETTINGS_SECTION_IDS,
  type SettingsSectionId,
} from '@/components/layout/settings-section-registry';
import { cn } from '@/lib/utils';

const SECTION_ICONS: Record<SettingsSectionId, LucideIcon> = {
  agent: Bot,
  workspace: FolderOpen,
  appearance: Palette,
  modules: Cpu,
  advanced: Settings,
};

export interface SettingsSectionFrameProps {
  activeSection: SettingsSectionId;
  onSelectSection: (section: SettingsSectionId) => void;
  children: ReactNode;
  /** Optional helper under the title row — modal host usually omits (title is Dialog). */
  showPageChrome?: boolean;
}

export function SettingsSectionFrame({
  activeSection,
  onSelectSection,
  children,
  showPageChrome = false,
}: SettingsSectionFrameProps) {
  const { t } = useTranslation('settings');

  return (
    <div
      className="flex min-h-0 flex-1 flex-col gap-4"
      data-testid="settings-shell"
    >
      {showPageChrome ? (
        <div className="flex flex-col gap-2 px-6 pt-2">
          <h2 className="text-heading-24 font-heading text-gray-1000">
            {t('title')}
          </h2>
          <p className="text-copy-14 text-gray-900">{t('helper')}</p>
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1 gap-0">
        <nav
          aria-label={t('aria.sections')}
          className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-gray-alpha-200 px-3 py-2"
          data-testid="settings-section-nav"
        >
          {SETTINGS_SECTION_IDS.map((id) => {
            const Icon = SECTION_ICONS[id];
            const active = activeSection === id;
            return (
              <button
                key={id}
                type="button"
                data-testid={`settings-section-nav-${id}`}
                aria-current={active ? 'page' : undefined}
                onClick={() => onSelectSection(id)}
                className={cn(
                  'inline-flex items-center gap-2 rounded-control px-3 py-2 text-left text-label-14 font-medium',
                  'transition-colors duration-state ease-standard',
                  active
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )}
              >
                <Icon className="size-4 shrink-0" aria-hidden="true" />
                <span>{t(`nav.${id}`)}</span>
              </button>
            );
          })}
        </nav>

        <div
          className="min-h-0 min-w-0 flex-1 overflow-y-auto px-6 py-2"
          data-testid="settings-shell-outlet"
        >
          {children}
        </div>
      </div>
    </div>
  );
}
