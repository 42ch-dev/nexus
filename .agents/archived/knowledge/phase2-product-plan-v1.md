# V1.0-phase2 Product Plan — V1.1

**Date**: 2026-04-06
**Author**: @product-manager
**Based on**: V1.0-phase1 Product Review V1, V1.0-phase1 Architecture Review V1
**Status**: Superseded — merged into `v1.1-overview-v1.md`. P0/P1/P2 features, user stories, and competitive analysis fully retained.

---

## Executive Summary

Nexus42 V1.1 transforms the CLI from a **developer preview** (V1.0: 24% functional commands) into a **usable beta** for technical writers and ACP developers. V1.1 delivers three core user values: (1) **working sync** with platform — users can push/pull manuscript changes with visibility into sync state; (2) **functional AI agent integration** — the `agent run` command actually communicates with ACP agents via a completed prompt loop; and (3) **manuscript file operations** — users can create, edit, and export manuscript files, not just track metadata.

The **primary V1.1 user** is a technical writer or developer who is comfortable with CLI workflows, wants AI agent assistance for creative writing, and tolerates "beta" software in exchange for early access to ACP-native tooling. V1.1 differentiates from Obsidian/Notion/Scrivener by being **CLI-first, ACP-powered, and open-source** — targeting users who want automation and AI integration, not just GUI-based organization.

V1.1 success is measured by: **70%+ commands functional** (vs. 24% today), **5 end-to-end workflows working** without skeleton messages, and **installation via Homebrew/npm** (no Rust toolchain required for end users).

---

## 1. V1.1 Product Vision

### Problem Statement

Creative writers working with AI agents lack a **CLI-native, open-source platform** that combines world-building structure, manuscript lifecycle management, and seamless ACP agent integration. Current tools (Obsidian, Notion, Scrivener) are GUI-first, proprietary, and lack native AI agent protocol support. Nexus42 V1.0 built the foundation but left most user-facing commands as skeleton implementations — users hit "V1.0 skeleton" messages at every turn.

### Target User

| Segment | Characteristics | Primary Need |
|---------|----------------|--------------|
| **Primary**: ACP Developer | Builds writing tools on ACP, comfortable with Rust/CLI, early adopter | Test ACP integration patterns, contribute to open-source |
| **Primary**: Technical Writer | Writes documentation or technical content, uses CLI daily, wants AI assistance | Manage manuscript phases, sync with team, run agents for editing |
| **Secondary**: Creative Writer (Technical) | Writes fiction/non-fiction, tolerant of CLI, curious about AI | Track world-building, manage draft→publish lifecycle, get agent feedback |

### Value Proposition

**For technical writers and ACP developers**, Nexus42 V1.1 is a **CLI-first creative writing platform** that **integrates 16+ ACP AI agents directly into your manuscript workflow**, unlike Obsidian/Notion/Scrivener which are GUI-only and lack native agent protocol support.

### One-Sentence Pitch

> "Nexus42 V1.1 is the first CLI-native writing platform with built-in AI agent integration — manage your manuscript lifecycle, sync with your team, and get AI feedback without leaving the terminal."

---

## 2. Feature Prioritization

### P0 Features (Must Have for V1.1)

Features without which V1.1 should not ship. These are "table stakes" for beta readiness.

| ID | Feature | User Story | Acceptance Criteria | Effort |
|----|---------|------------|---------------------|--------|
| **P0-1** | Fix `daemon start` auto-spawn | As a user, I expect `nexus42 daemon start` to actually start the daemon so I don't have to manually run cargo commands. | - `nexus42 daemon start` spawns `nexus42d` as background process<br>- Returns PID and port: "Daemon started on http://127.0.0.1:8420 (PID: 12345)"<br>- `daemon status` correctly reports running state | XS |
| **P0-2** | Complete ACP prompt loop | As a user, I want `nexus42 agent run <agent>` to actually send my prompts to the agent so I can get AI assistance. | - User types prompt in interactive loop<br>- Agent receives and processes message<br>- Agent response displayed to user<br>- ACP tool permission requests handled (auto-grant with warning for V1.1) | M |
| **P0-3** | `sync status` with real data | As a user, I need to see sync state (pending changes, last sync time, conflicts) so I know if my work is backed up. | - `sync status` shows: pending outbox count, last successful sync timestamp, conflict count<br>- Returns real data from outbox/SQLite, not "—" placeholders<br>- Works without daemon running (graceful degradation) | M |
| **P0-4** | Workspace init bug fix | As a user, I expect workspace initialization to persist so `manuscript status` recognizes my workspace. | - `is_initialized()` returns `true` after `init workspace`<br>- Manuscript/research commands recognize initialized workspace<br>- No "no workspace initialized" false errors | XS |
| **P0-5** | Missing context assembly route | As a user, I expect `context assemble` to work when daemon is running so I can generate context summaries. | - `POST /v1/local/context/assemble` route registered in daemon<br>- CLI receives valid response, not 404 | XS |

### P1 Features (Should Have)

Important features that significantly improve UX. V1.1 can ship without these, but they're expected by beta users.

| ID | Feature | User Story | Acceptance Criteria | Effort |
|----|---------|------------|---------------------|--------|
| **P1-1** | Manuscript file operations (create, phase transition) | As a user, I want to create manuscript files and transition phases (draft→review→finalize) so I can track my writing progress. | - `manuscript create <title>` creates file in `Stories/`<br>- `manuscript phase <phase>` persists phase to workspace.json<br>- `manuscript status` shows file path + phase | L |
| **P1-2** | Installation binaries (Homebrew + npm) | As a user, I want to install via `brew install nexus42` or `npm install -g nexus42` so I don't need to build from source. | - Homebrew formula published<br>- npm package `@42ch/nexus42` published<br>- Installation docs updated | M |
| **P1-3** | End-user documentation (Quick Start, Command Reference) | As a user, I need documentation so I can learn how to use Nexus42 without reading source code. | - `README.md` rewritten for end users (what/who/why)<br>- `docs/QUICKSTART.md` with step-by-step tutorial<br>- `docs/COMMANDS.md` with examples for all commands | M |
| **P1-4** | Research content extraction (PDF text, URL fetch) | As a user, I want `research scan --extract-text` to pull text from PDFs and URLs so my research is searchable. | - PDF text extraction (via `pdftotext` or pure Rust)<br>- URL content fetching (with basic HTTP client)<br>- Extracted text cached in SQLite | M |
| **P1-5** | Better error messages with suggested fixes | As a user, I want actionable error messages so I can fix issues without searching docs. | - All errors include: what went wrong, why, how to fix<br>- Example: "Daemon not running. Start with: nexus42 daemon start" | S |
| **P1-6** | Auth login flow (device code OAuth skeleton → working) | As a user, I want `auth login` to complete device flow so I can authenticate without manual token copy. | - Device code flow implemented (requires platform API mock or real endpoint)<br>- Token stored in `auth.json` | M |

### P2 Features (Nice to Have)

Enhancements that can wait for V1.2. These improve polish but aren't required for beta.

| ID | Feature | User Story | Acceptance Criteria | Effort |
|----|---------|------------|---------------------|--------|
| **P2-1** | Interactive tutorial/wizard mode | As a first-time user, I want `nexus42 tutorial` to guide me through setup so I don't feel lost. | - Interactive mode walks through: init, auth, create manuscript, run agent<br>- Sample workspace created | M |
| **P2-2** | Sync push/pull with real platform | As a user, I want `sync push` to actually upload my bundle to the platform so my team can see my changes. | - Requires platform API (nexus-platform repo)<br>- Full delta bundle upload<br>- Response handling, retry logic | L |
| **P2-3** | Conflict resolution UI | As a user, I want to resolve sync conflicts in the CLI so I don't have to manually edit JSON. | - `sync resolve` command with interactive choice (ours/theirs/merge)<br>- Visual diff display | L |
| **P2-4** | Manuscript export (PDF, DOCX, EPUB) | As a user, I want to export my manuscript to common formats so I can share with non-Nexus users. | - `manuscript export --format pdf|docx|epub`<br>- Uses export libraries (e.g., `printpdf`, `docx-rs`) | L |
| **P2-5** | Agent session persistence | As a user, I want agent sessions to persist across CLI invocations so I don't lose conversation history. | - Session state saved to SQLite<br>- `agent resume <session-id>` command | M |
| **P2-6** | Multi-agent orchestration basics | As a power user, I want to run multiple agents in sequence so I can chain specialized workflows. | - `agent run --sequence claude,codex`<br>- Output of first agent piped to second | L |
| **P2-7** | Command aliases and shell completions | As a frequent user, I want `nexus42 i` as alias for `init` and shell tab completions so I can work faster. | - Clap aliases configured<br>- Bash/zsh completion scripts generated | S |

---

## 3. User Stories

### Primary User Journey (V1.1 "Happy Path")

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        NEXUS42 V1.1 PRIMARY JOURNEY                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. INSTALL                                                                 │
│     $ brew install nexus42                                                  │
│     ✓ Binary installed, no Rust toolchain needed                           │
│                                                                             │
│  2. INITIALIZE WORKSPACE                                                    │
│     $ nexus42 init workspace "My Novel"                                     │
│     ✓ Creates: Stories/, References/, .nexus42/                            │
│     ✓ Message: "Workspace 'My Novel' initialized at /path"                 │
│                                                                             │
│  3. START DAEMON                                                            │
│     $ nexus42 daemon start                                                  │
│     ✓ Daemon spawns automatically                                          │
│     ✓ Returns: "Daemon started on http://127.0.0.1:8420 (PID: 12345)"      │
│                                                                             │
│  4. CREATE MANUSCRIPT                                                       │
│     $ nexus42 manuscript create "Chapter 1" --phase draft                  │
│     ✓ Creates: Stories/chapter-1.md                                        │
│     ✓ Sets phase to "draft"                                                │
│                                                                             │
│  5. RUN AI AGENT                                                            │
│     $ nexus42 agent run claude-acp                                          │
│     > "Help me outline Chapter 1 for a fantasy novel"                      │
│     ✓ Agent receives prompt, returns outline                               │
│     ✓ User saves response to manuscript file                               │
│                                                                             │
│  6. ADD RESEARCH                                                            │
│     $ nexus42 research scan --extract-text                                 │
│     ✓ Scans References/ for PDF, MD, TXT, URL files                        │
│     ✓ Extracts text content, caches to SQLite                              │
│                                                                             │
│  7. SYNC TO PLATFORM                                                        │
│     $ nexus42 sync status                                                   │
│     ✓ Shows: 3 pending changes, last sync 2 hours ago, 0 conflicts         │
│     $ nexus42 sync push                                                     │
│     ✓ Uploads delta bundle to platform (V1.1: mock or real API)            │
│                                                                             │
│  8. CHECK STATUS                                                            │
│     $ nexus42 manuscript status                                             │
│     ✓ Shows: Phase: draft, File: Stories/chapter-1.md, Synced: Yes         │
│                                                                             │
│  9. EXPORT (P2)                                                             │
│     $ nexus42 manuscript export --format pdf                               │
│     ✓ Generates: output/chapter-1.pdf                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Edge Cases and Error Scenarios

| Scenario | Current Behavior | V1.1 Expected Behavior |
|----------|-----------------|------------------------|
| **Daemon not running when sync attempted** | Returns "daemon not running" error | Auto-spawns daemon OR clear error with fix: "Start with: nexus42 daemon start" |
| **Workspace not initialized** | "no workspace initialized" (even after init) | Clear error: "No workspace found. Initialize with: nexus42 init workspace" |
| **Agent subprocess crashes** | Silent failure, loop continues | Graceful restart or clear exit: "Agent crashed. Restart with: nexus42 agent run" |
| **Sync conflict detected** | Returns error with no resolution path | `sync status` shows conflict count; `sync resolve` launched interactively |
| **PDF extraction fails (missing tool)** | Silent skip or panic | Warning: "pdftotext not found. Install or skip PDF: nexus42 research scan --skip-pdf" |
| **Auth token expired** | Generic API error | "Auth token expired. Re-authenticate: nexus42 auth login" |
| **Manuscript phase invalid** | Accepts any string | Validates against schema: "Invalid phase. Valid: draft, review, finalize, published" |

### Feature Discovery

| Feature | How Users Find It | V1.1 Improvement |
|---------|------------------|------------------|
| **All commands** | `nexus42 --help` | Add examples section: `nexus42 --examples` |
| **Agent integration** | Buried as last subcommand | Move `agent` to second position (after `init`, before `auth`) |
| **Sync status** | Users guess it exists | Add hint after `manuscript create`: "Sync your work: nexus42 sync status" |
| **Research extraction** | Not obvious from `--help` | `research scan --help` shows `--extract-text` flag prominently |
| **First-time onboarding** | None | Add `nexus42 welcome` command with 2-min interactive tutorial |

---

## 4. Feature Completeness Plan

### Current State (V1.0)

From V1.0-phase1 Product Review:

| Status | Count | Percentage |
|--------|-------|------------|
| **Functional** | 9 commands | 24% |
| **Partial/Skeleton** | 16 commands | 43% |
| **Not Implemented** | 4 commands | 11% |
| **Platform-Dependent** | 10 commands | 28% |

### V1.1 Target

| Metric | V1.0 | V1.1 Target |
|--------|------|-------------|
| **Commands Functional** | 24% (9/32) | **70%+ (23/32)** |
| **Commands Skeleton** | 43% (16/32) | **<20% (6/32)** |
| **Commands Not Implemented** | 11% (4/32) | **<10% (3/32)** |
| **End-to-End Workflows** | 0 | **5 workflows** |
| **Installation Methods** | Source build only | **Homebrew + npm + source** |

### Command-by-Command Status Plan

| Command | V1.0 Status | V1.1 Target | Notes |
|---------|-------------|-------------|-------|
| `init workspace` | ✅ Functional | ✅ Functional | Already works |
| `auth login` | ⚠️ Skeleton | ✅ Functional | Implement device flow (P1-6) |
| `auth token` | ✅ Functional | ✅ Functional | Already works |
| `auth logout` | ✅ Functional | ✅ Functional | Already works |
| `auth status` | ✅ Functional | ✅ Functional | Already works |
| `daemon start` | ⚠️ Skeleton | ✅ Functional | Auto-spawn daemon (P0-1) |
| `daemon stop` | ⚠️ Skeleton | ✅ Functional | Graceful shutdown |
| `daemon status` | ✅ Functional | ✅ Functional | Already works |
| `sync push` | ❌ Not Implemented | ⚠️ Partial | Mock API or platform integration (P2-2) |
| `sync pull` | ❌ Not Implemented | ❌ Not Implemented | Defer to V1.2 |
| `sync status` | ❌ Not Implemented | ✅ Functional | Real data from outbox (P0-3) |
| `creator register` | ⚠️ Skeleton | ⚠️ Partial | Depends on platform API |
| `creator status` | ⚠️ Skeleton | ⚠️ Partial | Depends on platform API |
| `creator use` | ✅ Functional | ✅ Functional | Already works |
| `creator list` | ⚠️ Skeleton | ⚠️ Partial | Local cache only |
| `creator pair` | ⚠️ Skeleton | ⚠️ Partial | Depends on platform API |
| `creator unpair` | ⚠️ Skeleton | ⚠️ Partial | Depends on platform API |
| `creator credentials rotate` | ⚠️ Skeleton | ⚠️ Partial | Depends on platform API |
| `manuscript status` | ⚠️ Skeleton | ✅ Functional | Fix workspace bug (P0-4) |
| `manuscript phase` | ⚠️ Skeleton | ✅ Functional | Persist to workspace.json |
| `manuscript output` | ⚠️ Skeleton | ⚠️ Partial | Basic output status |
| `manuscript promote` | ⚠️ Skeleton | ⚠️ Partial | Metadata-only, no strict checks |
| `manuscript verify` | ⚠️ Skeleton | ⚠️ Partial | Metadata-only (`--check-content` deferred) |
| `manuscript create` | ❌ Not Implemented | ✅ Functional | Create file in Stories/ (P1-1) |
| `research scan` | ✅ Partial | ✅ Functional | Add `--extract-text` (P1-4) |
| `research list` | ⚠️ Skeleton | ✅ Functional | Show cached entries |
| `research extract` | ⚠️ Skeleton | ✅ Functional | Extract by ID (P1-4) |
| `context assemble` | ✅ Functional | ✅ Functional | Fix missing route (P0-5) |
| `agent list` | ✅ Functional | ✅ Functional | Already works |
| `agent show` | ✅ Functional | ✅ Functional | Already works |
| `agent run` | ⚠️ Partial | ✅ Functional | Complete prompt loop (P0-2) |
| `agent probe` | ✅ Functional | ✅ Functional | Already works |

**Summary**: 23 commands functional (72%), 6 partial (19%), 3 not implemented (9%).

---

## 5. Documentation Plan

### Quick Start Guide (`docs/QUICKSTART.md`)

**Audience**: First-time users who just installed Nexus42.

**Content**:
1. What is Nexus42? (2 sentences)
2. Installation (brew/npm/source)
3. 5-minute workflow:
   - `nexus42 init workspace "My Project"`
   - `nexus42 daemon start`
   - `nexus42 manuscript create "Chapter 1"`
   - `nexus42 agent run claude-acp`
   - `nexus42 sync status`
4. Next steps (links to Command Reference, Architecture)

**Format**: Markdown with copy-pasteable commands.

### Command Reference (`docs/COMMANDS.md`)

**Audience**: Users who need to look up specific commands.

**Structure**:
- Organized by command group: `init`, `auth`, `creator`, `manuscript`, `research`, `daemon`, `sync`, `agent`, `context`
- Per command:
  - Syntax: `nexus42 <command> <subcommand> [options]`
  - Description: What it does (1 sentence)
  - When to use: Typical use case
  - Options: Table of flags
  - Examples: 2-3 copy-pasteable examples
  - Errors: Common errors and fixes
  - Related: Links to related commands

**Format**: Markdown, auto-generated from `--help` output + manual examples.

### Architecture Overview (`docs/ARCHITECTURE.md` — existing, update for V1.1)

**Audience**: Contributors and advanced users.

**Updates Needed**:
- Add V1.1 features (sync flow, agent integration diagram)
- Clarify daemon role (when required vs. optional)
- Add data flow diagram: CLI → Daemon → Platform
- Link to JSON Schema contracts

### Contribution Guide (`docs/CONTRIBUTING.md` — existing, keep as-is)

**Audience**: Developers who want to contribute code.

**Status**: Already adequate for V1.0. No changes required for V1.1.

### Additional V1.1 Documentation

| Doc | Audience | Priority | Owner |
|-----|----------|----------|-------|
| `README.md` rewrite | End users | P1 | @product-manager |
| `docs/QUICKSTART.md` | New users | P1 | @product-manager |
| `docs/COMMANDS.md` | All users | P1 | @fullstack-dev |
| `docs/TROUBLESHOOTING.md` | All users | P2 | @product-manager |
| `docs/AGENT-INTEGRATION.md` | ACP users | P1 | @architect |
| `docs/SYNC-GUIDE.md` | Sync users | P2 | @fullstack-dev |

---

## 6. Go/No-Go Criteria

### Must-Have Before Ship (V1.1)

All P0 features must be **functional and tested**:

- [ ] **P0-1**: `daemon start` auto-spawns daemon (verified: `ps aux | grep nexus42d` shows process)
- [ ] **P0-2**: `agent run` sends prompts to ACP agents (verified: agent responds to test prompt)
- [ ] **P0-3**: `sync status` returns real data (verified: shows outbox count, not "—")
- [ ] **P0-4**: Workspace init persists (verified: `manuscript status` works after init)
- [ ] **P0-5**: Context assembly route exists (verified: `context assemble` returns 200, not 404)
- [ ] **P1-1**: Manuscript create works (verified: file created in `Stories/`)
- [ ] **P1-2**: Homebrew formula published (verified: `brew install nexus42` works on clean macOS)
- [ ] **P1-3**: Quick Start and Command Reference published (verified: links work, no TODOs)

### Test Requirements

| Test Category | V1.0 Count | V1.1 Requirement |
|--------------|------------|------------------|
| **Unit Tests** | ~445 | **No regression** (all must pass) |
| **CLI Integration Tests** | ~50 | **+20 new** (cover all P0/P1 commands) |
| **Daemon Integration Tests** | ~10 | **+15 new** (cover daemon routes, workspace state) |
| **E2E Workflows** | 0 | **5 workflows** (full happy paths) |
| **CI Gates** | fmt, clippy, typecheck | **+ schema-validator, codegen diff** |

**CI Requirements**:
- All tests pass on PR
- `cargo +nightly fmt --all --check` passes
- `cargo clippy --all -- -D warnings` passes
- `pnpm run typecheck` passes
- `pnpm run codegen` produces no diff
- Homebrew formula build passes (CI matrix includes macOS)

### End-to-End Workflows (Must Work)

| # | Workflow | Steps | Verification |
|---|----------|-------|--------------|
| **1** | First-time setup | `brew install` → `init workspace` → `daemon start` | All commands return success, workspace dir created |
| **2** | Manuscript lifecycle | `manuscript create` → `manuscript phase review` → `manuscript status` | File exists, phase persists, status shows correct state |
| **3** | Agent interaction | `agent run claude-acp` → send prompt → receive response | Agent responds to prompt, response displayed |
| **4** | Research scan | Add PDF to References/ → `research scan --extract-text` → `research list` | PDF text extracted and displayed |
| **5** | Sync visibility | Make change → `sync status` → see pending count | Outbox count increments, not "—" |

---

## 7. Effort Summary

### By Feature

| Feature | Effort | Agent Sessions | Notes |
|---------|--------|----------------|-------|
| P0-1: Daemon auto-spawn | XS | ~0.5 | Bug fix: set `workspace_path` in `init_workspace()` |
| P0-2: ACP prompt loop | M | ~2-3 | Implement `LocalSet` bridge, channel wiring |
| P0-3: Sync status | M | ~1-2 | Query outbox, format output |
| P0-4: Workspace init bug | XS | ~0.5 | Bug fix: register daemon route |
| P0-5: Context route | XS | ~0.5 | Bug fix: add route to `create_router()` |
| P1-1: Manuscript create | L | ~2-4 | File I/O, workspace.json updates |
| P1-2: Homebrew/npm | M | ~1-2 | Formula + package publish workflow |
| P1-3: Documentation | M | ~1-2 | Writing, examples, screenshots |
| P1-4: Content extraction | M | ~2-3 | PDF parsing, HTTP client |
| P1-5: Error messages | S | ~1 | Audit all error paths |
| P1-6: Auth login | M | ~2-3 | Device flow, platform API |
| P2-1: Tutorial | M | ~1-2 | Interactive CLI wizard |
| P2-2: Sync push/pull | L | ~4-6 | Platform API integration |
| P2-3: Conflict resolution | L | ~3-4 | UI design, merge logic |
| P2-4: Export | L | ~3-5 | PDF/DOCX/EPUB libraries |
| P2-5: Session persistence | M | ~2-3 | SQLite session storage |
| P2-6: Multi-agent | L | ~3-4 | Orchestration logic |
| P2-7: Aliases/completions | S | ~0.5-1 | Clap config, shell scripts |

### By Priority

| Priority | Count | Total Effort | Agent Sessions |
|----------|-------|--------------|----------------|
| **P0** | 5 | **XS + M + M + XS + XS** | **~5-6 sessions** |
| **P1** | 6 | **L + M + M + M + S + M** | **~8-11 sessions** |
| **P2** | 7 | **M + L + L + L + M + L + S** | **~15-21 sessions** |

### Total Estimate

| Scope | Effort | Agent Sessions | Calendar (single agent) |
|-------|--------|----------------|------------------------|
| **V1.1 MVP (P0+P1)** | **S + M + L** | **~13-17 sessions** | ~2-3 weeks |
| **V1.1 Full (P0+P1+P2)** | **M + L + XL** | **~28-38 sessions** | ~5-7 weeks |
| **Recommended** | Ship P0+P1 as V1.1 Beta, defer P2 to V1.2 | | |

**Recommendation**: Ship V1.1 with P0+P1 only. P2 features require platform API integration (P2-2), complex UI (P2-3), or heavy dependencies (P2-4 export libraries). These are better suited for V1.2.

---

## Appendix

### A: Command Inventory (Current → V1.1 Target)

See §4 "Command-by-Command Status Plan" table above.

### B: Competitive Analysis Summary

| Feature | Nexus42 V1.1 | Obsidian | Notion | Scrivener |
|---------|-------------|----------|--------|-----------|
| **CLI-first** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **AI Agent Integration** | ✅ ACP (16 agents) | ⚠️ Community plugins | ⚠️ AI features (proprietary) | ❌ No |
| **World-Building Focus** | ✅ 15 domain aggregates | ⚠️ Via plugins | ⚠️ Via templates | ✅ Yes |
| **Manuscript Phases** | ✅ brainstorm→published | ❌ No | ❌ No | ✅ Yes |
| **Sync with Platform** | ⚠️ V1.1: status only | ⚠️ Sync service | ✅ Built-in | ⚠️ Dropbox/etc |
| **Open Source** | ✅ MIT | ❌ Proprietary | ❌ Proprietary | ❌ Proprietary |
| **Self-Hostable** | ✅ Yes | ⚠️ Limited | ❌ No | ❌ No |
| **ACP Protocol** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **Installation** | brew/npm/source | App store | Web | Desktop installer |
| **Cost** | Free | Free (paid sync) | Freemium | $59 one-time |

**Differentiation**: Nexus42 is the **only** CLI-first, ACP-native, open-source writing platform. Target users who value automation, AI integration, and extensibility over polished GUI.

### C: Reference Documents

| Document | Location | Purpose |
|----------|----------|---------|
| V1.0-phase1 Product Review | `.agents/archived/knowledge/phase1-product-review-v1.md` | Feature completeness analysis, user journey mapping |
| V1.0-phase1 Architecture Review | `.agents/archived/knowledge/phase1-architecture-review-v1.md` | Technical findings, bug list, debt items |
| ACP Client Tech Spec | `.agents/archived/knowledge/acp-client-tech-spec-v1.md` (archived 2026-04-17) | ACP integration details |
| status.json | `.agents/status.json` | Residual findings, plan tracking |
| AGENTS.md | Repository root | Development workflow, CI requirements |

---

## Alignment with Architecture Plan

The @architect is writing a parallel V1.0-phase2 architecture plan. This product plan assumes the following architectural decisions (pending confirmation):

| Product Feature | Architecture Dependency | Status |
|-----------------|------------------------|--------|
| P0-1: Daemon auto-spawn | CLI spawns daemon subprocess (no architecture change) | ✅ Confirmed feasible |
| P0-2: ACP prompt loop | `LocalSet` bridge for `!Send` futures | Requires implementation (ACP-ARCH-1) |
| P0-3: Sync status | Outbox SQLite query (no daemon required) | ✅ Confirmed feasible |
| P0-4: Workspace init | Fix `WorkspaceState::init_workspace()` | Bug fix (CLI-DAEMON-1) |
| P0-5: Context route | Add `POST /v1/local/context/assemble` to daemon router | Bug fix (CLI-DAEMON-2) |
| P1-1: Manuscript create | File I/O in CLI, workspace.json updates | ✅ Confirmed feasible |
| P1-4: Content extraction | PDF parsing library, HTTP client | New dependency required |
| P1-6: Auth login | Platform API for device flow | Requires platform mock or real endpoint |
| P2-2: Sync push/pull | Platform API endpoints | **BLOCKER** — requires nexus-platform repo |

**Risks**:
- P2-2 (sync push/pull) is **blocked** on platform API. Recommend deferring to V1.2 or implementing mock mode.
- P1-6 (auth login) requires platform OAuth endpoints. May need to implement local-only mode for V1.1.
- ACP prompt loop (P0-2) depends on `agent-client-protocol` SDK stability. Pin exact version (v0.10.4) to avoid breaking changes.

**Compatibility**: This product plan is compatible with likely architectural decisions. No features require breaking changes to crate boundaries or schema contracts.

---

*End of V1.0-phase2 Product Plan — V1.1*
