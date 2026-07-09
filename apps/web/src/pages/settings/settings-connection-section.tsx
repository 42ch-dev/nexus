/**
 * Settings Connection section — V1.103 P0.
 *
 * Hosts the existing ConnectDaemonPage so `/connect` → `/settings/connection`
 * keeps TOFU / fingerprint recovery reachable. P2 extracts ConnectDaemonForm
 * and removes the page wrapper.
 */

import { ConnectDaemonPage } from '@/pages/connect-daemon-page';

export function SettingsConnectionSection() {
  return (
    <div data-testid="settings-connection-section">
      <ConnectDaemonPage />
    </div>
  );
}
