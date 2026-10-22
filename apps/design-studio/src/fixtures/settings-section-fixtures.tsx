import {
  Card,
  CardContent,
  Input,
  Select,
  SettingRow,
  SettingsSection,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<SettingsSection>` + `<SettingRow>`
 * (V1.150 P2 promotion).
 *
 * The promoted settings family: section chrome (native fieldset/legend) and
 * label/control/hint rows. The caller owns the controls and all copy;
 * `htmlFor` wires the promoted Label to the caller's control.
 */
export function SettingsSectionFixtures() {
  return (
    <div data-testid="settings-section-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <SettingsSection
          title="Appearance"
          description="Theme and density for this workspace."
        >
          <Card>
            <CardContent className="flex flex-col gap-4">
              <SettingRow
                label="Theme"
                htmlFor="settings-section-theme"
                hint="Follow system switches with the OS appearance."
              >
                <Select id="settings-section-theme" data-testid="settings-row-theme" defaultValue="system">
                  <option value="system">Follow system</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </Select>
              </SettingRow>
              <SettingRow
                label="Display name"
                htmlFor="settings-section-name"
                hint="Shown to other members of the workspace."
              >
                <Input
                  id="settings-section-name"
                  data-testid="settings-row-name"
                  defaultValue="Ada Lovelace"
                />
              </SettingRow>
            </CardContent>
          </Card>
        </SettingsSection>
        <p className="text-copy-13 text-gray-700 mt-6">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          SettingsSection + SettingRow — the settings family behind
          appearance/connection/workspace surfaces. Section chrome uses
          native fieldset/legend; rows wire the promoted Label to the
          caller's control via htmlFor and own the hint copy.
        </p>
      </div>
    </div>
  );
}
