/**
 * Service-unavailable surface (plan v1219-onboarding T4).
 *
 * One instance at a time: full-screen while the wizard owns the screen (first
 * run, `/setup`), a top banner inside the shell. Repeated failures update this
 * same node — the probe never raises a toast.
 */
import type { ReactNode } from 'react';
import { useLocation } from 'react-router';

import { Button } from '@/components/ui/button';
import { useOptionalServiceStatus, useServiceStatus } from '@/lib/service-status-context';
import { useSetupCompleted } from '@/lib/setup-completed-context';

/** Product-owned copy (plan § Task 4) — ships verbatim, no i18n. */
const COPY = {
  body: (reason: string) => `Nexus 服务暂不可用：${reason}。请确认本地服务正在运行后重试。`,
  retry: '重试',
};

function ServiceUnavailable({
  reason,
  variant,
}: {
  reason: string;
  variant: 'full' | 'banner';
}) {
  const { checking, retry } = useServiceStatus();
  const action = (
    <Button
      type="button"
      variant="secondary"
      size={variant === 'banner' ? 'small' : 'default'}
      disabled={checking}
      onClick={() => void retry()}
    >
      {COPY.retry}
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
          <p className="text-copy-14 text-gray-1000">{COPY.body(reason)}</p>
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
      <p className="min-w-0">{COPY.body(reason)}</p>
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
