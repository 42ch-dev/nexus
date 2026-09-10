import { useState, type HTMLAttributes, type ReactNode } from 'react';

import {
  cn,
  Badge,
  Button,
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Input,
  Label,
  Textarea,
  Select,
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
} from '@42ch/nexus-ui';

import { SurfaceSourceBadges } from '@/components/surface-source-badge';
import { Dialog, DialogTrigger, DialogContent } from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)
import { Spinner, LoadingState, EmptyState, ErrorState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
import { ToastFixtures } from '@/fixtures/toast-fixtures';
import { TransportErrorBlockFixtures } from '@/fixtures/transport-error-block';
import { ComputeRunStudioFixtures } from '@/fixtures/compute-run-studio';
import { TimelineComputeFixtures } from '@/fixtures/timeline-compute';
import {
  ViButtonAcceptanceFixtures,
  ViTransportErrorAcceptanceFixtures,
} from '@/fixtures/vi-aesthetic-retune-fixtures';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@web-ui/table'; // transitional — keep-web (responsive overflow wrapper; not in V1.99 first batch)

/* ------------------------------------------------------------------ */
/*  Shared helpers                                                      */
/* ------------------------------------------------------------------ */

function SectionHeading({
  id,
  children,
}: {
  id: string;
  children: ReactNode;
}) {
  return (
    <h3
      id={id}
      className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-sticky-header"
    >
      {children}
    </h3>
  );
}

function MatrixCard({
  children,
  className,
  ...rest
}: { children: ReactNode; className?: string } & HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'border border-gray-alpha-300 rounded-card bg-background-100 p-6',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
}

function VariantLabel({ label }: { label: string }) {
  return (
    <span className="text-copy-13 text-gray-700 font-medium shrink-0 min-w-[80px]">
      {label}
    </span>
  );
}

function MatrixRow({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-4 py-3 border-b border-gray-alpha-200 last:border-b-0">
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Sub-nav                                                             */
/* ------------------------------------------------------------------ */

function SubNav() {
  const sections = [
    { label: 'Badge', href: '#comp-badge' },
    { label: 'Button', href: '#comp-button' },
    { label: 'Card', href: '#comp-card' },
    { label: 'Dialog', href: '#comp-dialog' },
    { label: 'Domain Badges', href: '#comp-domain-badges' },
    { label: 'Input', href: '#comp-input' },
    { label: 'Label', href: '#comp-label' },
    { label: 'Select', href: '#comp-select' },
    { label: 'States', href: '#comp-states' },
    { label: 'Table', href: '#comp-table' },
    { label: 'Tabs', href: '#comp-tabs' },
    { label: 'Textarea', href: '#comp-textarea' },
    { label: 'Form Field', href: '#comp-form-field' },
    { label: 'Toast', href: '#comp-toast' },
    { label: 'Transport Error', href: '#comp-transport-error-block' },
    { label: 'Run Studio', href: '#comp-run-studio' },
    { label: 'Compute Timeline', href: '#comp-compute-timeline' },
    { label: 'VI acceptance', href: '#comp-vi-acceptance' },
  ];

  return (
    <nav
      aria-label="Component sub-sections"
      className="flex flex-wrap gap-1 mb-8"
    >
      {sections.map(({ label, href }) => (
        <a
          key={href}
          href={href}
          className="px-3 py-1.5 rounded-md text-label-14 text-gray-700 hover:text-gray-1000 hover:bg-gray-alpha-100 transition-colors no-underline"
        >
          {label}
        </a>
      ))}
    </nav>
  );
}

/* ------------------------------------------------------------------ */
/*  1. Badge                                                            */
/* ------------------------------------------------------------------ */

function BadgeSection() {
  const variants = [
    { variant: 'neutral' as const, label: 'neutral' },
    { variant: 'running' as const, label: 'running' },
    { variant: 'queued' as const, label: 'queued' },
    { variant: 'warning' as const, label: 'warning' },
    { variant: 'error' as const, label: 'error' },
    { variant: 'preset' as const, label: 'preset' },
  ];

  return (
    <section data-testid="badge-fixtures">
      <SectionHeading id="comp-badge">Badge</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        Status pill — 6 semantic variants × soft/solid tone per DESIGN.md § Badge.
        Default tone is soft; solid is opt-in emphasis.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">Soft (default)</p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-center gap-4">
          {variants.map(({ variant, label }) => (
            <div key={`soft-${variant}`} className="flex flex-col items-center gap-2">
              <Badge tone="soft" variant={variant}>
                {label}
              </Badge>
              <VariantLabel label={label} />
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Solid</p>
      <MatrixCard>
        <div className="flex flex-wrap items-center gap-4">
          {variants.map(({ variant, label }) => (
            <div key={`solid-${variant}`} className="flex flex-col items-center gap-2">
              <Badge tone="solid" variant={variant} data-testid={`badge-solid-${variant}`}>
                {label}
              </Badge>
              <VariantLabel label={label} />
            </div>
          ))}
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  1a. Domain Badges                                                   */
/* ------------------------------------------------------------------ */

function humanizeDomainValue(value: string): string {
  return value
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function DomainBadgeSection() {
  const statusItems = [
    { value: 'running', variant: 'running' as const },
    { value: 'queued', variant: 'queued' as const },
    { value: 'warning', variant: 'warning' as const },
    { value: 'error', variant: 'error' as const },
    { value: 'unknown', variant: 'neutral' as const },
  ];

  const chapterItems = [
    { value: 'not_started', variant: 'neutral' as const },
    { value: 'outlined', variant: 'queued' as const },
    { value: 'draft', variant: 'warning' as const },
    { value: 'finalized', variant: 'running' as const },
    { value: 'published', variant: 'preset' as const },
  ];

  // Full literal class strings (no template interpolation) so Tailwind's
  // content scan picks up every token class — mirrors production
  // `findingStatusClasses` in apps/web/src/components/status-badge.tsx.
  const findingItems = [
    {
      value: 'open',
      classes:
        'bg-finding-status-open-bg text-finding-status-open-text border-finding-status-open-border',
    },
    {
      value: 'triaged',
      classes:
        'bg-finding-status-triaged-bg text-finding-status-triaged-text border-finding-status-triaged-border',
    },
    {
      value: 'in_review',
      classes:
        'bg-finding-status-in-review-bg text-finding-status-in-review-text border-finding-status-in-review-border',
    },
    {
      value: 'resolved',
      classes:
        'bg-finding-status-resolved-bg text-finding-status-resolved-text border-finding-status-resolved-border',
    },
    {
      value: 'wont_fix',
      classes:
        'bg-finding-status-wont-fix-bg text-finding-status-wont-fix-text border-finding-status-wont-fix-border',
    },
    {
      value: 'duplicate',
      classes:
        'bg-finding-status-duplicate-bg text-finding-status-duplicate-text border-finding-status-duplicate-border',
    },
  ];

  // Mirrors production `taskKindClasses` in
  // apps/web/src/components/memory/task-kind-badge.tsx.
  const taskKindItems = [
    {
      value: 'brainstorm',
      classes:
        'bg-memory-task-kind-brainstorm-bg text-memory-task-kind-brainstorm-text border-memory-task-kind-brainstorm-border',
    },
    {
      value: 'outline',
      classes:
        'bg-memory-task-kind-outline-bg text-memory-task-kind-outline-text border-memory-task-kind-outline-border',
    },
    {
      value: 'chapter',
      classes:
        'bg-memory-task-kind-chapter-bg text-memory-task-kind-chapter-text border-memory-task-kind-chapter-border',
    },
    {
      value: 'research',
      classes:
        'bg-memory-task-kind-research-bg text-memory-task-kind-research-text border-memory-task-kind-research-border',
    },
    {
      value: 'unknown',
      classes:
        'bg-memory-task-kind-unknown-bg text-memory-task-kind-unknown-text border-memory-task-kind-unknown-border',
    },
  ];

  return (
    <section data-testid="domain-badge-fixtures">
      <SectionHeading id="comp-domain-badges">Domain Badges</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        Domain-specific status pills mapped to the DESIGN.md semantic palette.
        Status and Chapter use the standard Badge variants; Finding and TaskKind
        use the{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">finding-status-*</code>{' '}
        and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">memory-task-kind-*</code>{' '}
        token classes (V1.121 P1) so each domain state stays distinct in both
        light and dark.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">Status</p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-center gap-4">
          {statusItems.map(({ value, variant }) => (
            <div key={`status-${value}`} className="flex flex-col items-center gap-2">
              <Badge variant={variant} data-testid={`domain-badge-status-${value}`}>
                {humanizeDomainValue(value)}
              </Badge>
              <VariantLabel label={value} />
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Chapter</p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-center gap-4">
          {chapterItems.map(({ value, variant }) => (
            <div key={`chapter-${value}`} className="flex flex-col items-center gap-2">
              <Badge variant={variant} data-testid={`domain-badge-chapter-${value}`}>
                {humanizeDomainValue(value)}
              </Badge>
              <VariantLabel label={value} />
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Finding</p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-center gap-4">
          {findingItems.map(({ value, classes }) => (
            <div key={`finding-${value}`} className="flex flex-col items-center gap-2">
              <Badge
                variant="neutral"
                className={classes}
                data-testid={`domain-badge-finding-${value}`}
              >
                {humanizeDomainValue(value)}
              </Badge>
              <VariantLabel label={value} />
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">TaskKind</p>
      <MatrixCard>
        <div className="flex flex-wrap items-center gap-4">
          {taskKindItems.map(({ value, classes }) => (
            <div key={`task-kind-${value}`} className="flex flex-col items-center gap-2">
              <Badge
                variant="neutral"
                className={classes}
                data-testid={`domain-badge-task-kind-${value}`}
              >
                {humanizeDomainValue(value)}
              </Badge>
              <VariantLabel label={value} />
            </div>
          ))}
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  2. Button                                                           */
/* ------------------------------------------------------------------ */

function ButtonSection() {
  const variants = [
    { variant: 'primary' as const, label: 'primary' },
    { variant: 'secondary' as const, label: 'secondary' },
    { variant: 'tertiary' as const, label: 'tertiary' },
    { variant: 'destructive' as const, label: 'destructive' },
  ];

  const sizes = [
    { size: 'tiny' as const, label: 'tiny (h-6)' },
    { size: 'small' as const, label: 'small (h-8)' },
    { size: 'default' as const, label: 'default (h-10)' },
    { size: 'large' as const, label: 'large (h-12)' },
  ];

  return (
    <section>
      <SectionHeading id="comp-button">Button</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        4 variants × 4 sizes = 16 combinations, plus disabled and focus-visible
        states per DESIGN.md § Button.
      </p>
      <p
        data-testid="button-chronos-note"
        className="text-copy-14 text-gray-700 mb-4 max-w-prose"
      >
        Primary cobalt uses{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">blue-700/800/900</code>{' '}
        rest/hover/active with{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">text-brand-white</code>{' '}
        in light and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          text-brand-deep-blue
        </code>{' '}
        on the lighter cobalt fill in dark. Toggle the theme to confirm both shells.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">
        Variant × Size matrix
      </p>
      <MatrixCard className="mb-6" data-testid="button-variant-matrix">
        <div className="space-y-4">
          {variants.map(({ variant }) => (
            <div key={variant} className="flex flex-wrap items-center gap-4">
              <VariantLabel label={variant} />
              {sizes.map(({ size, label }) => (
                <Button
                  key={size}
                  variant={variant}
                  size={size}
                  data-testid={variant === 'primary' ? `button-primary-${size}` : undefined}
                >
                  {label}
                </Button>
              ))}
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Disabled states</p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-center gap-4">
          {variants.map(({ variant }) => (
            <div key={variant} className="flex flex-col items-center gap-2">
              <Button variant={variant} disabled>
                {variant}
              </Button>
              <VariantLabel label={variant} />
            </div>
          ))}
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Focus-visible (Tab through to see the two-layer ring)
      </p>
      <MatrixCard>
        <div className="flex flex-wrap items-center gap-4">
          <Button variant="primary">focus me</Button>
          <Button variant="secondary">focus me</Button>
          <Button variant="tertiary">focus me</Button>
          <Button variant="destructive">focus me</Button>
        </div>
        <p className="text-copy-13 text-gray-700 mt-4">
          Press <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">Tab</kbd> to cycle
          through — the two-layer focus ring is applied globally via{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">{'src/index.css'}</code>.
        </p>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  3. Card                                                             */
/* ------------------------------------------------------------------ */

function CardSection() {
  return (
    <section data-testid="card-fixtures">
      <SectionHeading id="comp-card">Card</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        Rest surface plus the V1.121 v0.4 additions per DESIGN.md § Card: the{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">interactive</code>{' '}
        elevation recipe and the additive{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">CardTitle voice</code>{' '}
        opt-in.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">
        Elevation — rest vs interactive (hover the right card)
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-start gap-6">
          <div className="flex flex-col items-center gap-2">
            <Card data-testid="card-rest" className="w-72">
              <CardHeader>
                <CardTitle>Rest card</CardTitle>
                <CardDescription>
                  Default surface — elevation-1 at rest, no hover recipe.
                </CardDescription>
              </CardHeader>
              <CardContent>
                Static container for grouped content on background-100.
              </CardContent>
            </Card>
            <VariantLabel label="rest (default)" />
          </div>
          <div className="flex flex-col items-center gap-2">
            <Card data-testid="card-interactive" interactive className="w-72">
              <CardHeader>
                <CardTitle>Interactive card</CardTitle>
                <CardDescription>
                  Hover lifts to elevation-2 + translateY(-1px) over 160ms
                  ease-standard.
                </CardDescription>
              </CardHeader>
              <CardContent>
                Pressed returns to elevation-1; reduced-motion drops the lift.
              </CardContent>
            </Card>
            <VariantLabel label="interactive (hover me)" />
          </div>
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Interactive focus — a focusable consumer card keeps the two-layer ring
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-wrap items-start gap-6">
          <div className="flex flex-col items-center gap-2">
            <Card
              data-testid="card-interactive-focus"
              interactive
              tabIndex={0}
              aria-label="Focusable interactive card"
              className="w-72"
            >
              <CardHeader>
                <CardTitle>Focusable card</CardTitle>
                <CardDescription>
                  Native tabIndex keeps the shared surface gap + cobalt band
                  visible over the elevation recipe.
                </CardDescription>
              </CardHeader>
            </Card>
            <VariantLabel label="interactive focus-visible" />
          </div>
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Title voice — interface (default) vs content (creative entities)
      </p>
      <MatrixCard>
        <div className="flex flex-wrap items-start gap-6">
          <div className="flex flex-col items-center gap-2">
            <Card className="w-72">
              <CardHeader>
                <CardTitle data-testid="card-title-interface">
                  Interface title
                </CardTitle>
                <CardDescription>
                  Sans heading-16 — settings, dialogs, dashboards.
                </CardDescription>
              </CardHeader>
            </Card>
            <VariantLabel label='voice="interface" (default)' />
          </div>
          <div className="flex flex-col items-center gap-2">
            <Card className="w-72">
              <CardHeader>
                <CardTitle data-testid="card-title-content" voice="content">
                  The Lost City
                </CardTitle>
                <CardDescription>
                  Sans display-20 — reserved for creative-entity cards
                  (work/world).
                </CardDescription>
              </CardHeader>
            </Card>
            <VariantLabel label='voice="content"' />
          </div>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  4. Dialog                                                           */
/* ------------------------------------------------------------------ */

function DialogSection() {
  const [open, setOpen] = useState(false);

  return (
    <section data-testid="dialog-fixtures">
      <SectionHeading id="comp-dialog">Dialog</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Modal dialog — built on Radix for focus trap, Escape close, and ARIA.
        The overlay uses the{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">bg-scrim</code>{' '}
        token and the panel{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">shadow-elevation-4</code>{' '}
        (V1.121 scrim convergence). Open the dialog, press{' '}
        <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">Escape</kbd>{' '}
        to close, and focus returns to the trigger.
      </p>
      <MatrixCard>
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button variant="primary" data-testid="dialog-open-trigger">
              Open dialog
            </Button>
          </DialogTrigger>
          <DialogContent
            title="Example dialog"
            description="This dialog demonstrates the title, description, and action pattern."
          >
            <div className="flex flex-col gap-4">
              <p className="text-copy-14 text-gray-900">
                Dialog body content. The overlay dims the background and focus
                is trapped inside the modal until dismissed.
              </p>
              <div className="flex justify-end gap-3">
                <Button variant="secondary" onClick={() => setOpen(false)}>
                  Cancel
                </Button>
                <Button variant="primary" onClick={() => setOpen(false)}>
                  Confirm
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
        <p className="text-copy-13 text-gray-700 mt-4">
          Transitional{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/dialog</code>{' '}
          — Radix portal, focus trap, and scroll lock. Keyboard: Tab cycles
          trapped controls; Escape closes and restores focus to the trigger.
        </p>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  5. Input                                                            */
/* ------------------------------------------------------------------ */

function InputSection() {
  return (
    <section>
      <SectionHeading id="comp-input">Input</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Text input — default, hover, focus-visible, disabled, and invalid states.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-4 max-w-md">
          <MatrixRow>
            <VariantLabel label="default" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="input-default">Project codename</Label>
              <Input id="input-default" placeholder="Default input…" />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="focus-visible" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="input-focus">Focus target</Label>
              <Input id="input-focus" data-testid="input-focus" defaultValue="Tab or click to focus" />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="hover" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="input-hover">Hover target</Label>
              <Input
                id="input-hover"
                data-testid="input-fixture-hover"
                defaultValue="Point at the field"
              />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="input-disabled">Archived field</Label>
              <Input id="input-disabled" placeholder="Disabled input…" disabled />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="input-invalid">Executor label 执行器标签</Label>
              <Input
                id="input-invalid"
                invalid
                aria-describedby="input-invalid-error"
                defaultValue="bad value"
              />
              <p id="input-invalid-error" role="alert" className="text-copy-13 text-red-700">
                Choose a valid executor.
              </p>
            </div>
          </MatrixRow>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  6. Label                                                            */
/* ------------------------------------------------------------------ */

function LabelSection() {
  return (
    <section>
      <SectionHeading id="comp-label">Label</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Form label — label-14 weight 500, gray-1000 text. Wired to its control via{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">htmlFor</code>.
        Labels expose no disabled/invalid state of their own; the associated control shows it.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-3 max-w-md">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="demo-input">Field label</Label>
            <Input id="demo-input" placeholder="Click the label to focus this input" />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="label-disabled-control">Bio (associated disabled control)</Label>
            <Input id="label-disabled-control" placeholder="Disabled input" disabled />
          </div>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  7. Select                                                           */
/* ------------------------------------------------------------------ */

function SelectOptionList({ options }: { options: string[] }) {
  return (
    <>
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </>
  );
}

/**
 * Select gallery — V1.101 P2 visual acceptance fixtures.
 *
 * Source: `@42ch/nexus-ui` (promoted native `<select>`). Web keeps a thin
 * re-export under `apps/web/src/components/ui/select.tsx`.
 *
 * Open/expanded is UA-owned for native `<select>` — no package `open` prop.
 * The “open (manual)” row documents keyboard/pointer acceptance; automated
 * tests assert closed-control attributes and focus-visible class path only.
 */
function SelectSection() {
  const options = ['Option A', 'Option B', 'Option C'];
  const closedId = 'studio-select-closed';
  const invalidId = 'studio-select-invalid';
  const invalidHelperId = `${invalidId}-helper`;
  const focusId = 'studio-select-focus';

  return (
    <section data-testid="select-fixtures">
      <SectionHeading id="comp-select">Select</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        Native styled select per the locked V1.101 Select promotion contract —
        closed default, disabled, invalid, and focus-visible. Uses the native
        element for accessibility; DESIGN.md{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          input-select-textarea
        </code>{' '}
        tokens plus{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components.select
        </code>{' '}
        chevron inset. Imported directly from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>
        .
      </p>
      <p className="text-copy-13 text-gray-700 mb-6">
        Open listbox chrome is UA-owned — Tab to the control, then Space /
        Enter / Alt+↓ (platform-dependent) to open. Automated Studio tests do
        not drive OS listbox UI.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">
        States — closed, disabled, invalid
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-col gap-4 max-w-md">
          <MatrixRow>
            <VariantLabel label="closed" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor={closedId}>Work profile</Label>
              <Select
                id={closedId}
                data-testid="select-fixture-closed"
                className="w-full"
                defaultValue="Option A"
              >
                <SelectOptionList options={options} />
              </Select>
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="studio-select-disabled">Retired profile</Label>
              <Select
                id="studio-select-disabled"
                disabled
                data-testid="select-fixture-disabled"
                className="w-full"
                defaultValue="Option A"
              >
                <SelectOptionList options={options} />
              </Select>
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor={invalidId}>Executor</Label>
              <Select
                id={invalidId}
                invalid
                data-testid="select-fixture-invalid"
                className="w-full"
                defaultValue="Option A"
                aria-describedby={invalidHelperId}
              >
                <SelectOptionList options={options} />
              </Select>
              <p id={invalidHelperId} className="text-copy-13 text-red-700" role="alert">
                Choose a valid executor.
              </p>
            </div>
          </MatrixRow>
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Focus-visible (Tab to see border + global ring)
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-col gap-1.5 max-w-md">
          <Label htmlFor={focusId}>Focus target</Label>
          <Select
            id={focusId}
            data-testid="select-fixture-focus"
            className="w-full"
            defaultValue="Option B"
          >
            <SelectOptionList options={options} />
          </Select>
        </div>
        <p className="text-copy-13 text-gray-700 mt-4">
          Press{' '}
          <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">Tab</kbd>{' '}
          onto the control — package class{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            focus-visible:border-blue-700
          </code>{' '}
          plus the global two-layer ring from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            src/index.css
          </code>
          .
        </p>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Hover (point at the control — background-200 fill wash, gray-500 border preserved)
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-col gap-1.5 max-w-md">
          <Label htmlFor="studio-select-hover">Hover target</Label>
          <Select
            id="studio-select-hover"
            data-testid="select-fixture-hover"
            className="w-full"
            defaultValue="Option B"
          >
            <SelectOptionList options={options} />
          </Select>
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Open (manual visual acceptance)
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-1.5 max-w-md">
          <Label htmlFor="studio-select-open-manual">Open listbox manually</Label>
          <Select
            id="studio-select-open-manual"
            data-testid="select-fixture-open-manual"
            className="w-full"
            defaultValue="Option A"
          >
            <SelectOptionList options={options} />
          </Select>
        </div>
        <p className="text-copy-13 text-gray-700 mt-4">
          With the control focused, open the native list (Space / Enter /
          Alt+↓). There is no package{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            open
          </code>{' '}
          or{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            aria-expanded
          </code>{' '}
          API — expanded state stays with the user agent (contract §5.3).
        </p>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  8. States                                                           */
/* ------------------------------------------------------------------ */

function StatesSection() {
  const [retries, setRetries] = useState(0);

  return (
    <section data-testid="states-fixtures">
      <SectionHeading id="comp-states">States</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Spinner, Loading, Empty, and Error affordances per DESIGN.md § Voice &amp;
        Content. V1.121 v0.4: the Empty headline uses the serif display tier
        (content voice) and ErrorState sits on the token-backed{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          error-surface
        </code>{' '}
        fill/border pair.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">Spinner</p>
      <MatrixCard className="mb-6">
        <div className="flex items-center gap-4" data-testid="states-spinner">
          <Spinner />
          <VariantLabel label="Spinner" />
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Loading</p>
      <MatrixCard className="mb-6">
        <div data-testid="states-loading">
          <LoadingState label="Loading data…" />
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Empty — serif display headline (content voice)
      </p>
      <MatrixCard className="mb-6">
        <div data-testid="states-empty">
          <EmptyState
            title="No works yet"
            description="Create a Work to start the local loop."
          />
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Error — error-surface tokens + retry action
      </p>
      <MatrixCard>
        <div data-testid="states-error">
          <ErrorState
            title="Could not load this view"
            description="The daemon returned an unexpected error."
            onRetry={() => setRetries((n) => n + 1)}
          />
        </div>
        {retries > 0 && (
          <p
            data-testid="states-error-retry-count"
            className="text-copy-13 text-gray-700 mt-4"
          >
            Retry requested {retries} {retries === 1 ? 'time' : 'times'}.
          </p>
        )}
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  9. Table                                                            */
/* ------------------------------------------------------------------ */

function TableSection() {
  const rows = [
    { id: 'work-01', title: 'The Lost City', profile: 'Novel', status: 'Active' },
    { id: 'work-02', title: 'Echo Protocol', profile: 'Script', status: 'Archived' },
    { id: 'work-03', title: 'Starfall', profile: 'Novel', status: 'Draft' },
    {
      id: 'work-04-very-long-correlation-id-for-overflow',
      title:
        'A deliberately long work title that exercises horizontal scroll inside the table wrapper without forcing document-wide overflow',
      profile: 'Novel with an extended profile label',
      status: 'Active',
    },
  ];

  return (
    <section data-testid="table-fixtures">
      <SectionHeading id="comp-table">Table</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Data table — header with label-12 font, body with copy-14, hover row
        highlighting, and a local{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">overflow-x-auto</code>{' '}
        scroll container. Wide rows scroll inside the matrix card; the page
        itself must not gain horizontal overflow.
      </p>
      <MatrixCard className="p-0 overflow-hidden" data-testid="table-overflow-wrapper">
        <div className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Title</TableHead>
                <TableHead>Profile</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="text-copy-13-mono text-gray-700 whitespace-nowrap">
                    {r.id}
                  </TableCell>
                  <TableCell className="min-w-[280px]">{r.title}</TableCell>
                  <TableCell className="whitespace-nowrap">{r.profile}</TableCell>
                  <TableCell>{r.status}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
        <p className="text-copy-13 text-gray-700 p-4">
          Row hover triggers background-200; header uses background-200 with
          bottom border gray-alpha-400. The last row carries long copy to prove
          local horizontal scroll.
        </p>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  10. Tabs                                                            */
/* ------------------------------------------------------------------ */

function TabsSection() {
  const [tab, setTab] = useState('tab1');

  return (
    <section data-testid="tabs-fixtures">
      <SectionHeading id="comp-tabs">Tabs</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        Controlled tab set with roving focus, automatic keyboard activation, and
        per-instance trigger/panel associations.{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">TabsTrigger</code>{' '}
        has no disabled prop — label that state unsupported rather than simulating it.
      </p>
      <p className="text-copy-13 text-gray-700 mb-6">
        Focus a tab, then use{' '}
        <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">←</kbd> /{' '}
        <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">→</kbd>,{' '}
        <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">Home</kbd>, or{' '}
        <kbd className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">End</kbd> to move
        selection.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">Controlled — hover, focus, keyboard</p>
      <MatrixCard className="mb-6" data-testid="tabs-controlled-fixture">
        <Tabs value={tab} onValueChange={setTab}>
          <TabsList>
            <TabsTrigger value="tab1">Overview</TabsTrigger>
            <TabsTrigger value="tab2">Details</TabsTrigger>
          </TabsList>
          <TabsContent value="tab1">
            <p className="text-copy-14 text-gray-900">
              Overview panel — pointer or keyboard selection. Controlled via{' '}
              <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">value</code> +{' '}
              <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">onValueChange</code>.
            </p>
          </TabsContent>
          <TabsContent value="tab2">
            <p className="text-copy-14 text-gray-900">
              Details panel — active trigger uses{' '}
              <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">shadow-card</code>;
              inactive triggers respond to real hover.
            </p>
          </TabsContent>
        </Tabs>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">
        Uncontrolled + duplicate values (two independent instances)
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-6">
          <Tabs defaultValue="shared">
            <TabsList>
              <TabsTrigger value="shared">Instance A</TabsTrigger>
              <TabsTrigger value="other">Other</TabsTrigger>
            </TabsList>
            <TabsContent value="shared">Instance A — shared value panel</TabsContent>
            <TabsContent value="other">Instance A — other panel</TabsContent>
          </Tabs>
          <Tabs defaultValue="shared">
            <TabsList>
              <TabsTrigger value="shared">Instance B</TabsTrigger>
              <TabsTrigger value="other">Other</TabsTrigger>
            </TabsList>
            <TabsContent value="shared">Instance B — shared value panel</TabsContent>
            <TabsContent value="other">Instance B — other panel</TabsContent>
          </Tabs>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  11. Textarea                                                        */
/* ------------------------------------------------------------------ */

function TextareaSection() {
  return (
    <section>
      <SectionHeading id="comp-textarea">Textarea</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Multi-line text input — min-height 96px, default, hover, focus-visible,
        disabled, and invalid states.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-4 max-w-lg">
          <MatrixRow>
            <VariantLabel label="default" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="textarea-default">Notes</Label>
              <Textarea id="textarea-default" placeholder="Default textarea…" />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="focus-visible" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="textarea-focus">Focus target</Label>
              <Textarea id="textarea-focus" data-testid="textarea-focus" defaultValue="Tab or click to focus" />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="hover" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="textarea-hover">Hover target</Label>
              <Textarea
                id="textarea-hover"
                data-testid="textarea-fixture-hover"
                defaultValue="Point at the field"
              />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="textarea-disabled">Locked draft</Label>
              <Textarea id="textarea-disabled" placeholder="Disabled textarea…" disabled />
            </div>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <div className="flex flex-col gap-1.5 flex-1">
              <Label htmlFor="textarea-invalid">Synopsis 概要</Label>
              <Textarea
                id="textarea-invalid"
                invalid
                aria-describedby="textarea-invalid-error"
                defaultValue="content with errors"
              />
              <p id="textarea-invalid-error" role="alert" className="text-copy-13 text-red-700">
                Synopsis must be at least 20 characters.
              </p>
            </div>
          </MatrixRow>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  12. Form Field (composition fixture)                                  */
/* ------------------------------------------------------------------ */

function FormFieldSection() {
  const [hasError, setHasError] = useState(false);
  const fieldId = 'ff-name';
  const helperId = `${fieldId}-helper`;
  const errorId = `${fieldId}-error`;

  return (
    <section>
      <SectionHeading id="comp-form-field">Form Field (composition)</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Composition fixture demonstrating the locked form-field contract:
        app-owned IDs,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">htmlFor</code>
        /<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">id</code>{' '}
        association,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">aria-describedby</code>{' '}
        wiring, required/optional indicators, and conditional error with{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">role=&quot;alert&quot;</code>.
        All IDs and copy are app-owned — the package controls are pure presentational.
      </p>

      {/* Standard composition with error toggle */}
      <p className="text-label-14 text-gray-900 mb-4">
        Standard composition — label, control, helper, error
      </p>
      <MatrixCard className="mb-6">
        <div className="flex flex-col gap-2 max-w-md">
          <Label htmlFor={fieldId}>Work title</Label>
          <Input
            id={fieldId}
            invalid={hasError}
            aria-describedby={hasError ? `${helperId} ${errorId}` : helperId}
            placeholder="Enter work title…"
            defaultValue="The Lost City"
          />
          <p id={helperId} className="text-copy-13 text-gray-700">
            Must be between 3 and 50 characters.
          </p>
          {hasError && (
            <p id={errorId} role="alert" className="text-copy-13 text-red-700">
              Name is required.
            </p>
          )}
        </div>

        <div className="mt-4">
          <Button
            variant="secondary"
            size="small"
            onClick={() => setHasError((v) => !v)}
          >
            {hasError ? 'Clear error' : 'Trigger error'}
          </Button>
        </div>
      </MatrixCard>

      {/* Required/optional + disabled */}
      <p className="text-label-14 text-gray-900 mb-4">
        Required, optional, and disabled variants
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-6 max-w-md">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ff-email">
              Email address <span className="text-red-700">*</span>
            </Label>
            <Input
              id="ff-email"
              required
              aria-describedby="ff-email-helper"
              placeholder="you@example.com"
            />
            <p id="ff-email-helper" className="text-copy-13 text-gray-700">
              We will never share your email.
            </p>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ff-bio">
              Bio <span className="text-gray-700 font-normal">(optional)</span>
            </Label>
            <Textarea
              id="ff-bio"
              disabled
              aria-describedby="ff-bio-helper"
              placeholder="Tell us about yourself…"
            />
            <p id="ff-bio-helper" className="text-copy-13 text-gray-700">
              This field is disabled in the current context.
            </p>
          </div>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  13. Toast                                                          */
/* ------------------------------------------------------------------ */

function ToastSection() {
  return (
    <section>
      <SectionHeading id="comp-toast">Toast</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Live notification renderer using promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        Toast primitives. Variants: success, error, warning, info. Use the
        fixture controls to queue toasts through the public{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">toast()</code>{' '}
        API and dismiss via the close button or{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">dismiss(id)</code>.
        Error toasts use{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          role=&quot;alert&quot;
        </code>
        . There is no action-CTA field or update operation on the public contract.
      </p>
      <ToastFixtures />
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  14. TransportErrorBlock                                            */
/* ------------------------------------------------------------------ */

function TransportErrorBlockSection() {
  return (
    <section>
      <SectionHeading id="comp-transport-error-block">Transport Error Block</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        primitive for transport-failure UX (V1.129 P1). V1.136 P2: primary and
        secondary CTAs are compact text links (ErrorState-aligned), not filled
        buttons. Renders the per-kind headline + body + CTA matrix for all six{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          TransportErrorKind
        </code>{' '}
        values, the callback-omission (toast) variant, and a caller-supplied
        detail line. Toggle the theme to verify light + dark contrast.
      </p>
      <TransportErrorBlockFixtures />
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  15. Compute Run Studio (V1.147 P1)                                   */
/* ------------------------------------------------------------------ */

function RunStudioSection() {
  return (
    <section>
      <SectionHeading id="comp-run-studio">Run Studio (Compute)</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        primitives for the V1.147 Compute Run Studio:{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">RunFormFields</code>,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">EntityPickerField</code>,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">ProposalSections</code>,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">RunStatusBadge</code>,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">RunsTable</code>.
        Schema-driven form variants (basic-combat pickers, kitchen sink,
        missing-schema empty state, picker selected/disabled/invalid),
        proposal inspector variants (success with all four parts, truncated,
        failed), and Runs history variants (all statuses, empty, long
        correlation id, read-only rows without Open Run). All copy is caller-owned literal English (studio is
        developer-auxiliary). Toggle the theme to verify light + dark.
      </p>
      <ComputeRunStudioFixtures />
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  16. Compute Timeline (V1.147 P2)                                     */
/* ------------------------------------------------------------------ */

function ComputeTimelineSection() {
  return (
    <section>
      <SectionHeading id="comp-compute-timeline">Compute Timeline</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Promoted{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        primitives for compute-as-a-Timeline-citizen (V1.147 P2):{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">ComputeResultNodeChrome</code>{' '}
        (Narrative node body) and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">ComputeInspectorSections</code>{' '}
        (node inspector content), composed with the shared{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-canvas/*</code>{' '}
        node shell. Variants: compute node alongside KB events (direct run /
        preset / summary-less / selected / dragging), inspector (direct /
        preset / sparse / read-only), Run Module entry chrome (toolbar + empty-state
        hint), and Brief-layer-unaffected evidence. All copy is caller-owned
        literal English (studio is developer-auxiliary). Toggle the theme to
        verify light + dark.
      </p>
      <TimelineComputeFixtures />
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function ComponentsPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">
        Components
      </h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        UI primitive matrices per IA guide §4.3 and DESIGN.md. Promoted
        primitives are imported via{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>
        ; transitional primitives remain on{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/*</code>{' '}
        until their promotion slice lands. Dialog remains transitional; Tabs is
        promoted. Every matrix cell renders a real component — hover,
        focus-visible, disabled, and loading states are live; toggle the theme
        to verify both light and dark (V1.121 states matrix).
      </p>
      <SurfaceSourceBadges importPaths={["@42ch/nexus-ui", "@web-ui/dialog", "@web-ui/states", "@web-ui/table"]} /> {/* transitional — badge path labels (not imports) */}
      <SubNav />

      <BadgeSection />
      <DomainBadgeSection />
      <ButtonSection />
      <CardSection />
      <DialogSection />
      <InputSection />
      <LabelSection />
      <SelectSection />
      <StatesSection />
      <TableSection />
      <TabsSection />
      <TextareaSection />
      <FormFieldSection />
      <ToastSection />
      <TransportErrorBlockSection />
      <RunStudioSection />
      <ComputeTimelineSection />

      <section id="comp-vi-acceptance" data-testid="comp-vi-acceptance" className="scroll-mt-sticky-header">
        <SectionHeading id="comp-vi-acceptance-heading">VI acceptance (P2)</SectionHeading>
        <p
          data-testid="comp-vi-acceptance-note"
          className="text-copy-16 text-gray-700 mb-6"
        >
          Theme-aware primary Button and TransportError Retry in light + dark shells.
          Toggle the theme to verify both shells.
        </p>
        <ViButtonAcceptanceFixtures />
        <ViTransportErrorAcceptanceFixtures />
      </section>

      <p className="text-copy-13 text-gray-700 mt-12 pt-8 border-t border-gray-alpha-200">
        17 promoted (Badge, Button, Card, Input, Label, Textarea, Select, Toast,
        TransportErrorBlock, Tabs, RunFormFields, EntityPickerField,
        ProposalSections, RunStatusBadge, RunsTable, ComputeResultNodeChrome,
        ComputeInspectorSections) + 3 transitional (Dialog,
        States, Table) rendered live via{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        (promoted) and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/*</code>{' '}
        (transitional). No transitional primitives are migrated, copied, or
        re-implemented in this gallery.
      </p>
    </div>
  );
}
