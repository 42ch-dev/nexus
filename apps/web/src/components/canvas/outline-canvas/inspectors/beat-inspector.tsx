/**
 * Outline canvas — Beat inspector (V1.109 C2 T3; FB-C2-002).
 *
 * Read-only Beat detail panel. The outline wire model carries no beat data
 * today (`wire_contracts_changed: false`), so there is no write wire — the
 * panel renders title + status + parent-scene helper behind a locked
 * read-only banner. Voice & Content locks the heading (**Beat**), the Status
 * field label (**Status**), the parent helper (*Part of {scene_title}.*), and
 * the banner (*Beat details are view-only for now.*).
 *
 * Selection wiring: the orchestrator resolves the selected Beat from
 * `useOutlineCanvasGraph().selectedBeatId` + the fixture payload, then passes
 * the beat data + parent scene title here.
 */
import { useTranslation } from 'react-i18next';

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

import { SCENE_STATUS_LABEL_KEYS, type OutlineSceneStatus } from '../graph-projection';

/** Resolved Beat data passed in by the orchestrator. */
export interface BeatInspectorBeat {
  beatId: string;
  title: string | null;
  status: OutlineSceneStatus | null;
}

export interface BeatInspectorProps {
  /** Selected Beat data, or `null` when no Beat is selected. */
  beat: BeatInspectorBeat | null;
  /** Display title of the parent scene (for the *Part of* helper). */
  parentSceneTitle: string | null;
}

export function BeatInspector({ beat, parentSceneTitle }: BeatInspectorProps) {
  const { t } = useTranslation('canvas');
  if (!beat) {
    return (
      <Card>
        <CardContent className="py-12 text-center text-copy-14 text-gray-700">
          {t('beatInspector.empty')}
        </CardContent>
      </Card>
    );
  }

  const title = beat.title?.trim() ? beat.title : t('outlineAltView.untitledBeat');

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('beatInspector.title')}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3 text-copy-13 text-gray-700">
          {t('beatInspector.readOnly')}
        </div>

        <div className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">{t('beatInspector.field.title')}</span>
          <span className="text-gray-1000">{title}</span>
        </div>

        <div className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">{t('beatInspector.field.status')}</span>
          {beat.status ? (
            <span className="text-gray-1000">{t(SCENE_STATUS_LABEL_KEYS[beat.status])}</span>
          ) : (
            <span className="text-gray-700">{t('beatInspector.statusNotSet')}</span>
          )}
        </div>

        {parentSceneTitle ? (
          <p className="text-copy-13 text-gray-700">{t('beatInspector.partOf', { title: parentSceneTitle })}</p>
        ) : null}
      </CardContent>
    </Card>
  );
}
