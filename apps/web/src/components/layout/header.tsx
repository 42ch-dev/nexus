import { Moon, Settings, Sun } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { DaemonHealthIndicator } from '@/components/daemon-health-indicator';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/components/theme-provider';
import { useDesktopCapabilities } from '@/lib/client-context';
import { cn } from '@/lib/utils';

/**
 * App shell header. Shows the surface title, the daemon health indicator
 * (browser build only — the desktop build uses the persistent footer status
 * bar), and the token-driven theme toggle.
 */
export function Header({ title }: { title: string }) {
  const { t } = useTranslation('shell');
  const { resolvedTheme, toggleTheme } = useTheme();
  const desktop = useDesktopCapabilities();
  const isDark = resolvedTheme === 'dark';
  const themeLabel = isDark
    ? t('theme.switchToLight')
    : t('theme.switchToDark');
  return (
    <header className="flex h-14 items-center justify-between border-b border-gray-alpha-400 bg-background-100 px-4 md:px-6">
      <div className="flex min-w-0 items-center gap-3">
        <NexusLogo className="h-7 lg:hidden" />
        <h1 className="truncate text-heading-20 font-heading tracking-tight text-gray-1000">{title}</h1>
      </div>
      <div className="flex items-center gap-2">
        {desktop ? null : <DaemonHealthIndicator />}
        <NavLink
          to="/settings"
          data-testid="header-settings-link"
          aria-label={t('nav.settings')}
          title={t('nav.settings')}
          className={({ isActive }) =>
            cn(
              'inline-flex h-8 w-8 items-center justify-center rounded-control text-gray-1000 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 motion-reduce:transition-none',
              isActive && 'bg-gray-alpha-100',
            )
          }
        >
          <Settings className="h-4 w-4" aria-hidden />
        </NavLink>
        <Button
          variant="tertiary"
          size="small"
          onClick={toggleTheme}
          aria-label={themeLabel}
          title={themeLabel}
        >
          {isDark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
      </div>
    </header>
  );
}
