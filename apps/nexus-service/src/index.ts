import type { Server } from 'node:http';
import { createAcpProvider } from '@42ch/nexus-provider-acp';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import { validateStartupBind } from './bind.js';
import { CLOSE_BUDGET_MS, formatHttpAuthority, loadTlsMaterial, resolveServiceConfig, type ServiceOptions } from './config.js';
import { closeServiceCore, openServiceCore } from './lifecycle.js';
import { createServiceServer, listenServer, type RunningService } from './server.js';

export type { ServiceOptions, RunningService };

/** A confirmed close is final: nothing is retained, so later calls are idempotent. */
function isConfirmedClosed(report: CoreCloseReport): boolean {
  return report.state === 'closed' && report.cleanup_confirmed;
}

export interface CloseOwnerOptions {
  getServer: () => Server | undefined;
  closeCore: () => Promise<CoreCloseReport>;
  /** Total wall-clock budget for one close attempt. */
  budgetMs?: number;
  /** Listener teardown strategy (defaults to {@link stopListening}). */
  teardownListener?: (server: Server, deadlineMs: number) => Promise<void>;
}

/**
 * The single close owner for one service instance.
 *
 * One absolute wall-clock budget covers the whole teardown. Listener teardown
 * (admission stop + socket force-close) and the native close both start in the
 * same turn and are awaited together, so the worst case is one budget rather
 * than a listener wait followed by a fresh native budget.
 *
 * Only a confirmed `closed` report is cached. An `interrupted`/unconfirmed
 * report clears the in-flight slot, so a later call genuinely retries the
 * retained native owner; concurrent callers meanwhile share the one attempt.
 */
export function createCloseOwner(options: CloseOwnerOptions): () => Promise<CoreCloseReport> {
  const budgetMs = options.budgetMs ?? CLOSE_BUDGET_MS;
  const teardown = options.teardownListener ?? stopListening;
  let inFlight: Promise<CoreCloseReport> | null = null;
  let confirmed: CoreCloseReport | null = null;

  return (): Promise<CoreCloseReport> => {
    if (confirmed) return Promise.resolve(confirmed);
    if (!inFlight) {
      const attempt = (async (): Promise<CoreCloseReport> => {
        const deadline = Date.now() + budgetMs;
        const server = options.getServer();
        // Ordering matters: stop accepts synchronously, then start the native
        // close in the same turn. Awaiting the listener first would spend the
        // budget twice.
        const listener = server?.listening ? teardown(server, deadline) : Promise.resolve();
        const native = options.closeCore();
        const [report] = await Promise.all([native, listener]);
        return report;
      })();
      inFlight = attempt
        .then((report) => {
          if (isConfirmedClosed(report)) confirmed = report;
          return report;
        })
        .finally(() => {
          inFlight = null;
        });
    }
    return inFlight;
  };
}

export async function startService(options: ServiceOptions): Promise<RunningService> {
  const config = resolveServiceConfig(options);
  if (config.tlsCert && config.tlsKey) {
    const material = loadTlsMaterial(config.tlsCert, config.tlsKey);
    config.tlsCert = material.cert;
    config.tlsKey = material.key;
    config.tlsCertMtimeMs = material.certMtimeMs;
  }

  validateStartupBind(config);

  const providers = config.domainOnly ? undefined : createAcpProvider();
  const serviceCore = await openServiceCore(config, providers);

  let server: Server | undefined;
  const close = createCloseOwner({
    getServer: () => server,
    closeCore: () => closeServiceCore(serviceCore.core),
  });

  try {
    const created = createServiceServer(config, serviceCore, close);
    server = created.server;
    await listenServer(server, config.host, config.port);
    const bound = server.address();
    if (bound && typeof bound === 'object') {
      const protocol = config.tlsCert && config.tlsKey ? 'https' : 'http';
      return {
        url: `${protocol}://${formatHttpAuthority(config.host, bound.port)}`,
        close: created.running.close,
      };
    }
    return created.running;
  } catch (error) {
    await close();
    throw error;
  }
}

/**
 * Stop admitting and tear the listener down *now*: `server.close()` and the
 * connection force-close run synchronously, so an in-flight request can never
 * delay the native close that owns the budget. `deadlineMs` is the absolute
 * wall-clock bound; the timer is only a backstop for a socket that refuses to
 * die, which is why this Promise always settles within the shared budget.
 */
export function stopListening(server: Server, deadlineMs: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  const timer = setTimeout(resolve, Math.max(0, deadlineMs - Date.now()));
  server.close(() => {
    clearTimeout(timer);
    resolve();
  });
  server.closeIdleConnections();
  server.closeAllConnections();
  return promise;
}
