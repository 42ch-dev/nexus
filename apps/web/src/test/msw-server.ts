/**
 * Shared msw server for component/integration tests.
 *
 * Per src/test/setup.ts the server listens for the whole suite and resets
 * handlers between tests. Individual tests call `server.use(...)` to register
 * the Local API routes they exercise against the BrowserClient.
 *
 * The daemon base URL is the same origin (relative paths) in the BrowserClient
 * default config, so handlers match relative `/v1/local/*` paths.
 */
import { setupServer, type HttpHandler } from 'msw/node';

export const server = setupServer();

/** Register handlers for a single test (replaces the previous test's handlers). */
export function useHandlers(...handlers: HttpHandler[]): void {
  server.use(...handlers);
}
