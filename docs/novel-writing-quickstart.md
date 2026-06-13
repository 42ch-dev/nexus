# Novel Writing Quickstart

Follow this guide to write a novel with Nexus from a clean install — no platform account, no cloud sync, no harness knowledge required. Everything runs locally.

## Prerequisites

- **Nexus installed** — see [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, or use a pre-built binary.
- **Daemon reachable** — verify with `system doctor` below.
- **A git repository** with the work directory initialized (Nexus uses a workspace root; any directory works).
- **An ACP-compatible agent** connected (the examples assume one is running; ACP setup is outside this quickstart’s scope).

> **Pre-release note (version < 1.0):** Breaking changes are expected. Local data may need to be wiped between upgrades. See [ARCHITECTURE.md](ARCHITECTURE.md) for storage layout details.

---

## Part I — Ongoing Serial

This section covers the full happy path: bootstrap, World-bound project init, first finalized chapter, auto-chain serial writing, quality feedback, and completion. Every command is copy-pasteable on a clean local install.

### §1 Prerequisites & Bootstrap

Before starting a novel, make sure the runtime is healthy and your Creator identity is set up.

```bash
# 1. Check system health
nexus42 system doctor

# 2. Register a Creator identity (or skip if you already have one)
nexus42 creator register --name "Your Author Name"

# 3. Set the active Creator
nexus42 creator use <your-handle>

# 4. Initialize a workspace
nexus42 creator workspace init

# 5. Start the daemon runtime (keep this running in another terminal)
nexus42 daemon start
```

After step 5, the daemon runs in the background. All subsequent commands communicate with it.

### §2 World & Project Init

Every novel Work is bound to a **World** — a shared space for characters, locations, and rules that persist across Works. Start by creating your World, then scaffold the novel project.

```bash
# 6. Create a new World for your story
nexus42 creator world create --title "Neon River"

# This prints a world_id (e.g. wld_abc123). Use it below.

# 7. Bootstrap a new novel Work — this creates the Work, runs the init preset
#    (which scaffolds the project structure), and chains intake → produce.
#    The init preset asks interactive questions:
#    - Working title / directory name
#    - How many chapters you plan
#    - Confirm or override the World to bind
nexus42 creator bootstrap \
  --idea "A solarpunk noir detective story set in a floating canal city" \
  --init-preset novel-project-init
```

The init preset creates a directory tree under `Works/<your-work-ref>/`:

```text
Works/<your-work-ref>/
  README.md
  Outlines/
    volume-outline.md
    chapters/          (empty; outlines appear as you write)
    foreshadowing.md
    event-index.md
  Stories/             (empty; chapter files appear as you draft)
  Logs/                (process logs)
```

> If you already have a World, list available ones with `nexus42 creator world list` and pass `--world-id <world_id>` to the bootstrap command.

### §3 First Chapter

`creator bootstrap` chains the init preset → intake → produce automatically. Check your status:

```bash
# 8. Bootstrap already chained intake. Confirm progress:
nexus42 creator works status

# 9. If intake is complete and the chapter is "not_started",
#    trigger the production preset to outline → draft → finalize chapter 1:
nexus42 creator run novel-writing <work_id>
```

The `novel-writing` preset will:

1. **Outline** the chapter (saved to `Outlines/chapters/ch01-outline.md`).
2. **Draft** the chapter body (saved to `Stories/ch01-<slug>.md`).
3. **Finalize** after passing a built-in quality check (the 五问 gate). If the gate returns **NOGO**, add more direction with `creator works inspire <work_id> --note "..."` and re-run `creator run novel-writing <work_id>`.

```bash
# Check progress at any time
nexus42 creator works status
```

The output shows each chapter’s status (`not_started → outlined → draft → finalized`), word count, file path, and a suggested next action.

### §4 Serial Writing with Auto-Chain

After chapter 1 is finalized, the daemon can automatically advance through chapters — no manual stage commands needed. This is called **auto-chain**.

```bash
# 10. Auto-chain is enabled by default for new Works.
#     The daemon will:
#       - Detect chapter 1 is finalized
#       - Auto-enqueue the produce stage for chapter 2
#       - Repeat through chapter 3, 4, ... until complete
#
#     Watch progress:
nexus42 creator works status
```

While auto-chain runs, you can inject inspiration at any time:

```bash
nexus42 creator run continue <work_id> --note "New plot twist idea: the detective's partner is the informant"
```

This does **not** interrupt the current chapter; it merges into the next chapter’s prompt context.

If the daemon restarts, resume where you left off:

```bash
nexus42 creator run resume <work_id>
```

If chapter files get out of sync with the database, reconcile them:

```bash
nexus42 creator run reconcile-chapters <work_id>
```

### §5 Quality Loop

Nexus records **findings** (continuity issues, craft notes, plot holes) to help you improve the manuscript without losing track.

Findings flow through a simple lifecycle:

```
open → resolved | wont_fix
```

- **Continuity / craft findings** are attached to specific chapters.
- **Severity levels** range from `info` (non-blocking notes) to `blocker` (must resolve).
- Findings from earlier chapters are visible in status output, including count and highest severity.

```bash
# See findings alongside chapter progress
nexus42 creator works status
# Output includes:
#   findings: 2 open (1 blocker, 1 minor) — highest: blocker
#     #1 [blocker] "Continuity error in chapter 5" → write
#     #2 [minor] "Style inconsistency" → none
#     Address findings or run: nexus42 creator run stage advance <work_id> --stage review
#     See docs/novel-writing-quickstart.md §5
```

A **96-hour master-decision banner** appears if any finding stays `open` too long. The daemon will prompt you to run a master-decision review:

```bash
# Primary path — run the master-decision review on open findings
nexus42 creator run review-master <work_id>

# List master findings (default), then enqueue the review for a specific finding:
nexus42 creator run review-master <work_id> --finding-id <finding_id>

# Opt-in: auto-schedule review when stale findings exist:
nexus42 creator run review-master <work_id> --auto-schedule
```

> `review-master` enqueues the `novel-review-master` preset for master decisions on **existing** findings. This is distinct from `creator run stage advance --stage review`, which runs the `reflection-loop` FL-E review stage to **generate** new findings from chapter content. Use `stage advance --stage review` to produce findings, then `review-master` to decide on them.
>
> The quality loop uses local SQLite and the daemon — no Redis, no cron, no cloud dependency.

### §6 Completion

A novel Work is **complete** when three conditions all hold:

1. Every planned chapter is **finalized**.
2. `current_chapter >= total_planned_chapters`.
3. Intake is **complete**.

When all conditions are met, the daemon sets the Work status to `completed` and stops auto-chain. A completion-lock file prevents accidental writes.

```bash
# Check completion state
nexus42 creator works status
# Output (completed Work):
# ═════════════════════════════════════════════════════
#   "Your Novel" — Work wrk_... (novel)
#   COMPLETED at 2026-06-12T10:00:00Z
#   12/12 chapters finalized.
#   No further novel-writing schedules will be enqueued.
#
#   findings: none open
#   This Work is complete; see docs/novel-writing-quickstart.md §6
#
#   To start a new Work, run:
#     nexus42 creator run start \
#       --init-preset novel-project-init --idea "..."
# ═════════════════════════════════════════════════════
```

To start a **new** novel in the same World:

```bash
nexus42 creator run start --idea "..." --init-preset novel-project-init --world-id <world_id>
```

To **reopen** a completed Work (e.g., to add bonus chapters):

```bash
nexus42 creator works completion-lock release <work_id>
nexus42 creator run resume <work_id> --reopen --reason "Adding epilogue"
```

---

## Part II — Optional / Advanced

The sections below cover multi-Work, multi-volume, and inspiration management. These are **optional** features — you can write a complete novel with only Part I.

### A) Multi-Work Desk

You can work on **multiple novels concurrently**. Each Work runs its own auto-chain independently.

```bash
# List all Works
nexus42 creator works list

# Switch the default Work (the one used when you omit <work_id>)
nexus42 creator works use <work_id>

# See the selection pool
nexus42 creator works pool list
```

The **selection pool** tracks which Work is `active` (the CLI default). Completing a Work clears the active slot — promote a new Work to active explicitly.

### B) Multi-Volume

If you declare multiple volumes during project init, each volume gets a volume outline file and chapters are grouped by volume.

```text
Works/<your-work-ref>/
  Outlines/
    volume-1-outline.md
    volume-2-outline.md
```

Chapter numbers may repeat across volumes (e.g., both volumes have a chapter 1). The primary key `(work_id, volume, chapter)` allows this.

Status output shows volume-aware progress:

```text
Works/<your-work-ref>/
  Outlines/
    volume-1-outline.md    (chapters 1–12)
    volume-2-outline.md    (chapters 1–10)
```

Cross-volume continuity is maintained through the shared World KB — characters and locations stay consistent because they live in the World, not per-Work.

### C) Work-Level Notes / Mid-Session Inspiration

As you write, inspiration notes accumulate in the Work's **inspiration log**. These are injected into the next chapter's prompt context, so stray ideas and mid-session brainstorms are never lost.

```bash
# Add inspiration at any time — even during auto-chain
nexus42 creator run continue <work_id> --note "Character X should have a hidden motive from chapter 3 onward"
```

Inspiration notes are:
- **Appended** to the Work's log (never overwrite).
- **Visible** in `creator works status`.
- **Merged** into prompt context at the next chapter boundary.

No special setup is needed — inspiration works out of the box with any ongoing novel Work.

> For the **creator-scoped Inspiration Pool** (long-lived idea backlog, persisted at `Pool/Ideas/`), see `creator works pool inspiration *` and [novel-work-pool.md](../.mstar/knowledge/specs/novel-work-pool.md) §3.

---

## Further Reading

| Topic | Document |
|-------|----------|
| Repo layout, crate responsibilities, storage | [ARCHITECTURE.md](ARCHITECTURE.md) |
| CLI command reference (normative) | [`.mstar/knowledge/specs/cli-spec.md`](../.mstar/knowledge/specs/cli-spec.md) |
| CLI command groups and IA | [`.mstar/knowledge/specs/cli-command-ia.md`](../.mstar/knowledge/specs/cli-command-ia.md) |
| Creator entry model | [`.mstar/knowledge/specs/creator-centric-entry-model.md`](../.mstar/knowledge/specs/creator-centric-entry-model.md) |
| Novel workflow profile (artifacts, completion) | [`.mstar/knowledge/specs/novel-workflow-profile.md`](../.mstar/knowledge/specs/novel-workflow-profile.md) |
| Contribution guide | [CONTRIBUTING.md](CONTRIBUTING.md) |
