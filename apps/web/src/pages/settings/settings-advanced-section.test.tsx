/**
 * Settings Advanced section — Connection + Setup stacking and fingerprint
 * gate recovery view.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { screen } from '@testing-library/react';
import { Routes, Route } from 'react-router';

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

function renderAdvanced(initialHash = '') {
  return renderInApp(<Routes>{advancedRoute}</Routes>, {
    client: makeClient(),
    initialRouterEntries: [`/settings/advanced${initialHash}`],
    setupCompleted: true,
  });
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('SettingsAdvancedSection', () => {
  it('renders both Connection and Setup sections when the gate does not apply', () => {
    renderAdvanced();

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

    renderAdvanced();

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });

  it('renders only the Connection section while fingerprint verification is in progress', () => {
    const verifyingState: ResumeFingerprintGateState = { status: 'verifying' };
    vi.spyOn(clientContext, 'useFingerprintGateState').mockReturnValue(verifyingState);

    renderAdvanced();

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });

  it('renders only the Connection section when fingerprint verification fails to fetch', () => {
    const fetchFailedState: ResumeFingerprintGateState = {
      status: 'fetch-failed',
      message: 'Could not reach daemon',
    };
    vi.spyOn(clientContext, 'useFingerprintGateState').mockReturnValue(fetchFailedState);

    renderAdvanced();

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.queryByTestId('settings-setup-section')).not.toBeInTheDocument();
  });

  it('renders both Connection and Setup sections when the fingerprint gate is verified', () => {
    const verifiedState: ResumeFingerprintGateState = { status: 'verified' };
    vi.spyOn(clientContext, 'useFingerprintGateState').mockReturnValue(verifiedState);

    renderAdvanced();

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
  });
});
