/**
 * ModulesPage render tests.
 *
 * Verifies list + detail read-only UX: the page lists installed compute modules
 * and renders a manifest detail panel when the author selects one.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { ModulesPageBody } from '@/pages/modules-page';

const client = () => new BrowserClient();

function renderModules() {
  return renderInApp(<ModulesPageBody />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('ModulesPage', () => {
  it('renders the modules list and the basic-combat module', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              description: 'A simple combat resolution module.',
              required_key_block_types: ['unit', 'terrain'],
              battle_report_kind: 'combat-log',
            },
          ],
          has_more: false,
        }),
      ),
    );

    renderModules();

    expect(await screen.findByText('Basic Combat')).toBeInTheDocument();
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
    expect(screen.getByText('A simple combat resolution module.')).toBeInTheDocument();
    expect(screen.getByText('unit')).toBeInTheDocument();
    expect(screen.getByText('terrain')).toBeInTheDocument();
    expect(screen.getByText('combat-log')).toBeInTheDocument();
  });

  it('renders the detail panel when a module is selected', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              description: 'A simple combat resolution module.',
              required_key_block_types: ['unit'],
            },
          ],
          has_more: false,
        }),
      ),
      http.get('/v1/daemon/compute/modules/basic-combat', () =>
        HttpResponse.json({
          module_id: 'basic-combat',
          name: 'Basic Combat',
          version: '1.0.0',
          nexus_abi_version: 1,
          required_key_block_types: ['unit'],
          compute_export: 'compute',
          init_export: 'init',
          description: 'A simple combat resolution module.',
          author: 'Nexus Team',
          host_functions: ['kb_read'],
          battle_report_kind: 'combat-log',
          max_fuel: 1_000_000,
          max_memory_mib: 128,
          max_wall_time_ms: 5000,
        }),
      ),
    );

    renderModules();

    await screen.findByText('Basic Combat');
    await user.click(screen.getByRole('button', { name: 'Basic Combat' }));

    await waitFor(() => {
      expect(screen.getByText('Module manifest')).toBeInTheDocument();
    });

    expect(screen.getByText('basic-combat')).toBeInTheDocument();
    expect(screen.getByText('Nexus Team')).toBeInTheDocument();
    expect(screen.getByText('compute')).toBeInTheDocument();
    expect(screen.getByText('init')).toBeInTheDocument();
    expect(screen.getByText('kb_read')).toBeInTheDocument();
    expect(screen.getByText('1000000')).toBeInTheDocument();
    expect(screen.getByText('128')).toBeInTheDocument();
    expect(screen.getByText('5000')).toBeInTheDocument();
  });

  it('renders the empty state when no modules are installed', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({ items: [], has_more: false }),
      ),
    );

    renderModules();

    expect(await screen.findByText('No modules installed')).toBeInTheDocument();
  });

  it('renders the error state when the daemon fails', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json(
          { success: false, error: { code: 'internal', message: 'boom' } },
          { status: 500 },
        ),
      ),
    );

    renderModules();

    expect(await screen.findByText('Could not load modules')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders unavailable state when orchestration engine is down (503)', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'service_unavailable', message: 'engine not available' },
          },
          { status: 503 },
        ),
      ),
    );

    renderModules();

    expect(await screen.findByText('Orchestration engine not running')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('Could not load this view')).not.toBeInTheDocument();
  });

  it('renders the loading state before data resolves', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', async () => {
        await new Promise((resolve) => { setTimeout(resolve, 50); });
        return HttpResponse.json({ items: [], has_more: false });
      }),
    );

    renderModules();

    expect(await screen.findByText('Loading modules…')).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'basic-combat',
              name: 'Basic Combat',
              version: '1.0.0',
              required_key_block_types: ['unit'],
            },
          ],
          has_more: false,
        }),
      ),
    );

    renderModules();
    expect(await screen.findByRole('heading', { name: 'Compute Modules' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    expect(await screen.findByRole('heading', { name: '计算模块' })).toBeInTheDocument();
  });
});
