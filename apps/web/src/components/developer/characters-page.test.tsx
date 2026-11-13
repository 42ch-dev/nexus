/**
 * `CharactersPage` (plan v240-actor-web-surfaces T6 + T7) — list, create,
 * bind and the per-character authoring surfaces.
 *
 * The list keeps the server's order; create posts `{ display_name, world_id }`
 * to POST /v1/daemon/characters and refreshes the list; the bindings panel
 * opens per character and adds a World binding via
 * POST /v1/daemon/characters/:id/bindings. Archive/restore are not surfaced
 * (product surfaces never archive a Character — see the wire contract).
 *
 * T7 covers one scoped test per artifact kind behind the row's `Artifacts`
 * action: pending review (capture/list/count/delete), the memory review
 * drain, fragments (list + promote), the SOUL narrative (read + regenerate)
 * and ToM beliefs (list + record, gated on a selected binding). T8 adds the
 * knowledge surface: character-scoped list/get/patch/delete plus creation
 * through the admitted-principal route, with the address split pinned in a
 * test and every `expected_revision` taken from the listed row.
 */
import { http, HttpResponse, type RequestHandler } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';

import { CharactersPage } from '@/components/developer/characters-page';
import { i18n } from '@/lib/i18n/config';
import { BrowserClient } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { renderInApp } from '@/test/test-providers';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent, { type UserEvent } from '@testing-library/user-event';

const PAGINATION = { limit: 20, has_more: false };

interface CharacterItem {
  schema_version: number;
  character_id: string;
  owner_creator_id: string;
  display_name: string;
  status: 'active' | 'archived';
  persona: Record<string, unknown>;
  revision: number;
  created_at: string;
  updated_at: string;
}

function character(overrides: Partial<CharacterItem> = {}): CharacterItem {
  return {
    schema_version: 1,
    character_id: 'chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    owner_creator_id: 'creator-a',
    display_name: 'Aria',
    status: 'active',
    persona: {},
    revision: 0,
    created_at: '2026-11-01T00:00:00Z',
    updated_at: '2026-11-01T00:00:00Z',
    ...overrides,
  };
}

/** Read one string field from a parsed request body, narrowed (not cast). */
function stringField(body: unknown, key: string): string | undefined {
  if (body && typeof body === 'object' && key in body) {
    const value = (body as Record<string, unknown>)[key];
    return typeof value === 'string' ? value : undefined;
  }
  return undefined;
}

const asCreateBody = (body: unknown) => ({
  display_name: stringField(body, 'display_name'),
  world_id: stringField(body, 'world_id'),
});

const asBindBody = (body: unknown) => ({
  world_id: stringField(body, 'world_id'),
});

function renderPage() {
  return renderInApp(<CharactersPage />, { client: new BrowserClient() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('CharactersPage', () => {
  it('lists characters in the server order', async () => {
    useHandlers(
      http.get('/v1/daemon/characters', () =>
        HttpResponse.json({
          items: [
            character({ character_id: 'chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', display_name: 'Second' }),
            character({ character_id: 'chr_cccccccccccccccccccccccccccccccc', display_name: 'First' }),
          ],
          pagination: PAGINATION,
        }),
      ),
    );

    renderPage();

    const rows = await screen.findAllByText(/Second|First/);
    expect(rows.map((r) => r.textContent)).toEqual(['Second', 'First']);
  });

  it('creates a character with display name and owned world, then refreshes the list', async () => {
    let posted: { display_name: string | undefined; world_id: string | undefined } | null = null;
    useHandlers(
      http.get('/v1/daemon/characters', () =>
        HttpResponse.json({
          // After create the list invalidates and re-fetches; serve the new row.
          items: posted?.display_name
            ? [character({ display_name: posted.display_name })]
            : [],
          pagination: PAGINATION,
        }),
      ),
      http.post('/v1/daemon/characters', async ({ request }) => {
        posted = asCreateBody(await request.json());
        return HttpResponse.json({
          character: character({ display_name: posted.display_name ?? '' }),
        });
      }),
    );

    const user = userEvent.setup();
    renderPage();

    await screen.findByText('No characters yet');
    await user.type(screen.getByLabelText('Display name'), 'Nova');
    await user.type(screen.getByLabelText('World'), 'wld_owned1');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(posted).toEqual({ display_name: 'Nova', world_id: 'wld_owned1' }),
    );
    await screen.findByText('Nova');
  });

  it('lists and adds World bindings for the selected character', async () => {
    let bound: { world_id: string | undefined } | null = null;
    const characterId = 'chr_dddddddddddddddddddddddddddddddd';
    useHandlers(
      http.get('/v1/daemon/characters', () =>
        HttpResponse.json({
          items: [character({ character_id: characterId, display_name: 'Aria' })],
          pagination: PAGINATION,
        }),
      ),
      http.get(`/v1/daemon/characters/${characterId}/bindings`, () =>
        HttpResponse.json({
          items: bound?.world_id
            ? [
                {
                  schema_version: 1,
                  binding_id: 'awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
                  character_id: characterId,
                  world_id: bound.world_id,
                  status: 'active',
                  revision: 0,
                  created_at: '2026-11-01T00:00:00Z',
                  updated_at: '2026-11-01T00:00:00Z',
                },
              ]
            : [],
          pagination: PAGINATION,
        }),
      ),
      http.post(`/v1/daemon/characters/${characterId}/bindings`, async ({ request }) => {
        bound = asBindBody(await request.json());
        return HttpResponse.json({
          binding: {
            schema_version: 1,
            binding_id: 'awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
            character_id: characterId,
            world_id: bound.world_id ?? '',
            status: 'active',
            revision: 0,
            created_at: '2026-11-01T00:00:00Z',
            updated_at: '2026-11-01T00:00:00Z',
          },
        });
      }),
    );

    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole('button', { name: 'Bindings' }));
    await screen.findByText('This character is not bound to any world.');

    await user.type(screen.getByLabelText('World ID'), 'wld_realm2');
    await user.click(screen.getByRole('button', { name: 'Bind' }));

    await waitFor(() => expect(bound).toEqual({ world_id: 'wld_realm2' }));
    await screen.findByText('wld_realm2');
  });

  describe('artifact surfaces (T7)', () => {
    const characterId = 'chr_dddddddddddddddddddddddddddddddd';
    const binding = {
      schema_version: 1,
      binding_id: 'awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
      character_id: characterId,
      world_id: 'wld_realm1',
      status: 'active',
      revision: 0,
      created_at: '2026-11-01T00:00:00Z',
      updated_at: '2026-11-01T00:00:00Z',
    };
    const SOUL_BASE = {
      character_id: characterId,
      stale: false,
      current_fragment_count: 0,
      current_distinct_keyword_count: 0,
      min_fragment_count: 10,
      min_distinct_keyword_count: 20,
    };

    /** Baseline handlers for every query the artifact panels issue on open. */
    function artifactsBaseline() {
      return [
        http.get(`/v1/daemon/characters/${characterId}/bindings`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/memory/pending-review`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/memory/pending-review/count`, () =>
          HttpResponse.json({ count: 0 }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/memory/fragments`, () =>
          HttpResponse.json({ fragments: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/knowledge`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.post(`/v1/daemon/characters/${characterId}/soul/reflect`, () =>
          HttpResponse.json({ ...SOUL_BASE, state: 'ungenerated' }),
        ),
      ];
    }

    /**
     * Register the page + per-test handlers; the baseline fills the remaining
     * artifact queries. msw picks the FIRST matching handler in a batch, so
     * per-test handlers must precede the baseline.
     */
    function useArtifactsPage(handlers: RequestHandler[] = []) {
      useHandlers(
        http.get('/v1/daemon/characters', () =>
          HttpResponse.json({
            items: [character({ character_id: characterId, display_name: 'Aria' })],
            pagination: PAGINATION,
          }),
        ),
        ...handlers,
        ...artifactsBaseline(),
      );
    }

    async function openArtifacts(user: UserEvent) {
      renderPage();
      await user.click(await screen.findByRole('button', { name: 'Artifacts' }));
    }

    it('captures, lists, counts and deletes pending review digests', async () => {
      let rows: object[] = [];
      let captured: { pending_id: unknown; session_id: unknown; raw_digest: unknown } | null = null;
      let deleted: string | null = null;
      useArtifactsPage([
        http.get(`/v1/daemon/characters/${characterId}/memory/pending-review`, () =>
          HttpResponse.json({ items: rows, pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/memory/pending-review/count`, () =>
          HttpResponse.json({ count: rows.length }),
        ),
        http.post(`/v1/daemon/characters/${characterId}/memory/pending-review`, async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          captured = {
            pending_id: body.pending_id,
            session_id: body.session_id,
            raw_digest: body.raw_digest,
          };
          rows = [
            {
              pending_id: 'pending-1',
              session_id: body.session_id,
              character_id: characterId,
              task_kind: 'unknown',
              raw_digest: body.raw_digest,
              created_at: '2026-11-10T00:00:00Z',
              source_operation_id: null,
            },
          ];
          return HttpResponse.json({ success: true, pending_id: 'pending-1' });
        }),
        http.delete(
          `/v1/daemon/characters/${characterId}/memory/pending-review/pending-1`,
          () => {
            deleted = 'pending-1';
            rows = [];
            return HttpResponse.json({ success: true, pending_id: 'pending-1' });
          },
        ),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      await screen.findByText('No pending digests.');
      await user.type(screen.getByLabelText('Session ID'), 'ses_1234');
      await user.type(screen.getByLabelText('Raw digest'), 'The party crossed the bridge.');
      await user.click(screen.getByRole('button', { name: 'Capture' }));

      await waitFor(() =>
        expect(captured).toEqual({
          pending_id: expect.any(String),
          session_id: 'ses_1234',
          raw_digest: 'The party crossed the bridge.',
        }),
      );
      await screen.findByText('The party crossed the bridge.');
      await screen.findByText('1 pending');

      await user.click(screen.getByRole('button', { name: 'Delete' }));
      await waitFor(() => expect(deleted).toBe('pending-1'));
      await screen.findByText('No pending digests.');
    });

    it('drains one review batch and reports the classifier counts', async () => {
      let drained = false;
      useArtifactsPage([
        http.post(`/v1/daemon/characters/${characterId}/memory/review`, async ({ request }) => {
          drained = true;
          expect(await request.json()).toEqual({});
          return HttpResponse.json({
            promoted: 2,
            fragmented: 1,
            dropped: 0,
            processed: 3,
            has_more: true,
          });
        }),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      await user.click(await screen.findByRole('button', { name: 'Run review' }));
      await waitFor(() => expect(drained).toBe(true));
      await screen.findByText('Promoted 2 · fragmented 1 · dropped 0 · processed 3');
      await screen.findByText('More remains — run review again.');
    });

    it('lists fragments and promotes one with its observed revision', async () => {
      let fragments: object[] = [
        {
          fragment_id: 'frg_11111111111111111111111111111111',
          session_id: 'ses_1234',
          character_id: characterId,
          binding_id: binding.binding_id,
          summary: 'The bridge toll doubled.',
          keywords: ['bridge', 'toll'],
          created_at: '2026-11-10T00:00:00Z',
          revision: 3,
        },
      ];
      let promoted: { fragment_id: string; expected_revision: unknown } | null = null;
      useArtifactsPage([
        http.get(`/v1/daemon/characters/${characterId}/memory/fragments`, () =>
          HttpResponse.json({ fragments, pagination: PAGINATION }),
        ),
        http.post(
          `/v1/daemon/characters/${characterId}/memory/fragments/frg_11111111111111111111111111111111:promote`,
          async ({ request }) => {
            promoted = {
              fragment_id: 'frg_11111111111111111111111111111111',
              expected_revision: (await request.json() as Record<string, unknown>).expected_revision,
            };
            fragments = [];
            return HttpResponse.json({
              fragment: { fragment_id: 'frg_11111111111111111111111111111111', summary: 'The bridge toll doubled.' },
            });
          },
        ),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      await screen.findByText('The bridge toll doubled.');
      await user.click(screen.getByRole('button', { name: 'Promote' }));

      await waitFor(() =>
        expect(promoted).toEqual({
          fragment_id: 'frg_11111111111111111111111111111111',
          expected_revision: 3,
        }),
      );
      await screen.findByText('No fragments yet.');
    });

    it('reads the SOUL narrative and regenerates it on demand', async () => {
      const seen: boolean[] = [];
      let regenerated = false;
      useArtifactsPage([
        http.post(`/v1/daemon/characters/${characterId}/soul/reflect`, async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          seen.push(body.force_regenerate === true);
          if (body.force_regenerate) regenerated = true;
          return HttpResponse.json({
            ...SOUL_BASE,
            state: 'current',
            narrative: regenerated ? 'A new reflection.' : 'Aria keeps to the road.',
            current_fragment_count: 12,
            current_distinct_keyword_count: 25,
          });
        }),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      // The read path fires on mount with force_regenerate=false.
      await screen.findByText('Aria keeps to the road.');
      expect(seen).toEqual([false]);

      await user.click(screen.getByRole('button', { name: 'Regenerate' }));
      await screen.findByText('A new reflection.');
      // Read on mount, explicit regenerate, then the cache-busting refetch.
      await waitFor(() => expect(seen).toEqual([false, true, false]));
    });

    it('never lists ToM beliefs without a binding selected', async () => {
      let tomCalls = 0;
      useArtifactsPage([
        http.get(`/v1/daemon/characters/${characterId}/tom`, () => {
          tomCalls += 1;
          return HttpResponse.json({ items: [], pagination: PAGINATION });
        }),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      await screen.findByText('Bind the character to a world before reading beliefs.');
      // The list query is `enabled`-gated on the binding selection; with no
      // binding row and an empty selection there is no code path that fires it.
      expect(tomCalls).toBe(0);
    });

    it('lists ToM beliefs for the selected binding and records a new belief', async () => {
      let tomCalls = 0;
      let listedWith: { binding_id: string | null; world_id: string | null } | null = null;
      let recorded: Record<string, unknown> | null = null;
      useArtifactsPage([
        http.get(`/v1/daemon/characters/${characterId}/bindings`, () =>
          HttpResponse.json({ items: [binding], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${characterId}/tom`, ({ request }) => {
          tomCalls += 1;
          const url = new URL(request.url);
          listedWith = {
            binding_id: url.searchParams.get('binding_id'),
            world_id: url.searchParams.get('world_id'),
          };
          return HttpResponse.json({
            items: [
              {
                carrier_entry_id: 'kne_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                row_ordinal: 0,
                holder: characterId,
                proposition: 'The bridge is unsafe.',
                order: 1,
                truth: 'True',
              },
            ],
            pagination: PAGINATION,
          });
        }),
        http.post(`/v1/daemon/characters/${characterId}/tom`, async ({ request }) => {
          recorded = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({
            carrier_entry_id: 'kne_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
            revision: 1,
            mind_state_id: 'mind_cccccccccccccccccccccccccccccccc',
          });
        }),
      ]);

      const user = userEvent.setup();
      await openArtifacts(user);

      // No binding pre-selected → still no list call.
      await screen.findByLabelText('Binding');
      expect(tomCalls).toBe(0);

      await user.selectOptions(screen.getByLabelText('Binding'), binding.binding_id);
      await screen.findByText('The bridge is unsafe.');
      expect(tomCalls).toBe(1);
      expect(listedWith).toEqual({ binding_id: binding.binding_id, world_id: 'wld_realm1' });

      await user.type(screen.getByLabelText('Carrier entry ID'), 'kne_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
      await user.type(screen.getByLabelText('Carrier revision'), '0');
      await user.type(screen.getByLabelText('Holder (character ID)'), characterId);
      await user.type(screen.getByLabelText('Proposition'), 'The toll keeper lies.');
      await user.type(screen.getByLabelText('Order (1 = self, 2 = other)'), '1');
      await user.click(screen.getByRole('button', { name: 'Record' }));

      await waitFor(() =>
        expect(recorded).toEqual({
          world_id: 'wld_realm1',
          binding_id: binding.binding_id,
          carrier_entry_id: 'kne_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
          expected_revision: 0,
          holder: characterId,
          proposition: 'The toll keeper lies.',
          order: 1,
        }),
      );
    });

    describe('knowledge surface (T8)', () => {
      const entryId = 'kne_11111111111111111111111111111111';
      // revision 3 (not 0): proves expected_revision comes from the list row.
      const entry = {
        entry_id: entryId,
        owner: { kind: 'character', id: characterId },
        holder_entry_id: 'hld_11111111111111111111111111111111',
        disclosure: 'owner-private',
        block_type: 'character',
        canonical_name: 'Mira',
        status: 'confirmed',
        revision: 3,
        created_at: '2026-11-10T00:00:00Z',
      };
      const knowledgeList = (items: object[]) =>
        http.get(`/v1/daemon/characters/${characterId}/knowledge`, () =>
          HttpResponse.json({ items, pagination: PAGINATION }),
        );

      it('lists knowledge entries for the character', async () => {
        useArtifactsPage([knowledgeList([entry])]);

        const user = userEvent.setup();
        await openArtifacts(user);

        await screen.findByText('Mira');
        await screen.findByText('character · rev 3 · owner-private');
      });

      it('reads one entry and patches name/summary/audience under the list revision', async () => {
        let detailCalls = 0;
        let patched: Record<string, unknown> | null = null;
        useArtifactsPage([
          knowledgeList([entry]),
          http.get(`/v1/daemon/characters/${characterId}/knowledge/${entryId}`, () => {
            detailCalls += 1;
            return HttpResponse.json({ item: entry, summary: 'the harbor pilot' });
          }),
          http.patch(`/v1/daemon/characters/${characterId}/knowledge/${entryId}`, async ({ request }) => {
            patched = (await request.json()) as Record<string, unknown>;
            return HttpResponse.json({
              item: { ...entry, revision: 4 },
              summary: 'the harbor pilot',
            });
          }),
        ]);

        const user = userEvent.setup();
        await openArtifacts(user);

        await screen.findByText('Mira');
        expect(detailCalls).toBe(0);
        await user.click(screen.getByRole('button', { name: 'Edit' }));

        // The read fires only when a row is opened for editing, and the
        // returned summary prefills the form.
        const form = await screen.findByRole('form', { name: 'Edit entry' });
        await waitFor(() => expect(detailCalls).toBe(1));
        const nameInput = within(form).getByLabelText('Canonical name');
        expect(nameInput).toHaveValue('Mira');
        expect(within(form).getByLabelText('Summary')).toHaveValue('the harbor pilot');

        await user.clear(nameInput);
        await user.type(nameInput, 'Mira Vell');
        await user.selectOptions(within(form).getByLabelText('Audience'), 'shared');
        await user.click(within(form).getByRole('button', { name: 'Save' }));

        await waitFor(() =>
          expect(patched).toEqual({
            expected_revision: 3,
            canonical_name: 'Mira Vell',
            summary: 'the harbor pilot',
            audience: { kind: 'shared' },
          }),
        );
      });

      it('deletes an entry with the list revision as a query member', async () => {
        let deletedUrl = '';
        useArtifactsPage([
          knowledgeList([entry]),
          http.delete(`/v1/daemon/characters/${characterId}/knowledge/${entryId}`, ({ request }) => {
            deletedUrl = request.url;
            return new HttpResponse(null, { status: 204 });
          }),
        ]);

        const user = userEvent.setup();
        await openArtifacts(user);

        await screen.findByText('Mira');
        await user.click(screen.getByRole('button', { name: 'Delete' }));

        await waitFor(() => expect(deletedUrl).not.toBe(''));
        const url = new URL(deletedUrl);
        expect(url.pathname).toBe(`/v1/daemon/characters/${characterId}/knowledge/${entryId}`);
        expect(url.searchParams.get('expected_revision')).toBe('3');
      });

      it('creates an entry through the admitted-principal route, not the character path', async () => {
        let createdUrl = '';
        let createdBody: Record<string, unknown> | null = null;
        let characterPathPosts = 0;
        useArtifactsPage([
          // The character knowledge path has no create; a POST there is the
          // address-split violation this test pins against.
          http.post(`/v1/daemon/characters/${characterId}/knowledge`, () => {
            characterPathPosts += 1;
            return HttpResponse.json({ item: entry }, { status: 201 });
          }),
          http.post('/v1/daemon/actor-knowledge/entries', async ({ request }) => {
            createdUrl = request.url;
            createdBody = (await request.json()) as Record<string, unknown>;
            return HttpResponse.json({ item: entry }, { status: 201 });
          }),
        ]);

        const user = userEvent.setup();
        await openArtifacts(user);

        const form = await screen.findByRole('form', { name: 'Create entry' });
        await user.type(within(form).getByLabelText('Canonical name'), 'Mira');
        await user.type(within(form).getByLabelText('Summary'), 'the harbor pilot');
        await user.click(within(form).getByRole('button', { name: 'Create' }));

        await waitFor(() => expect(createdBody).not.toBeNull());
        expect(new URL(createdUrl).pathname).toBe('/v1/daemon/actor-knowledge/entries');
        expect(createdBody).toEqual({
          owner_kind: 'character',
          character_id: characterId,
          block_type: 'character',
          canonical_name: 'Mira',
          summary: 'the harbor pilot',
          audience: { kind: 'shared' },
        });
        expect(characterPathPosts).toBe(0);
      });
    });
  });

  describe('cross-character isolation (F-001/F-002 regression guard)', () => {
    const charA = 'chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
    const charB = 'chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
    const entryA = {
      entry_id: 'kne_11111111111111111111111111111111',
      owner: { kind: 'character', id: charA },
      holder_entry_id: 'hld_11111111111111111111111111111111',
      disclosure: 'owner-private',
      block_type: 'character',
      canonical_name: 'Mira',
      status: 'confirmed',
      revision: 3,
      created_at: '2026-11-10T00:00:00Z',
    };
    const SOUL = (character_id: string) => ({
      character_id,
      stale: false,
      state: 'ungenerated',
      current_fragment_count: 0,
      current_distinct_keyword_count: 0,
      min_fragment_count: 10,
      min_distinct_keyword_count: 20,
    });

    it('drops per-character panel state and never reads the cross pair', async () => {
      // Every knowledge detail read issued for character B. Without the panel
      // remount key, switching A → B carries `editingId` over and fires a
      // detail read for (B, A's entry) — the cross pair this test pins down.
      const bDetailReads: string[] = [];
      useHandlers(
        http.get('/v1/daemon/characters', () =>
          HttpResponse.json({
            items: [
              character({ character_id: charA, display_name: 'Aria' }),
              character({ character_id: charB, display_name: 'Bex' }),
            ],
            pagination: PAGINATION,
          }),
        ),
        http.get(`/v1/daemon/characters/${charA}/bindings`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charB}/bindings`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charA}/memory/pending-review`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charA}/memory/pending-review/count`, () =>
          HttpResponse.json({ count: 0 }),
        ),
        http.get(`/v1/daemon/characters/${charB}/memory/pending-review`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charB}/memory/pending-review/count`, () =>
          HttpResponse.json({ count: 0 }),
        ),
        http.get(`/v1/daemon/characters/${charA}/memory/fragments`, () =>
          HttpResponse.json({ fragments: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charB}/memory/fragments`, () =>
          HttpResponse.json({ fragments: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charA}/knowledge`, () =>
          HttpResponse.json({ items: [entryA], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charB}/knowledge`, () =>
          HttpResponse.json({ items: [], pagination: PAGINATION }),
        ),
        http.get(`/v1/daemon/characters/${charA}/knowledge/${entryA.entry_id}`, () =>
          HttpResponse.json({ item: entryA, summary: 'the harbor pilot' }),
        ),
        http.get(`/v1/daemon/characters/${charB}/knowledge/:entryId`, ({ params }) => {
          bDetailReads.push(String(params.entryId));
          return HttpResponse.json({ item: entryA, summary: '' });
        }),
        http.post(`/v1/daemon/characters/${charA}/soul/reflect`, () =>
          HttpResponse.json(SOUL(charA)),
        ),
        http.post(`/v1/daemon/characters/${charB}/soul/reflect`, () =>
          HttpResponse.json(SOUL(charB)),
        ),
      );

      const user = userEvent.setup();
      renderPage();

      const ariaRow = (await screen.findByText('Aria')).closest('li') as HTMLElement | null;
      const bexRow = screen.getByText('Bex').closest('li') as HTMLElement | null;
      if (!ariaRow || !bexRow) throw new Error('character rows missing');

      // Bindings panel for A: leave a draft in the form.
      await user.click(within(ariaRow).getByRole('button', { name: 'Bindings' }));
      await screen.findByText('World bindings — Aria');
      await user.type(screen.getByLabelText('World ID'), 'wld-from-aria');

      // Artifacts panel for A: leave a pending digest draft and open the
      // knowledge edit form (selects A's entry).
      await user.click(within(ariaRow).getByRole('button', { name: 'Artifacts' }));
      await screen.findByText('Pending review — Aria');
      await user.type(screen.getByLabelText('Raw digest'), 'digest-from-aria');
      await screen.findByText('Mira');
      await user.click(screen.getByRole('button', { name: 'Edit' }));
      const editForm = await screen.findByRole('form', { name: 'Edit entry' });
      await within(editForm).findByDisplayValue('Mira');

      // Switch both panels to B.
      await user.click(within(bexRow).getByRole('button', { name: 'Bindings' }));
      await screen.findByText('World bindings — Bex');
      // The bindings draft must not survive the switch.
      expect(screen.getByLabelText('World ID')).toHaveValue('');
      await user.click(within(bexRow).getByRole('button', { name: 'Artifacts' }));
      await screen.findByText('Pending review — Bex');

      // Nothing from A carries over: no draft, no open edit form, no entry.
      expect(screen.getByLabelText('Raw digest')).toHaveValue('');
      expect(screen.queryByRole('form', { name: 'Edit entry' })).toBeNull();
      expect(screen.queryByText('Mira')).toBeNull();
      await screen.findByText('No knowledge entries yet.');
      // B's knowledge list resolving means B's panel queries have settled; by
      // then a cross-pair detail read (the F-001 leak) would have been issued.
      await waitFor(() => expect(bDetailReads).toEqual([]));
    });
  });
});
