/**
 * States — DESIGN.md §Component Primitives/States.
 *
 * Re-exported from @42ch/nexus-ui (V1.148 P1 promotion).
 * The package owns the presentational implementation; this file is a thin
 * wrapper to avoid call-site churn in apps/web. Default copy stays
 * locale-aware: the wrapper injects the i18n defaults for loading / error
 * title / retry labels, callers may still pass explicit props to override.
 */
import { useTranslation } from 'react-i18next';

import {
  ErrorState as NexusErrorState,
  LoadingState as NexusLoadingState,
  UnavailableState as NexusUnavailableState,
} from '@42ch/nexus-ui';

export { Spinner, EmptyState } from '@42ch/nexus-ui';

export function LoadingState({ label, ...props }: { label?: string }) {
  const { t } = useTranslation('common');
  return <NexusLoadingState label={label ?? t('status.loading')} {...props} />;
}

export function UnavailableState({
  retryLabel,
  ...props
}: Parameters<typeof NexusUnavailableState>[0]) {
  const { t } = useTranslation('common');
  return <NexusUnavailableState retryLabel={retryLabel ?? t('error.retry')} {...props} />;
}

export function ErrorState({
  title,
  retryLabel,
  ...props
}: Parameters<typeof NexusErrorState>[0]) {
  const { t } = useTranslation('common');
  return (
    <NexusErrorState
      title={title ?? t('error.title')}
      retryLabel={retryLabel ?? t('error.retry')}
      {...props}
    />
  );
}
