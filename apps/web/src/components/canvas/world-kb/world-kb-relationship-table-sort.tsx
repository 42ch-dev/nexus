/**
 * Relationship table sort header (V1.74 A10 split).
 *
 * Extracted from `world-kb-relationship-table.tsx` (and the original utils
 * candidate) so the table stays under the 250-line cap and JSX lives in a
 * `.tsx` file.
 */
export type RelationshipTableSortKey =
  | 'source'
  | 'target'
  | 'type'
  | 'symmetric'
  | 'confidence'
  | 'anchors'
  | 'updated';

export type RelationshipTableSortDir = 'asc' | 'desc';

export function SortHeader({
  label,
  sortKey,
  current,
  dir,
  onToggle,
}: {
  label: string;
  sortKey: RelationshipTableSortKey;
  current: RelationshipTableSortKey;
  dir: RelationshipTableSortDir;
  onToggle: (key: RelationshipTableSortKey) => void;
}) {
  return (
    <th
      scope="col"
      aria-sort={current === sortKey ? (dir === 'asc' ? 'ascending' : 'descending') : 'none'}
      className="px-3 py-2 text-left text-label-12 text-gray-700"
    >
      <button
        type="button"
        onClick={() => onToggle(sortKey)}
        className="inline-flex items-center gap-1 hover:text-gray-1000"
      >
        {label}
        {current === sortKey ? <span aria-hidden>{dir === 'asc' ? '▲' : '▼'}</span> : null}
      </button>
    </th>
  );
}
