/**
 * Settings Setup section — V1.103 P3 Re-run Setup (R1).
 *
 * Confirm → clear setup_completed marker only → navigate /setup.
 * No workspace / agent / DB wipe. Browser: honest desktop-only copy.
 */

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useToast } from '@/lib/use-toast';

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

export function SettingsSetupSection() {
  const desktop = useDesktopCapabilities();
  const { setCompleted } = useSetupCompleted();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [isConfirming, setIsConfirming] = useState(false);

  async function handleConfirm() {
    if (!desktop || isConfirming) return;
    setIsConfirming(true);
    try {
      // R1: await clear (IPC + context) then navigate. Failure stays on Settings.
      await setCompleted(false);
      setConfirmOpen(false);
      navigate('/setup', { replace: true });
    } catch (err) {
      const description = errorMessage(err) || 'Could not clear the setup marker.';
      toast({
        variant: 'error',
        title: 'Could not re-run setup',
        description,
      });
    } finally {
      setIsConfirming(false);
    }
  }

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-setup-section"
      data-desktop={desktop ? 'true' : 'false'}
      id="setup"
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Setup</h3>
        <p className="text-copy-14 text-gray-900">{SETUP_SECTION_HELPER}</p>
      </div>

      {desktop ? (
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

      <Dialog
        open={confirmOpen}
        onOpenChange={(open) => {
          // Ignore dismiss while IPC is in flight (Escape / overlay / X).
          if (isConfirming) return;
          setConfirmOpen(open);
        }}
      >
        <DialogContent title={SETUP_CONFIRM_TITLE} description={SETUP_CONFIRM_BODY}>
          <div
            className="flex justify-end gap-3"
            data-testid="settings-rerun-setup-confirm"
          >
            <Button
              type="button"
              variant="secondary"
              data-testid="settings-rerun-setup-cancel"
              disabled={isConfirming}
              onClick={() => setConfirmOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              data-testid="settings-rerun-setup-confirm-action"
              disabled={isConfirming}
              onClick={() => {
                void handleConfirm();
              }}
            >
              Re-run Setup
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
