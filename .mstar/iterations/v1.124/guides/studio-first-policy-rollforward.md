# Studio-first policy roll-forward — Tokens need a gallery

**Iteration:** V1.124  
**Audience:** implementers, QC, future PMs landing `--color-*` tokens  
**Authority:** root `AGENTS.md` § UI Component Policy → **"Tokens need a gallery"**; recurrence skeleton in `specs/tokens-gallery-audit.md` §6

## Rule

Any PR that adds a gallery-projected `--color-*` token to `tooling/design-tokens/src/tokens.css` **must**, in the **same PR**, register that token in `apps/design-studio/src/pages/tokens.tsx` (Tokens page swatch + group). Light and dark values must both resolve (theme toggle or equivalent test). If gallery registration cannot ship in the same change, file a `residual_findings` row (severity ≥ medium) naming the token and the Tokens page path — do not merge silent CSS-only tokens.

## Why this note exists

P1 of V1.124 was the **catch-up sweep** for nine Timeline / Layer / Outline-timeline-pin / Soul-viz-timeline tokens landed in V1.122/V1.123 without Studio gallery rows. That debt is closed for those families. Future iterations must **not** re-accumulate the same gap: a token that exists in CSS but is invisible in Studio is a **defect**, not backlog flavor. QC and PR descriptions can fail a change against this gate and the four bullets in `tokens-gallery-audit.md` §6.
