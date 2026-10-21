import * as DialogPrimitive from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { type ReactNode } from 'react';

import { cn } from '../lib/cn';

/**
 * Dialog — DESIGN.md §Component Primitives/Dialog.
 *
 * V1.148 P1 — promoted presentational primitive (from apps/web
 * components/ui/dialog.tsx). Built on @radix-ui/react-dialog for accessibility
 * (focus trap, escape, aria). Pure presentational: caller-owned copy via
 * props; no app, query, router, or i18n imports. Radix handles the portal +
 * scroll lock. Theme parity comes from `--nexus-*` token projections
 * (background-100, gray-*, scrim), light and dark.
 *
 * background-100, radius-popover, shadow-elevation-4 (alias shadow-modal),
 * max-width 560px, space-6 padding. Overlay uses the scrim token
 * (§Elevation — V1.121 scrim convergence).
 */
export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export interface DialogContentProps {
  children: ReactNode;
  className?: string;
  /** Optional override for the scrollable body wrapper (e.g. full-height modal frames). */
  bodyClassName?: string;
  title: string;
  description?: string;
  /**
   * Accessible name for the dismiss control — caller-owned copy. Falls back
   * to the app `en` locale string ("Close dialog"); apps/web overrides with
   * its i18n default through the wrapper.
   */
  closeLabel?: string;
}

export function DialogContent({
  children,
  className,
  bodyClassName,
  title,
  description,
  closeLabel = 'Close dialog',
}: DialogContentProps) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-scrim data-[state=open]:animate-in" />
      <DialogPrimitive.Content
        className={cn(
          'fixed left-1/2 top-1/2 z-50 flex max-h-dialog w-dialog max-w-dialog -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-elevation-4',
          className,
        )}
      >
        <div className="flex shrink-0 items-start justify-between gap-4 p-6 pb-4">
          <div className="flex flex-col gap-1">
            <DialogPrimitive.Title className="text-heading-20 font-heading tracking-tight text-gray-1000">
              {title}
            </DialogPrimitive.Title>
            {description && (
              <DialogPrimitive.Description className="text-copy-14 text-gray-900">
                {description}
              </DialogPrimitive.Description>
            )}
          </div>
          <DialogPrimitive.Close
            aria-label={closeLabel}
            className="shrink-0 rounded-control p-1 text-gray-700 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 hover:text-gray-1000"
          >
            <X className="h-4 w-4" aria-hidden />
          </DialogPrimitive.Close>
        </div>
        <div className={cn('overflow-y-auto px-6 pb-6', bodyClassName)}>{children}</div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
