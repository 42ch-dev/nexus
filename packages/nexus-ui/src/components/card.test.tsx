import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Card, CardHeader, CardTitle, CardDescription, CardContent } from './card';

describe('Card', () => {
  // --- base rendering ---


  it('renders children inside Card', () => {
    render(<Card><span data-testid="child">Hello</span></Card>);
    expect(screen.getByTestId('child')).toBeInTheDocument();
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  // --- interactive elevation recipe (V1.121 v0.4 — DESIGN.md §Elevation) ---



  it('keeps structural classes and merges className when interactive', () => {
    render(<Card interactive className="work-card" data-testid="card">Merge</Card>);
    const el = screen.getByTestId('card');
    expect(el).toHaveClass('work-card');
  });

  // --- CardTitle ---

  it('renders CardTitle as a heading element', () => {
    render(<CardTitle>Project Name</CardTitle>);
    expect(screen.getByText('Project Name').tagName).toBe('H3');
  });

  it('merges custom className on CardTitle with voice="content"', () => {
    render(<CardTitle voice="content" className="brand-title">Content Merge</CardTitle>);
    const el = screen.getByText('Content Merge');
    expect(el).toHaveClass('brand-title');
  });

  // --- CardDescription ---


  // --- CardContent ---


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

  // --- ref-as-prop for Card ---

  it('passes the ref on Card', () => {
    let ref: HTMLDivElement | null = null;
    const setRef = (el: HTMLDivElement | null) => {
      ref = el;
    };
    render(<Card ref={setRef}>Ref</Card>);
    expect(ref).not.toBeNull();
    expect(ref!).toHaveProperty('tagName', 'DIV');
  });
});
