import { type ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';

import { cn } from '../lib/cn';

/**
 * SettingsLayout — DESIGN.md §Component Primitives (settings-layout).
 *
 * V1.150 P2 — L2 pattern component: the settings section nav rail + outlet
 * region promoted from the app frame (`settings-section-frame.tsx`).
 * Caller-owned section registry (labels/icons), active selection, and
 * navigation callback; the typed registry, URL aliasing, and section content
 * renderers stay app-side.
 */
export interface SettingsLayoutSection {
  id: string;
  /** Caller-owned (i18n resolution stays app-side). */
  label: string;
  icon: LucideIcon;
}

/** Stable default for `hiddenIds` — avoids a fresh array allocation per render. */
const NO_HIDDEN_IDS: readonly string[] = [];

export interface SettingsLayoutProps {
  /** Replaces the app-side SETTINGS_SECTION_DESCRIPTORS import. */
  sections: readonly SettingsLayoutSection[];
  activeId: string;
  onNavigate: (id: string) => void;
  /** Entrance hide-table (AR-15 precedent). */
  hiddenIds?: readonly string[];
  /** Nav aria-label copy (replaces useTranslation). */
  navLabel: string;
  /** Optional page-chrome heading (replaces showPageChrome mode). */
  title?: string;
  /** Optional page-chrome helper copy. */
  helper?: string;
  /** Section outlet content. */
  children: ReactNode;
  className?: string;
}

export function SettingsLayout({
  sections,
  activeId,
  onNavigate,
  hiddenIds = NO_HIDDEN_IDS,
  navLabel,
  title,
  helper,
  children,
  className,
}: SettingsLayoutProps) {
  return (
    <div
      className={cn('flex min-h-0 flex-1 flex-col gap-4', className)}
      data-testid="settings-shell"
    >
      {title ? (
        <div className="flex flex-col gap-2 px-6 pt-2">
          <h2 className="text-heading-24 font-heading text-gray-1000">{title}</h2>
          {helper && <p className="text-copy-14 text-gray-900">{helper}</p>}
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1 flex-col gap-4 lg:flex-row lg:gap-0">
        <nav
          aria-label={navLabel}
          className="flex flex-row flex-wrap gap-1 border-b border-gray-alpha-200 px-3 py-2 lg:w-44 lg:shrink-0 lg:flex-col lg:flex-nowrap lg:gap-0.5 lg:border-b-0 lg:border-r"
          data-testid="settings-section-nav"
        >
          {sections
            .filter(({ id }) => !hiddenIds.includes(id))
            .map(({ id, label, icon: Icon }) => {
              const active = activeId === id;
              return (
                <button
                  key={id}
                  type="button"
                  data-testid={`settings-section-nav-${id}`}
                  aria-current={active ? 'page' : undefined}
                  onClick={() => onNavigate(id)}
                  className={cn(
                    'inline-flex items-center gap-2 rounded-control px-3 py-2 text-left text-label-14 font-medium',
                    'transition-colors duration-state ease-standard',
                    active
                      ? 'bg-gray-alpha-100 text-gray-1000'
                      : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
                  )}
                >
                  <Icon className="size-4 shrink-0" aria-hidden="true" />
                  <span>{label}</span>
                </button>
              );
            })}
        </nav>

        <div
          className="min-h-0 min-w-0 flex-1 overflow-y-auto px-6 py-2"
          data-testid="settings-shell-outlet"
        >
          {children}
        </div>
      </div>
    </div>
  );
}
