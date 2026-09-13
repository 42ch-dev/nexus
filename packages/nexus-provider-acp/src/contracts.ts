import type { ProviderEventBatch, ProviderHostEvent, ProviderReply } from '@42ch/nexus-contracts';

export type ProviderError = NonNullable<ProviderReply['error']>;
export type CoreStreamGap = NonNullable<ProviderEventBatch['gap']>;
export type { ProviderHostEvent };
