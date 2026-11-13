/**
 * Compose-path inspector for a selected `pattern_slot` node
 * (v239-p3-canvas-compose-ui T3): bind_slot writes, replace_subtree and
 * unnest. Binding inputs are per the pinned manifest's slot declarations;
 * a required slot with no binding names itself before approval can pass.
 *
 * A refused op (409/422) is presented by the hook's conflict/refusal state, so
 * each handler consumes its rejection instead of rethrowing it at the boundary.
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

export interface SlotInspectorProps {
  node: StrategyNodeData;
  patterns: { id: string | null; version: string | null; available: boolean; reason: string | null }[];
  isComposing: boolean;
  onBindSlot: (nestId: string, slot: string, value: unknown) => Promise<void>;
  onReplaceSubtree: (nestId: string, pkg: { id: string; version: string }) => Promise<void>;
  onUnnest: (nestId: string) => Promise<void>;
}

export function SlotInspector({
  node,
  patterns,
  isComposing,
  onBindSlot,
  onReplaceSubtree,
  onUnnest,
}: SlotInspectorProps) {
  const { t } = useTranslation('canvas');
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const nestId = node.nestId;
  if (!nestId) return null;

  const candidates = patterns.filter((pattern) => pattern.available);
  const replacement = drafts.__replace
    ? (JSON.parse(drafts.__replace) as { id: string; version: string })
    : undefined;

  return (
    <div className="flex flex-col gap-3" data-testid="slot-inspector">
      <h3 className="text-heading-16 font-heading text-gray-1000">
        {t('strategy.slot.title', { pattern: `${node.patternId}@${node.patternVersion}` })}
      </h3>

      {(node.slotRows ?? []).map((row) => (
        <div key={row.name} className="flex flex-col gap-1">
          <label className="text-label-12 text-gray-700" htmlFor={`slot-${row.name}`}>
            {row.name}
            {row.required ? ' *' : ''} ({row.kind})
          </label>
          <div className="flex items-center gap-1">
            <input
              id={`slot-${row.name}`}
              data-testid={`slot-input-${row.name}`}
              className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-copy-13 text-gray-1000"
              value={drafts[row.name] ?? (typeof row.value === 'string' ? row.value : '')}
              onChange={(event) =>
                setDrafts((prev) => ({ ...prev, [row.name]: event.target.value }))
              }
            />
            <button
              type="button"
              data-testid={`slot-bind-${row.name}`}
              disabled={isComposing || (drafts[row.name] ?? '') === ''}
              className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:cursor-not-allowed disabled:opacity-50"
              onClick={() => void onBindSlot(nestId, row.name, drafts[row.name]).catch(() => {})}
            >
              {t('strategy.slot.bind')}
            </button>
          </div>
        </div>
      ))}

      <div className="flex flex-col gap-1 border-t border-gray-alpha-400 pt-2">
        <label className="text-label-12 text-gray-700" htmlFor="slot-replace">
          {t('strategy.slot.replace')}
        </label>
        <div className="flex items-center gap-1">
          <select
            id="slot-replace"
            data-testid="slot-replace"
            className="w-full rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-copy-13 text-gray-1000"
            value={drafts.__replace ?? ''}
            onChange={(event) => setDrafts((prev) => ({ ...prev, __replace: event.target.value }))}
          >
            <option value="">{t('strategy.slot.replacePlaceholder')}</option>
            {candidates.map((pattern) => (
              <option
                key={`${pattern.id}@${pattern.version}`}
                value={JSON.stringify({ id: pattern.id, version: pattern.version })}
              >
                {pattern.id}@{pattern.version}
              </option>
            ))}
          </select>
          <button
            type="button"
            data-testid="slot-replace-commit"
            disabled={isComposing || !replacement}
            className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => replacement && void onReplaceSubtree(nestId, replacement).catch(() => {})}
          >
            {t('strategy.slot.replaceCommit')}
          </button>
        </div>
      </div>

      <button
        type="button"
        data-testid="slot-unnest"
        disabled={isComposing}
        className="self-start rounded-control border border-red-700 px-2 py-1 text-button-12 text-red-1000 hover:bg-red-100"
        onClick={() => void onUnnest(nestId).catch(() => {})}
      >
        {t('strategy.slot.unnest')}
      </button>
    </div>
  );
}
