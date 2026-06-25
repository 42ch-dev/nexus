import { Moon, Sun } from 'lucide-react';

import { DaemonHealthIndicator } from '@/components/daemon-health-indicator';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/components/theme-provider';
import { useDesktopCapabilities } from '@/lib/client-context';

/**
 * App shell header. Shows the surface title, the daemon health indicator
 * (browser build only — the desktop build uses the persistent footer status
 * bar), and the token-driven theme toggle.
 */
export function Header({ title }: { title: string }) {
  const { theme, toggleTheme } = useTheme();
  const desktop = useDesktopCapabilities();
  return (
    <header className="flex h-14 items-center justify-between border-b border-gray-alpha-400 bg-background-100 px-4 md:px-6">
      <h1 className="text-heading-20 font-heading tracking-tight text-gray-1000">{title}</h1>
      <div className="flex items-center gap-3">
        {desktop ? null : <DaemonHealthIndicator />}
        <Button
          variant="tertiary"
          size="small"
          onClick={toggleTheme}
          aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
          title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
        >
          {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
      </div>
    </header>
  );
}
