/**
 * Settings Modules section — V1.131 P2 (`DF-V1130-COMPUTE-IN-SETTINGS`).
 *
 * Reuses {@link ModulesPageBody} query/detail implementation; no nested dialog.
 */
import { ModulesPageBody } from '@/pages/modules-page';

export function SettingsModulesSection() {
  return (
    <div data-testid="settings-modules-section">
      <ModulesPageBody />
    </div>
  );
}
