export class ProviderNextError extends Error {
  readonly code = 'not_found' as const;

  constructor(operationId: string) {
    super(`operation_not_found:${operationId}`);
    this.name = 'ProviderNextError';
  }
}

/** Raised when owned-child cleanup cannot be confirmed; carries the owner for fencing. */
export class CleanupUnconfirmedError extends Error {
  readonly code = 'cleanup_unconfirmed' as const;
  readonly owner: import('./process-owner.js').OwnedConnection | null;
  readonly cause: unknown;

  constructor(
    message: string,
    owner: import('./process-owner.js').OwnedConnection | null = null,
    cause?: unknown,
  ) {
    super(message);
    this.name = 'CleanupUnconfirmedError';
    this.owner = owner;
    this.cause = cause;
  }
}
