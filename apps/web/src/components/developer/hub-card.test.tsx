/**
 * Hub card (plan v1220 T1) — the read-only status-section frame.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';

import { HubCard } from '@/components/developer/presentational/hub-card';
import { i18n } from '@/lib/i18n/config';
import { renderInApp } from '@/test/test-providers';

describe('HubCard', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en');
  });

  it('renders title, status pill, body and the single navigation link', () => {
    renderInApp(
      <HubCard
        id="service"
        title="Local service"
        status={{ label: 'Running', variant: 'running' }}
        go={{ to: '/strategies', label: 'Manage →' }}
      >
        <p>nexus-service v0.1.0</p>
      </HubCard>,
    );

    expect(screen.getByTestId('hub-card-service')).toBeInTheDocument();
    expect(screen.getByText('Local service')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
    expect(screen.getByText('nexus-service v0.1.0')).toBeInTheDocument();
    expect(screen.getByTestId('hub-card-service-go')).toHaveAttribute(
      'href',
      '/strategies',
    );
    // Read-only: no form controls, only the one navigation link.
    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(screen.queryAllByRole('textbox')).toHaveLength(0);
  });

  // v221-caps-sessions-rail T2 — the sessions card needs an inline node title
  // (mono short id) and `error` for a `failed` session; the markup is unchanged.
  it('accepts a node title and an error pill', () => {
    renderInApp(
      <HubCard
        id="session"
        title={<span className="font-mono">sess-0001</span>}
        status={{ label: 'Failed', variant: 'error' }}
      />,
    );

    const card = screen.getByTestId('hub-card-session');
    expect(screen.getByText('sess-0001')).toBeInTheDocument();
    expect(screen.getByText('Failed')).toBeInTheDocument();
    expect(card.innerHTML).toContain('nexus-ui-badge-soft-error-bg');
  });

  it('renders a title-only card without a navigation link', () => {
    renderInApp(<HubCard id="agent" title="Agent (ACP)" />);

    expect(screen.getByTestId('hub-card-agent')).toBeInTheDocument();
    expect(screen.queryByTestId('hub-card-agent-go')).not.toBeInTheDocument();
  });
});
