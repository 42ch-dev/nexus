/**
 * Regression coverage for R-V172P1-QC3-001.
 *
 * Each inspector used to fire handleSave on every render once saveTrigger became
 * non-zero because the effect depends on saveTrigger + handleSave, and
 * handleSave is recreated every render. A single Cmd/Ctrl+S could therefore
 * issue duplicate PATCH requests on later renders.
 *
 * The fix uses a last-handled trigger ref so each new trigger value is consumed
 * exactly once, even if React re-renders the inspector before the mutation
 * settles.
 */
import { waitFor } from '@testing-library/react';
import type { MutableRefObject } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { PresetState } from '@/lib/canvas/preset-yaml';
import type { EditForm } from '../state-machine';

import { StateInspector } from './state-inspector';
import { EdgeInspector } from './edge-inspector';
import { PromptInspector } from './prompt-inspector';

const baseState: PresetState = {
  id: 'draft',
  description: 'Original description',
  next: 'revise',
  context_update: { template_file: 'prompts/draft.md' },
};

const baseForm: EditForm = {
  label: 'edited-label',
  description: 'Original description',
  nextTarget: 'edit',
  promptBody: '# edited prompt',
};

function makeWorkingRevisionRef(): MutableRefObject<number> {
  return { current: 1 };
}

function makeClient(): NexusClient {
  return {
    strategyPatchState: vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    }),
    strategyPatchTransition: vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    }),
    strategyPatchPromptTemplate: vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    }),
  } as unknown as NexusClient;
}

describe('inspector save trigger replay guard (R-V172P1-QC3-001)', () => {
  it('StateInspector patches once per trigger value, not once per render', async () => {
    const client = makeClient();
    const workingRevisionRef = makeWorkingRevisionRef();
    const { rerender } = renderInApp(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={0}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
      { client },
    );

    rerender(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );

    // Re-render with the same trigger value simulates an unrelated render
    // (e.g., from parent state change) before the mutation settles.
    rerender(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );

    await waitFor(() => expect(client.strategyPatchState).toHaveBeenCalledTimes(1));

    // A fresh trigger value should still be handled exactly once.
    rerender(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={2}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );

    await waitFor(() => expect(client.strategyPatchState).toHaveBeenCalledTimes(2));
  });

  it('EdgeInspector patches once per trigger value, not once per render', async () => {
    const client = makeClient();
    const workingRevisionRef = makeWorkingRevisionRef();
    const { rerender } = renderInApp(
      <EdgeInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={0}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
      { client },
    );

    rerender(
      <EdgeInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );
    rerender(
      <EdgeInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );

    await waitFor(() => expect(client.strategyPatchTransition).toHaveBeenCalledTimes(1));
  });

  it('PromptInspector patches once per trigger value, not once per render', async () => {
    const client = makeClient();
    const workingRevisionRef = makeWorkingRevisionRef();
    const { rerender } = renderInApp(
      <PromptInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        promptTemplateRef="prompts/draft.md"
        saveTrigger={0}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
      { client },
    );

    rerender(
      <PromptInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        promptTemplateRef="prompts/draft.md"
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );
    rerender(
      <PromptInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={workingRevisionRef}
        promptTemplateRef="prompts/draft.md"
        saveTrigger={1}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
    );

    await waitFor(() => expect(client.strategyPatchPromptTemplate).toHaveBeenCalledTimes(1));
  });
});
