/**
 * Settings Advanced section — V1.106 P2.
 *
 * Stacks Connection and Setup sections on a single page. Hash anchors
 * enable deep links from legacy `/settings/connection`, `/settings/setup`,
 * and `/connect` redirects.
 */

import { useFingerprintGateState } from '@/lib/client-context';
import { SettingsConnectionSection } from '@/pages/settings/settings-connection-section';
import { SettingsSetupSection } from '@/pages/settings/settings-setup-section';

export function SettingsAdvancedSection() {
  const gate = useFingerprintGateState();
  const isMismatch = gate?.status === 'mismatch';

  return (
    <div className="flex flex-col gap-10" data-testid="settings-advanced-section">
      <SettingsConnectionSection />
      {!isMismatch && <SettingsSetupSection />}
    </div>
  );
}
