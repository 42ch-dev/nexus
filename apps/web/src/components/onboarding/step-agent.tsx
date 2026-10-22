/**
 * AI partner step — creator onboarding (plan v1219-onboarding T2; prototype
 * `onboarding-v3.html` S2).
 *
 * Copy is product-owned and final (plan § Copy), so it ships verbatim and not
 * through i18n — same convention as the identity step. Scan states: loading
 * (Next disabled), ready (rows + recommended preselected), empty and error
 * (recoverable through the install guide / manual launch command).
 */
import { useState } from 'react';

import { AgentRow, ManualCommandField, ScanErrorNotice, useAgentSelection } from '@/components/onboarding/agent-selection';
import { Button } from '@/components/ui';
import { resolveInstallUrl } from '@/lib/agent-catalog';
import { useDesktopCapabilities } from '@/lib/client-context';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

interface InstallTarget {
  label: string;
  url: string;
}

const INSTALL_TARGETS: InstallTarget[] = [
  { label: 'Claude Code', url: resolveInstallUrl('claude-native') },
  { label: 'Codex CLI', url: resolveInstallUrl('codex-native') },
].filter((target): target is InstallTarget => target.url !== null);

const LINK_CLASS =
  'self-start text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:opacity-100';

/**
 * Outbound install link. Desktop routes every activation through the
 * system-browser opener so the agent page never loads inside the app webview;
 * the browser build uses a normal new-tab link.
 */
function InstallLink({ target, desktop }: { target: InstallTarget; desktop: DesktopCapabilities | null }) {
  if (!desktop) {
    return (
      <a href={target.url} target="_blank" rel="noopener noreferrer" className={LINK_CLASS}>
        {target.label}
      </a>
    );
  }
  return (
    <a
      href={target.url}
      onClick={(event) => {
        event.preventDefault();
        void desktop.openExternalUrl(target.url).catch(() => {
          console.error('Failed to open external URL:', target.url);
        });
      }}
      className={LINK_CLASS}
    >
      {target.label}
    </a>
  );
}

interface StepAgentProps {
  onBack: () => void;
  onNext: () => void;
}

export function StepAgent({ onBack, onNext }: StepAgentProps) {
  const selection = useAgentSelection();
  const desktop = useDesktopCapabilities();
  const [showInstallGuide, setShowInstallGuide] = useState(false);
  const [showManual, setShowManual] = useState(false);

  const scanning = selection.status === 'loading';
  const canContinue = !scanning && selection.canContinue;

  function advance() {
    selection.commit();
    onNext();
  }

  return (
    <div className="flex flex-col items-center">
      <div className="text-label-12 text-gray-700">第 2 步 / 共 3 步</div>
      <h1 className="mt-4 font-display text-display-32 text-gray-1000">
        {selection.status === 'empty' ? '没有找到可用的 AI' : '你的 AI 搭档'}
      </h1>

      {scanning ? (
        <p role="status" className="mt-3 text-copy-14 text-gray-900">
          正在你的电脑上寻找可用的 AI…
        </p>
      ) : null}

      {selection.status === 'ready' ? (
        <p className="mt-3 text-copy-14 text-gray-900">
          在你的电脑上找到了 {selection.choices.length} 个可用的 AI，推荐用第一个
        </p>
      ) : null}

      {selection.status === 'empty' ? (
        <p className="mt-3 max-w-[480px] text-center text-copy-14 text-gray-900">
          Nexus 需要一个本地 AI 助手（如 Claude Code、Codex CLI）来协同创作。
        </p>
      ) : null}

      {selection.status === 'error' ? (
        <div className="mt-3">
          <ScanErrorNotice reason={selection.errorReason} onRetry={selection.retry} />
        </div>
      ) : null}

      {selection.status === 'ready' ? (
        <div
          role="radiogroup"
          aria-label="你的 AI 搭档"
          className="mt-8 flex w-[480px] flex-col gap-3"
        >
          {selection.choices.map((choice) => (
            <AgentRow
              key={choice.pickerId}
              choice={choice}
              description={
                choice.pickerId === selection.recommendedId
                  ? '已安装 · 开箱即用 · 推荐'
                  : '已安装 · 可切换'
              }
              selected={choice.pickerId === selection.selectedId}
              onSelect={() => selection.select(choice.pickerId)}
            />
          ))}
        </div>
      ) : null}

      {scanning ? null : (
        <div className="mt-6 flex w-[480px] flex-col gap-3" data-testid="agent-step-advanced">
          {selection.status === 'error' ? null : (
            <>
              <button
                type="button"
                aria-expanded={showInstallGuide}
                onClick={() => setShowInstallGuide((open) => !open)}
                className={LINK_CLASS}
              >
                ▸ 都没有？查看如何安装一个 AI 助手
              </button>
              {showInstallGuide ? (
                <div className="flex flex-col gap-1.5 pl-2">
                  {INSTALL_TARGETS.map((target) => (
                    <InstallLink key={target.url} target={target} desktop={desktop} />
                  ))}
                </div>
              ) : null}
            </>
          )}

          <button
            type="button"
            aria-expanded={showManual}
            onClick={() => setShowManual((open) => !open)}
            className={LINK_CLASS}
          >
            ▸ 高级：手动指定启动命令
          </button>
          {showManual ? (
            <ManualCommandField
              id="onboarding-manual-launch"
              value={selection.manual}
              onChange={selection.setManual}
            />
          ) : null}
        </div>
      )}

      <div className="mt-12 flex w-[480px] items-center justify-between">
        <Button variant="tertiary" onClick={onBack}>
          返回
        </Button>
        <Button variant="primary" disabled={!canContinue} onClick={advance}>
          下一步
        </Button>
      </div>
    </div>
  );
}
