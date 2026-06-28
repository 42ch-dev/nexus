/**
 * Transition / edge inspector section — edits the scalar `next` transition.
 *
 * Owns its own save button and partial-failure UI (R-V171P0-QC1-004).
 */
import type { MutableRefObject } from 'react';
import { useCallback, useEffect, useRef } from 'react';
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
        title: 'Transition updated',
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
      onSaveStatus({ type: 'success', message: 'Saved transition' });
    } catch (error) {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        onConflict(currentRevision, 'transition');
      } else {
        const message = error instanceof Error ? error.message : 'Failed to save transition';
        onSaveStatus({ type: 'error', message });
      }
    }
  }, [dirty, patch.isPending, form, original, presetId, selectedState, patch, onSaveStatus, onConflict]);

  useEffect(() => {
    if (saveTrigger > 0 && saveTrigger !== lastHandledTriggerRef.current) {
      lastHandledTriggerRef.current = saveTrigger;
      void handleSave();
    }
  }, [saveTrigger, handleSave]);

  if (typeof selectedState.next !== 'string') return null;

  return (
    <section className="flex flex-col gap-2" aria-label="Transition editor">
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">Transition</span>
        <button
          type="button"
          onClick={handleSave}
          disabled={!dirty || patch.isPending}
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:text-gray-500"
        >
          {patch.isPending ? 'Saving…' : 'Save transition'}
        </button>
      </div>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">Transition target</span>
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
