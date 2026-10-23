import { useEffect, useRef } from 'react';
import type { LucideIcon } from 'lucide-react';

import { cn } from '@/lib/utils';

export interface SelectionMenuItem {
  id: string;
  label: string;
  icon?: LucideIcon;
  disabled?: boolean;
  variant?: 'default' | 'danger';
  onSelect: () => void;
}

export interface SelectionSubmenuProps {
  items: SelectionMenuItem[];
  open: boolean;
  onClose: () => void;
  anchorEl?: HTMLElement | null;
  width?: number;
  ariaLabel: string;
}

export function SelectionSubmenu({
  items,
  open,
  onClose,
  anchorEl,
  width = 280,
  ariaLabel,
}: SelectionSubmenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const focusIndexRef = useRef(0);

  useEffect(() => {
    if (!open || !menuRef.current) return;
    const firstFocusable = itemRefs.current.find((el) => el && !el.disabled);
    if (firstFocusable) {
      firstFocusable.focus();
      focusIndexRef.current = itemRefs.current.indexOf(firstFocusable);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (!menuRef.current) return;
      const enabledItems = itemRefs.current.filter(
        (el): el is HTMLButtonElement => el !== null && !el.disabled,
      );
      if (enabledItems.length === 0) return;

      const currentIdx = enabledItems.indexOf(
        itemRefs.current[focusIndexRef.current] as HTMLButtonElement,
      );

      switch (e.key) {
        case 'ArrowDown': {
          e.preventDefault();
          const next = (currentIdx + 1) % enabledItems.length;
          enabledItems[next].focus();
          focusIndexRef.current = itemRefs.current.indexOf(enabledItems[next]);
          break;
        }
        case 'ArrowUp': {
          e.preventDefault();
          const prev = (currentIdx - 1 + enabledItems.length) % enabledItems.length;
          enabledItems[prev].focus();
          focusIndexRef.current = itemRefs.current.indexOf(enabledItems[prev]);
          break;
        }
        case 'Escape': {
          e.preventDefault();
          onClose();
          break;
        }
        case 'Tab': {
          e.preventDefault();
          onClose();
          break;
        }
        default:
          break;
      }
    }

    const menu = menuRef.current;
    menu?.addEventListener('keydown', handleKeyDown);
    return () => menu?.removeEventListener('keydown', handleKeyDown);
  }, [open, onClose]);

  useEffect(() => {
    if (!open || !anchorEl) return;
    // Containment is the in-tree menu node only; there is no portal consumer of this submenu.
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open, anchorEl, onClose]);

  useEffect(() => {
    if (!open || !anchorEl) return;
    // V1.127 P0 T4 (AC-V1127-4): dismiss on layout change. The popover is
    // `position: fixed` and its coordinates are computed once from the
    // anchor's rect at render (see below); scrolling the sidebar chrome's
    // `overflow-auto` <ul> or resizing the window moves the anchor but not
    // the popover, causing drift. Dismiss-on-layout-change keeps geometry
    // simple (no reposition) and the submenu is cheap to reopen.
    //
    // `resize` fires only on `window`. `scroll` does NOT bubble natively, so
    // a bubble-phase window listener would miss sidebar-container scrolls;
    // capturing on `window` catches `scroll` on ANY descendant (the capture
    // phase descends root → target), covering body-scroll and sidebar-scroll
    // with a single listener. The capture flag must match on remove.
    function handleLayoutChange() {
      onClose();
    }
    window.addEventListener('resize', handleLayoutChange);
    window.addEventListener('scroll', handleLayoutChange, { capture: true });
    return () => {
      window.removeEventListener('resize', handleLayoutChange);
      window.removeEventListener('scroll', handleLayoutChange, { capture: true });
    };
  }, [open, anchorEl, onClose]);

  if (!open || !anchorEl) return null;

  const anchorRect = anchorEl.getBoundingClientRect();
  const popoverWidth = Math.min(width, window.innerWidth - 16);
  let left = anchorRect.right + 4;
  let top = anchorRect.top;

  if (left + popoverWidth > window.innerWidth - 8) {
    left = anchorRect.left - popoverWidth - 4;
  }
  if (left < 8) {
    left = 8;
  }

  const estimatedHeight = items.length * 40 + 16;
  if (top + estimatedHeight > window.innerHeight - 8) {
    top = Math.max(8, window.innerHeight - estimatedHeight - 8);
  }

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label={ariaLabel}
      className={cn(
        'fixed z-50 rounded-card border border-gray-alpha-400 bg-background-100 py-2 shadow-popover',
        'motion-safe:animate-in motion-safe:fade-in motion-safe:zoom-in-95',
        'motion-safe:duration-popover motion-safe:ease-standard',
      )}
      style={{ left, top, width: popoverWidth }}
    >
      {items.map((item, index) => {
        const Icon = item.icon;
        return (
          <button
            key={item.id}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
            role="menuitem"
            disabled={item.disabled}
            onClick={() => {
              if (!item.disabled) {
                item.onSelect();
                onClose();
              }
            }}
            className={cn(
              'flex w-full items-center gap-3 px-3 py-2 text-left text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
              item.disabled
                ? 'cursor-not-allowed text-gray-400'
                : item.variant === 'danger'
                  ? 'text-red-700 hover:bg-red-50 hover:text-red-800 dark:text-red-400 dark:hover:bg-red-950 dark:hover:text-red-300'
                  : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
            )}
          >
            {Icon && (
              <Icon
                className={cn(
                  'h-4 w-4 shrink-0',
                  item.disabled
                    ? 'text-gray-300'
                    : item.variant === 'danger'
                      ? 'text-red-600 dark:text-red-400'
                      : 'text-gray-500',
                )}
                aria-hidden
              />
            )}
            <span className="flex-1 truncate">{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}