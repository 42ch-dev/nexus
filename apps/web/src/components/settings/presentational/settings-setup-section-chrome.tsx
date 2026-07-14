import { Button } from '@42ch/nexus-ui';

export interface SettingsSetupSectionChromeProps {
  /** When false, renders the honest browser-only disabled state. */
  desktopAvailable?: boolean;
  /** Called when the desktop Re-run Setup CTA is clicked. */
  onReRunSetup?: () => void;
  /** Section title. Defaults to English "Setup". */
  title?: string;
  /** Section body helper. */
  helper?: string;
  /** Re-run Setup CTA label. */
  rerunLabel?: string;
  /** Browser-only helper paragraph. */
  browserOnlyHelper?: string;
  /** Tooltip shown on the disabled browser Re-run Setup button. */
  browserTooltip?: string;
  'data-testid'?: string;
}

/** Locked by settings-setup-section.md — section body helper (sentence case). */
const DEFAULT_SETUP_SECTION_HELPER =
  'Return to the first-run wizard to walk through setup steps again. Your workspace and agent choices are kept.';

const DEFAULT_SETUP_BROWSER_HELPER =
  'Re-run setup is available on the desktop app only.';

const DEFAULT_SETUP_BROWSER_TOOLTIP =
  'Open the Nexus desktop app to re-run setup.';

/**
 * Setup section body chrome — locked helper + Re-run Setup CTA.
 * Props-driven only; no internal Dialog state. The host owns the confirm
 * dialog via the `onReRunSetup` callback (settings-setup-section.md).
 *
 * `desktopAvailable` toggles honest browser-only copy vs the desktop CTA.
 */
export function SettingsSetupSectionChrome({
  desktopAvailable = true,
  onReRunSetup,
  title = 'Setup',
  helper = DEFAULT_SETUP_SECTION_HELPER,
  rerunLabel = 'Re-run',
  browserOnlyHelper = DEFAULT_SETUP_BROWSER_HELPER,
  browserTooltip = DEFAULT_SETUP_BROWSER_TOOLTIP,
  'data-testid': dataTestId,
}: SettingsSetupSectionChromeProps) {
  return (
    <div
      className="flex flex-col gap-6"
      data-testid={dataTestId}
      data-desktop={desktopAvailable ? 'true' : 'false'}
      id="setup"
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{title}</h3>
        <p className="text-copy-14 text-gray-900">{helper}</p>
      </div>

      {desktopAvailable ? (
        <div className="flex items-center gap-3">
          <Button
            type="button"
            variant="destructive"
            data-testid="settings-rerun-setup"
            onClick={() => onReRunSetup?.()}
          >
            {rerunLabel}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-3" data-testid="settings-setup-browser-only">
          <p className="text-copy-14 text-gray-700">{browserOnlyHelper}</p>
          <div className="flex items-center gap-3">
            <Button
              type="button"
              variant="secondary"
              disabled
              title={browserTooltip}
              data-testid="settings-rerun-setup"
            >
              {rerunLabel}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Static confirm-dialog chrome for visual acceptance — mirrors DialogContent
 * layout without Radix portal/aria-hidden (keeps Surfaces page a11y tree intact).
 */
export function SettingsSetupConfirmChromeStatic() {
  return (
    <div
      className="flex max-w-[560px] flex-col overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-modal"
      data-testid="settings-rerun-setup-confirm-chrome"
      role="group"
      aria-label="Re-run Setup confirm dialog chrome"
    >
      <div className="flex flex-col gap-1 p-6 pb-4">
        <p className="text-heading-20 font-heading tracking-tight text-gray-1000">
          Re-run Setup?
        </p>
        <p className="text-copy-14 text-gray-900">
          This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted.
        </p>
      </div>
      <div className="flex justify-end gap-3 px-6 pb-6">
        <Button type="button" variant="secondary" tabIndex={-1}>
          Cancel
        </Button>
        <Button type="button" variant="destructive" tabIndex={-1}>
          Re-run
        </Button>
      </div>
    </div>
  );
}
