import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { TransportErrorBlock, type TransportErrorKind } from '@42ch/nexus-ui';

interface DaemonReadySplashProps {
  /**
   * Transport-failure sub-classification (V1.129 P1). When `null`, the splash
   * renders the "Starting daemon…" waiting state. When set, the splash renders
   * the promoted `<TransportErrorBlock>` for the matching kind.
   */
  errorKind: TransportErrorKind | null;
  /** Optional caller-supplied detail line (e.g., the daemon's last detail). */
  errorMessage?: string;
  onRetry: () => void;
  /** Desktop-only recovery: wipe local daemon DB then reload (V1.105 gate). */
  onResetLocalDatabase?: () => void;
  resetBusy?: boolean;
}

/**
 * Full-screen splash while waiting for the daemon on every desktop launch.
 * Owns wait chrome + diagnostic affordances (retry / reset local database).
 * Avoids the main UI shell so the author never sees a "starting" pill inside
 * the Control Room.
 *
 * V1.129 P1: the error state renders the promoted `<TransportErrorBlock>`
 * (kind-classified headline + body + Retry CTA). The Reset Local Database
 * recovery is composed alongside the primitive — it is not a CTA the
 * primitive owns. `onOpenSettings` is omitted because the desktop launch
 * gate runs before the router mounts (no settings route to deep-link to).
 */
export function DaemonReadySplash({
  errorKind,
  errorMessage,
  onRetry,
  onResetLocalDatabase,
  resetBusy = false,
}: DaemonReadySplashProps) {
  const { t } = useTranslation('setup');
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-background-100 p-6 text-center">
      <div className="flex max-w-md flex-col items-center gap-4">
        {errorKind ? (
          <>
            <TransportErrorBlock
              kind={errorKind}
              onRetry={onRetry}
              // Desktop launch gate runs before the router mounts — no settings
              // route is available, so onOpenSettings is intentionally omitted
              // and the Open Connection Settings CTA stays hidden.
              detail={errorMessage}
            />
            {onResetLocalDatabase ? (
              <div className="flex flex-col items-center gap-2">
                <Button
                  variant="tertiary"
                  onClick={onResetLocalDatabase}
                  disabled={resetBusy}
                >
                  {t('action.resetLocalDatabase')}
                </Button>
                <p className="max-w-xs text-center text-label-12 text-gray-800">
                  {t('daemon.resetDatabaseDescription')}
                </p>
              </div>
            ) : null}
          </>
        ) : (
          <>
            <Loader2 className="h-8 w-8 animate-spin text-blue-700" aria-hidden />
            <h1 className="text-heading-24 font-heading text-gray-1000">{t('daemon.starting.title')}</h1>
            <p className="text-copy-14 text-gray-900">{t('daemon.starting.description')}</p>
          </>
        )}
      </div>
    </div>
  );
}
