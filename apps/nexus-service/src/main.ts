#!/usr/bin/env node
import { parseCliArgs } from './config.js';
import { startService } from './index.js';

async function main(): Promise<void> {
  const args = parseCliArgs(process.argv.slice(2));
  const running = await startService({
    home: args.home,
    host: args.host,
    port: args.port,
    allowRemote: args.allowRemote,
    domainOnly: args.domainOnly,
    tlsCert: args.tlsCert,
    tlsKey: args.tlsKey,
  });

  let shuttingDown = false;
  const shutdown = async (signal: string) => {
    if (shuttingDown) return;
    shuttingDown = true;
    const report = await running.close();
    const code = report.cleanup_confirmed ? 0 : 1;
    console.error(`[nexus-service] ${signal} close state=${report.state} confirmed=${report.cleanup_confirmed}`);
    process.exit(code);
  };

  process.on('SIGINT', () => {
    void shutdown('SIGINT');
  });
  process.on('SIGTERM', () => {
    void shutdown('SIGTERM');
  });

  console.log(`[nexus-service] listening on ${running.url}`);
}

main().catch((error) => {
  console.error('[nexus-service] failed to start:', error instanceof Error ? error.message : error);
  process.exit(1);
});
