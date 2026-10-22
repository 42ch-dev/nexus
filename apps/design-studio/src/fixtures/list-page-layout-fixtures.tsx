import { useState } from 'react';

import {
  Button,
  Input,
  ListPageLayout,
  Select,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@42ch/nexus-ui';

const ROWS = [
  { name: 'Harbor light', owner: 'Ada', runs: 12 },
  { name: 'Night market', owner: 'Bo', runs: 4 },
];

/**
 * Studio Components fixture for `<ListPageLayout>` (V1.150 P1 promotion).
 *
 * The promoted list-page chrome (Card > header title/description + actions >
 * filters row > results > footer) behind the app's list pages. Filter
 * state, list data, and pagination stay caller-owned; the results region
 * composes the promoted Table. The refresh and load-more callbacks are live.
 */
export function ListPageLayoutFixtures() {
  const [refreshes, setRefreshes] = useState(0);
  const [loadedMore, setLoadedMore] = useState(0);

  return (
    <div data-testid="list-page-layout-fixtures">
      <ListPageLayout
        className="border-gray-alpha-300"
        title="Findings"
        description="All findings in the current workspace."
        actions={
          <Button
            variant="secondary"
            size="small"
            data-testid="list-page-refresh"
            onClick={() => setRefreshes((n) => n + 1)}
          >
            Refresh
          </Button>
        }
        filters={
          <>
            <div className="flex flex-col gap-1.5">
              <label htmlFor="list-page-filter-q" className="text-label-14 text-gray-700">
                Search
              </label>
              <Input id="list-page-filter-q" data-testid="list-page-filter-search" placeholder="Filter findings…" />
            </div>
            <div className="flex flex-col gap-1.5">
              <label htmlFor="list-page-filter-status" className="text-label-14 text-gray-700">
                Status
              </label>
              <Select id="list-page-filter-status" data-testid="list-page-filter-status" defaultValue="all">
                <option value="all">All</option>
                <option value="open">Open</option>
                <option value="closed">Closed</option>
              </Select>
            </div>
          </>
        }
        footer={
          <div className="flex items-center gap-3">
            <Button
              variant="secondary"
              size="small"
              data-testid="list-page-load-more"
              onClick={() => setLoadedMore((n) => n + 1)}
            >
              Load more
            </Button>
            <span className="text-copy-13 text-gray-700">
              Refreshed {refreshes}× · loaded more {loadedMore}×
            </span>
          </div>
        }
      >
        <Table data-testid="list-page-table">
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Owner</TableHead>
              <TableHead>Runs</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {ROWS.map((row) => (
              <TableRow key={row.name}>
                <TableCell>{row.name}</TableCell>
                <TableCell>{row.owner}</TableCell>
                <TableCell>{row.runs}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </ListPageLayout>
      <p className="text-copy-13 text-gray-700 mt-4">
        Promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        ListPageLayout — the shared list-page chrome over the promoted Card
        primitives. Copy via string props, visuals via slots; filter state,
        results, and pagination stay caller-owned.
      </p>
    </div>
  );
}
