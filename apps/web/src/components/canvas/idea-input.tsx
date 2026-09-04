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
import { useTranslation } from 'react-i18next';
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

export function IdeaInput({ presetId, creatorId, scheduleId, onArtifact }: IdeaInputProps) {
  const { t } = useTranslation('canvas');
  const [text, setText] = useState('');
  const [verb, setVerb] = useState<IdeaVerb>('run');
  const run = useRunStrategy();
  const steer = useSteerStrategy();
  const resume = useResumeStrategy();

  const VERB_LABEL: Record<IdeaVerb, string> = {
    run: t('ideaInput.verb.run'),
    steer: t('ideaInput.verb.steer'),
    resume: t('ideaInput.verb.resume'),
  };

  const pending = run.isPending || steer.isPending || resume.isPending;

  // Default verb follows availability: steer when a schedule is active, else run.
  const effectiveVerb: IdeaVerb = scheduleId && verb === 'run' ? 'steer' : verb;

  const canSubmit =
    (effectiveVerb === 'run' && Boolean(creatorId) && text.trim().length > 0) ||
    (effectiveVerb === 'steer' && Boolean(scheduleId) && text.trim().length > 0) ||
    (effectiveVerb === 'resume' && Boolean(scheduleId));

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

  const runHelper = !creatorId
    ? t('ideaInput.helper.run.noCreator')
    : t('ideaInput.helper.run.default');
  const steerHelper = t('ideaInput.helper.steer');
  const resumeHelper = t('ideaInput.helper.resume');

  return (
    <div className="flex flex-col gap-2 rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover">
      <label htmlFor="idea-input" className="text-label-14 text-gray-1000">
        {t('ideaInput.label')}
      </label>
      <textarea
        id="idea-input"
        className="min-h-[64px] w-full resize-y rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-14 text-gray-1000 placeholder:text-gray-700 focus:border-blue-1000 focus:outline-none dark:focus:border-blue-700"
        placeholder={t('ideaInput.placeholder')}
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
        <div className="flex items-center gap-1" role="group" aria-label={t('ideaInput.verbGroupAria')}>
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
                    ? 'bg-canvas-strategy-accent text-white dark:text-brand-deep-blue'
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
          {effectiveVerb === 'run'
            ? t('ideaInput.submit.run')
            : effectiveVerb === 'steer'
              ? t('ideaInput.submit.steer')
              : t('ideaInput.submit.resume')}
        </Button>
      </div>
      <p id="idea-helper" className="text-copy-13 text-gray-700">
        {effectiveVerb === 'run'
          ? runHelper
          : effectiveVerb === 'steer'
            ? steerHelper
            : resumeHelper}
      </p>
    </div>
  );
}
