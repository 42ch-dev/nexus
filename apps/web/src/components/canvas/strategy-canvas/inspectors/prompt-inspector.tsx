/**
 * Prompt-template inspector section — edits the body of the state's prompt.
 *
 * Owns its own save button and partial-failure UI (R-V171P0-QC1-004).
 */
import type { MutableRefObject } from 'react';
import { useCallback, useEffect, useRef } from 'react';
import { useMutation, useQueryClient, type UseMutationResult } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import type { StrategyPatchPromptTemplateRequest, StrategyPatchResponse } from '@42ch/nexus-contracts';

import { isStrategyConflictError } from '@/lib/canvas/use-strategy-data';
import { isSectionDirty, originalFormOf, type EditForm, type SaveStatus, type Section } from '../state-machine';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export interface PatchStrategyPromptTemplateArgs {
  strategyId: string;
  stateId: string;
  baseRevision: number;
  templateRef: string;
  body: string;
}

export function usePatchStrategyPromptTemplate(): UseMutationResult<
  StrategyPatchResponse,
  unknown,
  PatchStrategyPromptTemplateArgs
> {
  const client = useNexusClient();
  const qc = useQueryClient();
  const { toast } = useToast();
  return useMutation({
    mutationFn: (args: PatchStrategyPromptTemplateArgs) => {
      const req: StrategyPatchPromptTemplateRequest = {
        strategy_id: args.strategyId,
        state_id: args.stateId,
        base_revision: args.baseRevision,
        template_ref: args.templateRef,
        set: { body: args.body },
      };
      return client.strategyPatchPromptTemplate(args.strategyId, args.stateId, req);
    },
    onSuccess: (_data, args) => {
      toast({ variant: 'success', title: 'Prompt template saved', description: args.templateRef });
      void qc.invalidateQueries({ queryKey: queryKeys.presets.detail(args.strategyId) });
    },
    onError: () => {},
  });
}

interface PromptInspectorProps {
  presetId: string;
  selectedState: PresetState;
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  workingRevisionRef: MutableRefObject<number>;
  promptTemplateRef: string;
  saveTrigger: number;
  saveStatus: SaveStatus | undefined;
  onSaveStatus: (status: SaveStatus | undefined) => void;
  onConflict: (currentRevision: number, section: Section) => void;
}

export function PromptInspector({
  presetId,
  selectedState,
  form,
  onChange,
  workingRevisionRef,
  promptTemplateRef,
  saveTrigger,
  saveStatus,
  onSaveStatus,
  onConflict,
}: PromptInspectorProps) {
  const patch = usePatchStrategyPromptTemplate();
  const original = originalFormOf(selectedState);
  const dirty = isSectionDirty('prompt', form, original);
  const lastHandledTriggerRef = useRef(0);

  const handleSave = useCallback(async () => {
    if (!dirty || patch.isPending) return;
    onSaveStatus(undefined);

    try {
      const res = await patch.mutateAsync({
        strategyId: presetId,
        stateId: selectedState.id,
        baseRevision: workingRevisionRef.current,
        templateRef: promptTemplateRef,
        body: form.promptBody,
      });
      workingRevisionRef.current = Number(res.new_revision);
      onSaveStatus({ type: 'success', message: 'Saved prompt template' });
    } catch (error) {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        onConflict(currentRevision, 'prompt');
      } else {
        const message = error instanceof Error ? error.message : 'Failed to save prompt template';
        onSaveStatus({ type: 'error', message });
      }
    }
  }, [dirty, patch.isPending, form, original, presetId, selectedState, onSaveStatus, onConflict, promptTemplateRef]);

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

  return (
    <section className="flex flex-col gap-2" aria-label="Prompt template editor">
      <div className="flex items-center justify-between">
        <span className="text-label-14 font-semibold text-gray-900">Prompt template</span>
        <button
          type="button"
          onClick={handleSave}
          disabled={!dirty || patch.isPending}
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:text-gray-500"
        >
          {patch.isPending ? 'Saving…' : 'Save prompt'}
        </button>
      </div>
      <span className="text-copy-13-mono text-gray-700">{promptTemplateRef}</span>
      <textarea
        value={form.promptBody}
        onChange={(e) => onChange('promptBody', e.target.value)}
        rows={4}
        placeholder="Enter new prompt body…"
        className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
      />
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
