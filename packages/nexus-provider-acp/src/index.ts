import type { ProviderCall, ProviderEventBatch, ProviderReply } from '@42ch/nexus-contracts';
import { createEngine } from './acp.js';

export interface ProviderCallbacks {
  call(request: ProviderCall): Promise<ProviderReply>;
  next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch>;
}

export {
  OperationDelivery,
  MAX_EVENT_BYTES,
  MAX_PENDING_MESSAGES,
} from './delivery.js';
export { CleanupUnconfirmedError, ProviderNextError } from './errors.js';
export { observeProcessIdentity, parseProcessIdentity, type ProcessIdentity } from './identity.js';
export { parseAdmittedRecipe } from './recipe.js';

export function createAcpProvider(): ProviderCallbacks {
  return createEngine();
}

export { createTestEngine, AcpProviderEngine } from './acp.js';
export { reapChild } from './process-owner.js';
