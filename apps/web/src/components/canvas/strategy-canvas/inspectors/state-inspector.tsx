/**
 * State inspector section — edits the state label and description.
 *
 * Owns its own save button and partial-failure UI (R-V171P0-QC1-004).
 */
import type { MutableRefObject } from 'react';
import { useCallback, useEffect, useRef } from 'react';
import { useMutation, useQueryClient, type UseMutationResult } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import type { StrategyPatchResponse, StrategyPatchStateRequest } from '@42ch/nexus-contracts';

import { isStrategyConflictError } from '@/lib/canvas/use-strategy-data';
import { isSectionDirty, originalFormOf, type EditForm, type SaveStatus, type Section } from '../state-machine';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export interface PatchStrategyStateArgs {
  strategyId: string;
  stateId: string;
  baseRevision: number;
  label?: string;
  description?: string;
}

function buildPatchStateRequest(args: PatchStrategyStateArgs): StrategyPatchStateRequest {
  const set: StrategyPatchStateRequest['set'] = {};
  if (args.label !== undefined) set.label = args.label;
  if (args.description !== undefined) set.description = args.description;
  return {
    strategy_id: args.strategyId,
    state_id: args.stateId,
    base_revision: args.baseRevision,
    set,
  };
}

export function usePatchStrategyState(): UseMutationResult<
  StrategyPatchResponse,
  unknown,
  PatchStrategyStateArgs
> {
  const client = useNexusClient();
  const qc = useQueryClient();
  const { toast } = useToast();
  return useMutation({
    mutationFn: (args: PatchStrategyStateArgs) =>
      client.strategyPatchState(args.strategyId, args.stateId, buildPatchStateRequest(args)),
    onSuccess: (_data, args) => {
      toast({ variant: 'success', title: 'State updated', description: args.stateId });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(args.strategyId) });
    },
    onError: () => {},
  });
}

interface StateInspectorProps {
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

export function StateInspector({
  presetId,
  selectedState,
  form,
  onChange,
  workingRevisionRef,
  saveTrigger,
  saveStatus,
  onSaveStatus,
  onConflict,
}: StateInspectorProps) {
  const patch = usePatchStrategyState();
  const original = originalFormOf(selectedState);
  const dirty = isSectionDirty('state', form, original);
  const lastHandledTriggerRef = useRef(0);

  const handleSave = useCallback(async () => {
    if (!dirty || patch.isPending) return;
    onSaveStatus(undefined);

    const args: PatchStrategyStateArgs = {
      strategyId: presetId,
      stateId: selectedState.id,
      baseRevision: workingRevisionRef.current,
    };
    if (form.label !== original.label) args.label = form.label;
    if (form.description !== original.description) args.description = form.description;

    try {
      const res = await patch.mutateAsync(args);
      workingRevisionRef.current = Number(res.new_revision);
      onSaveStatus({ type: 'success', message: 'Saved label' });
    } catch (error) {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        onConflict(currentRevision, 'state');
      } else {
        const message = error instanceof Error ? error.message : 'Failed to save label';
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

  return (
    <section className="flex flex-col gap-2" aria-label="State editor">
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">State</span>
        <button
          type="button"
          onClick={handleSave}
          disabled={!dirty || patch.isPending}
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:text-gray-500"
        >
          {patch.isPending ? 'Saving…' : 'Save state'}
        </button>
      </div>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">Label / state id</span>
        <input
          type="text"
          value={form.label}
          onChange={(e) => onChange('label', e.target.value)}
          className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
        />
      </label>
      <label className="flex flex-col gap-1 text-copy-13">
        <span className="text-gray-700">Description</span>
        <textarea
          value={form.description}
          onChange={(e) => onChange('description', e.target.value)}
          rows={3}
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
