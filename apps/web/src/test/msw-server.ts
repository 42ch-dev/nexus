/**
 * Shared msw server for component/integration tests.
 *
 * Per src/test/setup.ts the server listens for the whole suite and resets
 * handlers between tests. Individual tests call `server.use(...)` to register
 * the Daemon API routes they exercise against the BrowserClient.
 *
 * The daemon base URL is the same origin (relative paths) in the BrowserClient
 * default config, so handlers match relative `/v1/daemon/*` paths.
 */
import { http, HttpResponse, type RequestHandler } from 'msw';
import { setupServer } from 'msw/node';

/**
 * Shell-wide baseline: the saved agent profile is read on every mount (topnav
 * agent chip, Hub Agent card, onboarding restore). Tests that are not about the
 * profile get "no profile saved"; profile tests override this with their own
 * handler via `useHandlers` (later handlers win in msw).
 */
const baselineHandlers: RequestHandler[] = [
  http.get('/v1/daemon/agent-host/profile', () => HttpResponse.json({ profile: null })),
];

export const server = setupServer(...baselineHandlers);

/** Register handlers for a single test (replaces the previous test's handlers). */
export function useHandlers(...handlers: RequestHandler[]): void {
  server.use(...handlers);
}
