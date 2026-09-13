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
  assertCompatibility,
  expectedPlatformPackage,
  loadNativeBinding,
  loadNodePath,
  readBundledCompatibility,
  readCompatibilityManifest,
  readPackageManifest,
  type NativeCoreBinding,
} from './loader.js';
import { parseJsonBuffer, stringifyToBuffer } from './json.js';
import { assertSafeInteger } from './validate.js';


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
        await inner.patchWorldKbEntity(principal, worldId, stringifyToBuffer(request)),
      );
    },
    async worldKbCandidates(principal, worldId, limit, cursor) {
      return parseJsonBuffer(
        await inner.worldKbCandidates(principal, worldId, assertSafeInteger(limit ?? null, "limit"), cursor ?? null),
      );
    },
    async hostQuery(request) {
      return parseJsonBuffer(await inner.hostQuery(stringifyToBuffer(request)));
    },
    async changes(principal, request) {
      return parseJsonBuffer(await inner.changes(principal, stringifyToBuffer(request)));
    },
    async providerCall(request) {
      return parseJsonBuffer(await inner.providerCall(stringifyToBuffer(request)));
    },
    async nextProviderEvents(operationId, maxEvents, maxBytes) {
      return parseJsonBuffer(await inner.nextProviderEvents(operationId, maxEvents, maxBytes));
    },
    async close() {
      return parseJsonBuffer(await inner.close());
    },
  };
}

export function nativeCompatibility(): NativeCompatibility {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const bundled = readBundledCompatibility(nodePath);
  const pkg = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(pkg.name);
  assertCompatibility(manifest, bundled, pkgManifest.version);
  return manifest;
}

export function openCore(options: NativeOpenOptions, providers?: ProviderCallbacks): NativeCore {
  const nodePath = loadNodePath();
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const bundled = readBundledCompatibility(nodePath);
  const pkg = expectedPlatformPackage();
  const pkgManifest = readPackageManifest(pkg.name);
  assertCompatibility(manifest, bundled, pkgManifest.version);
  const callbacks = providers
    ? {
        call: async (requestJson: string) =>
          JSON.stringify(await providers.call(JSON.parse(requestJson))),
        next: async (requestJson: string) => {
          const { operation_id, max_events, max_bytes } = JSON.parse(requestJson) as {
            operation_id: string;
            max_events: number;
            max_bytes: number;
          };
          return JSON.stringify(await providers.next(operation_id, max_events, max_bytes));
        },
      }
    : undefined;
  const core = binding.open(JSON.stringify(options), callbacks);
  return wrapCore(core);
}

