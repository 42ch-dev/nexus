import type { ReactNode } from 'react';

import { cn, Badge, Card } from '@42ch/nexus-ui';

/* ------------------------------------------------------------------ */
/*  Data — Voice & Content specimens from DESIGN.md §Voice & Content   */
/*  and IA guide §4.4. Every fixture string is canonical from the      */
/*  merged DESIGN SSOT.                                                */
/* ------------------------------------------------------------------ */

interface VoiceSpecimen {
  /** Pattern label per §4.4 table. */
  label: string;
  /** The voice rule this specimen demonstrates. */
  rule: string;
  /** Canonical fixture string from DESIGN SSOT. */
  fixture: string | { parts: string[] };
  /** Optional DESIGN.md surface-label from the §4.4 table (e.g. "Page title"). */
  surfaceLabel?: string;
  /** Whether the fixture is a multi-part row (like Verb-only buttons). */
  multi?: boolean;
}

const SPECIMENS: VoiceSpecimen[] = [
  {
    label: 'Title Case',
    rule: 'Page titles, nav items, tabs, table headers',
    fixture: 'Welcome to Nexus',
    surfaceLabel: 'Page title',
  },
  {
    label: 'Sentence case',
    rule: 'Helper text, empty states, descriptions',
    fixture:
      'Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.',
    surfaceLabel: 'Helper text',
  },
  {
    label: 'Verb-only',
    rule: 'Buttons and CTAs use a single Title Case verb; name the object in the dialog title when screen readers need it',
    fixture: { parts: ['Save', 'Create', 'Delete'] },
    surfaceLabel: 'Button / CTA',
    multi: true,
  },
  {
    label: 'Action + object',
    rule: 'Dialog titles name the action and the changed object — the button stays Verb-only',
    fixture: 'Delete Work',
    surfaceLabel: 'Dialog title',
  },
  {
    label: 'Sentence case, object named',
    rule: 'What happened. What to do next. — no protocol jargon',
    fixture:
      'Preset validation failed. Fix the YAML errors and validate again.',
    surfaceLabel: 'Error toast',
  },
  {
    label: 'Empty state',
    rule: 'Sentence case; offer the first action when applicable',
    fixture: 'No works yet. Create a Work to start the local loop.',
    surfaceLabel: 'Empty state',
  },
  {
    label: 'Loading',
    rule: 'Progress indicator — sentence case, no trailing period',
    fixture: 'Loading works…',
    surfaceLabel: 'Loading',
  },
  {
    label: 'Success toast',
    rule: 'Toast — name the changed object; no trailing period',
    fixture: 'Preset validated',
    surfaceLabel: 'Toast',
  },
];

/* ------------------------------------------------------------------ */
/*  Sub-components                                                      */
/* ------------------------------------------------------------------ */

function SectionHeading({ children }: { children: ReactNode }) {
  return (
    <h3 className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-16">
      {children}
    </h3>
  );
}

function VoiceCard({
  specimen,
}: {
  specimen: VoiceSpecimen;
}) {
  const fixtureText =
    typeof specimen.fixture === 'string'
      ? specimen.fixture
      : specimen.fixture.parts.join(' · ');

  return (
    <Card className="transition-colors">
      {/* Pattern label + surface label + rule */}
      <div className="flex flex-wrap items-center gap-2 mb-4">
        <span className="text-heading-16 font-semibold text-gray-1000">
          {specimen.label}
        </span>
        {specimen.surfaceLabel && (
          <Badge variant="neutral">{specimen.surfaceLabel}</Badge>
        )}
      </div>
      <p className="text-copy-14 text-gray-700 mb-5">{specimen.rule}</p>

      {/* Fixture display */}
      <div className="border border-gray-alpha-200 rounded-control bg-background-200 p-5">
        {specimen.multi ? (
          <div className="flex flex-wrap items-center gap-2">
            {(specimen.fixture as { parts: string[] }).parts.map(
              (part, idx) => (
                <span
                  key={idx}
                  className={cn(
                    'inline-flex items-center rounded-control border border-gray-alpha-300',
                    'bg-brand-cyan text-brand-deep-blue px-4 h-10 text-button-14 font-button',
                  )}
                >
                  {part}
                </span>
              ),
            )}
          </div>
        ) : (
          <p className="text-copy-16 text-gray-1000 leading-relaxed">
            {fixtureText}
          </p>
        )}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  GUIDANCE SUMMARY (DESIGN.md §Voice & Content verbatim)             */
/* ------------------------------------------------------------------ */

const GUIDANCE_RULES = [
  'Helpful, plain, local-first — a careful CLI message translated into UI copy.',
  'Title Case for nav, buttons, page titles, action verbs; sentence case for helpers, errors, and toasts.',
  'Buttons and CTAs are Verb-only: Save, Create, Delete (zh-CN: 保存, 创建, 删除).',
  'Boundary: page titles, dialog titles, helpers, and toasts still name the object (e.g. dialog title Delete Work; button Delete).',
  'Prefer author-facing nouns: Work, preset, finding, local daemon.',
  'Toasts name the changed object; no trailing period.',
  'Avoid protocol jargon (ACP, cursor token) in product surfaces unless diagnostics explicitly require it.',
];

function GuidanceBlock() {
  return (
    <Card>
      <h4 className="text-heading-16 font-semibold text-gray-1000 mb-4">
        DESIGN.md § Voice &amp; Content — Summary
      </h4>
      <ul className="space-y-2">
        {GUIDANCE_RULES.map((rule, idx) => (
          <li key={idx} className="flex items-start gap-2">
            <span
              aria-hidden="true"
              className="text-brand-cyan font-semibold shrink-0 mt-0.5"
            >
              •
            </span>
            <span className="text-copy-14 text-gray-900">{rule}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function VoicePage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">
        Voice &amp; Content
      </h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        Labeled writing-pattern specimens from the{' '}
        <a
          href="https://github.com/42ch/nexus/blob/main/DESIGN.md#voice--content"
          target="_blank"
          rel="noopener noreferrer"
          className="text-brand-deep-blue underline hover:opacity-80 dark:text-blue-700"
        >
          root DESIGN.md § Voice &amp; Content
        </a>
        , rendered per IA guide §4.4. Every fixture string is canonical — do not
        substitute marketing voice in product copy.
      </p>

      {/* Guidance block first */}
      <GuidanceBlock />

      {/* Page title pattern */}
      <section>
        <SectionHeading>Writing Patterns</SectionHeading>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {SPECIMENS.map((s) => (
            <VoiceCard key={s.label} specimen={s} />
          ))}
        </div>
      </section>

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        8 writing-pattern specimens drawn from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          DESIGN.md § Voice &amp; Content
        </code>{' '}
        — Verb-only button rule per DESIGN.md §Voice &amp; Content (V1.117);
        labels and rules per IA guide §4.4. Applied across{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          apps/web
        </code>{' '}
        for page titles, buttons/CTAs, dialog titles, helpers, toasts, empty
        states, and loading states.
      </p>
    </div>
  );
}
