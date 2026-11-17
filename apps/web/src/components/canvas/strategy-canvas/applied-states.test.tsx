/**
 * The Canvas's applied presentations (D16) and the no-parse guarantee.
 *
 * One `getStrategyApplied` read resolves four distinct states: never applied,
 * approved-but-unapplied, an apply whose document no longer verifies, and the
 * normal graph. Each is asserted on its own heading and its own controls, so a
 * component that rendered the same thing for all of them would fail. The parse
 * spy proves the Canvas path never parses preset YAML — no state is "empty
 * because the YAML was unhelpful".
 */
import { cleanup, screen, waitFor, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { StrategyAppliedStatus } from '@42ch/nexus-contracts';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { parsePresetYaml } from '@/lib/canvas/preset-yaml';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { candidateDocument } from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

const shell = vi.hoisted(() => ({ nodes: [] as { id: string }[] }));

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: (props: { nodes: { id: string }[] }) => {
    shell.nodes = props.nodes;
    return <div data-testid="canvas-shell" />;
  },
  useNodeChangeHandler: () => () => {},
}));

vi.mock('@/lib/canvas/preset-yaml', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/canvas/preset-yaml')>();
  return { ...actual, parsePresetYaml: vi.fn(actual.parsePresetYaml) };
});

const STRATEGY_ID = 'novel-writing';
const APPLIED_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/applied`;
const CANDIDATE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/candidate`;
const APPLY_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/apply`;

const CURRENT_APPLY = {
  apply_id: 'app_3',
  approval_id: 'apr_42',
  content_hash: 'hash-3',
  applied_at: '2026-11-13T01:00:00Z',
  supersedes_apply_id: null,
};

const NEWEST_APPROVAL = {
  approval_id: 'apr_42',
  content_hash: 'hash-3',
  candidate_revision: 3,
  approved_at: '2026-11-13T00:00:00Z',
};

function appliedStatus(overrides: Partial<StrategyAppliedStatus> = {}): StrategyAppliedStatus {
  return {
    current_apply: null,
    newest_approval: null,
    applied_document: null,
    applied_document_refusal: null,
    ...overrides,
  };
}

function renderCanvas(status: StrategyAppliedStatus) {
  useHandlers(http.get(APPLIED_PATH, () => HttpResponse.json(status)));
  renderInApp(<StrategyCanvas presetId={STRATEGY_ID} />, { client: new BrowserClient() });
}

afterEach(() => cleanup());
beforeEach(() => {
  vi.mocked(parsePresetYaml).mockClear();
  shell.nodes = [];
});

describe('StrategyCanvas — applied states', () => {
  it('never applied: an empty state whose only action is Compose, and Compose actually opens', async () => {
    const document = candidateDocument({ id: STRATEGY_ID });
    useHandlers(http.get(CANDIDATE_PATH, () => HttpResponse.json(document)));
    renderCanvas(appliedStatus());

    const state = await screen.findByTestId('applied-state-never-applied');
    expect(within(state).getByRole('heading', { name: 'No preset applied yet' })).toBeInTheDocument();
    const actions = within(state).getAllByRole('button');
    expect(actions).toHaveLength(1);
    expect(actions[0]).toHaveTextContent('Open Compose');
    expect(screen.queryByTestId('canvas-shell')).toBeNull();
    expect(parsePresetYaml).not.toHaveBeenCalled();

    actions[0].click();
    await waitFor(() => expect(screen.getByTestId('canvas-shell')).toBeInTheDocument());
    expect(shell.nodes.map((node) => node.id).sort()).toEqual(
      document.nodes.map((node) => node.id).sort(),
    );
    expect(parsePresetYaml).not.toHaveBeenCalled();
  });

  it('approved but unapplied: apply leads, Compose is not offered, and Apply posts the hash', async () => {
    const posted: unknown[] = [];
    useHandlers(
      http.post(APPLY_PATH, async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json(CURRENT_APPLY);
      }),
    );
    renderCanvas(appliedStatus({ newest_approval: NEWEST_APPROVAL }));

    const state = await screen.findByTestId('applied-state-approved-unapplied');
    expect(
      within(state).getByRole('heading', { name: 'Approved, not yet live' }),
    ).toBeInTheDocument();
    const actions = within(state).getAllByRole('button');
    expect(actions).toHaveLength(1);
    expect(actions[0]).toHaveTextContent('Apply the latest approval');
    expect(within(state).queryByText('Open Compose')).toBeNull();
    expect(screen.queryByTestId('canvas-shell')).toBeNull();
    expect(parsePresetYaml).not.toHaveBeenCalled();

    actions[0].click();
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(posted[0]).toMatchObject({ approval_content_hash: 'hash-3' });
  });

  it('an apply whose document no longer verifies: an error naming the refusal, with no import or apply action', async () => {
    renderCanvas(
      appliedStatus({
        current_apply: CURRENT_APPLY,
        newest_approval: NEWEST_APPROVAL,
        applied_document_refusal: {
          reason: 'content_hash_mismatch',
          message: 'The applied document for novel-writing no longer verifies against app_3.',
        },
      }),
    );

    const state = await screen.findByTestId('applied-state-unreadable');
    expect(
      within(state).getByRole('heading', { name: 'The running preset cannot be read' }),
    ).toBeInTheDocument();
    expect(within(state).getByText(/no longer verifies against app_3/)).toBeInTheDocument();
    // Not an empty state: no "not yet applied" invitation, and no import/apply.
    expect(screen.queryByTestId('applied-state-never-applied')).toBeNull();
    expect(within(state).queryByRole('button', { name: /apply|compose|import/i })).toBeNull();
    const actions = within(state).getAllByRole('button');
    expect(actions).toHaveLength(1);
    expect(actions[0]).toHaveTextContent('Recheck');
    expect(screen.queryByTestId('canvas-shell')).toBeNull();
    expect(parsePresetYaml).not.toHaveBeenCalled();
  });

  it('applied: projects the applied document through the IR projection', async () => {
    const document = candidateDocument({ id: STRATEGY_ID });
    renderCanvas(
      appliedStatus({
        current_apply: CURRENT_APPLY,
        newest_approval: NEWEST_APPROVAL,
        applied_document: document,
      }),
    );

    await waitFor(() => expect(screen.getByTestId('canvas-shell')).toBeInTheDocument());
    expect(shell.nodes.map((node) => node.id).sort()).toEqual(
      document.nodes.map((node) => node.id).sort(),
    );
    expect(screen.queryByTestId('applied-state-never-applied')).toBeNull();
    expect(screen.queryByTestId('applied-state-approved-unapplied')).toBeNull();
    expect(screen.queryByTestId('applied-state-unreadable')).toBeNull();
    expect(parsePresetYaml).not.toHaveBeenCalled();
  });
});
