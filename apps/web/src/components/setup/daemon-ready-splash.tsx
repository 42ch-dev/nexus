import { Loader2, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';

interface DaemonReadySplashProps {
  error: string | null;
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
 */
export function DaemonReadySplash({
  error,
  onRetry,
  onResetLocalDatabase,
  resetBusy = false,
}: DaemonReadySplashProps) {
  const { t } = useTranslation('setup');
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-background-100 p-6 text-center">
      <div className="flex max-w-md flex-col items-center gap-4">
        {error ? (
          <>
            <h1 className="text-heading-24 font-heading text-gray-1000">{t('daemon.notReady.title')}</h1>
            <p className="whitespace-pre-wrap break-words text-copy-14 text-gray-900">{error}</p>
            <Button variant="primary" onClick={onRetry} disabled={resetBusy}>
              <RefreshCw className="h-4 w-4" aria-hidden />
              {t('daemon.restartNexus')}
            </Button>
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
