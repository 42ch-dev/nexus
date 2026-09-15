# nexus-provider-ports

Port-only provider contracts. Imports only `nexus-contracts`, `async-trait`, `futures-core`, and std.

- Owns the `ProviderPort` trait and `ProviderResult` alias.
- No Host implementation, DB, SQL, napi, SDK, or orchestration imports.
