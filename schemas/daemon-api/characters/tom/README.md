# Daemon API — Character ToM schemas

JSON Schemas for the Character ToM (L1/L2 belief) endpoints under `/v1/daemon/characters/{character_id}/tom`. Cross-language contracts consumed by generated TypeScript and Rust clients.

v1.184 P4 Task 2 — generated record/query DTOs over the existing l5 KnowledgeEntry/MindState carriers. The viewer Character's private or selected-binding-owned KnowledgeEntry is the carrier; `modules.belief[*].holder` is the epistemic subject (`chr_*`). Record CAS-patches the authoritative belief array and appends a derivative MindState atomically; query returns a bounded keyset page ordered `(order, carrier_entry_id, row_ordinal)`. Neither path calls a provider. The server resolves the owner from the active-Creator config; request bodies never carry `owner_creator_id`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/characters/{character_id}/tom` | `record-character-tom-request.schema.json`, `record-character-tom-response.schema.json` |
| `GET /v1/daemon/characters/{character_id}/tom` | `list-character-tom-query.schema.json`, `list-character-tom-response.schema.json`, `tom-belief-item.schema.json` |
