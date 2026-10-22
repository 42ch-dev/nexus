/**
 * Identity step (plan v1219-onboarding T1; prototype `onboarding-v3.html` S1).
 *
 * Two entrance cards — the choice IS the entrance, which selects the rail
 * registry (`appsForEntrance`) used after the wizard finishes. Copy is
 * product-owned and final (plan § Copy), so it ships verbatim, not through
 * i18n.
 */
import type { EntranceId } from '@/components/layout/entrance-registry';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface IdentityOption {
  id: EntranceId;
  emoji: string;
  title: string;
  body: string;
  recommended?: boolean;
}

const OPTIONS: readonly IdentityOption[] = [
  {
    id: 'content-creator',
    emoji: '✍️',
    title: '我来写故事',
    body: '长篇小说 · 跑团战役 · 世界观构建 / AI 记得你的设定，帮你写出初稿',
    recommended: true,
  },
  {
    id: 'developer',
    emoji: '🛠️',
    title: '我来开发工具',
    body: '编排引擎 · 预设 · WASM 模块 / 完整的操作台与策略画布',
  },
];

interface StepIdentityProps {
  value: EntranceId | null;
  onChange: (value: EntranceId) => void;
  onNext: () => void;
}

export function StepIdentity({ value, onChange, onNext }: StepIdentityProps) {
  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">NEXUS · 初次设置</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">你的 AI 创作工作室</h1>
      <p className="mt-3 max-w-[560px] text-center text-copy-14 text-gray-900">
        世界有记忆，故事不自相矛盾。先告诉我——你想用 Nexus 做什么？
      </p>

      <div
        role="radiogroup"
        aria-label="你想用 Nexus 做什么？"
        className="mt-10 flex flex-col gap-6 sm:flex-row sm:gap-8"
      >
        {OPTIONS.map((option) => {
          const selected = value === option.id;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => onChange(option.id)}
              data-testid={`identity-option-${option.id}`}
              className={cn(
                'relative flex w-[320px] flex-col items-center rounded-card border-2 bg-background-100 px-7 pb-8 pt-10 text-center transition-colors duration-state ease-standard motion-reduce:transition-none',
                selected
                  ? 'border-blue-700'
                  : 'border-gray-alpha-400 hover:border-blue-700',
              )}
            >
              {option.recommended && (
                <span className="absolute -top-3 rounded-pill bg-blue-700 px-3.5 py-1 text-label-12 font-semibold text-brand-white dark:text-brand-deep-blue">
                  推荐路径
                </span>
              )}
              <span aria-hidden="true" className="text-4xl">
                {option.emoji}
              </span>
              <span className="mt-5 text-heading-20 font-heading text-gray-1000">{option.title}</span>
              <span className="mt-2.5 text-copy-13 text-gray-900">{option.body}</span>
            </button>
          );
        })}
      </div>

      <div className="mt-12 flex w-[480px] justify-end">
        <Button variant="primary" disabled={value === null} onClick={onNext}>
          下一步
        </Button>
      </div>
    </div>
  );
}
