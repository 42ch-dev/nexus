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
import { validateServiceHome } from './config.js';
import { HttpError, mapNativeError } from './errors.js';
import { certCreatedAtFromMtime, fingerprintFromPem } from './tls.js';
import { ProviderRegistry } from './provider-registry.js';

export interface ServiceCore {
  core: NativeCore;
  /** Absolute daemon home this process opened (owns `config.toml`). */
  home: string;
  domainOnly: boolean;
  tlsFingerprint: CertFingerprintResponse | null;
  startedAt: string;
  /** False for the status-only profile: no principal, no KB/host/provider effects. */
  workspaceInitialized: boolean;
  /**
   * Native-reported readiness of the providers this host configuration
   * selects. Catalog membership, Host liveness or an unrelated available
   * provider never substitutes for the selected providers' own probes.
   */
  providerReady: boolean;
  /** HTTP-side session/operation mirror for JS-provider inspect paths (not a second HostManager). */
  providerRegistry: ProviderRegistry;
  /** Random per-start identity; the only stop authorization (never the pid). */
  instanceId: string;
  /** Selected identity of an initialized core; null on the uninitialized shell. */
  creatorId: string | null;
  workspaceSlug: string | null;
  /**
   * Engine identity of this process: the ACTUAL established hosted owner's
   * epoch, null on the uninitialized shell, and 0 when the profile owns no
   * execution engine (the domain-only profile, or a selected workspace that
   * cannot host a hosted owner). 0 is the core's own "no engine epoch"
   * sentinel, never a synthesized epoch.
   */
  engineEpoch: number | null;
}

/**
 * Decode the selected identity from the native principal handle. The handle
 * is the established `p:<generation>:<creator_id>:<workspace_slug>` encoding
 * produced by the native core (`EnvState::encode_principal`); the identity
 * still originates in Rust — this only reads it for the discovery record and
 * the instance-bound stop comparison. A malformed handle is a native contract
 * break and fails the open loudly instead of publishing a lying record.
 */
function parsePrincipalIdentity(handle: string): { creatorId: string; workspaceSlug: string } {
  if (!handle.startsWith('p:')) {
    throw new HttpError(500, 'internal', 'native principal identity unreadable');
  }
  const parts = handle.slice(2).split(':');
  // Both ids are colon-free by construction, so the handle splits into
  // exactly generation, creator id and workspace slug.
  if (parts.length !== 3 || !/^\d+$/.test(parts[0])) {
    throw new HttpError(500, 'internal', 'native principal identity unreadable');
  }
  const creatorId = parts[1];
  const workspaceSlug = parts[2];
  if (creatorId.length === 0 || workspaceSlug.length === 0) {
    throw new HttpError(500, 'internal', 'native principal identity unreadable');
  }
  return { creatorId, workspaceSlug };
}

/**
 * Start the ONE hosted execution owner, compensating a pre-IA home first.
 *
 * A home initialized before the workspace IA has no admission pin. The native
 * core tolerates the missing hosted-workspace deps as `engine_epoch: null`
 * WITHOUT establishing the owner, so every later compute call fails. Selecting
 * the canonical workspace pins admission (exactly the primitive the workspace
 * API uses), and the second start then establishes the owner. A failed
 * compensation degrades to the tolerated shape instead of crashing startup.
 */
export async function startExecutionOwnerWithCompensation(
  core: Pick<NativeCore, 'startExecutionOwner' | 'selectWorkspace'>,
  identity: { creatorId: string; workspaceSlug: string },
): Promise<{ engine_epoch: number | null; provider_ready: boolean }> {
  const owner = await core.startExecutionOwner();
  if (owner.engine_epoch != null) return owner;

  try {
    await core.selectWorkspace({
      creator_id: identity.creatorId,
      workspace_slug: identity.workspaceSlug,
    });
    const retried = await core.startExecutionOwner();
    process.stderr.write(
      '[nexus-service] pre-IA home: selected default workspace to start the execution owner\n',
    );
    return retried;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    process.stderr.write(
      `[nexus-service] pre-IA home: workspace selection compensation failed: ${reason}\n`,
    );
    return owner;
  }
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
  let identity: { creatorId: string; workspaceSlug: string } | null = null;
  try {
    // Publication gate: the initialized profile is published only once the
    // native principal resolves. A typed `uninitialized` rejection is the
    // status-only profile, not a startup failure; anything else aborts before
    // the listener binds.
    const principal = await core.activePrincipal();
    identity = parsePrincipalIdentity(principal);
    workspaceInitialized = true;
  } catch (error) {
    if (!(error instanceof HttpError) && !isNativeCoreErrorCode(error, 'uninitialized')) {
      await core.close().catch(() => undefined);
      throw mapNativeError(error);
    }
    if (error instanceof HttpError) {
      await core.close().catch(() => undefined);
      throw error;
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
  let engineEpoch: number | null = null;
  // `identity` is published exactly when the profile is initialized.
  if (identity) {
    try {
      if (config.domainOnly) {
        // The domain-only profile owns no execution engine; it also acquires
        // no provider runtime edge, so nothing can make it "provider ready"
        // and there is no engine epoch to report.
        providerReady = true;
        engineEpoch = 0;
      } else {
        // The ONE hosted execution owner is established BEFORE readiness is
        // computed and before the record is published. The native factory
        // composes the complete owner (selected-root workspace ports and
        // startup recovery, prompt executor, catalog, run-event registry,
        // cancellation map, scheduler) and reports the ACTUAL epoch plus the
        // native-owned selected-provider readiness — never a default. A pre-IA
        // home has no admission pin yet; the helper selects the canonical
        // workspace once and retries before readiness is published.
        const owner = await startExecutionOwnerWithCompensation(core, identity);
        engineEpoch = owner.engine_epoch ?? 0;
        providerReady = owner.provider_ready;
      }
    } catch (error) {
      await core.close().catch(() => undefined);
      throw mapNativeError(error);
    }
  }
  return {
    core,
    home: config.home,
    domainOnly: Boolean(config.domainOnly),
    tlsFingerprint,
    startedAt: new Date().toISOString(),
    workspaceInitialized,
    providerReady,
    providerRegistry: new ProviderRegistry(),
    instanceId: randomUUID(),
    creatorId: identity?.creatorId ?? null,
    workspaceSlug: identity?.workspaceSlug ?? null,
    engineEpoch,
  };
}

export async function closeServiceCore(core: NativeCore): Promise<CoreCloseReport> {
  return core.close();
}
