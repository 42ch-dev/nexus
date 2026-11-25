/**
 * Both shapes the canvas can render are read-only: the applied document and the
 * Compose working document. Neither may offer edge creation, transition
 * creation or an inspector save — the deleted YAML-authoring entry points must
 * not come back through a graph gesture, the keyboard shortcut, the palette,
 * the header, or the inspector sections the adapter renders for a selected node.
 */
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { NodeChangeset } from '@xyflow/react';
import type { MutableRefObject } from 'react';
import type { WorkflowDocument } from '@42ch/nexus-contracts';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import {
  createStrategyCanvasAdapter,
  type StrategyCanvasAdapterContext,
} from '@/components/canvas/strategy-canvas/strategy-canvas-adapter';
import { originalFormOf } from '@/components/canvas/strategy-canvas/state-machine';
import { clearCommands, getCommands } from '@/lib/canvas/command-registry';
import { parsedPresetFromIr } from '@/lib/canvas/ir-preset';
import { COMPOSE_FLAG_STORAGE_KEY } from '@/lib/canvas/strategy-compose-flag';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { servedIrDocument } from '@/test/simulation-fixture';
import { renderInApp } from '@/test/test-providers';

const shellProps = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));

vi.mock('@/components/canvas/canvas-shell', () => ({
  CanvasShell: (props: { children?: React.ReactNode } & Record<string, unknown>) => {
    shellProps.current = props;
    return <>{props.children}</>;
  },
  useNodeChangeHandler: () => () => {},
}));

vi.mock('@/components/canvas/idea-input', () => ({
  IdeaInput: () => null,
}));

const STRATEGY_ID = 'novel-writing';
const APPLIED_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/applied`;
const CANDIDATE_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/candidate`;
const APPROVALS_PATH = `/v1/daemon/strategies/${STRATEGY_ID}/approvals`;
const PATTERNS_PATH = '/v1/daemon/patterns';
const SESSIONS_PATH = '/v1/daemon/orchestration/sessions';
const SCHEDULES_PATH = '/v1/daemon/orchestration/schedules';

const SEED = servedIrDocument({ id: STRATEGY_ID });
const DOCUMENT: WorkflowDocument = {
  ...SEED,
  nodes: SEED.nodes.map((node) =>
    node.id === 'draft'
      ? { ...node, context_update: { op: 'append' as const, template: 'prompts/draft.md' } }
      : node,
  ),
};

function orchestrationHandlers() {
  return [
    http.get(SESSIONS_PATH, () => HttpResponse.json({ items: [] })),
    http.get(SCHEDULES_PATH, () => HttpResponse.json({ items: [] })),
  ];
}

function renderCanvas() {
  return renderInApp(<StrategyCanvas presetId={STRATEGY_ID} />, { client: new BrowserClient() });
}

const composeHandlers = () => ({
  isComposing: false,
  onToggleSlotExpanded: () => {},
  onBindSlot: () => Promise.resolve(),
  onReplaceSubtree: () => Promise.resolve(),
  onUnnest: () => Promise.resolve(),
});

/** The canvas itself: no graph gesture, header button or palette create route. */
function expectCanvasOffersNoCreateRoute(container: HTMLElement) {
  const canvas = within(container);

  expect(shellProps.current.onConnect).toBeUndefined();
  expect(shellProps.current.onReconnect).toBeUndefined();

  fireEvent.keyDown(window, { key: 'N', shiftKey: true });
  expect(canvas.queryByText('Create Transition')).not.toBeInTheDocument();

  expect(canvas.queryByRole('button', { name: 'Create' })).not.toBeInTheDocument();
  expect(getCommands().map((command) => command.id)).toEqual(['strategy.toggle-view']);

  act(() => {
    (shellProps.current.onNodesChange as (changes: NodeChangeset) => void)(
      new NodeChangeset([{ id: 'draft', type: 'select', selected: true }]),
    );
  });
  fireEvent.click(canvas.getByRole('button', { name: 'Edit' }));
  expect(canvas.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
}

/** The inspector sections the adapter renders for that shape's selected state. */
function expectInspectorOffersNoSave(compose: boolean) {
  const parsed = parsedPresetFromIr(DOCUMENT);
  const selectedState = parsed.manifest.states.find((state) => state.id === 'draft');
  if (!selectedState) throw new Error('fixture lost its draft state');
  const ctxRef = {
    current: {
      form: originalFormOf(selectedState),
      onChange: () => {},
      setActiveSection: () => {},
      selectedState,
      promptTemplateRef: 'prompts/draft.md',
      selectedNode: null,
      parsed,
      danglingTargets: [],
      ...(compose ? { compose: composeHandlers() } : {}),
    },
  } as MutableRefObject<StrategyCanvasAdapterContext>;

  const { container } = render(
    <>
      {createStrategyCanvasAdapter(ctxRef).renderInspector?.({
        id: 'draft',
        position: { x: 0, y: 0 },
        data: { stateId: 'draft' },
      } as never)}
    </>,
  );
  const inspector = within(container);
  expect(inspector.getByLabelText('State inspector')).toBeInTheDocument();
  expect(inspector.getByLabelText('Transition inspector')).toBeInTheDocument();
  expect(inspector.getByLabelText('Prompt inspector')).toBeInTheDocument();

  const fields = inspector.getAllByRole('textbox');
  expect(fields.length).toBeGreaterThan(0);
  for (const field of fields) expect(field).toBeDisabled();
  expect(inspector.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
}

beforeEach(() => {
  clearCommands();
});

afterEach(() => {
  cleanup();
  clearCommands();
  window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
});

describe('StrategyCanvas — no create affordance', () => {
  it('offers none on the applied document', async () => {
    window.localStorage.removeItem(COMPOSE_FLAG_STORAGE_KEY);
    useHandlers(
      ...orchestrationHandlers(),
      http.get(APPLIED_PATH, () =>
        HttpResponse.json({
          current_apply: {
            apply_id: 'app_1',
            approval_id: 'apr_42',
            content_hash: 'hash-1',
            applied_at: '2026-11-13T01:00:00Z',
            supersedes_apply_id: null,
          },
          newest_approval: null,
          applied_document: DOCUMENT,
          applied_document_refusal: null,
        }),
      ),
    );

    const { container } = renderCanvas();
    expect(await screen.findByText('rev 0')).toBeInTheDocument();

    expectCanvasOffersNoCreateRoute(container);
    expectInspectorOffersNoSave(false);
  });

  it('offers none on the Compose working document', async () => {
    window.localStorage.setItem(COMPOSE_FLAG_STORAGE_KEY, 'on');
    const workingDocument = { ...DOCUMENT, revision: 4 };
    useHandlers(
      ...orchestrationHandlers(),
      http.get(APPLIED_PATH, () =>
        HttpResponse.json({ current_apply: null, newest_approval: null }),
      ),
      http.get(APPROVALS_PATH, () => HttpResponse.json({ items: [] })),
      http.get(CANDIDATE_PATH, () => HttpResponse.json(workingDocument)),
      http.get(PATTERNS_PATH, () =>
        HttpResponse.json({ items: [], pagination: { limit: 100, has_more: false } }),
      ),
    );

    const { container } = renderCanvas();
    expect(await screen.findByTestId('strategy-staging')).toBeInTheDocument();
    expect(await screen.findByText('rev 4')).toBeInTheDocument();

    expectCanvasOffersNoCreateRoute(container);
    expectInspectorOffersNoSave(true);
  });
});
