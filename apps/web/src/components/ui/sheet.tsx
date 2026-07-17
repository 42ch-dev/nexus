import * as DialogPrimitive from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';

/**
 * Sheet — end-aligned drawer built on @radix-ui/react-dialog.
 *
 * Radix provides focus trap, Escape dismiss, and return-focus. Styled for the
 * work-shell right rail at `lg` breakpoint collapse (canvas-work-shell.md).
 */
export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export function SheetContent({
  children,
  className,
  title,
  description,
  side = 'end',
}: {
  children: ReactNode;
  className?: string;
  title: string;
  description?: string;
  /** Only `end` (right rail) is used today; kept for symmetry with DESIGN patterns. */
  side?: 'end';
}) {
  const { t } = useTranslation('common');
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-scrim data-[state=open]:animate-in" />
      <DialogPrimitive.Content
        className={cn(
          'fixed z-50 flex h-full w-sheet flex-col overflow-hidden border-gray-alpha-400 bg-background-100 shadow-elevation-4',
          side === 'end' ? 'right-0 top-0 border-l' : undefined,
          className,
        )}
      >
        <div className="flex items-start justify-between gap-4 border-b border-gray-alpha-400 p-4">
          <div className="flex min-w-0 flex-col gap-1">
            <DialogPrimitive.Title className="text-heading-16 font-heading tracking-tight text-gray-1000">
              {title}
            </DialogPrimitive.Title>
            {description && (
              <DialogPrimitive.Description className="text-copy-14 text-gray-900">
                {description}
              </DialogPrimitive.Description>
            )}
          </div>
          <DialogPrimitive.Close
            aria-label={t('dialog.close')}
            className="shrink-0 rounded-control p-1 text-gray-700 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 hover:text-gray-1000"
          >
            <X className="h-4 w-4" aria-hidden />
          </DialogPrimitive.Close>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
