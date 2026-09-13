# nexus-service (reserved)

Standalone Node service shell. Implementation is owned by the downstream
service task; P2-T1 only reserved the workspace metadata.

- `dev` / `start` point at the downstream-owned entrypoint `src/main.mjs`.
- `proof:open-smoke` runs the P2-T1 binding smoke (`src/open-smoke.mjs`) and is
  not a product entrypoint.
