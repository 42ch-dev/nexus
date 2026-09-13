import type { Server } from 'node:http';
import { createAcpProvider } from '@42ch/nexus-provider-acp';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
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
  }

  const providers = config.domainOnly ? undefined : createAcpProvider();
  const serviceCore = await openServiceCore(config, providers);

  let server: Server | undefined;
  let closing: Promise<CoreCloseReport> | null = null;
  const close = async (): Promise<CoreCloseReport> => {
    if (!closing) {
      closing = (async () => {
        const deadline = Date.now() + CLOSE_BUDGET_MS;
        if (server?.listening) {
          await closeListener(server, Math.max(0, deadline - Date.now()));
        }
        return closeServiceCore(serviceCore.core, Math.max(0, deadline - Date.now()));
      })();
    }
    return closing;
  };

  const created = createServiceServer(config, serviceCore, close);
  server = created.server;
  try {
    await listenServer(server, config.host, config.port);
    return created.running;
  } catch (error) {
    await close();
    throw error;
  }
}

function closeListener(server: Server, budgetMs: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  const timer = setTimeout(() => {
    server.closeAllConnections();
    resolve();
  }, budgetMs);
  server.close(() => {
    clearTimeout(timer);
    resolve();
  });
  return promise;
}
