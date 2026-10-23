/**
 * Presentational body of the Validate Preset dialog (`POST /v1/daemon/presets:validate`).
 *
 * The product dialog owns the mutation, the path state and i18n; this module
 * owns the markup so the Design Studio fixture can render the same tree with
 * static copy in every state (pending, success, success with warnings, errors).
 *
 * Boundary: no transport, no i18n, no QueryClient — every string arrives as a
 * prop. Warnings are INDEPENDENT of validity: the core derives `valid` from
 * errors alone and reports warnings alongside it, so a valid preset with
 * warnings renders both blocks.
 */
import { AlertCircle, AlertTriangle, CheckCircle2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
// Subpath imports: the Design Studio's tsconfig aliases `@/components/ui/*`
// (not the barrel) to apps/web, so the fixture resolves the same modules.
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

export interface ValidatePresetCopy {
  pathLabel: string;
  pathPlaceholder: string;
  helpText: string;
  close: string;
  submit: string;
  submitting: string;
  valid: string;
  safeToCommit: string;
  validationFailed: string;
  warnings: string;
}

/** The wire result, reduced to what the body renders. */
export interface ValidatePresetResultView {
  valid: boolean;
  errors: readonly string[];
  warnings?: readonly string[];
  /** Localized " · N states" tail for the success line (empty when unknown). */
  stateSuffix?: string;
}

export interface ValidatePresetDialogChromeProps {
  copy: ValidatePresetCopy;
  path: string;
  onPathChange: (value: string) => void;
  pending: boolean;
  /** `null` before the first submission. */
  result: ValidatePresetResultView | null;
  onClose: () => void;
  onSubmit: () => void;
}

function WarningList({ title, warnings }: { title: string; warnings: readonly string[] }) {
  return (
    <div
      role="status"
      className="flex items-start gap-2 rounded-card border border-warning-surface-border bg-warning-surface p-3"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-700" aria-hidden />
      <div className="flex flex-col gap-1">
        <p className="text-label-14 font-medium text-amber-1000">{title}</p>
        <ul className="list-disc pl-4 text-copy-13 text-amber-900">
          {warnings.map((warning, index) => (
            <li key={index}>{warning}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function ValidationResultView({
  copy,
  result,
}: {
  copy: ValidatePresetCopy;
  result: ValidatePresetResultView;
}) {
  const warnings = result.warnings ?? [];
  const warningsBlock =
    warnings.length > 0 ? <WarningList title={copy.warnings} warnings={warnings} /> : null;

  if (result.valid && result.errors.length === 0) {
    return (
      <div className="flex flex-col gap-3">
        <div
          role="status"
          className="flex items-start gap-2 rounded-card border border-success-surface-border bg-success-surface p-3"
        >
          <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-green-700" aria-hidden />
          <div className="flex flex-col gap-1">
            <p className="text-label-14 font-medium text-green-1000" data-testid="preset-valid">
              {copy.valid}
            </p>
            <p className="text-copy-13 text-green-900">
              {copy.safeToCommit}
              {result.stateSuffix ?? ''}.
            </p>
          </div>
        </div>
        {warningsBlock}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div
        role="alert"
        className="flex items-start gap-2 rounded-card border border-error-surface-border bg-error-surface p-3"
      >
        <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-700" aria-hidden />
        <div className="flex flex-col gap-1">
          <p className="text-label-14 font-medium text-red-1000" data-testid="preset-invalid">
            {copy.validationFailed}
          </p>
          <ul className="list-disc pl-4 text-copy-13 text-red-900">
            {result.errors.map((error, index) => (
              <li key={index}>{error}</li>
            ))}
          </ul>
        </div>
      </div>
      {warningsBlock}
    </div>
  );
}

export function ValidatePresetDialogChrome({
  copy,
  path,
  onPathChange,
  pending,
  result,
  onClose,
  onSubmit,
}: ValidatePresetDialogChromeProps) {
  return (
    <form
      className="flex flex-col gap-4"
      data-testid="validate-preset-body"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="preset-path">{copy.pathLabel}</Label>
        <Input
          id="preset-path"
          value={path}
          onChange={(event) => onPathChange(event.target.value)}
          placeholder={copy.pathPlaceholder}
          className="text-copy-13-mono"
          autoFocus
        />
        <p className="text-copy-13 text-gray-700">{copy.helpText}</p>
      </div>

      {result && <ValidationResultView copy={copy} result={result} />}

      <div className="flex justify-end gap-2 pt-2">
        <Button type="button" variant="tertiary" size="small" onClick={onClose}>
          {copy.close}
        </Button>
        <Button type="submit" variant="primary" size="small" disabled={!path.trim() || pending}>
          {pending ? copy.submitting : copy.submit}
        </Button>
      </div>
    </form>
  );
}
