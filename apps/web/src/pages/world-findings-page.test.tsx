/**
 * World Findings panel + page tests — V1.166 P2 T1 (DR-64 surfacing half).
 *
 * Covers the three panel states (populated / empty / truncated) plus the
 * spoke-vocabulary contract (PD-2 locked):
 *   - severity renders `info|warning|error` verbatim with an open-string
 *     fallback — NEVER remapped to the work-findings `minor/major/blocker`;
 *   - `kind` is an open string shown verbatim;
 *   - the panel is read-only — no accept/resolve/remediate controls.
 */
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { Route, Routes } from 'react-router';

import { WorldFindingsPage } from '@/pages/world-findings-page';
import { WorldFindingsPanel } from '@/components/worlds/world-findings/world-findings-panel';
import { worldSeverityVariant } from '@/components/worlds/world-findings/world-severity-badge';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { WorldFindingsListResponse } from '@42ch/nexus-contracts';

type WorldFinding = WorldFindingsListResponse['findings'][number];

const WORLD_ID = 'world-9';

function makeFinding(over: Partial<WorldFinding> = {}): WorldFinding {
  return {
    finding_id: 'fnd_00000000000000000000000000000001',
    schema_version: 1,
    title: 'Missing belief summary',
    description: "entry ent_abc has no populated body.summary",
    severity: 'warning',
    status: 'open',
    kind: 'required_field',
    target_entry_id: 'ent_abc',
    created_at: '2026-08-15T10:00:00Z',
    updated_at: '2026-08-15T10:00:00Z',
    ...over,
  };
}

function findingsResponse(findings: WorldFinding[], truncated = false): WorldFindingsListResponse {
  return { findings, truncated };
}

function renderPanel(worldId = WORLD_ID) {
  return renderInApp(<WorldFindingsPanel worldId={worldId} />, {
    client: new BrowserClient(),
  });
}

describe('worldSeverityVariant — spoke vocabulary with open-string fallback', () => {
  it('maps the three spoke severities to design-token tones', () => {
    expect(worldSeverityVariant('info')).toBe('queued'); // DESIGN.md informational → teal
    expect(worldSeverityVariant('warning')).toBe('warning');
    expect(worldSeverityVariant('error')).toBe('error');
  });

  it('is case-insensitive for the spoke vocabulary', () => {
    expect(worldSeverityVariant('INFO')).toBe('queued');
    expect(worldSeverityVariant('Warning')).toBe('warning');
  });

  it('falls back to neutral for open strings (no remap to minor/major/blocker)', () => {
    expect(worldSeverityVariant('urgent')).toBe('neutral');
    expect(worldSeverityVariant('minor')).toBe('neutral');
    expect(worldSeverityVariant('major')).toBe('neutral');
    expect(worldSeverityVariant('blocker')).toBe('neutral');
    expect(worldSeverityVariant(undefined)).toBe('neutral');
  });
});

describe('WorldFindingsPanel — populated', () => {
  it('renders spoke severities verbatim (info/warning/error + open string, no remap)', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([
            makeFinding({ finding_id: 'fnd_info', severity: 'info', title: 'Info item' }),
            makeFinding({ finding_id: 'fnd_warn', severity: 'warning', title: 'Warning item' }),
            makeFinding({ finding_id: 'fnd_err', severity: 'error', title: 'Error item' }),
            makeFinding({
              finding_id: 'fnd_open',
              severity: 'urgent',
              kind: 'custom-check',
              title: 'Open-string item',
            }),
          ]),
        ),
      ),
    );

    renderPanel();
    await waitFor(() => expect(screen.getByText('Info item')).toBeInTheDocument());

    // Severity strings render verbatim — never coerced to work vocabulary.
    expect(screen.getByText('info')).toBeInTheDocument();
    expect(screen.getByText('warning')).toBeInTheDocument();
    expect(screen.getByText('error')).toBeInTheDocument();
    expect(screen.getByText('urgent')).toBeInTheDocument();
    expect(screen.queryByText(/minor|major|blocker/i)).not.toBeInTheDocument();

    // Open-string `kind` renders verbatim too.
    expect(screen.getByText('custom-check')).toBeInTheDocument();

    // Count reflects the returned rows.
    expect(screen.getByTestId('world-findings-count')).toHaveTextContent('4');
  });

  it('shows the shortened target entry id per row and full id on expand', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([
            makeFinding({
              finding_id: 'fnd_expand',
              target_entry_id: 'ent_very_long_target_entry_id',
              created_at: '2026-08-15T10:00:00Z',
            }),
          ]),
        ),
      ),
    );

    renderPanel();
    const row = await screen.findByTestId('world-finding-row');
    expect(within(row).getByTestId('world-finding-target')).toHaveTextContent('…');

    fireEvent.click(row);
    await waitFor(() => expect(screen.getByTestId('world-finding-detail')).toBeInTheDocument());
    expect(screen.getByTestId('world-finding-target-full')).toHaveTextContent(
      'ent_very_long_target_entry_id',
    );
    expect(screen.getByTestId('world-finding-detail')).toHaveTextContent(
      'entry ent_abc has no populated body.summary',
    );
    // Expand toggles back.
    fireEvent.click(row);
    await waitFor(() => expect(screen.queryByTestId('world-finding-detail')).not.toBeInTheDocument());
  });

  it('is read-only: no accept/resolve/remediate controls, only expand toggles', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([
            makeFinding({ finding_id: 'fnd_ro', title: 'Read-only item' }),
            makeFinding({ finding_id: 'fnd_ro2', title: 'Read-only item 2' }),
          ]),
        ),
      ),
    );

    renderPanel();
    await waitFor(() => expect(screen.getByText('Read-only item')).toBeInTheDocument());

    // No remediation affordances (DR-28 is a separate roadmap item).
    expect(
      screen.queryByRole('button', { name: /accept|resolve|remediate|dismiss/i }),
    ).not.toBeInTheDocument();

    // Every button is a read-only expand/collapse toggle.
    const buttons = screen.getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    for (const button of buttons) {
      expect(button).toHaveAttribute('aria-expanded');
    }
  });
});

describe('WorldFindingsPanel — empty state', () => {
  it('renders the honest empty copy when the world has no findings', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(findingsResponse([], false)),
      ),
    );

    renderPanel();
    await waitFor(() => expect(screen.getByText('No findings')).toBeInTheDocument());
    expect(
      screen.getByText(/No findings are recorded for this World yet/),
    ).toBeInTheDocument();
    expect(screen.queryByTestId('world-findings-count')).not.toBeInTheDocument();
  });
});

describe('WorldFindingsPanel — truncated honesty', () => {
  it('renders the 500-cap copy when truncated is true', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([makeFinding({ finding_id: 'fnd_trunc', title: 'Truncated item' })], true),
        ),
      ),
    );

    renderPanel();
    await waitFor(() => expect(screen.getByText('Truncated item')).toBeInTheDocument());
    expect(screen.getByTestId('world-findings-truncated')).toHaveTextContent(
      'Showing the 500 newest findings',
    );
  });

  it('shows no truncation banner when truncated is false', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([makeFinding({ finding_id: 'fnd_nt', title: 'Not truncated' })], false),
        ),
      ),
    );

    renderPanel();
    await waitFor(() => expect(screen.getByText('Not truncated')).toBeInTheDocument());
    expect(screen.queryByTestId('world-findings-truncated')).not.toBeInTheDocument();
  });
});

describe('WorldFindingsPage — route', () => {
  it('renders the panel for the worldId from the URL', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/findings', () =>
        HttpResponse.json(
          findingsResponse([makeFinding({ finding_id: 'fnd_route', title: 'Routed item' })]),
        ),
      ),
    );

    renderInApp(
      <Routes>
        <Route path="/worlds/:worldId/findings" element={<WorldFindingsPage />} />
      </Routes>,
      { client: new BrowserClient(), initialRouterEntries: [`/worlds/${WORLD_ID}/findings`] },
    );

    await waitFor(() => expect(screen.getByTestId('world-findings-page')).toBeInTheDocument());
    expect(await screen.findByText('Routed item')).toBeInTheDocument();
  });
});
