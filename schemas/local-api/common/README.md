# Local API — Common Shared Envelopes

Cross-resource shared envelopes served under `/v1/local/*`. These are
cross-language contracts consumed by future WebApp/Web-UI clients (and the
`@42ch/nexus-contracts` npm package).

V1.64 (F-E1) — introduced for the canonical Local API error envelope.

## Schemas

| Schema | Purpose |
|--------|---------|
| `error-response.schema.json` | Canonical `ErrorResponse` detail (`{ code, message, details? }`). The daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire. All Local API failure paths converge on this shape. |

## Pagination

Cursor pagination metadata (`PaginationInfo`: `limit`, `next_cursor`, `has_more`)
currently lives at [`../kb/pagination-info.schema.json`](../kb/pagination-info.schema.json)
and is `$ref`-shared by all cursor-paginated Local API list responses. Promoting
it into `common/` is a tracked future cleanup (coordinate with F-P3 array-rename
sweep).

## Related

- **Convention spec:** `.mstar/knowledge/specs/local-api-surface-conventions.md` (§3 error envelope, §2 pagination)
- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Layout spec:** `.mstar/knowledge/specs/schemas-directory-layout.md`
