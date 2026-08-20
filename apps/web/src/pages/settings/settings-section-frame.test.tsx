/**
 * SettingsSectionFrame — entrance-filtered rail (V1.170 P1 — plan QC W-2).
 *
 * The frame is presentational: the host supplies `hiddenSettingsSections`
 * from the entrance registry (Create hides agent/modules/advanced). This
 * pins the rail filter — a hidden section must not render a nav button, so
 * the Create tree cannot advertise a develop-only section that the guard
 * would bounce.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import { SettingsSectionFrame } from '@/pages/settings/settings-section-frame';
import type { SettingsSectionId } from '@/components/layout/settings-section-registry';

function makeClient() {
  return new BrowserClient();
}

function renderFrame(hiddenSettingsSections: readonly SettingsSectionId[]) {
  const onSelectSection = vi.fn();
  renderInApp(
    <SettingsSectionFrame
      activeSection="workspace"
      onSelectSection={onSelectSection}
      hiddenSettingsSections={hiddenSettingsSections}
    >
      <div data-testid="section-outlet">content</div>
    </SettingsSectionFrame>,
    { client: makeClient() },
  );
  return { onSelectSection };
}

describe('SettingsSectionFrame rail (W-2)', () => {
  it('renders the full rail when no sections are hidden (Develop)', () => {
    renderFrame([]);

    const nav = screen.getByTestId('settings-section-nav');
    for (const id of ['agent', 'workspace', 'appearance', 'modules', 'advanced']) {
      expect(within(nav).getByTestId(`settings-section-nav-${id}`)).toBeInTheDocument();
    }
  });

  it('hides the develop-only sections on Create (agent/modules/advanced)', () => {
    renderFrame(['agent', 'modules', 'advanced']);

    const nav = screen.getByTestId('settings-section-nav');
    expect(within(nav).getByTestId('settings-section-nav-workspace')).toBeInTheDocument();
    expect(within(nav).getByTestId('settings-section-nav-appearance')).toBeInTheDocument();
    expect(within(nav).queryByTestId('settings-section-nav-agent')).not.toBeInTheDocument();
    expect(within(nav).queryByTestId('settings-section-nav-modules')).not.toBeInTheDocument();
    expect(within(nav).queryByTestId('settings-section-nav-advanced')).not.toBeInTheDocument();
  });

  it('forwards a visible-section click to the host', async () => {
    const user = userEvent.setup();
    const { onSelectSection } = renderFrame(['agent', 'modules', 'advanced']);

    await user.click(screen.getByTestId('settings-section-nav-appearance'));

    expect(onSelectSection).toHaveBeenCalledWith('appearance');
  });
});
