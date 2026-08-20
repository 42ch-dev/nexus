/**
 * Wizard Entrance step (V1.170 P1 — AR-17, product EL §2).
 *
 * Pins the locked copy (title, subtitle, option cards), the default
 * content-creator highlight, and the state write on selection. Continue is
 * always enabled (a default always exists) and just advances the wizard.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useCallback, useState } from 'react';

import { SetupStepEntrance } from '@/pages/setup-step-entrance';
import { renderInApp } from '@/test/test-providers';
import { DEFAULT_ENTRANCE, type EntranceId } from '@/components/layout/entrance-registry';
import type { WizardState } from '@/pages/setup-wizard-page';

function makeState(overrides: Partial<WizardState> = {}): WizardState {
  return {
    entrance: DEFAULT_ENTRANCE,
    workspaceRoot: '',
    selectedAgent: null,
    customLaunchCommand: '',
    profileDisplayName: '',
    ...overrides,
  };
}

function Harness({
  initial,
  onNext = vi.fn(),
  onBack,
}: {
  initial: WizardState;
  onNext?: () => void;
  onBack?: () => void;
}) {
  const [state, setState] = useState<WizardState>(initial);
  const onChange = useCallback((next: WizardState) => setState(next), []);
  return (
    <SetupStepEntrance state={state} onChange={onChange} onNext={onNext} onBack={onBack} />
  );
}

describe('SetupStepEntrance', () => {
  it('renders the locked EL §2 copy with both option cards', () => {
    renderInApp(<Harness initial={makeState()} />);

    expect(
      screen.getByRole('heading', { name: 'How do you use Nexus?' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'This chooses your workspace layout. It is not your writing identity — Creator profiles stay as they are.',
      ),
    ).toBeInTheDocument();

    const contentCreator = screen.getByTestId('entrance-option-content-creator');
    expect(contentCreator).toHaveTextContent('Content creator');
    expect(contentCreator).toHaveTextContent(
      'Write and worldbuild. Agents help you. You do not install modules or edit presets.',
    );

    const developer = screen.getByTestId('entrance-option-developer');
    expect(developer).toHaveTextContent('Developer');
    expect(developer).toHaveTextContent(
      'Build on Nexus. Modules, presets, capabilities, Connect, and the full Control Room.',
    );
  });

  it('defaults the highlight to content-creator', () => {
    renderInApp(<Harness initial={makeState()} />);

    const contentCreator = screen.getByTestId('entrance-option-content-creator');
    const developer = screen.getByTestId('entrance-option-developer');
    expect(contentCreator).toHaveAttribute('aria-checked', 'true');
    expect(developer).toHaveAttribute('aria-checked', 'false');
  });

  it('pre-highlights the state entrance (wizard carries the choice across steps)', () => {
    renderInApp(<Harness initial={makeState({ entrance: 'developer' })} />);

    expect(screen.getByTestId('entrance-option-developer')).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(screen.getByTestId('entrance-option-content-creator')).toHaveAttribute(
      'aria-checked',
      'false',
    );
  });

  it('selecting an option updates the wizard state', async () => {
    const user = userEvent.setup();
    const captured: { latest: WizardState | null } = { latest: null };
    function HarnessWithCapture() {
      const [state, setState] = useState<WizardState>(makeState());
      const onChange = useCallback((next: WizardState) => {
        captured.latest = next;
        setState(next);
      }, []);
      return <SetupStepEntrance state={state} onChange={onChange} onNext={vi.fn()} />;
    }
    renderInApp(<HarnessWithCapture />);

    await user.click(screen.getByTestId('entrance-option-developer'));
    expect(captured.latest?.entrance).toBe('developer' satisfies EntranceId);
    expect(screen.getByTestId('entrance-option-developer')).toHaveAttribute(
      'aria-checked',
      'true',
    );
  });

  it('Continue advances the wizard without persisting (persist happens in finish)', async () => {
    const user = userEvent.setup();
    const onNext = vi.fn();
    renderInApp(<Harness initial={makeState()} onNext={onNext} />);

    await user.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it('hides the Back affordance on the first step', () => {
    renderInApp(<Harness initial={makeState()} />);
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });
});
