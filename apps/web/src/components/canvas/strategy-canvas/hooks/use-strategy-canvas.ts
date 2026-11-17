/**
 * Orchestrator-level state hook for the Strategy canvas.
 *
 * The Canvas renders a workflow IR document (the applied one, or the candidate
 * being composed) — never YAML. The hook owns: the shared edit form, revision
 * tracking, and conflict/reapply coordination. The editing surface still
 * speaks the flat state shape, so `parsedPresetFromIr` presents the document's
 * nodes to it without any YAML parse.
 */
import { useEffect, useMemo, useState } from 'react';

import type { WorkflowDocument } from '@42ch/nexus-contracts';

import { parsedPresetFromIr } from '@/lib/canvas/ir-preset';
import {
  useActiveSession,
  useDerivedCreatorId,
  usePresetSchedules,
} from '@/lib/canvas/use-strategy-data';

import type { EditForm, Section } from '../state-machine';

/**
 * Conflict state shared across the canvas write boundary.
 *
 * `retry` is present when the conflict carries the command that must be
 * replayed instead of the section save trigger. In that case
 * {@link useStrategyCanvas.handleReapply} replays the original command, which
 * would otherwise wrongly replay a state-edit save (QC1 W-001).
 */
export interface ConflictInfo {
  currentRevision: number;
  section: Section;
  retry?: () => void;
}

/** Where the rendered document comes from, and how to re-read it. */
export interface StrategyDocumentSource {
  /** The document the canvas is rendering; null when there is nothing to show. */
  document: WorkflowDocument | null;
  /** Re-reads that document's source (the applied-status read). */
  refetch: () => Promise<unknown>;
}

export function useStrategyCanvas(presetId: string, source: StrategyDocumentSource) {
  const { document, refetch } = source;
  // The editing surface's flat state view of the rendered document. No YAML
  // is parsed on this path.
  const parsed = useMemo(() => (document ? parsedPresetFromIr(document) : undefined), [document]);
  const revision = document?.revision ?? 0;
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const [form, setForm] = useState<EditForm>({ label: '', description: '', nextTarget: '', promptBody: '' });
  const [activeSection, setActiveSection] = useState<Section>('state');
  const [conflict, setConflict] = useState<ConflictInfo | null>(null);
  const [saveTriggers, setSaveTriggers] = useState<Record<Section, number>>({
    state: 0,
    transition: 0,
    prompt: 0,
  });

  useEffect(() => {
    if (conflict && revision !== conflict.currentRevision) {
      setConflict(null);
    }
  }, [revision, conflict]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        setSaveTriggers((prev) => ({ ...prev, [activeSection]: prev[activeSection] + 1 }));
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activeSection]);

  const activeScheduleId = useMemo(() => {
    if (!activeSession) return undefined;
    const list = schedules.data ?? [];
    return [...list].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]?.schedule_id;
  }, [activeSession, schedules.data]);

  function handleConflict(currentRevision: number, section: Section, retry?: () => void) {
    setConflict({ currentRevision, section, retry });
    void refetch();
  }

  function handleReapply() {
    if (!conflict) return;
    const { section, retry } = conflict;
    setConflict(null);
    void refetch().then(() => {
      // Transition create/reconnect conflicts replay the original transition
      // command; state-edit conflicts replay the section save trigger
      // (QC1 W-001).
      if (retry) {
        retry();
      } else {
        setSaveTriggers((prev) => ({ ...prev, [section]: prev[section] + 1 }));
      }
    });
  }

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
    conflict,
    setConflict,
    saveTriggers,
    setSaveTriggers,
    handleConflict,
    handleReapply,
  };
}
