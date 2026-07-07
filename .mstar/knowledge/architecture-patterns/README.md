# Architecture Patterns (compound output)

Distilled reusable patterns captured by `mstar-compound` at iteration-close. Parent rules: [`../AGENTS.md`](../AGENTS.md).

| Document | Source iteration | Pattern |
| --- | --- | --- |
| [resolved-residual-verification.md](resolved-residual-verification.md) | V1.86 | `lifecycle: resolved` is a claim, not a guarantee — verify the class on current `main`; 2 of 5 V1.86 same-class "resolved" residuals were insufficient |
| [bounded-drain-completion-contract.md](bounded-drain-completion-contract.md) | V1.80 | `has_more` must reflect queue advancement, not rows attempted |
| [contracts-gap-on-shipped-backend.md](contracts-gap-on-shipped-backend.md) | V1.78 | shipping a backend before its wire contracts → normalize hand-written DTOs after |
| [fingerprint-cached-live-aggregate.md](fingerprint-cached-live-aggregate.md) | V1.81/V1.82 | fingerprint-gated recompute of a live aggregate; threshold-saturated response field |
| [on-demand-synthesis-read-path-invariant.md](on-demand-synthesis-read-path-invariant.md) | V1.81 | LLM synthesis gated behind `force_regenerate`; read path never triggers compute |
| [pagination-cursor-without-total-count-labels.md](pagination-cursor-without-total-count-labels.md) | V1.79 | cursor pagination without a total count — label discipline |
| [canvas-surface-implementation-pattern.md](canvas-surface-implementation-pattern.md) | V1.7x | React Flow canvas surface + structured write-boundary |
| [nexus-brand-token-hierarchy.md](nexus-brand-token-hierarchy.md) | V1.83/V1.84/V1.94 | root DESIGN.md brand SSOT → app consumption mappings; V1.94 corrects the dark-mode button contrast rule (white-on-cyan, not deep-blue-on-cyan) |
| [daemon-ready-gate-pattern.md](daemon-ready-gate-pattern.md) | V1.94 | single source of truth (SidecarManager) + multiple observers (setup wizard step 2 + per-launch splash + crash banner) of `onDaemonStatusChanged`; avoid `is_daemon_ready()` commands |
| [local-environment-scan-safety-boundary.md](local-environment-scan-safety-boundary.md) | V1.94 | five normative constraints for any "scan local environment for installed tools" feature (registry-known names only; bounded concurrency; ≤2s timeout; no shell expansion; no user-supplied commands during scan) |
