import { useCallback, useState } from 'react';

import type { CertFingerprintResponse } from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus/browser-client';
import { NexusClientError } from '@/lib/nexus/errors';

export type FingerprintState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'success'; response: CertFingerprintResponse }
  | { status: 'error'; message: string; code?: string };

export interface UseFingerprintOptions {
  fetchImpl?: typeof fetch;
}

/**
 * Fetch the daemon's TLS certificate fingerprint for TOFU confirmation.
 *
 * The fingerprint endpoint is unauthenticated by design (daemon-runtime.md
 * §15.4). We use a throw-away {@link BrowserClient} so the API key is never
 * sent during verification.
 */
export function useFingerprint(options: UseFingerprintOptions = {}) {
  const [state, setState] = useState<FingerprintState>({ status: 'idle' });

  const fetchFingerprint = useCallback(
    async (endpointUrl: string) => {
      setState({ status: 'loading' });
      const client = new BrowserClient({
        baseUrl: endpointUrl,
        fetchImpl: options.fetchImpl,
      });
      try {
        const response = await client.certFingerprint();
        setState({ status: 'success', response });
        return response;
      } catch (error) {
        const message =
          error instanceof NexusClientError
            ? error.message
            : 'Could not fetch the certificate fingerprint. Check the URL and try again.';
        const next: FingerprintState = {
          status: 'error',
          message,
          code: error instanceof NexusClientError ? error.code : undefined,
        };
        setState(next);
        return null;
      }
    },
    [options.fetchImpl],
  );

  const reset = useCallback(() => setState({ status: 'idle' }), []);

  return { state, fetchFingerprint, reset };
}
