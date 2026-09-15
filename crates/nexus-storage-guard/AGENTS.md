# nexus-storage-guard

Tiny audited SQLite FFI boundary. Installs the five connection-local writer
protocol scalar functions via `sqlite3_create_function_v2` under SQLx's locked
handle.

- Only `src/sqlite.rs` may contain documented `unsafe` code.
- No business SQL, HTTP, JS, or independent connection pools.
- Domain crates keep `unsafe_code = forbid` and depend on this crate for UDF install.
