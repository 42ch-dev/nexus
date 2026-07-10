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
import { SettingsSetupSectionChrome } from '@/components/settings/presentational/settings-setup-section-chrome';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useToast } from '@/lib/use-toast';

const SETUP_CONFIRM_TITLE = 'Re-run Setup?';

const SETUP_CONFIRM_BODY =
  'This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted.';

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
    <>
      <SettingsSetupSectionChrome
        data-testid="settings-setup-section"
        desktopAvailable={Boolean(desktop)}
        onReRunSetup={() => setConfirmOpen(true)}
      />

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
    </>
  );
}
