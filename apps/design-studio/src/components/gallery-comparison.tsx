import { useCallback, useEffect, useId, useMemo, useRef, useState, type RefObject } from 'react';

import {
  buildStudioEmbedSrc,
  isStudioEmbedReadyMessage,
  type EmbeddedTheme,
} from '@/lib/studio-embed';

export type GalleryComparisonProps = {
  path: string;
  hash: string;
  resetKey: number;
  galleryLabel: string;
  onOpenCurrentGallery: () => void;
};

type FrameState = {
  ready: boolean;
  failed: boolean;
};

const READY_TIMEOUT_MS = 10_000;

function normalizeHash(hash: string): string {
  if (!hash) return '';
  return hash.startsWith('#') ? hash : `#${hash}`;
}

function FramePanel({
  headingId,
  title,
  theme,
  src,
  state,
  iframeRef,
  remountKey,
  onLoadError,
}: {
  headingId: string;
  title: string;
  theme: EmbeddedTheme;
  src: string;
  state: FrameState;
  iframeRef: RefObject<HTMLIFrameElement | null>;
  remountKey: number;
  onLoadError: () => void;
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-2">
      <h3 id={headingId} className="text-heading-16 font-semibold text-gray-1000">
        {title}
      </h3>
      <div className="relative h-[640px] w-full overflow-hidden rounded-card border border-gray-alpha-300 bg-background-200">
        {!state.ready && !state.failed ? (
          <p className="absolute inset-0 flex items-center justify-center px-4 text-center text-copy-14 text-gray-700">
            Loading {theme} gallery…
          </p>
        ) : null}
        {state.failed ? (
          <p className="absolute inset-0 flex items-center justify-center px-4 text-center text-copy-14 text-red-800">
            Loading failed — the embedded gallery did not become ready in time.
          </p>
        ) : null}
        <iframe
          key={remountKey}
          ref={iframeRef}
          title={title}
          aria-labelledby={headingId}
          src={src}
          className="h-full w-full border-0 bg-background-100"
          onError={onLoadError}
        />
      </div>
    </div>
  );
}

export function GalleryComparison({
  path,
  hash,
  resetKey,
  galleryLabel,
  onOpenCurrentGallery,
}: GalleryComparisonProps) {
  const lightRef = useRef<HTMLIFrameElement>(null);
  const darkRef = useRef<HTMLIFrameElement>(null);
  const timersRef = useRef<number[]>([]);
  const [lightState, setLightState] = useState<FrameState>({ ready: false, failed: false });
  const [darkState, setDarkState] = useState<FrameState>({ ready: false, failed: false });
  const [localResetKey, setLocalResetKey] = useState(0);
  const lightHeadingId = useId();
  const darkHeadingId = useId();

  const normalizedHash = normalizeHash(hash);
  const effectiveResetKey = resetKey + localResetKey;
  const lightFrameTitle = `Light — ${galleryLabel}`;
  const darkFrameTitle = `Dark — ${galleryLabel}`;

  const lightSrc = useMemo(
    () => buildStudioEmbedSrc(path, normalizedHash, 'light'),
    [path, normalizedHash, effectiveResetKey],
  );
  const darkSrc = useMemo(
    () => buildStudioEmbedSrc(path, normalizedHash, 'dark'),
    [path, normalizedHash, effectiveResetKey],
  );

  const clearTimers = useCallback(() => {
    timersRef.current.forEach((timer) => window.clearTimeout(timer));
    timersRef.current = [];
  }, []);

  const markFailed = useCallback((theme: EmbeddedTheme) => {
    if (theme === 'light') setLightState({ ready: false, failed: true });
    else setDarkState({ ready: false, failed: true });
  }, []);

  const resetFrames = useCallback(() => {
    clearTimers();
    setLightState({ ready: false, failed: false });
    setDarkState({ ready: false, failed: false });
    setLocalResetKey((value) => value + 1);
  }, [clearTimers]);

  useEffect(() => {
    resetFrames();
  }, [path, resetKey, resetFrames]);

  useEffect(() => {
    clearTimers();

    const scheduleTimeout = (theme: EmbeddedTheme) => {
      const timer = window.setTimeout(() => {
        if (theme === 'light') {
          setLightState((current) => (current.ready ? current : { ready: false, failed: true }));
        } else {
          setDarkState((current) => (current.ready ? current : { ready: false, failed: true }));
        }
      }, READY_TIMEOUT_MS);
      timersRef.current.push(timer);
    };

    scheduleTimeout('light');
    scheduleTimeout('dark');

    const handleMessage = (event: MessageEvent) => {
      if (event.origin !== window.location.origin) return;
      if (!isStudioEmbedReadyMessage(event.data)) return;

      const { theme, path: readyPath } = event.data;
      if (readyPath !== path) return;

      const frameWindow =
        theme === 'light' ? lightRef.current?.contentWindow : darkRef.current?.contentWindow;
      if (frameWindow !== event.source) return;

      if (theme === 'light') setLightState({ ready: true, failed: false });
      else setDarkState({ ready: true, failed: false });
    };

    window.addEventListener('message', handleMessage);
    return () => {
      window.removeEventListener('message', handleMessage);
      clearTimers();
    };
  }, [path, effectiveResetKey, clearTimers]);

  const anyFailed = lightState.failed || darkState.failed;

  return (
    <div className="space-y-4" data-testid="gallery-comparison">
      {anyFailed ? (
        <div
          role="alert"
          className="rounded-card border border-red-800/30 bg-red-100/40 px-4 py-3 text-copy-14 text-gray-1000"
        >
          <p className="font-medium">Comparison frame loading failed.</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              type="button"
              onClick={resetFrames}
              className="rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2 text-label-14 hover:bg-gray-alpha-100"
            >
              Retry
            </button>
            <button
              type="button"
              onClick={onOpenCurrentGallery}
              className="rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2 text-label-14 hover:bg-gray-alpha-100"
            >
              Open current gallery
            </button>
          </div>
        </div>
      ) : null}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <FramePanel
          headingId={lightHeadingId}
          title={lightFrameTitle}
          theme="light"
          src={lightSrc}
          state={lightState}
          iframeRef={lightRef}
          remountKey={effectiveResetKey}
          onLoadError={() => markFailed('light')}
        />
        <FramePanel
          headingId={darkHeadingId}
          title={darkFrameTitle}
          theme="dark"
          src={darkSrc}
          state={darkState}
          iframeRef={darkRef}
          remountKey={effectiveResetKey}
          onLoadError={() => markFailed('dark')}
        />
      </div>
    </div>
  );
}
