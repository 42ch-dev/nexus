import {
  DetailCard,
  FieldRow,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<DetailCard>` + `<FieldRow>`
 * (V1.150 P3 promotion).
 *
 * The promoted read-only detail family: DetailCard is the findings-page
 * detail Card chrome; FieldRow is the label/value row with shared
 * null-hide behavior (null / undefined / '' renders nothing).
 */
export function DetailPartsFixtures() {
  return (
    <div data-testid="detail-parts-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="grid gap-6 md:grid-cols-2">
          <DetailCard
            title="Run detail"
            description="Read-only summary of the selected run."
          >
            <FieldRow label="Status" value="Succeeded" />
            <FieldRow label="Duration" value="12.4s" />
            <FieldRow label="Correlation ID" value="run-9f31c2" />
            <FieldRow label="Preset" value="basic-combat" />
          </DetailCard>
          <DetailCard title="Sparse record">
            <FieldRow label="Status" value="Failed" />
            <FieldRow
              label="Error"
              value="Invocation schema rejected: unknown property 'agression'."
            />
            {/* Null-hide proof: these rows render nothing. */}
            <FieldRow label="Report" value={null} />
            <FieldRow label="Params" value={undefined} />
            <FieldRow label="Note" value="" />
            <p className="text-copy-13 text-gray-700" data-testid="detail-parts-null-note">
              Report / Params / Note rows are null/empty — FieldRow hides them,
              so only two rows render above.
            </p>
          </DetailCard>
        </div>
        <p className="text-copy-13 text-gray-700 mt-6">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          DetailCard + FieldRow — the read-only detail parts over the
          promoted Card primitives. FieldRow hides rows whose value is
          null/undefined/'' so callers can map sparse data without guarding
          each row.
        </p>
      </div>
    </div>
  );
}
