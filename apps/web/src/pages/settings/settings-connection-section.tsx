/**
 * Settings Connection section — V1.103 P2.
 *
 * Hosts ConnectDaemonForm under SettingsShellLayout. Legacy `/connect`
 * permanently redirects here (C1).
 *
 * Author-facing copy: settings-connection-section.md.
 */

import { ConnectDaemonForm } from '@/components/settings/connect-daemon-form';

/** Locked by settings-connection-section.md — section body helper (sentence case). */
const CONNECTION_SECTION_HELPER =
  'Connect this app to a remote Nexus daemon. Your local daemon stays the default until you activate a remote connection.';

export function SettingsConnectionSection() {
  return (
    <div className="flex flex-col gap-6" data-testid="settings-connection-section">
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Connection</h3>
        <p className="text-copy-14 text-gray-900">{CONNECTION_SECTION_HELPER}</p>
      </div>
      <ConnectDaemonForm />
    </div>
  );
}
