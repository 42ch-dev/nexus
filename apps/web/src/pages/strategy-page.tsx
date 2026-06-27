/**
 * Strategy page — route entry for the Canvas Strategy Surface (α).
 *
 * Lists available presets grouped by source and renders the selected preset as
 * a state-machine graph via {@link StrategyCanvas}. UI label is "Strategy";
 * persisted identifiers remain "preset" (Draft §4.2).
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk
 * (Draft §3.1 bundle/performance).
 */
import { useMemo, useState } from 'react';
import { Sparkles } from 'lucide-react';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { usePresets } from '@/api/queries';

export function StrategyPage() {
  const presets = usePresets();
  const [selectedId, setSelectedId] = useState<string | undefined>(undefined);

  const all = useMemo(() => {
    const p = presets.data;
    if (!p) return [];
    return [...p.user, ...p.system, ...p.embedded];
  }, [presets.data]);

  const activeId = selectedId ?? all[0]?.id;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-heading-24 font-heading text-gray-1000">Strategy</h1>
          <p className="text-copy-14 text-gray-900">
            See a preset as a state-machine graph and steer execution with an Idea. Nexus owns the prose.
          </p>
        </div>
      </div>

      {presets.isError ? (
        <ErrorState description="Could not load presets." onRetry={() => presets.refetch()} />
      ) : presets.isLoading ? (
        <LoadingState label="Loading presets…" />
      ) : all.length === 0 ? (
        <EmptyState title="No presets" description="Presets appear here once discovered by the daemon." />
      ) : (
        <>
          <Card className="shadow-card">
            <CardHeader>
              <CardTitle>Choose a Strategy</CardTitle>
            </CardHeader>
            <CardContent>
              <ul className="flex flex-wrap gap-2">
                {all.map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => setSelectedId(p.id)}
                      aria-pressed={activeId === p.id}
                      className={[
                        'flex items-center gap-1.5 rounded-control border px-3 py-1.5 text-copy-14 transition-colors duration-state ease-standard',
                        activeId === p.id
                          ? 'border-purple-700 bg-[color-mix(in_srgb,var(--color-purple-700)_8%,transparent)] text-gray-1000'
                          : 'border-gray-alpha-400 text-gray-900 hover:bg-gray-alpha-100',
                      ].join(' ')}
                    >
                      <Sparkles className="h-4 w-4 text-purple-700" aria-hidden />
                      <span className="font-mono">{p.id}</span>
                      {p.run_intents?.slice(0, 2).map((intent) => (
                        <Badge key={intent} variant="preset">{intent}</Badge>
                      ))}
                    </button>
                  </li>
                ))}
              </ul>
            </CardContent>
          </Card>

          {activeId ? <StrategyCanvas presetId={activeId} /> : null}
        </>
      )}
    </div>
  );
}
