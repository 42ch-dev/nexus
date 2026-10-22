import { beforeEach, describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { WizardShell } from './wizard-shell';
import { renderInApp } from '@/test/test-providers';
import { ENTRANCE_STORAGE_KEY } from '@/lib/entrance-context';
import type { NexusClient } from '@/lib/nexus';

const client = {
  runtimeStatus: () => Promise.resolve({ workspace_initialized: true }),
} as unknown as NexusClient;

describe('WizardShell', () => {
  beforeEach(() => window.localStorage.clear());

  it('starts on the identity step with 下一步 gated by the choice', () => {
    renderInApp(<WizardShell />, { client });

    expect(screen.getByRole('heading', { name: '你的 AI 创作工作室' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '下一步' })).toBeDisabled();
  });

  it('persists the developer entrance, which selects the developer rail registry', async () => {
    const user = userEvent.setup();
    renderInApp(<WizardShell />, { client });

    await user.click(screen.getByTestId('identity-option-developer'));
    await user.click(screen.getByRole('button', { name: '下一步' }));

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('developer'),
    );
  });

  it('persists the creator entrance', async () => {
    const user = userEvent.setup();
    renderInApp(<WizardShell />, { client });

    await user.click(screen.getByTestId('identity-option-content-creator'));
    await user.click(screen.getByRole('button', { name: '下一步' }));

    await waitFor(() =>
      expect(window.localStorage.getItem(ENTRANCE_STORAGE_KEY)).toBe('content-creator'),
    );
  });
});
