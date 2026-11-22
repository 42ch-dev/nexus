/**
 * note: Orchestrator-level state hook for the Strategy canvas.
 *
 * The Canvas renders a workflow IR document (the applied one, or the candidate
 * being composed) — never YAML. The hook owns the shared edit form, the active
 * inspector section, and the reads around the open preset. The editing surface
 * still speaks the flat state shape, so `parsedPresetFromIr` presents the
 * document's nodes to it without any YAML parse.
 */
import { useMemo, useState } from 'react';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { parsedPresetFromIr } from '@/lib/canvas/ir-preset';
import {
  useActiveSession,
  useDerivedCreatorId,
  usePresetSchedules,
} from '@/lib/canvas/use-strategy-data';

import type { EditForm, Section } from '../state-machine';

export function useStrategyCanvas(presetId: string, document: WorkflowDocument | null) {
  // note: the editing surface's flat state view of the rendered document. No YAML
  // is parsed on this path.
  const parsed = useMemo(() => (document ? parsedPresetFromIr(document) : undefined), [document]);
  const revision = document?.revision ?? 0;
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const [form, setForm] = useState<EditForm>({ label: '', description: '', nextTarget: '', promptBody: '' });
  const [activeSection, setActiveSection] = useState<Section>('state');

  const activeScheduleId = useMemo(() => {
    if (!activeSession) return undefined;
    const list = schedules.data ?? [];
    return [...list].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]?.schedule_id;
  }, [activeSession, schedules.data]);

  return {
    parsed,
    revision,
    activeSession,
    schedules,
    creatorId,
    activeScheduleId,
    form,
    setForm,
    activeSection,
    setActiveSection,
  };
}
