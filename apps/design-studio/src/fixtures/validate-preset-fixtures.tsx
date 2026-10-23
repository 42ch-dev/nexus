import { useState } from 'react';

import {
  ValidatePresetDialogChrome,
  type ValidatePresetCopy,
  type ValidatePresetResultView,
} from '@web-harness/validate-preset-dialog-chrome';

/**
 * Studio fixture for the Validate Preset dialog body (Harness catalog rows).
 *
 * The product dialog (`apps/web/src/pages/dialogs/validate-preset-dialog.tsx`)
 * owns the mutation and i18n; this gallery renders the same presentational body
 * with literal English copy in every result state, so light + dark can be
 * checked side by side with the Studio theme toggle.
 *
 * States: pending (submission in flight), success (no warnings), success WITH
 * warnings (the core derives `valid` from errors alone, so warnings and a valid
 * verdict coexist), and validation failure.
 *
 * Boundary: no transport, no i18n, no QueryClient, no daemon.
 */

const COPY: ValidatePresetCopy = {
  pathLabel: 'Preset path',
  pathPlaceholder: 'Path to the preset file on disk',
  helpText: 'Resolved by the daemon against the local home layout.',
  close: 'Close',
  submit: 'Validate',
  submitting: 'Validating preset…',
  valid: 'Preset is valid',
  safeToCommit: 'Safe to commit',
  validationFailed: 'Validation failed',
  warnings: 'Warnings',
};

const PRESET_PATH = '/home/author/.nexus42/presets/novel-writing/preset.yaml';

function StateFrame({
  title,
  description,
  testId,
  pending = false,
  result,
}: {
  title: string;
  description: string;
  testId: string;
  pending?: boolean;
  result?: ValidatePresetResultView;
}) {
  const [path, setPath] = useState(PRESET_PATH);

  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      <div className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-elevation-2">
        <ValidatePresetDialogChrome
          copy={COPY}
          path={path}
          onPathChange={setPath}
          pending={pending}
          result={result ?? null}
          onClose={() => undefined}
          onSubmit={() => undefined}
        />
      </div>
    </div>
  );
}

export function ValidatePresetFixtures() {
  return (
    <div data-testid="validate-preset-fixtures">
      <StateFrame
        title="Pending"
        description="Dry-run in flight — the submit control is disabled and reads “Validating preset…” while the request runs."
        testId="validate-preset-fixture-pending"
        pending
      />
      <StateFrame
        title="Success"
        description="Every structural and semantic check passed; the state count is echoed into the success line."
        testId="validate-preset-fixture-success"
        result={{ valid: true, errors: [], warnings: [], stateSuffix: ' · 3 states' }}
      />
      <StateFrame
        title="Success with warnings"
        description="Validity is derived from errors alone, so warnings render BESIDE the success block instead of replacing it."
        testId="validate-preset-fixture-warning"
        result={{
          valid: true,
          errors: [],
          warnings: [
            'states[1].enter[0]: capability "narrative.compute" has no inputs declared',
            'preset.run_intents is empty — the preset only runs on a direct trigger',
          ],
          stateSuffix: ' · 2 states',
        }}
      />
      <StateFrame
        title="Validation failed"
        description="Structural or semantic problems block the preset; each diagnostic is listed verbatim."
        testId="validate-preset-fixture-error"
        result={{
          valid: false,
          errors: [
            'preset.initial: state "start" is not declared',
            'states[0].next: unknown target "finish"',
          ],
          warnings: [],
          stateSuffix: '',
        }}
      />
    </div>
  );
}
