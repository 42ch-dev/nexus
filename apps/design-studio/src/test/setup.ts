/**
 * Vitest global setup for the Design Studio — jest-dom matchers only.
 *
 * The studio is a read-only gallery with no daemon transport, so we don't
 * wire msw. This setup mirrors apps/web conventions where applicable, but
 * stays minimal.
 */
import '@testing-library/jest-dom/vitest';

/**
 * Node 24+ may install an experimental `localStorage` shim that shadows
 * jsdom's implementation. ThemeProvider tests need a working store.
 */
function ensureLocalStorage() {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem('__nexus_studio_test__', '1');
    window.localStorage.removeItem('__nexus_studio_test__');
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
