/**
 * Pattern picker — the nest entry point (v239-p3-canvas-compose-ui T3).
 * Lists the read-only pattern registry; the chosen package is nested at the
 * attach node and the interaction wires the connecting edge (T3b).
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { ListPatternsResponse } from '@42ch/nexus-contracts';

import { Dialog, DialogContent } from '@/components/ui/dialog';

export interface PatternPickerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  patterns: ListPatternsResponse['items'];
  /** Why the listing could not be read; the picker says so instead of "no patterns". */
  error?: string | null;
  attachNodeId: string;
  isComposing: boolean;
  onNest: (target: { attachNodeId: string; package: { id: string; version: string } }) => Promise<void>;
}

export function PatternPickerDialog({
  open,
  onOpenChange,
  patterns,
  error,
  attachNodeId,
  isComposing,
  onNest,
}: PatternPickerDialogProps) {
  const { t } = useTranslation('canvas');
  const [selected, setSelected] = useState('');

  const available = patterns.filter((pattern) => pattern.available);
  const chosen = available.find(
    (pattern) => `${pattern.id}@${pattern.version}` === selected,
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t('strategy.picker.title')}>
        <p className="mt-2 text-copy-13 text-gray-900">
          {t('strategy.picker.attach', { node: attachNodeId })}
        </p>
        <ul className="mt-2 flex max-h-64 flex-col gap-1 overflow-y-auto">
          {available.map((pattern) => (
            <li key={`${pattern.id}@${pattern.version}`}>
              <button
                type="button"
                data-testid={`pattern-option-${pattern.id}`}
                aria-pressed={selected === `${pattern.id}@${pattern.version}`}
                className={`w-full rounded-card border px-3 py-2 text-left hover:bg-gray-alpha-100 ${
                  selected === `${pattern.id}@${pattern.version}`
                    ? 'border-purple-700'
                    : 'border-gray-alpha-400'
                }`}
                onClick={() => setSelected(`${pattern.id}@${pattern.version}`)}
              >
                <span className="font-mono text-copy-13 text-gray-1000">
                  {pattern.id}@{pattern.version}
                </span>
                <span className="ml-2 text-copy-13 text-gray-700">
                  {pattern.description ?? ''}
                </span>
              </button>
            </li>
          ))}
          {error ? (
            <li data-testid="pattern-picker-error" className="text-copy-13 text-red-700">
              {error}
            </li>
          ) : available.length === 0 ? (
            <li className="text-copy-13 text-gray-700">{t('strategy.picker.empty')}</li>
          ) : null}
        </ul>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            {t('strategy.picker.cancel')}
          </button>
          <button
            type="button"
            data-testid="pattern-nest"
            disabled={!chosen || isComposing}
            className="rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800 disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => {
              if (!chosen || !chosen.id || !chosen.version) return;
              void onNest({
                attachNodeId,
                package: { id: chosen.id, version: chosen.version },
              })
                .then(() => onOpenChange(false))
                // A refused nest is already presented by the hook's conflict/refusal
                // state; the picker only stays open for the retry.
                .catch(() => {});
            }}
          >
            {t('strategy.picker.nest')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
