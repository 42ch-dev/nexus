/**
 * Idea-input affordance — the persistent canvas steering control
 * (canvas-strategy-surface.md Draft §4.1 / §3.7).
 *
 * The author gives direction; Nexus executes and owns prose. Verbs prefer
 * Steer / Run / Resume / Ask Nexus to revise over "Edit body" (Draft §4.1).
 *
 * Modes (A4, reuses existing endpoints — no new steering DTO):
 *   • Run    — create a new schedule with the Idea as seed (addSchedule)
 *   • Steer  — append the Idea to an active schedule's core context, then
 *              signal resume (editCoreContext + signalSchedule)
 *   • Resume — signal an existing schedule to resume (signalSchedule)
 *
 * Submitted Ideas land as visible steering artifacts (a note badge in the
 * artifacts list) so the author can later understand why Nexus did something.
 */
import { useState } from 'react';
import { Loader2, Play, RotateCcw, Send } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useResumeStrategy, useRunStrategy, useSteerStrategy } from '@/lib/canvas/use-strategy-data';

export type IdeaVerb = 'run' | 'steer' | 'resume';

export interface IdeaArtifact {
  id: string;
  verb: IdeaVerb;
  text: string;
  target?: string;
  at: number;
}

export interface IdeaInputProps {
  presetId: string;
  creatorId?: string;
  /** Active schedule id for Steer/Resume (derived from the live overlay). */
  scheduleId?: string;
  /** Called when an Idea is submitted so the canvas can show the artifact. */
  onArtifact: (artifact: IdeaArtifact) => void;
}

const VERB_LABEL: Record<IdeaVerb, string> = {
  run: 'Run',
  steer: 'Steer',
  resume: 'Resume',
};

export function IdeaInput({ presetId, creatorId, scheduleId, onArtifact }: IdeaInputProps) {
  const [text, setText] = useState('');
  const [verb, setVerb] = useState<IdeaVerb>('run');
  const run = useRunStrategy();
  const steer = useSteerStrategy();
  const resume = useResumeStrategy();

  const canRun = verb === 'run' && Boolean(creatorId) && text.trim().length > 0;
  const canSteer = verb === 'steer' && Boolean(scheduleId) && text.trim().length > 0;
  const canResume = verb === 'resume' && Boolean(scheduleId);
  const canSubmit = canRun || canSteer || canResume;
  const pending = run.isPending || steer.isPending || resume.isPending;

  // Default verb follows availability: steer when a schedule is active, else run.
  const effectiveVerb: IdeaVerb = scheduleId && verb === 'run' ? 'steer' : verb;

  const handleSubmit = async () => {
    if (pending || !canSubmit) return;
    const idea = text.trim();
    if (effectiveVerb === 'run' && creatorId) {
      onArtifact({ id: crypto.randomUUID(), verb: 'run', text: idea, at: Date.now() });
      run.mutate({ creatorId, presetId, idea });
    } else if (effectiveVerb === 'steer' && scheduleId) {
      onArtifact({ id: crypto.randomUUID(), verb: 'steer', text: idea, target: scheduleId, at: Date.now() });
      steer.mutate({ scheduleId, idea });
    } else if (effectiveVerb === 'resume' && scheduleId) {
      onArtifact({ id: crypto.randomUUID(), verb: 'resume', text: '(resume)', target: scheduleId, at: Date.now() });
      resume.mutate(scheduleId);
    }
    setText('');
  };

  return (
    <div className="flex flex-col gap-2 rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover">
      <label htmlFor="idea-input" className="text-label-14 text-gray-1000">
        Steer the Strategy
      </label>
      <textarea
        id="idea-input"
        className="min-h-[64px] w-full resize-y rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-14 text-gray-1000 placeholder:text-gray-700 focus:border-blue-700 focus:outline-none"
        placeholder="Describe a direction for Nexus — it will execute and write the prose."
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
            e.preventDefault();
            void handleSubmit();
          }
        }}
        aria-describedby="idea-helper"
      />
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1" role="group" aria-label="Steering verb">
          {(['run', 'steer', 'resume'] as const).map((v) => {
            const disabled =
              (v === 'run' && !creatorId) ||
              ((v === 'steer' || v === 'resume') && !scheduleId);
            return (
              <button
                key={v}
                type="button"
                disabled={disabled}
                aria-pressed={effectiveVerb === v}
                onClick={() => setVerb(v)}
                className={[
                  'rounded-control px-2.5 py-1 text-button-12 transition-colors duration-state ease-standard',
                  effectiveVerb === v
                    ? 'bg-purple-700 text-white'
                    : 'bg-gray-alpha-100 text-gray-900 hover:bg-gray-alpha-200',
                  disabled ? 'cursor-not-allowed opacity-40' : '',
                ].join(' ')}
              >
                {VERB_LABEL[v]}
              </button>
            );
          })}
        </div>
        <Button
          type="button"
          variant="primary"
          size="small"
          onClick={() => void handleSubmit()}
          disabled={!canSubmit || pending}
        >
          {pending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : (
            effectiveVerb === 'resume'
              ? <RotateCcw className="h-4 w-4" aria-hidden />
              : effectiveVerb === 'run'
                ? <Play className="h-4 w-4" aria-hidden />
                : <Send className="h-4 w-4" aria-hidden />
          )}
          {effectiveVerb === 'run' ? 'Run Strategy' : effectiveVerb === 'steer' ? 'Send Idea' : 'Resume'}
        </Button>
      </div>
      <p id="idea-helper" className="text-copy-13 text-gray-700">
        {effectiveVerb === 'run'
          ? !creatorId
            ? 'No active creator found. Start the daemon and create a Work first, or steer an existing run.'
            : 'Starts a new run with this Idea as the seed. Nexus will execute the Strategy.'
          : effectiveVerb === 'steer'
            ? 'Appends this Idea to the active run’s context and resumes execution.'
            : 'Signals the active run to resume from its current state.'}
      </p>
    </div>
  );
}
