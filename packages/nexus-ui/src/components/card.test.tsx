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
