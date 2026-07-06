import { Loader2, RefreshCw } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface DaemonReadySplashProps {
  error: string | null;
  onRetry: () => void;
}

/**
 * Minimal full-screen splash shown while waiting for the daemon on every
 * launch. Avoids the main UI shell so the user never sees a "starting" pill
 * inside the Control Room.
 */
export function DaemonReadySplash({ error, onRetry }: DaemonReadySplashProps) {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-background-100 p-6 text-center">
      <div className="flex max-w-md flex-col items-center gap-4">
        {error ? (
          <>
            <h1 className="text-heading-24 font-heading text-gray-1000">Daemon not ready</h1>
            <p className="text-copy-14 text-gray-900">{error}</p>
            <Button variant="primary" onClick={onRetry}>
              <RefreshCw className="h-4 w-4" aria-hidden />
              Restart Nexus
            </Button>
          </>
        ) : (
          <>
            <Loader2 className="h-8 w-8 animate-spin text-blue-700" aria-hidden />
            <h1 className="text-heading-24 font-heading text-gray-1000">Starting daemon…</h1>
            <p className="text-copy-14 text-gray-900">This takes a few seconds on first launch.</p>
          </>
        )}
      </div>
    </div>
  );
}
