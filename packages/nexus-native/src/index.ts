import type {
  CoreChangesRequest,
  CoreChangesResponse,
  CoreCloseReport,
  CoreHostQuery,
  CoreHostQueryResponse,
  NativeCompatibility,
  NativeOpenOptions,
  ProviderCall,
  ProviderEventBatch,
  ProviderReply,
  WorldKbCandidatesResponse,
  WorldKbGraphResponse,
  WorldKbPatchEntityRequest,
  WorldKbPatchEntityResponse,
} from '@42ch/nexus-contracts';
import {
  expectedPlatformPackage,
  fenceCompatibilityPair,
  loadNativeBinding,
  loadNodePath,
  readBundledCompatibility,
  readCompatibilityManifest,
  readPackageManifest,
  type NativeCoreBinding,
} from './loader.js';
import { parseJsonBuffer } from './json.js';
export { isNativeCoreErrorCode, parseNativeCoreError } from './errors.js';
import {
  CORE_CHANGES_REQUEST_SHAPE,
  CORE_HOST_QUERY_SHAPE,
  NATIVE_OPEN_OPTIONS_SHAPE,
  PROVIDER_CALL_SHAPE,
  WORLD_KB_PATCH_ENTITY_SHAPE,
  assertSafeInteger,
  encodeWireBuffer,
  stringifyWire,
} from './validate.js';

function unpackTsfnJson(...args: unknown[]): string {
  const payload = args.length > 1 ? args[1] : args[0];
  if (typeof payload !== 'string') {
    throw new Error('provider callback payload must be a JSON string');
  }
  return payload;
}

export interface ProviderCallbacks {
  call(request: ProviderCall): Promise<ProviderReply>;
  next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch>;
}

export type PrincipalHandle = string & { readonly __brand: unique symbol };

export interface NativeCore {
  activePrincipal(): Promise<PrincipalHandle>;
  worldKbGraph(
    principal: PrincipalHandle,
    worldId: string,
    includeSuggested: boolean,
  ): Promise<WorldKbGraphResponse>;
  patchWorldKbEntity(
    principal: PrincipalHandle,
    worldId: string,
    request: WorldKbPatchEntityRequest,
  ): Promise<WorldKbPatchEntityResponse>;
  worldKbCandidates(
    principal: PrincipalHandle,
    worldId: string,
    limit?: number,
    cursor?: string,
  ): Promise<WorldKbCandidatesResponse>;
  hostQuery(request: CoreHostQuery): Promise<CoreHostQueryResponse>;
  changes(principal: PrincipalHandle, request: CoreChangesRequest): Promise<CoreChangesResponse>;
  providerCall(request: ProviderCall): Promise<ProviderReply>;
  nextProviderEvents(
    operationId: string,
    maxEvents: number,
    maxBytes: number,
  ): Promise<ProviderEventBatch>;
  close(): Promise<CoreCloseReport>;
}

function wrapCore(inner: NativeCoreBinding): NativeCore {
  return {
    async activePrincipal() {
      return (await inner.activePrincipal()) as PrincipalHandle;
    },
    async worldKbGraph(principal, worldId, includeSuggested) {
      return parseJsonBuffer(await inner.worldKbGraph(principal, worldId, includeSuggested));
    },
    async patchWorldKbEntity(principal, worldId, request) {
      return parseJsonBuffer(
        await inner.patchWorldKbEntity(
          principal,
          worldId,
          encodeWireBuffer(request, WORLD_KB_PATCH_ENTITY_SHAPE, 'request'),
        ),
      );
    },
    async worldKbCandidates(principal, worldId, limit, cursor) {
      return parseJsonBuffer(
        await inner.worldKbCandidates(
          principal,
          worldId,
          assertSafeInteger(limit ?? null, 'limit'),
          cursor ?? null,
        ),
      );
    },
    async hostQuery(request) {
      return parseJsonBuffer(
        await inner.hostQuery(encodeWireBuffer(request, CORE_HOST_QUERY_SHAPE, 'request')),
      );
    },
    async changes(principal, request) {
      return parseJsonBuffer(
        await inner.changes(
          principal,
          encodeWireBuffer(request, CORE_CHANGES_REQUEST_SHAPE, 'request'),
        ),
      );
    },
    async providerCall(request) {
      return parseJsonBuffer(
        await inner.providerCall(encodeWireBuffer(request, PROVIDER_CALL_SHAPE, 'request')),
      );
    },
    async nextProviderEvents(operationId, maxEvents, maxBytes) {
      return parseJsonBuffer(await inner.nextProviderEvents(operationId, maxEvents, maxBytes));
    },
    async close() {
      return parseJsonBuffer(await inner.close());
    },
  };
}

/** Compatibility manifest for this platform, before any DB open. */
export function nativeCompatibility(): NativeCompatibility {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const adjacent = readBundledCompatibility(nodePath);
  const target = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(target.name);
  fenceCompatibilityPair(manifest, adjacent, {
    target_triple: target.targetTriple,
    package_version: pkgManifest.version,
  });
  return manifest;
}

/**
 * Open the native core. Always awaited: the frozen facade returns a Promise so
 * callers can await the open boundary.
 */
export async function openCore(
  options: NativeOpenOptions,
  providers?: ProviderCallbacks,
): Promise<NativeCore> {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const adjacent = readBundledCompatibility(nodePath);
  const target = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(target.name);
  fenceCompatibilityPair(manifest, adjacent, {
    target_triple: target.targetTriple,
    package_version: pkgManifest.version,
  });

  const callbacks = providers
    ? {
        call: async (...args: unknown[]) =>
          stringifyWire(
            await providers.call(JSON.parse(unpackTsfnJson(...args))),
            undefined,
            'provider_reply',
          ),
        next: async (...args: unknown[]) => {
          const { operation_id, max_events, max_bytes } = JSON.parse(unpackTsfnJson(...args)) as {
            operation_id: string;
            max_events: number;
            max_bytes: number;
          };
          return stringifyWire(
            await providers.next(operation_id, max_events, max_bytes),
            undefined,
            'provider_events',
          );
        },
      }
    : undefined;

  const core = binding.open(
    stringifyWire(options, NATIVE_OPEN_OPTIONS_SHAPE, 'options'),
    callbacks,
  );
  return wrapCore(core);
}
