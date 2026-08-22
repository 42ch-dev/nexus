/**
 * CapabilitiesPage render tests.
 *
 * Verifies the admission-gate UX affordance: because CapabilityInfo does not
 * carry gate data, the page must explicitly tell authors that gates are
 * enforced at invocation time rather than leaving the absence unexplained.
 *
 * Also verifies the AR-42 provenance surface: `origin: "user"` capabilities
 * render a Local badge + local-only copy; builtins render a plain row.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { CapabilitiesPage } from '@/pages/capabilities-page';
import { act, screen } from '@testing-library/react';
import enCapabilities from '@/locales/en/capabilities.json';
import zhCapabilities from '@/locales/zh-CN/capabilities.json';

const client = () => new BrowserClient();

function renderCaps() {
  return renderInApp(<CapabilitiesPage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('CapabilitiesPage', () => {
  it('renders capability schemas and the admission-gate notice', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'nexus.example.greet',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
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
    // Wire-shape guard (P2 fix wave F2): the daemon serves the local camelCase
    // DTO (inputSchema/outputSchema — AR-40), so the schema payload must render
    // through the page's field access. If the page read a snake_case key the
    // wire never sends, SchemaBlock falls back to '—' and this assertion fails.
    expect(screen.getByText('{"type":"object"}')).toBeInTheDocument();
    expect(screen.getByText('{"type":"string"}')).toBeInTheDocument();
    expect(
      screen.getByText(/Admission gates are enforced by the daemon/i),
    ).toBeInTheDocument();
  });

  it('renders the empty state when no capabilities are registered', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('No capabilities')).toBeInTheDocument();
  });

  it('renders the error state when the capabilities fetch fails', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({ message: 'server error' }, { status: 500 }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('Could not load capabilities.')).toBeInTheDocument();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('seeds the filter from the ?filter= search param (PL-13 capability deep link)', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'nexus.world.describe',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
            },
            {
              name: 'nexus.other.cap',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<CapabilitiesPage />, {
      client: client(),
      initialRouterEntries: ['/capabilities?filter=nexus.world.describe'],
    });

    // The linked schema is visible on arrival; others are filtered out.
    expect(await screen.findByText('nexus.world.describe')).toBeInTheDocument();
    expect(screen.queryByText('nexus.other.cap')).not.toBeInTheDocument();
    expect(screen.getByDisplayValue('nexus.world.describe')).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'nexus.example.greet',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderCaps();
    expect(await screen.findByText('nexus.example.greet')).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    expect(await screen.findByText('输入 schema')).toBeInTheDocument();
    expect(screen.getByText('输出 schema')).toBeInTheDocument();
  });

  it('renders a Local badge + local-only copy for a user capability (AR-42)', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'sync.pull',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
              origin: 'user',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('sync.pull')).toBeInTheDocument();
    expect(screen.getByText('Local')).toBeInTheDocument();
    expect(
      screen.getByText('Local module — no distribution or signing'),
    ).toBeInTheDocument();
  });

  it('renders a plain row for a builtin capability (AR-42)', async () => {
    useHandlers(
      http.get('/v1/daemon/orchestration/capabilities', () =>
        HttpResponse.json({
          items: [
            {
              name: 'nexus.example.greet',
              inputSchema: '{"type":"object"}',
              outputSchema: '{"type":"string"}',
              origin: 'builtin',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderCaps();

    expect(await screen.findByText('nexus.example.greet')).toBeInTheDocument();
    expect(screen.queryByText('Local')).not.toBeInTheDocument();
    expect(
      screen.queryByText('Local module — no distribution or signing'),
    ).not.toBeInTheDocument();
  });

  it('carries the provenance keys in both locale files with parity', async () => {
    const keys = ['userBadge', 'localOnlyCopy'] as const;
    for (const key of keys) {
      expect(enCapabilities[key]).toBeTruthy();
      expect(zhCapabilities[key]).toBeTruthy();
    }
  });
});
