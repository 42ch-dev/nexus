# Body Editor — Specification (SUPERSEDED)

| Attribute | Value |
| --- | --- |
| **Status** | **Superseded (2026-06-26)** — direction rejected; see pointer below |
| **Document class** | Archived (pointer stub) |

**This spec is superseded.** The V1.67 Prepare locked a per-chapter rich-text body editor as the V1.68 authoring lead. On 2026-06-26 the user re-opened V1.67 with a product-vision correction: **Nexus is an AI-autonomous creative executor (like Codex / a design tool) — the human inputs Ideas and steers via a Canvas; the AI owns the prose writing.** A manual rich-text body editor is the wrong direction.

The successor design is the **Canvas Strategy Surface** vision, where the human-facing control surfaces (Strategy/Preset orchestration, Work outline + timeline, World KB browsing) are infinite-canvas graphs (React Flow), and rich-text (TipTap) survives only as an **in-node** editing capability. A core architectural principle of the successor: **visualization products must not edit raw files directly** — edits are structured/node-granular to avoid corrupting file structure.

➡️ **Successor**: [canvas-strategy-surface.md](canvas-strategy-surface.md) (Exploration, V1.67)
📄 **Archived full text**: [../../archived/knowledge/body-editor.md](../../archived/knowledge/body-editor.md)

The per-chapter write-coordination / lock content formerly specified here is **not** carried forward as-is; if the canvas ever flushes structured node-edits to chapter files, the coordination design will be re-derived in the canvas context (V1.68+).
