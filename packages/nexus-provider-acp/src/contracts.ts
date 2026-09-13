import type {
  ProviderCall,
  ProviderEventBatch,
  ProviderHostEvent,
  ProviderReply,
} from '@42ch/nexus-contracts';

/** Public provider callback contract; shape matches `@42ch/nexus-native` ProviderCallbacks. */
export interface ProviderCallbacks {
  call(request: ProviderCall): Promise<ProviderReply>;
  next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch>;
}

export type ProviderError = NonNullable<ProviderReply['error']>;
export type CoreStreamGap = NonNullable<ProviderEventBatch['gap']>;
export type { ProviderHostEvent };
