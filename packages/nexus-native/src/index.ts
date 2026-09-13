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
  loadNativeBinding,
  readBundledCompatibility,
  readCompatibilityManifest,
  type NativeCoreBinding,
} from './loader.js';
import { parseJsonBuffer, stringifyToBuffer } from './json.js';


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
        await inner.worldKbCandidates(principal, worldId, limit ?? null, cursor ?? null),
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
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const bundled = readBundledCompatibility();
  assertCompatibility(manifest, bundled?.contract_tree_sha256);
  return manifest;
}

export async function openCore(
  options: NativeOpenOptions,
  providers?: ProviderCallbacks,
): Promise<NativeCore> {
  const binding = loadNativeBinding();
  const manifest = readCompatibilityManifest(binding);
  const bundled = readBundledCompatibility();
  assertCompatibility(manifest, bundled?.contract_tree_sha256);
  if (providers) {
    binding.registerProviderCallbacks({
      call: async (requestJson) => JSON.stringify(await providers.call(JSON.parse(requestJson))),
      next: async (requestJson) => {
        const { operation_id, max_events, max_bytes } = JSON.parse(requestJson) as {
          operation_id: string;
          max_events: number;
          max_bytes: number;
        };
        return JSON.stringify(
          await providers.next(operation_id, max_events, max_bytes),
        );
      },
    });
  }
  const core = await binding.open(JSON.stringify(options));
  return wrapCore(core);
}

