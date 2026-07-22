import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

/** Left inset reserved for macOS native traffic lights (Overlay titlebar). */
export const CHRONOS_TITLEBAR_DESKTOP_INSET_PX = 72;

export interface ChronosTitlebarChromeProps {
  title: string;
  /** When true, title labels use cyan (dark shell); otherwise white (light shell). */
  isDark: boolean;
  /** Reserve native traffic-light safe inset and enable desktop drag spacer. */
  desktopSafeInset?: boolean;
  logo?: ReactNode;
  settingsControl?: ReactNode;
  themeToggle?: ReactNode;
  healthIndicator?: ReactNode;
  /** Desktop overlay: double-click empty paint (safe inset + drag spacer) to maximize. */
  onEmptyPaintDoubleClick?: () => void;
  'data-testid'?: string;
}

/**
 * Presentational Chronos titlebar — full-width ink strip (DESIGN.md §desktop-window-chrome).
 *
 * Props-driven slots only: no routing, theme hooks, or daemon clients.
 * `data-tauri-drag-region` is applied only to empty paint (safe inset + flex spacer),
 * never to interactive logo, title, gear, theme, or health controls.
 */
export function ChronosTitlebarChrome({
  title,
  isDark,
  desktopSafeInset = false,
  logo,
  settingsControl,
  themeToggle,
  healthIndicator,
  onEmptyPaintDoubleClick,
  'data-testid': dataTestId = 'chronos-titlebar',
}: ChronosTitlebarChromeProps) {
  const labelClass = isDark ? 'text-brand-cyan' : 'text-white';

  return (
    <header
      data-testid={dataTestId}
      className="flex h-14 shrink-0 items-center bg-brand-deep-blue"
    >
      {desktopSafeInset ? (
        <div
          className="shrink-0"
          style={{ width: CHRONOS_TITLEBAR_DESKTOP_INSET_PX }}
          data-tauri-drag-region
          data-testid="chronos-titlebar-desktop-inset"
          onDoubleClick={onEmptyPaintDoubleClick}
          aria-hidden
        />
      ) : null}

      {logo ? (
        <div
          className="flex shrink-0 items-center px-3"
          data-testid="chronos-titlebar-logo-slot"
        >
          {logo}
        </div>
      ) : null}

      <h1
        className={cn(
          'shrink-0 truncate px-2 text-heading-20 font-heading tracking-tight',
          labelClass,
        )}
        data-testid="chronos-titlebar-title"
      >
        {title}
      </h1>

      <div
        className="min-w-4 flex-1"
        data-tauri-drag-region={desktopSafeInset ? true : undefined}
        data-testid="chronos-titlebar-drag-spacer"
        onDoubleClick={onEmptyPaintDoubleClick}
        aria-hidden
      />

      <div
        className="flex shrink-0 items-center gap-2 px-4"
        data-testid="chronos-titlebar-controls"
      >
        {healthIndicator}
        {settingsControl}
        {themeToggle}
      </div>
    </header>
  );
}
