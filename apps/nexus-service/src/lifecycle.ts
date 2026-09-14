import { randomUUID } from 'node:crypto';
import {
  isNativeCoreErrorCode,
  nativeCompatibility,
  openCore,
  type NativeCore,
  type ProviderCallbacks,
} from '@42ch/nexus-native';
import type { CertFingerprintResponse, CoreCloseReport } from '@42ch/nexus-contracts';
import type { ResolvedServiceConfig } from './config.js';
import { PROVIDER_DEFAULT_DEADLINE_MS, validateServiceHome } from './config.js';
import { HttpError, mapNativeError } from './errors.js';
import { certCreatedAtFromMtime, fingerprintFromPem } from './tls.js';
import { ProviderRegistry } from './provider-registry.js';

export interface ServiceCore {
  core: NativeCore;
  domainOnly: boolean;
  tlsFingerprint: CertFingerprintResponse | null;
  startedAt: string;
  /** False for the status-only profile: no principal, no KB/host/provider effects. */
  workspaceInitialized: boolean;
  providerReady: boolean;
  /** HTTP-side session/operation mirror for JS-provider inspect paths (not a second HostManager). */
  providerRegistry: ProviderRegistry;
}

export async function openServiceCore(
  config: ResolvedServiceConfig,
  providers?: ProviderCallbacks,
): Promise<ServiceCore> {
  nativeCompatibility();
  validateServiceHome(config.home);

  const access: 'direct_writer' | 'engine_owner' = config.domainOnly ? 'direct_writer' : 'engine_owner';
  if (!config.domainOnly && !providers) {
    throw new HttpError(
      503,
      'busy',
      'Provider-enabled profile requires a registered provider adapter',
    );
  }

  // `allow_uninitialized` is the service-only admission flag: an initialized
  // home opens the full core, a missing profile opens the status-only shell so
  // `/runtime/status` can report the existing uninitialized state.
  const openOptions = {
    user_home: config.home,
    access,
    allow_uninitialized: true,
  };
  const core = await openCore(openOptions, config.domainOnly ? undefined : providers);

  let workspaceInitialized = false;
  try {
    // Publication gate: the initialized profile is published only once the
    // native principal resolves. A typed `uninitialized` rejection is the
    // status-only profile, not a startup failure; anything else aborts before
    // the listener binds.
    await core.activePrincipal();
    workspaceInitialized = true;
  } catch (error) {
    if (!isNativeCoreErrorCode(error, 'uninitialized')) {
      await core.close().catch(() => undefined);
      throw mapNativeError(error);
    }
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

  let providerReady = false;
  if (workspaceInitialized) {
    providerReady = config.domainOnly ? true : await confirmProviderReadiness(core);
  }

  return {
    core,
    domainOnly: Boolean(config.domainOnly),
    tlsFingerprint,
    startedAt: new Date().toISOString(),
    workspaceInitialized,
    providerReady,
    providerRegistry: new ProviderRegistry(),
  };
}

/**
 * Confirm provider readiness before publication.
 *
 * Host-manager liveness alone is not readiness: a running host with no admitted
 * provider still reports `running: true`. Readiness requires at least one
 * admitted provider in the catalog **and** at least one provider whose bounded
 * no-model availability probe succeeded. When either is unavailable the
 * provider-enabled profile is published as degraded/not-ready — a failed or
 * unperformed probe is never reported as ready.
 */
async function confirmProviderReadiness(core: NativeCore): Promise<boolean> {
  try {
    const catalog = await core.hostQuery({ query: 'catalog', format: 'catalog' });
    const providers = catalog.catalog?.providers ?? [];
    if (providers.length === 0) {
      return false;
    }
    for (const provider of providers) {
      const reply = await core.providerCall({
        method: 'probe',
        request_id: randomUUID(),
        deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
        payload: { provider_id: provider.provider_id },
      });
      if (reply.ok && reply.health?.available) {
        return true;
      }
    }
    return false;
  } catch {
    return false;
  }
}

export async function closeServiceCore(core: NativeCore): Promise<CoreCloseReport> {
  return core.close();
}
