/**
 * CapabilitiesPage render tests.
 *
 * Verifies the admission-gate UX affordance: because CapabilityInfo does not
 * carry gate data, the page must explicitly tell authors that gates are
 * enforced at invocation time rather than leaving the absence unexplained.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { CapabilitiesPage } from '@/pages/capabilities-page';
import { screen, waitFor } from '@testing-library/react';

const client = () => new BrowserClient();

function renderCaps() {
  return renderInApp(<CapabilitiesPage />, { client: client() });
}

describe('CapabilitiesPage', () => {
  it('renders capability schemas and the admission-gate notice', async () => {
    useHandlers(
      http.get('/v1/local/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'nexus.example.greet',
              input_schema: '{"type":"object"}',
              output_schema: '{"type":"string"}',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('nexus.example.greet')).toBeInTheDocument();
    expect(screen.getByText('Input schema')).toBeInTheDocument();
    expect(screen.getByText('Output schema')).toBeInTheDocument();
    expect(
      screen.getByText(/Admission gates are enforced by the daemon/i),
    ).toBeInTheDocument();
  });

  it('renders the empty state when no capabilities are registered', async () => {
    useHandlers(
      http.get('/v1/local/orchestration/capabilities', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('No capabilities')).toBeInTheDocument();
  });
});
