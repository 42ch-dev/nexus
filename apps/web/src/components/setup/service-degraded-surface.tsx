/**
 * Service-unavailable surface (plan v1219-onboarding T4).
 *
 * One instance at a time: full-screen while the wizard owns the screen (first
 * run, `/setup`), a top banner inside the shell. Repeated failures update this
 * same node — the probe never raises a toast.
 *
 * The engine is the one subsystem the product takes over the whole screen for:
 * no provider means nothing can run, and the fix is a concrete CLI step, so it
 * gets a repair page rather than a banner over a dead app. Other causes keep
 * the full-screen/banner split.
 *
 * Copy lives in the `setup` namespace (`degraded.*`): the zh-CN body is the
 * product-owned string verbatim, and the EN namespace is the fallback.
 */
import type { TFunction } from 'i18next';
import { useState, type ReactNode } from 'react';
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
function reasonText(
  reason: Exclude<ServiceDegradedReason, { kind: 'provider_unselected' }>,
  t: TFunction<'setup'>,
): string {
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

type EngineFailure = Exclude<ServiceDegradedReason, { kind: 'unreachable' | 'server' }>;

/** The engine is the only degraded subsystem: the workspace cannot run tasks. */
function isEngineFailure(reason: ServiceDegradedReason): reason is EngineFailure {
  return (
    reason.kind === 'provider_unselected' ||
    (reason.kind === 'subsystems' &&
      reason.subsystems.length === 1 &&
      reason.subsystems[0] === 'engine')
  );
}

const CONNECT_COMMAND = 'nexus42 connect';

/**
 * Full-screen repair page for an engine that cannot serve the workspace.
 *
 * Two states, one page: no provider selected (`provider_unselected`) is steps
 * only; a selected provider whose readiness probe failed adds the failure line
 * under step 1. The retry button is the existing probe, so a fixed engine
 * releases the gate and the app renders.
 */
function EngineRepair({ reason }: { reason: EngineFailure }) {
  const { t } = useTranslation('setup');
  const { checking, retry } = useServiceStatus();
  const [copied, setCopied] = useState(false);

  const copyCommand = async () => {
    try {
      await navigator.clipboard.writeText(CONNECT_COMMAND);
      setCopied(true);
    } catch {
      // Clipboard access denied: the command is still visible to copy by hand.
    }
  };

  return (
    <div
      role="alert"
      data-testid="service-degraded-surface"
      data-variant="full"
      data-state={reason.kind === 'provider_unselected' ? 'unconfigured' : 'failed'}
      className="flex min-h-screen items-center justify-center bg-background-100 p-6"
    >
      <div className="flex w-full max-w-lg flex-col gap-6">
        <h1 className="font-display text-display-24 text-gray-1000">
          {t('degraded.repair.title')}
        </h1>
        <p className="text-copy-14 text-gray-900">{t('degraded.repair.description')}</p>
        <ol className="flex list-none flex-col gap-5 p-0">
          <li className="flex flex-col gap-2">
            <h2 className="text-copy-16 text-gray-1000">{t('degraded.repair.stepConnect.title')}</h2>
            <p className="text-copy-14 text-gray-900">{t('degraded.repair.stepConnect.body')}</p>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 rounded-md bg-background-200 px-3 py-2 font-mono text-copy-13 text-gray-1000">
                {CONNECT_COMMAND}
              </code>
              <Button type="button" variant="secondary" size="small" onClick={() => void copyCommand()}>
                {copied ? t('degraded.repair.copied') : t('degraded.repair.copy')}
              </Button>
            </div>
            {reason.kind !== 'provider_unselected' && (
              <p data-testid="engine-repair-failure" className="text-copy-13 text-red-1000">
                {t('degraded.repair.failure', { reason: reasonText(reason, t) })}
              </p>
            )}
          </li>
          <li className="flex flex-col items-start gap-2">
            <h2 className="text-copy-16 text-gray-1000">{t('degraded.repair.stepRecheck.title')}</h2>
            <p className="text-copy-14 text-gray-900">{t('degraded.repair.stepRecheck.body')}</p>
            <Button
              type="button"
              variant="secondary"
              disabled={checking}
              onClick={() => void retry()}
            >
              {t('degraded.repair.recheck')}
            </Button>
          </li>
        </ol>
      </div>
    </div>
  );
}

function ServiceUnavailable({
  reason,
  variant,
}: {
  reason: Exclude<ServiceDegradedReason, { kind: 'provider_unselected' }>;
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
 * Route wrapper: an engine failure takes over every route (nothing can run);
 * the wizard takes the full-screen variant for other causes, and the shell
 * keeps its chrome and renders {@link ServiceDegradedBanner} itself. An
 * unresolved first-run gate means the wizard is the next screen, so it takes
 * the full-screen variant too.
 */
export function ServiceDegradedGate({ children }: { children: ReactNode }) {
  const { reason } = useServiceStatus();
  const { completed } = useSetupCompleted();
  const { pathname } = useLocation();

  if (reason !== null) {
    if (isEngineFailure(reason)) return <EngineRepair reason={reason} />;
    if (!completed || pathname === '/setup') {
      return <ServiceUnavailable reason={reason} variant="full" />;
    }
  }
  return <>{children}</>;
}

/** Shell-level variant; renders nothing while the service is healthy. */
export function ServiceDegradedBanner() {
  const status = useOptionalServiceStatus();
  const reason = status?.reason;
  // Engine failures never reach the shell: the gate owns the whole screen.
  return reason == null || isEngineFailure(reason) ? null : (
    <ServiceUnavailable reason={reason} variant="banner" />
  );
}
