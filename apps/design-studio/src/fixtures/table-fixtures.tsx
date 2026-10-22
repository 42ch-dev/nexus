import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@42ch/nexus-ui';

const ROWS = [
  { id: 'work-01', title: 'The Lost City', profile: 'Novel', status: 'Active' },
  { id: 'work-02', title: 'Echo Protocol', profile: 'Script', status: 'Archived' },
  { id: 'work-03', title: 'Starfall', profile: 'Novel', status: 'Draft' },
  {
    id: 'work-04-very-long-correlation-id-for-overflow',
    title:
      'A deliberately long work title that exercises horizontal scroll inside the table wrapper without forcing document-wide overflow',
    profile: 'Novel with an extended profile label',
    status: 'Active',
  },
];

/**
 * Studio Components fixture for the promoted `<Table>` primitives (V1.148 P1).
 *
 * Header uses the label-12 font, body copy-14, hover row highlighting, and a
 * local overflow-x-auto scroll container — wide rows scroll inside the card;
 * the page itself must not gain horizontal overflow.
 */
export function TableFixtures() {
  return (
    <div data-testid="table-fixtures">
      <div
        className="border border-gray-alpha-300 rounded-card bg-background-100 p-0 overflow-hidden"
        data-testid="table-overflow-wrapper"
      >
        <div className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Title</TableHead>
                <TableHead>Profile</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {ROWS.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="text-copy-13-mono text-gray-700 whitespace-nowrap">
                    {r.id}
                  </TableCell>
                  <TableCell className="min-w-[280px]">{r.title}</TableCell>
                  <TableCell className="whitespace-nowrap">{r.profile}</TableCell>
                  <TableCell>{r.status}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
        <p className="text-copy-13 text-gray-700 p-4">
          Row hover triggers background-200; header uses background-200 with
          bottom border gray-alpha-400. The last row carries long copy to
          prove local horizontal scroll.
        </p>
      </div>
    </div>
  );
}
