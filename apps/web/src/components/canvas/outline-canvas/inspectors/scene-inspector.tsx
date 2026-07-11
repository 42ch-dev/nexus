/**
 * Outline canvas — Scene inspector (V1.109 C2 T3; FB-C2-002).
 *
 * Read-only Scene detail panel. The outline wire model carries no scene data
 * today (`wire_contracts_changed: false`), so there is no write wire — the
 * panel renders title + status + parent-chapter helper behind a locked
 * read-only banner. Voice & Content locks the heading (**Scene**), the Status
 * field label (**Status**), the parent helper (*Part of {chapter_title}.*),
 * and the banner (*Scene details are view-only for now.*).
 *
 * Selection wiring: the orchestrator resolves the selected Scene from
 * `useOutlineCanvasGraph().selectedSceneId` + the fixture payload, then passes
 * the scene data + parent chapter title here.
 */
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

import type { OutlineSceneStatus } from '../rf-projection';

/** Status value → human label (matches the Scene node chip labels). */
const SCENE_STATUS_LABEL: Record<OutlineSceneStatus, string> = {
  drafted: 'Drafted',
  completed: 'Completed',
};

/** Resolved Scene data passed in by the orchestrator. */
export interface SceneInspectorScene {
  sceneId: string;
  title: string | null;
  status: OutlineSceneStatus | null;
}

export interface SceneInspectorProps {
  /** Selected Scene data, or `null` when no Scene is selected. */
  scene: SceneInspectorScene | null;
  /** Display title of the parent chapter (for the *Part of* helper). */
  parentChapterTitle: string | null;
}

export function SceneInspector({ scene, parentChapterTitle }: SceneInspectorProps) {
  if (!scene) {
    return (
      <Card>
        <CardContent className="py-12 text-center text-copy-14 text-gray-700">
          Select a scene on the graph to inspect it.
        </CardContent>
      </Card>
    );
  }

  const title = scene.title || 'Untitled Scene';

  return (
    <Card>
      <CardHeader>
        <CardTitle>Scene</CardTitle>
        <CardDescription>Structural metadata for this scene.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="rounded-card border border-gray-alpha-300 bg-background-100 p-3 text-copy-13 text-gray-700">
          Scene details are view-only for now.
        </div>

        <div className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">Title</span>
          <span className="text-gray-1000">{title}</span>
        </div>

        <div className="flex flex-col gap-1 text-copy-13">
          <span className="text-gray-700">Status</span>
          {scene.status ? (
            <span className="text-gray-1000">{SCENE_STATUS_LABEL[scene.status]}</span>
          ) : (
            <span className="text-gray-700">Not set</span>
          )}
        </div>

        {parentChapterTitle ? (
          <p className="text-copy-13 text-gray-700">Part of {parentChapterTitle}.</p>
        ) : null}
      </CardContent>
    </Card>
  );
}
