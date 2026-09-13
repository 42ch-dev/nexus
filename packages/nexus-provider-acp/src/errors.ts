export class ProviderNextError extends Error {
  readonly code = 'not_found' as const;

  constructor(operationId: string) {
    super(`operation_not_found:${operationId}`);
    this.name = 'ProviderNextError';
  }
}

/** Raised when owned-child cleanup cannot be confirmed; callers must retain the fence. */
export class CleanupUnconfirmedError extends Error {
  readonly code = 'cleanup_unconfirmed' as const;

  constructor(message: string) {
    super(message);
    this.name = 'CleanupUnconfirmedError';
  }
}
