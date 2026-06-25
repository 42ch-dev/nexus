import { useState } from 'react';
import { Plus, RefreshCw, ShieldCheck, Sparkles } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { usePresets, useReloadPreset } from '@/api/queries';
import type { PresetSummary } from '@42ch/nexus-contracts';

import { ScaffoldPresetDialog } from './dialogs/scaffold-preset-dialog';
import { ValidatePresetDialog } from './dialogs/validate-preset-dialog';

/**
 * Preset management (Setup — CRUD) — web-ui.md §6.2 #7.
 *
 * Lists presets grouped by source (embedded / system / user) and offers the
 * actions the Local API exposes today: scaffold (create), validate (dry-run,
 * product-priority #1), and reload. Get/update/delete are not yet exposed by
 * the daemon (no routes/contracts); that gap is tracked as a residual.
 */
export function PresetsPage() {
  const presets = usePresets();
  const reload = useReloadPreset();
  const [scaffoldOpen, setScaffoldOpen] = useState(false);
  const [validateOpen, setValidateOpen] = useState(false);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-copy-14 text-gray-900">
          Validate a preset before running it — the dry-run is the safest way to confirm it is ready.
        </p>
        <div className="flex items-center gap-2">
          <Button type="button" variant="secondary" size="small" onClick={() => setValidateOpen(true)}>
            <ShieldCheck className="h-4 w-4" aria-hidden />
            Validate Preset
          </Button>
          <Button type="button" variant="primary" size="small" onClick={() => setScaffoldOpen(true)}>
            <Plus className="h-4 w-4" aria-hidden />
            Scaffold Preset
          </Button>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => presets.refetch()}
            disabled={presets.isFetching}
            aria-label="Refresh presets"
          >
            <RefreshCw className={`h-4 w-4 ${presets.isFetching ? 'animate-spin' : ''}`} aria-hidden />
          </Button>
        </div>
      </div>

      {presets.isError ? (
        <ErrorState description="Could not load presets." onRetry={() => presets.refetch()} />
      ) : presets.isLoading ? (
        <Card className="shadow-card">
          <CardContent>
            <LoadingState label="Loading presets…" />
          </CardContent>
        </Card>
      ) : !presets.data ? null : (
        <div className="flex flex-col gap-4">
          <PresetGroup
            title="User presets"
            description="Presets you scaffolded and edit."
            presets={presets.data.user}
            onReload={(id) => reload.mutate(id)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty="No user presets yet. Scaffold one to start."
          />
          <PresetGroup
            title="System presets"
            description="Discovered from the local home layout."
            presets={presets.data.system}
            onReload={(id) => reload.mutate(id)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty="No system presets discovered."
          />
          <PresetGroup
            title="Embedded presets"
            description="Bundled with the runtime."
            presets={presets.data.embedded}
            onReload={(id) => reload.mutate(id)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty="No embedded presets."
          />
        </div>
      )}

      <ScaffoldPresetDialog open={scaffoldOpen} onOpenChange={setScaffoldOpen} />
      <ValidatePresetDialog open={validateOpen} onOpenChange={setValidateOpen} />
    </div>
  );
}

function PresetGroup({
  title,
  description,
  presets,
  onReload,
  reloadingId,
  empty,
}: {
  title: string;
  description: string;
  presets: PresetSummary[];
  onReload: (id: string) => void;
  reloadingId: string | undefined;
  empty: string;
}) {
  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>
        {presets.length === 0 ? (
          <EmptyState title={empty} />
        ) : (
          <ul className="flex flex-col gap-2">
            {presets.map((p) => (
              <li
                key={p.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-gray-alpha-400 p-3"
              >
                <div className="flex items-center gap-2">
                  <Sparkles className="h-4 w-4 text-purple-700" aria-hidden />
                  <span className="text-copy-13-mono text-gray-1000">{p.id}</span>
                  {p.run_intents && p.run_intents.length > 0 && (
                    <div className="flex flex-wrap gap-1">
                      {p.run_intents.map((intent) => (
                        <Badge key={intent} variant="preset">
                          {intent}
                        </Badge>
                      ))}
                    </div>
                  )}
                </div>
                <Button
                  type="button"
                  variant="tertiary"
                  size="small"
                  onClick={() => onReload(p.id)}
                  disabled={reloadingId === p.id}
                >
                  {reloadingId === p.id ? 'Reloading…' : 'Reload'}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
