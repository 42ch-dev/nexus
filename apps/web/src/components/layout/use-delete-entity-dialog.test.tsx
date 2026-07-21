import { describe, it, expect, beforeEach, vi } from 'vitest';
import { screen, fireEvent, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';
import {
  useDeleteEntityDialog,
  type DeleteEntityTarget,
} from '@/components/layout/use-delete-entity-dialog';

// Stub the delete hooks so we can drive success synchronously and observe the
// mutation target without hitting the daemon.
let nextDeleteWorkSuccess: () => void = () => {};
let nextDeleteWorldSuccess: () => void = () => {};
let lastWorkMutationArg: string | undefined;
let lastWorldMutationArg: string | undefined;

vi.mock('@/api/queries', () => ({
  useDeleteWork: () => ({
    mutate: (id: string, opts?: { onSuccess?: () => void }) => {
      lastWorkMutationArg = id;
      if (opts?.onSuccess) nextDeleteWorkSuccess = opts.onSuccess;
    },
    isPending: false,
  }),
  useDeleteWorld: () => ({
    mutate: (id: string, opts?: { onSuccess?: () => void }) => {
      lastWorldMutationArg = id;
      if (opts?.onSuccess) nextDeleteWorldSuccess = opts.onSuccess;
    },
    isPending: false,
  }),
}));

/** Test harness that exposes the dialog + the selection context. */
function Harness({ target }: { target: DeleteEntityTarget }) {
  const selection = useCreatorEntitySelection();
  const dialog = useDeleteEntityDialog();
  return (
    <div>
      <button type="button" onClick={() => dialog.openDelete(target)}>
        open
      </button>
      <button
        type="button"
        onClick={() =>
          selection.setSelectedEntity({
            kind: target.kind,
            id: target.id,
            label: target.label,
          })
        }
      >
        select
      </button>
      <span data-testid="selected-id">{selection.selectedEntity?.id ?? 'none'}</span>
      {dialog.dialog}
    </div>
  );
}

describe('useDeleteEntityDialog — clear selection on delete (Greptile P1)', () => {
  beforeEach(() => {
    nextDeleteWorkSuccess = () => {};
    nextDeleteWorldSuccess = () => {};
    lastWorkMutationArg = undefined;
    lastWorldMutationArg = undefined;
  });

  it('clears the selected entity when the deleted World is the current selection', async () => {
    const target: DeleteEntityTarget = {
      kind: 'world',
      id: 'world-1',
      label: 'Aerda',
    };
    renderInApp(<Harness target={target} />);

    // Select the World in the Creator hub context, then open the delete dialog.
    fireEvent.click(screen.getByText('select'));
    expect(screen.getByTestId('selected-id').textContent).toBe('world-1');
    fireEvent.click(screen.getByText('open'));

    // Confirm the delete — the stub captures the onSuccess callback.
    fireEvent.click(screen.getByRole('button', { name: /Delete/i }));
    expect(lastWorldMutationArg).toBe('world-1');

    // Drive the daemon success. The selection should clear before the dialog
    // closes; otherwise /worlds would keep rendering the deleted item.
    nextDeleteWorldSuccess();

    await waitFor(() => {
      expect(screen.getByTestId('selected-id').textContent).toBe('none');
    });
  });

  it('does NOT clear selection when the deleted entity differs from the selected one', async () => {
    // Two separate worlds: deleted = world-A, selected = world-B.
    function TwoWorldHarness() {
      const selection = useCreatorEntitySelection();
      const dialog = useDeleteEntityDialog();
      return (
        <div>
          <button
            type="button"
            onClick={() =>
              selection.setSelectedEntity({
                kind: 'world',
                id: 'world-B',
                label: 'Selected world',
              })
            }
          >
            select-B
          </button>
          <button
            type="button"
            onClick={() =>
              dialog.openDelete({
                kind: 'world',
                id: 'world-A',
                label: 'Deleted world',
              })
            }
          >
            open-delete-A
          </button>
          <span data-testid="selected-id">
            {selection.selectedEntity?.id ?? 'none'}
          </span>
          {dialog.dialog}
        </div>
      );
    }

    renderInApp(<TwoWorldHarness />);
    fireEvent.click(screen.getByText('select-B'));
    expect(screen.getByTestId('selected-id').textContent).toBe('world-B');
    fireEvent.click(screen.getByText('open-delete-A'));
    fireEvent.click(screen.getByRole('button', { name: /Delete/i }));
    expect(lastWorldMutationArg).toBe('world-A');

    nextDeleteWorldSuccess();

    // Selection should remain world-B (NOT cleared) because world-A ≠ world-B.
    await waitFor(() => {
      expect(screen.getByTestId('selected-id').textContent).toBe('world-B');
    });
  });

  it('clears the selected entity when a Work is deleted and matches the selection', async () => {
    const target: DeleteEntityTarget = {
      kind: 'work',
      id: 'work-1',
      label: 'My Novel',
    };
    renderInApp(<Harness target={target} />);
    fireEvent.click(screen.getByText('select'));
    expect(screen.getByTestId('selected-id').textContent).toBe('work-1');
    fireEvent.click(screen.getByText('open'));
    fireEvent.click(screen.getByRole('button', { name: /Delete/i }));
    expect(lastWorkMutationArg).toBe('work-1');

    nextDeleteWorkSuccess();

    await waitFor(() => {
      expect(screen.getByTestId('selected-id').textContent).toBe('none');
    });
  });
});
