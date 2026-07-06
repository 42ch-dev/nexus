import { useEffect, useMemo } from 'react';
import { Check, Loader2, Terminal } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useScanAgents } from '@/api/queries';
import type { AgentScanEntry } from '@42ch/nexus-contracts';
import type { WizardState } from '@/pages/setup-wizard-page';

interface SetupStepAgentProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  onBack: () => void;
}

export function SetupStepAgent({ state, onChange, onNext, onBack }: SetupStepAgentProps) {
  const scan = useScanAgents({ filter: 'all', registry_refresh: true });
  const agents = scan.data?.agents ?? [];
  const recommendedIndex = useMemo(
    () => agents.findIndex((a) => a.installed),
    [agents],
  );

  // Default to the first installed agent.
  useEffect(() => {
    if (state.selectedAgent || state.customLaunchCommand) return;
    if (recommendedIndex >= 0) {
      onChange({ ...state, selectedAgent: agents[recommendedIndex] });
    }
  }, [recommendedIndex, agents, state, onChange]);

  function selectAgent(agent: AgentScanEntry) {
    onChange({ ...state, selectedAgent: agent, customLaunchCommand: '' });
  }

  function useCustom(command: string) {
    onChange({
      ...state,
      selectedAgent: null,
      customLaunchCommand: command,
    });
  }

  const canContinue = Boolean(state.selectedAgent || state.customLaunchCommand.trim());

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Choose an ACP agent</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus uses an ACP-compatible agent to run strategies. Select a discovered agent or provide a custom launch command.
        </p>
      </div>

      <div className="flex min-h-[160px] flex-col gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-4">
        {scan.isLoading ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-2">
            <Loader2 className="h-5 w-5 animate-spin text-blue-700" aria-hidden />
            <span className="text-copy-14 text-gray-900">Scanning for local ACP agents…</span>
          </div>
        ) : agents.length === 0 ? (
          <div className="flex flex-col gap-3">
            <p className="text-copy-14 text-gray-900">No agents found on PATH.</p>
            <CustomLaunchCommand value={state.customLaunchCommand} onChange={useCustom} />
          </div>
        ) : (
          <ul className="flex flex-col gap-2">
            {agents.map((agent, index) => {
              const selected = state.selectedAgent?.name === agent.name;
              const recommended = index === recommendedIndex;
              return (
                <li key={agent.name}>
                  <button
                    type="button"
                    onClick={() => selectAgent(agent)}
                    aria-pressed={selected}
                    className={[
                      'flex w-full items-center justify-between gap-3 rounded-control border p-3 text-left transition-colors',
                      selected
                        ? 'border-blue-700 bg-blue-700/8'
                        : 'border-gray-alpha-400 bg-background-100 hover:bg-gray-alpha-100',
                    ].join(' ')}
                  >
                    <div className="flex flex-col">
                      <span className={['text-copy-14 font-medium', selected ? 'text-gray-1000' : 'text-gray-1000'].join(' ')}>
                        {agent.name}
                      </span>
                      {agent.version && (
                        <span className="text-copy-13 text-gray-700">Version {agent.version}</span>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      {recommended && (
                        <span className="rounded-pill bg-green-700 px-2 py-0.5 text-label-12 text-white">Recommended</span>
                      )}
                      {agent.installed ? (
                        <>
                          <Check className="h-4 w-4 text-green-800" aria-hidden />
                          <span className="text-copy-13 text-gray-700">Installed</span>
                        </>
                      ) : (
                        <span className="text-copy-13 text-gray-700">Not installed</span>
                      )}
                    </div>
                  </button>
                </li>
              );
            })}
            <li className="mt-2 border-t border-gray-alpha-400 pt-3">
              <CustomLaunchCommand value={state.customLaunchCommand} onChange={useCustom} />
            </li>
          </ul>
        )}
      </div>

      <div className="flex justify-between">
        <Button variant="tertiary" onClick={onBack}>Back</Button>
        <Button variant="primary" onClick={onNext} disabled={!canContinue}>
          Continue
        </Button>
      </div>
    </div>
  );
}

function CustomLaunchCommand({ value, onChange }: { value: string; onChange: (command: string) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor="custom-launch-command" className="flex items-center gap-1.5 text-copy-14 text-gray-900">
        <Terminal className="h-4 w-4 text-gray-700" aria-hidden />
        Use custom launch command
      </Label>
      <Input
        id="custom-launch-command"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="e.g. /usr/local/bin/my-agent"
      />
    </div>
  );
}
