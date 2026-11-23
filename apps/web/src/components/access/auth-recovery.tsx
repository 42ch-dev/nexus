import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@42ch/nexus-ui';
import { clearStoredApiKey, storeApiKey } from '@/lib/client-context';
import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { AUTHORIZED_EVENT, UNAUTHORIZED_EVENT } from '@/lib/nexus/errors';

type ProbeState = 'idle' | 'checking' | 'invalid';

/**
 * App-level 401 recovery surface (access spec §6.1): the first 401 replaces the
 * view with ONE instance; concurrent 401s collapse into it because the blocked
 * state is a boolean and the gate never raises toasts. A valid API key is
 * stored and the client provider rebuilds from it, restoring the intended
 * destination. Keyless loopback mode never returns 401, so it never shows.
 */
export function AuthRecoveryGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation('shell');
  const [blocked, setBlocked] = useState(false);
  const [key, setKey] = useState('');
  const [probe, setProbe] = useState<ProbeState>('idle');

  useEffect(() => {
    const onUnauthorized = () => setBlocked(true);
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
  }, []);

  /**
   * unguarded, so it would answer 200 with or without a key. With no candidate
   * key the probe re-reads the session cookie (bootstrap finished in another tab).
   */
  async function probeAccess(candidate?: string) {
    setProbe('checking');
    try {
      await new BrowserClient(candidate ? { apiKey: candidate } : {}).listCreators();
      // stale stored key, because a non-empty X-API-Key wins service-side and
      // would keep rejecting requests (access spec §6.1).
      if (candidate) storeApiKey(candidate);
      else clearStoredApiKey();
      window.dispatchEvent(new Event(AUTHORIZED_EVENT));
      setKey('');
      setProbe('idle');
      setBlocked(false);
    } catch (error) {
      setProbe(error instanceof NexusClientError && error.status === 401 ? 'invalid' : 'idle');
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    void probeAccess(key.trim());
  }

  if (!blocked) return <>{children}</>;

  return (
    <div className="flex min-h-screen items-center justify-center bg-background-100 p-6">
      <div className="w-full max-w-md space-y-4">
        <h1 className="text-heading-20 text-gray-1000">{t('access.recovery.title')}</h1>
        <p className="text-copy-14 text-gray-900">{t('access.recovery.body')}</p>
        <form className="space-y-3" onSubmit={submit}>
          <label className="block text-copy-13 text-gray-700" htmlFor="auth-recovery-key">
            {t('access.recovery.keyLabel')}
          </label>
          <Input
            id="auth-recovery-key"
            type="password"
            autoComplete="off"
            value={key}
            invalid={probe === 'invalid'}
            aria-describedby={probe === 'invalid' ? 'auth-recovery-error' : undefined}
            onChange={(event) => setKey(event.target.value)}
          />
          <Button type="submit" disabled={key.trim().length === 0 || probe === 'checking'}>
            {t('access.recovery.submit')}
          </Button>
        </form>
        {probe === 'invalid' ? (
          <p id="auth-recovery-error" role="alert" className="text-copy-13 text-red-700">
            {t('access.recovery.invalid')}
          </p>
        ) : null}
        <Button variant="secondary" onClick={() => void probeAccess()}>
          {t('access.recovery.recheck')}
        </Button>
      </div>
    </div>
  );
}
