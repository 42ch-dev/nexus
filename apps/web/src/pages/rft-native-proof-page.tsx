/**
 * P4-T3 development-only proof page (RFT-M1 browser vertical).
 *
 * Mounts the **existing** {@link WorldKbCanvas} against a real seeded World and
 * exposes a stable page-level driver contract on `window.__RFT_NATIVE_PROOF__`
 * so the real browser runner can exercise the generated `CoreSliceClient`
 * methods on the real {@link BrowserClient} in page context (create-on-absent,
 * core-changes watermark, provider session/operation/SSE). The runner then
 * observes the existing canvas — no invented provider UI and no canvas create
 * button are added here.
 *
 * Reached only when `import.meta.env.DEV && VITE_RFT_NATIVE_PROOF === '1'`
 * (the route is registered from a compile-time guarded lazy import in
 * `App.tsx`, so this module and its chunk are excluded from the production
 * route bundle). It is proof scaffolding, retired with the rest of the
 * development-only mount by RFT-11.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router';

import type {
  AgentHostListSessionsQuery,
  CoreChangesRequest,
  CreateSessionRequest,
  ExecuteOperationRequest,
  ProviderEventBatch,
  ProviderHostEvent,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus';
import type { NexusClient } from '@/lib/nexus';
import { NotFoundPage } from '@/pages/not-found-page';
import { WorldKbCanvas } from '@/components/canvas/world-kb/world-kb-canvas';
import { RFT_NATIVE_PROOF_MARKER } from './rft-native-proof-marker';

/** Control frame carried by the generated batch — same `$defs` alias as the client. */
type CoreStreamGap = NonNullable<ProviderEventBatch['gap']>;

/** One structured proof record. The runner also reads these over CDP. */
export interface RftProofEvidenceRecord {
  kind: string;
  at: string;
  detail: unknown;
}

/**
 * The stable driver contract the CDP runner consumes. Every method delegates to
 * the real {@link BrowserClient}; nothing here fabricates a wire shape.
 */
export interface RftNativeProofHandle {
  ready: true;
  worldId: string;
  client: NexusClient;
  evidence: RftProofEvidenceRecord[];
  record(kind: string, detail: unknown): void;
  createEntityOnAbsent(request: WorldKbPatchEntityRequest): Promise<unknown>;
  readGraph(): Promise<unknown>;
  readCandidates(): Promise<unknown>;
  readCoreChanges(request: CoreChangesRequest): Promise<unknown>;
  createAgentHostSession(request: CreateSessionRequest): Promise<unknown>;
  listAgentHostSessions(query?: AgentHostListSessionsQuery): Promise<unknown>;
  getAgentHostSession(sessionId: string): Promise<unknown>;
  shutdownAgentHostSession(sessionId: string): Promise<unknown>;
  executeAgentHostOperation(sessionId: string, request: ExecuteOperationRequest): Promise<unknown>;
  getAgentHostOperation(operationId: string): Promise<unknown>;
  cancelAgentHostOperation(operationId: string): Promise<unknown>;
  /** Drain the SSE iterable until terminal/abort, returning the observed values. */
  drainAgentHostEvents(
    sessionId: string,
    options?: { timeoutMs?: number },
  ): Promise<Array<ProviderHostEvent | CoreStreamGap>>;
}

declare global {
  interface Window {
    __RFT_NATIVE_PROOF__?: RftNativeProofHandle;
    /** Edit-loop marker mirrored from `rft-native-proof-marker.ts` (runner-owned). */
    __RFT_NATIVE_PROOF_MARKER__?: string;
  }
}

/**
 * True when the compile-time proof gate is active. Kept non-exported: it has no
 * external consumers, and exporting a non-component value from this module
 * breaks React Fast Refresh (the 30-edit DX loop triggers full invalidation and
 * drops the proof handle).
 */
function isRftNativeProofEnabled(): boolean {
  return import.meta.env.DEV && import.meta.env.VITE_RFT_NATIVE_PROOF === '1';
}

/** Terminal provider event names — the drain stops on the first one observed. */
const TERMINAL_EVENT_KEYS = ['OpFinished', 'OpFailed', 'SessionStopped'] as const;

export function RftNativeProofPage() {
  const [searchParams] = useSearchParams();
  const worldId = searchParams.get('world') ?? '';
  // A dedicated real BrowserClient (same class the app uses; same-origin via
  // the Vite `/v1/daemon` proxy → VITE_DAEMON_URL) so the runner drives the
  // transport directly and the canvas observes the resulting DB state.
  const client = useMemo(() => new BrowserClient(), []);
  const evidenceRef = useRef<RftProofEvidenceRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!worldId) return;
    window.__RFT_NATIVE_PROOF_MARKER__ = RFT_NATIVE_PROOF_MARKER;
    const handle: RftNativeProofHandle = {
      ready: true,
      worldId,
      client,
      evidence: evidenceRef.current,
      record(kind, detail) {
        evidenceRef.current.push({ kind, at: new Date().toISOString(), detail });
      },
      createEntityOnAbsent: (request) => client.worldKbPatchEntity(worldId, request),
      readGraph: () => client.getWorldKbGraph(worldId, { includeSuggested: true }),
      readCandidates: () => client.getWorldKbCandidates(worldId),
      readCoreChanges: (request) => client.getCoreChanges(request),
      createAgentHostSession: (request) => client.createAgentHostSession(request),
      listAgentHostSessions: (query) => client.listAgentHostSessions(query),
      getAgentHostSession: (sessionId) => client.getAgentHostSession(sessionId),
      shutdownAgentHostSession: (sessionId) => client.shutdownAgentHostSession(sessionId),
      executeAgentHostOperation: (sessionId, request) =>
        client.executeAgentHostOperation(sessionId, request),
      getAgentHostOperation: (operationId) => client.getAgentHostOperation(operationId),
      cancelAgentHostOperation: (operationId) => client.cancelAgentHostOperation(operationId),
      async drainAgentHostEvents(sessionId, options) {
        const controller = new AbortController();
        const timeoutMs = options?.timeoutMs ?? 20_000;
        const timer = setTimeout(() => controller.abort(), timeoutMs);
        const observed: Array<ProviderHostEvent | CoreStreamGap> = [];
        try {
          for await (const value of client.subscribeAgentHostEvents(
            sessionId,
            controller.signal,
          )) {
            observed.push(value);
            const isGap = 'resync_required' in value;
            const isTerminal =
              !isGap && TERMINAL_EVENT_KEYS.some((key) => key in value);
            if (isTerminal) break;
          }
        } finally {
          clearTimeout(timer);
        }
        return observed;
      },
    };
    window.__RFT_NATIVE_PROOF__ = handle;
    return () => {
      if (window.__RFT_NATIVE_PROOF__ === handle) {
        delete window.__RFT_NATIVE_PROOF__;
      }
    };
  }, [client, worldId]);

  useEffect(() => {
    function onError(event: ErrorEvent) {
      setError(event.message);
    }
    window.addEventListener('error', onError);
    return () => window.removeEventListener('error', onError);
  }, []);

  if (!isRftNativeProofEnabled()) return <NotFoundPage />;
  if (!worldId) {
    return (
      <div className="p-6 text-copy-14 text-gray-700" data-testid="rft-native-proof-missing-world">
        RFT native proof requires a real seeded World: append `?world=wld_...` to the URL.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-6" data-testid="rft-native-proof-page">
      <div className="text-label-12 text-gray-700">
        Development-only RFT-M1 browser proof · world {worldId}
        {error ? ` · error: ${error}` : ''}
      </div>
      <div className="text-label-12 text-gray-700" data-testid="rft-native-proof-marker">
        marker: {RFT_NATIVE_PROOF_MARKER}
      </div>
      <WorldKbCanvas worldId={worldId} watchCoreChanges />
    </div>
  );
}
