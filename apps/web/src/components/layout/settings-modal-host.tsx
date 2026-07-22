import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { useSettingsModal } from '@/components/layout/settings-modal-context';
import { Dialog, DialogContent } from '@/components/ui/dialog';

export function SettingsModalHost({ children }: { children?: ReactNode }) {
  const { t } = useTranslation('shell');
  const { open, closeSettings } = useSettingsModal();

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) closeSettings();
      }}
    >
      <DialogContent
        title={t('settings.title')}
        className="!w-[80vw] !max-w-[80vw] !h-[80vh] !max-h-[80vh]"
      >
        <div className="flex-1 overflow-auto" data-testid="settings-modal-body">
          {children}
        </div>
      </DialogContent>
    </Dialog>
  );
}
