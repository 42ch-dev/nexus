import { createHash } from 'node:crypto';
import { nativeCompatibility, openCore, type NativeCore, type ProviderCallbacks } from '@42ch/nexus-native';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import { CLOSE_BUDGET_MS, type ResolvedServiceConfig } from './config.js';
import { HttpError } from './errors.js';

export interface ServiceCore {
  core: NativeCore;
  domainOnly: boolean;
  tlsFingerprint: string | null;
  startedAt: string;
}

let closingPromise: Promise<CoreCloseReport> | null = null;

export async function openServiceCore(
  config: ResolvedServiceConfig,
  providers?: ProviderCallbacks,
): Promise<ServiceCore> {
  nativeCompatibility();

  const access = config.domainOnly ? 'direct_writer' : 'engine_owner';
  if (!config.domainOnly && !providers) {
    throw new HttpError(
      503,
      'busy',
      'Provider-enabled profile requires a registered provider adapter',
    );
  }

  const core = await openCore(
    {
      user_home: config.home,
      access,
      allow_uninitialized: false,
    },
    config.domainOnly ? undefined : providers,
  );

  let tlsFingerprint: string | null = null;
  if (config.tlsCert) {
    tlsFingerprint = createHash('sha256').update(config.tlsCert).digest('hex');
  }

  return {
    core,
    domainOnly: Boolean(config.domainOnly),
    tlsFingerprint,
    startedAt: new Date().toISOString(),
  };
}

export async function closeServiceCore(
  core: NativeCore,
  budgetMs = CLOSE_BUDGET_MS,
): Promise<CoreCloseReport> {
  if (closingPromise) return closingPromise;

  closingPromise = (async () => {
    const { promise: timeout, resolve: resolveTimeout } = Promise.withResolvers<CoreCloseReport>();
    const timer = setTimeout(() => {
      resolveTimeout({
        state: 'interrupted',
        cleanup_confirmed: false,
        pending_operations: [],
        reason: 'user_requested',
      });
    }, budgetMs);
    try {
      return await Promise.race([core.close(), timeout]);
    } finally {
      clearTimeout(timer);
      closingPromise = null;
    }
  })();

  return closingPromise;
}
