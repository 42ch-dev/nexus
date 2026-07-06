/**
 * Strategy detail — route entry for the Canvas Strategy Surface (α).
 *
 * Renders the selected preset as a state-machine graph via {@link StrategyCanvas}.
 * UI label is "Strategy"; persisted identifiers remain "preset" (Draft §4.2).
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk
 * (Draft §3.1 bundle/performance).
 */
import { useParams } from 'react-router-dom';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { EmptyState, LoadingState } from '@/components/ui/states';
import { usePresets } from '@/api/queries';

export function StrategyPage() {
  const { presetId } = useParams<{ presetId: string }>();
  const presets = usePresets();

  if (presets.isLoading) {
    return <LoadingState label="Loading Strategy…" />;
  }

  const all = presets.data
    ? [...presets.data.user, ...presets.data.system, ...presets.data.embedded]
    : [];
  const activePreset = all.find((p) => p.id === presetId);

  if (!activePreset) {
    return <EmptyState title="Strategy not found" description="Choose a Strategy from the Strategies list." />;
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-heading-24 font-heading text-gray-1000">Strategy</h1>
        <p className="text-copy-14 text-gray-900">
          See a preset as a state-machine graph and steer execution with an Idea. Nexus owns the prose.
        </p>
      </div>
      <StrategyCanvas presetId={activePreset.id} />
    </div>
  );
}
