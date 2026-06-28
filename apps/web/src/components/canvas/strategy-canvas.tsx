/**
 * Strategy canvas — public orchestrator facade.
 *
 * B1: per-inspector saves (R-V171P0-QC1-004).
 * B2: split into focused sibling modules ≤200 lines (R-V171P0-QC1-006).
 */
import { useEffect, useState } from 'react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { StrategyAltView } from '@/components/canvas/strategy-alt-view';
import { strategyNodeTypes } from '@/components/canvas/strategy-nodes';
import { ErrorState, LoadingState } from '@/components/ui/states';

import { StateInspector } from './strategy-canvas/inspectors/state-inspector';
import { EdgeInspector } from './strategy-canvas/inspectors/edge-inspector';
import { PromptInspector } from './strategy-canvas/inspectors/prompt-inspector';
import { InspectorPanel, StrategyConflictModal } from './strategy-canvas/inspector-panel';
import { useStrategyCanvas } from './strategy-canvas/hooks/use-strategy-canvas';
import { CanvasFooter, CanvasHeader } from './strategy-canvas/canvas-layout';
import { ValidationPanel, originalFormOf, type SaveStatus, type Section } from './strategy-canvas/state-machine';
import type { IdeaArtifact } from '@/components/canvas/idea-input';

export interface StrategyCanvasProps {
  presetId: string;
}

export function StrategyCanvas({ presetId }: StrategyCanvasProps) {
  const {
    graphQuery,
    activeSession,
    creatorId,
    nodes,
    edges,
    onNodesChange,
    onEdgesChange,
    selected,
    selectedState,
    baseRevision,
    promptTemplateRef,
    revisionStatus,
    summaryText,
    activeScheduleId,
    form,
    setForm,
    saveStatuses,
    setSaveStatuses,
    setActiveSection,
    conflict,
    setConflict,
    saveTriggers,
    workingRevisionRef,
    handleConflict,
    handleReapply,
  } = useStrategyCanvas(presetId);

  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [showAlt, setShowAlt] = useState(false);
  const [isEditing, setIsEditing] = useState(false);

  useEffect(() => {
    if (!isEditing || !selectedState) {
      setForm({ label: '', description: '', nextTarget: '', promptBody: '' });
      setSaveStatuses({});
      return;
    }
    setForm(originalFormOf(selectedState));
    setSaveStatuses({});
  }, [isEditing, selectedState?.id, setForm, setSaveStatuses]);

  function updateField<K extends keyof typeof form>(field: K, value: (typeof form)[K]) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  function onSaveStatus(section: Section, status: SaveStatus | undefined) {
    setSaveStatuses((prev) => ({ ...prev, [section]: status }));
  }

  if (graphQuery.isLoading) return <LoadingState label="Loading Strategy…" />;
  if (graphQuery.isError)
    return <ErrorState description="Could not load the Strategy preset." onRetry={() => graphQuery.refetch()} />;

  const parsed = graphQuery.data?.parsed;
  const problems = parsed?.problems ?? [];
  const dangling = graphQuery.data?.graph.danglingTargets ?? [];

  return (
    <div className="flex flex-col gap-4">
      <CanvasHeader
        revision={baseRevision}
        status={revisionStatus}
        activeSession={activeSession}
        showAlt={showAlt}
        setShowAlt={setShowAlt}
      />

      {showAlt && parsed ? (
        <StrategyAltView parsed={parsed} statusByState={{}} />
      ) : (
        <CanvasShell
          nodes={nodes}
          edges={edges}
          nodeTypes={strategyNodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          summaryText={summaryText}
          ariaLabel="Strategy state-machine graph"
        >
          <InspectorPanel
            selected={selected}
            selectedState={selectedState}
            isEditing={isEditing}
            setIsEditing={setIsEditing}
            onFocusSection={setActiveSection}
          >
            {selectedState ? (
              <>
                <StateInspector
                  presetId={presetId}
                  selectedState={selectedState}
                  form={form}
                  onChange={updateField}
                  workingRevisionRef={workingRevisionRef}
                  saveTrigger={saveTriggers.state}
                  saveStatus={saveStatuses.state}
                  onSaveStatus={(s) => onSaveStatus('state', s)}
                  onConflict={handleConflict}
                />
                <div onFocusCapture={() => setActiveSection('transition')}>
                  <EdgeInspector
                    presetId={presetId}
                    selectedState={selectedState}
                    form={form}
                    onChange={updateField}
                    workingRevisionRef={workingRevisionRef}
                    saveTrigger={saveTriggers.transition}
                    saveStatus={saveStatuses.transition}
                    onSaveStatus={(s) => onSaveStatus('transition', s)}
                    onConflict={handleConflict}
                  />
                </div>
                {promptTemplateRef ? (
                  <div onFocusCapture={() => setActiveSection('prompt')}>
                    <PromptInspector
                      presetId={presetId}
                      selectedState={selectedState}
                      form={form}
                      onChange={updateField}
                      workingRevisionRef={workingRevisionRef}
                      promptTemplateRef={promptTemplateRef}
                      saveTrigger={saveTriggers.prompt}
                      saveStatus={saveStatuses.prompt}
                      onSaveStatus={(s) => onSaveStatus('prompt', s)}
                      onConflict={handleConflict}
                    />
                  </div>
                ) : null}
              </>
            ) : null}
          </InspectorPanel>
          <ValidationPanel problems={problems} dangling={dangling} />
        </CanvasShell>
      )}

      <StrategyConflictModal
        conflict={conflict}
        form={form}
        canonicalState={selectedState}
        promptTemplateRef={promptTemplateRef}
        onUseCurrent={() => {
          setConflict(null);
          setIsEditing(false);
          void graphQuery.refetch();
        }}
        onReapply={handleReapply}
        onDismiss={() => setConflict(null)}
      />

      <CanvasFooter
        presetId={presetId}
        creatorId={creatorId}
        scheduleId={activeScheduleId}
        artifacts={artifacts}
        setArtifacts={setArtifacts}
      />
    </div>
  );
}
