import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { useLocation, useNavigate } from 'react-router';

import { GalleryComparison } from '@/components/gallery-comparison';
import { SectionIndex } from '@/components/section-index';
import {
  focusGalleryHeading,
  getGalleryEntries,
  getGalleryLabel,
  type GalleryEntry,
} from '@/lib/gallery-index';

type GalleryShellProps = {
  children: ReactNode;
};

export function GalleryShell({ children }: GalleryShellProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const announceId = useId();
  const [compareEnabled, setCompareEnabled] = useState(false);
  const [resetKey, setResetKey] = useState(0);
  const [selectionAnnouncement, setSelectionAnnouncement] = useState('');

  const pathname = location.pathname;
  const hash = location.hash;
  const entries = getGalleryEntries(pathname);
  const cancelFocusRef = useRef<(() => void) | null>(null);

  const scheduleHeadingFocus = useCallback((id: string) => {
    cancelFocusRef.current?.();
    cancelFocusRef.current = focusGalleryHeading(id);
  }, []);
  // Deep links and Back/Forward focus the hash heading once the matched route
  // mounts. Pair mode never searches the parent document for fixture IDs —
  // the announcement and frame URL hash updates carry the selection there.
  useEffect(() => {
    const id = hash.startsWith('#') ? hash.slice(1) : hash;
    if (!id || compareEnabled) return;
    scheduleHeadingFocus(id);
    return () => {
      cancelFocusRef.current?.();
      cancelFocusRef.current = null;
    };
  }, [pathname, hash, compareEnabled, scheduleHeadingFocus]);

  function handleNavigate(entry: GalleryEntry) {
    const target = `${entry.path}#${entry.id}`;
    navigate(target);
    setSelectionAnnouncement(`Selected ${entry.label}.`);
    if (!compareEnabled) scheduleHeadingFocus(entry.id);
  }

  function handleCompareToggle() {
    setCompareEnabled((enabled) => {
      if (enabled) return false;
      setResetKey((value) => value + 1);
      return true;
    });
  }

  return (
    <>
      <div className="max-w-6xl mx-auto px-4 pt-4">
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <button
            type="button"
            aria-pressed={compareEnabled}
            onClick={handleCompareToggle}
            className="rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
          >
            {compareEnabled ? 'Exit light/dark compare' : 'Compare light/dark'}
          </button>
          {compareEnabled ? (
            <button
              type="button"
              onClick={() => setResetKey((value) => value + 1)}
              className="rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            >
              Reset comparison
            </button>
          ) : null}
        </div>

        <SectionIndex entries={entries} onNavigate={handleNavigate} />

        <p id={announceId} className="sr-only" aria-live="polite">
          {selectionAnnouncement}
        </p>
      </div>

      {compareEnabled ? (
        <div className="max-w-6xl mx-auto px-4 pb-8">
          <GalleryComparison
            path={pathname}
            hash={hash}
            resetKey={resetKey}
            galleryLabel={getGalleryLabel(pathname)}
            onOpenCurrentGallery={() => setCompareEnabled(false)}
          />
        </div>
      ) : (
        children
      )}
    </>
  );
}
