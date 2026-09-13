import { nativeCompatibility, openCore, type NativeCore, type ProviderCallbacks } from '@42ch/nexus-native';
import type { CertFingerprintResponse, CoreCloseReport } from '@42ch/nexus-contracts';
import type { ResolvedServiceConfig } from './config.js';
import { validateServiceHome } from './config.js';
import { HttpError } from './errors.js';
import { certCreatedAtFromMtime, fingerprintFromPem } from './tls.js';

export interface ServiceCore {
  core: NativeCore;
  domainOnly: boolean;
  tlsFingerprint: CertFingerprintResponse | null;
  startedAt: string;
  workspaceInitialized: boolean;
  principalId: string;
  providerReady: boolean;
}

export async function openServiceCore(
  config: ResolvedServiceConfig,
  providers?: ProviderCallbacks,
): Promise<ServiceCore> {
  nativeCompatibility();
  validateServiceHome(config.home);

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

  let principalId = '';
  try {
    principalId = await core.activePrincipal();
  } catch (error) {
    await core.close().catch(() => undefined);
    throw error;
  }

  let tlsFingerprint: CertFingerprintResponse | null = null;
  if (config.tlsCert) {
    const { fingerprint, algorithm } = fingerprintFromPem(config.tlsCert);
    tlsFingerprint = {
      fingerprint,
      algorithm,
      ...(config.tlsCertMtimeMs !== undefined
        ? { created_at: certCreatedAtFromMtime(config.tlsCertMtimeMs) }
        : {}),
    };
  }

  let providerReady = Boolean(config.domainOnly);
  if (!config.domainOnly) {
    try {
      await core.hostQuery({ query: 'health' });
      providerReady = true;
    } catch {
      providerReady = false;
    }
  }

  return {
    core,
    domainOnly: Boolean(config.domainOnly),
    tlsFingerprint,
    startedAt: new Date().toISOString(),
    workspaceInitialized: true,
    principalId,
    providerReady,
  };
}

export async function closeServiceCore(core: NativeCore): Promise<CoreCloseReport> {
  return core.close();
}
