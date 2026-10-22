/**
 * Creator profile switcher — topnav identity control (IA spec §3.1).
 *
 * Dropdown lists creators from the existing surface (`GET /v1/daemon/creators`),
 * shows ✓ on the active row, appends the § Copy trailing item (`＋ 新建 Creator…`),
 * and renders `SOUL {n}%` only when the surface supplies a maturity value —
 * silently omitted otherwise (no fake number, no error state; the wire
 * `CreatorInfo` carries no such field today).
 *
 * Switching writes through `setActiveCreator`, updates `ActiveCreatorContext`,
 * and invalidates queries so creator-scoped content re-renders (assignment T2).
 */
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';

import { useCreators } from '@/api/queries';
import {
  useActiveCreatorId,
  useSetActiveCreatorId,
} from '@/lib/active-creator-context';
import { useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import { cn } from '@/lib/utils';

/** SOUL maturity is display-only and not yet on the wire contract (IA O3). */
function soulMaturityOf(creator: unknown): number | undefined {
  if (creator === null || typeof creator !== 'object' || !('soul_maturity' in creator)) {
    return undefined;
  }
  const value = creator.soul_maturity;
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function ProfileSwitcher() {
  const { t } = useTranslation('shell');
  const { t: commonT } = useTranslation('common');
  const creators = useCreators({ limit: 100 });
  const activeCreatorId = useActiveCreatorId();
  const setActiveCreatorId = useSetActiveCreatorId();
  const client = useNexusClient();
  const queryClient = useQueryClient();
  const { toast } = useToast();
  const [open, setOpen] = useState(false);
  const [switching, setSwitching] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const items = creators.data?.items ?? [];
  const active = items.find((c) => c.creator_id === activeCreatorId) ?? null;

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') setOpen(false);
    }
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  async function handleSelect(creatorId: string) {
    if (creatorId === activeCreatorId || switching) {
      setOpen(false);
      return;
    }
    setSwitching(true);
    try {
      await client.setActiveCreator({ creator_id: creatorId });
      setActiveCreatorId(creatorId);
      // Nearly every surface is creator-scoped — refetch active queries.
      void queryClient.invalidateQueries();
    } catch (error) {
      toast({
        variant: 'error',
        title: commonT('error.couldNotSwitchCreator'),
        description: errorMessage(error),
      });
    } finally {
      setSwitching(false);
      setOpen(false);
    }
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        data-testid="profile-switcher"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((prev) => !prev)}
        className="flex h-8 items-center gap-2 rounded-control px-2.5 text-label-14 text-gray-1000 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none"
      >
        <span
          aria-hidden="true"
          className="flex h-[18px] w-[18px] items-center justify-center rounded-[5px] bg-blue-700 text-[10px] font-bold text-gray-100"
        >
          {(active?.display_name ?? '?').trim().charAt(0).toUpperCase() || '?'}
        </span>
        <span className="max-w-40 truncate">{active?.display_name ?? activeCreatorId ?? '—'}</span>
      </button>

      {open && (
        <div
          role="menu"
          data-testid="profile-switcher-menu"
          className="absolute left-0 top-[38px] z-50 min-w-[220px] rounded-popover border border-gray-alpha-400 bg-background-100 p-1.5 shadow-elevation-3"
        >
          <div className="px-2.5 pb-1 pt-1.5 text-label-12 text-gray-600">
            {t('topnav.profileSection')}
          </div>
          {items.map((creator) => {
            const isActive = creator.creator_id === activeCreatorId;
            const maturity = soulMaturityOf(creator);
            return (
              <button
                key={creator.creator_id}
                type="button"
                role="menuitem"
                disabled={switching}
                onClick={() => void handleSelect(creator.creator_id)}
                className={cn(
                  'flex w-full items-center gap-2.5 rounded-control px-2.5 py-2 text-left text-copy-13 text-gray-1000 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none',
                  isActive && 'bg-gray-alpha-100',
                )}
              >
                <span
                  aria-hidden="true"
                  className="flex h-[18px] w-[18px] items-center justify-center rounded-[5px] bg-blue-700 text-[10px] font-bold text-gray-100"
                >
                  {creator.display_name.trim().charAt(0).toUpperCase() || '?'}
                </span>
                <span className="flex-1 truncate">{creator.display_name}</span>
                {maturity !== undefined && (
                  <span className="text-label-12 text-gray-600" data-testid="soul-maturity">
                    SOUL {maturity}%
                  </span>
                )}
                {isActive && (
                  <span className="text-blue-700" aria-label={t('topnav.activeMarker')}>
                    ✓
                  </span>
                )}
              </button>
            );
          })}
          <hr className="mx-1 my-1.5 border-gray-alpha-400" />
          <button
            type="button"
            role="menuitem"
            data-testid="profile-switcher-new"
            onClick={() => setOpen(false)}
            className="flex w-full items-center gap-2.5 rounded-control px-2.5 py-2 text-left text-copy-13 text-gray-800 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 motion-reduce:transition-none"
          >
            <span aria-hidden="true">＋</span>
            <span>{t('topnav.newCreator')}</span>
          </button>
        </div>
      )}
    </div>
  );
}
