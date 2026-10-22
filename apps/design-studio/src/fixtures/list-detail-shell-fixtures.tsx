import { useState } from 'react';

import {
  Button,
  EmptyState,
  ListDetailShell,
} from '@42ch/nexus-ui';

const ITEMS = [
  { id: 'alpha', label: 'Finding Alpha' },
  { id: 'beta', label: 'Finding Beta' },
];

/**
 * Studio Components fixture for `<ListDetailShell>` (V1.150 P3 promotion).
 *
 * The promoted master/detail split (stacks below lg; 360px detail column at
 * lg). The shell owns no open/close state — selection is caller-owned and
 * the caller renders the detail panel or a "no selection" EmptyState in the
 * detail region.
 */
export function ListDetailShellFixtures() {
  const [selectedId, setSelectedId] = useState<string | null>('alpha');

  const selected = ITEMS.find((item) => item.id === selectedId) ?? null;

  return (
    <div data-testid="list-detail-shell-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <ListDetailShell
          list={
            <ul className="flex flex-col gap-2">
              {ITEMS.map((item) => (
                <li key={item.id}>
                  <Button
                    variant={selectedId === item.id ? 'primary' : 'secondary'}
                    size="small"
                    data-testid={`list-detail-item-${item.id}`}
                    onClick={() => setSelectedId(item.id)}
                  >
                    {item.label}
                  </Button>
                </li>
              ))}
              <li>
                <Button
                  variant="tertiary"
                  size="small"
                  data-testid="list-detail-clear"
                  onClick={() => setSelectedId(null)}
                >
                  Clear selection
                </Button>
              </li>
            </ul>
          }
          detail={
            selected ? (
              <div
                data-testid="list-detail-panel"
                className="border border-gray-alpha-200 rounded-card bg-background-100 p-4"
              >
                <p className="text-heading-16 font-semibold text-gray-1000">
                  {selected.label}
                </p>
                <p className="text-copy-14 text-gray-700 mt-1">
                  Caller-owned detail panel for the selected item.
                </p>
              </div>
            ) : (
              <EmptyState
                title="No item selected"
                description="Select an item from the list to inspect it."
              />
            )
          }
        />
        <p className="text-copy-13 text-gray-700 mt-6">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          ListDetailShell — the fixed-width responsive master/detail split
          behind findings/modules pages. Selection state is caller-owned; the
          caller renders the detail panel or a "no selection" EmptyState.
        </p>
      </div>
    </div>
  );
}
