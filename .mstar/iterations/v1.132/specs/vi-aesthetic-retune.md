# Spec: VI aesthetic retune

**plan_id:** `2026-07-22-v1.132-p2-vi-aesthetic-retune`  
**Status:** plan locked (architect, 2026-07-22)  
**Wave:** 2

**Related documents**

- **Compass:** [delivery-compass.md](../delivery-compass.md) (AC-3..AC-5d)
- **Plan:** [2026-07-22-v1.132-p2-vi-aesthetic-retune.md](../../../plans/2026-07-22-v1.132-p2-vi-aesthetic-retune.md)
- **Studio policy:** [design-studio.md](../../../specs/design-studio.md)
- **Prior VI:** [logo-gallery-lockup.md](../../v1.131/specs/logo-gallery-lockup.md) (V1.131; P2 extends)

## Problem

Chronos VI feels out of control after recent ships: oversized titlebar/hero marks, competing Setup selection signals, neon cyan primary + deep ink on light shell (including TransportErrorBlock Retry), app-icon plate edge halo, and plate vs plain asset naming confusion.

## Goals

- Split plain marks vs `*-square` plates; inset icon compose
- Compact timeline mark (−30%–50% SSOT) vs wordmark / titlebar
- Setup agent selected state: one clear affordance
- Theme-aware primary Button: light shell ≠ neon cyan + deep ink; dark keeps strong cyan CTA
- Studio-first fixtures; DESIGN.md / tokens updated when values change
- VI dogfood ledger for further notes (Must/Should triage)

## User Value

Chronos VI reads as intentional across light and dark shells: compact timeline marks, a clear plain vs plated asset split, one obvious Setup selection affordance, and a theme-aware primary Button that no longer blasts neon cyan on deep ink in light mode. Reduces visual noise and restores brand confidence during dogfood.

## Non-Goals

- Runtime multi-theme switcher (Umbra/Aurora)
- Wholesale DESIGN.md rewrite beyond seeded + ledger Must items
- Marketing brand campaign

## Architecture decision (locked 2026-07-22)

### Boundaries and ownership

- `@42ch/nexus-ui` owns the theme-split `Button` SSOT and shared token values. Light-shell primary actions use the light treatment; dark-shell primary actions retain the strong cyan CTA. `TransportErrorBlock` consumes the Button and must not add a one-off Retry style.
- `apps/design-studio` owns visual proving fixtures and the Tokens/Brand gallery in both themes. `apps/web` consumes accepted primitives and composes product states; `apps/desktop` consumes the square asset for platform icon composition.
- Plain marks and plated marks are separate asset contracts: `logo-primary.svg` is the plain no-plate mark and plated lockups use the `*-square.svg` naming. The square source is composed with transparent inset margins for the Dock squircle; the compose layer owns the inset, not consumers.
- Compact timeline marks are a shared presentation scale decision applied consistently to Brand hero, titlebar, and app icon usage. Setup selection is a single selected-state affordance owned by the AgentPicker/card primitive.

### Failure modes and rollback

- If a light-shell primary still renders neon cyan on deep ink, fix the shared Button variant before touching error-block consumers; rollback is to the prior Button token mapping, not a local class.
- If a plate halo or crop appears in Dock/preview, revert the compose output and adjust the square asset's transparent inset; do not switch plain and square assets ad hoc.
- If Studio and App diverge, Studio remains the acceptance authority and App wiring is held until the primitive/token fixture matches. Untriaged VI notes remain in the ledger rather than being silently dropped.

## Wire

- Locked verdict: `wire_contracts_changed: false`; assets, tokens, and presentational primitives do not alter wire DTOs or daemon routes.

## Acceptance

Maps to compass AC-3, AC-4, AC-5, AC-5b, AC-5c, AC-5d.

### Success criteria (dogfood)

- Plain vs `*-square` asset split shipped; Studio Brand fixture matches.
- Timeline mark compact (−30%–50% SSOT scale); Brand hero + titlebar + app icon match AC-4.
- App icon inset compose; no light rectangular halo.
- Setup agent card has one clear selected affordance (no competing signals).
- Primary Button theme-aware: light shell ≠ neon cyan + deep ink (including TransportErrorBlock Retry); dark keeps strong cyan CTA.
- VI ledger populated with Must/Should triage; no silent drop of further notes.
- DESIGN.md / tokens updated when values change; Studio Tokens gallery reflects new tokens.
