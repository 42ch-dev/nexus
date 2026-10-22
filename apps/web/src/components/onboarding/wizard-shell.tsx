/**
 * Onboarding wizard shell (plan v1219-onboarding T1–T3) — route owner of
 * `/setup`.
 *
 * Reached when the derived browser first-run gate finds no initialized
 * workspace (compass D21), and always directly reachable as the re-run entry.
 * The identity choice is the entrance: it selects the rail registry
 * (`appsForEntrance`) used after the wizard finishes and decides the step path —
 * creator: AI partner → first World; developer: agent + workspace.
 *
 * Finish path (compass D25 — no workspace create anywhere): creator bootstrap
 * (`createCreator` only when no creator exists; `ensure_profile_home_ssot`
 * pre-creates that creator's `default` workspace) → `POST
 * /v1/daemon/workspace/active` selecting `{creator_id, workspace_slug:
 * 'default'}` → `createWorld`. Selection promotes the shell in-process, so the
 * derived gate re-probes `true` in the same session; there is no client-side
 * marker (D21). The developer path finishes after the workspace selection and
 * lands in the Hub.
 */
import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router';

import { StepAgent } from '@/components/onboarding/step-agent';
import { StepAgentWorkspace } from '@/components/onboarding/step-agent-workspace';
import { StepFirstWorld } from '@/components/onboarding/step-first-world';
import { StepIdentity } from '@/components/onboarding/step-identity';
import type { EntranceId } from '@/components/layout/entrance-registry';
import { useActiveCreatorId, useSetActiveCreatorId } from '@/lib/active-creator-context';
import { useNexusClient } from '@/lib/client-context';
import { useEntrance } from '@/lib/entrance-context';
import { errorMessage } from '@/lib/error-message';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useSetupCompleted } from '@/lib/setup-completed-context';
import { useToast } from '@/lib/use-toast';

type WizardStep = 'identity' | 'agent' | 'agent-workspace' | 'first-world';

/** Display name of the creator the wizard bootstraps (the v1.94 default creator). */
const DEFAULT_CREATOR_NAME = 'Default';

export function WizardShell() {
  const client = useNexusClient();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { toast } = useToast();
  const { setEntrance } = useEntrance();
  const { setCompleted } = useSetupCompleted();
  const activeCreatorId = useActiveCreatorId();
  const setActiveCreatorId = useSetActiveCreatorId();

  const [choice, setChoice] = useState<EntranceId | null>(null);
  const [step, setStep] = useState<WizardStep>('identity');
  // Creator creation is not idempotent: a second click must not make a second
  // creator while the first request is still in flight.
  const creatorRequest = useRef<Promise<string> | null>(null);

  /**
   * Resolve the local creator, creating the default one only when none exists
   * (D25: the creator's `default` workspace directory is created by creator
   * bootstrap, never by a workspace-create call). Runs before the agent step so
   * the choice can be persisted under `nexus-agent-profile:<creator_id>` (D27).
   */
  function ensureCreator(): Promise<string> {
    if (activeCreatorId) return Promise.resolve(activeCreatorId);
    creatorRequest.current ??= (async () => {
      const creator = await client.createCreator({ display_name: DEFAULT_CREATOR_NAME });
      setActiveCreatorId(creator.creator_id);
      void queryClient.invalidateQueries({ queryKey: queryKeys.creators.all });
      return creator.creator_id;
    })().catch((error: unknown) => {
      creatorRequest.current = null;
      throw error;
    });
    return creatorRequest.current;
  }

  async function handleIdentityNext() {
    if (choice === null) return;
    // Persisted here so a finish/landing already sees the chosen rail tree.
    void setEntrance(choice);
    try {
      await ensureCreator();
    } catch (error) {
      toast({
        variant: 'error',
        title: '无法初始化本地创作者',
        description: `${errorMessage(error) || '未知错误'}。请重试。`,
      });
      return;
    }
    setStep(choice === 'developer' ? 'agent-workspace' : 'agent');
  }

  /**
   * Step 1 of the finish path: select the workspace bootstrap pre-created.
   * The derived-gate re-probe is a separate stage (see
   * {@link refreshGateOrReport}) so a re-probe failure is never reported as a
   * workspace-selection failure.
   */
  async function selectDefaultWorkspace(): Promise<void> {
    const creatorId = await ensureCreator();
    await client.setActiveWorkspace({ creator_id: creatorId, workspace_slug: 'default' });
    // The active-workspace read feeds the storage note's actual root.
    void queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.all });
  }

  /**
   * Re-read the derived gate (the service promotes the shell in place). The
   * workspace is already selected server-side when this runs, so a failure
   * here is a status-refresh failure, with its own copy — never the
   * workspace-selection toast. Returns false when the user should stay put.
   */
  async function refreshGateOrReport(): Promise<boolean> {
    try {
      await setCompleted(true);
      return true;
    } catch (error) {
      toast({
        variant: 'error',
        title: '状态刷新失败',
        description: `${errorMessage(error) || '未知错误'}。请重试。`,
      });
      return false;
    }
  }

  /**
   * Selection is the shell's stage, not the step's: a failure there is reported
   * by the shell (the same path both finishes use) so it is never misreported
   * as a World-creation failure. Returns false when the user should stay put.
   */
  async function selectWorkspaceOrReport(): Promise<boolean> {
    try {
      await selectDefaultWorkspace();
    } catch (error) {
      toast({
        variant: 'error',
        title: '无法选择默认工作区',
        description: `${errorMessage(error) || '未知错误'}。请重试。`,
      });
      return false;
    }
    return refreshGateOrReport();
  }

  /** Creator finish: select the pre-created workspace, then create the World. */
  async function finishCreator(name: string) {
    if (!(await selectWorkspaceOrReport())) return;
    // Only a World-creation failure reaches the step's inline copy.
    const world = await client.createWorld({ title: name });
    void queryClient.invalidateQueries({ queryKey: queryKeys.memory.worlds() });
    toast({ variant: 'success', title: `🎉 世界「${name}」已创建` });
    navigate(`/worlds/${encodeURIComponent(world.world_id)}?layer=brief`);
  }

  /** Developer finish: workspace selection only; the agent choice persists in the step. */
  async function finishDeveloper() {
    if (!(await selectWorkspaceOrReport())) return;
    navigate('/developer');
  }

  function backToIdentity() {
    setStep('identity');
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background-200 px-6 py-16">
      {step === 'identity' ? (
        <StepIdentity
          value={choice}
          onChange={setChoice}
          onNext={() => void handleIdentityNext()}
        />
      ) : null}
      {step === 'agent' ? (
        <StepAgent onBack={backToIdentity} onNext={() => setStep('first-world')} />
      ) : null}
      {step === 'first-world' ? (
        <StepFirstWorld onBack={() => setStep('agent')} onFinish={finishCreator} />
      ) : null}
      {step === 'agent-workspace' ? (
        <StepAgentWorkspace onBack={backToIdentity} onFinish={() => void finishDeveloper()} />
      ) : null}
    </div>
  );
}
