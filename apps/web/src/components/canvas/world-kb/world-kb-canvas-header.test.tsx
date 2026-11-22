/**
 * WorldKbHeader empty-state honesty tests — V1.108 FB-UI-010.
 *
 * The empty World KB helper copy was dishonest — it referenced a "command
 * palette (kb adopt/snapshot)" that does not exist in the product UI. The
 * locked replacement describes real next steps without fake promises.
 *
 * Voice & Content lock (spec §5.3):
 *   Empty:    *No entries to show yet. Refresh after adding world content,
 *             or continue from a linked Work.*
 *   Non-empty: unchanged.
 *
 * Forbidden in empty copy: "command palette", `kb adopt`, `snapshot`.
 */
import { describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { WorldKbHeader } from '@/components/canvas/world-kb/world-kb-canvas-header';

describe('WorldKbHeader empty-state honesty (V1.108 FB-UI-010)', () => {
  it('shows the locked honest helper copy when there are no entries', () => {
    renderInApp(
      <WorldKbHeader
        entryCount={0}
        lastFetched="just now"
        showList={false}
        onToggleView={() => {}}
        onRefresh={() => {}}
        refreshing={false}
      />,
    );

    expect(
      screen.getByText(
        /No entries to show yet\. Refresh after adding world content, or continue from a linked Work\./i,
      ),
    ).toBeInTheDocument();
  });

  it('does not promise a command palette or kb adopt/snapshot in empty copy', () => {
    renderInApp(
      <WorldKbHeader
        entryCount={0}
        lastFetched="just now"
        showList={false}
        onToggleView={() => {}}
        onRefresh={() => {}}
        refreshing={false}
      />,
    );

    expect(screen.queryByText(/command palette/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/kb adopt/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/snapshot/i)).not.toBeInTheDocument();
  });

  it('keeps the non-empty helper copy unchanged', () => {
    renderInApp(
      <WorldKbHeader
        entryCount={3}
        lastFetched="just now"
        showList={false}
        onToggleView={() => {}}
        onRefresh={() => {}}
        refreshing={false}
      />,
    );

    expect(
      screen.getByText(
        /Browse entities and promotion candidates\. Edits are guarded by per-row version checks\./i,
      ),
    ).toBeInTheDocument();
  });
});
