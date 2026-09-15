# nexus-core-node

Thin Node-API cdylib composing `nexus-core` and provider ports. Unsafe confined to audited FFI boundary.

- Pins: napi 3.12.4 / napi-derive 3.6.5 / napi-build 2.4.2
- One process-level Tokio runtime; environment-local instance state via `Env::set_instance_data`.
