import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { resolveInitialHubTab, useHubTabState } from './use-hub-tab-state';

describe('resolveInitialHubTab', () => {
  it('prefers World when worlds exist', () => {
    expect(resolveInitialHubTab(2, 5)).toBe('world');
  });

  it('selects Work when only works exist (IA §1.2)', () => {
    expect(resolveInitialHubTab(0, 3)).toBe('work');
  });

  it('defaults to World when both lists are empty', () => {
    expect(resolveInitialHubTab(0, 0)).toBe('world');
  });
});

describe('useHubTabState', () => {
  it('auto-resolves tab after lists finish loading', async () => {
    const { result, rerender } = renderHook(
      ({ worldCount, workCount, isListsLoading }) =>
        useHubTabState(worldCount, workCount, isListsLoading),
      {
        initialProps: { worldCount: 0, workCount: 0, isListsLoading: true },
      },
    );

    expect(result.current.activeTab).toBe('world');

    rerender({ worldCount: 0, workCount: 2, isListsLoading: false });

    await waitFor(() => {
      expect(result.current.activeTab).toBe('work');
    });
  });

  it('does not auto-switch after the user manually changes tabs', async () => {
    const { result, rerender } = renderHook(
      ({ worldCount, workCount, isListsLoading }) =>
        useHubTabState(worldCount, workCount, isListsLoading),
      {
        initialProps: { worldCount: 0, workCount: 0, isListsLoading: true },
      },
    );

    act(() => {
      result.current.onTabChange('world');
    });

    rerender({ worldCount: 0, workCount: 2, isListsLoading: false });

    await waitFor(() => {
      expect(result.current.activeTab).toBe('world');
    });
  });
});
