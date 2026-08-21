import { Moon, Settings, Sun } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { NexusInkLogo } from '@/components/brand/nexus-ink-logo';
import { DaemonHealthIndicator } from '@/components/daemon-health-indicator';
import { ChronosTitlebarChrome } from '@/components/layout/presentational/chronos-titlebar-chrome';
import { firstSettingsSectionFor } from '@/components/layout/entrance-registry';
import { useSettingsModal } from '@/components/layout/settings-modal-context';
import { useEntrance } from '@/lib/entrance-context';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/components/theme-provider';
import { useDesktopCapabilities } from '@/lib/client-context';
import { cn } from '@/lib/utils';

export interface ChronosTitlebarProps {
  title: string;
}

/**
 * App shell Chronos titlebar — wires route title, ink logo, Settings gear,
 * theme toggle, and daemon health into {@link ChronosTitlebarChrome}.
 */
export function ChronosTitlebar({ title }: ChronosTitlebarProps) {
  const { t } = useTranslation('shell');
  const { resolvedTheme, toggleTheme } = useTheme();
  const desktop = useDesktopCapabilities();
  const { openSettings } = useSettingsModal();
  const { entrance } = useEntrance();
  // W-2 (plan QC): the gear opens the first section VISIBLE on this entrance —
  // Create must not land on the develop-only `agent` default (the guard would
  // bounce the whole app); Develop keeps the full Control Room default.
  const settingsSection = firstSettingsSectionFor(entrance);
  const isDark = resolvedTheme === 'dark';
  const themeLabel = isDark
    ? t('theme.switchToLight')
    : t('theme.switchToDark');

  const inkControlClass = cn(
    'inline-flex h-8 w-8 items-center justify-center rounded-control transition-colors duration-state ease-standard',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-cyan focus-visible:ring-offset-2 focus-visible:ring-offset-brand-deep-blue',
    'motion-reduce:transition-none',
    isDark
      ? 'text-brand-cyan hover:bg-white/10'
      : 'text-white hover:bg-white/10',
  );

  const handleEmptyPaintDoubleClick = () => {
    void desktop?.toggleMaximizeWindow();
  };

  return (
    <ChronosTitlebarChrome
      title={title}
      isDark={isDark}
      desktopSafeInset={desktop !== null}
      onEmptyPaintDoubleClick={desktop ? handleEmptyPaintDoubleClick : undefined}
      logo={<NexusInkLogo />}
      healthIndicator={desktop ? null : <DaemonHealthIndicator />}
      settingsControl={
        <button
          type="button"
          data-testid="chronos-titlebar-settings-gear"
          aria-label={t('settings.title')}
          title={t('settings.title')}
          className={inkControlClass}
          onClick={(event) => openSettings(settingsSection, event.currentTarget)}
        >
          <Settings className="h-4 w-4" aria-hidden />
        </button>
      }
      themeToggle={
        <Button
          variant="tertiary"
          size="small"
          onClick={toggleTheme}
          aria-label={themeLabel}
          title={themeLabel}
          className={cn(
            'border-transparent bg-transparent shadow-none',
            isDark
              ? 'text-brand-cyan hover:bg-white/10'
              : 'text-white hover:bg-white/10',
          )}
        >
          {isDark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
      }
    />
  );
}
