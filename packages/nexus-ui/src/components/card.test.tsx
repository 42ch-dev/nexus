import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Card, CardHeader, CardTitle, CardDescription, CardContent } from './card';

describe('Card', () => {
  // --- base rendering ---

  it('renders the Card with structural classes', () => {
    render(<Card data-testid="card">Content</Card>);
    const el = screen.getByTestId('card');
    expect(el).toHaveClass('rounded-card');
    expect(el).toHaveClass('border');
    expect(el).toHaveClass('bg-background-100');
    expect(el).toHaveClass('p-6');
    expect(el).toHaveClass('text-gray-1000');
    expect(el).toHaveClass('shadow-card');
  });

  it('renders children inside Card', () => {
    render(<Card><span data-testid="child">Hello</span></Card>);
    expect(screen.getByTestId('child')).toBeInTheDocument();
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  // --- interactive elevation recipe (V1.121 v0.4 — DESIGN.md §Elevation) ---

  it('rests on shadow-card (elevation-1 alias) without hover recipe by default', () => {
    render(<Card data-testid="card">Static</Card>);
    const el = screen.getByTestId('card');
    expect(el).toHaveClass('shadow-card');
    // No interactive recipe unless opted in — existing call sites unchanged.
    expect(el).not.toHaveClass('hover:shadow-elevation-2');
    expect(el.className).not.toContain('motion-safe:hover:-translate-y-px');
    expect(el.className).not.toContain('duration-popover');
  });

  it('applies the v0.4 interactive elevation recipe when interactive', () => {
    render(<Card interactive data-testid="card">Work card</Card>);
    const el = screen.getByTestId('card');
    // Rest stays elevation-1 (shadow-card alias); hover lifts to elevation-2.
    expect(el).toHaveClass('shadow-card');
    expect(el).toHaveClass('hover:shadow-elevation-2');
    // Hover lift + pressed settle (transform removed on active).
    expect(el.className).toContain('motion-safe:hover:-translate-y-px');
    expect(el).toHaveClass('active:shadow-elevation-1');
    expect(el.className).toContain('motion-safe:active:translate-y-0');
    // 160ms ease-standard via the duration-popover token.
    expect(el).toHaveClass('duration-popover');
    expect(el).toHaveClass('ease-standard');
    // Reduced-motion safe: transitions drop to instant under motion-reduce.
    expect(el).toHaveClass('motion-reduce:transition-none');
  });

  it('keeps structural classes and merges className when interactive', () => {
    render(<Card interactive className="work-card" data-testid="card">Merge</Card>);
    const el = screen.getByTestId('card');
    expect(el).toHaveClass('rounded-card');
    expect(el).toHaveClass('bg-background-100');
    expect(el).toHaveClass('work-card');
    expect(el).toHaveClass('hover:shadow-elevation-2');
  });

  // --- CardHeader ---

  it('renders CardHeader with structural classes', () => {
    render(<CardHeader data-testid="header">Title Area</CardHeader>);
    const el = screen.getByTestId('header');
    expect(el).toHaveClass('flex');
    expect(el).toHaveClass('flex-col');
    expect(el).toHaveClass('space-y-1.5');
    expect(el).toHaveClass('pb-4');
  });

  // --- CardTitle ---

  it('renders CardTitle as an <h3> with heading classes', () => {
    render(<CardTitle>Project Name</CardTitle>);
    const el = screen.getByText('Project Name');
    expect(el.tagName).toBe('H3');
    expect(el).toHaveClass('text-heading-16');
    expect(el).toHaveClass('font-heading');
    expect(el).toHaveClass('leading-tight');
    expect(el).toHaveClass('tracking-tight');
  });

  // --- CardTitle voice (V1.121 v0.4 — DESIGN.md components.card.title.voice) ---

  it('defaults to the interface voice (sans heading) when voice is omitted', () => {
    render(<CardTitle>Interface Default</CardTitle>);
    const el = screen.getByText('Interface Default');
    expect(el).toHaveClass('text-heading-16');
    expect(el).toHaveClass('font-heading');
    expect(el).not.toHaveClass('font-display');
    expect(el).not.toHaveClass('text-display-20');
  });

  it('pins the exact default-voice class list when voice is omitted (QC2-W-003)', () => {
    // Exact-string pin: any regression to the default treatment (e.g. the
    // content voice leaking into existing call sites) fails here.
    render(<CardTitle>Title</CardTitle>);
    const el = screen.getByText('Title');
    expect(el.className).toBe('text-heading-16 font-heading leading-tight tracking-tight');
  });

  it('renders the sans interface treatment when voice="interface" is explicit', () => {
    render(<CardTitle voice="interface">Interface Explicit</CardTitle>);
    const el = screen.getByText('Interface Explicit');
    expect(el).toHaveClass('text-heading-16');
    expect(el).toHaveClass('font-heading');
    expect(el).toHaveClass('leading-tight');
    expect(el).not.toHaveClass('font-display');
  });

  it('swaps to the serif display tier when voice="content"', () => {
    render(<CardTitle voice="content">Work Title</CardTitle>);
    const el = screen.getByText('Work Title');
    expect(el.tagName).toBe('H3');
    expect(el).toHaveClass('font-display');
    expect(el).toHaveClass('text-display-20');
    expect(el).toHaveClass('tracking-tight');
    // Interface treatment is fully replaced (no sans leftovers).
    expect(el).not.toHaveClass('text-heading-16');
    expect(el).not.toHaveClass('font-heading');
  });

  it('merges custom className on CardTitle with voice="content"', () => {
    render(<CardTitle voice="content" className="brand-title">Content Merge</CardTitle>);
    const el = screen.getByText('Content Merge');
    expect(el).toHaveClass('font-display');
    expect(el).toHaveClass('brand-title');
  });

  // --- CardDescription ---

  it('renders CardDescription as a <p> with copy classes', () => {
    render(<CardDescription>Optional detail text</CardDescription>);
    const el = screen.getByText('Optional detail text');
    expect(el.tagName).toBe('P');
    expect(el).toHaveClass('text-copy-14');
    expect(el).toHaveClass('text-gray-900');
  });

  // --- CardContent ---

  it('renders CardContent with copy classes', () => {
    render(<CardContent data-testid="content">Body</CardContent>);
    const el = screen.getByTestId('content');
    expect(el).toHaveClass('text-copy-14');
  });

  // --- composition (full card) ---

  it('composes all sub-primitives correctly', () => {
    render(
      <Card>
        <CardHeader>
          <CardTitle>Work Title</CardTitle>
          <CardDescription>Last edited 3 days ago</CardDescription>
        </CardHeader>
        <CardContent>
          <p>Description text here.</p>
        </CardContent>
      </Card>,
    );

    expect(screen.getByText('Work Title')).toBeInTheDocument();
    expect(screen.getByText('Last edited 3 days ago')).toBeInTheDocument();
    expect(screen.getByText('Description text here.')).toBeInTheDocument();
  });

  // --- className merge (cn integration) for each sub-primitive ---

  it('merges custom className on Card', () => {
    render(<Card className="extra-card" data-testid="merge-card">CardMerge</Card>);
    expect(screen.getByTestId('merge-card')).toHaveClass('extra-card');
  });

  it('merges custom className on CardHeader', () => {
    render(<CardHeader className="compact-header" data-testid="merge-hdr">HdrMerge</CardHeader>);
    expect(screen.getByTestId('merge-hdr')).toHaveClass('compact-header');
  });

  it('merges custom className on CardTitle', () => {
    render(<CardTitle className="brand-title">TitleMerge</CardTitle>);
    expect(screen.getByText('TitleMerge')).toHaveClass('brand-title');
  });

  it('merges custom className on CardDescription', () => {
    render(<CardDescription className="muted">DescMerge</CardDescription>);
    expect(screen.getByText('DescMerge')).toHaveClass('muted');
  });

  it('merges custom className on CardContent', () => {
    render(<CardContent className="prose" data-testid="merge-cc">ContentMerge</CardContent>);
    expect(screen.getByTestId('merge-cc')).toHaveClass('prose');
  });

  // --- forwardRef for Card ---

  it('forwards the ref on Card', () => {
    let ref: HTMLDivElement | null = null;
    const setRef = (el: HTMLDivElement | null) => {
      ref = el;
    };
    render(<Card ref={setRef}>Ref</Card>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'DIV');
  });
});
