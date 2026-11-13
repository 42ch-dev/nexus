/**
 * Character management surface (plan v240-actor-web-surfaces T6) — list,
 * create and World-bind Creator-owned characters behind the single
 * `Characters` rail entry (compass D7/D8: lives in the developer rail, reads
 * as a creator-facing surface).
 *
 * `/characters` is the single owner of the character surface: the list, the
 * create form (name + owned World for the atomic initial binding) and the
 * per-character binding panel all live here once (T6). T7 adds the authoring
 * surfaces for the selected character — pending review, memory review drain,
 * fragments, SOUL narrative and ToM beliefs — behind the row's `Artifacts`
 * action; T8 adds the knowledge browser/editor with the character-scoped
 * read/update/delete vs admitted-principal create split. Archive/restore are
 * deliberately absent — the wire contract notes product surfaces never
 * archive a Character, so the list is usable without them.
 *
 * Markup pattern follows `sessions-page.tsx`: app wiring (queries, router
 * reads, i18n copy) in this file, presentational pieces from `@/components/ui`.
 */
import { useState, type FormEvent } from 'react';
import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  AddKnowledgeEntryRequest,
  CaptureCharacterPendingReviewRequest,
  CharacterDetail,
  CharacterMemoryFragmentInfo,
  KnowledgeViewItem,
  UpdateKnowledgeEntryRequest,
} from '@42ch/nexus-contracts';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Textarea } from '@/components/ui/textarea';
import { useNexusClient } from '@/lib/client-context';
import { humanizeStatus, shortId } from '@/lib/format';
import { queryKeys } from '@/lib/nexus/query-keys';

/** `/characters` page entry — the registry's `load` target. */
export function CharactersPage() {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();

  const characters = useQuery({
    queryKey: queryKeys.characters.list(),
    queryFn: () => client.listCharacters(),
  });
  const create = useMutation({
    mutationFn: (request: { display_name: string; world_id: string }) =>
      client.createCharacter(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.lists() });
    },
  });

  const [name, setName] = useState('');
  const [worldId, setWorldId] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [artifactId, setArtifactId] = useState<string | null>(null);

  const submitCreate = (event: FormEvent) => {
    event.preventDefault();
    const display_name = name.trim();
    const world_id = worldId.trim();
    if (!display_name || !world_id) return;
    create.mutate(
      { display_name, world_id },
      {
        onSuccess: () => {
          setName('');
          setWorldId('');
        },
      },
    );
  };

  const items = characters.data?.items ?? [];

  return (
    <div className="flex flex-col gap-4">
      <Card className="shadow-card">
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <div>
              <CardTitle>{t('title')}</CardTitle>
              <CardDescription>{t('description')}</CardDescription>
            </div>
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => characters.refetch()}
              disabled={characters.isFetching}
              aria-label={t('refreshAria')}
            >
              <RefreshCw
                className={`h-4 w-4 ${characters.isFetching ? 'animate-spin' : ''}`}
                aria-hidden
              />
              {t('refresh')}
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <form onSubmit={submitCreate} className="flex flex-col gap-3" aria-label={t('create.title')}>
            <p className="text-copy-14 font-medium">{t('create.title')}</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="flex flex-col gap-1">
                <Label htmlFor="character-name">{t('create.nameLabel')}</Label>
                <Input
                  id="character-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder={t('create.namePlaceholder')}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="character-world">{t('create.worldLabel')}</Label>
                <Input
                  id="character-world"
                  value={worldId}
                  onChange={(e) => setWorldId(e.target.value)}
                  placeholder={t('create.worldPlaceholder')}
                />
              </div>
            </div>
            <div className="flex items-center gap-3">
              <Button type="submit" size="small" disabled={create.isPending}>
                {create.isPending ? t('create.submitting') : t('create.submit')}
              </Button>
              {create.isError && (
                <p role="alert" className="text-copy-13 text-red-700">
                  {t('createError')}
                </p>
              )}
            </div>
          </form>

          {characters.isError ? (
            <ErrorState
              title={t('errorTitle')}
              description={t('errorDescription')}
              onRetry={() => characters.refetch()}
            />
          ) : characters.isLoading ? (
            <LoadingState label={t('loading')} />
          ) : items.length === 0 ? (
            <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
          ) : (
            <ul className="flex flex-col divide-y rounded-card border">
              {items.map((c) => (
                <li
                  key={c.character_id}
                  className="flex items-center justify-between gap-2 px-4 py-3"
                >
                  <div>
                    <p className="text-copy-14 font-medium">{c.display_name}</p>
                    <p className="text-copy-13 text-gray-600">
                      {shortId(c.character_id)} · {humanizeStatus(c.status)}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      onClick={() =>
                        setArtifactId(artifactId === c.character_id ? null : c.character_id)
                      }
                    >
                      {t('list.artifactsAction')}
                    </Button>
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      onClick={() =>
                        setSelectedId(selectedId === c.character_id ? null : c.character_id)
                      }
                    >
                      {t('list.bindingsAction')}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      {selectedId &&
        (() => {
          const selected = items.find((c) => c.character_id === selectedId);
          return selected ? (
            <BindingsPanel
              key={`bindings:${selected.character_id}`}
              character={selected}
              onClose={() => setSelectedId(null)}
            />
          ) : null;
        })()}

      {artifactId &&
        (() => {
          const selected = items.find((c) => c.character_id === artifactId);
          return selected ? (
            <CharacterArtifactsPanel
              key={`artifacts:${selected.character_id}`}
              character={selected}
            />
          ) : null;
        })()}
    </div>
  );
}

/** Per-character World bindings: list + add (POST /v1/daemon/characters/:id/bindings). */
function BindingsPanel({
  character,
  onClose,
}: {
  character: CharacterDetail['character'];
  onClose: () => void;
}) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();

  const bindings = useQuery({
    queryKey: queryKeys.characters.bindings.list(character.character_id),
    queryFn: () => client.listCharacterBindings(character.character_id),
  });
  const addBinding = useMutation({
    mutationFn: (request: { world_id: string; world_sheet_entry_id?: string }) =>
      client.addCharacterBinding(character.character_id, request),
    onSuccess: () => {
      void qc.invalidateQueries({
        queryKey: queryKeys.characters.bindings.lists(),
      });
    },
  });

  const [worldId, setWorldId] = useState('');
  const [sheetId, setSheetId] = useState('');

  const submitBind = (event: FormEvent) => {
    event.preventDefault();
    const world_id = worldId.trim();
    if (!world_id) return;
    const sheet = sheetId.trim();
    addBinding.mutate(
      sheet ? { world_id, world_sheet_entry_id: sheet } : { world_id },
      {
        onSuccess: () => {
          setWorldId('');
          setSheetId('');
        },
      },
    );
  };

  const items = bindings.data?.items ?? [];

  return (
    <Card className="shadow-card" data-testid="bindings-panel">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{t('bindings.title', { name: character.display_name })}</CardTitle>
          </div>
          <Button type="button" variant="tertiary" size="small" onClick={onClose}>
            {t('bindings.close')}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {bindings.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('bindings.error')}
          </p>
        ) : bindings.isLoading ? (
          <LoadingState label={t('bindings.loading')} />
        ) : items.length === 0 ? (
          <EmptyState title={t('bindings.empty')} description="" />
        ) : (
          <ul className="flex flex-col divide-y rounded-card border">
            {items.map((b) => (
              <li key={b.binding_id} className="flex items-center justify-between gap-2 px-4 py-3">
                <p className="text-copy-14">{b.world_id}</p>
                <p className="text-copy-13 text-gray-600">
                  {b.world_sheet_entry_id ? shortId(b.world_sheet_entry_id) : '—'}
                </p>
              </li>
            ))}
          </ul>
        )}

        <form onSubmit={submitBind} className="flex flex-col gap-3" aria-label={t('bindings.addTitle')}>
          <p className="text-copy-14 font-medium">{t('bindings.addTitle')}</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="binding-world">{t('bindings.worldLabel')}</Label>
              <Input
                id="binding-world"
                value={worldId}
                onChange={(e) => setWorldId(e.target.value)}
                placeholder={t('bindings.worldPlaceholder')}
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="binding-sheet">{t('bindings.sheetLabel')}</Label>
              <Input
                id="binding-sheet"
                value={sheetId}
                onChange={(e) => setSheetId(e.target.value)}
                placeholder={t('bindings.sheetPlaceholder')}
              />
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" size="small" disabled={addBinding.isPending}>
              {addBinding.isPending ? t('bindings.submitting') : t('bindings.submit')}
            </Button>
            {addBinding.isError && (
              <p role="alert" className="text-copy-13 text-red-700">
                {t('bindError')}
              </p>
            )}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

/**
 * Per-character authoring surfaces (plan v240 T7, I-000127): pending review,
 * memory review drain, fragments, SOUL narrative and ToM beliefs. One section
 * per artifact kind; every section is scoped to the character opened from the
 * list. The ToM section reads bindings through the same query key the
 * `BindingsPanel` uses, so both share Task 6's cache entry.
 */
function CharacterArtifactsPanel({ character }: { character: CharacterDetail['character'] }) {
  return (
    <div className="flex flex-col gap-4">
      <PendingReviewSection character={character} />
      <ReviewSection characterId={character.character_id} />
      <FragmentsSection characterId={character.character_id} />
      <SoulSection characterId={character.character_id} />
      <TomSection character={character} />
      <KnowledgeSection character={character} />
    </div>
  );
}

/** Pending review queue: count + list + capture + delete. */
function PendingReviewSection({ character }: { character: CharacterDetail['character'] }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();
  const characterId = character.character_id;

  const pending = useQuery({
    queryKey: queryKeys.characters.memory.pendingList(characterId),
    queryFn: () => client.listCharacterPendingReviews(characterId),
  });
  const count = useQuery({
    queryKey: queryKeys.characters.memory.count(characterId),
    queryFn: () => client.countCharacterPendingReviews(characterId),
  });
  const capture = useMutation({
    mutationFn: (request: CaptureCharacterPendingReviewRequest) =>
      client.captureCharacterPendingReview(characterId, request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.pendingLists() });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.count(characterId) });
    },
  });
  const remove = useMutation({
    mutationFn: (pendingId: string) => client.deleteCharacterPendingReview(characterId, pendingId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.pendingLists() });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.count(characterId) });
    },
  });

  const [sessionId, setSessionId] = useState('');
  const [digest, setDigest] = useState('');
  const [taskKind, setTaskKind] = useState('');
  const [bindingId, setBindingId] = useState('');

  const submitCapture = (event: FormEvent) => {
    event.preventDefault();
    const session_id = sessionId.trim();
    const raw_digest = digest.trim();
    if (!session_id || !raw_digest) return;
    const task_kind = taskKind.trim();
    const binding_id = bindingId.trim();
    capture.mutate(
      {
        pending_id: crypto.randomUUID(),
        session_id,
        raw_digest,
        ...(task_kind ? { task_kind } : {}),
        ...(binding_id ? { binding_id } : {}),
      },
      {
        onSuccess: () => {
          setSessionId('');
          setDigest('');
          setTaskKind('');
          setBindingId('');
        },
      },
    );
  };

  const items = pending.data?.items ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{t('pending.title', { name: character.display_name })}</CardTitle>
        {count.data && <CardDescription>{t('pending.count', { count: count.data.count })}</CardDescription>}
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {pending.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('pending.error')}
          </p>
        ) : pending.isLoading ? (
          <LoadingState label={t('pending.loading')} />
        ) : items.length === 0 ? (
          <EmptyState title={t('pending.empty')} description="" />
        ) : (
          <ul className="flex flex-col divide-y rounded-card border">
            {items.map((p) => (
              <li
                key={p.pending_id}
                className="flex items-center justify-between gap-2 px-4 py-3"
              >
                <div>
                  <p className="text-copy-14">{p.raw_digest}</p>
                  <p className="text-copy-13 text-gray-600">
                    {p.task_kind} · {shortId(p.session_id)}
                  </p>
                </div>
                <Button
                  type="button"
                  variant="tertiary"
                  size="small"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(p.pending_id)}
                >
                  {t('pending.delete')}
                </Button>
              </li>
            ))}
          </ul>
        )}
        {remove.isError && (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('pending.deleteError')}
          </p>
        )}

        <form onSubmit={submitCapture} className="flex flex-col gap-3" aria-label={t('pending.captureTitle')}>
          <p className="text-copy-14 font-medium">{t('pending.captureTitle')}</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="pending-session">{t('pending.sessionLabel')}</Label>
              <Input
                id="pending-session"
                value={sessionId}
                onChange={(e) => setSessionId(e.target.value)}
                placeholder={t('pending.sessionPlaceholder')}
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="pending-kind">{t('pending.kindLabel')}</Label>
              <Input
                id="pending-kind"
                value={taskKind}
                onChange={(e) => setTaskKind(e.target.value)}
                placeholder={t('pending.kindPlaceholder')}
              />
            </div>
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="pending-digest">{t('pending.digestLabel')}</Label>
            <Textarea
              id="pending-digest"
              value={digest}
              onChange={(e) => setDigest(e.target.value)}
              placeholder={t('pending.digestPlaceholder')}
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="pending-binding">{t('pending.bindingLabel')}</Label>
            <Input
              id="pending-binding"
              value={bindingId}
              onChange={(e) => setBindingId(e.target.value)}
              placeholder={t('pending.bindingPlaceholder')}
            />
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" size="small" disabled={capture.isPending}>
              {capture.isPending ? t('pending.submitting') : t('pending.submit')}
            </Button>
            {capture.isError && (
              <p role="alert" className="text-copy-13 text-red-700">
                {t('pending.captureError')}
              </p>
            )}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

/** Memory review drain: one bounded batch through the shared review pipeline. */
function ReviewSection({ characterId }: { characterId: string }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();

  const review = useMutation({
    mutationFn: () => client.reviewCharacterMemory(characterId, {}),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.pendingLists() });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.count(characterId) });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.fragments(characterId) });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.soul.narrative(characterId) });
    },
  });

  const result = review.data;

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{t('review.title')}</CardTitle>
        <CardDescription>{t('review.description')}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <Button type="button" size="small" disabled={review.isPending} onClick={() => review.mutate()}>
            {review.isPending ? t('review.running') : t('review.run')}
          </Button>
          {review.isError && (
            <p role="alert" className="text-copy-13 text-red-700">
              {t('review.error')}
            </p>
          )}
        </div>
        {result && (
          <>
            <p className="text-copy-14">
              {t('review.result', {
                promoted: result.promoted,
                fragmented: result.fragmented,
                dropped: result.dropped,
                processed: result.processed ?? 0,
              })}
            </p>
            {result.has_more && <p className="text-copy-13 text-gray-600">{t('review.more')}</p>}
          </>
        )}
      </CardContent>
    </Card>
  );
}

/** Binding-local memory fragments: list + explicit revision-checked promotion. */
function FragmentsSection({ characterId }: { characterId: string }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();

  const fragments = useQuery({
    queryKey: queryKeys.characters.memory.fragments(characterId),
    queryFn: () => client.listCharacterMemoryFragments(characterId),
  });
  const promote = useMutation({
    mutationFn: (fragment: CharacterMemoryFragmentInfo) =>
      client.promoteCharacterFragment(characterId, fragment.fragment_id, {
        expected_revision: fragment.revision,
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.memory.fragments(characterId) });
    },
  });

  const items = fragments.data?.fragments ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{t('fragments.title')}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {fragments.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('fragments.error')}
          </p>
        ) : fragments.isLoading ? (
          <LoadingState label={t('fragments.loading')} />
        ) : items.length === 0 ? (
          <EmptyState title={t('fragments.empty')} description="" />
        ) : (
          <ul className="flex flex-col divide-y rounded-card border">
            {items.map((f) => (
              <li
                key={f.fragment_id}
                className="flex items-center justify-between gap-2 px-4 py-3"
              >
                <div>
                  <p className="text-copy-14">{f.summary}</p>
                  <p className="text-copy-13 text-gray-600">
                    {f.keywords.join(', ')} · rev {f.revision}
                  </p>
                </div>
                {f.binding_id && (
                  <Button
                    type="button"
                    variant="tertiary"
                    size="small"
                    disabled={promote.isPending}
                    onClick={() => promote.mutate(f)}
                  >
                    {t('fragments.promote')}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
        {promote.isError && (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('fragments.promoteError')}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

/** SOUL narrative: read the cache; regenerate explicitly on demand. */
function SoulSection({ characterId }: { characterId: string }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();

  // Reflect with force_regenerate=false is the read path (cache hit or gate
  // report); synthesis only runs from the explicit regenerate mutation.
  const soul = useQuery({
    queryKey: queryKeys.characters.soul.narrative(characterId),
    queryFn: () => client.reflectCharacterSoul(characterId, { force_regenerate: false }),
  });
  const regenerate = useMutation({
    mutationFn: () => client.reflectCharacterSoul(characterId, { force_regenerate: true }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.soul.narrative(characterId) });
    },
  });

  const data = soul.data;

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{t('soul.title')}</CardTitle>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            disabled={regenerate.isPending}
            onClick={() => regenerate.mutate()}
          >
            {regenerate.isPending ? t('soul.regenerating') : t('soul.regenerate')}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {soul.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('soul.error')}
          </p>
        ) : soul.isLoading ? (
          <LoadingState label={t('soul.loading')} />
        ) : data ? (
          <>
            <p className="text-copy-13 text-gray-600">
              {t('soul.state', { state: data.state })} ·{' '}
              {t('soul.stats', {
                count: data.current_fragment_count,
                keywords: data.current_distinct_keyword_count,
              })}
            </p>
            {data.narrative ? (
              <p className="text-copy-14">{data.narrative}</p>
            ) : data.state === 'insufficient_data' ? (
              <p className="text-copy-14">{t('soul.insufficient')}</p>
            ) : (
              <p className="text-copy-14">{t('soul.ungenerated')}</p>
            )}
          </>
        ) : null}
        {regenerate.isError && (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('soul.regenerateError')}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * ToM beliefs. The list call is only issued once a binding is selected — the
 * query is `enabled`-gated on the selection and the wire requires both
 * `binding_id` and `world_id`, which come from the selected binding row, so
 * there is no code path that lists beliefs without one.
 */
function TomSection({ character }: { character: CharacterDetail['character'] }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();
  const characterId = character.character_id;

  // Same query key the BindingsPanel uses: one cache entry per character.
  const bindings = useQuery({
    queryKey: queryKeys.characters.bindings.list(characterId),
    queryFn: () => client.listCharacterBindings(characterId),
  });
  const [bindingId, setBindingId] = useState('');
  const binding = (bindings.data?.items ?? []).find((b) => b.binding_id === bindingId);
  const scope = binding ? { binding_id: binding.binding_id, world_id: binding.world_id } : null;

  const tom = useQuery({
    queryKey: queryKeys.characters.tom.list(characterId, scope ?? {}),
    queryFn: () => client.listCharacterTom(characterId, scope!),
    // No binding selected → no list call (wire requires binding_id + world_id).
    enabled: scope !== null,
  });
  const record = useMutation({
    mutationFn: (request: {
      carrier_entry_id: string;
      expected_revision: number;
      holder: string;
      proposition: string;
      order: number;
    }) => client.recordCharacterTom(characterId, { ...scope!, ...request }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.tom.lists() });
    },
  });

  const [carrierId, setCarrierId] = useState('');
  const [revision, setRevision] = useState('');
  const [holder, setHolder] = useState('');
  const [proposition, setProposition] = useState('');
  const [order, setOrder] = useState('');

  const submitRecord = (event: FormEvent) => {
    event.preventDefault();
    if (!scope) return;
    const carrier_entry_id = carrierId.trim();
    const holderValue = holder.trim();
    const propositionValue = proposition.trim();
    const expected_revision = Number.parseInt(revision, 10);
    const orderValue = Number.parseInt(order, 10);
    if (!carrier_entry_id || !holderValue || !propositionValue || Number.isNaN(expected_revision) || Number.isNaN(orderValue))
      return;
    record.mutate(
      {
        carrier_entry_id,
        expected_revision,
        holder: holderValue,
        proposition: propositionValue,
        order: orderValue,
      },
      {
        onSuccess: () => {
          setCarrierId('');
          setRevision('');
          setHolder('');
          setProposition('');
          setOrder('');
        },
      },
    );
  };

  const items = tom.data?.items ?? [];
  const bindingItems = bindings.data?.items ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{t('tom.title')}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {bindings.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('tom.bindingsError')}
          </p>
        ) : bindings.isLoading ? (
          <LoadingState label={t('tom.bindingsLoading')} />
        ) : bindingItems.length === 0 ? (
          <EmptyState title={t('tom.noBindings')} description="" />
        ) : (
          <div className="flex flex-col gap-1">
            <Label htmlFor="tom-binding">{t('tom.bindingLabel')}</Label>
            <Select
              id="tom-binding"
              value={bindingId}
              onChange={(e) => setBindingId(e.target.value)}
            >
              <option value="">{t('tom.selectBinding')}</option>
              {bindingItems.map((b) => (
                <option key={b.binding_id} value={b.binding_id}>
                  {b.world_id} ({shortId(b.binding_id)})
                </option>
              ))}
            </Select>
          </div>
        )}

        {scope &&
          (tom.isError ? (
            <p role="alert" className="text-copy-13 text-red-700">
              {t('tom.error')}
            </p>
          ) : tom.isLoading ? (
            <LoadingState label={t('tom.loading')} />
          ) : items.length === 0 ? (
            <EmptyState title={t('tom.empty')} description="" />
          ) : (
            <ul className="flex flex-col divide-y rounded-card border">
              {items.map((row) => (
                <li
                  key={`${row.carrier_entry_id}:${row.row_ordinal}`}
                  className="flex items-center justify-between gap-2 px-4 py-3"
                >
                  <div>
                    <p className="text-copy-14">{row.proposition}</p>
                    <p className="text-copy-13 text-gray-600">
                      L{row.order} · {shortId(row.holder)}
                      {row.truth ? ` · ${row.truth}` : ''}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          ))}

        {scope && (
          <form onSubmit={submitRecord} className="flex flex-col gap-3" aria-label={t('tom.recordTitle')}>
            <p className="text-copy-14 font-medium">{t('tom.recordTitle')}</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="flex flex-col gap-1">
                <Label htmlFor="tom-carrier">{t('tom.carrierLabel')}</Label>
                <Input
                  id="tom-carrier"
                  value={carrierId}
                  onChange={(e) => setCarrierId(e.target.value)}
                  placeholder={t('tom.carrierPlaceholder')}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="tom-revision">{t('tom.revisionLabel')}</Label>
                <Input
                  id="tom-revision"
                  inputMode="numeric"
                  value={revision}
                  onChange={(e) => setRevision(e.target.value)}
                  placeholder="0"
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="tom-holder">{t('tom.holderLabel')}</Label>
                <Input
                  id="tom-holder"
                  value={holder}
                  onChange={(e) => setHolder(e.target.value)}
                  placeholder={characterId}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="tom-order">{t('tom.orderLabel')}</Label>
                <Input
                  id="tom-order"
                  inputMode="numeric"
                  value={order}
                  onChange={(e) => setOrder(e.target.value)}
                  placeholder="1"
                />
              </div>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="tom-proposition">{t('tom.propositionLabel')}</Label>
              <Textarea
                id="tom-proposition"
                value={proposition}
                onChange={(e) => setProposition(e.target.value)}
                placeholder={t('tom.propositionPlaceholder')}
              />
            </div>
            <div className="flex items-center gap-3">
              <Button type="submit" size="small" disabled={record.isPending}>
                {record.isPending ? t('tom.submitting') : t('tom.submit')}
              </Button>
              {record.isError && (
                <p role="alert" className="text-copy-13 text-red-700">
                  {t('tom.recordError')}
                </p>
              )}
            </div>
          </form>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * Knowledge entries (plan v240 T8, I-000128). The address split is the wire
 * contract's, not a UI choice: list/get/patch/delete are character-scoped,
 * while creation exists only on the admitted-principal route and carries
 * `owner_kind` + `character_id` in the body. Every `expected_revision` comes
 * from the revision the list returned for that row.
 */
function KnowledgeSection({ character }: { character: CharacterDetail['character'] }) {
  const { t } = useTranslation('characters');
  const client = useNexusClient();
  const qc = useQueryClient();
  const characterId = character.character_id;

  const knowledge = useQuery({
    queryKey: queryKeys.characters.knowledge.list(characterId),
    queryFn: () => client.listCharacterKnowledge(characterId),
  });

  const [editingId, setEditingId] = useState<string | null>(null);
  const detail = useQuery({
    queryKey: queryKeys.characters.knowledge.detail(characterId, editingId ?? ''),
    queryFn: () => client.getCharacterKnowledge(characterId, editingId!),
    // No row selected → no read (same gating pattern as the ToM list).
    enabled: editingId !== null,
  });

  const patch = useMutation({
    mutationFn: (request: UpdateKnowledgeEntryRequest) =>
      client.patchCharacterKnowledge(characterId, editingId!, request),
    onSuccess: () => {
      // Drop the cached detail (stale text) alongside the list, so a reopen
      // refetches instead of replaying pre-edit values.
      void qc.invalidateQueries({
        queryKey: queryKeys.characters.knowledge.detail(characterId, editingId ?? ''),
      });
      setEditingId(null);
      void qc.invalidateQueries({ queryKey: queryKeys.characters.knowledge.lists() });
    },
  });
  const remove = useMutation({
    mutationFn: (entry: KnowledgeViewItem) =>
      client.deleteCharacterKnowledge(characterId, entry.entry_id, {
        expected_revision: entry.revision,
      }),
    onSuccess: (_data, entry) => {
      setEditingId((current) => (current === entry.entry_id ? null : current));
      void qc.invalidateQueries({
        queryKey: queryKeys.characters.knowledge.detail(characterId, entry.entry_id),
      });
      void qc.invalidateQueries({ queryKey: queryKeys.characters.knowledge.lists() });
    },
  });
  const create = useMutation({
    mutationFn: (request: AddKnowledgeEntryRequest) => client.createActorKnowledgeEntry(request),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.characters.knowledge.lists() });
    },
  });

  const [createName, setCreateName] = useState('');
  const [createSummary, setCreateSummary] = useState('');
  const [createAudience, setCreateAudience] = useState('shared');

  const audienceFor = (value: string) =>
    value === 'author-only'
      ? ({ kind: 'author-only' } as const)
      : value === 'character-private'
        ? ({ kind: 'character-private', character_id: characterId } as const)
        : ({ kind: 'shared' } as const);

  const submitCreate = (event: FormEvent) => {
    event.preventDefault();
    const canonical_name = createName.trim();
    if (!canonical_name) return;
    const summary = createSummary.trim();
    create.mutate(
      {
        owner_kind: 'character',
        character_id: characterId,
        // This surface authors character blocks; other block types are
        // created from their own surfaces.
        block_type: 'character',
        canonical_name,
        ...(summary ? { summary } : {}),
        audience: audienceFor(createAudience),
      },
      {
        onSuccess: () => {
          setCreateName('');
          setCreateSummary('');
          setCreateAudience('shared');
        },
      },
    );
  };

  const submitEdit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = detail.data;
    if (!data) return;
    const fields = new FormData(event.currentTarget);
    const canonical_name = String(fields.get('canonical_name') ?? '').trim();
    if (!canonical_name) return;
    const summary = String(fields.get('summary') ?? '').trim();
    const audienceValue = String(fields.get('audience') ?? '');
    patch.mutate({
      // CAS against the revision the list returned for this row.
      expected_revision: data.item.revision,
      canonical_name,
      summary: summary === '' ? null : summary,
      // An untouched select omits audience: the wire preserves the stored
      // governance pair, and only an explicit "shared" clears it.
      ...(audienceValue ? { audience: audienceFor(audienceValue) } : {}),
    });
  };

  const items = knowledge.data?.items ?? [];

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{t('knowledge.title', { name: character.display_name })}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {knowledge.isError ? (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('knowledge.error')}
          </p>
        ) : knowledge.isLoading ? (
          <LoadingState label={t('knowledge.loading')} />
        ) : items.length === 0 ? (
          <EmptyState title={t('knowledge.empty')} description="" />
        ) : (
          <ul className="flex flex-col divide-y rounded-card border">
            {items.map((entry) => (
              <li
                key={entry.entry_id}
                className="flex items-center justify-between gap-2 px-4 py-3"
              >
                <div>
                  <p className="text-copy-14">{entry.canonical_name}</p>
                  <p className="text-copy-13 text-gray-600">
                    {entry.block_type} · rev {entry.revision}
                    {entry.disclosure ? ` · ${entry.disclosure}` : ''}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="tertiary"
                    size="small"
                    onClick={() => setEditingId(editingId === entry.entry_id ? null : entry.entry_id)}
                  >
                    {t('knowledge.edit')}
                  </Button>
                  <Button
                    type="button"
                    variant="tertiary"
                    size="small"
                    disabled={remove.isPending}
                    onClick={() => remove.mutate(entry)}
                  >
                    {t('knowledge.delete')}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
        {remove.isError && (
          <p role="alert" className="text-copy-13 text-red-700">
            {t('knowledge.deleteError')}
          </p>
        )}

        {editingId &&
          (detail.isError ? (
            <p role="alert" className="text-copy-13 text-red-700">
              {t('knowledge.error')}
            </p>
          ) : !detail.data ? (
            <LoadingState label={t('knowledge.editLoading')} />
          ) : (
            // Uncontrolled inputs keep DOM state; a revision bump after a
            // successful patch must remount the form to drop stale text.
            <form
              key={`${detail.data.item.entry_id}:${detail.data.item.revision}`}
              onSubmit={submitEdit}
              className="flex flex-col gap-3"
              aria-label={t('knowledge.editTitle')}
            >
              <p className="text-copy-14 font-medium">{t('knowledge.editTitle')}</p>
              <div className="flex flex-col gap-1">
                <Label htmlFor="knowledge-edit-name">{t('knowledge.nameLabel')}</Label>
                <Input
                  id="knowledge-edit-name"
                  name="canonical_name"
                  defaultValue={detail.data.item.canonical_name}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="knowledge-edit-summary">{t('knowledge.summaryLabel')}</Label>
                <Textarea
                  id="knowledge-edit-summary"
                  name="summary"
                  defaultValue={detail.data.summary ?? ''}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="knowledge-edit-audience">{t('knowledge.audienceLabel')}</Label>
                <Select id="knowledge-edit-audience" name="audience" defaultValue="">
                  <option value="">{t('knowledge.audienceKeep')}</option>
                  <option value="shared">{t('knowledge.audienceShared')}</option>
                  <option value="author-only">{t('knowledge.audienceAuthorOnly')}</option>
                  <option value="character-private">
                    {t('knowledge.audienceCharacterPrivate')}
                  </option>
                </Select>
              </div>
              <div className="flex items-center gap-3">
                <Button type="submit" size="small" disabled={patch.isPending}>
                  {patch.isPending ? t('knowledge.saving') : t('knowledge.save')}
                </Button>
                <Button type="button" variant="tertiary" size="small" onClick={() => setEditingId(null)}>
                  {t('knowledge.cancel')}
                </Button>
                {patch.isError && (
                  <p role="alert" className="text-copy-13 text-red-700">
                    {t('knowledge.editError')}
                  </p>
                )}
              </div>
            </form>
          ))}

        <form
          onSubmit={submitCreate}
          className="flex flex-col gap-3"
          aria-label={t('knowledge.createTitle')}
        >
          <p className="text-copy-14 font-medium">{t('knowledge.createTitle')}</p>
          <div className="flex flex-col gap-1">
            <Label htmlFor="knowledge-create-name">{t('knowledge.nameLabel')}</Label>
            <Input
              id="knowledge-create-name"
              value={createName}
              onChange={(e) => setCreateName(e.target.value)}
              placeholder={t('knowledge.namePlaceholder')}
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="knowledge-create-summary">{t('knowledge.summaryLabel')}</Label>
            <Textarea
              id="knowledge-create-summary"
              value={createSummary}
              onChange={(e) => setCreateSummary(e.target.value)}
              placeholder={t('knowledge.summaryPlaceholder')}
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="knowledge-create-audience">{t('knowledge.audienceLabel')}</Label>
            <Select
              id="knowledge-create-audience"
              value={createAudience}
              onChange={(e) => setCreateAudience(e.target.value)}
            >
              <option value="shared">{t('knowledge.audienceShared')}</option>
              <option value="author-only">{t('knowledge.audienceAuthorOnly')}</option>
              <option value="character-private">
                {t('knowledge.audienceCharacterPrivate')}
              </option>
            </Select>
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" size="small" disabled={create.isPending}>
              {create.isPending ? t('knowledge.creating') : t('knowledge.createSubmit')}
            </Button>
            {create.isError && (
              <p role="alert" className="text-copy-13 text-red-700">
                {t('knowledge.createError')}
              </p>
            )}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
