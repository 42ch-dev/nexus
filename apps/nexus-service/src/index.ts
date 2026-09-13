import type { Server } from 'node:http';
import { createAcpProvider } from '@42ch/nexus-provider-acp';
import type { CoreCloseReport } from '@42ch/nexus-contracts';
import { validateStartupBind } from './bind.js';
import { loadTlsMaterial, resolveServiceConfig, type ServiceOptions } from './config.js';
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
  let closing: Promise<CoreCloseReport> | null = null;
  const close = async (): Promise<CoreCloseReport> => {
    if (!closing) {
      closing = (async () => {
        if (server?.listening) {
          await new Promise<void>((resolve) => {
            server!.close(() => resolve());
          });
        }
        return closeServiceCore(serviceCore.core);
      })();
    }
    return closing;
  };

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
