# nexus-cloud-domain — User + Pairing (Cloud Sync Domain)

Cloud sync domain logic: User and Pairing aggregates. Provides the domain
layer for cloud synchronization (user identity, device pairing).

## Key Rules

- **Contracts-first**: all public types come from `nexus-contracts`. Do not
  hand-write duplicate User or Pairing DTOs.
- This crate contains **domain logic only** — no HTTP transport.
  Transport lives in `nexus-cloud-sync`.

## Dependencies

- `nexus-contracts` (generated types)
