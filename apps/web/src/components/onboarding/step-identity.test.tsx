import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type { EntranceId } from '@/components/layout/entrance-registry';
import { StepIdentity } from './step-identity';

function Harness({ onNext }: { onNext: () => void }) {
  const [value, setValue] = useState<EntranceId | null>(null);
  return <StepIdentity value={value} onChange={setValue} onNext={onNext} />;
}

describe('StepIdentity', () => {
  it('renders the product-final identity copy', () => {
    render(<StepIdentity value={null} onChange={() => {}} onNext={() => {}} />);

    expect(screen.getByText('NEXUS · First-Time Setup')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Your AI Creative Studio' })).toBeInTheDocument();
    expect(
      screen.getByText('Worlds remember, so stories never contradict themselves. First, tell us — what do you want to do with Nexus?'),
    ).toBeInTheDocument();
    expect(screen.getByText('I Write Stories')).toBeInTheDocument();
    expect(
      screen.getByText('Novels · Tabletop campaigns · Worldbuilding / AI remembers your lore and helps you draft'),
    ).toBeInTheDocument();
    expect(screen.getByText('Recommended')).toBeInTheDocument();
    expect(screen.getByText('I Build Tools')).toBeInTheDocument();
    expect(
      screen.getByText('Orchestration engine · Presets · WASM modules / The full Control Room and Strategy Canvas'),
    ).toBeInTheDocument();
  });

  it('keeps Next disabled until a card is chosen, then reports the entrance', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    render(<Harness onNext={onNext} />);

    const next = screen.getByRole('button', { name: 'Next' });
    expect(next).toBeDisabled();

    const developer = screen.getByTestId('identity-option-developer');
    expect(developer).toHaveAttribute('aria-checked', 'false');
    await user.click(developer);

    expect(developer).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('identity-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'false',
    );
    expect(next).toBeEnabled();

    await user.click(next);
    expect(onNext).toHaveBeenCalledTimes(1);
  });
});
