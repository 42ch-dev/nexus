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

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
