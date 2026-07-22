/**
 * Studio fixtures for V1.132 P2 VI aesthetic retune — visual acceptance targets.
 *
 * Proves VI-001..VI-005 target states in light + dark before App wiring (T3+).
 * Fixture-local target styling only; wire and package primitives unchanged in T1.
 */

import type { ReactNode } from 'react';

import {
  Button,
  NexusLogo,
  NexusMark,
  TransportErrorBlock,
  logoCompactMarkHeightPx,
  logoMarkAspectRatio,
  logoShellHeightPx,
  logoSquareVariants,
  logoVariants,
} from '@42ch/nexus-ui';

import logoMonoSrc from '@42ch/nexus-ui/assets/logos/logo-mono.svg';
import logoPrimarySrc from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import logoPrimarySquareSrc from '@42ch/nexus-ui/assets/logos/logo-primary-square.svg';
import logoWhiteBgSquareSrc from '@42ch/nexus-ui/assets/logos/logo-white-bg-square.svg';
import logoWhiteSrc from '@42ch/nexus-ui/assets/logos/logo-white.svg';

import {
  AgentPicker,
  type AgentPickerItem,
} from '@web-setup/agent-picker';

/* ------------------------------------------------------------------ */
/*  Shared chrome                                                       */
/* ------------------------------------------------------------------ */

const VI_IDS = ['VI-001', 'VI-002', 'VI-003', 'VI-004', 'VI-005'] as const;
export type ViLedgerId = (typeof VI_IDS)[number];

/** Compact timeline mark SSOT (−30% from legacy shell height). */
export const VI_COMPACT_MARK_HEIGHT_PX = logoCompactMarkHeightPx;

function ViLedgerBadge({ id }: { id: ViLedgerId }) {
  return (
    <span
      data-testid={`vi-ledger-${id.toLowerCase()}`}
      className="inline-flex items-center rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-2 py-0.5 text-label-12 font-medium text-gray-1000"
    >
      {id}
    </span>
  );
}

function ThemePair({
  testId,
  light,
  dark,
}: {
  testId: string;
  light: ReactNode;
  dark: ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className="grid grid-cols-1 gap-4 sm:grid-cols-2"
    >
      <div
        data-testid={`${testId}-light`}
        className="rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      >
        <p className="mb-3 text-label-14 font-medium text-gray-1000">Light shell</p>
        {light}
      </div>
      <div
        data-testid={`${testId}-dark`}
        className="rounded-card border border-gray-alpha-300 bg-[#08141C] p-4"
      >
        <p className="mb-3 text-label-14 font-medium text-brand-cyan">Dark shell</p>
        {dark}
      </div>
    </div>
  );
}

function ViSection({
  id,
  ledgerId,
  title,
  description,
  sectionTestId,
  children,
}: {
  id: string;
  ledgerId: ViLedgerId;
  title: string;
  description: string;
  /** Override when multiple sections share one ledger id (e.g. VI-002 button vs transport). */
  sectionTestId?: string;
  children: ReactNode;
}) {
  return (
    <section
      id={id}
      data-testid={sectionTestId ?? `vi-section-${ledgerId.toLowerCase()}`}
      className="scroll-mt-16 border-t border-gray-alpha-200 pt-8 first:border-t-0 first:pt-0"
    >
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <ViLedgerBadge id={ledgerId} />
        <h4 className="text-heading-16 font-heading text-gray-1000">{title}</h4>
      </div>
      <p className="mb-4 max-w-prose text-copy-14 text-gray-700">{description}</p>
      {children}
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-005 — plain vs *-square asset split (Brand)                      */
/* ------------------------------------------------------------------ */

function AssetSplitPanel({
  label,
  fileName,
  children,
  panelClassName,
}: {
  label: string;
  fileName: string;
  children: ReactNode;
  panelClassName: string;
}) {
  return (
    <div className="overflow-hidden rounded-card border border-gray-alpha-300">
      <div className={`flex min-h-[120px] items-center justify-center p-6 ${panelClassName}`}>
        {children}
      </div>
      <div className="border-t border-gray-alpha-200 bg-gray-alpha-100 px-4 py-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-label-14 font-medium text-gray-1000">{label}</span>
          <code className="truncate text-copy-13-mono text-gray-600">{fileName}</code>
        </div>
      </div>
    </div>
  );
}

export function ViBrandAssetSplitFixture() {
  return (
    <ViSection
      id="vi-005-asset-split"
      ledgerId="VI-005"
      title="Plain mark vs square plate"
      description="Plain wide marks (`logoVariants`) vs plated lockups (`logoSquareVariants` / *-square.svg). Consumers must not swap plain and square assets ad hoc."
    >
      <ThemePair
        testId="vi-005-asset-split"
        light={
          <div
            data-testid="vi-005-asset-split-grid"
            className="grid grid-cols-1 gap-4"
          >
            <AssetSplitPanel
              label="Plain primary mark"
              fileName={logoVariants.primary}
              panelClassName="bg-brand-deep-blue"
            >
              <NexusLogo
                variant="primary"
                src={logoPrimarySrc}
                size={32}
                className="h-8 w-auto"
              />
            </AssetSplitPanel>

            <AssetSplitPanel
              label="Square plate lockup"
              fileName={logoSquareVariants.primary}
              panelClassName="bg-brand-deep-blue"
            >
              <img
                src={logoPrimarySquareSrc}
                alt="Nexus square plate"
                decoding="async"
                data-testid="vi-005-square-plate"
                className="block h-24 w-24 object-contain"
              />
            </AssetSplitPanel>

            <AssetSplitPanel
              label="White plate (light surfaces)"
              fileName={logoSquareVariants.whiteBg}
              panelClassName="bg-white"
            >
              <img
                src={logoWhiteBgSquareSrc}
                alt="Nexus white plate"
                decoding="async"
                className="block h-24 w-24 object-contain"
              />
            </AssetSplitPanel>
          </div>
        }
        dark={
          <div className="grid grid-cols-1 gap-4">
            <AssetSplitPanel
              label="Inline mark (currentColor)"
              fileName="logo-white.svg / &lt;NexusMark&gt;"
              panelClassName="bg-brand-deep-blue"
            >
              <NexusMark size={28} className="w-auto text-brand-cyan" />
            </AssetSplitPanel>

            <AssetSplitPanel
              label="Plain mono mark"
              fileName={logoVariants.mono}
              panelClassName="bg-background-100"
            >
              <NexusLogo
                variant="mono"
                src={logoMonoSrc}
                size={32}
                className="h-8 w-auto text-brand-deep-blue"
              />
            </AssetSplitPanel>
          </div>
        }
      />
    </ViSection>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-003 — compact timeline mark scale (Brand)                        */
/* ------------------------------------------------------------------ */

function TitlebarMarkRow({
  markHeightPx,
  label,
}: {
  markHeightPx: number;
  label: string;
}) {
  return (
    <div className="overflow-hidden rounded-card border border-gray-alpha-300">
      <div className="flex h-10 items-center gap-3 bg-brand-deep-blue px-3">
        <NexusLogo
          variant="white"
          src={logoWhiteSrc}
          size={markHeightPx}
          className="w-auto shrink-0"
          style={{ height: `${markHeightPx}px` }}
        />
        <span className="text-label-14 text-white">{label}</span>
      </div>
    </div>
  );
}

export function ViBrandCompactMarkFixture() {
  const legacyHeight = logoShellHeightPx;
  const compactHeight = logoCompactMarkHeightPx;

  return (
    <ViSection
      id="vi-003-compact-mark"
      ledgerId="VI-003"
      title="Compact timeline mark (−30%–50%)"
      description={`SSOT compact mark height: ${compactHeight}px (−30% from legacy ${legacyHeight}px shell baseline). Titlebar, Brand hero, and app icon share this scale. Wordmark scale stays separate.`}
    >
      <ThemePair
        testId="vi-003-compact-mark"
        light={
          <div className="space-y-3">
            <TitlebarMarkRow markHeightPx={legacyHeight} label={`Legacy — ${legacyHeight}px`} />
            <TitlebarMarkRow markHeightPx={compactHeight} label={`SSOT — ${compactHeight}px`} />
          </div>
        }
        dark={
          <div className="space-y-3">
            <TitlebarMarkRow markHeightPx={legacyHeight} label={`Legacy — ${legacyHeight}px`} />
            <TitlebarMarkRow markHeightPx={compactHeight} label={`SSOT — ${compactHeight}px`} />
          </div>
        }
      />

      <p className="mt-4 text-copy-13 text-gray-600">
        Wide mark aspect ≈ {logoMarkAspectRatio.toFixed(2)}:1 — size by height, not a square box.
      </p>
    </ViSection>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-004 — app icon inset compose (Brand)                             */
/* ------------------------------------------------------------------ */

function SquircleFrame({
  insetPx,
  label,
  testId,
}: {
  insetPx: number;
  label: string;
  testId: string;
}) {
  const innerSize = 96 - insetPx * 2;
  return (
    <div className="flex flex-col items-center gap-2" data-testid={testId}>
      <div
        className="relative flex h-24 w-24 items-center justify-center overflow-hidden rounded-[22%] bg-[#1a1a1a] shadow-elevation-2"
        style={{ boxShadow: insetPx === 0 ? '0 0 0 2px rgba(255,255,255,0.35)' : undefined }}
      >
        <img
          src={logoPrimarySquareSrc}
          alt=""
          decoding="async"
          className="object-contain"
          style={{
            width: innerSize,
            height: innerSize,
          }}
        />
      </div>
      <span className="text-center text-copy-13 text-gray-700">{label}</span>
    </div>
  );
}

export function ViBrandAppIconFixture() {
  return (
    <ViSection
      id="vi-004-app-icon"
      ledgerId="VI-004"
      title="App icon inset compose"
      description="Square plate source (`logo-primary-square.svg`) with transparent inset margins inside the macOS squircle (T4 compose). No light rectangular halo at plate edges."
    >
      <ThemePair
        testId="vi-004-app-icon"
        light={
          <div
            data-testid="vi-004-app-icon-compare"
            className="flex flex-wrap items-start justify-center gap-8 rounded-card border border-gray-alpha-300 bg-background-100 p-6"
          >
            <SquircleFrame
              insetPx={0}
              label="Current — edge-to-edge (halo risk)"
              testId="vi-004-app-icon-current"
            />
            <SquircleFrame
              insetPx={12}
              label="Target — ~12% inset margin"
              testId="vi-004-app-icon-target"
            />
          </div>
        }
        dark={
          <div className="flex flex-wrap items-start justify-center gap-8 rounded-card border border-gray-alpha-300 bg-[#08141C] p-6">
            <SquircleFrame
              insetPx={0}
              label="Current — edge-to-edge (halo risk)"
              testId="vi-004-app-icon-current-dark"
            />
            <SquircleFrame
              insetPx={12}
              label="Target — ~12% inset margin"
              testId="vi-004-app-icon-target-dark"
            />
          </div>
        }
      />
      <p className="mt-4 text-copy-13 text-gray-600">
        Compose layer owns inset — consumers use the composed asset, not ad-hoc padding.
      </p>
    </ViSection>
  );
}

export function ViBrandAcceptanceFixtures() {
  return (
    <div data-testid="vi-brand-acceptance-fixtures" className="space-y-8">
      <ViBrandAssetSplitFixture />
      <ViBrandCompactMarkFixture />
      <ViBrandAppIconFixture />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-002 — theme-aware primary Button (Components)                    */
/* ------------------------------------------------------------------ */

/** Target light-shell primary — ink fill, not neon cyan + deep ink label. */
function TargetPrimaryButton({ children }: { children: ReactNode }) {
  return (
    <button
      type="button"
      data-testid="vi-002-target-primary-button"
      className="inline-flex h-10 items-center justify-center rounded-control bg-brand-deep-blue px-4 text-button-14 font-button text-brand-white transition-colors duration-state ease-standard hover:bg-blue-900 active:bg-blue-950"
    >
      {children}
    </button>
  );
}

/** Target dark-shell primary — strong cyan CTA (matches current dark intent). */
function TargetDarkPrimaryButton({ children }: { children: ReactNode }) {
  return (
    <button
      type="button"
      data-testid="vi-002-target-dark-primary-button"
      className="inline-flex h-10 items-center justify-center rounded-control bg-brand-cyan px-4 text-button-14 font-button text-brand-deep-blue transition-colors duration-state ease-standard hover:bg-blue-800 active:bg-blue-900"
    >
      {children}
    </button>
  );
}

export function ViButtonAcceptanceFixtures() {
  return (
    <ViSection
      id="vi-002-primary-button"
      ledgerId="VI-002"
      title="Theme-aware primary Button"
      description="Target (T3): light-shell primary uses ink-structure fill — not neon cyan on deep ink label. Dark shell keeps the strong cyan CTA. TransportError Retry consumes the same Button variant."
    >
      <ThemePair
        testId="vi-002-primary-button"
        light={
          <div className="space-y-4">
            <div>
              <p className="mb-2 text-copy-13 text-gray-600">Current (package)</p>
              <Button variant="primary" data-testid="vi-002-current-primary-light">
                Retry
              </Button>
            </div>
            <div>
              <p className="mb-2 text-copy-13 text-gray-600">Target (fixture)</p>
              <TargetPrimaryButton>Retry</TargetPrimaryButton>
            </div>
          </div>
        }
        dark={
          <div className="space-y-4">
            <div>
              <p className="mb-2 text-copy-13 text-gray-300">Current (package)</p>
              <Button variant="primary" data-testid="vi-002-current-primary-dark">
                Retry
              </Button>
            </div>
            <div>
              <p className="mb-2 text-copy-13 text-gray-300">Target (fixture)</p>
              <TargetDarkPrimaryButton>Retry</TargetDarkPrimaryButton>
            </div>
          </div>
        }
      />
    </ViSection>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-002 — TransportError Retry (Components)                          */
/* ------------------------------------------------------------------ */

function noop() {
  /* Studio fixture — clicks have nowhere to go. */
}

export function ViTransportErrorAcceptanceFixtures() {
  return (
    <ViSection
      id="vi-002-transport-error"
      ledgerId="VI-002"
      sectionTestId="vi-section-vi-002-transport"
      title="TransportError Retry inherits Button"
      description="Target: daemon_down Retry uses the theme-aware primary Button — no one-off error-block styling."
    >
      <ThemePair
        testId="vi-002-transport-error"
        light={
          <div className="space-y-4">
            <TransportErrorBlock kind="daemon_down" onRetry={noop} />
            <div className="rounded-card border border-dashed border-brand-cyan/40 p-3">
              <p className="mb-2 text-copy-13 text-gray-600">Target Retry styling (fixture)</p>
              <TargetPrimaryButton>Retry</TargetPrimaryButton>
            </div>
          </div>
        }
        dark={
          <div className="space-y-4">
            <TransportErrorBlock kind="daemon_down" onRetry={noop} />
            <div className="rounded-card border border-dashed border-brand-cyan/30 p-3">
              <p className="mb-2 text-copy-13 text-gray-300">Target Retry styling (fixture)</p>
              <TargetDarkPrimaryButton>Retry</TargetDarkPrimaryButton>
            </div>
          </div>
        }
      />
    </ViSection>
  );
}

/* ------------------------------------------------------------------ */
/*  VI-001 — AgentPicker single selected affordance                     */
/* ------------------------------------------------------------------ */

const VI_AGENT_GRID: AgentPickerItem[] = [
  {
    id: 'claude-native',
    name: 'claude (native CLI)',
    version: '1.0.42',
    description: 'Anthropic coding agent via native CLI.',
    installed: true,
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  {
    id: 'codex-native',
    name: 'codex (native CLI)',
    version: '0.12.0',
    description: 'OpenAI Codex CLI.',
    installed: true,
    installUrl: 'https://github.com/openai/codex',
    docsUrl: null,
  },
];

/**
 * Target card mock — single ring affordance; no competing bg tint + status-dot fill.
 * Fixture-only; T3 retunes the shared AgentPicker primitive.
 */
function TargetSelectedAgentCard({ isDark }: { isDark: boolean }) {
  const shellClass = isDark ? 'bg-[#0D1B26]' : 'bg-background-100';
  const textClass = isDark ? 'text-gray-100' : 'text-gray-1000';
  const mutedClass = isDark ? 'text-gray-400' : 'text-gray-700';

  return (
    <div
      data-testid={isDark ? 'vi-001-target-card-dark' : 'vi-001-target-card-light'}
      className={`w-full max-w-sm rounded-control border-2 border-blue-700 ${shellClass} p-3`}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-2">
            <span
              aria-hidden
              className="h-2 w-2 shrink-0 rounded-full border border-gray-500 bg-transparent"
            />
            <span className={`truncate text-copy-14 font-medium ${textClass}`}>
              claude (native CLI)
            </span>
          </div>
          <span className={`text-copy-13 ${mutedClass}`}>v1.0.42</span>
        </div>
        <span className="shrink-0 rounded-pill bg-green-700/15 px-2 py-0.5 text-label-12 text-green-700 dark:text-green-400">
          Installed
        </span>
      </div>
      <p className={`mt-2 text-copy-13 ${mutedClass}`}>
        Single affordance: 2px selection ring only — no card fill tint, no lit status dot.
      </p>
    </div>
  );
}

export function ViAgentPickerAcceptanceFixtures() {
  return (
    <ViSection
      id="vi-001-agent-picker"
      ledgerId="VI-001"
      title="Setup agent selection — one affordance"
      description="Target (T3): selected installed agent shows one clear affordance — not competing border tint, status-dot fill, and ring signals."
    >
      <ThemePair
        testId="vi-001-agent-picker"
        light={
          <div className="space-y-6">
            <div>
              <p className="mb-2 text-copy-13 text-gray-600">Current (package)</p>
              <AgentPicker
                status="ready"
                defaultGrid={VI_AGENT_GRID}
                selectedId="claude-native"
                onSelect={() => undefined}
                customLaunchValue=""
                onCustomLaunchChange={() => undefined}
              />
            </div>
            <div>
              <p className="mb-2 text-copy-13 text-gray-600">Target (fixture)</p>
              <TargetSelectedAgentCard isDark={false} />
            </div>
          </div>
        }
        dark={
          <div className="space-y-6">
            <div>
              <p className="mb-2 text-copy-13 text-gray-300">Current (package)</p>
              <AgentPicker
                status="ready"
                defaultGrid={VI_AGENT_GRID}
                selectedId="claude-native"
                onSelect={() => undefined}
                customLaunchValue=""
                onCustomLaunchChange={() => undefined}
              />
            </div>
            <div>
              <p className="mb-2 text-copy-13 text-gray-300">Target (fixture)</p>
              <TargetSelectedAgentCard isDark />
            </div>
          </div>
        }
      />
    </ViSection>
  );
}
