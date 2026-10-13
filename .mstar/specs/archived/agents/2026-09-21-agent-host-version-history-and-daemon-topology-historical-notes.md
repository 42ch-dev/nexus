# Agent Host — Version History and Daemon-Topology Historical Notes

**Provenance**: frozen from [`specs/active/agents/agent-host.md`](../../active/agents/agent-host.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 deleted the `nexus-daemon-runtime` host; the live spec's own notes marked the topology/mapping blocks historical as of that cutover. (The `V1.186`/`V1.188`/`v1.190` heading annotations removed in the same pass were version labels on still-current contracts, not separate supersessions.)
**Freeze date**: 2026-10-13

---

## Former "Former filename" row (§0 attribute table)

"| **Former filename** | `agent-host-architecture.md` (renamed 2026-05-20, spec hygiene — `-v1` suffix dropped; same document, git history is the record) |"

## Dropped provenance clauses (verbatim)

- §1 Purpose: "(historically the daemon runtime, retired in v1.193 P2)"
- §2 Frozen decisions: "hosting agents does not make the local host (the pre-v1.193 "daemon runtime") an ACP Agent advertised on Registry" — the parenthetical was removed.
- §3 Topology diagram comment: "the\n       │    historical host-side layering record is [daemon-runtime.md §2](../../archived/runtime/2026-09-21-daemon-runtime.md)" — removed from the diagram (the `daemon-runtime.md` archive record itself remains).
- §4 Provider model: "**Current runtime wiring (V1.188):** daemon boot loads …" — version tag dropped; "daemon boot" reworded to "service boot".
- §4: "**Shipped orchestration path (V1.186, retained in V1.188):**" — version tags dropped.
- §4 Runtime rule: "(The rule originally bound the `nexus-daemon-runtime` crate, deleted in v1.193 P2; the boundary itself is retained.)"
- §5 item 4: "**dsh message-level real time (V1.188).**" — version tag dropped.
- §7: "**V1.188 readiness contract:**" — version tag dropped.
- §8 heading: "## 8. dsh SDK runtime and retained cleanup (V1.188)" — version tag and "retained cleanup" framing dropped; "V1.188 cuts over to the SDK 0.2 runtime contract" reworded to the present-tense contract; "the retired helper" wording reworded.
- Appendix §2 crate boundary: "(Pre-v1.193 P2 this edge was `nexus-daemon-runtime` → `nexus-agent-host`; the crate is deleted.)"
- §6.5 heading tag "(v1.190 P2)"; §6.6 heading tag "(v1.190 P4)" — version tags dropped.
- §14 intro: "These decisions were made during V1.18 PM review and remain authoritative." — provenance clause dropped; the decisions remain authoritative.
- R-006: "`ToolRiskClassifier` trait created in Wave 1 with `StaticToolRiskClassifier`. `AutoToolRiskClassifier` implemented in V1.19 (D-005) borrowing OpenDesign regex approach." — Wave-1/V1.19 provenance dropped.
- §11.3 table row: "(pre-v1.193 mapping: `nexus-daemon-runtime` + `HostManager`)"; §11.4 diagram line "pre-v1.193 P2 mapping: nexus-daemon-runtime, deleted" — old mappings dropped; current mappings already stood in the same cells.
- §11.2 table row: "Tool execution via host mediation | `HostToolExecutor` trait (V1.20 spec §4.4)" — the `HostToolExecutor` mapping referred to the deleted daemon surface; current mapping is `nexus-core` `execution/capabilities.rs`.

## §3 historical host note (removed blockquote)

> **Historical host note (v1.193 P2):** the pre-retirement topology ran this tree under the `nexus-daemon-runtime` crate; that host was deleted in v1.193 P2 and the composition above is the current owner.

## §10 header note and §10.1 (daemon boot wiring; removed)

> **Host note (v1.193 P2):** the §10.1 boot/`WorkspaceState` wiring is a historical record of the deleted `nexus-daemon-runtime` crate. The facade boundary rule survives: the serving host reaches the agent host through `HostFacade` only. The §10.2 route family is retained wire, now served by `apps/nexus-service` (`routes.ts` + `provider.ts`/`discovery.ts`) over `nexus-core` (`provider-host` feature) / `nexus-core-node`.

### 10.1 Facade boundary

Integration is via narrow facade only:
- `WorkspaceState` stores `Option<Arc<dyn HostFacade>>` plus setters/getters.
- `boot.rs` constructs `HostManager`, loads config, calls `start`, stores the facade, and adds `AgentHostSubsystem` to lifecycle.
- Runtime handlers call `HostFacade` only — they do not import provider modules.

## §10 trailing sentence (removed)

"V1.20 redesign extends these routes with additional agent-host endpoints (see V1.20 spec §4.3)."

## §11 header note (removed blockquote)

> **Historical mapping note (v1.193 P2):** the Nexus-side mappings below were written against the deleted `nexus-daemon-runtime` host. The current owner of the mapped runtime role is `apps/nexus-service` over `nexus-core` / `nexus-core-node`, with `nexus-agent-host` unchanged below the `HostFacade` boundary.

## Former §15 (Version History) and document footer

## 15. Version History

| Version | Scope | Key additions |
|---|---|---|
| v1 | Consolidated from V1.18 + V1.19 | Initial SSOT: architecture, provider impl, discovery, policy, streaming, Multica/OpenDesign research, ACP protocol findings, design review decisions |
| v2 | V1.29 Batch 2 hardening | DF-21 timeout enforcement (audit + close gaps in TimeoutConfig coverage); R11 persistent child Drop kill; R12 persistent mode cancel(); Batch 2  DF-22/23/27/28 already resolved in V1.28. |

---

*Created: 2026-05-18. Consolidated from V1.18 / V1.19. Authoritative SSOT for `nexus-agent-host` subsystem design.*
