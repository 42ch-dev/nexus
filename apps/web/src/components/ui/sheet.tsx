/**
 * Sheet — end-aligned drawer, DESIGN.md §Component Primitives/Sheet.
 *
 * Re-exported from @42ch/nexus-ui (V1.148 P1 promotion).
 * The package owns the presentational implementation; this file is a thin
 * wrapper to avoid call-site churn in apps/web. The close control's
 * accessible name stays locale-aware: the wrapper injects the i18n default,
 * callers may still pass `closeLabel` to override.
 */
import { useTranslation } from 'react-i18next';

import {
  SheetContent as NexusSheetContent,
  type SheetContentProps as NexusSheetContentProps,
} from '@42ch/nexus-ui';

export { Sheet, SheetTrigger, SheetClose } from '@42ch/nexus-ui';

export type SheetContentProps = Omit<NexusSheetContentProps, 'closeLabel'> & {
  closeLabel?: string;
};

export function SheetContent({ closeLabel, ...props }: SheetContentProps) {
  const { t } = useTranslation('common');
  return <NexusSheetContent closeLabel={closeLabel ?? t('dialog.close')} {...props} />;
}
