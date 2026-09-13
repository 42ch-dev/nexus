export class ProviderNextError extends Error {
  readonly code = 'not_found' as const;

  constructor(operationId: string) {
    super(`operation_not_found:${operationId}`);
    this.name = 'ProviderNextError';
  }
}
