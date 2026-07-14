import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useCreators, useCreateCreator } from '@/api/queries';
import { useActiveCreatorId, useSetActiveCreatorId } from '@/lib/active-creator-context';
import { useDesktopCapabilities } from '@/lib/client-context';
import { useToast } from '@/lib/use-toast';
import { errorMessage } from '@/lib/error-message';
import { FooterProfilesChrome } from './presentational/footer-profiles-chrome';

/**
 * Sidebar footer profile switcher — Slack/Chrome-style avatar row.
 *
 * Thin wrapper around {@link FooterProfilesChrome}: owns the creator query,
 * active-creator context, and the create-creator dialog. The chrome owns the
 * presentational markup and data-testid SSOT.
 *
 * Desktop build: selecting a different Profile invokes the Tauri
 * `switch_active_creator` command, mirrors the target workspace path, and
 * refreshes the cached workspace root so the footer can show restart-honesty
 * copy when the new path differs from the running daemon's startup-captured root
 * (V1.104 honesty pattern).
 *
 * Browser build: the switch is a no-op with an honest desktop-only banner; no
 * fake Profile switch is performed.
 */
export function FooterProfiles() {
  const { t } = useTranslation('shell');
  const { t: commonT } = useTranslation('common');
  const creators = useCreators();
  const activeCreatorId = useActiveCreatorId();
  const setActiveCreatorId = useSetActiveCreatorId();
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const [createOpen, setCreateOpen] = useState(false);
  const [focusIndex, setFocusIndex] = useState(0);
  const [switching, setSwitching] = useState(false);
  const [restartHonest, setRestartHonest] = useState(false);
  const [browserNotice, setBrowserNotice] = useState(false);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const items = creators.data?.items ?? [];
  const total = items.length + 1; // last slot is the "Add creator" button

  useEffect(() => {
    setFocusIndex((prev) => Math.min(prev, Math.max(total - 1, 0)));
  }, [total]);

  function focusAt(index: number) {
    const next = Math.max(0, Math.min(total - 1, index));
    itemRefs.current[next]?.focus();
    setFocusIndex(next);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    switch (event.key) {
      case 'ArrowRight':
        event.preventDefault();
        focusAt(focusIndex + 1);
        break;
      case 'ArrowLeft':
        event.preventDefault();
        focusAt(focusIndex - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusAt(0);
        break;
      case 'End':
        event.preventDefault();
        focusAt(total - 1);
        break;
      case 'Escape':
        event.preventDefault();
        itemRefs.current[focusIndex]?.blur();
        break;
      default:
        break;
    }
  }

  async function handleSelect(id: string) {
    if (id === activeCreatorId || switching) return;
    if (items.length <= 1) return;

    if (!desktop) {
      setBrowserNotice(true);
      setRestartHonest(false);
      return;
    }

    setSwitching(true);
    setRestartHonest(false);
    setBrowserNotice(false);

    try {
      const newPath = await desktop.switchActiveCreator(id);
      const cachedRoot = await desktop.getWorkspaceRoot();
      setActiveCreatorId(id);
      if (newPath !== cachedRoot) {
        setRestartHonest(true);
      }
    } catch (err) {
      toast({
        variant: 'error',
        title: commonT('error.couldNotSwitchCreator'),
        description: errorMessage(err) || commonT('toast.actionFailed'),
      });
    } finally {
      setSwitching(false);
    }
  }

  const profiles = items.map((creator) => ({
    id: creator.creator_id,
    displayName: creator.display_name,
    active: creator.creator_id === activeCreatorId,
  }));

  return (
    <>
      <FooterProfilesChrome
        sectionLabel={t('profile.sectionLabel')}
        addButtonLabel={t('profile.addButtonLabel')}
        profiles={profiles}
        focusIndex={focusIndex}
        onSelect={handleSelect}
        onAdd={() => setCreateOpen(true)}
        onFocus={setFocusIndex}
        onKeyDown={handleKeyDown}
        onItemRef={(index, el) => {
          itemRefs.current[index] = el;
        }}
        onAddRef={(el) => {
          itemRefs.current[items.length] = el;
        }}
      />

      {browserNotice && (
        <div
          className="rounded-control border border-gray-alpha-400 bg-background-200 p-4 space-y-1"
          data-testid="footer-profile-browser-notice"
        >
          <p className="text-copy-14 text-gray-900">
            {t('profile.switchBrowserOnly')}
          </p>
        </div>
      )}

      {restartHonest && (
        <div
          className="rounded-control border border-gray-alpha-400 bg-background-200 p-4 space-y-1"
          data-testid="footer-profile-switch-honesty"
        >
          <p className="text-copy-14 text-gray-900">{t('profile.switchedTitle')}</p>
          <p className="text-copy-13 text-gray-700">{t('profile.switchedDescription')}</p>
        </div>
      )}

      <CreateCreatorDialog open={createOpen} onOpenChange={setCreateOpen} />
    </>
  );
}

function CreateCreatorDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation('shell');
  const create = useCreateCreator();
  const [displayName, setDisplayName] = useState('');

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!displayName.trim()) return;
    create.mutate(
      { display_name: displayName.trim() },
      {
        onSuccess: () => {
          setDisplayName('');
          onOpenChange(false);
        },
      },
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-md"
        title={t('profile.addDialogTitle')}
        description={t('profile.addDialogDescription')}
      >
        <form onSubmit={submit}>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="creator-name">{t('profile.displayNameLabel')}</Label>
              <Input
                id="creator-name"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                placeholder={t('profile.displayNamePlaceholder')}
                autoFocus
              />
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="tertiary"
              onClick={() => onOpenChange(false)}
              disabled={create.isPending}
            >
              {t('profile.cancel')}
            </Button>
            <Button type="submit" variant="primary" disabled={!displayName.trim() || create.isPending}>
              {create.isPending ? t('profile.creating') : t('profile.create')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
