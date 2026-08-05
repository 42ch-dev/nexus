/**
 * PackPanel — P1 T5 (DF-77) coverage.
 *
 * Export: the real `useExportPack` hook runs against an msw handler for
 * `POST /v1/daemon/worlds/:world_id/kb/pack/export` (V1.151 precedent —
 * real hook + mocked transport), with jsdom's missing Blob-download APIs
 * stubbed so the anchor click + object URL round-trip are observable.
 *
 * Import: `useImportPack` is mocked via `vi.mock` (brief T5) as a real
 * TanStack `useMutation` whose `mutationFn` reads per-test behavior from
 * `packMock`, so the panel's mutation lifecycle (isPending/isError/onSuccess)
 * behaves like production without a daemon. Outcomes cover the three
 * conflict policies (skip / rename / overwrite), the overwrite confirm gate
 * (confirm proceeds, cancel aborts), and the inline error states (file-not-
 * JSON parse failure, daemon 403 ownership).
 *
 * The T2-T4 review flagged duplicate testids (`pack-atom-counts`,
 * `pack-count-*`) across the entries + relations blocks — assertions scope
 * with `getAllByTestId` + `within()`.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent, { type UserEvent } from '@testing-library/user-event';

import { PackPanel, type PackConflictPolicy } from './pack-panel';
import type * as QueriesModule from '@/api/queries';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import type { PackExportResponse, PackImportResponse } from '@42ch/nexus-contracts';

type ImportVars = { worldId: string; file: File; conflict: PackConflictPolicy };

/** Per-test import behavior — read by the mocked `useImportPack` mutationFn. */
const packMock = vi.hoisted(() => ({
  importCalls: [] as ImportVars[],
  importMutationFn: null as null | ((vars: ImportVars) => Promise<PackImportResponse>),
}));

vi.mock('@/api/queries', async (importOriginal) => {
  // The dynamic `await import` is required here: vi.mock factories are
  // hoisted above the test's static imports, so a statically imported
  // `useMutation` binding would be uninitialized inside the factory.
  const mod = await importOriginal<typeof QueriesModule>();
  const { useMutation } = await import('@tanstack/react-query');
  return {
    ...mod,
    // Brief T5: control the mutation outcome per test. `useExportPack` stays
    // real so the Blob-download path is exercised end to end (V1.151 pattern).
    useImportPack: () =>
      useMutation({
        mutationFn: async (vars: ImportVars) => {
          packMock.importCalls.push(vars);
          if (!packMock.importMutationFn) {
            throw new Error('useImportPack mock: wire packMock.importMutationFn before submitting');
          }
          return packMock.importMutationFn(vars);
        },
      }),
  };
});

const WORLD_ID = 'world-a';

const ZERO_COUNTS = { created: 0, skipped: 0, rejected: 0, renamed: 0, overwritten: 0 };

function makeSummary(
  over: {
    entries?: Partial<PackImportResponse['entries']>;
    relations?: Partial<PackImportResponse['relations']>;
    details?: PackImportResponse['details'];
  } = {},
): PackImportResponse {
  return {
    entries: { ...ZERO_COUNTS, ...over.entries },
    relations: { ...ZERO_COUNTS, ...over.relations },
    details: over.details ?? [],
  };
}

const PACK_FILE = new File(
  [JSON.stringify({ modules: { pack: { title: 'Test Pack' } }, entries: [], relations: [] })],
  'test-pack.json',
  { type: 'application/json' },
);

const EXPORT_FIXTURE: PackExportResponse = {
  modules: { pack: { title: 'My World' } },
  entries: [{ id: 'e1' }],
  relations: [],
};

function renderPanel() {
  return renderInApp(<PackPanel worldId={WORLD_ID} />, { client: new BrowserClient() });
}

/** Pick the pack file, optionally switch the conflict policy, then submit. */
async function uploadAndSubmit(user: UserEvent, policy?: PackConflictPolicy) {
  await user.upload(screen.getByTestId('pack-file-input'), PACK_FILE);
  if (policy && policy !== 'skip') {
    await user.selectOptions(screen.getByTestId('pack-conflict-select'), policy);
  }
  await user.click(screen.getByTestId('pack-import-submit'));
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
  packMock.importCalls = [];
  packMock.importMutationFn = null;
});

describe('PackPanel — export (T5)', () => {
  it('triggers a Blob download of the pack envelope on click', async () => {
    let exportBody: unknown;
    useHandlers(
      http.post('/v1/daemon/worlds/world-a/kb/pack/export', async ({ request }) => {
        exportBody = await request.json();
        return HttpResponse.json(EXPORT_FIXTURE);
      }),
    );

    // jsdom has no Blob-download APIs — stub them so the hook's download
    // mechanics (object-URL round-trip + anchor click) are observable.
    const createObjectURL = vi.fn<(blob: Blob) => string>(() => 'blob:pack');
    const revokeObjectURL = vi.fn();
    URL.createObjectURL = createObjectURL;
    URL.revokeObjectURL = revokeObjectURL;
    const anchorClick = vi.fn();
    const originalCreateElement = document.createElement.bind(document);
    let capturedAnchor: HTMLAnchorElement | null = null;
    document.createElement = ((tagName: string, options?: ElementCreationOptions) => {
      const el = originalCreateElement(tagName, options);
      if (tagName === 'a') {
        capturedAnchor = el as HTMLAnchorElement;
        el.addEventListener('click', (event) => {
          // jsdom would otherwise schedule a (not-implemented) navigation for
          // the blob: href — prevent it; the click itself is what we assert.
          event.preventDefault();
          anchorClick();
        });
      }
      return el;
    }) as typeof document.createElement;

    try {
      const user = userEvent.setup();
      renderPanel();

      await user.click(screen.getByTestId('pack-export-button'));

      // Mutation called against the daemon export route with an empty body
      // ("export with defaults"), then the envelope is downloaded.
      await waitFor(() => expect(anchorClick).toHaveBeenCalledTimes(1));
      expect(exportBody).toEqual({});
      expect(createObjectURL).toHaveBeenCalledTimes(1);
      expect(createObjectURL.mock.calls[0][0]).toBeInstanceOf(Blob);
      expect(capturedAnchor).not.toBeNull();
      expect(capturedAnchor!.download).toBe('My World.json');
      expect(revokeObjectURL).toHaveBeenCalledWith('blob:pack');

      // Success state surfaces the inline status message.
      expect(await screen.findByTestId('pack-export-success')).toHaveTextContent(
        'Pack exported — check your downloads.',
      );
    } finally {
      document.createElement = originalCreateElement;
    }
  });
});

describe('PackPanel — import results (T5)', () => {
  it('renders created/skipped counts for the skip policy', async () => {
    packMock.importMutationFn = async () =>
      makeSummary({
        entries: { created: 3, skipped: 2 },
        relations: { created: 1, skipped: 0 },
      });
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user);

    expect(await screen.findByTestId('pack-import-results')).toBeInTheDocument();
    expect(packMock.importCalls).toHaveLength(1);
    expect(packMock.importCalls[0]).toMatchObject({ worldId: WORLD_ID, conflict: 'skip' });
    expect(packMock.importCalls[0].file).toBe(PACK_FILE);

    // Duplicate testids across the entries/relations blocks — scope by block.
    const [entriesBlock, relationsBlock] = screen.getAllByTestId('pack-atom-counts');
    expect(within(entriesBlock).getByTestId('pack-count-created')).toHaveTextContent('3');
    expect(within(entriesBlock).getByTestId('pack-count-skipped')).toHaveTextContent('2');
    expect(within(entriesBlock).getByTestId('pack-count-overwritten')).toHaveTextContent('0');
    expect(within(relationsBlock).getByTestId('pack-count-created')).toHaveTextContent('1');
    expect(within(relationsBlock).getByTestId('pack-count-skipped')).toHaveTextContent('0');

    // The sr-only live region summarizes the non-zero outcomes (T4 a11y).
    expect(screen.getByTestId('pack-results-live')).toHaveTextContent(
      'Import finished. Entries: 3 created, 2 skipped. Relations: 1 created.',
    );
  });

  it('renders renamed counts for the rename policy', async () => {
    packMock.importMutationFn = async () =>
      makeSummary({
        entries: { created: 1, renamed: 4 },
        relations: { renamed: 2 },
      });
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user, 'rename');

    expect(await screen.findByTestId('pack-import-results')).toBeInTheDocument();
    expect(packMock.importCalls).toHaveLength(1);
    expect(packMock.importCalls[0].conflict).toBe('rename');

    const [entriesBlock, relationsBlock] = screen.getAllByTestId('pack-atom-counts');
    expect(within(entriesBlock).getByTestId('pack-count-renamed')).toHaveTextContent('4');
    expect(within(entriesBlock).getByTestId('pack-count-created')).toHaveTextContent('1');
    expect(within(relationsBlock).getByTestId('pack-count-renamed')).toHaveTextContent('2');
  });

  it('gates the overwrite policy behind the confirm dialog, then renders overwritten counts', async () => {
    packMock.importMutationFn = async () =>
      makeSummary({
        entries: { created: 2, overwritten: 5 },
        relations: { overwritten: 1 },
        details: [
          { kind: 'entry', id: 'e-shadow', outcome: 'overwritten', reason: 'replaced by import' },
        ],
      });
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user, 'overwrite');

    // Confirm dialog appears FIRST — the mutation has not run yet.
    expect(await screen.findByTestId('overwrite-confirm-dialog')).toBeInTheDocument();
    expect(packMock.importCalls).toHaveLength(0);

    await user.click(screen.getByTestId('overwrite-confirm-ok'));

    expect(await screen.findByTestId('pack-import-results')).toBeInTheDocument();
    expect(screen.queryByTestId('overwrite-confirm-dialog')).toBeNull();
    expect(packMock.importCalls).toHaveLength(1);
    expect(packMock.importCalls[0].conflict).toBe('overwrite');

    const [entriesBlock, relationsBlock] = screen.getAllByTestId('pack-atom-counts');
    expect(within(entriesBlock).getByTestId('pack-count-overwritten')).toHaveTextContent('5');
    expect(within(entriesBlock).getByTestId('pack-count-created')).toHaveTextContent('2');
    expect(within(relationsBlock).getByTestId('pack-count-overwritten')).toHaveTextContent('1');

    // Per-atom details render inside the disclosure (row kind + outcome).
    expect(screen.getByTestId('pack-detail-row')).toHaveTextContent('e-shadow');
    expect(screen.getByTestId('pack-detail-row')).toHaveTextContent('Overwritten');
  });
});

describe('PackPanel — overwrite confirm gate (T5)', () => {
  it('cancel closes the dialog without calling the import mutation', async () => {
    packMock.importMutationFn = async () => makeSummary(); // must never run
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user, 'overwrite');

    expect(await screen.findByTestId('overwrite-confirm-dialog')).toBeInTheDocument();

    await user.click(screen.getByTestId('overwrite-confirm-cancel'));

    expect(screen.queryByTestId('overwrite-confirm-dialog')).toBeNull();
    expect(packMock.importCalls).toHaveLength(0);
    expect(screen.queryByTestId('pack-import-results')).toBeNull();
  });
});

describe('PackPanel — import error states (T5)', () => {
  it('surfaces the file-not-JSON parse error inline', async () => {
    packMock.importMutationFn = async () => {
      throw new SyntaxError('Unexpected token } in JSON at position 12');
    };
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user);

    expect(await screen.findByTestId('pack-import-errors')).toHaveTextContent(
      'The selected file is not valid JSON.',
    );
    expect(screen.queryByTestId('pack-import-results')).toBeNull();
  });

  it('surfaces the daemon 403 ownership error inline', async () => {
    packMock.importMutationFn = async () => {
      throw new NexusClientError(403, 'forbidden', 'You do not own this World');
    };
    const user = userEvent.setup();
    renderPanel();

    await uploadAndSubmit(user);

    expect(await screen.findByTestId('pack-import-errors')).toHaveTextContent(
      'You do not have permission to import into this World.',
    );
    expect(screen.queryByTestId('pack-import-results')).toBeNull();
  });
});
