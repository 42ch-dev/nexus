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
export { ProviderNextError } from './errors.js';
export { parseAdmittedRecipe } from './recipe.js';

export function createAcpProvider(): ProviderCallbacks {
  return createEngine();
}
