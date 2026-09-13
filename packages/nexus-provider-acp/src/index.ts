import type { ProviderCallbacks } from './contracts.js';
import { createEngine } from './acp.js';

export type { ProviderCallbacks } from './contracts.js';
export function createAcpProvider(): ProviderCallbacks {
  return createEngine();
}

export { createTestEngine, AcpProviderEngine } from './acp.js';
export { CleanupUnconfirmedError, ProviderNextError } from './errors.js';
export { parseAdmittedRecipe } from './recipe.js';
export {
  observeProcessIdentity,
  parseProcessIdentity,
  queryOsProcessIdentity,
  identitiesEqual,
  identityStillMatches,
  type ProcessIdentity,
} from './identity.js';
export { reapChild, spawnOwnedConnection } from './process-owner.js';
export {
  setCommandRunners,
  setPlatformOverride,
  platformOf,
  isWindows,
} from './platform-exec.js';
export {
  MAX_EVENT_BYTES,
  MAX_PENDING_MESSAGES,
  OperationDelivery,
} from './delivery.js';
