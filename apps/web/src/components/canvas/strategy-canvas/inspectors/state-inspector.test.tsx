/**
 * Regression coverage for R-V171P1-QC1-003 (B10):
 * the success toast after renaming a strategy state should show the new label,
 * not the old state id.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { PresetState } from '@/lib/canvas/preset-yaml';
import type { EditForm } from '../state-machine';

import { StateInspector } from './state-inspector';

const baseState: PresetState = {
  id: 'draft',
  description: 'Original description',
  next: 'revise',
  context_update: { template_file: 'prompts/draft.md' },
};

const baseForm: EditForm = {
  label: 'renamed-label',
  description: 'Original description',
  nextTarget: 'edit',
  promptBody: '# edited prompt',
};

function makeClient(): NexusClient {
  return {
    strategyPatchState: vi.fn().mockResolvedValue({
      new_revision: 2,
      validation_summary: { errors: [], warnings: [] },
      side_effects: [],
    }),
  } as unknown as NexusClient;
}

describe('StateInspector toast (R-V171P1-QC1-003 B10)', () => {
  it('shows the new label in the success toast after a state rename', async () => {
    const client = makeClient();
    renderInApp(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={baseForm}
        onChange={vi.fn()}
        workingRevisionRef={{ current: 1 }}
        saveTrigger={0}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
      { client },
    );

    await userEvent.click(screen.getByRole('button', { name: /save state/i }));

    await waitFor(() => {
      expect(screen.getByText('State updated')).toBeInTheDocument();
    });
    expect(screen.getByText('renamed-label')).toBeInTheDocument();
    expect(screen.queryByText('draft')).not.toBeInTheDocument();
  });

  it('falls back to the state id when only the description changed', async () => {
    const client = makeClient();
    renderInApp(
      <StateInspector
        presetId="novel-writing"
        selectedState={baseState}
        form={{ ...baseForm, label: baseState.id, description: 'Updated description' }}
        onChange={vi.fn()}
        workingRevisionRef={{ current: 1 }}
        saveTrigger={0}
        saveStatus={undefined}
        onSaveStatus={vi.fn()}
        onConflict={vi.fn()}
      />,
      { client },
    );

    await userEvent.click(screen.getByRole('button', { name: /save state/i }));

    await waitFor(() => {
      expect(screen.getByText('State updated')).toBeInTheDocument();
    });
    expect(screen.getByText('draft')).toBeInTheDocument();
    expect(screen.queryByText('renamed-label')).not.toBeInTheDocument();
  });
});
