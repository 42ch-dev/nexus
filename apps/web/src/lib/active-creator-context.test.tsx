import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, renderHook, screen, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import {
  ActiveCreatorProvider,
  useActiveCreatorId,
  useSetActiveCreatorId,
} from './active-creator-context';

function TestReader() {
  const id = useActiveCreatorId();
  return <span data-testid="active-id">{id ?? 'null'}</span>;
}

function TestSetter({ id }: { id: string }) {
  const set = useSetActiveCreatorId();
  return <button onClick={() => set(id)}>Set</button>;
}

describe('ActiveCreatorProvider', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('restores the active creator id from localStorage', () => {
    window.localStorage.setItem('nexus:activeCreatorId', 'creator-stored');

    render(
      <ActiveCreatorProvider>
        <TestReader />
      </ActiveCreatorProvider>,
    );

    expect(screen.getByTestId('active-id')).toHaveTextContent('creator-stored');
  });

  it('persists the active creator id to localStorage when changed', async () => {
    const user = userEvent.setup();
    render(
      <ActiveCreatorProvider>
        <TestReader />
        <TestSetter id="creator-new" />
      </ActiveCreatorProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Set' }));

    expect(screen.getByTestId('active-id')).toHaveTextContent('creator-new');
    expect(window.localStorage.getItem('nexus:activeCreatorId')).toBe('creator-new');
  });

  it('syncs across tabs via the storage event', () => {
    render(
      <ActiveCreatorProvider>
        <TestReader />
      </ActiveCreatorProvider>,
    );

    act(() => {
      window.dispatchEvent(
        new StorageEvent('storage', {
          key: 'nexus:activeCreatorId',
          newValue: 'creator-remote',
        }),
      );
    });

    expect(screen.getByTestId('active-id')).toHaveTextContent('creator-remote');
  });

  it('throws when useActiveCreatorId is called outside the provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => renderHook(() => useActiveCreatorId())).toThrow(
      'useActiveCreatorId must be used within ActiveCreatorProvider',
    );
    spy.mockRestore();
  });

  it('throws when useSetActiveCreatorId is called outside the provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => renderHook(() => useSetActiveCreatorId())).toThrow(
      'useSetActiveCreatorId must be used within ActiveCreatorProvider',
    );
    spy.mockRestore();
  });
});
