# ADR: Redirect stub — Local API surface conventions renamed to Daemon API

Status: implemented — the rename shipped; this record is a migration archive stub
Archived: 2026-07-05 — frozen redirect note; it carries no decision of its own
Date: 2026-07-05
Scope: architecture — redirect stub
Source: migrated from the retired tracked specs corpus

## Problem

Historical iteration compasses and plan files still reference the retired
`local-api-surface-conventions.md` filename. V1.90 renamed the surface from
"Local API" to "Daemon API" and the path prefix from `/v1/local/` to
`/v1/daemon/`; without a redirect those links break.

## Decision

This record is a **migration archive stub, not a decision record**: it records
the rename and redirects readers, and deliberately carries no proposal,
alternatives, or consequences of its own. The surviving normative contract is
[`daemon-api-surface-conventions.md`](../../../../docs/specs/runtime/daemon-api-surface-conventions.md);
all of its conventions continue to apply unchanged to the renamed surface.

## Alternatives considered

None recorded — the stub does not encode a decision of its own, so no
alternatives were weighed here. The decision to rename the surface is recorded
in the live Daemon API conventions and its Git history.

## Consequences

New content links to `daemon-api-surface-conventions.md`; this stub exists only
to keep historical plan and iteration links resolving. It is frozen and is not
current authority.
