#!/usr/bin/env node
import { parseCliArgs } from './config.js';
import { SERVICE_READY_PREFIX } from './discovery.js';
import { startService } from './index.js';

/**
 * Service process boot (architecture §7): the transport host owns the boot
 * signal/server-task responsibilities moved here from the former Rust daemon
 * boot path. Exactly ONE stdout line is the machine-readable ready contract —
 * `NEXUS_SERVICE_READY <discovery-json>` after the record is published; every
 * human log goes to stderr.
 */
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
    transport: args.transport,
    socketPath: args.socketPath,
    cdnUrl: args.cdnUrl,
    embeddedMcp: args.embeddedMcp,
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

  process.stdout.write(`${SERVICE_READY_PREFIX}${JSON.stringify(running.discovery)}\n`);
  const where = running.url ?? `unix:${running.endpoint.transport === 'unix' ? running.endpoint.path : ''}`;
  console.error(`[nexus-service] listening on ${where}`);
}

main().catch((error) => {
  console.error('[nexus-service] failed to start:', error instanceof Error ? error.message : error);
  process.exit(1);
});
