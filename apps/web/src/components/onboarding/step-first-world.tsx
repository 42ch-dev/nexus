/**
 * First World step — creator onboarding (plan v1219-onboarding T3; prototype
 * `onboarding-v3.html` S3).
 *
 * Presentation only: the shell owns the finish sequence (creator bootstrap →
 * workspace selection → `createWorld`), so a rejected finish leaves this step
 * mounted with the draft and the product copy. There is no client-side
 * first-run marker — the gate flips from the selected/default workspace state
 * (compass D21/D25).
 *
 * Copy is product-owned and final (plan § Task 3), so it ships verbatim and
 * not through i18n — same convention as the identity and agent steps.
 */
import { useState } from 'react';

import { Button, Input, Label } from '@/components/ui';
import { errorMessage } from '@/lib/error-message';
import { cn } from '@/lib/utils';
import { useActiveWorkspaceQuery } from '@/lib/workspace-context';

const LINK_CLASS =
  'text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:opacity-100';

interface WorldTemplate {
  id: string;
  emoji: string;
  label: string;
  description: string;
  /** Name the template preselects; the field stays editable after picking. */
  name: string;
}

const TEMPLATES: readonly WorldTemplate[] = [
  { id: 'blank', emoji: '📄', label: '空白世界', description: '从零开始', name: '' },
  {
    id: 'fantasy',
    emoji: '🏰',
    label: '奇幻冒险',
    description: '带示例时间线与角色',
    name: '灰烬王座',
  },
  {
    id: 'scifi',
    emoji: '🚀',
    label: '科幻远征',
    description: '带示例时间线与角色',
    name: '远星号',
  },
];

/** Prototype default: the fantasy template, with its name prefilled. */
const DEFAULT_TEMPLATE = TEMPLATES[1]!;

interface StepFirstWorldProps {
  onBack: () => void;
  /** Runs the finish sequence; a rejection keeps this step mounted with the draft. */
  onFinish: (name: string) => Promise<void>;
}

export function StepFirstWorld({ onBack, onFinish }: StepFirstWorldProps) {
  const [templateId, setTemplateId] = useState(DEFAULT_TEMPLATE.id);
  const [name, setName] = useState(DEFAULT_TEMPLATE.name);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [showRoot, setShowRoot] = useState(false);
  const activeWorkspace = useActiveWorkspaceQuery();

  // Never a hardcoded path: only the root the service reports for the selected
  // workspace (D25 — the base note claims no absolute path).
  const workspaceRoot = activeWorkspace.data?.creative_root ?? null;
  const trimmed = name.trim();

  function pickTemplate(template: WorldTemplate) {
    setTemplateId(template.id);
    setName(template.name);
    setError(null);
  }

  async function submit() {
    if (trimmed === '' || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await onFinish(trimmed);
    } catch (failure) {
      setError(`创建世界失败：${errorMessage(failure) || '未知错误'}。请重试或更换名称。`);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">第 3 步 / 共 3 步</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">创建你的第一个世界</h1>
      <p className="mt-3 text-copy-14 text-gray-900">
        世界 = 你的故事宇宙。设定、时间线、角色都住在里面
      </p>

      <div
        role="radiogroup"
        aria-label="世界模板"
        className="mt-10 grid w-[720px] max-w-full grid-cols-1 gap-3 sm:grid-cols-3"
      >
        {TEMPLATES.map((template) => {
          const selected = templateId === template.id;
          return (
            <button
              key={template.id}
              type="button"
              role="radio"
              aria-checked={selected}
              data-testid={`world-template-${template.id}`}
              onClick={() => pickTemplate(template)}
              className={cn(
                'flex flex-col items-center rounded-card border-2 bg-background-100 px-4 py-6 text-center transition-colors duration-state ease-standard motion-reduce:transition-none',
                selected ? 'border-blue-700' : 'border-gray-alpha-400 hover:border-blue-700',
              )}
            >
              <span aria-hidden="true" className="text-2xl">
                {template.emoji}
              </span>
              <span className="mt-3 text-heading-16 font-heading text-gray-1000">
                {template.label}
              </span>
              <span className="mt-1 text-copy-13 text-gray-900">{template.description}</span>
            </button>
          );
        })}
      </div>

      <div className="mt-8 flex w-[480px] flex-col gap-1.5">
        <Label htmlFor="onboarding-world-name" className="text-label-14 font-medium text-gray-1000">
          世界名称
        </Label>
        <Input
          id="onboarding-world-name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="例如：灰烬王座"
        />
      </div>

      <div className="mt-4 flex w-[480px] flex-col gap-2">
        <p className="text-copy-13 text-gray-900">
          文件会保存在默认工作区 ·{' '}
          <button
            type="button"
            aria-expanded={showRoot}
            onClick={() => setShowRoot((open) => !open)}
            className={LINK_CLASS}
          >
            高级：更改位置
          </button>
        </p>
        {showRoot ? (
          <p
            data-testid="first-world-root"
            className="rounded-control border border-gray-alpha-400 bg-background-200 px-3 py-2 text-copy-14 text-gray-900"
          >
            {workspaceRoot ?? '默认工作区'}
          </p>
        ) : null}
      </div>

      {error ? (
        <p role="alert" className="mt-4 w-[480px] text-copy-13 text-red-700">
          {error}
        </p>
      ) : null}

      <div className="mt-12 flex w-[480px] items-center justify-between">
        <Button variant="tertiary" onClick={onBack}>
          返回
        </Button>
        <Button
          variant="primary"
          disabled={trimmed === '' || submitting}
          onClick={() => void submit()}
        >
          创建并进入
        </Button>
      </div>
    </div>
  );
}
