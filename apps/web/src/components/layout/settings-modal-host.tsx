/**
 * Single app-level Settings modal host — V1.131 P2.
 *
 * Owns the Radix dialog, ≥80vw×80vh desktop sizing, section frame, dirty
 * discard confirmation, and section content via the typed descriptor registry.
 * Routes and the Chronos titlebar gear share this host — no second Settings dialog.
 */

import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { useSettingsModal } from '@/components/layout/settings-modal-context';
import {
  DEFAULT_SETTINGS_SECTION,
  SETTINGS_SECTION_BY_ID,
  type SettingsSectionId,
} from '@/components/layout/settings-section-registry';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { SettingsSectionFrame } from '@/pages/settings/settings-section-frame';

function SettingsSectionBody({
  section,
  sectionHash,
}: {
  section: SettingsSectionId;
  sectionHash: string;
}) {
  useEffect(() => {
    if (!sectionHash) return;
    const id = sectionHash;
    // Scroll advanced anchors (connection / setup) into view after mount.
    const frame = requestAnimationFrame(() => {
      document.getElementById(id)?.scrollIntoView({ block: 'start' });
    });
    return () => cancelAnimationFrame(frame);
  }, [section, sectionHash]);

  const Content =
    SETTINGS_SECTION_BY_ID[section]?.Content ??
    SETTINGS_SECTION_BY_ID[DEFAULT_SETTINGS_SECTION].Content;

  return <Content />;
}

export function SettingsModalHost() {
  const { t } = useTranslation('settings');
  const {
    open,
    activeSection,
    sectionHash,
    selectSection,
    requestClose,
    discardConfirmOpen,
    confirmDiscard,
    cancelDiscard,
  } = useSettingsModal();

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) requestClose('escape');
        }}
      >
        <DialogContent
          title={t('title')}
          description={t('helper')}
          className="h-[80vh] min-h-[80vh] max-h-none w-[80vw] min-w-[80vw] max-w-none"
          bodyClassName="flex min-h-0 flex-1 flex-col overflow-hidden px-0 pb-0"
        >
          <div
            className="flex min-h-0 flex-1 flex-col"
            data-testid="settings-modal-body"
          >
            <SettingsSectionFrame
              activeSection={activeSection}
              onSelectSection={selectSection}
            >
              <SettingsSectionBody
                section={activeSection}
                sectionHash={sectionHash}
              />
            </SettingsSectionFrame>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog
        open={discardConfirmOpen}
        onOpenChange={(next) => {
          if (!next) cancelDiscard();
        }}
      >
        <DialogContent
          title={t('discard.title')}
          description={t('discard.body')}
        >
          <div
            className="flex justify-end gap-2"
            data-testid="settings-discard-confirm"
          >
            <Button
              type="button"
              variant="tertiary"
              onClick={cancelDiscard}
              data-testid="settings-discard-cancel"
            >
              {t('discard.cancel')}
            </Button>
            <Button
              type="button"
              variant="primary"
              onClick={confirmDiscard}
              data-testid="settings-discard-confirm-button"
            >
              {t('discard.confirm')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
