/**
 * Settings Advanced section — V1.106 P2.
 *
 * Stacks Connection and Setup sections on a single page. Hash anchors
 * enable deep links from legacy `/settings/connection`, `/settings/setup`,
 * and `/connect` redirects.
 */

import { SettingsConnectionSection } from '@/pages/settings/settings-connection-section';
import { SettingsSetupSection } from '@/pages/settings/settings-setup-section';

export function SettingsAdvancedSection() {
  return (
    <div className="flex flex-col gap-10" data-testid="settings-advanced-section">
      <SettingsConnectionSection />
      <SettingsSetupSection />
    </div>
  );
}
