import { useState } from 'react';

import { Button } from '@42ch/nexus-ui';

import { Dialog, DialogContent } from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)

export interface SettingsSetupSectionChromeProps {
  /** When false, renders the honest browser-only disabled state. */
  desktopAvailable?: boolean;
  'data-testid'?: string;
}

/** Locked by settings-setup-section.md — section body helper (sentence case). */
const SETUP_SECTION_HELPER =
  'Return to the first-run wizard to walk through setup steps again. Your workspace and agent choices are kept.';

const SETUP_CONFIRM_TITLE = 'Re-run Setup?';

const SETUP_CONFIRM_BODY =
  'This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted.';

const SETUP_BROWSER_HELPER =
  'Re-run setup is available on the desktop app only.';

const SETUP_BROWSER_TOOLTIP =
  'Open the Nexus desktop app to re-run setup.';

/**
 * Setup section body chrome — locked helper + Re-run Setup CTA + confirm
 * dialog (settings-setup-section.md). Props-driven only; no App IPC.
 *
 * `desktopAvailable` toggles honest browser-only copy vs the desktop CTA.
 */
export function SettingsSetupSectionChrome({
  desktopAvailable = true,
  'data-testid': dataTestId,
}: SettingsSetupSectionChromeProps) {
  const [confirmOpen, setConfirmOpen] = useState(false);

  return (
    <div
      className="flex flex-col gap-6"
      data-testid={dataTestId}
      data-desktop={desktopAvailable ? 'true' : 'false'}
      id="setup"
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Setup</h3>
        <p className="text-copy-14 text-gray-900">{SETUP_SECTION_HELPER}</p>
      </div>

      {desktopAvailable ? (
        <div className="flex items-center gap-3">
          <Button
            type="button"
            variant="secondary"
            data-testid="settings-rerun-setup"
            onClick={() => setConfirmOpen(true)}
          >
            Re-run Setup
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-3" data-testid="settings-setup-browser-only">
          <p className="text-copy-14 text-gray-700">{SETUP_BROWSER_HELPER}</p>
          <div className="flex items-center gap-3">
            <Button
              type="button"
              variant="secondary"
              disabled
              title={SETUP_BROWSER_TOOLTIP}
              data-testid="settings-rerun-setup"
            >
              Re-run Setup
            </Button>
          </div>
        </div>
      )}

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent title={SETUP_CONFIRM_TITLE} description={SETUP_CONFIRM_BODY}>
          <div className="flex justify-end gap-3" data-testid="settings-rerun-setup-confirm">
            <Button
              type="button"
              variant="secondary"
              data-testid="settings-rerun-setup-cancel"
              onClick={() => setConfirmOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              data-testid="settings-rerun-setup-confirm-action"
              onClick={() => setConfirmOpen(false)}
            >
              Re-run Setup
            </Button>
          </div>
        </DialogContent>
      </Dialog>
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
          {SETUP_CONFIRM_TITLE}
        </p>
        <p className="text-copy-14 text-gray-900">{SETUP_CONFIRM_BODY}</p>
      </div>
      <div className="flex justify-end gap-3 px-6 pb-6">
        <Button type="button" variant="secondary" tabIndex={-1}>
          Cancel
        </Button>
        <Button type="button" variant="destructive" tabIndex={-1}>
          Re-run Setup
        </Button>
      </div>
    </div>
  );
}
