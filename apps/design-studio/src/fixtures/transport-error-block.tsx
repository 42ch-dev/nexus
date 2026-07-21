import { TransportErrorBlock, type TransportErrorKind } from '@42ch/nexus-ui';

/**
 * Studio fixture for `<TransportErrorBlock>` (V1.129 P1).
 *
 * Renders all six `TransportErrorKind` values with both callbacks supplied so
 * the full CTA visibility matrix is visible. The kind label appears above
 * each block so a reviewer can match the rendered copy to the matrix table.
 *
 * Boundary: imports only the promoted primitive from `@42ch/nexus-ui`. No
 * daemon, no routing, no product state, no `react-i18next`.
 */
const KIND_MATRIX: Array<{
  kind: TransportErrorKind;
  /** Short note explaining the CTA layout the matrix produces. */
  ctaNote: string;
}> = [
  { kind: 'daemon_down', ctaNote: 'Retry primary · no secondary' },
  { kind: 'http_fallback', ctaNote: 'Retry primary · no secondary' },
  { kind: 'network', ctaNote: 'Open Connection Settings primary · Retry secondary' },
  {
    kind: 'tls',
    ctaNote: 'No primary (body carries the desktop-app note) · Open Settings secondary',
  },
  { kind: 'timeout', ctaNote: 'Retry primary · Open Settings secondary' },
  { kind: 'unknown', ctaNote: 'Retry primary · Open Settings secondary' },
];

function noop() {
  /* Studio fixture only — clicks have nowhere to go. */
}

export function TransportErrorBlockFixtures() {
  return (
    <div data-testid="transport-error-block-fixtures" className="grid gap-6">
      <div data-testid="transport-error-block-full-matrix" className="grid gap-6">
        {KIND_MATRIX.map(({ kind, ctaNote }) => (
          <div key={kind} data-testid={`transport-error-block-row-${kind}`} className="space-y-2">
            <div className="flex items-baseline justify-between gap-4">
              <h4 className="text-heading-16 font-heading text-gray-1000">
                <code className="text-copy-13-mono rounded bg-gray-alpha-100 px-1 py-0.5">
                  {kind}
                </code>
              </h4>
              <p className="text-copy-13 text-gray-700">{ctaNote}</p>
            </div>
            <TransportErrorBlock
              kind={kind}
              onRetry={noop}
              onOpenSettings={noop}
            />
          </div>
        ))}
      </div>

      {/* Callback omission row — verifies "omit to hide CTA" contract. */}
      <div className="space-y-2">
        <h4 className="text-heading-16 font-heading text-gray-1000">
          Callback omission (toast-style: no CTAs)
        </h4>
        <p className="text-copy-13 text-gray-700">
          When the caller omits both <code className="text-copy-13-mono">onRetry</code> and{' '}
          <code className="text-copy-13-mono">onOpenSettings</code>, only headline + body render.
          The <code className="text-copy-13-mono">tls</code> kind never renders a primary button —
          its <em>Use Desktop App</em> instruction is informational and is carried by the body copy
          (QC1-F-002).
        </p>
        <div
          data-testid="transport-error-block-no-callbacks"
          className="grid gap-4 sm:grid-cols-2"
        >
          <TransportErrorBlock kind="daemon_down" />
          <TransportErrorBlock kind="tls" />
        </div>
      </div>

      {/* Detail line variant. */}
      <div className="space-y-2">
        <h4 className="text-heading-16 font-heading text-gray-1000">
          Optional detail line (caller-supplied)
        </h4>
        <TransportErrorBlock
          kind="daemon_down"
          detail="Last daemon exit code: 1 (subprocess crashed during boot)"
          onRetry={noop}
        />
      </div>
    </div>
  );
}
