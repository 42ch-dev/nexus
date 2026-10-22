/**
 * Side checklist (plan v1219-world-module T3) — the `接下来可以` progress card
 * riding beside the Brief layer (prototype onboarding-v3 S4). Three
 * product-owned steps; only step ① (`写完 Brief`) is wired this iteration —
 * steps ②③ ship with their module rebuilds and stay pending. The first
 * pending step is highlighted as `next`; done steps are struck through; the
 * footer reads `进度 {n} / 3`.
 */
import { useTranslation } from 'react-i18next';

export interface SideChecklistProps {
  /** Step ① done — the world has a saved Brief (era KnowledgeEntry with summary). */
  briefDone: boolean;
}

export function SideChecklist({ briefDone }: SideChecklistProps) {
  const { t } = useTranslation('worlds');
  const done = [briefDone, false, false];
  const doneCount = done.filter(Boolean).length;

  return (
    <div className="rounded-card border border-gray-500 p-4" data-testid="world-side-checklist">
      <h3 className="text-heading-16 font-heading text-gray-1000">
        {t('worldApp.checklist.heading')}
      </h3>
      <ol className="flex flex-col gap-1">
        {done.map((isDone, i) => (
          <li
            key={i}
            data-testid={`world-checklist-step-${i + 1}`}
            className={
              isDone
                ? 'text-copy-14 text-gray-700 line-through'
                : i === doneCount
                  ? 'text-copy-14 font-medium text-gray-1000'
                  : 'text-copy-14 text-gray-700'
            }
          >
            {t(`worldApp.checklist.step${i + 1}`)}
          </li>
        ))}
      </ol>
      <p className="text-copy-14 text-gray-900" data-testid="world-checklist-progress">
        {t('worldApp.checklist.progress', { n: doneCount })}
      </p>
    </div>
  );
}
