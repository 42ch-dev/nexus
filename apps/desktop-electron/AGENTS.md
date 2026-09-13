# desktop-electron (reserved)

Electron feasibility shell. Implementation is owned by the downstream desktop
task; P2-T1 only reserved the workspace metadata.

- `package` points at the downstream-owned entrypoint `scripts/package.mjs`.
- `proof:native-smoke` runs the P2-T1 compatibility smoke
  (`src/native-smoke.mjs`) and is not a product entrypoint.
