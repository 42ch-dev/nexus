import { useState } from 'react';

import {
  Spinner,
  LoadingState,
  EmptyState,
  UnavailableState,
  ErrorState,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for the promoted `<States>` family (V1.148 P1):
 * Spinner, LoadingState, EmptyState, UnavailableState, ErrorState.
 *
 * The Empty headline uses the serif display tier (content voice); ErrorState
 * sits on the token-backed error-surface fill/border pair (V1.121). The
 * ErrorState retry action is live so the callback path is visible.
 */
export function StatesFixtures() {
  const [retries, setRetries] = useState(0);
  const [unavailableRetries, setUnavailableRetries] = useState(0);

  return (
    <div data-testid="states-fixtures">
      <p className="text-label-14 text-gray-900 mb-4">Spinner</p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6 mb-6">
        <div className="flex items-center gap-4" data-testid="states-spinner">
          <Spinner />
          <span className="text-copy-13 text-gray-700 font-medium shrink-0 min-w-[80px]">
            Spinner
          </span>
        </div>
      </div>

      <p className="text-label-14 text-gray-900 mb-4">Loading</p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6 mb-6">
        <div data-testid="states-loading">
          <LoadingState label="Loading data…" />
        </div>
      </div>

      <p className="text-label-14 text-gray-900 mb-4">
        Empty — serif display headline (content voice)
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6 mb-6">
        <div data-testid="states-empty">
          <EmptyState
            title="No works yet"
            description="Create a Work to start the local loop."
          />
        </div>
      </div>

      <p className="text-label-14 text-gray-900 mb-4">
        Unavailable — engine idle, neutral tone with optional retry
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6 mb-6">
        <div data-testid="states-unavailable">
          <UnavailableState
            title="Orchestration engine is not running"
            description="Start the engine to run this workflow."
            onRetry={() => setUnavailableRetries((n) => n + 1)}
          />
        </div>
        {unavailableRetries > 0 && (
          <p
            data-testid="states-unavailable-retry-count"
            className="text-copy-13 text-gray-700 mt-4"
          >
            Retry requested {unavailableRetries}{' '}
            {unavailableRetries === 1 ? 'time' : 'times'}.
          </p>
        )}
      </div>

      <p className="text-label-14 text-gray-900 mb-4">
        Error — error-surface tokens + retry action
      </p>
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div data-testid="states-error">
          <ErrorState
            title="Could not load this view"
            description="The daemon returned an unexpected error."
            onRetry={() => setRetries((n) => n + 1)}
          />
        </div>
        {retries > 0 && (
          <p
            data-testid="states-error-retry-count"
            className="text-copy-13 text-gray-700 mt-4"
          >
            Retry requested {retries} {retries === 1 ? 'time' : 'times'}.
          </p>
        )}
      </div>
    </div>
  );
}
