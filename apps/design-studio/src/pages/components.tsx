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
} from '@42ch/nexus-ui';
import { Dialog, DialogTrigger, DialogContent } from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)
import { Spinner, LoadingState, EmptyState, ErrorState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
import { ToastFixtures } from '@/fixtures/toast-fixtures';
import { TransportErrorBlockFixtures } from '@/fixtures/transport-error-block';
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
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@web-ui/tabs'; // transitional — keep-web (compound component owns selection state; not purely presentational)

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
      className="text-heading-20 font-semibold text-gray-1000 mb-4 pt-8 scroll-mt-16"
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
    <span className="text-copy-13 text-gray-600 font-medium shrink-0 min-w-[80px]">
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
    { size: 'small' as const, label: 'small (h-8)' },
    { size: 'default' as const, label: 'default (h-10)' },
    { size: 'large' as const, label: 'large (h-12)' },
  ];

  return (
    <section>
      <SectionHeading id="comp-button">Button</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-2">
        4 variants × 3 sizes = 12 combinations, plus disabled and focus-visible
        states per DESIGN.md § Button.
      </p>
      <p
        data-testid="button-chronos-note"
        className="text-copy-14 text-gray-700 mb-4 max-w-prose"
      >
        Chronos primary is theme-split: light shell uses{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">bg-brand-deep-blue</code>{' '}
        +{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">text-brand-white</code>;
        dark shell keeps{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">bg-brand-cyan</code> +{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          text-brand-deep-blue
        </code>
        . Toggle the theme to confirm both shells.
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
        <p className="text-copy-13 text-gray-500 mt-4">
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
                  Serif display-20 — reserved for creative-entity cards
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
        Modal dialog — built on Radix for focus trap, escape close, and ARIA.
        The overlay uses the{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">bg-scrim</code>{' '}
        token and the panel{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">shadow-elevation-4</code>{' '}
        (V1.121 scrim convergence). Click the trigger to open.
      </p>
      <MatrixCard>
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button variant="primary">Open dialog</Button>
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
        <p className="text-copy-13 text-gray-500 mt-4">
          Uses Radix <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">Dialog.Portal</code> for
          body-level overlay and scroll lock.
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
        Text input — default, disabled, and invalid states.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-4 max-w-md">
          <MatrixRow>
            <VariantLabel label="default" />
            <Input placeholder="Default input..." className="flex-1" />
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <Input placeholder="Disabled input..." disabled className="flex-1" />
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <Input placeholder="Invalid input..." invalid className="flex-1" defaultValue="bad value" />
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
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-3 max-w-md">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="demo-input">Field label</Label>
            <Input id="demo-input" placeholder="Click the label to focus this input" />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label>Disabled label (visual only)</Label>
            <Input placeholder="Disabled input" disabled />
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
      <p className="text-copy-13 text-gray-500 mb-6">
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
            <Select
              disabled
              data-testid="select-fixture-disabled"
              className="flex-1"
              defaultValue="Option A"
              aria-label="Disabled select fixture"
            >
              <SelectOptionList options={options} />
            </Select>
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
        <p className="text-copy-13 text-gray-500 mt-4">
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
        <p className="text-copy-13 text-gray-500 mt-4">
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
            className="text-copy-13 text-gray-500 mt-4"
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
  ];

  return (
    <section>
      <SectionHeading id="comp-table">Table</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Data table — header with label-12 font, body with copy-14, hover row
        highlighting, and overflow-x scroll container.
      </p>
      <MatrixCard className="p-0">
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
                <TableCell className="text-copy-13-mono text-gray-700">
                  {r.id}
                </TableCell>
                <TableCell>{r.title}</TableCell>
                <TableCell>{r.profile}</TableCell>
                <TableCell>{r.status}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <p className="text-copy-13 text-gray-500 p-4">
          Row hover triggers background-200; header uses background-200 with
          bottom border gray-alpha-400.
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
    <section>
      <SectionHeading id="comp-tabs">Tabs</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Tab set — interactive, two panels. Added to barrel by P0 T1 (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">55dd06cc</code>
        ). Active tab uses background-100 + shadow-card; inactive tabs are
        hover-responsive.
      </p>
      <MatrixCard>
        <Tabs value={tab} onValueChange={setTab}>
          <TabsList>
            <TabsTrigger value="tab1">Overview</TabsTrigger>
            <TabsTrigger value="tab2">Details</TabsTrigger>
          </TabsList>
          <TabsContent value="tab1">
            <p className="text-copy-14 text-gray-900">
              Overview panel — click Details to switch tabs. The Tabs component
              supports both controlled (<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">value</code>{' '}
              + <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">onValueChange</code>) and
              uncontrolled (<code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">defaultValue</code>) modes.
            </p>
          </TabsContent>
          <TabsContent value="tab2">
            <p className="text-copy-14 text-gray-900">
              Details panel — the active tab trigger has a raised card
              appearance with <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">shadow-card</code>.
              Hovering an inactive trigger applies a subtle gray-alpha-100
              background.
            </p>
          </TabsContent>
        </Tabs>
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
        Multi-line text input — min-height 96px, default, disabled, and invalid
        states.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-4 max-w-lg">
          <MatrixRow>
            <VariantLabel label="default" />
            <Textarea placeholder="Default textarea…" className="flex-1" />
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <Textarea
              placeholder="Disabled textarea…"
              disabled
              className="flex-1"
            />
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <Textarea
              placeholder="Invalid textarea…"
              invalid
              className="flex-1"
              defaultValue="content with errors"
            />
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
            aria-describedby={`${helperId} ${errorId}`}
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
        Toast primitives. Variants: success, error, warning, info. Each toast
        shows a title and optional description; error toasts use{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          role=&quot;alert&quot;
        </code>
        .
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
        primitive for transport-failure UX (V1.129 P1). Renders the per-kind
        headline + body + CTA matrix for all six{' '}
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
        until their promotion slice lands. Interactive controls (Dialog, Tabs)
        are functional. Every matrix cell renders a real component — hover,
        focus-visible, disabled, and loading states are live; toggle the theme
        to verify both light and dark (V1.121 states matrix).
      </p>
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

      <section id="comp-vi-acceptance" data-testid="comp-vi-acceptance" className="scroll-mt-16">
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

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        9 promoted (Badge, Button, Card, Input, Label, Textarea, Select, Toast,
        TransportErrorBlock) + 4 transitional (Dialog, States, Table, Tabs)
        rendered live via{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        (promoted) and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/*</code>{' '}
        (transitional). No transitional primitives are migrated, copied, or
        re-implemented in this gallery.
      </p>
    </div>
  );
}
