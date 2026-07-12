/**
 * Transition / edge inspector section — edits the scalar `next` transition.
 *
 * Owns its own save button and partial-failure UI (R-V171P0-QC1-004).
 */
import type { MutableRefObject } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQueryClient, type UseMutationResult } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import type { StrategyPatchResponse, StrategyPatchTransitionRequest } from '@42ch/nexus-contracts';

import { isStrategyConflictError } from '@/lib/canvas/use-strategy-data';
import { isSectionDirty, originalFormOf, type EditForm, type SaveStatus, type Section } from '../state-machine';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export interface PatchStrategyTransitionArgs {
  strategyId: string;
  sourceStateId: string;
  baseRevision: number;
  oldTarget: string;
  newTarget?: string;
  condition?: string;
  transitionKind?: StrategyPatchTransitionRequest['transition_kind'];
}

export function usePatchStrategyTransition(): UseMutationResult<
  StrategyPatchResponse,
  unknown,
  PatchStrategyTransitionArgs
> {
  const client = useNexusClient();
  const qc = useQueryClient();
  const { t } = useTranslation('canvas');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (args: PatchStrategyTransitionArgs) =>
      client.strategyPatchTransition(args.strategyId, {
        strategy_id: args.strategyId,
        base_revision: args.baseRevision,
        source_state_id: args.sourceStateId,
        old_target: args.oldTarget,
        new_target: args.newTarget,
        condition: args.condition,
        transition_kind: args.transitionKind,
      }),
    onSuccess: (_data, args) => {
      toast({
        variant: 'success',
        title: t('strategy.inspector.transition.updated'),
        description: `${args.sourceStateId} → ${args.newTarget ?? args.oldTarget}`,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(args.strategyId) });
    },
    onError: () => {},
  });
}

interface EdgeInspectorProps {
  presetId: string;
  selectedState: PresetState;
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  workingRevisionRef: MutableRefObject<number>;
  saveTrigger: number;
  saveStatus: SaveStatus | undefined;
  onSaveStatus: (status: SaveStatus | undefined) => void;
  onConflict: (currentRevision: number, section: Section) => void;
}

export function EdgeInspector({
  presetId,
  selectedState,
  form,
  onChange,
  workingRevisionRef,
  saveTrigger,
  saveStatus,
  onSaveStatus,
  onConflict,
}: EdgeInspectorProps) {
  const { t } = useTranslation('canvas');
  const patch = usePatchStrategyTransition();
  const original = originalFormOf(selectedState);
  const dirty = isSectionDirty('transition', form, original);
  const lastHandledTriggerRef = useRef(0);

  const handleSave = useCallback(async () => {
    if (!dirty || patch.isPending || typeof selectedState.next !== 'string') return;
    onSaveStatus(undefined);

    try {
      const res = await patch.mutateAsync({
        strategyId: presetId,
        sourceStateId: selectedState.id,
        baseRevision: workingRevisionRef.current,
        oldTarget: original.nextTarget,
        newTarget: form.nextTarget,
        transitionKind: 'next',
      });
      workingRevisionRef.current = Number(res.new_revision);
      onSaveStatus({ type: 'success', message: t('strategy.inspector.transition.saved') });
    } catch (error) {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        onConflict(currentRevision, 'transition');
      } else {
        const message = error instanceof Error ? error.message : t('strategy.inspector.transition.saveFailed');
        onSaveStatus({ type: 'error', message });
      }
    }
  }, [dirty, patch.isPending, form, original, presetId, selectedState, onSaveStatus, onConflict, t]);

  // Keep a fresh callback reference for the keyboard shortcut effect so the
  // effect itself does not need to depend on the callback (R-V172P1-QC1-001).
  const handleSaveRef = useRef(handleSave);
  handleSaveRef.current = handleSave;

  useEffect(() => {
    if (saveTrigger > 0 && saveTrigger !== lastHandledTriggerRef.current) {
      lastHandledTriggerRef.current = saveTrigger;
      void handleSaveRef.current();
    }
  }, [saveTrigger]);

  if (typeof selectedState.next !== 'string') return null;

  return (
    <section className="flex flex-col gap-2" aria-label={t('strategy.inspector.transition.ariaLabel')}>
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">{t('strategy.inspector.transition.title')}</span>
        <button
          type="button"
          onClick={handleSave}
          disabled={!dirty || patch.isPending}
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:text-gray-500"
        >
          {patch.isPending ? t('strategy.inspector.transition.saving') : t('strategy.inspector.transition.save')}
        </button>
      </div>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">{t('strategy.inspector.transition.targetLabel')}</span>
        <input
          type="text"
          value={form.nextTarget}
          onChange={(e) => onChange('nextTarget', e.target.value)}
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
        />
      </label>
      {saveStatus ? (
        <p
          className={
            saveStatus.type === 'success'
              ? 'text-copy-12 text-canvas-write-success'
              : 'text-copy-12 text-canvas-write-conflict'
          }
        >
          {saveStatus.message}
        </p>
      ) : null}
    </section>
  );
}

/**
 * Commit payload assembled by {@link DraftEdgeInspector}. Empty strings are
 * normalized to `undefined` so the wire request omits the field entirely
 * (matches the optional `condition` semantics on the daemon side).
 *
 * Note: the DTO (`StrategyPatchTransitionRequest`) has no `label` field — the
 * `condition` field is the branch identifier for conditional/labeled branches.
 * A Label input was previously collected and silently dropped (QC1 W-002);
 * it has been removed.
 */
export interface DraftCommitArgs {
  condition?: string;
}

export interface DraftEdgeInspectorProps {
  /** Source state id (read-only display). */
  sourceStateId: string;
  /** Target state id (read-only display). */
  targetStateId: string;
  /** True while the hook-owned commit mutation is in flight. */
  isCommitting: boolean;
  /** Commit the draft — sends `strategy.patch_transition` with `op: "create"`. */
  onCommit: (args: DraftCommitArgs) => void;
  /** Discard the draft edge without a daemon call. */
  onCancel: () => void;
}

/**
 * FB-SE-001 / FB-SE-002 — draft transition commit UI.
 *
 * Renders when a spatial `onConnect` draft edge is selected. Collects
 * Condition before commit; the commit routes through the hook-owned
 * mutation that sends `strategy.patch_transition` with **`op: "create"`**.
 * A 409 keeps the draft and opens the conflict modal (Use current / Reapply /
 * Review side-by-side). Voice & Content is locked by the primary spec.
 */
export function DraftEdgeInspector({
  sourceStateId,
  targetStateId,
  isCommitting,
  onCommit,
  onCancel,
}: DraftEdgeInspectorProps) {
  const { t } = useTranslation('canvas');
  const [condition, setCondition] = useState('');

  function handleCommit() {
    const args: DraftCommitArgs = {};
    if (condition.trim()) args.condition = condition.trim();
    onCommit(args);
  }

  return (
    <section className="flex flex-col gap-2" aria-label={t('strategy.edgeCreate.draftAriaLabel')}>
      <div className="flex items-center justify-between gap-2">
        <span className="text-label-14 font-semibold text-gray-900">{t('strategy.edgeCreate.title')}</span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onCancel}
            disabled={isCommitting}
            className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:text-gray-500"
          >
            {t('strategy.edgeCreate.cancel')}
          </button>
          <button
            type="button"
            onClick={handleCommit}
            disabled={isCommitting}
            className="rounded-control bg-purple-700 px-2 py-1 text-button-12 text-white hover:bg-purple-800 disabled:opacity-50"
          >
            {isCommitting ? t('strategy.edgeCreate.creating') : t('strategy.edgeCreate.submit')}
          </button>
        </div>
      </div>
      <p className="text-copy-13 text-gray-700">
        <span className="font-mono text-gray-1000">{sourceStateId}</span>
        <span className="mx-1" aria-hidden>
          →
        </span>
        <span className="font-mono text-gray-1000">{targetStateId}</span>
      </p>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">{t('strategy.edgeCreate.conditionLabel')}</span>
        <input
          type="text"
          value={condition}
          onChange={(e) => setCondition(e.target.value)}
          placeholder={t('strategy.edgeCreate.conditionPlaceholder')}
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
        />
      </label>
    </section>
  );
}
