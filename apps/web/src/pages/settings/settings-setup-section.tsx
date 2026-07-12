/**
 * Settings Setup section — V1.103 P3 Re-run Setup (R1).
 *
 * Confirm → clear setup_completed marker only → navigate /setup.
 * No workspace / agent / DB wipe. Browser: honest desktop-only copy.
 */

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { SettingsSetupSectionChrome } from '@/components/settings/presentational/settings-setup-section-chrome';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useToast } from '@/lib/use-toast';

export function SettingsSetupSection() {
  const { t } = useTranslation('settings');
  const { t: commonT } = useTranslation('common');
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
      const description = errorMessage(err) || t('setup.couldNotClear');
      toast({
        variant: 'error',
        title: t('setup.couldNotReRun'),
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
        title={t('setup.title')}
        helper={t('setup.helper')}
        rerunLabel={t('setup.rerun')}
        browserOnlyHelper={t('setup.browserOnly')}
        browserTooltip={t('setup.browserTooltip')}
      />

      <Dialog
        open={confirmOpen}
        onOpenChange={(open) => {
          // Ignore dismiss while IPC is in flight (Escape / overlay / X).
          if (isConfirming) return;
          setConfirmOpen(open);
        }}
      >
        <DialogContent title={t('setup.confirmTitle')} description={t('setup.confirmBody')}>
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
              {commonT('action.cancel')}
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
              {t('setup.rerun')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
