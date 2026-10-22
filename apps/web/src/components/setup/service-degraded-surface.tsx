/**
 * Service-unavailable surface (plan v1219-onboarding T4).
 *
 * One instance at a time: full-screen while the wizard owns the screen (first
 * run, `/setup`), a top banner inside the shell. Repeated failures update this
 * same node — the probe never raises a toast.
 *
 * Copy lives in the `setup` namespace (`degraded.*`): the zh-CN body is the
 * product-owned string verbatim, and the EN namespace is the fallback.
 */
import type { TFunction } from 'i18next';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router';

import { Button } from '@/components/ui/button';
import type { ServiceDegradedReason } from '@/lib/service-status-context';
import { useOptionalServiceStatus, useServiceStatus } from '@/lib/service-status-context';
import { useSetupCompleted } from '@/lib/setup-completed-context';

/**
 * The `{reason}` slot. A server-provided message is already human copy, so it
 * is kept as-is; transport and subsystem reasons are localized.
 */
function reasonText(reason: ServiceDegradedReason, t: TFunction<'setup'>): string {
  switch (reason.kind) {
    case 'unreachable':
      return t('degraded.reason.unreachable');
    case 'server':
      return reason.message;
    case 'subsystems':
      return reason.subsystems
        .map((name) => t(`degraded.subsystem.${name}`, { defaultValue: name }))
        .join(t('degraded.separator'));
  }
}

function ServiceUnavailable({
  reason,
  variant,
}: {
  reason: ServiceDegradedReason;
  variant: 'full' | 'banner';
}) {
  const { t } = useTranslation('setup');
  const { checking, retry } = useServiceStatus();
  const body = t('degraded.body', { reason: reasonText(reason, t) });
  const action = (
    <Button
      type="button"
      variant="secondary"
      size={variant === 'banner' ? 'small' : 'default'}
      disabled={checking}
      onClick={() => void retry()}
    >
      {t('degraded.retry')}
    </Button>
  );

  if (variant === 'full') {
    return (
      <div
        role="alert"
        data-testid="service-degraded-surface"
        data-variant="full"
        className="flex min-h-screen items-center justify-center bg-background-100 p-6"
      >
        <div className="flex w-full max-w-md flex-col items-start gap-4">
          <p className="text-copy-14 text-gray-1000">{body}</p>
          {action}
        </div>
      </div>
    );
  }

  return (
    <div
      role="alert"
      data-testid="service-degraded-surface"
      data-variant="banner"
      className="flex flex-wrap items-center justify-between gap-3 border-b border-red-700/30 bg-red-700/10 px-4 py-2 text-copy-13 text-red-1000"
    >
      <p className="min-w-0">{body}</p>
      {action}
    </div>
  );
}

/**
 * Route wrapper: the wizard takes the full-screen variant, the shell keeps its
 * chrome and renders {@link ServiceDegradedBanner} itself. An unresolved
 * first-run gate means the wizard is the next screen, so it takes the
 * full-screen variant too.
 */
export function ServiceDegradedGate({ children }: { children: ReactNode }) {
  const { reason } = useServiceStatus();
  const { completed } = useSetupCompleted();
  const { pathname } = useLocation();

  if (reason !== null && (!completed || pathname === '/setup')) {
    return <ServiceUnavailable reason={reason} variant="full" />;
  }
  return <>{children}</>;
}

/** Shell-level variant; renders nothing while the service is healthy. */
export function ServiceDegradedBanner() {
  const status = useOptionalServiceStatus();
  return status?.reason == null ? null : (
    <ServiceUnavailable reason={status.reason} variant="banner" />
  );
}
