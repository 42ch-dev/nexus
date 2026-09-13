import type { Server } from 'node:http';
import { createAcpProvider } from '@42ch/nexus-provider-acp';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import { validateStartupBind } from './bind.js';
import { CLOSE_BUDGET_MS, loadTlsMaterial, resolveServiceConfig, type ServiceOptions } from './config.js';
import { closeServiceCore, openServiceCore } from './lifecycle.js';
import { createServiceServer, listenServer, type RunningService } from './server.js';

export type { ServiceOptions, RunningService };

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
  // Concurrent callers share one in-flight close. Only a confirmed native
  // `closed` report is cached: an `interrupted`/unconfirmed report must stay
  // retryable so a later close can settle the retained native owner.
  let inFlight: Promise<CoreCloseReport> | null = null;
  let confirmed: CoreCloseReport | null = null;

  const close = (): Promise<CoreCloseReport> => {
    if (confirmed) return Promise.resolve(confirmed);
    if (!inFlight) {
      inFlight = performClose().then((report) => {
        if (report.state === 'closed' && report.cleanup_confirmed) {
          confirmed = report;
        }
        return report;
      });
      // Clear the in-flight slot whether or not the close settled confirmed, so
      // the next caller starts a new attempt against the retained owner.
      inFlight = inFlight.finally(() => {
        inFlight = null;
      });
    }
    return inFlight;
  };

  /**
   * One absolute five-second budget for the whole teardown. Listener admission
   * stops immediately and remaining sockets are force-closed within the budget,
   * so a stuck request can never delay the native close. The native close owns
   * its own settlement and reports the real outcome (including retained ids).
   */
  async function performClose(): Promise<CoreCloseReport> {
    const deadline = Date.now() + CLOSE_BUDGET_MS;
    if (server?.listening) {
      await stopListening(server, Math.max(0, deadline - Date.now()));
    }
    return closeServiceCore(serviceCore.core);
  }

  try {
    const created = createServiceServer(config, serviceCore, close);
    server = created.server;
    await listenServer(server, config.host, config.port);
    return created.running;
  } catch (error) {
    await close();
    throw error;
  }
}

/**
 * Stop accepting connections and tear the listener down *now*: idle
 * connections close immediately and remaining sockets are force-closed, so an
 * in-flight request can never delay the native close that owns the budget.
 * The `budgetMs` timer is the hard backstop if a socket refuses to die.
 */
function stopListening(server: Server, budgetMs: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  const timer = setTimeout(resolve, budgetMs);
  server.close(() => {
    clearTimeout(timer);
    resolve();
  });
  server.closeIdleConnections();
  server.closeAllConnections();
  return promise;
}
