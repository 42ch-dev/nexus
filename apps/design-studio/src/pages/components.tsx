import { useState, type ReactNode } from 'react';

import { cn } from '@web-lib/utils';

import { Badge, Button, Card, CardHeader, CardTitle, CardDescription, CardContent } from '@42ch/nexus-ui';
import { Dialog, DialogTrigger, DialogContent } from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)
import { Input } from '@web-ui/input'; // transitional — deferred to Form Field slice (form-field contract incomplete per Grill-Me lock)
import { Label } from '@web-ui/label'; // transitional — deferred to Form Field slice (label/control/helper/error composition)
import { Select } from '@web-ui/select'; // transitional — keep-web (native select wrapper; no cross-app demand proven yet)
import { Spinner, LoadingState, EmptyState, ErrorState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@web-ui/table'; // transitional — keep-web (responsive overflow wrapper; not in V1.99 first batch)
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@web-ui/tabs'; // transitional — keep-web (compound component owns selection state; not purely presentational)
import { Textarea } from '@web-ui/textarea'; // transitional — deferred to Form Field slice (form-field contract incomplete per Grill-Me lock)

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

function MatrixCard({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        'border border-gray-alpha-300 rounded-card bg-background-100 p-6',
        className,
      )}
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
    { label: 'Input', href: '#comp-input' },
    { label: 'Label', href: '#comp-label' },
    { label: 'Select', href: '#comp-select' },
    { label: 'States', href: '#comp-states' },
    { label: 'Table', href: '#comp-table' },
    { label: 'Tabs', href: '#comp-tabs' },
    { label: 'Textarea', href: '#comp-textarea' },
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
    <section>
      <SectionHeading id="comp-badge">Badge</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Status pill — 6 semantic variants per DESIGN.md § Badge.
      </p>
      <MatrixCard>
        <div className="flex flex-wrap items-center gap-4">
          {variants.map(({ variant, label }) => (
            <div key={variant} className="flex flex-col items-center gap-2">
              <Badge variant={variant}>{label}</Badge>
              <VariantLabel label={label} />
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

      <p className="text-label-14 text-gray-900 mb-4">
        Variant × Size matrix
      </p>
      <MatrixCard className="mb-6">
        <div className="space-y-4">
          {variants.map(({ variant }) => (
            <div key={variant} className="flex flex-wrap items-center gap-4">
              <VariantLabel label={variant} />
              {sizes.map(({ size, label }) => (
                <Button key={size} variant={variant} size={size}>
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
    <section>
      <SectionHeading id="comp-card">Card</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Default card with header, title, description, and content slots.
      </p>
      <MatrixCard>
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>Card title</CardTitle>
            <CardDescription>
              Card description — secondary text below the title.
            </CardDescription>
          </CardHeader>
          <CardContent>
            Body content goes here. Cards use background-100 fill,
            gray-alpha-400 border, 24px padding, and an optional shadow.
          </CardContent>
        </Card>
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
    <section>
      <SectionHeading id="comp-dialog">Dialog</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Modal dialog — built on Radix for focus trap, escape close, and ARIA.
        Click the trigger to open.
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

function SelectSection() {
  const options = ['Option A', 'Option B', 'Option C'];

  return (
    <section>
      <SectionHeading id="comp-select">Select</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Native styled select — default, disabled, and invalid states. Uses the native
        element for accessibility; DESIGN.md control styling applied.
      </p>
      <MatrixCard>
        <div className="flex flex-col gap-4 max-w-md">
          <MatrixRow>
            <VariantLabel label="default" />
            <Select className="flex-1">
              {options.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </Select>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="disabled" />
            <Select disabled className="flex-1">
              {options.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </Select>
          </MatrixRow>
          <MatrixRow>
            <VariantLabel label="invalid" />
            <Select invalid className="flex-1">
              {options.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </Select>
          </MatrixRow>
        </div>
      </MatrixCard>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  8. States                                                           */
/* ------------------------------------------------------------------ */

function StatesSection() {
  return (
    <section>
      <SectionHeading id="comp-states">States</SectionHeading>
      <p className="text-copy-16 text-gray-700 mb-6">
        Spinner, Loading, Empty, and Error affordances per DESIGN.md § Voice &amp; Content.
      </p>

      <p className="text-label-14 text-gray-900 mb-4">Spinner</p>
      <MatrixCard className="mb-6">
        <div className="flex items-center gap-4">
          <Spinner />
          <VariantLabel label="Spinner" />
        </div>
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Loading</p>
      <MatrixCard className="mb-6">
        <LoadingState label="Loading data…" />
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Empty</p>
      <MatrixCard className="mb-6">
        <EmptyState
          title="No works yet"
          description="Create a Work to start the local loop."
        />
      </MatrixCard>

      <p className="text-label-14 text-gray-900 mb-4">Error</p>
      <MatrixCard>
        <ErrorState
          title="Could not load this view"
          description="The daemon returned an unexpected error."
        />
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
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function ComponentsPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">
        Components
      </h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        All 11 UI primitive modules from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          apps/web/src/components/ui
        </code>{' '}
        — live variant/state matrices per IA guide §4.3 and DESIGN.md.         Every
        component is imported via <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        (promoted) or{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/*</code>{' '}
        (transitional);
        interactive controls (Dialog, Tabs) are functional.
      </p>
      <SubNav />

      <BadgeSection />
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

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        11/11 primitive modules from the <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">apps/web/src/components/ui</code> barrel — all rendered
        live via <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@42ch/nexus-ui</code>{' '}
        (promoted) and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">@web-ui/*</code>{' '}
        (transitional).
        No primitives are migrated, copied, or re-implemented in this gallery.
      </p>
    </div>
  );
}
