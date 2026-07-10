import { DaemonReadySplash } from '@web-setup/daemon-ready-splash';

/**
 * Studio Surfaces fixture for the daemon-ready splash.
 *
 * Imports the presentational App module via `@web-setup/daemon-ready-splash`.
 * Props-driven — no daemon IPC. Renders DESIGN.md §Launch & daemon status
 * waiting, error+retry, and reset-local-DB recovery variants.
 */
export function LaunchDaemonFixtures() {
  return (
    <div className="space-y-8">
      <div data-testid="daemon-ready-splash" className="border border-gray-alpha-300 rounded-card overflow-hidden">
        <DaemonReadySplash error={null} onRetry={() => {}} />
      </div>

      <div data-testid="daemon-ready-splash" className="border border-gray-alpha-300 rounded-card overflow-hidden">
        <DaemonReadySplash
          error="Could not reach the daemon after retrying."
          onRetry={() => {}}
        />
      </div>

      <div data-testid="daemon-ready-splash" className="border border-gray-alpha-300 rounded-card overflow-hidden">
        <DaemonReadySplash
          error="Could not reach the daemon after retrying."
          onRetry={() => {}}
          onResetLocalDatabase={() => {}}
        />
      </div>
    </div>
  );
}
