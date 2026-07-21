import { DaemonReadySplash } from '@web-setup/daemon-ready-splash';

/**
 * Studio Surfaces fixture for the daemon-ready splash.
 *
 * Imports the presentational App module via `@web-setup/daemon-ready-splash`.
 * Props-driven — no daemon IPC. Renders DESIGN.md §Launch & daemon status
 * waiting, error+retry (V1.129 P1 promoted `<TransportErrorBlock>`), and
 * reset-local-DB recovery variants.
 *
 * V1.129 P1: error state now consumes the promoted `<TransportErrorBlock>`
 * (kind-classified headline + body + Retry CTA). Reset Local Database stays
 * a separate affordance composed alongside the primitive. The launch gate
 * omits `onOpenSettings` — the gate runs before the router mounts, so there
 * is no settings route to deep-link to.
 */
export function LaunchDaemonFixtures() {
  return (
    <div className="space-y-8" data-testid="launch-daemon-fixtures">
      <div
        data-testid="daemon-ready-splash"
        className="border border-gray-alpha-300 rounded-card overflow-hidden"
      >
        <DaemonReadySplash errorKind={null} onRetry={() => {}} />
      </div>

      <div
        data-testid="daemon-ready-splash"
        className="border border-gray-alpha-300 rounded-card overflow-hidden"
      >
        <DaemonReadySplash
          errorKind="daemon_down"
          errorMessage="Daemon is not responding."
          onRetry={() => {}}
        />
      </div>

      <div
        data-testid="daemon-ready-splash"
        className="border border-gray-alpha-300 rounded-card overflow-hidden"
      >
        <DaemonReadySplash
          errorKind="timeout"
          errorMessage="Daemon is taking longer than expected to start."
          onRetry={() => {}}
          onResetLocalDatabase={() => {}}
        />
      </div>
    </div>
  );
}
