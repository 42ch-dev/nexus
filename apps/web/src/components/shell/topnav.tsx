/**
 * Topnav — identity, not navigation (IA spec §3.1).
 *
 * Left: logo + Creator profile switcher. Right: search (disabled), language,
 * theme toggle, settings entry, agent chip. Identical on both entrances.
 */
import { Globe, Moon, Search, Settings, Sun } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { useSettingsModal } from '@/components/layout/settings-modal-context';
import { AgentChip } from '@/components/shell/agent-chip';
import { ProfileSwitcher } from '@/components/shell/profile-switcher';
import { useLocale } from '@/components/locale-provider';
import { useTheme } from '@/components/theme-provider';

export function Topnav() {
  const { t } = useTranslation('shell');
  const { openSettings } = useSettingsModal();
  const { resolvedTheme, toggleTheme } = useTheme();
  const { resolvedLocale, setPreference } = useLocale();

  return (
    <header
      data-testid="topnav"
      className="flex h-12 shrink-0 items-center gap-2 border-b border-gray-alpha-400 bg-background-100 px-3"
    >
      <NexusLogo />
      <span aria-hidden="true" className="h-5 w-px bg-gray-alpha-400" />
      <ProfileSwitcher />

      <div className="ml-auto flex items-center gap-1">
        <button
          type="button"
          disabled
          title={t('topnav.searchDisabled')}
          aria-label={t('topnav.searchDisabled')}
          className="flex h-8 w-8 cursor-not-allowed items-center justify-center rounded-control text-gray-600 opacity-35"
        >
          <Search className="h-4 w-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          title={t('topnav.language')}
          aria-label={t('topnav.language')}
          onClick={() => setPreference(resolvedLocale === 'zh-CN' ? 'en' : 'zh-CN')}
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-800 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none"
        >
          <Globe className="h-4 w-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          title={t('topnav.theme')}
          aria-label={t('topnav.theme')}
          onClick={toggleTheme}
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-800 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none"
        >
          {resolvedTheme === 'dark' ? (
            <Sun className="h-4 w-4" aria-hidden="true" />
          ) : (
            <Moon className="h-4 w-4" aria-hidden="true" />
          )}
        </button>
        <button
          type="button"
          title={t('topnav.settings')}
          aria-label={t('topnav.settings')}
          onClick={() => openSettings()}
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-800 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none"
        >
          <Settings className="h-4 w-4" aria-hidden="true" />
        </button>
        <AgentChip />
      </div>
    </header>
  );
}
