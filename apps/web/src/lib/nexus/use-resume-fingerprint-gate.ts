import { useCallback, useEffect, useState } from 'react';

import { BrowserClient } from '@/lib/nexus/browser-client';
import { NexusClientError } from '@/lib/nexus/errors';
import type { ConnectionConfig } from '@/lib/nexus/connection-storage';

export type ResumeFingerprintGateState =
  | { status: 'idle' }
  | { status: 'verifying' }
  | { status: 'verified' }
  | { status: 'mismatch'; served: string | null }
  | { status: 'fetch-failed'; message: string };

export interface UseResumeFingerprintGateOptions {
  fetchImpl?: typeof fetch;
}

function shouldVerify(config: ConnectionConfig | null): boolean {
  if (!config) return false;
  if (config.active === false) return false;
  if (!config.endpointUrl) return false;
  // Local same-origin default has no baseUrl and no pin — skip.
  // Remote configs without a pinned fingerprint should not happen post-setup,
  // but be defensive and skip the gate rather than block indefinitely.
  return Boolean(config.pinnedFingerprint);
}

/**
 * Resume-time TOFU verification gate (daemon-runtime.md §16.2 Phases 2–3).
 *
 * When the active connection config carries a `pinnedFingerprint`, fetch the
 * currently-served fingerprint over the unauthenticated endpoint and compare it
 * to the stored pin before any authenticated request is allowed to leave the
 * transport. The returned state drives {@link ClientProvider}: only
 * `verified` produces an authenticated {@link NexusClient}.
 */
export function useResumeFingerprintGate(
  config: ConnectionConfig | null,
  options: UseResumeFingerprintGateOptions = {},
) {
  const [state, setState] = useState<ResumeFingerprintGateState>(() => {
    if (shouldVerify(config)) {
      return { status: 'verifying' };
    }
    return { status: 'verified' };
  });

  const verify = useCallback(async () => {
    if (!shouldVerify(config)) {
      setState({ status: 'verified' });
      return;
    }

    // TypeScript cannot narrow `config` across the shouldVerify helper boundary;
    // we have already confirmed it is a non-null remote config with a pin.
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const activeConfig = config!;
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const pinned = activeConfig.pinnedFingerprint!;

    setState({ status: 'verifying' });
    const client = new BrowserClient({
      baseUrl: activeConfig.endpointUrl,
      fetchImpl: options.fetchImpl,
    });

    try {
      const response = await client.certFingerprint();
      const served = response.fingerprint;

      if (served === '' || served === null || served === undefined) {
        // The daemon reports no TLS cert. A pinned remote config should not be
        // paired with a loopback-only daemon — treat this as a mismatch so the
        // user must re-pin or revert to local.
        setState({ status: 'mismatch', served: served ?? null });
        return;
      }

      if (served === pinned) {
        setState({ status: 'verified' });
      } else {
        setState({ status: 'mismatch', served });
      }
    } catch (error) {
      const message =
        error instanceof NexusClientError
          ? error.message
          : 'Could not verify the daemon certificate. Check the connection and try again.';
      setState({ status: 'fetch-failed', message });
    }
  }, [config, options.fetchImpl]);

  useEffect(() => {
    if (!shouldVerify(config)) {
      setState({ status: 'verified' });
      return;
    }
    let cancelled = false;
    void verify().then(() => {
      if (cancelled) return;
    });
    return () => {
      cancelled = true;
    };
  }, [config, verify]);

  return { state, verify };
}
