import { useState } from 'react';

import {
  Button,
  ListStateContainer,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<ListStateContainer>` (V1.149 P1 promotion).
 *
 * The promoted four-branch list/query container (loading / error / empty /
 * ready) over the promoted States primitives. Copy is caller-owned literal
 * English; the retry and empty-action callbacks are live.
 */
export function ListStateContainerFixtures() {
  const [retries, setRetries] = useState(0);
  const [created, setCreated] = useState(0);

  return (
    <div className="grid gap-6" data-testid="list-state-container-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="grid gap-6 md:grid-cols-2">
          <div data-testid="list-state-loading">
            <ListStateContainer status="loading" loadingLabel="Loading workspaces…">
              {null}
            </ListStateContainer>
          </div>
          <div data-testid="list-state-error">
            <ListStateContainer
              status="error"
              errorTitle="Workspaces failed to load"
              retryLabel="Try again"
              retry={() => setRetries((n) => n + 1)}
            >
              {null}
            </ListStateContainer>
            <p className="text-copy-13 text-gray-700 mt-2">
              Retry fired:{' '}
              <span data-testid="list-state-retry-count">{retries}</span>
            </p>
          </div>
          <div data-testid="list-state-empty">
            <ListStateContainer
              status="empty"
              emptyTitle="No workspaces yet"
              emptyHint="Create your first workspace to get started."
              emptyAction={
                <Button
                  variant="primary"
                  size="small"
                  data-testid="list-state-empty-create"
                  onClick={() => setCreated((n) => n + 1)}
                >
                  Create workspace
                </Button>
              }
            >
              {null}
            </ListStateContainer>
            <p className="text-copy-13 text-gray-700 mt-2">
              Create fired:{' '}
              <span data-testid="list-state-create-count">{created}</span>
            </p>
          </div>
          <div data-testid="list-state-ready">
            <ListStateContainer status="ready" emptyTitle="Nothing here yet">
              <ul className="text-copy-14 text-gray-1000 list-disc pl-5">
                <li>Workspace Alpha</li>
                <li>Workspace Beta</li>
              </ul>
            </ListStateContainer>
          </div>
        </div>
        <p className="text-copy-13 text-gray-700 mt-6">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          ListStateContainer — the single four-branch container (loading /
          error / empty / ready) behind list/query surfaces. Children render
          only on ready; empty copy and the action slot are caller-owned.
        </p>
      </div>
    </div>
  );
}
