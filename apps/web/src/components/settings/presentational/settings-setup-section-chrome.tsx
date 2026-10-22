import { Button } from '@42ch/nexus-ui';

export interface SettingsSetupSectionChromeProps {
  /** Called when the Re-run Setup CTA is clicked. */
  onReRunSetup?: () => void;
  /** Section title. Defaults to English "Setup". */
  title?: string;
  /** Section body helper. */
  helper?: string;
  /** Re-run Setup CTA label. */
  rerunLabel?: string;
  'data-testid'?: string;
}

const DEFAULT_SETUP_SECTION_HELPER =
  'Re-pick your identity, AI partner, and default workspace; created worlds and works are untouched.';

/**
 * Setup section chrome — helper + Re-run Setup CTA (v1219-onboarding T4).
 *
 * Re-running setup is a plain navigation to the always-reachable wizard route
 * (compass D21): nothing is cleared and no data is deleted, so the CTA is not
 * destructive and is available in the browser build too (access spec §6.3).
 */
export function SettingsSetupSectionChrome({
  onReRunSetup,
  title = 'Setup',
  helper = DEFAULT_SETUP_SECTION_HELPER,
  rerunLabel = 'Re-run setup',
  'data-testid': dataTestId,
}: SettingsSetupSectionChromeProps) {
  return (
    <div className="flex flex-col gap-6" data-testid={dataTestId} id="setup">
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{title}</h3>
        <p className="text-copy-14 text-gray-900">{helper}</p>
      </div>

      <div className="flex items-center gap-3">
        <Button
          type="button"
          variant="secondary"
          data-testid="settings-rerun-setup"
          onClick={() => onReRunSetup?.()}
        >
          {rerunLabel}
        </Button>
      </div>
    </div>
  );
}
