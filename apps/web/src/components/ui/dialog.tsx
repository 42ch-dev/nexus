/**
 * Dialog — DESIGN.md §Component Primitives/Dialog.
 *
 * Re-exported from @42ch/nexus-ui (V1.148 P1 promotion).
 * The package owns the presentational implementation; this file is a thin
 * wrapper to avoid call-site churn in apps/web. The close control's
 * accessible name stays locale-aware: the wrapper injects the i18n default,
 * callers may still pass `closeLabel` to override.
 */
import { useTranslation } from 'react-i18next';

import {
  DialogContent as NexusDialogContent,
  type DialogContentProps as NexusDialogContentProps,
} from '@42ch/nexus-ui';

export { Dialog, DialogTrigger, DialogClose } from '@42ch/nexus-ui';

export type DialogContentProps = Omit<NexusDialogContentProps, 'closeLabel'> & {
  closeLabel?: string;
};

export function DialogContent({ closeLabel, ...props }: DialogContentProps) {
  const { t } = useTranslation('common');
  return <NexusDialogContent closeLabel={closeLabel ?? t('dialog.close')} {...props} />;
}
