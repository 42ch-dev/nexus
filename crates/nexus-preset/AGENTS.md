# nexus-preset

Pure preset authoring and source-file domain library, consumed by nexus-core and nexus-orchestration.

- Own grammar re-exports, parser/limits, validation, capability input metadata, directory discovery, embedded assets, preset IDs, and source identity.
- Never depend on execution, graph-flow, tasks, Host, WASM, HTTP, or a database.
- CapabilityCatalog exposes metadata only. Missing capabilities remain validation errors.
- Builtin input schemas are shared with orchestration capability implementations; never duplicate them.
- Preserve PresetSourceIdentity serialization and content hashes over the manifest and referenced assets.
- Runtime graph builders and AgentBinding creation remain in nexus-orchestration::preset_runtime.
