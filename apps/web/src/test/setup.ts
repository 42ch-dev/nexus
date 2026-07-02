/**
 * Vitest global setup — jest-dom matchers + msw server lifecycle.
 *
 * msw is the mock transport for BrowserClient fetch in component/integration
 * tests (R-V164-QC1-S1-P1 baseline). The server is started once, reset between
 * tests (so each test declares its own handlers), and stopped on teardown.
 */
import '@testing-library/jest-dom/vitest';

import { afterAll, afterEach, beforeAll } from 'vitest';

import { server } from './msw-server';

/**
 * Node 24+ may install an experimental `localStorage` shim that is undefined
 * unless `--localstorage-file` is set, which shadows jsdom's implementation.
 * ThemeProvider tests (and P2 logo wiring) need a working store.
 */
function ensureLocalStorage() {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem('__nexus_test__', '1');
    window.localStorage.removeItem('__nexus_test__');
    return;
  } catch {
    // Fall through to a minimal in-memory polyfill.
  }

  const store = new Map<string, string>();
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
      key: (index: number) => [...store.keys()][index] ?? null,
      get length() {
        return store.size;
      },
    },
  });
}

ensureLocalStorage();

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
