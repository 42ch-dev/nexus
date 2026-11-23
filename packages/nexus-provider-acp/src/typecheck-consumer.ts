import type { ProviderCallbacks } from './index.js';
import { createAcpProvider } from './index.js';

const provider: ProviderCallbacks = createAcpProvider();
void provider.call;
void provider.next;
