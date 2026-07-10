import { RefreshCw } from 'lucide-react';
import { Button, cn } from '@42ch/nexus-ui';

/**
 * Studio Surfaces fixture for the degraded-daemon MainBanner.
 *
 * **Composition-only** — replicates `apps/web/src/components/layout/main-banner.tsx`
 * visuals from props. Does NOT import the App banner (it uses daemon/desktop
 * hooks). No IPC, no live status.
 */
export function MainBannerFixtures() {
  const variants = [
    {
      state: 'starting' as const,
      title: 'Daemon starting…',
      description: 'Nexus is starting the local daemon.',
    },
    {
      state: 'degraded' as const,
      title: 'Daemon reconnecting',
      description: 'Nexus is retrying the local daemon connection.',
    },
    {
      state: 'stopped' as const,
      title: 'Daemon stopped',
      description: 'Restart the daemon to use local workspace features.',
    },
    {
      state: 'error' as const,
      title: 'Port unavailable',
      description: 'Port 8420 is already in use by another process.',
    },
  ];

  return (
    <div className="space-y-6">
      {variants.map(({ state, title, description }) => (
        <div
          key={state}
          data-testid={`main-banner-fixture-${state}`}
          className="border-b border-gray-alpha-400 bg-amber-700/10 px-6 py-3"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-col">
              <span className="text-copy-14 font-semibold text-gray-1000">{title}</span>
              <span className="text-copy-13 text-gray-700">{description}</span>
            </div>
            <Button type="button" variant="primary" size="small">
              <RefreshCw className={cn('h-4 w-4')} aria-hidden />
              Restart Daemon
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}
