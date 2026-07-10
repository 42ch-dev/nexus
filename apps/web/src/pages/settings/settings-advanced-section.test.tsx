/**
 * Settings Advanced section — Connection + Setup stacking and fingerprint
 * mismatch recovery view.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import { Routes, Route } from 'react-router-dom';

import { SettingsAdvancedSection } from '@/pages/settings/settings-advanced-section';
import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import * as clientContext from '@/lib/client-context';
import type { ResumeFingerprintGateState } from '@/lib/nexus/use-resume-fingerprint-gate';

function makeClient() {
  return new BrowserClient();
}

const advancedRoute = (
  <Route path="settings/advanced" element={<SettingsAdvancedSection />} />
);

describe('SettingsAdvancedSection', () => {
  it('renders both Connection and Setup sections in normal state', () => {
    renderInApp(<Routes>{advancedRoute}</Routes>, {
      client: makeClient(),
      initialRouterEntries: ['/settings/advanced'],
      setupCompleted: true,
    });

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
  });

  it('renders only the Connection section when a fingerprint mismatch is active', () => {
    const mismatchState: ResumeFingerprintGateState = {
      status: 'mismatch',
      served: 'mismatch-fingerprint',
    };
    vi.spyOn(clientContext, 'useFingerprintGateState').mockReturnValue(mismatchState);

    renderInApp(<Routes>{advancedRoute}</Routes>, {
      client: makeClient(),
      initialRouterEntries: ['/settings/advanced'],
      setupCompleted: true,
    });

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });
});
