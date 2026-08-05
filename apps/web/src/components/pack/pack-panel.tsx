/**
 * Pack panel — Narrative Knowledge Pack export + import for one World
 * (V1.152 P1 T2–T4, DF-77).
 *
 * Two author actions, both pure UI over the T1 hooks:
 * - **Export**: `useExportPack().mutate({ worldId })` → the hook wraps the
 *   pack envelope in a `Blob` and triggers a `<world-title>.json` download.
 *   Loading + inline error states (403 ownership / generic).
 * - **Import**: file picker + conflict-policy selector + submit →
 *   `useImportPack().mutate({ worldId, file, conflict })`. When the policy
 *   is `overwrite` the submit is gated behind {@link OverwriteConfirmDialog}
 *   (the data-loss path — cancel aborts, confirm proceeds). On success the
 *   `PackImportResponse` renders via {@link PackImportResults}; errors show
 *   inline: client-side guards (no file, oversized), file-not-JSON parse
 *   failure, daemon 400 invalid pack / 403 ownership / 413 too large.
 *
 * A11y (brief T4): the file input is associated with a visible `<label>`
 * (htmlFor), every control is keyboard reachable, mutation errors use
 * `role="alert"`, export success uses `role="status"`, and the import
 * results mount inside the panel with their own live region (see
 * {@link PackImportResults}).
 */
import { useCallback, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useExportPack, useImportPack } from '@/api/queries';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { NexusClientError } from '@/lib/nexus';
import type { PackImportResponse } from '@42ch/nexus-contracts';

import { OverwriteConfirmDialog } from './overwrite-confirm-dialog';
import { PackImportResults } from './pack-import-results';

export type PackConflictPolicy = 'skip' | 'rename' | 'overwrite';

/**
 * Client-side oversized-file guard before upload. The daemon's axum JSON
 * extractor enforces a 2 MB default body limit; rejecting larger files in
 * the picker avoids a doomed upload and gives the author an actionable
 * message (plan error-states list: "oversized file (client-side guard
 * before upload)").
 */
export const MAX_PACK_FILE_BYTES = 2 * 1024 * 1024;

const CONFLICT_POLICIES: PackConflictPolicy[] = ['skip', 'rename', 'overwrite'];

export interface PackPanelProps {
  worldId: string;
}

export function PackPanel({ worldId }: PackPanelProps) {
  const { t } = useTranslation('pack');
  const uid = useId();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [file, setFile] = useState<File | null>(null);
  const [conflict, setConflict] = useState<PackConflictPolicy>('skip');
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [lastSummary, setLastSummary] = useState<PackImportResponse | null>(null);

  const exportPack = useExportPack();
  const importPack = useImportPack();

  const runImport = useCallback(() => {
    if (!file) return;
    importPack.mutate(
      { worldId, file, conflict },
      {
        onSuccess: (summary) => {
          setLastSummary(summary);
          // Reset the picker so selecting the same file again re-fires
          // onChange (native file inputs do not re-fire for the same file).
          setFile(null);
          setAttempted(false);
          if (fileInputRef.current) fileInputRef.current.value = '';
        },
      },
    );
  }, [file, conflict, importPack, worldId]);

  const handleConfirm = useCallback(() => {
    setConfirmOpen(false);
    runImport();
  }, [runImport]);

  const handleCancel = useCallback(() => setConfirmOpen(false), []);

  const submitImport = () => {
    setAttempted(true);
    if (!file || file.size > MAX_PACK_FILE_BYTES) return;
    if (conflict === 'overwrite') {
      setConfirmOpen(true);
      return;
    }
    runImport();
  };

  // Client-side validation failures (no file, oversized).
  const clientErrors: string[] = [];
  if (attempted && !file) clientErrors.push(t('import.errors.noFile'));
  if (attempted && file && file.size > MAX_PACK_FILE_BYTES) {
    clientErrors.push(t('import.errors.tooLarge', { size: MAX_PACK_FILE_BYTES / (1024 * 1024) }));
  }

  // Mutation failures: file-not-JSON (SyntaxError from the hook's JSON.parse),
  // daemon 400 invalid pack, 403 ownership, 413 too large, else the daemon
  // message with a generic fallback.
  const importErrorText = importPack.isError
    ? (() => {
        const error = importPack.error;
        if (error instanceof SyntaxError) return t('import.errors.invalidJson');
        if (error instanceof NexusClientError) {
          switch (error.status) {
            case 400:
              return t('import.errors.invalidPack');
            case 403:
              return t('import.errors.forbidden');
            case 413:
              return t('import.errors.tooLarge', {
                size: MAX_PACK_FILE_BYTES / (1024 * 1024),
              });
            default:
              return error.message || t('import.errors.generic');
          }
        }
        return error instanceof Error && error.message
          ? error.message
          : t('import.errors.generic');
      })()
    : null;

  const exportErrorText = exportPack.isError
    ? (() => {
        const error = exportPack.error;
        if (error instanceof NexusClientError && error.status === 403) {
          return t('export.errors.forbidden');
        }
        return error instanceof Error && error.message
          ? error.message
          : t('export.errors.generic');
      })()
    : null;

  const showImportErrors = clientErrors.length > 0 || importErrorText !== null;

  return (
    <section
      aria-labelledby={`${uid}-section-title`}
      className="flex flex-col gap-4 rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-elevation-2"
      data-testid="pack-panel"
    >
      <div className="flex flex-col gap-1">
        <h2 id={`${uid}-section-title`} className="text-heading-20 font-heading text-gray-1000">
          {t('section.title')}
        </h2>
        <p className="text-copy-13 text-gray-700">{t('section.description')}</p>
      </div>

      {/* ── Export ───────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2" data-testid="pack-export-section">
        <h3 className="text-label-14 font-semibold text-gray-900">{t('export.title')}</h3>
        <p className="text-copy-13 text-gray-700">{t('export.description')}</p>
        <div className="flex items-center gap-3">
          <Button
            type="button"
            variant="secondary"
            size="small"
            onClick={() => exportPack.mutate({ worldId })}
            disabled={exportPack.isPending}
            data-testid="pack-export-button"
          >
            {exportPack.isPending ? t('export.exporting') : t('export.button')}
          </Button>
          {exportPack.isSuccess ? (
            <p
              role="status"
              className="text-copy-13 text-gray-700"
              data-testid="pack-export-success"
            >
              {t('export.success')}
            </p>
          ) : null}
        </div>
        {exportErrorText ? (
          <p
            role="alert"
            className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
            data-testid="pack-export-error"
          >
            {exportErrorText}
          </p>
        ) : null}
      </div>

      {/* ── Import ───────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2" data-testid="pack-import-section">
        <h3 className="text-label-14 font-semibold text-gray-900">{t('import.title')}</h3>
        <p className="text-copy-13 text-gray-700">{t('import.description')}</p>

        <div className="flex flex-col gap-1">
          <Label htmlFor={`${uid}-file`}>{t('import.fileLabel')}</Label>
          <input
            id={`${uid}-file`}
            ref={fileInputRef}
            type="file"
            accept=".json,application/json"
            onChange={(e) => {
              const next = e.target.files?.[0] ?? null;
              setFile(next);
              setAttempted(false);
              // Results belong to the previously imported file — drop them
              // when the author picks a different one.
              setLastSummary(null);
              // A failed mutation keeps isError until reset — the new file
              // has not been submitted yet, so the stale error banner from
              // the previous attempt must not linger.
              importPack.reset();
            }}
            className="block w-full max-w-sm text-copy-13 text-gray-900 file:mr-3 file:rounded-control file:border-0 file:bg-gray-alpha-100 file:px-3 file:py-1.5 file:text-button-12 file:text-gray-1000 hover:file:bg-gray-alpha-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
            data-testid="pack-file-input"
          />
          <p className="text-copy-13 text-gray-700">{t('import.fileHint')}</p>
        </div>

        <div className="flex flex-col gap-1">
          <Label htmlFor={`${uid}-conflict`}>{t('import.conflictLabel')}</Label>
          <Select
            id={`${uid}-conflict`}
            value={conflict}
            onChange={(e) => setConflict(e.target.value as PackConflictPolicy)}
            className="w-64"
            data-testid="pack-conflict-select"
          >
            {CONFLICT_POLICIES.map((policy) => (
              <option key={policy} value={policy}>
                {t(`import.conflict.${policy}`)}
              </option>
            ))}
          </Select>
        </div>

        <div className="flex items-center gap-3">
          <Button
            type="button"
            variant="primary"
            size="small"
            onClick={submitImport}
            disabled={importPack.isPending}
            data-testid="pack-import-submit"
          >
            {importPack.isPending ? t('import.importing') : t('import.submit')}
          </Button>
        </div>

        {showImportErrors ? (
          <ul
            role="alert"
            className="flex flex-col gap-1 rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
            data-testid="pack-import-errors"
          >
            {clientErrors.map((err) => (
              <li key={err}>{err}</li>
            ))}
            {importErrorText ? <li>{importErrorText}</li> : null}
          </ul>
        ) : null}

        {lastSummary ? <PackImportResults summary={lastSummary} /> : null}
      </div>

      <OverwriteConfirmDialog
        open={confirmOpen}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </section>
  );
}
